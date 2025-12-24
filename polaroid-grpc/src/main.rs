use tonic::transport::Server;
use std::net::SocketAddr;
use tracing::{info, Level};
use tracing_subscriber;

// Re-export for library usage
pub mod handles;
pub mod service;
pub mod error;

// Generated proto code
pub mod proto {
    tonic::include_proto!("polaroid.v1");
}

use service::PolaroidDataFrameService;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();
    
    // Read bind address from environment or default
    let bind_addr = std::env::var("POLAROID_BIND_ADDRESS")
        .unwrap_or_else(|_| "0.0.0.0:50051".to_string());
    let addr: SocketAddr = bind_addr.parse()?;
    
    info!("🎬 Polaroid gRPC Server starting...");
    info!("📍 Binding to: {}", addr);
    info!("🚀 FDAP Stack: Flight-DataFusion-Arrow-Parquet");
    info!("📊 DataFrame operations via gRPC");
    info!("⚡ Zero-copy Arrow IPC streaming");
    info!("📈 Time-series native support");
    info!("🌐 Network data sources ready");
    
    // Create service
    let dataframe_service = PolaroidDataFrameService::new();
    
    info!("✅ Server ready! Listening on {}", addr);
    
    // Start server
    Server::builder()
        .add_service(proto::data_frame_service_server::DataFrameServiceServer::new(dataframe_service))
        .serve(addr)
        .await?;
    
    Ok(())
}
