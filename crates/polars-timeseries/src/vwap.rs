//! VWAP (Volume-Weighted Average Price) calculation
//!
//! VWAP is a trading benchmark that gives the average price a security has traded at
//! throughout the day, based on both volume and price.
//!
//! Formula: VWAP = Σ(Price × Volume) / Σ(Volume)

use polars::prelude::*;
use crate::error::{TimeSeriesError, TimeSeriesResult};

/// Calculate VWAP for a DataFrame
///
/// # Arguments
/// * `df` - Input DataFrame with OHLCV data
/// * `time_col` - Name of timestamp column
/// * `price_col` - Name of price column (typically "close" or "typical_price")
/// * `volume_col` - Name of volume column
///
/// # Returns
/// DataFrame with additional "vwap" column
///
/// # Example
/// ```rust,no_run
/// use polars::prelude::*;
/// use polars_timeseries::vwap;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let df = DataFrame::new(vec![
///     Series::new("timestamp".into(), vec![1i64, 2, 3, 4, 5]),
///     Series::new("close".into(), vec![100.0, 101.0, 102.0, 101.5, 103.0]),
///     Series::new("volume".into(), vec![1000i64, 1500, 1200, 1100, 1300]),
/// ])?;
///
/// let df_with_vwap = vwap(&df, "timestamp", "close", "volume")?;
/// # Ok(())
/// # }
/// ```
pub fn vwap(
    df: &DataFrame,
    time_col: &str,
    price_col: &str,
    volume_col: &str,
) -> TimeSeriesResult<DataFrame> {
    // Validate columns exist
    if !df.get_column_names().contains(&time_col) {
        return Err(TimeSeriesError::MissingColumn(time_col.to_string()));
    }
    if !df.get_column_names().contains(&price_col) {
        return Err(TimeSeriesError::MissingColumn(price_col.to_string()));
    }
    if !df.get_column_names().contains(&volume_col) {
        return Err(TimeSeriesError::MissingColumn(volume_col.to_string()));
    }

    if df.height() == 0 {
        return Err(TimeSeriesError::EmptyDataFrame);
    }

    // Calculate VWAP: cumsum(price * volume) / cumsum(volume)
    let price = df.column(price_col)?;
    let volume = df.column(volume_col)?;

    // price * volume
    let pv = (price * volume)?;
    
    // Cumulative sums
    let cum_pv = pv.cum_sum(false)?;
    let cum_volume = volume.cum_sum(false)?;

    // VWAP = cum_pv / cum_volume
    let vwap_series = (&cum_pv / &cum_volume)?;
    let vwap_series = vwap_series.with_name("vwap".into());

    // Add VWAP column to DataFrame
    let mut result = df.clone();
    result.with_column(vwap_series)?;

    Ok(result)
}

/// Calculate VWAP using lazy evaluation
///
/// More efficient for large datasets as it can optimize the query plan
///
/// # Example
/// ```rust,no_run
/// use polars::prelude::*;
/// use polars_timeseries::vwap_lazy;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let df = DataFrame::new(vec![
///     Series::new("timestamp".into(), vec![1i64, 2, 3, 4, 5]),
///     Series::new("close".into(), vec![100.0, 101.0, 102.0, 101.5, 103.0]),
///     Series::new("volume".into(), vec![1000i64, 1500, 1200, 1100, 1300]),
/// ])?;
///
/// let df_with_vwap = vwap_lazy(df.lazy(), "timestamp", "close", "volume")?.collect()?;
/// # Ok(())
/// # }
/// ```
pub fn vwap_lazy(
    lf: LazyFrame,
    time_col: &str,
    price_col: &str,
    volume_col: &str,
) -> TimeSeriesResult<LazyFrame> {
    let result = lf.with_columns([
        // Calculate VWAP
        (col(price_col) * col(volume_col))
            .cum_sum(false)
            .truediv(col(volume_col).cum_sum(false))
            .alias("vwap"),
    ]);

    Ok(result)
}

/// Calculate typical price (HLC/3) for VWAP
///
/// Typical price is often used instead of close price for VWAP calculation:
/// Typical Price = (High + Low + Close) / 3
pub fn typical_price(df: &DataFrame) -> TimeSeriesResult<Series> {
    let high = df.column("high")?;
    let low = df.column("low")?;
    let close = df.column("close")?;

    let typical = ((high + low)? + close)? / 3.0;
    Ok(typical.with_name("typical_price".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vwap_calculation() {
        let df = DataFrame::new(vec![
            Series::new("timestamp".into(), vec![1i64, 2, 3, 4, 5]).into(),
            Series::new("close".into(), vec![100.0, 101.0, 102.0, 101.5, 103.0]).into(),
            Series::new("volume".into(), vec![1000i64, 1500, 1200, 1100, 1300]).into(),
        ])
        .unwrap();

        let result = vwap(&df, "timestamp", "close", "volume").unwrap();
        
        assert!(result.column("vwap").is_ok());
        assert_eq!(result.height(), 5);
        
        // First VWAP should be same as first price
        let vwap_col = result.column("vwap").unwrap();
        let first_vwap = vwap_col.f64().unwrap().get(0).unwrap();
        assert!((first_vwap - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_typical_price() {
        let df = DataFrame::new(vec![
            Series::new("high".into(), vec![105.0, 106.0, 107.0]).into(),
            Series::new("low".into(), vec![95.0, 96.0, 97.0]).into(),
            Series::new("close".into(), vec![100.0, 101.0, 102.0]).into(),
        ])
        .unwrap();

        let typical = typical_price(&df).unwrap();
        let value = typical.f64().unwrap().get(0).unwrap();
        
        // (105 + 95 + 100) / 3 = 100
        assert!((value - 100.0).abs() < 0.01);
    }
}
