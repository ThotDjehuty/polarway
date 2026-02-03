# Architecture Overview

## System Architecture

Polarway uses a **client-server architecture** with gRPC for communication and hybrid storage for persistence:

```text
┌───────────────────────────────────────────────────────────────────┐
│                          Client Layer                             │
│                                                                   │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │
│  │   Python    │  │    Rust     │  │     Go      │              │
│  │   Client    │  │   Client    │  │   Client    │   ... (gRPC) │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘              │
│         │                │                │                       │
│         └────────────────┴────────────────┘                       │
│                          │                                        │
│                    gRPC (50051)                                   │
└──────────────────────────┼────────────────────────────────────────┘
                           │
                           ▼
┌───────────────────────────────────────────────────────────────────┐
│                       Server Layer                                │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │              Polarway gRPC Server                        │    │
│  │                                                           │    │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │    │
│  │  │  Handle     │  │  Service    │  │   HTTP      │     │    │
│  │  │  Manager    │  │  Layer      │  │   API       │     │    │
│  │  └─────────────┘  └─────────────┘  └─────────────┘     │    │
│  │         │                │                │              │    │
│  │         └────────────────┴────────────────┘              │    │
│  │                          │                               │    │
│  │                          ▼                               │    │
│  │         ┌──────────────────────────────────┐            │    │
│  │         │      Polars DataFrame Engine     │            │    │
│  │         │  • Lazy evaluation               │            │    │
│  │         │  • Query optimization            │            │    │
│  │         │  • Parallel execution            │            │    │
│  │         └──────────────────────────────────┘            │    │
│  └───────────────────────┬───────────────────────────────────┘    │
│                          │                                        │
│                          ▼                                        │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │                  Storage Layer                           │    │
│  │                                                           │    │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │    │
│  │  │   Cache     │  │   Parquet   │  │   DuckDB    │     │    │
│  │  │(LRU, 2GB)   │  │(Cold, zstd) │  │(SQL Query)  │     │    │
│  │  └─────────────┘  └─────────────┘  └─────────────┘     │    │
│  └──────────────────────────────────────────────────────────┘    │
└───────────────────────────────────────────────────────────────────┘
```

## Component Details

### Client Layer

**Supported Languages**:
- Python (polarway-py)
- Rust (polarway-rs)
- Go (polarway-go)
- Any gRPC-capable language

**Features**:
- DataFrame handle abstraction
- Lazy evaluation
- Result/Option monads for error handling
- Automatic reconnection
- Connection pooling

### Server Layer

#### Handle Manager

Manages DataFrame lifecycle:
- UUID-based handles
- Reference counting
- Automatic cleanup
- Thread-safe operations

```rust
pub struct HandleManager {
    handles: Arc<DashMap<String, DataFrameHandle>>,
    storage: Arc<HybridStorage>,
}
```

#### Service Layer

Implements gRPC service definition:
- DataFrame operations (filter, select, group_by, etc.)
- Streaming operations (scan, write)
- Storage operations (store, load, query)
- Metadata queries (schema, stats)

```rust
#[tonic::async_trait]
impl PolarwayService for PolarwayDataFrameService {
    async fn create_from_csv(...) -> Result<Response<HandleResponse>, Status>;
    async fn filter(...) -> Result<Response<HandleResponse>, Status>;
    async fn select(...) -> Result<Response<HandleResponse>, Status>;
    // ... more operations
}
```

#### HTTP API

REST-like endpoint for SQL queries (QuestDB-compatible):

```bash
# Execute SQL query
curl -X GET "http://localhost:8080/exec?query=SELECT * FROM read_parquet('/data/cold/*.parquet')"
```

### Storage Layer

Three-tier architecture:

1. **Cache** (Hot, RAM)
   - LRU eviction
   - 2GB default size
   - O(1) lookups

2. **Parquet** (Cold, Disk)
   - zstd compression level 19
   - 18× compression ratio
   - Column-oriented

3. **DuckDB** (Analytics)
   - SQL queries
   - Zero-copy Parquet reads
   - SIMD vectorization

## Data Flow

### Write Path

```text
Client
  │
  │ gRPC call: store(key, data)
  │
  ▼
Server (Handle Manager)
  │
  │ 1. Serialize to Arrow RecordBatch
  │
  ▼
Storage Layer
  │
  ├─► Cache: store(key, batch)        [Fast, RAM]
  │
  └─► Parquet: compress & write       [Durable, Disk]
      │
      └─► File: /data/cold/key.parquet
```

### Read Path (Smart Loading)

```text
Client
  │
  │ gRPC call: load(key)
  │
  ▼
Server (Handle Manager)
  │
  ▼
Storage Layer (smart_load)
  │
  ├─► Cache: check(key)
  │     │
  │     ├─► Hit? Return immediately    [~1ms]
  │     │
  │     └─► Miss? Continue below
  │
  ├─► Parquet: read(key)
  │     │
  │     ├─► Found? Decompress          [~50ms]
  │     │     │
  │     │     └─► Warm cache           [Cache for next access]
  │     │
  │     └─► Not found? Return None
  │
  ▼
Client (Handle created)
```

### Query Path (SQL Analytics)

```text
Client
  │
  │ gRPC call: query(sql)
  │
  ▼
Server (Handle Manager)
  │
  ▼
Storage Layer
  │
  └─► DuckDB: execute_sql(sql)
      │
      ├─► Parse SQL
      │
      ├─► Optimize query plan
      │
      ├─► Scan Parquet (zero-copy)
      │
      ├─► Execute (SIMD vectorized)
      │
      └─► Return Arrow RecordBatch
          │
          ▼
Server (Handle Manager)
  │
  └─► Create handle for result
      │
      ▼
Client (Handle ID returned)
```

