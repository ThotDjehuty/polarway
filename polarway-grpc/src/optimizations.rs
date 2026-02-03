/// Performance optimizations for Polaroid gRPC service
/// 
/// This module implements zero-copy, memory pooling, and batching
/// optimizations to eliminate overhead and surpass Polars performance.

use polars::prelude::*;
use arrow::record_batch::RecordBatch;
use arrow::ipc::writer::StreamWriter;
use std::sync::Arc;
use parking_lot::RwLock;
use once_cell::sync::Lazy;

/// Memory pool for reusing buffers
pub struct MemoryPool {
    small_buffers: Arc<RwLock<Vec<Vec<u8>>>>,  // < 1MB
    medium_buffers: Arc<RwLock<Vec<Vec<u8>>>>, // 1-10MB
    large_buffers: Arc<RwLock<Vec<Vec<u8>>>>,  // > 10MB
}

impl MemoryPool {
    pub fn new() -> Self {
        Self {
            small_buffers: Arc::new(RwLock::new(Vec::with_capacity(100))),
            medium_buffers: Arc::new(RwLock::new(Vec::with_capacity(50))),
            large_buffers: Arc::new(RwLock::new(Vec::with_capacity(20))),
        }
    }
    
    /// Get a buffer from the pool or allocate new one
    pub fn get_buffer(&self, size_hint: usize) -> Vec<u8> {
        let mut buffers = if size_hint < 1_000_000 {
            self.small_buffers.write()
        } else if size_hint < 10_000_000 {
            self.medium_buffers.write()
        } else {
            self.large_buffers.write()
        };
        
        buffers.pop().unwrap_or_else(|| Vec::with_capacity(size_hint))
    }
    
    /// Return a buffer to the pool for reuse
    pub fn return_buffer(&self, mut buffer: Vec<u8>) {
        buffer.clear();
        let cap = buffer.capacity();
        
        let mut buffers = if cap < 1_000_000 {
            self.small_buffers.write()
        } else if cap < 10_000_000 {
            self.medium_buffers.write()
        } else {
            self.large_buffers.write()
        };
        
        // Limit pool size to prevent memory bloat
        if buffers.len() < 100 {
            buffers.push(buffer);
        }
    }
}

/// Global memory pool instance
pub static MEMORY_POOL: Lazy<MemoryPool> = Lazy::new(MemoryPool::new);

/// Zero-copy Arrow IPC serialization
pub struct FastArrowSerializer;

impl FastArrowSerializer {
    /// Convert DataFrame to Arrow IPC with zero-copy when possible
    pub fn to_ipc_zero_copy(df: &DataFrame) -> crate::error::Result<Vec<u8>> {
        // Convert to Arrow RecordBatch (zero-copy)
        let batches = df.iter_chunks(false, false)
            .map(|chunk| chunk_to_record_batch(chunk, df.schema()))
            .collect::<Vec<_>>();
        
        // Use memory pool for buffer
        let estimated_size = estimate_ipc_size(df);
        let mut buffer = MEMORY_POOL.get_buffer(estimated_size);
        
        // Write IPC stream (Arrow's wire format)
        {
            let mut writer = StreamWriter::try_new(&mut buffer, &df.schema().to_arrow(true))
                .map_err(|e| crate::error::PolaroidError::Arrow(e))?;
            
            for batch in batches {
                writer.write(&batch)
                    .map_err(|e| crate::error::PolaroidError::Arrow(e))?;
            }
            
            writer.finish()
                .map_err(|e| crate::error::PolaroidError::Arrow(e))?;
        }
        
        Ok(buffer)
    }
    
    /// Streaming serialization for large DataFrames
    pub async fn to_ipc_streaming(
        df: &DataFrame,
        batch_size: usize
    ) -> impl futures::Stream<Item = crate::error::Result<Vec<u8>>> {
        let schema = df.schema().to_arrow(true);
        let chunks: Vec<_> = df.iter_chunks(false, false).collect();
        
        futures::stream::iter(chunks).map(move |chunk| {
            let batch = chunk_to_record_batch(chunk, &df.schema());
            let mut buffer = MEMORY_POOL.get_buffer(batch_size);
            
            let mut writer = StreamWriter::try_new(&mut buffer, &schema)
                .map_err(|e| crate::error::PolaroidError::Arrow(e))?;
            
            writer.write(&batch)
                .map_err(|e| crate::error::PolaroidError::Arrow(e))?;
            
            writer.finish()
                .map_err(|e| crate::error::PolaroidError::Arrow(e))?;
            
            Ok(buffer)
        })
    }
}

