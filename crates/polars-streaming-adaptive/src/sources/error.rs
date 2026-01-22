//! Error types for streaming sources

use std::fmt;

#[derive(Debug)]
pub enum SourceError {
    /// IO error
    Io(std::io::Error),
    /// Network error
    Network(String),
    /// Authentication error
    Auth(String),
    /// Configuration error
    Config(String),
    /// Unsupported source type
    UnsupportedSource(String),
    /// Unsupported operation
    UnsupportedOperation(String),
    /// Memory limit exceeded
    MemoryLimit(usize),
    /// Empty source (no data)
    EmptySource,
    /// Polars error
    PolarsError(String),
    /// Cloud provider error
    CloudError(String),
    /// Database error
    DatabaseError(String),
    /// Kafka error
    KafkaError(String),
    /// Parsing error
    ParseError(String),
    /// Other error
    Other(String),
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Network(e) => write!(f, "Network error: {}", e),
            Self::Auth(e) => write!(f, "Authentication error: {}", e),
            Self::Config(e) => write!(f, "Configuration error: {}", e),
            Self::UnsupportedSource(s) => write!(f, "Unsupported source: {}", s),
            Self::UnsupportedOperation(op) => write!(f, "Unsupported operation: {}", op),
            Self::MemoryLimit(limit) => write!(f, "Memory limit exceeded: {} bytes", limit),
            Self::EmptySource => write!(f, "Empty source (no data)"),
            Self::PolarsError(e) => write!(f, "Polars error: {}", e),
            Self::CloudError(e) => write!(f, "Cloud provider error: {}", e),
            Self::DatabaseError(e) => write!(f, "Database error: {}", e),
            Self::KafkaError(e) => write!(f, "Kafka error: {}", e),
            Self::ParseError(e) => write!(f, "Parse error: {}", e),
            Self::Other(e) => write!(f, "Error: {}", e),
        }
    }
}

impl std::error::Error for SourceError {}

impl From<std::io::Error> for SourceError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<PolarsError> for SourceError {
    fn from(err: PolarsError) -> Self {
        Self::PolarsError(err.to_string())
    }
}

pub type SourceResult<T> = Result<T, SourceError>;