## Threading Model

### Server

- **Tokio runtime**: Async I/O for gRPC
- **Rayon thread pool**: Parallel DataFrame operations
- **RwLock**: Thread-safe storage access

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Tokio async runtime
    let server = Server::builder()
        .add_service(service)
        .serve(addr)
        .await?;

    Ok(())
}

// Rayon for parallel operations
fn parallel_operation(df: DataFrame) -> DataFrame {
    df.par_iter()
        .map(|row| /* parallel processing */)
        .collect()
}
```

### Storage

- **RwLock<LruCache>**: Multiple readers, single writer
- **Mutex<ParquetWriter>**: Sequential writes (Parquet not thread-safe)
- **Arc**: Shared ownership across threads

## Memory Management

### Handle Lifecycle

```text
1. Create
   └─► Client requests operation
       └─► Server creates DataFrame
           └─► Assign UUID handle
               └─► Store in HandleManager

2. Use
   └─► Client references handle in operations
       └─► Server looks up DataFrame
           └─► Execute operation
               └─► Return new handle (lazy)

3. Release
   └─► Client drops handle (or explicit release)
       └─► Server decrements ref count
           └─► Ref count = 0?
               └─► Remove from HandleManager
                   └─► DataFrame dropped
```

### Cache Eviction

```text
Cache Full?
  │
  ├─► No: Add new item
  │
  └─► Yes: Evict LRU
      │
      ├─► Find least recently used
      │
      ├─► Remove from cache
      │
      └─► Add new item
```

## Error Handling

Polarway uses **Result/Option monads** for explicit error handling:

```rust
// Result<T, E>: Operation may fail
pub fn load(&self, key: &str) -> Result<Option<RecordBatch>, Box<dyn Error>> {
    // ...
}

// Option<T>: Value may not exist
pub fn find_handle(&self, id: &str) -> Option<DataFrameHandle> {
    self.handles.get(id).map(|h| h.clone())
}
```

**Error propagation**:
```rust
// ? operator: propagate errors up the stack
let data = self.storage.load(key)?;
let batch = data.ok_or("Key not found")?;
```

## Performance Optimizations

### Zero-Copy Arrow

DataFrames use Arrow format for zero-copy:
- No serialization overhead
- Direct memory access
- Efficient network transfer

### Lazy Evaluation

Operations build a query plan:
```python
# No computation yet
df = client.from_csv("data.csv")
df = df.filter(pl.col("x") > 10)
df = df.select(["x", "y"])

# Execute entire plan at once
result = df.collect()  # Optimized execution
```

### Streaming

Large datasets processed in chunks:
```python
# Stream 1GB file in 100MB chunks
for batch in client.scan_parquet("large.parquet", batch_size=100_000):
    process(batch)  # Constant memory usage
```

### Compression

Parquet with zstd level 19:
- 18× compression ratio
- Fast decompression (~50ms for 1M rows)
- Minimal CPU overhead

## Security

### Network

- **TLS**: gRPC with TLS 1.3
- **Authentication**: Token-based auth
- **Authorization**: Role-based access control

### Storage

- **Key sanitization**: Prevent directory traversal
- **Atomic writes**: fsync after each write
- **Read-only DuckDB**: No DDL operations

## Monitoring

### Metrics (Prometheus)

```text
# Cache metrics
polarway_cache_hits_total
polarway_cache_misses_total
polarway_cache_hit_rate

# Storage metrics
polarway_storage_size_bytes
polarway_storage_compression_ratio

# Request metrics
polarway_requests_total{operation="filter"}
polarway_request_duration_seconds{operation="filter"}

# Handle metrics
polarway_active_handles_total
polarway_handles_created_total
```

### Logging (tracing)

```rust
#[tracing::instrument(skip(self))]
async fn filter(&self, request: Request<FilterRequest>) -> Result<Response<HandleResponse>, Status> {
    tracing::info!("Filter operation started");
    // ...
    tracing::info!("Filter operation completed");
}
```

## Deployment

### Docker

```bash
docker run -d \
  -p 50051:50051 \
  -p 8080:8080 \
  -v /data/cold:/data/cold \
  -e STORAGE_PATH=/data/cold \
  -e CACHE_SIZE_GB=2.0 \
  polarway/polarway-grpc:v0.53.0
```

### Docker Compose

```yaml
services:
  polarway:
    image: polarway/polarway-grpc:v0.53.0
    ports:
      - "50051:50051"
      - "8080:8080"
    volumes:
      - ./data/cold:/data/cold
    environment:
      - STORAGE_PATH=/data/cold
      - CACHE_SIZE_GB=2.0
      - RUST_LOG=info
```

### Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: polarway
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: polarway
        image: polarway/polarway-grpc:v0.53.0
        ports:
        - containerPort: 50051
        volumeMounts:
        - name: storage
          mountPath: /data/cold
      volumes:
      - name: storage
        persistentVolumeClaim:
          claimName: polarway-storage
```

## References

- [gRPC Documentation](https://grpc.io/docs/)
- [Apache Arrow Format](https://arrow.apache.org/docs/format/Columnar.html)
- [Polars Documentation](https://pola-rs.github.io/polars/)
- [Tokio Async Runtime](https://tokio.rs/)
