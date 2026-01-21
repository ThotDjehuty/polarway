//! TWAP (Time-Weighted Average Price) calculation
//!
//! TWAP is the average price of a security over a specified time period.
//! Unlike VWAP, it doesn't weight by volume.

use polars::prelude::*;
use crate::error::{TimeSeriesError, TimeSeriesResult};

/// Calculate TWAP for a DataFrame
///
/// # Arguments
/// * `df` - Input DataFrame with time-series data
/// * `time_col` - Name of timestamp column
/// * `price_col` - Name of price column
/// * `window` - Time window (e.g., "5m", "1h", "1d")
///
/// # Returns
/// DataFrame with additional "twap" column
///
/// # Example
/// ```rust,no_run
/// use polars::prelude::*;
/// use polars_timeseries::twap;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let df = DataFrame::new(vec![
///     Series::new("timestamp".into(), vec![1i64, 2, 3, 4, 5]),
///     Series::new("close".into(), vec![100.0, 101.0, 102.0, 101.5, 103.0]),
/// ])?;
///
/// let df_with_twap = twap(&df, "timestamp", "close", "5m")?;
/// # Ok(())
/// # }
/// ```
pub fn twap(
    df: &DataFrame,
    time_col: &str,
    price_col: &str,
    window: &str,
) -> TimeSeriesResult<DataFrame> {
    // Validate columns
    if !df.get_column_names().contains(&time_col) {
        return Err(TimeSeriesError::MissingColumn(time_col.to_string()));
    }
    if !df.get_column_names().contains(&price_col) {
        return Err(TimeSeriesError::MissingColumn(price_col.to_string()));
    }

    if df.height() == 0 {
        return Err(TimeSeriesError::EmptyDataFrame);
    }

    // Convert to LazyFrame for efficient computation
    let lf = df.clone().lazy();
    let result = twap_lazy(lf, time_col, price_col, window)?;
    
    Ok(result.collect()?)
}

/// Calculate TWAP using lazy evaluation
///
/// More efficient for large datasets
pub fn twap_lazy(
    lf: LazyFrame,
    time_col: &str,
    price_col: &str,
    window: &str,
) -> TimeSeriesResult<LazyFrame> {
    // Parse window duration
    let duration = parse_duration(window)?;

    // Calculate rolling mean over time window
    let result = lf
        .sort([time_col], Default::default())
        .with_columns([
            col(price_col)
                .rolling_mean(RollingOptionsFixedWindow {
                    window_size: duration as usize,
                    min_periods: 1,
                    ..Default::default()
                })
                .alias("twap"),
        ]);

    Ok(result)
}

/// Parse duration string (e.g., "5m", "1h", "1d") to milliseconds
fn parse_duration(window: &str) -> TimeSeriesResult<i64> {
    let (value, unit) = window.split_at(window.len() - 1);
    let value: i64 = value
        .parse()
        .map_err(|_| TimeSeriesError::InvalidFrequency(window.to_string()))?;

    let multiplier = match unit {
        "s" => 1_000,           // seconds
        "m" => 60_000,          // minutes
        "h" => 3_600_000,       // hours
        "d" => 86_400_000,      // days
        _ => return Err(TimeSeriesError::InvalidFrequency(window.to_string())),
    };

    Ok(value * multiplier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("5m").unwrap(), 300_000);
        assert_eq!(parse_duration("1h").unwrap(), 3_600_000);
        assert_eq!(parse_duration("1d").unwrap(), 86_400_000);
        assert!(parse_duration("invalid").is_err());
    }

    #[test]
    fn test_twap_calculation() {
        let df = DataFrame::new(vec![
            Series::new("timestamp".into(), vec![1i64, 2, 3, 4, 5]).into(),
            Series::new("close".into(), vec![100.0, 101.0, 102.0, 101.5, 103.0]).into(),
        ])
        .unwrap();

        let result = twap(&df, "timestamp", "close", "3s");
        assert!(result.is_ok());
        
        let result_df = result.unwrap();
        assert!(result_df.column("twap").is_ok());
        assert_eq!(result_df.height(), 5);
    }
}
