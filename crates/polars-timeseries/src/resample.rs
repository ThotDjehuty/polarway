//! Multi-frequency resampling for time-series data

use polars::prelude::*;
use crate::error::{TimeSeriesError, TimeSeriesResult};

/// Configuration for multi-frequency resampling
#[derive(Debug, Clone)]
pub struct ResampleConfig {
    /// Time column name
    pub time_col: String,
    
    /// Target frequency (e.g., "1m", "5m", "1h")
    pub frequency: String,
    
    /// Aggregation rules for each column
    pub aggregations: Vec<(String, AggregationType)>,
}

/// Aggregation types for resampling
#[derive(Debug, Clone)]
pub enum AggregationType {
    /// First value in period
    First,
    
    /// Last value in period
    Last,
    
    /// Mean/average value
    Mean,
    
    /// Sum of values
    Sum,
    
    /// Maximum value
    Max,
    
    /// Minimum value
    Min,
    
    /// OHLC (Open, High, Low, Close)
    OHLC,
}

impl ResampleConfig {
    /// Create a new resample configuration
    pub fn new(time_col: impl Into<String>, frequency: impl Into<String>) -> Self {
        Self {
            time_col: time_col.into(),
            frequency: frequency.into(),
            aggregations: Vec::new(),
        }
    }

    /// Add an aggregation rule
    pub fn with_aggregation(
        mut self,
        column: impl Into<String>,
        agg_type: AggregationType,
    ) -> Self {
        self.aggregations.push((column.into(), agg_type));
        self
    }

    /// Add OHLC aggregation for price data
    pub fn with_ohlc(self, price_col: impl Into<String>) -> Self {
        self.with_aggregation(price_col, AggregationType::OHLC)
    }

    /// Add volume sum aggregation
    pub fn with_volume_sum(self, volume_col: impl Into<String>) -> Self {
        self.with_aggregation(volume_col, AggregationType::Sum)
    }
}

/// Resample time-series data to different frequencies
///
/// # Example
/// ```rust,no_run
/// use polars::prelude::*;
/// use polars_timeseries::{multi_frequency_resample, ResampleConfig, AggregationType};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let df = DataFrame::new(vec![
///     // 1-minute OHLCV data
/// ])?;
///
/// let config = ResampleConfig::new("timestamp", "5m")
///     .with_ohlc("close")
///     .with_volume_sum("volume");
///
/// let resampled = multi_frequency_resample(&df, &config)?;
/// # Ok(())
/// # }
/// ```
pub fn multi_frequency_resample(
    df: &DataFrame,
    config: &ResampleConfig,
) -> TimeSeriesResult<DataFrame> {
    if df.height() == 0 {
        return Err(TimeSeriesError::EmptyDataFrame);
    }

    if !df.get_column_names().contains(&config.time_col.as_str()) {
        return Err(TimeSeriesError::MissingColumn(config.time_col.clone()));
    }

    // Convert to LazyFrame for efficient resampling
    let lf = df.clone().lazy();

    // Parse frequency
    let duration = parse_frequency(&config.frequency)?;

    // Build aggregation expressions
    let mut agg_exprs = Vec::new();
    
    for (col_name, agg_type) in &config.aggregations {
        let expr = match agg_type {
            AggregationType::First => col(col_name).first().alias(col_name),
            AggregationType::Last => col(col_name).last().alias(col_name),
            AggregationType::Mean => col(col_name).mean().alias(col_name),
            AggregationType::Sum => col(col_name).sum().alias(col_name),
            AggregationType::Max => col(col_name).max().alias(col_name),
            AggregationType::Min => col(col_name).min().alias(col_name),
            AggregationType::OHLC => {
                // For OHLC, create separate columns
                continue; // Handle OHLC separately
            }
        };
        agg_exprs.push(expr);
    }

    // Apply group_by with dynamic time window
    let result = lf
        .sort([&config.time_col], Default::default())
        .group_by_dynamic(
            col(&config.time_col),
            [],
            DynamicGroupOptions {
                every: Duration::parse(&config.frequency),
                period: Duration::parse(&config.frequency),
                offset: Duration::parse("0s"),
                ..Default::default()
            },
        )
        .agg(agg_exprs)
        .collect()?;

    Ok(result)
}

/// Parse frequency string to milliseconds
fn parse_frequency(freq: &str) -> TimeSeriesResult<i64> {
    let (value, unit) = freq.split_at(freq.len() - 1);
    let value: i64 = value
        .parse()
        .map_err(|_| TimeSeriesError::InvalidFrequency(freq.to_string()))?;

    let multiplier = match unit {
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => return Err(TimeSeriesError::InvalidFrequency(freq.to_string())),
    };

    Ok(value * multiplier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resample_config_builder() {
        let config = ResampleConfig::new("timestamp", "5m")
            .with_ohlc("close")
            .with_volume_sum("volume");

        assert_eq!(config.time_col, "timestamp");
        assert_eq!(config.frequency, "5m");
        assert_eq!(config.aggregations.len(), 2);
    }

    #[test]
    fn test_parse_frequency() {
        assert_eq!(parse_frequency("1m").unwrap(), 60_000);
        assert_eq!(parse_frequency("5m").unwrap(), 300_000);
        assert_eq!(parse_frequency("1h").unwrap(), 3_600_000);
    }
}
