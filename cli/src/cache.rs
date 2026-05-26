//! cache.rs — Verification result caching with 24-hour TTL
//!
//! Manages local caching of contract verification results to reduce API calls
//! and improve CLI performance. Cache entries are automatically invalidated
//! after 24 hours or when explicitly cleared.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

const CACHE_DIR_NAME: &str = ".soroban-registry";
const CACHE_FILE_NAME: &str = "verification_cache.json";
const CACHE_TTL_HOURS: i64 = 24;

/// A cached verification result with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedVerification {
    /// The cached verification result JSON
    pub result: serde_json::Value,
    /// Timestamp when the result was cached
    pub cached_at: DateTime<Utc>,
    /// Verification detail (security scan + audit info)
    pub detail: Option<serde_json::Value>,
}

/// The entire verification cache
#[derive(Debug, Serialize, Deserialize, Default)]
struct VerificationCache {
    #[serde(flatten)]
    entries: BTreeMap<String, CachedVerification>,
}

impl VerificationCache {
    /// Load the cache from disk, or return an empty cache if it doesn't exist
    fn load() -> Result<Self> {
        let path = cache_file_path()?;
        if !path.exists() {
            return Ok(VerificationCache::default());
        }

        let content = fs::read_to_string(&path)
            .context("Failed to read verification cache file")?;
        serde_json::from_str(&content)
            .context("Failed to parse verification cache")
    }

    /// Save the cache to disk
    fn save(&self) -> Result<()> {
        let path = cache_file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create cache directory")?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)
            .context("Failed to write verification cache")?;
        Ok(())
    }

    /// Remove expired entries (older than 24 hours)
    fn prune_expired(&mut self) {
        let now = Utc::now();
        let ttl = Duration::hours(CACHE_TTL_HOURS);
        self.entries.retain(|_, cached| {
            let age = now.signed_duration_since(cached.cached_at);
            age < ttl
        });
    }
}

/// Generate a cache key from contract address and network
fn cache_key(address: &str, network: &str) -> String {
    format!("{}:{}", network, address)
}

/// Get the path to the cache file
fn cache_file_path() -> Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(CACHE_DIR_NAME).join(CACHE_FILE_NAME))
        .ok_or_else(|| anyhow::anyhow!("Could not resolve home directory"))
}

/// Retrieve a cached verification result if it exists and is still valid
pub fn get(address: &str, network: &str) -> Result<Option<CachedVerification>> {
    let mut cache = VerificationCache::load()?;
    cache.prune_expired();

    let key = cache_key(address, network);
    Ok(cache.entries.get(&key).cloned())
}

/// Store a verification result in the cache
pub fn set(
    address: &str,
    network: &str,
    result: serde_json::Value,
    detail: Option<serde_json::Value>,
) -> Result<()> {
    let mut cache = VerificationCache::load()?;
    cache.prune_expired();

    let key = cache_key(address, network);
    cache.entries.insert(
        key,
        CachedVerification {
            result,
            cached_at: Utc::now(),
            detail,
        },
    );

    cache.save()
}

/// Clear all cached verification results
pub fn clear_all() -> Result<()> {
    let path = cache_file_path()?;
    if path.exists() {
        fs::remove_file(&path).context("Failed to remove cache file")?;
    }
    Ok(())
}

/// Clear a specific contract's cached verification
pub fn clear(address: &str, network: &str) -> Result<()> {
    let mut cache = VerificationCache::load()?;

    let key = cache_key(address, network);
    cache.entries.remove(&key);

    cache.save()
}

/// Get cache statistics (number of entries, oldest entry, newest entry)
pub fn stats() -> Result<CacheStats> {
    let mut cache = VerificationCache::load()?;
    cache.prune_expired();

    let count = cache.entries.len();
    let oldest = cache.entries.values().map(|c| c.cached_at).min();
    let newest = cache.entries.values().map(|c| c.cached_at).max();

    Ok(CacheStats {
        total_entries: count,
        oldest_entry: oldest,
        newest_entry: newest,
        ttl_hours: CACHE_TTL_HOURS,
    })
}

#[derive(Debug)]
pub struct CacheStats {
    pub total_entries: usize,
    pub oldest_entry: Option<DateTime<Utc>>,
    pub newest_entry: Option<DateTime<Utc>>,
    pub ttl_hours: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_generation() {
        let key = cache_key("abc123", "testnet");
        assert_eq!(key, "testnet:abc123");
    }

    #[test]
    fn test_empty_cache() {
        // This test verifies that an empty cache loads correctly
        let cache = VerificationCache::default();
        assert_eq!(cache.entries.len(), 0);
    }
}
