//! GET /api/v1/contracts/{id}/interactions — interaction history with filtering,
//! pagination, aggregated stats, and CSV export (issue #46 v1).
//!
//! Features:
//!   - Pagination: limit / offset
//!   - Time filtering: since / until (RFC-3339)
//!   - Function filtering: ?function=NAME
//!   - Aggregated stats: call count, unique callers, error rate
//!   - 1-hour cache keyed by contract + filter params
//!   - Rate limit: 100 req/min per IP (enforced globally by middleware)
//!   - JSON and CSV export via ?format=csv

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    handlers::ensure_contract_exists,
    state::AppState,
};
use shared::pagination::Cursor;

const INTERACTIONS_V1_CACHE_NS: &str = "v1_interactions";
const INTERACTIONS_V1_CACHE_TTL_SECS: u64 = 3_600; // 1 hour

// ── Query params ──────────────────────────────────────────────────────────────

/// Query parameters for GET /api/v1/contracts/{id}/interactions
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct InteractionsV1Query {
    /// Maximum number of results to return (1–100, default 50).
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Zero-based offset for page-based navigation (default 0).
    #[serde(default)]
    pub offset: i64,
    /// Opaque cursor returned by a previous response for cursor-based pagination.
    pub cursor: Option<String>,
    /// Only return interactions at or after this timestamp (RFC-3339).
    pub since: Option<String>,
    /// Only return interactions at or before this timestamp (RFC-3339).
    pub until: Option<String>,
    /// Filter by called function / method name.
    pub function: Option<String>,
    /// Response format: "json" (default) or "csv".
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_limit() -> i64 {
    50
}
fn default_format() -> String {
    "json".to_string()
}

// ── Response types ────────────────────────────────────────────────────────────

/// A single interaction record returned by the v1 endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InteractionV1Item {
    pub id: Uuid,
    /// Caller / user address.
    pub caller: Option<String>,
    /// Function / method that was called.
    pub function: Option<String>,
    /// Transaction hash on-chain.
    pub transaction_hash: Option<String>,
    /// When the interaction occurred.
    pub timestamp: DateTime<Utc>,
    /// Whether this interaction resulted in an error.
    pub is_error: bool,
}

/// Aggregated statistics over the returned (filtered) interaction set.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InteractionStats {
    /// Total number of interactions matching the filters (before pagination).
    pub call_count: i64,
    /// Number of distinct caller addresses.
    pub unique_callers: i64,
    /// Fraction of interactions that resulted in an error. Range [0.0, 1.0].
    pub error_rate: f64,
}

/// Full response envelope for GET /api/v1/contracts/{id}/interactions.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InteractionsV1Response {
    pub contract_id: Uuid,
    pub items: Vec<InteractionV1Item>,
    pub stats: InteractionStats,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub next_cursor: Option<String>,
    /// Whether this response was served from the in-memory cache.
    pub cached: bool,
}

// ── Handler ───────────────────────────────────────────────────────────────────

