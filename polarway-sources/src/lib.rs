//! Polaroid Network Data Sources
//! ==============================
//!
//! Network-native data sources for streaming data into Polaroid DataFrames:
//! - WebSocket streams with automatic reconnection
//! - REST API pagination strategies (offset, cursor, link header)
//! - gRPC streaming sources for service-to-service communication
//! - Connection pooling and retry logic
//! - Rate limiting and backpressure handling

pub mod error;
pub mod traits;
pub mod websocket;
pub mod rest;
pub mod grpc_stream;
pub mod connection_pool;
pub mod rate_limiter;

pub use error::{SourceError, Result};
pub use traits::{DataSource, StreamingDataSource};
pub use websocket::{WebSocketSource, WebSocketConfig, ReconnectPolicy};
pub use rest::{RestApiSource, RestApiConfig, PaginationStrategy};
pub use grpc_stream::{GrpcStreamSource, GrpcStreamConfig};
pub use connection_pool::{ConnectionPool, PoolConfig};
pub use rate_limiter::{RateLimiter, RateLimiterConfig};
