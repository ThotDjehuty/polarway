# Polaroid Architecture Guide

Deep dive into Polaroid's architecture, design decisions, and implementation details.

## 🏗️ High-Level Architecture

### System Overview

```
┌─────────────────────────────────────────────────────────┐
│                    Client Layer                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐             │
│  │  Python  │  │   Rust   │  │  Other   │             │
│  │  Client  │  │  Client  │  │  gRPC    │             │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘             │
│       │             │             │                      │
│       └─────────────┴─────────────┘                      │
│                     │                                    │
│              gRPC (HTTP/2)                               │
│                     │                                    │
└─────────────────────┼────────────────────────────────────┘
                      │
┌─────────────────────┼────────────────────────────────────┐
│              Polaroid Server                             │
│                     │                                    │
│  ┌──────────────────▼────────────────────┐              │
│  │         gRPC Service Layer            │              │
│  │  (Tonic/Tower middleware stack)       │              │
│  └──────────────────┬────────────────────┘              │
│                     │                                    │
│  ┌──────────────────▼────────────────────┐              │
│  │      Handle Manager (TTL Cache)       │              │
│  │  ┌────────┐  ┌────────┐  ┌────────┐  │              │
│  │  │Handle 1│  │Handle 2│  │Handle N│  │              │
│  │  │(30 min)│  │(30 min)│  │(30 min)│  │              │
│  │  └───┬────┘  └───┬────┘  └───┬────┘  │              │
│  └──────┼───────────┼───────────┼────────┘              │
│         │           │           │                        │
│  ┌──────▼───────────▼───────────▼────────┐              │
│  │       DataFrame Engine (Polars)       │              │
│  │  ┌──────────┐  ┌──────────────────┐  │              │
│  │  │  Lazy    │  │  Query Optimizer │  │              │
│  │  │  Eval    │  │  (Projection/    │  │              │
│  │  │          │  │   Predicate      │  │              │
│  │  │          │  │   Pushdown)      │  │              │
│  │  └────┬─────┘  └────────┬─────────┘  │              │
│  └───────┼──────────────────┼────────────┘              │
│          │                  │                            │
│  ┌───────▼──────────────────▼────────────┐              │
│  │     Execution Engine (Arrow)          │              │
│  │  ┌──────────┐  ┌────────────────┐    │              │
│  │  │ Parallel │  │  SIMD Kernels  │    │              │
│  │  │ Executor │  │  (vectorized)  │    │              │
│  │  └────┬─────┘  └────────┬───────┘    │              │
│  └───────┼──────────────────┼────────────┘              │
│          │                  │                            │
│  ┌───────▼──────────────────▼────────────┐              │
│  │      Arrow Memory (Columnar)          │              │
│  │  ┌────────┐ ┌────────┐ ┌────────┐    │              │
│  │  │ Batch1 │ │ Batch2 │ │ BatchN │    │              │
│  │  └────────┘ └────────┘ └────────┘    │              │
│  └───────────────────────────────────────┘              │
│                     │                                    │
│  ┌──────────────────▼────────────────────┐              │
│  │         I/O Layer (Tokio)             │              │
│  │  ┌──────┐ ┌────────┐ ┌──────────┐    │              │
│  │  │Parquet│ │WebSocket│ │REST API │    │              │
│  │  │ CSV  │ │ Kafka  │ │ gRPC    │    │              │
│  │  └──────┘ └────────┘ └──────────┘    │              │
│  └───────────────────────────────────────┘              │
└──────────────────────────────────────────────────────────┘
```

### Key Components

1. **gRPC Service Layer**: Handles client requests via HTTP/2
2. **Handle Manager**: Manages DataFrame lifecycle with TTL
3. **DataFrame Engine**: Lazy evaluation and query optimization
4. **Execution Engine**: Parallel execution with Arrow compute kernels
5. **I/O Layer**: Async file/network operations

## 🎯 Core Design Principles

### 1. Performance First

**Zero-Copy Operations**

Polaroid uses Apache Arrow's zero-copy IPC for data transfer:

