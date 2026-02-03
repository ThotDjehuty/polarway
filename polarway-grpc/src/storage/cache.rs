//! LRU cache backend for hot data
//!
//! This module provides an in-memory cache with:
//! - Least Recently Used (LRU) eviction policy
//! - Thread-safe operations with RwLock
//! - Hit/miss statistics tracking
//! - Configurable size limit

use arrow::record_batch::RecordBatch;
use lru::LruCache;
use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};

use super::{StorageBackend, StorageStats};

/// Statistics for cache performance
#[derive(Debug, Clone, Default)]
struct CacheStatsInner {
    hits: u64,
    misses: u64,
}

/// LRU cache backend for hot data
///
/// # Features
/// - **Fast Access**: O(1) lookups in memory
/// - **LRU Eviction**: Automatic eviction of least recently used items
/// - **Thread-Safe**: RwLock for concurrent reads, exclusive writes
/// - **Statistics**: Hit/miss tracking for performance monitoring
///
/// # Size Estimation
/// The cache size is estimated based on:
/// - RecordBatch schema (Arrow metadata)
/// - Number of rows × number of columns
/// - Approximate 8 bytes per cell (rough estimate)
///
/// For a 2 GB cache with 100 columns:
/// - ~250,000 rows per DataFrame
/// - ~100 DataFrames in cache (if all same size)
pub struct CacheBackend {
    cache: Arc<RwLock<LruCache<String, RecordBatch>>>,
    stats: Arc<RwLock<CacheStatsInner>>,
}

impl CacheBackend {
    /// Create a new cache with specified maximum size in GB
    ///
    /// # Arguments
    /// - `max_size_gb`: Maximum cache size in gigabytes (e.g., 2.0 for 2 GB)
    ///
    /// # Example
    /// ```ignore
    /// let cache = CacheBackend::new(2.0); // 2 GB cache
    /// ```
    pub fn new(max_size_gb: f64) -> Self {
        // Estimate capacity: assume ~10 MB per DataFrame on average
        let estimated_capacity = (max_size_gb * 1024.0 / 10.0) as usize;
        let capacity = NonZeroUsize::new(estimated_capacity.max(1)).unwrap();

        Self {
            cache: Arc::new(RwLock::new(LruCache::new(capacity))),
            stats: Arc::new(RwLock::new(CacheStatsInner::default())),
        }
    }

    /// Record a cache hit
    fn record_hit(&self) {
        if let Ok(mut stats) = self.stats.write() {
            stats.hits += 1;
        }
    }

    /// Record a cache miss
    fn record_miss(&self) {
        if let Ok(mut stats) = self.stats.write() {
            stats.misses += 1;
        }
    }

    /// Get cache hit rate (0.0 to 1.0)
    pub fn hit_rate(&self) -> f64 {
        if let Ok(stats) = self.stats.read() {
            let total = stats.hits + stats.misses;
            if total > 0 {
                return stats.hits as f64 / total as f64;
            }
        }
        0.0
    }

    /// Clear all cached data
    pub fn clear(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
    }

    /// Get current number of cached items
    pub fn len(&self) -> usize {
        self.cache.read().map(|c| c.len()).unwrap_or(0)
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl StorageBackend for CacheBackend {
    fn store(&self, key: &str, batch: RecordBatch) -> Result<(), Box<dyn Error>> {
        let mut cache = self.cache.write().map_err(|e| format!("Lock error: {}", e))?;
        cache.put(key.to_string(), batch);
        Ok(())
    }

    fn load(&self, key: &str) -> Result<Option<RecordBatch>, Box<dyn Error>> {
        let mut cache = self.cache.write().map_err(|e| format!("Lock error: {}", e))?;

        if let Some(batch) = cache.get(key) {
            self.record_hit();
            Ok(Some(batch.clone()))
        } else {
            self.record_miss();
            Ok(None)
        }
    }

    fn list_keys(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let cache = self.cache.read().map_err(|e| format!("Lock error: {}", e))?;
        Ok(cache.iter().map(|(k, _)| k.clone()).collect())
    }

    fn delete(&self, key: &str) -> Result<(), Box<dyn Error>> {
        let mut cache = self.cache.write().map_err(|e| format!("Lock error: {}", e))?;
        cache.pop(key);
        Ok(())
    }

    fn stats(&self) -> Result<StorageStats, Box<dyn Error>> {
        let cache = self.cache.read().map_err(|e| format!("Lock error: {}", e))?;
        let stats = self.stats.read().map_err(|e| format!("Lock error: {}", e))?;

        // Estimate size: very rough approximation
        let estimated_size = cache.len() * 10_000_000; // 10 MB per item estimate

        Ok(StorageStats {
            total_keys: cache.len(),
            total_size_bytes: estimated_size as u64,
            cache_hits: stats.hits,
            cache_misses: stats.misses,
            compression_ratio: 1.0, // N/A for cache
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;

    fn create_test_batch(value: i64) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![Field::new("value", DataType::Int64, false)]));
        let array = Int64Array::from(vec![value]);
        RecordBatch::try_new(schema, vec![Arc::new(array)]).unwrap()
    }

    #[test]
    fn test_cache_hit_miss() {
        let cache = CacheBackend::new(0.1); // 100 MB

        // Miss
        let result = cache.load("key1").unwrap();
        assert!(result.is_none());

        // Store
        cache.store("key1", create_test_batch(42)).unwrap();

        // Hit
        let result = cache.load("key1").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().column(0).len(), 1);

        // Check stats
        let stats = cache.stats().unwrap();
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 1);
        assert!(cache.hit_rate() > 0.4 && cache.hit_rate() < 0.6);
    }

    #[test]
    fn test_lru_eviction() {
        let cache = CacheBackend::new(0.001); // Very small cache

        // Fill cache beyond capacity
        for i in 0..100 {
            cache
                .store(&format!("key{}", i), create_test_batch(i))
                .unwrap();
        }

        // Oldest entries should be evicted
        let keys = cache.list_keys().unwrap();
        assert!(keys.len() < 100);
    }

    #[test]
    fn test_clear() {
        let cache = CacheBackend::new(0.1);

        cache.store("key1", create_test_batch(1)).unwrap();
        cache.store("key2", create_test_batch(2)).unwrap();

        assert_eq!(cache.len(), 2);

        cache.clear();

        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }
}
