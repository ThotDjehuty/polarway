//! Error types for distributed execution

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DistributedError {
    #[error("Query planning error: {0}")]
    QueryPlanningError(String),

    #[error("Execution error on worker {worker}: {error}")]
    ExecutionError { worker: String, error: String },

    #[error("Communication error: {0}")]
    CommunicationError(String),

    #[error("No available workers")]
    NoWorkersAvailable,

    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("Coordination error: {0}")]
    CoordinationError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Arrow error: {0}")]
    ArrowError(String),

    #[error("DataFusion error: {0}")]
    DataFusionError(String),

    #[error("Worker timeout: {0}")]
    WorkerTimeout(String),

    #[error("Invalid configuration: {0}")]
    ConfigError(String),

    #[error("Other error: {0}")]
    Other(String),
}

impl From<arrow::error::ArrowError> for DistributedError {
    fn from(err: arrow::error::ArrowError) -> Self {
        DistributedError::ArrowError(err.to_string())
    }
}

impl From<datafusion::error::DataFusionError> for DistributedError {
    fn from(err: datafusion::error::DataFusionError) -> Self {
        DistributedError::DataFusionError(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, DistributedError>;