```rust
// No serialization needed - Arrow buffers shared directly
let batch: RecordBatch = dataframe.to_arrow_batch()?;
let ipc_bytes = arrow_ipc::writer::stream_to_bytes(&batch);
// Client receives same memory layout
```

**Memory Efficiency**

- Columnar storage reduces memory fragmentation
- Lazy evaluation defers computation until needed
- Streaming prevents OOM on large datasets
- Handle-based architecture avoids data duplication

**SIMD Optimization**

Arrow compute kernels use SIMD instructions:

```rust
// Vectorized filter operation (8 values at once on AVX2)
fn filter_gt_simd(column: &Float64Array, threshold: f64) -> BooleanArray {
    // Uses _mm256_cmp_pd for 4x speedup
    column.iter().map(|v| v > threshold).collect()
}
```

### 2. Type Safety

**Rust's Type System**

```rust
// Compile-time guarantees
fn apply_filter(df: DataFrame, expr: Expr) -> Result<DataFrame, PolarsError> {
    // Type checked at compile time
    // No null pointer exceptions
    // Thread-safe by default
}
```

**Result Types**

```rust
// Explicit error handling
pub enum Result<T> {
    Ok(T),
    Err(PolarsError),
}

// Forces caller to handle errors
let df = read_parquet("file.parquet")?;
```

### 3. Composability

**Expression System**

```python
# Expressions compose naturally
expr = (
    (pd.col("price") * pd.col("quantity"))
    .cast(pd.Float64)
    .alias("notional")
)

# Can be reused across operations
df1.with_column(expr)
df2.with_column(expr)
```

**Lazy Evaluation**

```python
# Operations build a query plan
df = pd.read_parquet("data.parquet")
df = df.filter(pd.col("price") > 100)  # No execution yet
df = df.select(["symbol", "price"])     # Still no execution

# Optimized as single query
result = df.collect()  # Execute once
```

## 🗄️ Data Model

### Columnar Storage

**Why Columnar?**

Traditional row-based storage:
```
Row 1: [id=1, name="Alice", age=30, city="NYC"]
Row 2: [id=2, name="Bob", age=25, city="LA"]
```

Columnar storage (Arrow):
```
id:   [1, 2]
name: ["Alice", "Bob"]
age:  [30, 25]
city: ["NYC", "LA"]
```

**Benefits:**
- Better compression (similar values together)
- SIMD operations (process 4-16 values at once)
- Cache efficiency (fewer cache misses)
- Selective column loading (skip unused columns)

### Arrow Format

```
┌─────────────────────────────────────┐
│         RecordBatch                 │
├─────────────────────────────────────┤
│ Schema:                             │
│   field: "price", type: Float64     │
│   field: "volume", type: Int64      │
├─────────────────────────────────────┤
│ Arrays:                             │
│   Float64Array [100.5, 101.2, ...]  │
│   Int64Array   [1000, 2000, ...]    │
└─────────────────────────────────────┘
       │
       ├─► Buffer (validity bitmap)
       ├─► Buffer (offsets for var-length)
       └─► Buffer (data values)
```

### Null Handling

```rust
// Validity bitmap (1 bit per value)
// 1 = valid, 0 = null
[1, 1, 0, 1] → values: [10, 20, _, 30]

// Efficient null checks with SIMD
fn has_nulls(array: &Array) -> bool {
    array.null_count() > 0  // O(1) operation
}
```

### Type System

```rust
pub enum DataType {
    // Numeric
    Int8, Int16, Int32, Int64,
    UInt8, UInt16, UInt32, UInt64,
    Float32, Float64,
    
    // Temporal
    Date,
    Datetime(TimeUnit, Option<String>),  // timezone
    Duration(TimeUnit),
    Time,
    
    // Complex
    Utf8,
    Boolean,
    List(Box<DataType>),
    Struct(Vec<Field>),
    Categorical,
}
```

## ⚙️ Query Execution

### Lazy Evaluation Pipeline

```
1. Expression Building
   ↓
2. Logical Plan Construction
   ↓
3. Query Optimization
   ↓
4. Physical Plan Generation
   ↓
5. Parallel Execution
   ↓
6. Result Materialization
```

### Query Optimizer

**Projection Pushdown**

