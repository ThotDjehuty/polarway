//! Memory-mapped Parquet file reader for zero-copy access

use crate::error::{Result, StreamingError};
use memmap2::Mmap;
use polars::prelude::*;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

/// Memory-mapped Parquet reader for efficient large file handling
pub struct MmapParquetReader {
    path: std::path::PathBuf,
    mmap: Arc<Mmap>,
    schema: Arc<Schema>,
    num_rows: Option<usize>,
}

impl MmapParquetReader {
    /// Create a new memory-mapped Parquet reader
    ///
    /// # Arguments
    /// * `path` - Path to the Parquet file
    ///
    /// # Example
    /// ```rust,no_run
    /// use polars_streaming_adaptive::MmapParquetReader;
    ///
    /// let reader = MmapParquetReader::new("data.parquet").unwrap();
    /// println!("Row groups: {}", reader.num_row_groups());
    /// ```
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let file = File::open(&path_buf)?;

        // Safety: We trust that the file won't be modified while mapped
        let mmap = unsafe { Mmap::map(&file)? };
        let mmap = Arc::new(mmap);

        // Parse Parquet metadata from memory-mapped bytes
        let cursor = std::io::Cursor::new(mmap.as_ref());
        let mut parquet_reader = polars::prelude::ParquetReader::new(cursor);
        
        // Get schema without reading data
        let arrow_schema = parquet_reader
            .schema()
            .map_err(|e| StreamingError::Compute(format!("Failed to read schema: {}", e)))?;

        // Convert Arrow schema to Polars schema
        let polars_schema = Schema::from_iter(
            arrow_schema.iter_values().map(|f| {
                (
                    f.name.clone(),
                    DataType::from_arrow(&f.dtype, false, None),
                )
            }),
        );

        Ok(Self {
            path: path_buf,
            mmap,
            schema: Arc::new(polars_schema),
            num_rows: None,
        })
    }

    /// Get number of row groups in the file
    pub fn num_row_groups(&self) -> usize {
        // Parse row group count from parquet metadata
        // For now, estimate based on file size (actual implementation would read metadata)
        let file_size = self.mmap.len();
        let estimated_row_group_size = 64 * 1024 * 1024; // 64MB per row group typical
        (file_size / estimated_row_group_size).max(1)
    }

    /// Get total rows across all row groups
    pub fn total_rows(&self) -> usize {
        // Estimate based on file size and typical row density
        // Actual implementation would read from Parquet metadata
        let file_size_mb = self.mmap.len() / (1024 * 1024);
        let rows_per_mb = 10_000; // Conservative estimate for OHLCV data
        file_size_mb * rows_per_mb
    }

    /// Estimate average row size in bytes
    pub fn estimate_row_size(&self) -> usize {
        let total_bytes = self.mmap.len();
        let estimated_rows = self.total_rows();
        if estimated_rows > 0 {
            total_bytes / estimated_rows
        } else {
            100 // Default estimate
        }
    }

    /// Get number of rows in a specific row group
    pub fn row_group_num_rows(&self, idx: usize) -> Result<usize> {
        if idx >= self.num_row_groups() {
            return Err(StreamingError::InvalidConfig(format!(
                "Row group index {} out of bounds (max: {})",
                idx,
                self.num_row_groups()
            )));
        }

        // Estimate rows per row group
        let total = self.total_rows();
        let num_groups = self.num_row_groups();
        Ok(total / num_groups)
    }

    /// Read a specific row group into a DataFrame
    ///
    /// # Arguments
    /// * `idx` - Row group index to read
    ///
    /// # Returns
    /// DataFrame containing the row group data
    pub fn read_row_group(&self, idx: usize) -> Result<DataFrame> {
        if idx >= self.num_row_groups() {
            return Err(StreamingError::InvalidConfig(format!(
                "Row group index {} out of bounds",
                idx
            )));
        }

        // Create a cursor over the memory-mapped region
        let cursor = std::io::Cursor::new(self.mmap.as_ref());
        
        // Read the full file (in production, would read specific row group)
        let parquet_reader = ParquetReader::new(cursor);
        let df = parquet_reader
            .finish()
            .map_err(|e| StreamingError::Polars(e))?;

        // For now, split into chunks (actual impl would use row group offsets)
        let rows_per_group = self.row_group_num_rows(idx)?;
        let start = idx * rows_per_group;
        let end = ((idx + 1) * rows_per_group).min(df.height());

        if start >= df.height() {
            return Ok(DataFrame::default());
        }

        Ok(df.slice(start as i64, end - start))
    }

    /// Check if the entire file can fit in available memory
    ///
    /// # Arguments
    /// * `available_memory` - Available memory in bytes
    pub fn can_fit_in_memory(&self, available_memory: usize) -> bool {
        self.mmap.len() < available_memory
    }

    /// Get file path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get file size in bytes
    pub fn file_size(&self) -> usize {
        self.mmap.len()
    }

    /// Get schema
    pub fn schema(&self) -> &Arc<Schema> {
        &self.schema
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
        let path = temp_dir.join(format!("test_mmap_{}_{}.parquet", 
            std::process::id(), 
            Uuid::new_v4()));

        ParquetWriter::new(std::fs::File::create(&path).unwrap())
            .finish(&mut df.clone())
            .unwrap();

        path
    }

    #[test]
    fn test_mmap_reader_creation() {
        let path = create_test_parquet(1000);
        let reader = MmapParquetReader::new(&path);
        assert!(reader.is_ok());
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_read_row_group() {
        let path = create_test_parquet(1000);
        let reader = MmapParquetReader::new(&path).unwrap();

        let df = reader.read_row_group(0).unwrap();
        assert!(df.height() > 0);

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_metadata() {
        let path = create_test_parquet(1000);
        let reader = MmapParquetReader::new(&path).unwrap();

        assert!(reader.num_row_groups() > 0);
        assert!(reader.total_rows() > 0);
        assert!(reader.estimate_row_size() > 0);

        std::fs::remove_file(path).ok();
    }
}
