//! Adaptive streaming reader - the core of the library

use crate::chunk_strategy::{AdaptiveChunkStrategy, ChunkStrategy};
use crate::error::{Result, StreamingError};
use crate::memory_manager::MemoryManager;
use crate::mmap_reader::MmapParquetReader;
use crate::predicate_pushdown::PredicatePushdown;
use polars::prelude::*;
use std::path::{Path, PathBuf};

/// Main adaptive streaming reader for Parquet files
pub struct AdaptiveStreamingReader {
    path: PathBuf,
    reader: MmapParquetReader,
    memory_manager: MemoryManager,
    chunk_strategy: Box<dyn ChunkStrategy>,
    predicate: Option<Box<dyn PredicatePushdown>>,
    current_row_group: usize,
}

impl AdaptiveStreamingReader {
    /// Create a new adaptive streaming reader
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let reader = MmapParquetReader::new(&path)?;
        let memory_manager = MemoryManager::new()?;
        let chunk_strategy = Box::new(AdaptiveChunkStrategy::new(memory_manager.clone()));

        tracing::info!(
            "AdaptiveStreamingReader created for {}: {} row groups, ~{} total rows",
            path.display(),
            reader.num_row_groups(),
            reader.total_rows()
        );

        Ok(Self {
            path,
            reader,
            memory_manager,
            chunk_strategy,
            predicate: None,
            current_row_group: 0,
        })
    }

    /// Set a custom chunk strategy
    pub fn with_chunk_strategy(mut self, strategy: Box<dyn ChunkStrategy>) -> Self {
        self.chunk_strategy = strategy;
        self
    }

    /// Add a predicate for pushdown filtering
    pub fn with_predicate(mut self, predicate: Box<dyn PredicatePushdown>) -> Self {
        self.predicate = Some(predicate);
        self
    }

    /// Collect into an iterator of DataFrames with adaptive batching
    ///
    /// This is the main entry point for streaming data
    pub fn collect_batches_adaptive(self) -> impl Iterator<Item = Result<DataFrame>> {
        AdaptiveBatchIterator {
            reader: self,
            exhausted: false,
        }
    }

    /// Collect all batches into a single DataFrame
    ///
    /// Note: This loads all data into memory - use only for small files
    pub fn collect(self) -> Result<DataFrame> {
        let batches: Vec<DataFrame> = self
            .collect_batches_adaptive()
            .collect::<Result<Vec<_>>>()?;

        if batches.is_empty() {
            return Err(StreamingError::NoData);
        }

        // Concatenate all batches vertically
        let mut result = batches[0].clone();
        for batch in &batches[1..] {
            result.vstack_mut(batch)?;
        }
        Ok(result)
    }

    /// Estimate total memory required for full load
    pub fn estimate_memory_required(&self) -> usize {
        let row_size = self.reader.estimate_row_size();
        let total_rows = self.reader.total_rows();
        row_size * total_rows
    }

    /// Check if file can fit in available memory
    pub fn can_fit_in_memory(&self) -> bool {
        let required = self.estimate_memory_required();
        let available = self.memory_manager.available_memory();
        required < available
    }
}

/// Iterator that produces DataFrames with adaptive batching
struct AdaptiveBatchIterator {
    reader: AdaptiveStreamingReader,
    exhausted: bool,
}

impl Iterator for AdaptiveBatchIterator {
    type Item = Result<DataFrame>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.exhausted {
            return None;
        }

        // Check if we've read all row groups
        if self.reader.current_row_group >= self.reader.reader.num_row_groups() {
            self.exhausted = true;
            return None;
        }

        // Read next row group
        let row_group_idx = self.reader.current_row_group;
        self.reader.current_row_group += 1;

        let result = self.read_row_group(row_group_idx);

        // Check for errors
        match &result {
            Ok(df) => {
                // Track memory usage
                let size = df.estimated_size();
                self.reader.memory_manager.track_usage(size);

                tracing::debug!(
                    "Read row group {}: {} rows, {}MB",
                    row_group_idx,
                    df.height(),
                    size / 1024 / 1024
                );
            }
            Err(e) => {
                tracing::error!("Error reading row group {}: {}", row_group_idx, e);
                self.exhausted = true;
            }
        }

        Some(result)
    }
}

impl AdaptiveBatchIterator {
    fn read_row_group(&mut self, row_group_idx: usize) -> Result<DataFrame> {
        // Read row group using memory-mapped reader
        let mut df = self.reader.reader.read_row_group(row_group_idx)?;

        // Apply predicate pushdown if specified
        if let Some(ref predicate) = self.reader.predicate {
            let mask = predicate.apply(&df)?;
            df = df.filter(&mask)?;

            tracing::trace!(
                "Predicate filtered row group {}: {} → {} rows",
                row_group_idx,
                self.reader.reader.row_group_num_rows(row_group_idx)?,
                df.height()
            );
        }

        Ok(df)
    }
}

impl Drop for AdaptiveBatchIterator {
    fn drop(&mut self) {
        tracing::debug!(
            "AdaptiveBatchIterator dropped for {}",
            self.reader.path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polars::prelude::*;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn create_test_parquet(rows: usize) -> PathBuf {
        let df = DataFrame::new(vec![
            Series::new("id".into(), (0..rows as i32).collect::<Vec<_>>()).into(),
            Series::new(
                "value".into(),
                (0..rows).map(|i| i as f64 * 1.5).collect::<Vec<_>>(),
            ).into(),
        ])
        .unwrap();

        // Create a temp file with UUID-based naming
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join(format!("test_adaptive_{}_{}.parquet", 
            std::process::id(), 
            Uuid::new_v4()));

        ParquetWriter::new(std::fs::File::create(&path).unwrap())
            .finish(&mut df.clone())
            .unwrap();

        path
    }

    #[test]
    fn test_adaptive_reader_creation() {
        let path = create_test_parquet(1000);
        let reader = AdaptiveStreamingReader::new(&path);
        assert!(reader.is_ok());
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_collect_batches() {
        let path = create_test_parquet(1000);
        let reader = AdaptiveStreamingReader::new(&path).unwrap();

        let batches: Vec<DataFrame> = reader
            .collect_batches_adaptive()
            .collect::<Result<Vec<_>>>()
            .unwrap();

        assert!(!batches.is_empty());

        let total_rows: usize = batches.iter().map(|df| df.height()).sum();
        assert_eq!(total_rows, 1000);

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_collect_single_dataframe() {
        let path = create_test_parquet(500);
        let reader = AdaptiveStreamingReader::new(&path).unwrap();

        let df = reader.collect().unwrap();
        assert_eq!(df.height(), 500);

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_memory_estimation() {
        let path = create_test_parquet(1000);
        let reader = AdaptiveStreamingReader::new(&path).unwrap();

        let estimated = reader.estimate_memory_required();
        assert!(estimated > 0);

        std::fs::remove_file(path).ok();
    }
}
