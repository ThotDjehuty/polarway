//! gRPC streaming data source

use crate::error::{Result, SourceError};
use crate::traits::DataSource;
use arrow::record_batch::RecordBatch;
use arrow_schema::SchemaRef;
use async_stream::stream;
use futures::stream::Stream;
use std::pin::Pin;
use tonic::transport::Channel;
use tracing::{debug, info};

#[derive(Debug, Clone)]
pub struct GrpcStreamConfig {
    /// gRPC endpoint (e.g., "http://localhost:50051")
    pub endpoint: String,
    /// Service name
    pub service: String,
    /// Method name
    pub method: String,
    /// Request message (as JSON)
    pub request: Option<String>,
    /// Connection timeout (seconds)
    pub timeout_secs: u64,
}

pub struct GrpcStreamSource {
    config: GrpcStreamConfig,
    schema: SchemaRef,
}

impl GrpcStreamSource {
    pub fn new(config: GrpcStreamConfig, schema: SchemaRef) -> Self {
        Self { config, schema }
    }

    async fn connect(&self) -> Result<Channel> {
        let endpoint = Channel::from_shared(self.config.endpoint.clone())
            .map_err(|e| SourceError::GrpcError(format!("Invalid endpoint: {}", e)))?;

        let channel = endpoint
            .connect()
            .await
            .map_err(|e| SourceError::GrpcError(format!("Connection failed: {}", e)))?;

        Ok(channel)
    }
}

impl DataSource for GrpcStreamSource {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn stream(&self) -> Pin<Box<dyn Stream<Item = Result<RecordBatch>> + Send + '_>> {
        let config = self.config.clone();

        let s = stream! {
            info!("Connecting to gRPC endpoint: {}", config.endpoint);

            match self.connect().await {
                Ok(_channel) => {
                    debug!("gRPC connection established");
                    // TODO: Implement actual gRPC streaming
                    // This requires generated protobuf code
                    yield Err(SourceError::GrpcError(
                        "gRPC streaming not yet fully implemented - requires protobuf codegen".to_string()
                    ));
                }
                Err(e) => {
                    yield Err(e);
                }
            }
        };

        Box::pin(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;

    #[test]
    fn test_grpc_stream_config() {
        let config = GrpcStreamConfig {
            endpoint: "http://localhost:50051".to_string(),
            service: "DataService".to_string(),
            method: "StreamData".to_string(),
            request: Some(r#"{"query": "SELECT * FROM table"}"#.to_string()),
            timeout_secs: 30,
        };

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let source = GrpcStreamSource::new(config, schema.clone());

        assert_eq!(source.schema(), schema);
    }
}
