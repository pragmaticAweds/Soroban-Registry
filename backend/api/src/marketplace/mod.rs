//! Marketplace — paid contract usage and licensing.
//!
//! Phase 1 scope (current):
//!   * `contract_pricing_plans` CRUD per contract
//!   * Ed25519-signed JWT licenses, issuance + offline validation
//!   * Usage metering ledger
//!
//! Out of scope (Phase 2/3, intentionally deferred):
//!   * Stripe checkout + webhook handling
//!   * USDC on-chain payment verification
//!   * Billing aggregation / invoicing
//!
//! Queries in this module use `sqlx::query()` / `query_as()` (non-macro)
//! rather than `sqlx::query!`. This matches `usage_counter.rs` and keeps
//! the module compile-checkable without forcing the new migration to be
//! applied at build time.

pub mod license;
pub mod license_handlers;
pub mod metering;
pub mod models;
pub mod pricing_handlers;

pub use license::LicenseSigner;
