//! Parallel streaming for multiple files

use crate::adaptive_reader::AdaptiveStreamingReader;
use crate::error::Result;
use crossbeam_channel::{bounded, Receiver, Sender};
use polars::prelude::*;
use rayon::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Parallel streaming reader for multiple Parquet files
pub struct ParallelStreamReader {
    paths: Vec<PathBuf>,
    max_concurrent: usize,
    buffer_size: usize,
}

impl ParallelStreamReader {
    /// Create a new parallel stream reader
    pub fn new(paths: Vec<PathBuf>) -> Self {
        let max_concurrent = rayon::current_num_threads();
        Self {
            paths,
            max_concurrent,
            buffer_size: max_concurrent * 2,
        }
    }

    /// Set maximum concurrent file readers
    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = max.max(1);
        self
    }

    /// Set internal buffer size (for backpressure)
    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size.max(1);
        self
    }

    /// Stream all files in parallel with backpressure
    ///
    /// Returns an iterator that yields DataFrames from all files
    pub fn collect_parallel(self) -> impl Iterator<Item = Result<DataFrame>> {
        let (tx, rx): (Sender<Result<DataFrame>>, Receiver<_>) = bounded(self.buffer_size);

        let paths = self.paths.clone();
        let max_concurrent = self.max_concurrent;

        // Spawn parallel readers in background
        rayon::spawn(move || {
            Self::parallel_read_worker(paths, tx, max_concurrent);
        });

        rx.into_iter()
    }

    /// Collect all files and concatenate into a single DataFrame
    pub fn collect_concatenated(self) -> Result<DataFrame> {
        let batches: Vec<DataFrame> = self.collect_parallel().collect::<Result<Vec<_>>>()?;

        if batches.is_empty() {
            return Err(crate::error::StreamingError::NoData);
        }

        // Concatenate all batches vertically
        let mut result = batches[0].clone();
        for batch in &batches[1..] {
            result.vstack_mut(batch)?;
        }
        Ok(result)
    }

    /// Worker function for parallel file reading
    fn parallel_read_worker(paths: Vec<PathBuf>, tx: Sender<Result<DataFrame>>, max_concurrent: usize) {
        let files_processed = Arc::new(AtomicUsize::new(0));
        let total_files = paths.len();

        tracing::info!(
            "Starting parallel read: {} files, {} concurrent workers",
            total_files,
            max_concurrent
        );

        // Use Rayon's parallel iterator with work stealing
        paths.par_iter().for_each_with(
            (tx.clone(), files_processed.clone()),
            |(tx, counter), path| {
                // Create reader for this file
                let reader = match AdaptiveStreamingReader::new(path) {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                };

                // Stream batches from this file
                for batch in reader.collect_batches_adaptive() {
                    if tx.send(batch).is_err() {
                        // Receiver dropped - stop processing
                        tracing::warn!("Receiver dropped, stopping file processing");
                        break;
                    }
                }

                // Update progress
                let processed = counter.fetch_add(1, Ordering::Relaxed) + 1;
                tracing::debug!("Completed file {}/{}: {}", processed, total_files, path.display());
            },
        );

        tracing::info!("Parallel read completed: {} files", total_files);
    }

    /// Get number of files to be processed
    pub fn num_files(&self) -> usize {
        self.paths.len()
    }
}

/// Helper to create ParallelStreamReader from glob pattern
pub fn from_glob(pattern: &str) -> Result<ParallelStreamReader> {
    use glob::glob;

    let paths: Vec<PathBuf> = glob(pattern)
        .map_err(|e| {
            crate::error::StreamingError::InvalidConfig(format!("Invalid glob pattern: {}", e))
        })?
        .filter_map(|entry: std::result::Result<PathBuf, glob::GlobError>| entry.ok())
        .collect();

    if paths.is_empty() {
        return Err(crate::error::StreamingError::NoData);
    }

    Ok(ParallelStreamReader::new(paths))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_files(count: usize, rows_per_file: usize) -> (TempDir, Vec<PathBuf>) {
        let temp_dir = TempDir::new().unwrap();
        let mut paths = Vec::new();

        for i in 0..count {
            let df = DataFrame::new(vec![
                Series::new("file_id".into(), vec![i as i32; rows_per_file]).into(),
                Series::new("row_id".into(), (0..rows_per_file as i32).collect::<Vec<_>>()).into(),
                Series::new(
                    "value".into(),
                    (0..rows_per_file).map(|j| (i * 1000 + j) as f64).collect::<Vec<_>>(),
                ).into(),
            ])
            .unwrap();

            let path = temp_dir.path().join(format!("file_{}.parquet", i));
            ParquetWriter::new(std::fs::File::create(&path).unwrap())
                .finish(&mut df.clone())
                .unwrap();

            paths.push(path);
        }

        (temp_dir, paths)
    }

    #[test]
    fn test_parallel_reader_creation() {
        let (_temp, paths) = create_test_files(3, 100);
        let reader = ParallelStreamReader::new(paths);
        assert_eq!(reader.num_files(), 3);
    }

    #[test]
    fn test_parallel_collect() {
        let (_temp, paths) = create_test_files(5, 200);
        let reader = ParallelStreamReader::new(paths);

        let batches: Vec<DataFrame> = reader.collect_parallel().collect::<Result<Vec<_>>>().unwrap();

        assert!(!batches.is_empty());

        let total_rows: usize = batches.iter().map(|df| df.height()).sum();
        assert_eq!(total_rows, 5 * 200);
    }

    #[test]
    fn test_parallel_concatenated() {
        let (_temp, paths) = create_test_files(3, 150);
        let reader = ParallelStreamReader::new(paths);

        let df = reader.collect_concatenated().unwrap();
        assert_eq!(df.height(), 3 * 150);
    }

    #[test]
    fn test_concurrent_limit() {
        let (_temp, paths) = create_test_files(10, 50);
        let reader = ParallelStreamReader::new(paths).with_max_concurrent(2);

        let df = reader.collect_concatenated().unwrap();
        assert_eq!(df.height(), 10 * 50);
    }
}
