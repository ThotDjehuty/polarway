use dashmap::DashMap;
use polars::prelude::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;
use tracing::{debug, info, warn};

use crate::error::{PolaroidError, Result};

/// Information about a DataFrame handle
#[derive(Clone, Debug)]
pub struct DataFrameHandleInfo {
    pub handle: String,
    pub dataframe: Arc<DataFrame>,
    pub created_at: Instant,
    pub last_accessed: Instant,
    pub ttl: Duration,
}

impl DataFrameHandleInfo {
    fn new(dataframe: DataFrame, ttl: Duration) -> Self {
        let now = Instant::now();
        Self {
            handle: Uuid::new_v4().to_string(),
            dataframe: Arc::new(dataframe),
            created_at: now,
            last_accessed: now,
            ttl,
        }
    }
    
    fn is_expired(&self) -> bool {
        self.last_accessed.elapsed() > self.ttl
    }
    
    fn touch(&mut self) {
        self.last_accessed = Instant::now();
    }
}

/// Manages DataFrame handles with TTL and reference counting
pub struct HandleManager {
    handles: DashMap<String, DataFrameHandleInfo>,
    default_ttl: Duration,
}

impl HandleManager {
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            handles: DashMap::new(),
            default_ttl,
        }
    }
    
    /// Create a new handle for a DataFrame
    pub fn create_handle(&self, dataframe: DataFrame) -> String {
        let info = DataFrameHandleInfo::new(dataframe, self.default_ttl);
        let handle = info.handle.clone();
        
        info!("Created handle: {} (shape: {:?})", handle, info.dataframe.shape());
        self.handles.insert(handle.clone(), info);
        
        handle
    }
    
    /// Get DataFrame by handle (updates last_accessed)
    pub fn get_dataframe(&self, handle: &str) -> Result<Arc<DataFrame>> {
        let mut entry = self.handles.get_mut(handle)
            .ok_or_else(|| PolaroidError::HandleNotFound(handle.to_string()))?;
        
        if entry.is_expired() {
            drop(entry);
            self.handles.remove(handle);
            return Err(PolaroidError::HandleExpired(handle.to_string()));
        }
        
        entry.touch();
        debug!("Accessed handle: {}", handle);
        Ok(Arc::clone(&entry.dataframe))
    }
    
    /// Clone a handle (cheap - shares underlying data)
    pub fn clone_handle(&self, handle: &str) -> Result<String> {
        let df = self.get_dataframe(handle)?;
        let new_handle = self.create_handle((*df).clone());
        debug!("Cloned handle {} -> {}", handle, new_handle);
        Ok(new_handle)
    }
    
    /// Drop a handle explicitly
    pub fn drop_handle(&self, handle: &str) -> Result<()> {
        self.handles.remove(handle)
            .ok_or_else(|| PolaroidError::HandleNotFound(handle.to_string()))?;
        info!("Dropped handle: {}", handle);
        Ok(())
    }
    
    /// Extend TTL for a handle (heartbeat)
    pub fn heartbeat(&self, handle: &str) -> Result<()> {
        let mut entry = self.handles.get_mut(handle)
            .ok_or_else(|| PolaroidError::HandleNotFound(handle.to_string()))?;
        
        entry.touch();
        debug!("Heartbeat for handle: {}", handle);
        Ok(())
    }
    
    /// Clean up expired handles
    pub fn cleanup_expired(&self) -> usize {
        let mut removed = 0;
        self.handles.retain(|handle, info| {
            if info.is_expired() {
                warn!("Removing expired handle: {}", handle);
                removed += 1;
                false
            } else {
                true
            }
        });
        
        if removed > 0 {
            info!("Cleaned up {} expired handles", removed);
        }
        
        removed
    }
    
    /// Get total number of active handles
    pub fn handle_count(&self) -> usize {
        self.handles.len()
    }
    
    /// Check if handle exists and is alive
    pub fn is_alive(&self, handle: &str) -> bool {
        if let Some(entry) = self.handles.get(handle) {
            !entry.is_expired()
        } else {
            false
        }
    }
}

impl Default for HandleManager {
    fn default() -> Self {
        Self::new(Duration::from_secs(3600)) // 1 hour default TTL
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polars::prelude::*;
    
    fn create_test_df() -> DataFrame {
        df! {
            "a" => &[1, 2, 3],
            "b" => &[4, 5, 6],
        }.unwrap()
    }
    
    #[test]
    fn test_create_and_get_handle() {
        let manager = HandleManager::default();
        let df = create_test_df();
        
        let handle = manager.create_handle(df);
        let retrieved = manager.get_dataframe(&handle).unwrap();
        
        assert_eq!(retrieved.shape(), (3, 2));
    }
    
    #[test]
    fn test_handle_not_found() {
        let manager = HandleManager::default();
        let result = manager.get_dataframe("nonexistent");
        
        assert!(matches!(result, Err(PolaroidError::HandleNotFound(_))));
    }
    
    #[test]
    fn test_drop_handle() {
        let manager = HandleManager::default();
        let handle = manager.create_handle(create_test_df());
        
        manager.drop_handle(&handle).unwrap();
        let result = manager.get_dataframe(&handle);
        
        assert!(matches!(result, Err(PolaroidError::HandleNotFound(_))));
    }
    
    #[test]
    fn test_clone_handle() {
        let manager = HandleManager::default();
        let handle = manager.create_handle(create_test_df());
        
        let cloned = manager.clone_handle(&handle).unwrap();
        assert_ne!(handle, cloned);
        
        let df1 = manager.get_dataframe(&handle).unwrap();
        let df2 = manager.get_dataframe(&cloned).unwrap();
        assert_eq!(df1.shape(), df2.shape());
    }
    
    #[test]
    fn test_handle_expiration() {
        let manager = HandleManager::new(Duration::from_millis(100));
        let handle = manager.create_handle(create_test_df());
        
        std::thread::sleep(Duration::from_millis(150));
        let result = manager.get_dataframe(&handle);
        
        assert!(matches!(result, Err(PolaroidError::HandleExpired(_))));
    }
}
