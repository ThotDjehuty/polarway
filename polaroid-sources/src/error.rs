//! Error types for polaroid-sources

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SourceError {
    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Authentication failed: {0}")]
    AuthenticationError(String),

    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Invalid schema: {0}")]
    InvalidSchema(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Retry exhausted after {attempts} attempts: {last_error}")]
    RetryExhausted {
        attempts: u32,
        last_error: String,
    },

    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    #[error("HTTP error: {0}")]
    HttpError(String),

    #[error("gRPC error: {0}")]
    GrpcError(String),

    #[error("Arrow error: {0}")]
    ArrowError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Other error: {0}")]
    Other(String),
}

impl From<reqwest::Error> for SourceError {
    fn from(err: reqwest::Error) -> Self {
        SourceError::HttpError(err.to_string())
    }
}

impl From<serde_json::Error> for SourceError {
    fn from(err: serde_json::Error) -> Self {
        SourceError::SerializationError(err.to_string())
    }
}

impl From<arrow::error::ArrowError> for SourceError {
    fn from(err: arrow::error::ArrowError) -> Self {
        SourceError::ArrowError(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, SourceError>;
