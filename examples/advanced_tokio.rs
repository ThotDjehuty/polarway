// Advanced Tokio patterns for Polaroid server
// Demonstrates zero-cost async, work-stealing, and monadic error handling

use polaroid_grpc::*;
use tokio::task;
use tokio::time::{sleep, Duration};
use std::sync::Arc;
use futures::future::join_all;

/// Monadic Result type - Rust's killer feature
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Functor trait - enables .map() on Results
trait Functor<T> {
    type Mapped<U>;
    fn fmap<U, F>(self, f: F) -> Self::Mapped<U>
    where
        F: FnOnce(T) -> U;
}

impl<T, E> Functor<T> for std::result::Result<T, E> {
    type Mapped<U> = std::result::Result<U, E>;
    
    fn fmap<U, F>(self, f: F) -> Self::Mapped<U>
    where
        F: FnOnce(T) -> U,
    {
        self.map(f)
    }
}

/// Example 1: Concurrent Parquet Reading with Work-Stealing
/// 
/// Tokio spawns tasks that can be stolen by idle threads.
/// This achieves near-linear scalability up to CPU core count.
async fn concurrent_parquet_reads(paths: Vec<String>) -> Result<Vec<String>> {
    println!("🚀 Reading {} files concurrently with Tokio work-stealing", paths.len());
    
    // Spawn tasks - Tokio scheduler distributes across threads
    let handles: Vec<_> = paths
        .into_iter()
        .map(|path| {
            task::spawn(async move {
                // Simulate Parquet read (in real impl, this would call polars)
                sleep(Duration::from_millis(100)).await;
                
                // Return handle ID
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(
                    format!("handle_{}", uuid::Uuid::new_v4())
                )
            })
        })
        .collect();
    
    // Wait for all tasks - concurrent execution
    let results = join_all(handles).await;
    
    // Flatten Results with monadic error handling
    results
        .into_iter()
        .map(|r| r.map_err(|e| format!("Task join error: {}", e))?)
        .collect::<Result<Vec<_>>>()
}

/// Example 2: Batch Operations with Backpressure
/// 
/// Uses semaphore to limit concurrent operations (prevent OOM).
/// This is production-ready pattern for high-throughput systems.
async fn batch_collect_with_backpressure(
    handles: Vec<String>,
    max_concurrent: usize,
) -> Result<Vec<Vec<u8>>> {
    use tokio::sync::Semaphore;
    
    println!("🎯 Collecting {} DataFrames with max_concurrent={}", handles.len(), max_concurrent);
    
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    
    let tasks: Vec<_> = handles
        .into_iter()
        .map(|handle| {
            let sem = Arc::clone(&semaphore);
            
            task::spawn(async move {
                // Acquire permit (blocks if limit reached)
                let _permit = sem.acquire().await.unwrap();
                
                // Simulate collect operation
                sleep(Duration::from_millis(50)).await;
                
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(
                    vec![1, 2, 3, 4, 5]  // Arrow IPC bytes
                )
            })
        })
        .collect();
    
    let results = join_all(tasks).await;
    
    results
        .into_iter()
        .map(|r| r.map_err(|e| format!("Task error: {}", e))?)
        .collect()
}

/// Example 3: Real-Time Streaming with Channels
/// 
/// Uses Tokio channels for zero-copy message passing.
/// Achieves microsecond latencies for real-time data.
async fn real_time_streaming() -> Result<()> {
    use tokio::sync::mpsc;
    
    println!("📡 Setting up real-time streaming pipeline");
    
    // Bounded channel (backpressure when full)
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(100);
    
    // Producer task (simulates WebSocket/gRPC stream)
    let producer = task::spawn(async move {
        for i in 0..1000 {
            let data = vec![i; 100];  // Simulate Arrow batch
            
            // Send with backpressure
            if tx.send(data).await.is_err() {
                break;  // Receiver dropped
            }
            
            // Simulate real-time data arrival
            sleep(Duration::from_micros(100)).await;
        }
    });
    
    // Consumer task (processes stream)
    let consumer = task::spawn(async move {
        let mut count = 0;
        
        while let Some(batch) = rx.recv().await {
            // Process batch (zero-copy - just moves ownership)
            count += batch.len();
            
            // Simulate processing
            sleep(Duration::from_micros(50)).await;
        }
        
        println!("✅ Processed {} bytes", count);
    });
    
    // Wait for completion
    tokio::try_join!(producer, consumer)?;
    
    Ok(())
}

