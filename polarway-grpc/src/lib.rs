pub mod handles;
pub mod service;
pub mod error;
pub mod storage;  // Storage layer: Parquet + DuckDB + Cache
// Temporarily disable optimizations module until Polars 0.52 API compatibility is fixed
// pub mod optimizations;

// Generated proto code
pub mod proto {
    tonic::include_proto!("polaroid.v1");
}

pub use service::PolaroidDataFrameService;
pub use handles::{HandleManager, DataFrameHandleInfo};
pub use error::{PolaroidError, Result};
pub use storage::{StorageBackend, HybridStorage, ParquetBackend, CacheBackend, DuckDBBackend};