/// Convert Polars chunk to Arrow RecordBatch
fn chunk_to_record_batch(chunk: DataFrame, schema: &Schema) -> RecordBatch {
    // This is zero-copy conversion
    let arrow_schema = schema.to_arrow(true);
    let columns: Vec<_> = chunk.get_columns()
        .iter()
        .map(|s| s.to_arrow(0, false))
        .collect();
    
    RecordBatch::try_new(Arc::new(arrow_schema), columns)
        .expect("Failed to create RecordBatch")
}

/// Estimate IPC buffer size needed
fn estimate_ipc_size(df: &DataFrame) -> usize {
    let row_bytes = df.get_columns()
        .iter()
        .map(|s| s.dtype().get_byte_width())
        .sum::<usize>();
    
    let data_size = row_bytes * df.height();
    let overhead = 1024 * 10; // 10KB overhead for metadata
    
    data_size + overhead
}

/// Parallel filter optimization
pub struct ParallelFilter;

impl ParallelFilter {
    /// Parse expression string to Polars Expr
    pub fn parse_expr(expr_str: &str) -> crate::error::Result<Expr> {
        // Simple expression parser for common patterns
        // Format: "column op value" e.g., "price > 100", "symbol == 'AAPL'"
        
        let parts: Vec<&str> = expr_str.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(crate::error::PolaroidError::InvalidExpression(
                format!("Invalid expression: {}", expr_str)
            ));
        }
        
        let col_name = parts[0];
        let op = parts[1];
        let value = parts[2..].join(" ");
        
        let col_expr = col(col_name);
        
        // Try to parse as different types
        let expr = match op {
            ">" => {
                if let Ok(v) = value.parse::<f64>() {
                    col_expr.gt(lit(v))
                } else {
                    col_expr.gt(lit(value.trim_matches('\'')))
                }
            },
            ">=" => {
                if let Ok(v) = value.parse::<f64>() {
                    col_expr.gt_eq(lit(v))
                } else {
                    col_expr.gt_eq(lit(value.trim_matches('\'')))
                }
            },
            "<" => {
                if let Ok(v) = value.parse::<f64>() {
                    col_expr.lt(lit(v))
                } else {
                    col_expr.lt(lit(value.trim_matches('\'')))
                }
            },
            "<=" => {
                if let Ok(v) = value.parse::<f64>() {
                    col_expr.lt_eq(lit(v))
                } else {
                    col_expr.lt_eq(lit(value.trim_matches('\'')))
                }
            },
            "==" | "=" => {
                if let Ok(v) = value.parse::<f64>() {
                    col_expr.eq(lit(v))
                } else if let Ok(v) = value.parse::<bool>() {
                    col_expr.eq(lit(v))
                } else {
                    col_expr.eq(lit(value.trim_matches('\'')))
                }
            },
            "!=" => {
                if let Ok(v) = value.parse::<f64>() {
                    col_expr.neq(lit(v))
                } else {
                    col_expr.neq(lit(value.trim_matches('\'')))
                }
            },
            _ => {
                return Err(crate::error::PolaroidError::InvalidExpression(
                    format!("Unsupported operator: {}", op)
                ));
            }
        };
        
        Ok(expr)
    }
    
    /// Apply filter with expression string
    pub fn apply_with_expr(df: &DataFrame, expr_str: &str) -> crate::error::Result<DataFrame> {
        let expr = Self::parse_expr(expr_str)?;
        Self::apply_simd_optimized(df, expr)
            .map_err(|e| crate::error::PolaroidError::Polars(e))
    }
    
    /// Apply filter in parallel chunks
    pub fn apply(df: &DataFrame, predicate: Expr) -> PolarsResult<DataFrame> {
        // Convert to lazy for optimization
        let lf = df.clone().lazy();
        
        // Apply filter with automatic parallelization
        lf.filter(predicate)
            .collect()
    }
    
    /// Apply filter with SIMD optimization hints
    pub fn apply_simd_optimized(df: &DataFrame, predicate: Expr) -> PolarsResult<DataFrame> {
        // Use Polars' optimized filter which uses SIMD when possible
        let lf = df.clone().lazy();
        
        // These optimizations are applied automatically by Polars when:
        // 1. Predicate is simple comparison (>, <, ==, etc.)
        // 2. Column is numeric type
        // 3. CPU supports SIMD (AVX2, AVX-512)
        lf.filter(predicate)
            .with_streaming(true) // Enable streaming for large datasets
            .collect()
    }
}

/// Optimized group_by with pre-aggregation
pub struct FastGroupBy;