/// GET /api/v1/contracts/{id}/interactions
///
/// Returns recent interactions for a contract with optional time/function
/// filtering, cursor-based pagination, aggregated stats, and CSV export.
#[utoipa::path(
    get,
    path = "/api/v1/contracts/{id}/interactions",
    params(
        ("id" = String, Path, description = "Contract UUID"),
        InteractionsV1Query
    ),
    responses(
        (status = 200, description = "Interaction history and aggregated stats",
         body = InteractionsV1Response),
        (status = 400, description = "Invalid contract ID or query parameters"),
        (status = 404, description = "Contract not found"),
    ),
    tag = "Analytics"
)]
pub async fn get_contract_interactions_v1(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<InteractionsV1Query>,
) -> ApiResult<Response> {
    let contract_uuid = Uuid::parse_str(&id).map_err(|_| {
        ApiError::bad_request(
            "InvalidContractId",
            format!("Invalid contract ID format: {}", id),
        )
    })?;

    ensure_contract_exists(&state, contract_uuid, &id, "get contract for v1 interactions").await?;

    let limit = params.limit.clamp(1, 100);
    let offset = params.offset.max(0);

    // Parse optional time bounds
    let since_ts = params
        .since
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    let until_ts = params
        .until
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    // Build a stable cache key from all filter dimensions
    let cache_key = format!(
        "{}:{}:{}:{}:{}:{}:{}",
        contract_uuid,
        limit,
        offset,
        params.cursor.as_deref().unwrap_or(""),
        params.since.as_deref().unwrap_or(""),
        params.until.as_deref().unwrap_or(""),
        params.function.as_deref().unwrap_or(""),
    );

    // Only serve from cache for JSON requests (CSV is always fresh)
    if params.format != "csv" {
        let (cached_val, cache_hit) = state.cache.get(INTERACTIONS_V1_CACHE_NS, &cache_key).await;
        if let (Some(json_str), true) = (cached_val, cache_hit) {
            if let Ok(mut resp) = serde_json::from_str::<InteractionsV1Response>(&json_str) {
                resp.cached = true;
                return Ok(Json(resp).into_response());
            }
        }
    }

    // Cursor-based pagination: decode cursor and use it as the upper bound
    let cursor = params.cursor.as_ref().and_then(|c| Cursor::decode(c).ok());
    let effective_offset = if cursor.is_some() { 0 } else { offset };

    // Fetch paginated interaction rows
    let rows: Vec<(Uuid, Option<String>, Option<String>, Option<String>, DateTime<Utc>, String)> =
        sqlx::query_as(
            r#"
            SELECT
                id,
                user_address,
                method,
                transaction_hash,
                created_at,
                interaction_type
            FROM contract_interactions
            WHERE contract_id = $1
              AND ($2::text IS NULL OR method = $2)
              AND ($3::timestamptz IS NULL OR created_at >= $3)
              AND ($4::timestamptz IS NULL OR created_at <= $4)
              AND ($5::timestamptz IS NULL OR
                   (created_at < $5 OR (created_at = $5 AND id < $6)))
            ORDER BY created_at DESC, id DESC
            LIMIT $7 OFFSET $8
            "#,
        )
        .bind(contract_uuid)
        .bind(params.function.as_deref())
        .bind(since_ts)
        .bind(until_ts)
        .bind(cursor.as_ref().map(|c| c.timestamp))
        .bind(cursor.as_ref().map(|c| c.id))
        .bind(limit)
        .bind(effective_offset)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            ApiError::internal(format!("fetch v1 interactions failed: {}", e))
        })?;

    // Total count (for stats and pagination metadata)
    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM contract_interactions
        WHERE contract_id = $1
          AND ($2::text IS NULL OR method = $2)
          AND ($3::timestamptz IS NULL OR created_at >= $3)
          AND ($4::timestamptz IS NULL OR created_at <= $4)
        "#,
    )
    .bind(contract_uuid)
    .bind(params.function.as_deref())
    .bind(since_ts)
    .bind(until_ts)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::internal(format!("count v1 interactions failed: {}", e)))?;

    // Aggregated stats over the full filtered set (not just the current page)
    let (unique_callers, error_count): (i64, i64) = sqlx::query_as(
        r#"
        SELECT
            COUNT(DISTINCT user_address)::bigint,
            COUNT(*) FILTER (WHERE interaction_type = 'publish_failed')::bigint
        FROM contract_interactions
        WHERE contract_id = $1
          AND ($2::text IS NULL OR method = $2)
          AND ($3::timestamptz IS NULL OR created_at >= $3)
          AND ($4::timestamptz IS NULL OR created_at <= $4)
        "#,
    )
    .bind(contract_uuid)
    .bind(params.function.as_deref())
    .bind(since_ts)
    .bind(until_ts)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::internal(format!("aggregate v1 interactions failed: {}", e)))?;

    let error_rate = if total > 0 {
        error_count as f64 / total as f64
    } else {
        0.0
    };

    let items: Vec<InteractionV1Item> = rows
        .iter()
        .map(|(id, caller, method, tx_hash, created_at, itype)| InteractionV1Item {
            id: *id,
            caller: caller.clone(),
            function: method.clone(),
            transaction_hash: tx_hash.clone(),
            timestamp: *created_at,
            is_error: itype == "publish_failed",
        })
        .collect();

    let next_cursor = if items.len() >= limit as usize {
        items
            .last()
            .map(|last| Cursor::new(last.timestamp, last.id).encode())
    } else {
        None
    };

    let response = InteractionsV1Response {
        contract_id: contract_uuid,
        stats: InteractionStats {
            call_count: total,
            unique_callers,
            error_rate,
        },
        total,
        limit,
        offset: effective_offset,
        next_cursor,
        items,
        cached: false,
    };

    // CSV export
    if params.format == "csv" {
        let csv = build_csv(&response);
        return Ok((
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"interactions.csv\"",
                ),
            ],
            csv,
        )
            .into_response());
    }

    // Cache the JSON response
    if let Ok(serialized) = serde_json::to_string(&response) {
        state
            .cache
            .put(
                INTERACTIONS_V1_CACHE_NS,
                &cache_key,
                serialized,
                Some(Duration::from_secs(INTERACTIONS_V1_CACHE_TTL_SECS)),
            )
            .await;
    }

    Ok(Json(response).into_response())
}

// ── CSV builder ───────────────────────────────────────────────────────────────

fn build_csv(resp: &InteractionsV1Response) -> String {
    let mut out = String::from("id,caller,function,transaction_hash,timestamp,is_error\n");
    for item in &resp.items {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            item.id,
            item.caller.as_deref().unwrap_or(""),
            item.function.as_deref().unwrap_or(""),
            item.transaction_hash.as_deref().unwrap_or(""),
            item.timestamp.to_rfc3339(),
            item.is_error,
        ));
    }
    out
}
