//! License issuance, validation, listing, and revocation.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    auth::AuthenticatedUser,
    error::{ApiError, ApiResult},
    marketplace::{
        license::{LicenseClaims, LicenseError, LicenseSigner},
        models::{
            IssueLicenseRequest, IssuedLicense, LicenseRecord, ValidateLicenseRequest,
            ValidateLicenseResponse,
        },
    },
    state::AppState,
};

/// POST /api/contracts/{contract_id}/licenses
pub async fn issue_license(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(contract_id): Path<Uuid>,
    Json(req): Json<IssueLicenseRequest>,
) -> ApiResult<(StatusCode, Json<IssuedLicense>)> {
    let signer = load_signer()?;

    // The plan must exist, be active, and belong to this contract.
    let plan = sqlx::query_as::<_, PlanForIssue>(
        r#"
        SELECT id, name, billing_period, call_quota
        FROM contract_pricing_plans
        WHERE id = $1 AND contract_id = $2 AND is_active
        "#,
    )
    .bind(req.plan_id)
    .bind(contract_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| {
        ApiError::not_found(
            "plan_not_found",
            "pricing plan not found or inactive for this contract",
        )
    })?;

    let now = Utc::now();
    let expires_at = match plan.billing_period.as_str() {
        "monthly" => Some(now + Duration::days(30)),
        _ => None, // one_time licenses are non-expiring by default
    };

    let metadata = req.metadata.unwrap_or_else(|| serde_json::json!({}));

    // Insert first to obtain the row's id and stable `jti`.
    let jti = Uuid::new_v4();
    let record = sqlx::query_as::<_, LicenseRecord>(
        r#"
        INSERT INTO contract_licenses
            (jti, contract_id, plan_id, owner_id, issued_at, expires_at, status, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, 'active', $7)
        RETURNING id, jti, contract_id, plan_id, owner_id, issued_at, expires_at,
                  revoked_at, status, metadata, created_at
        "#,
    )
    .bind(jti)
    .bind(contract_id)
    .bind(plan.id)
    .bind(user.publisher_id)
    .bind(now)
    .bind(expires_at)
    .bind(&metadata)
    .fetch_one(&state.db)
    .await?;

    let claims = LicenseClaims {
        jti: record.jti,
        sub: record.owner_id,
        aud: record.contract_id,
        plan_id: record.plan_id,
        plan_name: plan.name,
        iat: now.timestamp(),
        exp: expires_at.map(|t| t.timestamp()),
        quota: plan.call_quota,
    };
    let token = signer
        .sign(&claims)
        .map_err(|e| ApiError::internal_error("license_sign_failed", e.to_string()))?;

    Ok((
        StatusCode::CREATED,
        Json(IssuedLicense {
            license: record,
            token,
            public_key_b64: signer.public_key_b64(),
        }),
    ))
}

/// POST /api/marketplace/licenses/validate
///
/// Validates signature + expiry + DB status (revoked/expired). Public:
/// callers prove possession of the token by sending it.
pub async fn validate_license(
    State(state): State<AppState>,
    Json(req): Json<ValidateLicenseRequest>,
) -> ApiResult<Json<ValidateLicenseResponse>> {
    let signer = load_signer()?;

    let claims = match signer.verify(&req.token) {
        Ok(c) => c,
        Err(LicenseError::Expired) => {
            return Ok(Json(ValidateLicenseResponse {
                valid: false,
                reason: Some("expired".into()),
                claims: None,
                status: None,
                revoked_at: None,
                expires_at: None,
            }));
        }
        Err(e) => {
            return Ok(Json(ValidateLicenseResponse {
                valid: false,
                reason: Some(e.to_string()),
                claims: None,
                status: None,
                revoked_at: None,
                expires_at: None,
            }));
        }
    };

    let row = sqlx::query_as::<_, RevocationCheck>(
        r#"
        SELECT status, revoked_at, expires_at
        FROM contract_licenses
        WHERE jti = $1
        "#,
    )
    .bind(claims.jti)
    .fetch_optional(&state.db)
    .await?;

    let Some(row) = row else {
        return Ok(Json(ValidateLicenseResponse {
            valid: false,
            reason: Some("not_found".into()),
            claims: Some(claims),
            status: None,
            revoked_at: None,
            expires_at: None,
        }));
    };

    let valid = row.status == "active";
    let reason = if valid {
        None
    } else {
        Some(row.status.clone())
    };

    Ok(Json(ValidateLicenseResponse {
        valid,
        reason,
        claims: Some(claims),
        status: Some(row.status),
        revoked_at: row.revoked_at,
        expires_at: row.expires_at,
    }))
}

