//! CSV streaming source with adaptive chunking

use async_trait::async_trait;
use polars::prelude::*;
use std::fs::File;
use std::io::{BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;

use super::{
    CsvConfig, SourceConfig, SourceError, SourceFactory, SourceMetadata, SourceResult,
    StreamingSource, StreamingStats,
};

pub struct CsvSource {
    path: PathBuf,
    config: CsvConfig,
    reader: Option<BufReader<File>>,
    stats: StreamingStats,
    schema: Option<SchemaRef>,
    chunk_size: usize,
    current_position: u64,
    total_size: u64,
}

impl CsvSource {
    pub fn new(path: PathBuf, config: CsvConfig, chunk_size: usize) -> SourceResult<Self> {
        let file = File::open(&path)?;
        let total_size = file.metadata()?.len();
        let reader = Some(BufReader::new(file));
        
        Ok(Self {
            path,
            config,
            reader,
            stats: StreamingStats::default(),
            schema: None,
            chunk_size,
            current_position: 0,
            total_size,
        })
    }
    
    fn infer_schema(&mut self) -> SourceResult<SchemaRef> {
        if let Some(schema) = &self.schema {
            return Ok(schema.clone());
        }
        
        // Read first chunk to infer schema
        let df = CsvReadOptions::default()
            .with_has_header(self.config.has_header)
            .with_n_rows(Some(1000))
            .try_into_reader_with_file_path(Some(self.path.clone()))?
            .finish()?;
        
        let schema = df.schema();
        self.schema = Some(schema.clone());
        
        // Reset reader
        if let Some(reader) = &mut self.reader {
            reader.seek(SeekFrom::Start(0))?;
        }
        
        Ok(schema)
    }
}

#[async_trait]
impl StreamingSource for CsvSource {
    async fn metadata(&self) -> SourceResult<SourceMetadata> {
        Ok(SourceMetadata {
            size_bytes: Some(self.total_size),
            num_records: None,
            schema: self.schema.clone(),
            seekable: true,
            parallelizable: true,
        })
    }
    
    async fn read_chunk(&mut self) -> SourceResult<Option<DataFrame>> {
        if self.schema.is_none() {
            self.infer_schema()?;
        }
        
        if self.current_position >= self.total_size {
            return Ok(None);
        }
        
        let start_time = std::time::Instant::now();
        
        // Read chunk using polars CSV reader
        let df = CsvReadOptions::default()
            .with_has_header(self.config.has_header && self.stats.chunks_read == 0)
            .with_skip_rows(if self.stats.chunks_read == 0 {
                self.config.skip_rows
            } else {
                0
            })
            .with_n_rows(Some(self.chunk_size))
            .try_into_reader_with_file_path(Some(self.path.clone()))?
            .finish()?;
        
        let chunk_bytes = df.estimated_size();
        let chunk_records = df.height();
        
        // Update stats
        self.stats.bytes_read += chunk_bytes as u64;
        self.stats.records_processed += chunk_records;
        self.stats.chunks_read += 1;
        self.stats.memory_bytes = chunk_bytes as u64;
        
        let elapsed = start_time.elapsed().as_millis() as f64;
        self.stats.avg_chunk_time_ms = 
            (self.stats.avg_chunk_time_ms * (self.stats.chunks_read - 1) as f64 + elapsed) 
            / self.stats.chunks_read as f64;
        
        self.current_position += chunk_bytes as u64;
        
        Ok(Some(df))
    }
    
    fn stats(&self) -> StreamingStats {
        self.stats.clone()
    }
    
    async fn reset(&mut self) -> SourceResult<()> {
        if let Some(reader) = &mut self.reader {
            reader.seek(SeekFrom::Start(0))?;
            self.current_position = 0;
            self.stats = StreamingStats::default();
            Ok(())
        } else {
            Err(SourceError::Other("Reader not initialized".to_string()))
        }
    }
    
    async fn seek(&mut self, position: u64) -> SourceResult<()> {
        if let Some(reader) = &mut self.reader {
            reader.seek(SeekFrom::Start(position))?;
            self.current_position = position;
            Ok(())
        } else {
            Err(SourceError::Other("Reader not initialized".to_string()))
        }
    }
    
    fn has_more(&self) -> bool {
        self.current_position < self.total_size
    }
}

pub struct CsvSourceFactory;

impl SourceFactory for CsvSourceFactory {
    fn create(&self, config: SourceConfig) -> SourceResult<Box<dyn StreamingSource>> {
        let path = PathBuf::from(&config.location);
        let csv_config = CsvConfig::default(); // TODO: Parse from config.options
        let chunk_size = config.chunk_size.unwrap_or(10_000);
        
        Ok(Box::new(CsvSource::new(path, csv_config, chunk_size)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    
    fn create_test_csv() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "name,age,city").unwrap();
        writeln!(file, "Alice,30,NYC").unwrap();
        writeln!(file, "Bob,25,LA").unwrap();
        writeln!(file, "Charlie,35,Chicago").unwrap();
        file
    }
    
    #[tokio::test]
    async fn test_csv_source_creation() {
        let file = create_test_csv();
        let path = file.path().to_path_buf();
        let config = CsvConfig::default();
        
        let source = CsvSource::new(path, config, 1000);
        assert!(source.is_ok());
    }
    
    #[tokio::test]
    async fn test_csv_source_read() {
        let file = create_test_csv();
        let path = file.path().to_path_buf();
        let config = CsvConfig::default();
        
        let mut source = CsvSource::new(path, config, 1000).unwrap();
        let chunk = source.read_chunk().await.unwrap();
        
        assert!(chunk.is_some());
        let df = chunk.unwrap();
        assert_eq!(df.height(), 3);
    }
}
