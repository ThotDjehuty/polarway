// Storage layer abstraction for Polarway
// Supports: Parquet, DuckDB, In-Memory cache

pub mod parquet_backend;
pub mod duckdb_backend;
pub mod cache;

use arrow::record_batch::RecordBatch;
use std::error::Error;

/// Storage backend trait
pub trait StorageBackend: Send + Sync {
    /// Store a DataFrame (RecordBatch)
    fn store(&self, key: &str, batch: RecordBatch) -> Result<(), Box<dyn Error>>;
    
    /// Load a DataFrame by key
    fn load(&self, key: &str) -> Result<Option<RecordBatch>, Box<dyn Error>>;
    
    /// Query with SQL
    fn query(&self, sql: &str) -> Result<RecordBatch, Box<dyn Error>>;
    
    /// List available keys
    fn list_keys(&self) -> Result<Vec<String>, Box<dyn Error>>;
    
    /// Delete key
    fn delete(&self, key: &str) -> Result<(), Box<dyn Error>>;
    
    /// Get storage statistics
    fn stats(&self) -> Result<StorageStats, Box<dyn Error>>;
}

#[derive(Debug, Clone)]
pub struct StorageStats {
    pub total_size_bytes: u64,
    pub total_keys: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub compression_ratio: f64,
}

/// Hybrid storage: Hot cache (RAM) + Cold storage (Parquet)
pub struct HybridStorage {
    cache: cache::CacheBackend,
    cold_storage: parquet_backend::ParquetBackend,
    duckdb: duckdb_backend::DuckDBBackend,
}

impl HybridStorage {
    pub fn new(parquet_path: &str, cache_size_gb: usize) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            cache: cache::CacheBackend::new(cache_size_gb),
            cold_storage: parquet_backend::ParquetBackend::new(parquet_path)?,
            duckdb: duckdb_backend::DuckDBBackend::new(parquet_path)?,
        })
    }
    
    /// Smart load: Check cache first, then cold storage
    pub fn smart_load(&self, key: &str) -> Result<Option<RecordBatch>, Box<dyn Error>> {
        // Try cache first
        if let Some(batch) = self.cache.load(key)? {
            return Ok(Some(batch));
        }
        
        // Load from cold storage
        if let Some(batch) = self.cold_storage.load(key)? {
            // Warm cache
            self.cache.store(key, batch.clone())?;
            return Ok(Some(batch));
        }
        
        Ok(None)
    }
    
    /// Query with DuckDB (SQL on Parquet)
    pub fn sql_query(&self, sql: &str) -> Result<RecordBatch, Box<dyn Error>> {
        self.duckdb.query(sql)
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
        self.sql_query(sql)
    }
    
    fn list_keys(&self) -> Result<Vec<String>, Box<dyn Error>> {
        self.cold_storage.list_keys()
    }
    
    fn delete(&self, key: &str) -> Result<(), Box<dyn Error>> {
        self.cache.delete(key)?;
        self.cold_storage.delete(key)?;
        Ok(())
    }
    
    fn stats(&self) -> Result<StorageStats, Box<dyn Error>> {
        let cache_stats = self.cache.stats()?;
        let cold_stats = self.cold_storage.stats()?;
        
        Ok(StorageStats {
            total_size_bytes: cold_stats.total_size_bytes,
            total_keys: cold_stats.total_keys,
            cache_hits: cache_stats.cache_hits,
            cache_misses: cache_stats.cache_misses,
            compression_ratio: cold_stats.compression_ratio,
        })
    }
}