impl FastGroupBy {
    /// Group and aggregate with string-based aggregations (for proto)
    pub fn group_agg(
        df: &DataFrame,
        by: &[String],
        aggs: &[String] // Format: "column:agg_fn" e.g., "price:mean"
    ) -> crate::error::Result<DataFrame> {
        let lf = df.clone().lazy();
        
        // Parse aggregation strings
        let agg_specs: Vec<(&str, &str)> = aggs.iter()
            .filter_map(|s| {
                let parts: Vec<&str> = s.split(':').collect();
                if parts.len() == 2 {
                    Some((parts[0], parts[1]))
                } else {
                    None
                }
            })
            .collect();
        
        // Build aggregation expressions
        let agg_exprs: Vec<Expr> = agg_specs.iter()
            .map(|(col_name, agg_fn)| {
                let col_expr = col(col_name);
                match *agg_fn {
                    "mean" => col_expr.mean(),
                    "sum" => col_expr.sum(),
                    "min" => col_expr.min(),
                    "max" => col_expr.max(),
                    "count" => col_expr.count(),
                    "std" => col_expr.std(1),
                    "var" => col_expr.var(1),
                    "first" => col_expr.first(),
                    "last" => col_expr.last(),
                    "median" => col_expr.median(),
                    _ => col_expr.sum(), // fallback
                }
                .alias(&format!("{}_{}", col_name, agg_fn))
            })
            .collect();
        
        // Use optimized group_by with streaming
        let result = lf.group_by(by.iter().map(|s| col(s.as_str())).collect::<Vec<_>>())
            .agg(agg_exprs)
            .with_streaming(true)
            .collect()
            .map_err(|e| crate::error::PolaroidError::Polars(e))?;
        
        Ok(result)
    }
    
    /// Group and aggregate with hash table optimization (internal use)
    pub fn group_agg_internal(
        df: &DataFrame,
        by: Vec<&str>,
        aggs: Vec<(&str, &str)> // (column, agg_fn)
    ) -> PolarsResult<DataFrame> {
        let lf = df.clone().lazy();
        
        // Build aggregation expressions
        let agg_exprs: Vec<Expr> = aggs.iter()
            .map(|(col_name, agg_fn)| {
                let col_expr = col(col_name);
                match *agg_fn {
                    "mean" => col_expr.mean(),
                    "sum" => col_expr.sum(),
                    "min" => col_expr.min(),
                    "max" => col_expr.max(),
                    "count" => col_expr.count(),
                    "std" => col_expr.std(1),
                    "var" => col_expr.var(1),
                    "first" => col_expr.first(),
                    "last" => col_expr.last(),
                    "median" => col_expr.median(),
                    _ => col_expr.sum(), // fallback
                }
                .alias(&format!("{}_{}", col_name, agg_fn))
            })
            .collect();
        
        // Use optimized group_by with streaming
        lf.group_by(by.iter().map(|s| col(s)).collect::<Vec<_>>())
            .agg(agg_exprs)
            .with_streaming(true)
            .collect()
    }
}

/// Connection pooling for file I/O
pub struct IoPool {
    #[cfg(target_os = "linux")]
    io_uring: Option<tokio_uring::Runtime>,
}

impl IoPool {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            io_uring: tokio_uring::Runtime::new().ok(),
        }
    }
    
    /// Read file with optimal I/O strategy
    pub async fn read_file_optimized(&self, path: &str) -> std::io::Result<Vec<u8>> {
        #[cfg(target_os = "linux")]
        if let Some(uring) = &self.io_uring {
            // Use io_uring for zero-copy I/O on Linux
            return uring.spawn(async move {
                tokio_uring::fs::read(path).await
            }).await;
        }
        
        // Fallback to standard async I/O
        tokio::fs::read(path).await
    }
}

/// Batch operation optimizer
pub struct BatchOptimizer;

impl BatchOptimizer {
    /// Batch multiple operations to reduce overhead
    pub async fn batch_collect(
        handles: Vec<String>,
        manager: &crate::handles::HandleManager
    ) -> Vec<crate::error::Result<Vec<u8>>> {
        // Process in parallel using Tokio
        let tasks: Vec<_> = handles.into_iter()
            .map(|handle| {
                let manager_clone = manager.clone();
                tokio::spawn(async move {
                    let df = manager_clone.get_dataframe(&handle)?;
                    FastArrowSerializer::to_ipc_zero_copy(&df)
                })
            })
            .collect();
        
        // Wait for all tasks
        let results = futures::future::join_all(tasks).await;
        
        results.into_iter()
            .map(|r| r.unwrap_or_else(|e| Err(crate::error::PolaroidError::Tokio(e))))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memory_pool() {
        let pool = MemoryPool::new();
        let buf1 = pool.get_buffer(1024);
        assert!(buf1.capacity() >= 1024);
        
        pool.return_buffer(buf1);
        let buf2 = pool.get_buffer(1024);
        assert!(buf2.capacity() >= 1024);
    }
    
    #[tokio::test]
    async fn test_fast_serialization() {
        let df = df!{
            "a" => [1, 2, 3],
            "b" => [4.0, 5.0, 6.0]
        }.unwrap();
        
        let ipc = FastArrowSerializer::to_ipc_zero_copy(&df).unwrap();
        assert!(!ipc.is_empty());
    }
}
