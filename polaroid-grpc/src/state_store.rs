use std::path::{Path, PathBuf};

use polars::prelude::*;
use tracing::debug;
use uuid::Uuid;

use crate::error::Result;

/// Minimal external state store for DataFrames.
///
/// This is the core primitive for “stateless workers”: instead of keeping DataFrames
/// in-memory keyed by a handle, we serialize DataFrames to a shared external store
/// (filesystem now; object-store later) and make handles refer to stored objects.
#[derive(Clone)]
pub struct FileSystemStateStore {
    base_dir: PathBuf,
}

impl FileSystemStateStore {
    pub fn new(base_dir: impl Into<PathBuf>) -> Result<Self> {
        let base_dir = base_dir.into();
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn allocate_key(&self) -> String {
        Uuid::new_v4().to_string()
    }

    fn key_path(&self, key: &str) -> PathBuf {
        self.base_dir.join(format!("{key}.ipc"))
    }

    /// Persist a DataFrame as Arrow IPC to the store.
    pub fn put_ipc(&self, key: &str, df: &DataFrame) -> Result<()> {
        let path = self.key_path(key);
        debug!("Persisting dataframe to IPC: {}", path.display());

        let mut file = std::fs::File::create(path)?;
        polars::io::ipc::IpcWriter::new(&mut file).finish(&mut df.clone())?;
        Ok(())
    }

    /// Load a DataFrame from Arrow IPC.
    pub fn get_ipc(&self, key: &str) -> Result<DataFrame> {
        let path = self.key_path(key);
        debug!("Loading dataframe from IPC: {}", path.display());

        let file = std::fs::File::open(path)?;
        let df = polars::io::ipc::IpcReader::new(file).finish()?;
        Ok(df)
    }

    /// Delete a persisted DataFrame.
    pub fn delete(&self, key: &str) -> Result<()> {
        let path = self.key_path(key);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

/// Default base directory for external-state mode.
pub fn default_state_dir() -> PathBuf {
    std::env::var("POLAROID_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("polaroid_state"))
}
