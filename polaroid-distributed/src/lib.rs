//! Polaroid Distributed Query Execution & Caching
//! ==============================================
//!
//! Distributed query execution across multiple Polaroid nodes:
//! - Query planning and distribution
//! - Work stealing and load balancing
//! - Multi-level caching (memory, disk, distributed)
//! - Result aggregation

pub mod error;
pub mod query_planner;
pub mod executor;
pub mod cache;
pub mod coordinator;

pub use error::{DistributedError, Result};
pub use query_planner::{QueryPlan, QueryPlanner};
pub use executor::{DistributedExecutor, ExecutorConfig};
pub use cache::{CacheLayer, CacheConfig, CacheKey};
pub use coordinator::{Coordinator, CoordinatorConfig, WorkerNode};
