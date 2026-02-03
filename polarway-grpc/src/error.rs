use thiserror::Error;
use tonic::Status;

/// Polaroid error types - no panics, only Results
#[derive(Debug, Error)]
pub enum PolaroidError {
    #[error("Column not found: {0}")]
    ColumnNotFound(String),
    
    #[error("Handle not found: {0}")]
    HandleNotFound(String),
    
    #[error("Handle expired: {0}")]
    HandleExpired(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Polars error: {0}")]
    Polars(#[from] polars::error::PolarsError),
    
    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),
    
    #[error("Invalid predicate: {0}")]
    InvalidPredicate(String),
    
    #[error("Invalid expression: {0}")]
    InvalidExpression(String),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<PolaroidError> for Status {
    fn from(err: PolaroidError) -> Self {
        match err {
            PolaroidError::ColumnNotFound(msg) => Status::not_found(msg),
            PolaroidError::HandleNotFound(msg) => Status::not_found(msg),
            PolaroidError::HandleExpired(msg) => Status::deadline_exceeded(msg),
            PolaroidError::InvalidPredicate(msg) => Status::invalid_argument(msg),
            PolaroidError::InvalidExpression(msg) => Status::invalid_argument(msg),
            PolaroidError::Io(e) => Status::internal(e.to_string()),
            PolaroidError::Polars(e) => Status::internal(e.to_string()),
            PolaroidError::Arrow(e) => Status::internal(e.to_string()),
            PolaroidError::Serialization(msg) => Status::internal(msg),
            PolaroidError::Network(msg) => Status::unavailable(msg),
            PolaroidError::Internal(msg) => Status::internal(msg),
        }
    }
}

pub type Result<T> = std::result::Result<T, PolaroidError>;
