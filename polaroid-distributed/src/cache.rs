//! Multi-level caching system

use moka::future::Cache;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CacheKey {
    /// Query SQL or identifier
    pub query: String,
    /// Query parameters hash
    pub params_hash: u64,
    /// Schema version
    pub schema_version: u64,
}

impl CacheKey {
    pub fn new(query: String, params_hash: u64, schema_version: u64) -> Self {
        Self {
            query,
            params_hash,
            schema_version,
        }
    }

    pub fn from_query(query: &str) -> Self {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        query.hash(&mut hasher);

        Self {
            query: query.to_string(),
            params_hash: hasher.finish(),
            schema_version: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of entries
    pub max_capacity: u64,
    /// Time to live (seconds)
    pub ttl_secs: u64,
    /// Time to idle (seconds)
    pub tti_secs: u64,
    /// Enable LRU eviction
    pub enable_lru: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_capacity: 1000,
            ttl_secs: 3600,    // 1 hour
            tti_secs: 1800,    // 30 minutes
            enable_lru: true,
        }
    }
}

#[derive(Clone)]
pub struct CacheLayer<V: Clone + Send + Sync + 'static> {
    cache: Arc<Cache<CacheKey, V>>,
    _config: CacheConfig,
}

impl<V: Clone + Send + Sync + 'static> CacheLayer<V> {
    pub fn new(config: CacheConfig) -> Self {
        let cache = Cache::builder()
            .max_capacity(config.max_capacity)
            .time_to_live(Duration::from_secs(config.ttl_secs))
            .time_to_idle(Duration::from_secs(config.tti_secs))
            .build();

        Self {
            cache: Arc::new(cache),
            _config: config,
        }
    }

    pub async fn get(&self, key: &CacheKey) -> Option<V> {
        let value = self.cache.get(key).await;
        if value.is_some() {
            debug!("Cache HIT for key: {:?}", key.query);
        } else {
            debug!("Cache MISS for key: {:?}", key.query);
        }
        value
    }

    pub async fn insert(&self, key: CacheKey, value: V) {
        debug!("Cache INSERT for key: {:?}", key.query);
        self.cache.insert(key, value).await;
    }

    pub async fn invalidate(&self, key: &CacheKey) {
        debug!("Cache INVALIDATE for key: {:?}", key.query);
        self.cache.invalidate(key).await;
    }

    pub async fn invalidate_all(&self) {
        info!("Cache INVALIDATE ALL");
        self.cache.invalidate_all();
    }

    pub async fn stats(&self) -> CacheStats {
        CacheStats {
            entry_count: self.cache.entry_count(),
            weighted_size: self.cache.weighted_size(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub entry_count: u64,
    pub weighted_size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_layer() {
        let config = CacheConfig {
            max_capacity: 100,
            ttl_secs: 60,
            tti_secs: 30,
            enable_lru: true,
        };

        let cache: CacheLayer<String> = CacheLayer::new(config);

        let key = CacheKey::from_query("SELECT * FROM table");
        let value = "result".to_string();

        // Insert
        cache.insert(key.clone(), value.clone()).await;

        // Get
        let cached = cache.get(&key).await;
        assert_eq!(cached, Some(value));

        // Invalidate
        cache.invalidate(&key).await;
        let cached_after = cache.get(&key).await;
        assert_eq!(cached_after, None);
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let config = CacheConfig::default();
        let cache: CacheLayer<String> = CacheLayer::new(config);

        let key1 = CacheKey::from_query("SELECT 1");
        let key2 = CacheKey::from_query("SELECT 2");

        cache.insert(key1, "result1".to_string()).await;
        cache.insert(key2, "result2".to_string()).await;

        let stats = cache.stats().await;
        assert_eq!(stats.entry_count, 2);
    }
}
