//! Generic TTL (time-to-live) cache for response-level caching.
//!
//! Provides a thread-safe, async-compatible cache with automatic expiration
//! and bounded capacity. Designed for caching expensive query results such as
//! Neo4j graph data responses.
//!
//! # Example
//! ```ignore
//! use std::time::Duration;
//! use crate::utils::cache::TtlCache;
//!
//! let cache: TtlCache<String> = TtlCache::new(Duration::from_secs(60), 1000);
//! cache.set("key".to_string(), "value".to_string()).await;
//! assert_eq!(cache.get("key").await, Some("value".to_string()));
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// A single cache entry holding a value and its insertion timestamp.
struct CacheEntry<V> {
    value: V,
    inserted_at: Instant,
}

/// Thread-safe TTL cache with bounded capacity.
///
/// - Entries expire after `ttl` duration.
/// - When `max_entries` is reached, the oldest entry is evicted on `set`.
/// - All operations are async-safe via `tokio::sync::RwLock`.
pub struct TtlCache<V> {
    entries: Arc<RwLock<HashMap<String, CacheEntry<V>>>>,
    ttl: Duration,
    max_entries: usize,
}

impl<V: Clone + Send + Sync> TtlCache<V> {
    /// Create a new TTL cache.
    ///
    /// # Arguments
    /// * `ttl` - How long entries remain valid after insertion
    /// * `max_entries` - Maximum number of entries before eviction
    pub fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            ttl,
            max_entries,
        }
    }

    /// Retrieve a cached value if it exists and has not expired.
    ///
    /// Returns `None` if the key is missing or the entry has expired.
    /// Expired entries are lazily removed on access.
    pub async fn get(&self, key: &str) -> Option<V> {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get(key) {
            if entry.inserted_at.elapsed() < self.ttl {
                return Some(entry.value.clone());
            }
            // Entry expired — remove it
            entries.remove(key);
        }
        None
    }

    /// Insert or update a cache entry.
    ///
    /// If the cache is at capacity, the oldest entry (by insertion time)
    /// is evicted to make room.
    pub async fn set(&self, key: String, value: V) {
        let mut entries = self.entries.write().await;

        // Evict expired entries first to reclaim space
        let now = Instant::now();
        entries.retain(|_, entry| now.duration_since(entry.inserted_at) < self.ttl);

        // If still at capacity, evict the oldest entry
        if entries.len() >= self.max_entries && !entries.contains_key(&key) {
            if let Some(oldest_key) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.inserted_at)
                .map(|(k, _)| k.clone())
            {
                entries.remove(&oldest_key);
            }
        }

        entries.insert(
            key,
            CacheEntry {
                value,
                inserted_at: now,
            },
        );
    }

    /// Remove a specific entry from the cache.
    pub async fn invalidate(&self, key: &str) {
        self.entries.write().await.remove(key);
    }

    /// Remove all entries from the cache.
    pub async fn invalidate_all(&self) {
        self.entries.write().await.clear();
    }

    /// Return the number of non-expired entries currently in the cache.
    pub async fn len(&self) -> usize {
        let entries = self.entries.read().await;
        let now = Instant::now();
        entries
            .values()
            .filter(|entry| now.duration_since(entry.inserted_at) < self.ttl)
            .count()
    }

    /// Return true if the cache contains no valid (non-expired) entries.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_set_and_get() {
        let cache: TtlCache<String> = TtlCache::new(Duration::from_secs(60), 100);
        cache.set("k1".to_string(), "v1".to_string()).await;
        assert_eq!(cache.get("k1").await, Some("v1".to_string()));
    }

    #[tokio::test]
    async fn test_get_missing_key() {
        let cache: TtlCache<String> = TtlCache::new(Duration::from_secs(60), 100);
        assert_eq!(cache.get("missing").await, None);
    }

    #[tokio::test]
    async fn test_ttl_expiration() {
        let cache: TtlCache<String> = TtlCache::new(Duration::from_millis(50), 100);
        cache.set("k1".to_string(), "v1".to_string()).await;
        assert_eq!(cache.get("k1").await, Some("v1".to_string()));

        sleep(Duration::from_millis(60)).await;
        assert_eq!(cache.get("k1").await, None);
    }

    #[tokio::test]
    async fn test_invalidate() {
        let cache: TtlCache<String> = TtlCache::new(Duration::from_secs(60), 100);
        cache.set("k1".to_string(), "v1".to_string()).await;
        cache.invalidate("k1").await;
        assert_eq!(cache.get("k1").await, None);
    }

    #[tokio::test]
    async fn test_invalidate_all() {
        let cache: TtlCache<String> = TtlCache::new(Duration::from_secs(60), 100);
        cache.set("k1".to_string(), "v1".to_string()).await;
        cache.set("k2".to_string(), "v2".to_string()).await;
        cache.invalidate_all().await;
        assert!(cache.is_empty().await);
    }

    #[tokio::test]
    async fn test_max_entries_eviction() {
        let cache: TtlCache<String> = TtlCache::new(Duration::from_secs(60), 2);
        cache.set("k1".to_string(), "v1".to_string()).await;
        cache.set("k2".to_string(), "v2".to_string()).await;
        // This should evict the oldest (k1)
        cache.set("k3".to_string(), "v3".to_string()).await;

        assert_eq!(cache.get("k1").await, None);
        assert_eq!(cache.get("k2").await, Some("v2".to_string()));
        assert_eq!(cache.get("k3").await, Some("v3".to_string()));
    }

    #[tokio::test]
    async fn test_overwrite_existing_key() {
        let cache: TtlCache<String> = TtlCache::new(Duration::from_secs(60), 100);
        cache.set("k1".to_string(), "v1".to_string()).await;
        cache.set("k1".to_string(), "v2".to_string()).await;
        assert_eq!(cache.get("k1").await, Some("v2".to_string()));
    }

    #[tokio::test]
    async fn test_len_and_is_empty() {
        let cache: TtlCache<String> = TtlCache::new(Duration::from_secs(60), 100);
        assert!(cache.is_empty().await);
        assert_eq!(cache.len().await, 0);

        cache.set("k1".to_string(), "v1".to_string()).await;
        assert!(!cache.is_empty().await);
        assert_eq!(cache.len().await, 1);
    }
}
