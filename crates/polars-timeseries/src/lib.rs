//! Polars Time-Series Extensions
//!
//! This crate provides time-series specific functionality for Polars DataFrames,
//! focusing on trading and financial workflows:
//!
//! - **TWAP** (Time-Weighted Average Price): Calculate time-weighted averages
//! - **VWAP** (Volume-Weighted Average Price): Calculate volume-weighted averages
//! - **Multi-Frequency Resampling**: Resample data to different time frequencies
//! - **Session Handling**: Split data by trading sessions
//!
//! # Examples
//!
//! ```rust,no_run
//! use polars::prelude::*;
//! use polars_timeseries::{twap, vwap};
//!
//! # fn main() -> PolarsResult<()> {
//! let df = DataFrame::new(vec![
//!     // ... your OHLCV data
//! ])?;
//!
//! // Calculate VWAP
//! let df_with_vwap = vwap(&df, "timestamp", "close", "volume")?;
//!
//! // Calculate TWAP
//! let df_with_twap = twap(&df, "timestamp", "close", "5m")?;
//! # Ok(())
//! # }
//! ```

mod error;
mod vwap;
mod twap;
mod resample;
mod session;

pub use error::{TimeSeriesError, TimeSeriesResult};
pub use vwap::{vwap, vwap_lazy};
pub use twap::{twap, twap_lazy};
pub use resample::{multi_frequency_resample, ResampleConfig};
pub use session::{split_by_session, SessionConfig};
