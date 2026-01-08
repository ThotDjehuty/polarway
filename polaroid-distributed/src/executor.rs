//! Distributed query executor

use crate::error::{DistributedError, Result};
use crate::query_planner::{ExecutionStage, QueryPlan};
use arrow::record_batch::RecordBatch;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Maximum concurrent stages
    pub max_concurrent_stages: usize,
    /// Stage timeout (seconds)
    pub stage_timeout_secs: u64,
    /// Enable result streaming
    pub enable_streaming: bool,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            max_concurrent_stages: 4,
            stage_timeout_secs: 300,
            enable_streaming: true,
        }
    }
}

pub struct DistributedExecutor {
    config: ExecutorConfig,
    workers: Arc<RwLock<HashMap<String, WorkerInfo>>>,
}

#[derive(Debug, Clone)]
pub struct WorkerInfo {
    pub id: String,
    pub endpoint: String,
    pub available: bool,
    pub current_load: usize,
    pub max_load: usize,
}

impl DistributedExecutor {
    pub fn new(config: ExecutorConfig) -> Self {
        Self {
            config,
            workers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_worker(&self, worker: WorkerInfo) {
        let mut workers = self.workers.write().await;
        info!("Registering worker: {}", worker.id);
        workers.insert(worker.id.clone(), worker);
    }

    pub async fn execute(&self, plan: QueryPlan) -> Result<Vec<RecordBatch>> {
        info!("Executing query plan: {}", plan.id);

        // Assign stages to workers
        let assigned_plan = self.assign_stages(plan).await?;

        // Execute stages in dependency order
        let results = self.execute_stages(assigned_plan).await?;

        Ok(results)
    }

    async fn assign_stages(&self, mut plan: QueryPlan) -> Result<QueryPlan> {
        let workers = self.workers.read().await;

        if workers.is_empty() {
            return Err(DistributedError::NoWorkersAvailable);
        }

        // Simple round-robin assignment
        let available_workers: Vec<&WorkerInfo> = workers
            .values()
            .filter(|w| w.available && w.current_load < w.max_load)
            .collect();

        if available_workers.is_empty() {
            return Err(DistributedError::NoWorkersAvailable);
        }

        for (idx, stage) in plan.stages.iter_mut().enumerate() {
            let worker = available_workers[idx % available_workers.len()];
            stage.assigned_worker = Some(worker.id.clone());
            debug!("Assigned stage {} to worker {}", stage.id, worker.id);
        }

        Ok(plan)
    }

    async fn execute_stages(&self, plan: QueryPlan) -> Result<Vec<RecordBatch>> {
        // TODO: Implement actual distributed execution
        // For now, return empty result
        info!("Executing {} stages", plan.stages.len());
        
        for stage in &plan.stages {
            info!(
                "Stage {}: {} on worker {:?}",
                stage.id, stage.description, stage.assigned_worker
            );
        }

        Ok(vec![])
    }

    pub async fn worker_count(&self) -> usize {
        self.workers.read().await.len()
    }

    pub async fn available_workers(&self) -> usize {
        self.workers
            .read()
            .await
            .values()
            .filter(|w| w.available && w.current_load < w.max_load)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_planner::QueryPlanner;

    #[tokio::test]
    async fn test_distributed_executor() {
        let config = ExecutorConfig::default();
        let executor = DistributedExecutor::new(config);

        // Register workers
        let worker1 = WorkerInfo {
            id: "worker-1".to_string(),
            endpoint: "http://localhost:50051".to_string(),
            available: true,
            current_load: 0,
            max_load: 10,
        };

        let worker2 = WorkerInfo {
            id: "worker-2".to_string(),
            endpoint: "http://localhost:50052".to_string(),
            available: true,
            current_load: 0,
            max_load: 10,
        };

        executor.register_worker(worker1).await;
        executor.register_worker(worker2).await;

        assert_eq!(executor.worker_count().await, 2);
        assert_eq!(executor.available_workers().await, 2);

        // Create and execute plan
        let planner = QueryPlanner::new();
        let plan = planner.plan("SELECT * FROM table").unwrap();

        let result = executor.execute(plan).await;
        assert!(result.is_ok());
    }
}
