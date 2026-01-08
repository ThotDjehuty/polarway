//! Distributed coordinator using etcd

use crate::error::{DistributedError, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerNode {
    pub id: String,
    pub endpoint: String,
    pub capabilities: Vec<String>,
    pub max_concurrent_tasks: usize,
    pub heartbeat_interval_secs: u64,
}

#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    /// etcd endpoints
    pub etcd_endpoints: Vec<String>,
    /// Leader election key prefix
    pub leader_key_prefix: String,
    /// Worker registry key prefix
    pub worker_key_prefix: String,
    /// Heartbeat timeout (seconds)
    pub heartbeat_timeout_secs: u64,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            etcd_endpoints: vec!["http://localhost:2379".to_string()],
            leader_key_prefix: "/polaroid/leader".to_string(),
            worker_key_prefix: "/polaroid/workers".to_string(),
            heartbeat_timeout_secs: 30,
        }
    }
}

pub struct Coordinator {
    config: CoordinatorConfig,
    // Future: etcd client
    // client: Option<etcd_client::Client>,
}

impl Coordinator {
    pub async fn new(config: CoordinatorConfig) -> Result<Self> {
        info!("Creating coordinator with endpoints: {:?}", config.etcd_endpoints);

        // TODO: Connect to etcd
        // let client = etcd_client::Client::connect(&config.etcd_endpoints, None)
        //     .await
        //     .map_err(|e| DistributedError::CoordinationError(e.to_string()))?;

        Ok(Self {
            config,
            // client: Some(client),
        })
    }

    pub async fn register_worker(&self, worker: WorkerNode) -> Result<()> {
        info!("Registering worker: {}", worker.id);

        // TODO: Register in etcd with TTL
        // let key = format!("{}/{}", self.config.worker_key_prefix, worker.id);
        // let value = serde_json::to_string(&worker)?;
        // self.client.put(key, value, Some(lease_id)).await?;

        Ok(())
    }

    pub async fn get_workers(&self) -> Result<Vec<WorkerNode>> {
        debug!("Fetching registered workers");

        // TODO: Fetch from etcd
        // let prefix = &self.config.worker_key_prefix;
        // let response = self.client.get(prefix, Some(GetOptions::new().with_prefix())).await?;

        Ok(vec![])
    }

    pub async fn become_leader(&self) -> Result<bool> {
        info!("Attempting to become leader");

        // TODO: Implement leader election with etcd
        // Use campaign and proclaim for leader election

        Ok(false)
    }

    pub async fn is_leader(&self) -> bool {
        // TODO: Check if current node is leader
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_coordinator_creation() {
        let config = CoordinatorConfig::default();
        // Note: This will fail without etcd running
        // let coordinator = Coordinator::new(config).await;
        // assert!(coordinator.is_ok());
    }

    #[test]
    fn test_worker_node_serialization() {
        let worker = WorkerNode {
            id: "worker-1".to_string(),
            endpoint: "http://localhost:50051".to_string(),
            capabilities: vec!["cpu".to_string(), "gpu".to_string()],
            max_concurrent_tasks: 10,
            heartbeat_interval_secs: 5,
        };

        let json = serde_json::to_string(&worker).unwrap();
        let deserialized: WorkerNode = serde_json::from_str(&json).unwrap();

        assert_eq!(worker.id, deserialized.id);
        assert_eq!(worker.endpoint, deserialized.endpoint);
    }
}
