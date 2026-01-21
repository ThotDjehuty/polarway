//! Trading session handling
//!
//! Split time-series data by trading sessions (e.g., separate pre-market, regular, after-hours)

use polars::prelude::*;
use chrono::{NaiveTime, Timelike};
use crate::error::{TimeSeriesError, TimeSeriesResult};

/// Configuration for session splitting
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Time column name
    pub time_col: String,
    
    /// Session definitions
    pub sessions: Vec<Session>,
}

/// Trading session definition
#[derive(Debug, Clone)]
pub struct Session {
    /// Session name (e.g., "pre_market", "regular", "after_hours")
    pub name: String,
    
    /// Session start time (HH:MM format)
    pub start: NaiveTime,
    
    /// Session end time (HH:MM format)
    pub end: NaiveTime,
}

impl SessionConfig {
    /// Create a new session configuration
    pub fn new(time_col: impl Into<String>) -> Self {
        Self {
            time_col: time_col.into(),
            sessions: Vec::new(),
        }
    }

    /// Add a session
    pub fn with_session(
        mut self,
        name: impl Into<String>,
        start: NaiveTime,
        end: NaiveTime,
    ) -> Self {
        self.sessions.push(Session {
            name: name.into(),
            start,
            end,
        });
        self
    }

    /// Add US equity market sessions
    pub fn with_us_equity_sessions(self) -> Self {
        self.with_session(
            "pre_market",
            NaiveTime::from_hms_opt(4, 0, 0).unwrap(),
            NaiveTime::from_hms_opt(9, 30, 0).unwrap(),
        )
        .with_session(
            "regular",
            NaiveTime::from_hms_opt(9, 30, 0).unwrap(),
            NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
        )
        .with_session(
            "after_hours",
            NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
            NaiveTime::from_hms_opt(20, 0, 0).unwrap(),
        )
    }
}

/// Split DataFrame by trading sessions
///
/// # Example
/// ```rust,no_run
/// use polars::prelude::*;
/// use polars_timeseries::{split_by_session, SessionConfig};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let df = DataFrame::new(vec![
///     // Time-series data with timestamps
/// ])?;
///
/// let config = SessionConfig::new("timestamp")
///     .with_us_equity_sessions();
///
/// let sessions = split_by_session(&df, &config)?;
/// 
/// // Access individual sessions
/// let regular_session = &sessions["regular"];
/// # Ok(())
/// # }
/// ```
pub fn split_by_session(
    df: &DataFrame,
    config: &SessionConfig,
) -> TimeSeriesResult<std::collections::HashMap<String, DataFrame>> {
    use std::collections::HashMap;

    if df.height() == 0 {
        return Err(TimeSeriesError::EmptyDataFrame);
    }

    if !df.get_column_names().contains(&config.time_col.as_str()) {
        return Err(TimeSeriesError::MissingColumn(config.time_col.clone()));
    }

    let mut result = HashMap::new();

    // Extract time column
    let time_col = df.column(&config.time_col)?;

    // For each session, filter data
    for session in &config.sessions {
        // Create filter for this session's time range
        let mask = create_session_mask(time_col, &session.start, &session.end)?;
        
        // Filter DataFrame
        let session_df = df.filter(&mask)?;
        
        result.insert(session.name.clone(), session_df);
    }

    Ok(result)
}

/// Create boolean mask for session time range
fn create_session_mask(
    time_col: &Series,
    start: &NaiveTime,
    end: &NaiveTime,
) -> TimeSeriesResult<Series> {
    // This is a simplified implementation
    // In practice, you'd need to handle the time column type properly
    // (could be DateTime, Timestamp, etc.)
    
    let start_seconds = start.num_seconds_from_midnight() as i64;
    let end_seconds = end.num_seconds_from_midnight() as i64;

    // Convert timestamps to time-of-day
    // This depends on the actual time column type
    // For now, create a placeholder
    let mask = Series::new(
        "mask".into(),
        vec![true; time_col.len()],
    );

    Ok(mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_config_builder() {
        let config = SessionConfig::new("timestamp")
            .with_us_equity_sessions();

        assert_eq!(config.time_col, "timestamp");
        assert_eq!(config.sessions.len(), 3);
        assert_eq!(config.sessions[0].name, "pre_market");
        assert_eq!(config.sessions[1].name, "regular");
        assert_eq!(config.sessions[2].name, "after_hours");
    }

    #[test]
    fn test_session_times() {
        let config = SessionConfig::new("timestamp")
            .with_us_equity_sessions();

        let regular = &config.sessions[1];
        assert_eq!(regular.start.hour(), 9);
        assert_eq!(regular.start.minute(), 30);
        assert_eq!(regular.end.hour(), 16);
        assert_eq!(regular.end.minute(), 0);
    }
}
