//! Generic storage backend abstraction for Polarway
//!
//! This module provides a trait-based storage layer that supports multiple backends:
//! - Parquet: Cold storage with high compression (zstd level 19)
//! - DuckDB: SQL analytics engine for Parquet queries
//! - Cache: LRU in-memory cache for hot data
//!
//! The `HybridStorage` combines all three for optimal performance:
//! - Check cache first (fast, RAM)
//! - Fall back to Parquet (compressed, disk)
//! - Query via DuckDB (SQL analytics)

use arrow::record_batch::RecordBatch;
use std::error::Error;
use std::sync::Arc;

pub mod cache;
pub mod duckdb_backend;
pub mod parquet_backend;

pub use cache::CacheBackend;
pub use duckdb_backend::DuckDBBackend;
pub use parquet_backend::ParquetBackend;

/// Statistics about storage backend performance
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub total_keys: usize,
    pub total_size_bytes: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub compression_ratio: f64,
}

/// Generic storage backend trait for DataFrame persistence
///
/// All backends must implement thread-safe operations for:
/// - Storing DataFrames (RecordBatch)
/// - Loading DataFrames by key
/// - Querying with SQL (optional)
/// - Listing available keys
/// - Deleting data
/// - Collecting statistics
pub trait StorageBackend: Send + Sync {
    /// Store a DataFrame with the given key
    fn store(&self, key: &str, batch: RecordBatch) -> Result<(), Box<dyn Error>>;

    /// Load a DataFrame by key (returns None if not found)
    fn load(&self, key: &str) -> Result<Option<RecordBatch>, Box<dyn Error>>;

    /// Execute a SQL query (not all backends support this)
    fn query(&self, sql: &str) -> Result<RecordBatch, Box<dyn Error>> {
        Err(format!("SQL queries not supported by this backend").into())
    }

    /// List all available keys
    fn list_keys(&self) -> Result<Vec<String>, Box<dyn Error>>;

    /// Delete data by key
    fn delete(&self, key: &str) -> Result<(), Box<dyn Error>>;

    /// Get storage statistics
    fn stats(&self) -> Result<StorageStats, Box<dyn Error>>;
}

/// Hybrid storage combining cache, cold storage, and SQL analytics
///
/// This is the recommended storage backend for Polarway, providing:
/// - Fast cache lookups (LRU)
/// - Compressed cold storage (Parquet with zstd)
/// - SQL analytics (DuckDB)
///
/// # Architecture
///
/// ```text
/// ┌─────────────┐
/// │   Request   │
/// └──────┬──────┘
///        │
///        ▼
/// ┌─────────────┐  Cache Hit
/// │    Cache    │──────────────► Return
/// │ (LRU, RAM)  │
/// └──────┬──────┘
///        │ Cache Miss
///        ▼
/// ┌─────────────┐
/// │   Parquet   │  Load & Warm Cache
/// │ (Cold, zstd)│──────────────► Return
/// └──────┬──────┘
///        │
///        ▼
/// ┌─────────────┐
/// │   DuckDB    │  SQL Analytics
/// │  (Queries)  │
/// └─────────────┘
/// ```
pub struct HybridStorage {
    /// LRU cache for hot data (typically 1-2 GB)
    cache: Arc<CacheBackend>,
    /// Parquet backend for cold storage (compressed)
    cold_storage: Arc<ParquetBackend>,
    /// DuckDB backend for SQL queries
    duckdb: Arc<DuckDBBackend>,
}

impl HybridStorage {
    /// Create a new hybrid storage with specified paths and cache size
    ///
    /// # Arguments
    /// - `parquet_path`: Directory for Parquet files
    /// - `duckdb_path`: Directory for DuckDB database (or `:memory:`)
    /// - `cache_size_gb`: Maximum cache size in GB (e.g., 2.0 for 2 GB)
    pub fn new(
        parquet_path: String,
        duckdb_path: String,
        cache_size_gb: f64,
    ) -> Result<Self, Box<dyn Error>> {
        let cache = Arc::new(CacheBackend::new(cache_size_gb));
        let cold_storage = Arc::new(ParquetBackend::new(parquet_path)?);
        let duckdb = Arc::new(DuckDBBackend::new(duckdb_path)?);

        Ok(Self {
            cache,
            cold_storage,
            duckdb,
        })
    }

    /// Smart load: check cache first, then Parquet, warm cache on miss
    pub fn smart_load(&self, key: &str) -> Result<Option<RecordBatch>, Box<dyn Error>> {
        // Try cache first
        if let Some(batch) = self.cache.load(key)? {
            return Ok(Some(batch));
        }

        // Cache miss - load from Parquet
        if let Some(batch) = self.cold_storage.load(key)? {
            // Warm the cache for next access
            self.cache.store(key, batch.clone())?;
            return Ok(Some(batch));
        }

        // Not found anywhere
        Ok(None)
    }
}

impl StorageBackend for HybridStorage {
    fn store(&self, key: &str, batch: RecordBatch) -> Result<(), Box<dyn Error>> {
        // Store in both cache and cold storage
        self.cache.store(key, batch.clone())?;
        self.cold_storage.store(key, batch)?;
        Ok(())
    }

    fn load(&self, key: &str) -> Result<Option<RecordBatch>, Box<dyn Error>> {
        self.smart_load(key)
    }

    fn query(&self, sql: &str) -> Result<RecordBatch, Box<dyn Error>> {
        // Delegate SQL queries to DuckDB
        self.duckdb.query(sql)
    }

    fn list_keys(&self) -> Result<Vec<String>, Box<dyn Error>> {
        // List from cold storage (authoritative source)
        self.cold_storage.list_keys()
    }

    fn delete(&self, key: &str) -> Result<(), Box<dyn Error>> {
        // Delete from both cache and cold storage
        self.cache.delete(key)?;
        self.cold_storage.delete(key)?;
        Ok(())
    }

    fn stats(&self) -> Result<StorageStats, Box<dyn Error>> {
        let cache_stats = self.cache.stats()?;
        let cold_stats = self.cold_storage.stats()?;

        Ok(StorageStats {
            total_keys: cold_stats.total_keys,
            total_size_bytes: cold_stats.total_size_bytes,
            cache_hits: cache_stats.cache_hits,
            cache_misses: cache_stats.cache_misses,
            compression_ratio: cold_stats.compression_ratio,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc as ArrowArc;

    fn create_test_batch() -> RecordBatch {
        let schema = ArrowArc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let array = Int64Array::from(vec![1, 2, 3, 4, 5]);
        RecordBatch::try_new(schema, vec![ArrowArc::new(array)]).unwrap()
    }

    #[test]
    fn test_hybrid_storage_lifecycle() {
        let storage = HybridStorage::new(
            "/tmp/test_parquet".to_string(),
            ":memory:".to_string(),
            0.1, // 100 MB cache
        )
        .unwrap();

        let batch = create_test_batch();

        // Store
        storage.store("test_key", batch.clone()).unwrap();

        // Load (should hit cache)
        let loaded = storage.load("test_key").unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().num_rows(), 5);

        // Stats
        let stats = storage.stats().unwrap();
        assert_eq!(stats.total_keys, 1);
        assert_eq!(stats.cache_hits, 1);

        // Delete
        storage.delete("test_key").unwrap();
        let deleted = storage.load("test_key").unwrap();
        assert!(deleted.is_none());
    }
}