Before optimization:
```python
df = pd.read_parquet("data.parquet")  # Read all 100 columns
df = df.select(["col1", "col2"])       # Use only 2 columns
```

After optimization:
```
Only read col1, col2 from Parquet (98 columns skipped)
```

**Predicate Pushdown**

Before:
```python
df = pd.read_parquet("data.parquet")  # Read 1M rows
df = df.filter(pd.col("date") > "2024-01-01")  # Filter to 10K rows
```

After:
```
Push filter into Parquet reader → Read only 10K rows
```

**Example Optimization:**

```python
# User writes this
result = (
    pd.read_parquet("sales.parquet")
    .filter(pd.col("region") == "US")
    .select(["product", "revenue", "date"])
    .filter(pd.col("date") > "2024-01-01")
    .group_by("product")
    .agg({"revenue": "sum"})
)

# Optimizer rewrites to:
# 1. Push both filters to Parquet reader
# 2. Read only [product, revenue, date, region] columns
# 3. Apply filters during read (skip 90% of data)
# 4. Group and aggregate remaining rows
```

### Execution Engine

**Parallel Execution**

```rust
// Split data into chunks
let chunks = partition_data(data, num_cores);

// Process in parallel
let results: Vec<_> = chunks
    .par_iter()  // Rayon parallel iterator
    .map(|chunk| apply_filter(chunk, &predicate))
    .collect();

// Merge results
merge_chunks(results)
```

**Pipeline Parallelism**

```
Read Thread → Parse Thread → Filter Thread → Aggregate Thread
    ↓             ↓              ↓               ↓
 Chunk 1       Chunk 1        Chunk 1         Result 1
 Chunk 2       Chunk 2        Chunk 2         Result 2
 Chunk 3       Chunk 3        Chunk 3         Result 3
```

## 🌐 Handle-Based Architecture

### Why Handles?

**Problem with Embedded Libraries:**

```python
# Polars (PyO3) - data lives in Python process
df = pl.read_parquet("10GB.parquet")  # Uses 10GB RAM in Python
df2 = df.filter(...)                  # Another 10GB copy
df3 = df2.select(...)                 # Another copy
# Total: 30GB RAM used
```

**Solution with Handles:**

```python
# Polaroid - data lives on server
df = pd.read_parquet("10GB.parquet")  # Handle("abc"), server uses 10GB
df2 = df.filter(...)                  # Handle("def"), server uses 10GB
df3 = df2.select(...)                 # Handle("ghi"), server still 10GB
# Total: 10GB RAM on server, ~1KB in Python
```

### Handle Lifecycle

```rust
pub struct HandleManager {
    handles: Arc<RwLock<HashMap<String, DataFrame>>>,
    ttl: Duration,  // 30 minutes default
}

impl HandleManager {
    pub fn create(&self, df: DataFrame) -> String {
        let id = Uuid::new_v4().to_string();
        let expiry = Instant::now() + self.ttl;
        
        self.handles.write().insert(id.clone(), (df, expiry));
        
        // Spawn cleanup task
        tokio::spawn(cleanup_expired_handle(id.clone(), expiry));
        
        id
    }
    
    pub fn get(&self, id: &str) -> Option<DataFrame> {
        self.handles.read().get(id).map(|(df, _)| df.clone())
    }
    
    pub fn extend_ttl(&self, id: &str) {
        // Reset expiry on access
        if let Some((_, expiry)) = self.handles.write().get_mut(id) {
            *expiry = Instant::now() + self.ttl;
        }
    }
}
```

### Memory Management

**Reference Counting:**

```rust
// DataFrames use Arc<> for cheap cloning
pub struct DataFrame {
    data: Arc<Vec<Series>>,  // Shared ownership
}

// Multiple handles can reference same data
let df1 = DataFrame::new(data);
let df2 = df1.clone();  // Just increments Arc counter (O(1))
```

## 🚀 Distributed Computing

### Architecture Overview

Polaroid’s distributed direction is:
- **Time-series storage/metadata/observability** anchored in **QuestDB** (tables, partitions, retention, SQL, system tables).
- **Distributed query execution** via **DataFusion + Ballista** (scheduler + executors, shuffle, object store).
- **APIs**: gRPC remains the primary engine API; a **QuestDB-like REST endpoint** (`/exec`) is added for compatibility and easy integrations.

