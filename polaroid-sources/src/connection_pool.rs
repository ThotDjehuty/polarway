//! Connection pooling for data sources

use crate::error::{Result, SourceError};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum number of connections per endpoint
    pub max_connections: usize,
    /// Connection idle timeout (seconds)
    pub idle_timeout_secs: u64,
    /// Connection max lifetime (seconds)
    pub max_lifetime_secs: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            idle_timeout_secs: 300,
            max_lifetime_secs: 3600,
        }
    }
}

pub struct Connection {
    pub endpoint: String,
    pub created_at: std::time::Instant,
    pub last_used: std::time::Instant,
}

impl Connection {
    pub fn new(endpoint: String) -> Self {
        let now = std::time::Instant::now();
        Self {
            endpoint,
            created_at: now,
            last_used: now,
        }
    }

    pub fn is_expired(&self, config: &PoolConfig) -> bool {
        let now = std::time::Instant::now();

        // Check max lifetime
        if now.duration_since(self.created_at).as_secs() > config.max_lifetime_secs {
            return true;
        }

        // Check idle timeout (>= for 0 timeout)
        if config.idle_timeout_secs > 0 && now.duration_since(self.last_used).as_secs() >= config.idle_timeout_secs {
            return true;
        }

        false
    }

    pub fn touch(&mut self) {
        self.last_used = std::time::Instant::now();
    }
}

pub struct ConnectionPool {
    config: PoolConfig,
    pools: Arc<RwLock<HashMap<String, Vec<Connection>>>>,
}

impl ConnectionPool {
    pub fn new(config: PoolConfig) -> Self {
        Self {
            config,
            pools: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn acquire(&self, endpoint: &str) -> Result<Connection> {
        let mut pools = self.pools.write().await;

        // Get or create pool for endpoint
        let pool = pools.entry(endpoint.to_string()).or_insert_with(Vec::new);

        // Remove expired connections
        pool.retain(|conn| !conn.is_expired(&self.config));

        // Try to reuse existing connection
        if let Some(mut conn) = pool.pop() {
            conn.touch();
            debug!("Reused connection to {}", endpoint);
            return Ok(conn);
        }

        // Check if we can create new connection
        if pool.len() >= self.config.max_connections {
            return Err(SourceError::ConnectionError(format!(
                "Connection pool exhausted for {} (max: {})",
                endpoint, self.config.max_connections
            )));
        }

        // Create new connection
        info!("Creating new connection to {}", endpoint);
        let conn = Connection::new(endpoint.to_string());
        Ok(conn)
    }

    pub async fn release(&self, conn: Connection) {
        let mut pools = self.pools.write().await;

        if conn.is_expired(&self.config) {
            debug!("Connection expired, not returning to pool");
            return;
        }

        let pool = pools.entry(conn.endpoint.clone()).or_insert_with(Vec::new);

        if pool.len() < self.config.max_connections {
            pool.push(conn);
            debug!("Returned connection to pool");
        }
    }

    pub async fn stats(&self) -> HashMap<String, usize> {
        let pools = self.pools.read().await;
        pools.iter().map(|(k, v)| (k.clone(), v.len())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_pool() {
        let config = PoolConfig {
            max_connections: 2,
            idle_timeout_secs: 300,
            max_lifetime_secs: 3600,
        };

        let pool = ConnectionPool::new(config);

        // Acquire first connection
        let conn1 = pool.acquire("http://localhost:8080").await.unwrap();
        assert_eq!(conn1.endpoint, "http://localhost:8080");

        // Release it
        pool.release(conn1).await;

        // Acquire again - should reuse
        let conn2 = pool.acquire("http://localhost:8080").await.unwrap();
        assert_eq!(conn2.endpoint, "http://localhost:8080");
    }

    #[test]
    fn test_connection_expiration() {
        let config = PoolConfig {
            max_connections: 10,
            idle_timeout_secs: 0, // Expire immediately
            max_lifetime_secs: 3600,
        };

        let conn = Connection::new("http://localhost:8080".to_string());
        std::thread::sleep(std::time::Duration::from_millis(10));

        assert!(conn.is_expired(&config));
    }
}