/// POST /api/marketplace/licenses/{jti}/revoke
pub async fn revoke_license(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(jti): Path<Uuid>,
) -> ApiResult<Json<LicenseRecord>> {
    // The license owner OR the contract owner may revoke.
    let lic = sqlx::query_as::<_, LicenseRecord>(
        r#"SELECT id, jti, contract_id, plan_id, owner_id, issued_at, expires_at,
                  revoked_at, status, metadata, created_at
           FROM contract_licenses WHERE jti = $1"#,
    )
    .bind(jti)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::not_found("license_not_found", "license not found"))?;

    if lic.owner_id != user.publisher_id {
        // Check contract ownership as a fallback authorization.
        let owns_contract: Option<Uuid> = sqlx::query_scalar(
            "SELECT publisher_id FROM contracts WHERE id = $1 AND publisher_id = $2",
        )
        .bind(lic.contract_id)
        .bind(user.publisher_id)
        .fetch_optional(&state.db)
        .await?;
        if owns_contract.is_none() {
            return Err(ApiError::forbidden("not authorized to revoke this license"));
        }
    }

    if lic.status == "revoked" {
        return Ok(Json(lic));
    }

    let updated = sqlx::query_as::<_, LicenseRecord>(
        r#"
        UPDATE contract_licenses
        SET status = 'revoked', revoked_at = NOW()
        WHERE jti = $1
        RETURNING id, jti, contract_id, plan_id, owner_id, issued_at, expires_at,
                  revoked_at, status, metadata, created_at
        "#,
    )
    .bind(jti)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(updated))
}

/// GET /api/marketplace/licenses
pub async fn list_my_licenses(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> ApiResult<Json<Vec<LicenseRecord>>> {
    let rows = sqlx::query_as::<_, LicenseRecord>(
        r#"SELECT id, jti, contract_id, plan_id, owner_id, issued_at, expires_at,
                  revoked_at, status, metadata, created_at
           FROM contract_licenses
           WHERE owner_id = $1
           ORDER BY issued_at DESC"#,
    )
    .bind(user.publisher_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

/// GET /api/marketplace/license-pubkey
///
/// Returns the Ed25519 verification key so clients can validate JWTs
/// offline (e.g. a contract's gateway that doesn't want to phone home).
pub async fn license_pubkey(State(_state): State<AppState>) -> ApiResult<Json<PubKeyResponse>> {
    let signer = load_signer()?;
    Ok(Json(PubKeyResponse {
        alg: "EdDSA".into(),
        public_key_b64: signer.public_key_b64(),
    }))
}

fn load_signer() -> ApiResult<LicenseSigner> {
    LicenseSigner::from_env().map_err(|e| {
        ApiError::service_unavailable_with(
            "license_signing_unavailable",
            format!(
                "marketplace license signing key is not configured: {e}. \
                 Set MARKETPLACE_LICENSE_SIGNING_KEY (base64-encoded 32-byte Ed25519 seed)."
            ),
        )
    })
}

#[derive(Debug, sqlx::FromRow)]
struct PlanForIssue {
    id: Uuid,
    name: String,
    billing_period: String,
    call_quota: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct RevocationCheck {
    status: String,
    revoked_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct PubKeyResponse {
    pub alg: String,
    pub public_key_b64: String,
}
