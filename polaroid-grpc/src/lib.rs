pub mod handles;
pub mod service;
pub mod error;
pub mod optimizations;

// Generated proto code
pub mod proto {
    tonic::include_proto!("polaroid.v1");
}

pub use service::PolaroidDataFrameService;
pub use handles::{HandleManager, DataFrameHandleInfo};
pub use error::{PolaroidError, Result};
