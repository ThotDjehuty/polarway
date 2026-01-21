//! Error types for polars-timeseries

use thiserror::Error;

/// Result type for time-series operations
pub type TimeSeriesResult<T> = Result<T, TimeSeriesError>;

/// Error types for time-series operations
#[derive(Error, Debug)]
pub enum TimeSeriesError {
    /// Polars error
    #[error("Polars error: {0}")]
    Polars(#[from] polars::error::PolarsError),

    /// Missing column error
    #[error("Missing required column: {0}")]
    MissingColumn(String),

    /// Invalid time column
    #[error("Invalid time column: {0}")]
    InvalidTimeColumn(String),

    /// Invalid frequency
    #[error("Invalid frequency: {0}")]
    InvalidFrequency(String),

    /// Empty DataFrame
    #[error("DataFrame is empty")]
    EmptyDataFrame,

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}