/// Example 4: Monadic Error Handling Pipeline
/// 
/// Uses Result chaining for elegant error propagation.
/// No exceptions - all errors are explicit in types.
async fn monadic_pipeline(path: String) -> Result<usize> {
    // Chain operations with ? operator (monadic bind)
    let handle = read_parquet(&path).await?;
    let filtered = apply_filter(handle, "col > 100").await?;
    let aggregated = aggregate(filtered, "sum").await?;
    let result = materialize(aggregated).await?;
    
    Ok(result)
}

async fn read_parquet(path: &str) -> Result<String> {
    sleep(Duration::from_millis(10)).await;
    Ok(format!("handle_{}", uuid::Uuid::new_v4()))
}

async fn apply_filter(handle: String, predicate: &str) -> Result<String> {
    sleep(Duration::from_millis(5)).await;
    Ok(format!("{}_filtered", handle))
}

async fn aggregate(handle: String, op: &str) -> Result<String> {
    sleep(Duration::from_millis(5)).await;
    Ok(format!("{}_agg", handle))
}

async fn materialize(handle: String) -> Result<usize> {
    sleep(Duration::from_millis(10)).await;
    Ok(42)  // Result value
}

/// Example 5: Graceful Shutdown with tokio::select!
/// 
/// Production-ready shutdown handling.
/// All tasks cleanup properly, no resource leaks.
async fn server_with_graceful_shutdown() -> Result<()> {
    use tokio::signal;
    use tokio::sync::broadcast;
    
    println!("🔧 Starting server with graceful shutdown support");
    
    // Shutdown signal
    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    
    // Spawn worker tasks
    let mut tasks = vec![];
    for i in 0..10 {
        let mut shutdown_rx = shutdown_tx.subscribe();
        
        let task = task::spawn(async move {
            loop {
                tokio::select! {
                    // Normal work
                    _ = sleep(Duration::from_secs(1)) => {
                        println!("Worker {} processing...", i);
                    }
                    
                    // Shutdown signal
                    _ = shutdown_rx.recv() => {
                        println!("Worker {} shutting down gracefully", i);
                        break;
                    }
                }
            }
        });
        
        tasks.push(task);
    }
    
    // Wait for SIGINT (Ctrl+C)
    println!("Press Ctrl+C to trigger graceful shutdown...");
    signal::ctrl_c().await?;
    
    println!("🛑 Shutdown signal received, stopping workers...");
    
    // Send shutdown signal
    let _ = shutdown_tx.send(());
    
    // Wait for all tasks with timeout
    tokio::time::timeout(
        Duration::from_secs(5),
        join_all(tasks)
    ).await?;
    
    println!("✅ All workers stopped cleanly");
    
    Ok(())
}

/// Example 6: Performance Metrics with Tokio
/// 
/// Built-in instrumentation for production monitoring.
async fn measure_throughput() -> Result<()> {
    use std::time::Instant;
    
    let start = Instant::now();
    let num_operations = 10_000;
    
    println!("⚡ Measuring throughput for {} operations", num_operations);
    
    // Concurrent operations
    let handles: Vec<_> = (0..num_operations)
        .map(|i| {
            task::spawn(async move {
                // Simulate lightweight operation
                sleep(Duration::from_micros(10)).await;
                i
            })
        })
        .collect();
    
    let results = join_all(handles).await;
    
    let elapsed = start.elapsed();
    let ops_per_sec = num_operations as f64 / elapsed.as_secs_f64();
    
    println!("✅ Throughput: {:.0} ops/sec", ops_per_sec);
    println!("   Latency: {:.2} µs/op", elapsed.as_micros() as f64 / num_operations as f64);
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("╔═══════════════════════════════════════════════════════╗");
    println!("║   Polaroid: Advanced Tokio Patterns & Performance    ║");
    println!("╚═══════════════════════════════════════════════════════╝\n");
    
    // Example 1: Concurrent reads
    let paths: Vec<String> = (0..50)
        .map(|i| format!("data/file_{}.parquet", i))
        .collect();
    
    let handles = concurrent_parquet_reads(paths).await?;
    println!("✅ Got {} handles\n", handles.len());
    
    // Example 2: Batch collect with backpressure
    let results = batch_collect_with_backpressure(handles, 10).await?;
    println!("✅ Collected {} DataFrames\n", results.len());
    
    // Example 3: Real-time streaming
    real_time_streaming().await?;
    println!();
    
    // Example 4: Monadic pipeline
    let result = monadic_pipeline("data/input.parquet".to_string()).await?;
    println!("✅ Pipeline result: {}\n", result);
    
    // Example 6: Throughput measurement
    measure_throughput().await?;
    println!();
    
    // Example 5: Graceful shutdown (commented - requires Ctrl+C)
    // server_with_graceful_shutdown().await?;
    
    println!("\n🎉 All examples completed successfully!");
    
    Ok(())
}
