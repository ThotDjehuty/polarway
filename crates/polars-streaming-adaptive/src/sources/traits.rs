//! Core traits for streaming sources

use async_trait::async_trait;
use polars::prelude::*;
use std::sync::Arc;

use super::{SourceConfig, SourceError, SourceResult};

/// Metadata about a streaming source
#[derive(Debug, Clone)]
pub struct SourceMetadata {
    /// Total size in bytes (if known)
    pub size_bytes: Option<u64>,
    /// Number of records (if known)
    pub num_records: Option<usize>,
    /// Schema (if known ahead of time)
    pub schema: Option<SchemaRef>,
    /// Whether the source supports seeking
    pub seekable: bool,
    /// Whether the source supports parallel reads
    pub parallelizable: bool,
}

/// Statistics about streaming progress
#[derive(Debug, Clone, Default)]
pub struct StreamingStats {
    /// Bytes read so far
    pub bytes_read: u64,
    /// Records processed
    pub records_processed: usize,
    /// Number of chunks read
    pub chunks_read: usize,
    /// Current memory usage
    pub memory_bytes: u64,
    /// Average chunk processing time (ms)
    pub avg_chunk_time_ms: f64,
}

/// Core trait for all streaming sources
#[async_trait]
pub trait StreamingSource: Send + Sync {
    /// Get source metadata
    async fn metadata(&self) -> SourceResult<SourceMetadata>;
    
    /// Read the next chunk as a DataFrame
    async fn read_chunk(&mut self) -> SourceResult<Option<DataFrame>>;
    
    /// Get current streaming statistics
    fn stats(&self) -> StreamingStats;
    
    /// Reset the stream to the beginning (if supported)
    async fn reset(&mut self) -> SourceResult<()> {
        Err(SourceError::UnsupportedOperation("reset".to_string()))
    }
    
    /// Seek to a specific position (if supported)
    async fn seek(&mut self, _position: u64) -> SourceResult<()> {
        Err(SourceError::UnsupportedOperation("seek".to_string()))
    }
    
    /// Close the source and clean up resources
    async fn close(&mut self) -> SourceResult<()> {
        Ok(())
    }
    
    /// Check if there's more data to read
    fn has_more(&self) -> bool;
}

/// Factory trait for creating sources
pub trait SourceFactory: Send + Sync {
    fn create(&self, config: SourceConfig) -> SourceResult<Box<dyn StreamingSource>>;
}

/// Builder pattern for configuring adaptive streaming
pub struct AdaptiveStreamBuilder {
    source: Box<dyn StreamingSource>,
    memory_limit: Option<usize>,
    chunk_size: Option<usize>,
    parallel: bool,
    prefetch: bool,
}

impl AdaptiveStreamBuilder {
    pub fn new(source: Box<dyn StreamingSource>) -> Self {
        Self {
            source,
            memory_limit: None,
            chunk_size: None,
            parallel: false,
            prefetch: true,
        }
    }
    
    pub fn with_memory_limit(mut self, bytes: usize) -> Self {
        self.memory_limit = Some(bytes);
        self
    }
    
    pub fn with_chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = Some(size);
        self
    }
    
    pub fn with_parallel(mut self, enable: bool) -> Self {
        self.parallel = enable;
        self
    }
    
    pub fn with_prefetch(mut self, enable: bool) -> Self {
        self.prefetch = enable;
        self
    }
    
    pub async fn collect(mut self) -> SourceResult<DataFrame> {
        let mut frames = Vec::new();
        
        while let Some(chunk) = self.source.read_chunk().await? {
            frames.push(chunk);
            
            // Check memory limit
            if let Some(limit) = self.memory_limit {
                let stats = self.source.stats();
                if stats.memory_bytes as usize > limit {
                    return Err(SourceError::MemoryLimit(limit));
                }
            }
        }
        
        if frames.is_empty() {
            return Err(SourceError::EmptySource);
        }
        
        // Concatenate all frames
        polars::functions::concat_df(&frames)
            .map_err(|e| SourceError::PolarsError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_source_metadata_creation() {
        let metadata = SourceMetadata {
            size_bytes: Some(1024),
            num_records: Some(100),
            schema: None,
            seekable: true,
            parallelizable: false,
        };
        
        assert_eq!(metadata.size_bytes, Some(1024));
        assert!(metadata.seekable);
    }
    
    #[test]
    fn test_streaming_stats_default() {
        let stats = StreamingStats::default();
        assert_eq!(stats.bytes_read, 0);
        assert_eq!(stats.records_processed, 0);
    }
}
