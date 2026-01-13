use std::sync::Arc;

use polars::prelude::*;

use crate::error::{PolaroidError, Result};
use crate::handles::HandleManager;
use crate::state_store::{default_state_dir, FileSystemStateStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleStoreMode {
    InMemory,
    External,
}

/// Handle format for external (stateless) mode.
///
/// Minimal-effort format: `ext:fs:<key>`
/// - scheme `ext` indicates external state
/// - backend `fs` indicates filesystem store
/// - key is a UUID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalHandleRef {
    pub backend: String,
    pub key: String,
}

pub fn encode_external_handle(backend: &str, key: &str) -> String {
    format!("ext:{backend}:{key}")
}

pub fn decode_external_handle(handle: &str) -> Option<ExternalHandleRef> {
    let mut parts = handle.splitn(3, ':');
    let scheme = parts.next()?;
    if scheme != "ext" {
        return None;
    }
    let backend = parts.next()?.to_string();
    let key = parts.next()?.to_string();
    Some(ExternalHandleRef { backend, key })
}

/// Concrete handle provider implementation.
///
/// We use an enum (instead of a trait object) to keep this minimal-effort and
/// avoid dynamic-dispatch/object-safety issues with async traits.
pub enum HandleProviderImpl {
    InMemory(InMemoryHandleProvider),
    External(ExternalHandleProvider),
}

impl HandleProviderImpl {
    pub fn mode(&self) -> HandleStoreMode {
        match self {
            HandleProviderImpl::InMemory(_) => HandleStoreMode::InMemory,
            HandleProviderImpl::External(_) => HandleStoreMode::External,
        }
    }

    pub async fn create_handle(&self, df: DataFrame) -> Result<String> {
        match self {
            HandleProviderImpl::InMemory(p) => p.create_handle(df).await,
            HandleProviderImpl::External(p) => p.create_handle(df).await,
        }
    }

    pub async fn get_dataframe(&self, handle: &str) -> Result<Arc<DataFrame>> {
        match self {
            HandleProviderImpl::InMemory(p) => p.get_dataframe(handle).await,
            HandleProviderImpl::External(p) => p.get_dataframe(handle).await,
        }
    }

    pub async fn drop_handle(&self, handle: &str) -> Result<()> {
        match self {
            HandleProviderImpl::InMemory(p) => p.drop_handle(handle).await,
            HandleProviderImpl::External(p) => p.drop_handle(handle).await,
        }
    }

    pub async fn heartbeat(&self, handle: &str) -> Result<()> {
        match self {
            HandleProviderImpl::InMemory(p) => p.heartbeat(handle).await,
            HandleProviderImpl::External(p) => p.heartbeat(handle).await,
        }
    }
}

/// Existing behavior: in-memory handle manager.
pub struct InMemoryHandleProvider {
    manager: Arc<HandleManager>,
}

impl InMemoryHandleProvider {
    pub fn new(manager: Arc<HandleManager>) -> Self {
        Self { manager }
    }

    pub fn manager(&self) -> &Arc<HandleManager> {
        &self.manager
    }
}

impl InMemoryHandleProvider {
    async fn create_handle(&self, df: DataFrame) -> Result<String> {
        Ok(self.manager.create_handle(df))
    }

    async fn get_dataframe(&self, handle: &str) -> Result<Arc<DataFrame>> {
        self.manager.get_dataframe(handle)
    }

    async fn drop_handle(&self, handle: &str) -> Result<()> {
        self.manager.drop_handle(handle)
    }

    async fn heartbeat(&self, handle: &str) -> Result<()> {
        self.manager.heartbeat(handle)
    }
}

/// External-state behavior: handle -> persisted Arrow IPC; server loads on each request.
pub struct ExternalHandleProvider {
    store: FileSystemStateStore,
}

impl ExternalHandleProvider {
    pub fn from_env() -> Result<Self> {
        let dir = default_state_dir();
        tracing::info!("External handle store enabled (filesystem): {}", dir.display());
        Ok(Self {
            store: FileSystemStateStore::new(dir)?,
        })
    }

    fn parse(&self, handle: &str) -> Result<ExternalHandleRef> {
        decode_external_handle(handle)
            .ok_or_else(|| PolaroidError::HandleNotFound(handle.to_string()))
    }
}

impl ExternalHandleProvider {
    async fn create_handle(&self, df: DataFrame) -> Result<String> {
        // File IO + IPC serialization can be blocking; offload.
        let store = self.store.clone();
        let key = store.allocate_key();
        let key_for_store = key.clone();
        let df_clone = df.clone();

        tokio::task::spawn_blocking(move || store.put_ipc(&key_for_store, &df_clone))
            .await
            .map_err(|e| PolaroidError::Internal(format!("Join error: {e}")))??;

        let handle = encode_external_handle("fs", &key);
        tracing::debug!("Created external handle: {handle}");
        Ok(handle)
    }

    async fn get_dataframe(&self, handle: &str) -> Result<Arc<DataFrame>> {
        let href = self.parse(handle)?;
        if href.backend != "fs" {
            return Err(PolaroidError::Internal(format!(
                "Unsupported external backend: {}",
                href.backend
            )));
        }

        let store = self.store.clone();
        let key = href.key;
        let df = tokio::task::spawn_blocking(move || store.get_ipc(&key))
            .await
            .map_err(|e| PolaroidError::Internal(format!("Join error: {e}")))??;

        Ok(Arc::new(df))
    }

    async fn drop_handle(&self, handle: &str) -> Result<()> {
        let href = self.parse(handle)?;
        if href.backend != "fs" {
            return Ok(());
        }

        let store = self.store.clone();
        let key = href.key;
        tokio::task::spawn_blocking(move || store.delete(&key))
            .await
            .map_err(|e| PolaroidError::Internal(format!("Join error: {e}")))??;
        Ok(())
    }

    async fn heartbeat(&self, _handle: &str) -> Result<()> {
        // External store is the source of truth; TTL/lifecycle is handled externally.
        Ok(())
    }
}

pub fn handle_provider_from_env() -> Result<(Arc<HandleProviderImpl>, Option<Arc<HandleManager>>)> {
    // POLAROID_HANDLE_STORE=external enables external state.
    // Default keeps existing in-memory semantics.
    let mode = std::env::var("POLAROID_HANDLE_STORE").unwrap_or_else(|_| "memory".to_string());

    if mode.eq_ignore_ascii_case("external") {
        let provider = ExternalHandleProvider::from_env()?;
        return Ok((Arc::new(HandleProviderImpl::External(provider)), None));
    }

    let manager = Arc::new(HandleManager::default());
    let provider = InMemoryHandleProvider::new(Arc::clone(&manager));
    Ok((Arc::new(HandleProviderImpl::InMemory(provider)), Some(manager)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_df() -> DataFrame {
        df! {"a" => &[1,2,3]}.unwrap()
    }

    #[tokio::test]
    async fn test_external_handle_codec_roundtrip() {
        let h = encode_external_handle("fs", "k");
        let d = decode_external_handle(&h).unwrap();
        assert_eq!(d.backend, "fs");
        assert_eq!(d.key, "k");
    }

    #[tokio::test]
    async fn test_external_provider_put_get() {
        std::env::set_var("POLAROID_STATE_DIR", std::env::temp_dir().join("polaroid_state_test").to_string_lossy().to_string());
        let provider = ExternalHandleProvider::from_env().unwrap();

        let handle = provider.create_handle(test_df()).await.unwrap();
        let df = provider.get_dataframe(&handle).await.unwrap();
        assert_eq!(df.shape(), (3, 1));

        provider.drop_handle(&handle).await.unwrap();
    }
}
