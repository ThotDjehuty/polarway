//! Bus error types.

use std::fmt;

/// Errors that can occur during bus operations.
#[derive(Debug)]
pub enum BusError {
    /// No subscribers are currently listening.
    NoSubscribers,
    /// The subscriber's buffer overflowed and events were lost.
    Lagged(u64),
    /// The bus has been shut down.
    Closed,
    /// A custom error message.
    Other(String),
}

impl fmt::Display for BusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BusError::NoSubscribers => write!(f, "no subscribers listening"),
            BusError::Lagged(n) => write!(f, "subscriber lagged, missed {n} events"),
            BusError::Closed => write!(f, "bus closed"),
            BusError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for BusError {}
