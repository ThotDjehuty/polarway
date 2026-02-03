//! Trait definitions for data sources

use crate::error::Result;
use arrow::record_batch::RecordBatch;
use arrow_schema::SchemaRef;
use futures::stream::Stream;
use std::pin::Pin;

/// Trait for all data sources that produce Arrow RecordBatches
pub trait DataSource: Send + Sync {
    /// Get the schema of the data source
    fn schema(&self) -> SchemaRef;

    /// Create a stream of RecordBatches
    fn stream(&self) -> Pin<Box<dyn Stream<Item = Result<RecordBatch>> + Send + '_>>;

    /// Check if the source is available/healthy
    fn is_healthy(&self) -> Pin<Box<dyn std::future::Future<Output = bool> + Send>> {
        Box::pin(async { true })
    }
}

/// Trait for streaming data sources with backpressure support
pub trait StreamingDataSource: DataSource {
    /// Maximum buffer size before applying backpressure
    fn buffer_size(&self) -> usize {
        1000
    }

    /// Whether the source supports reconnection
    fn supports_reconnect(&self) -> bool {
        false
    }

    /// Attempt to reconnect if connection is lost
    fn reconnect(&self) -> Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>;
}