In that model, Polaroid is the **gateway / control-plane** that:
- receives API requests (gRPC/REST)
- resolves handles and metadata
- compiles queries (DataFusion logical plan)
- executes locally (single-node) or dispatches to Ballista (distributed)
- returns results as handles (and via Arrow IPC streams)

```
Clients
  (Python/Rust/REST)
      |
      v
  gRPC/HTTP Load Balancer
      |
      v
  Polaroid API Gateways (stateless)
   - gRPC Service
   - REST /exec (QuestDB-like)
   - Handle routing
   - Plan compilation
      |
      | dispatch/execute
      v
  DataFusion (single-node)  OR  Ballista (distributed)
      |
      | reads/writes
      v
  Object Store (results, shuffles) + QuestDB (time-series, metadata)
```

### Ballista (distributed execution)

Ballista architecture (at a high level):
- **Scheduler**: receives DataFusion plans and coordinates execution.
- **Executors**: run partitions of the plan and exchange shuffle data.
- **Object store**: stores shuffle and result artifacts (recommended for scale).

Polaroid integration direction:
- Polaroid builds a DataFusion plan and submits it to Ballista when `distributed=true`.
- Handles reference results stored externally (object store).
- QuestDB remains the authoritative store for time-series tables and operational metadata.

### Data Partitioning

```rust
// Hash partitioning
fn partition_by_hash(df: DataFrame, key: &str, n: usize) -> Vec<DataFrame> {
    df.partition_by(|row| {
        hash(row[key]) % n  // Deterministic assignment
    })
}

// Range partitioning
fn partition_by_range(df: DataFrame, key: &str, ranges: &[Range]) 
    -> Vec<DataFrame> 
{
    ranges.iter().map(|range| {
        df.filter(col(key).between(range.start, range.end))
    }).collect()
}
```

## 🔄 Async Operations

### Tokio Runtime

```rust
#[tokio::main]
async fn main() {
    let server = PolaroidServer::new();
    
    // Handle connections concurrently
    let listener = TcpListener::bind("0.0.0.0:50051").await?;
    
    loop {
        let (socket, _) = listener.accept().await?;
        
        // Spawn task for each connection
        tokio::spawn(async move {
            handle_connection(socket).await
        });
    }
}
```

### Streaming I/O

```rust
// Async Parquet reader
pub async fn read_parquet_stream(path: &str) 
    -> impl Stream<Item = Result<RecordBatch>> 
{
    let file = File::open(path).await?;
    let reader = AsyncParquetReader::new(file);
    
    // Yield batches as they're read
    stream! {
        while let Some(batch) = reader.next_batch().await? {
            yield Ok(batch);
        }
    }
}
```

## 📊 I/O Subsystem

### File Formats

**Parquet Reader:**

```rust
pub struct ParquetReader {
    // Memory-mapped file for zero-copy reads
    mmap: Mmap,
    
    // Metadata (footer) parsed once
    metadata: ParquetMetadata,
    
    // Row groups (can read in parallel)
    row_groups: Vec<RowGroupReader>,
}

impl ParquetReader {
    pub async fn read_row_group_parallel(&self, rg: usize) 
        -> Result<RecordBatch> 
    {
        // Read all columns in parallel
        let columns = join_all(
            self.row_groups[rg]
                .columns()
                .map(|col| tokio::spawn(read_column(col)))
        ).await?;
        
        RecordBatch::try_new(self.schema(), columns)
    }
}
```

### WebSocket Streaming

```rust
pub async fn websocket_stream(url: &str) 
    -> impl Stream<Item = Result<RecordBatch>> 
{
    let (mut ws, _) = connect_async(url).await?;
    let mut buffer = Vec::new();
    
    stream! {
        while let Some(msg) = ws.next().await {
            buffer.push(parse_message(msg)?);
            
            // Yield batch every 1000 messages
            if buffer.len() >= 1000 {
                let batch = create_batch(&buffer)?;
                buffer.clear();
                yield Ok(batch);
            }
        }
    }
}
```

