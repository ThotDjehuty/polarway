//! # Polarway Bus — Generic Event Fan-Out
//!
//! A domain-agnostic event bus with:
//! - **Broadcast fan-out**: every subscriber gets every event (unless filtered).
//! - **Per-subscriber drain buffers**: subscribers can drain all pending events
//!   in one batch call (no async required).
//! - **Filter predicates**: subscribers can register a filter closure to ignore
//!   irrelevant events.
//! - **Back-pressure policy**: configurable behavior when a subscriber falls behind.
//!
//! ## Design Principles
//!
//! - Generic over event type `T: Clone + Send + Sync + 'static`
//! - No finance-specific code — usable for signals, logs, metrics, or any domain
//! - Railway-oriented: methods return `Result` where appropriate
//! - Efficient: uses `tokio::sync::broadcast` for the hot path

mod bus;
mod subscriber;
mod error;

pub use bus::EventBus;
pub use subscriber::{Subscriber, FilteredSubscriber};
pub use error::BusError;

/// Result type for bus operations.
pub type BusResult<T> = Result<T, BusError>;