## 🐍 Python Integration

### PyO3 Bindings

```rust
#[pyclass]
pub struct DataFrame {
    handle: String,
    client: Arc<GrpcClient>,
}

#[pymethods]
impl DataFrame {
    fn select(&self, columns: Vec<String>) -> PyResult<Self> {
        let new_handle = self.client.select(self.handle, columns)?;
        Ok(DataFrame {
            handle: new_handle,
            client: self.client.clone(),
        })
    }
    
    fn collect(&self) -> PyResult<PyArrowTable> {
        let bytes = self.client.collect(self.handle)?;
        let table = deserialize_arrow_ipc(bytes)?;
        Ok(PyArrowTable(table))
    }
}
```

## 🎯 Design Decisions

### Why Fork Polars?

**Reasons:**

1. **gRPC Architecture**: Requires handle-based design incompatible with Polars
2. **Distributed Computing**: Needs network layer and partitioning
3. **Streaming-First**: Different memory management strategy
4. **Financial Focus**: Domain-specific optimizations (OHLCV, tick data)

**What We Keep:**

- Core DataFrame engine (solid foundation)
- Expression system (powerful and composable)
- Arrow integration (industry standard)
- Query optimizer (excellent performance)

**What We Change:**

- Remove PyO3 bindings (use gRPC instead)
- Add handle management layer
- Implement streaming execution
- Add time-series operations

### Why Rust?

**Advantages:**

1. **Memory Safety**: No null pointers, no data races
2. **Performance**: Zero-cost abstractions, SIMD
3. **Concurrency**: Safe parallelism with Send/Sync
4. **Type System**: Compile-time guarantees

**Example:**

```rust
// This won't compile (caught at compile time!)
let df = DataFrame::new();
thread::spawn(move || {
    df.filter(...)  // Error: DataFrame not Send
});

// Fix: Use Arc<> for shared ownership
let df = Arc::new(DataFrame::new());
let df_clone = df.clone();
thread::spawn(move || {
    df_clone.filter(...)  // OK!
});
```

### Why Arrow?

**Benefits:**

1. **Zero-Copy**: Share memory between processes
2. **Language Agnostic**: C++, Rust, Python, Java all use same format
3. **Columnar**: Perfect for analytics
4. **Ecosystem**: Works with Parquet, DataFusion, Spark

**Example:**

```rust
// Create Arrow data in Rust
let array = Int64Array::from(vec![1, 2, 3]);
let batch = RecordBatch::try_new(schema, vec![array])?;

// Send to Python (zero-copy)
let ipc_bytes = write_ipc_stream(&batch);
// Python receives same memory layout, no deserialization needed
```

## 📈 Future Architecture

### Planned Enhancements

**GPU Acceleration:**

```rust
// Use RAPIDS cuDF for GPU operations
pub trait GpuExecutable {
    fn to_gpu(&self) -> GpuDataFrame;
    fn execute_on_gpu(&self, expr: Expr) -> GpuDataFrame;
}
```

**Advanced Query Planning:**

```
Cost-Based Optimizer:
- Statistics collection (histograms, min/max, null count)
- Cardinality estimation
- Join reordering based on costs
- Adaptive execution (change plan based on runtime stats)
```

**SQL Interface:**

```sql
-- Use DataFusion for SQL parsing
SELECT symbol, AVG(price) as avg_price
FROM polaroid://data.parquet
WHERE date > '2024-01-01'
GROUP BY symbol
ORDER BY avg_price DESC
LIMIT 10;
```

---

**See Also**:
- [Quick Reference](QUICK_REFERENCE.md) - Common operations cheat sheet
- [API Documentation](API_DOCUMENTATION.md) - Complete API reference
- [Migration Guide](MIGRATION_GUIDE.md) - Moving from Polars
- [REST Exec API](REST_EXEC_API.md) - QuestDB-like `/exec` endpoint
- [Distributed Getting Started](DISTRIBUTED_GETTING_STARTED.md) - Multi-container/VM setup
- [Distributed Implementation Plan](DISTRIBUTED_IMPLEMENTATION_PLAN.md) - Phased rollout and best practices
