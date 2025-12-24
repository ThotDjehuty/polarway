# Polaroid: FDAP-Optimized DataFrame Engine

## Project Overview

**Polaroid** is a next-generation DataFrame library forked from Polars, redesigned around the FDAP (Flight-DataFusion-Arrow-Parquet) stack with emphasis on:

- **Functional Programming**: Type-safe, composable operations with monadic error handling
- **Streaming-First**: True streaming for time-series and real-time data processing
- **gRPC Interface**: Replace PyO3 with gRPC for language-agnostic, network-capable interface
- **Time-Series Native**: First-class support for financial and time-series operations
- **Type Safety**: Strong typing with Result/Option monads, no panics
- **Network-Native**: Built-in WebSocket, REST API, and gRPC data sources
- **Legacy Compatible**: Maintain API compatibility with Polars where possible

## Architecture Principles

### 1. **Functional Core, Imperative Shell**

```
┌───────────────────────────────────────────────────┐
│         Python/Client Layer (Imperative)          │
│  - polaroid.DataFrame, .scan_parquet(), .lazy()  │
└─────────────────────┬─────────────────────────────┘
                      │ gRPC (Arrow Flight)
┌─────────────────────▼─────────────────────────────┐
│          Rust gRPC Service Layer                  │
│  - Request routing, authentication, streaming     │
└─────────────────────┬─────────────────────────────┘
                      │
┌─────────────────────▼─────────────────────────────┐
│      Functional Core (Pure Rust)                  │
│  - Immutable operations, monadic error handling   │
│  - Query planning with Result<Plan, PolaroidError>│
│  - Stream processing with futures::Stream         │
└───────────────────────────────────────────────────┘
```

### 2. **Replace PyO3 with gRPC**

**Why gRPC over PyO3?**

| Aspect | PyO3 | gRPC (Polaroid) |
|--------|------|-----------------|
| **Coupling** | Tight - Python-specific | Loose - any language |
| **Safety** | Unsafe blocks, GIL issues | Memory-safe channels |
| **Network** | Local only | Network-native |
| **Streaming** | Limited | First-class via Arrow Flight |
| **Versioning** | Binary ABI issues | Protocol-based versioning |
| **Type Safety** | Runtime errors | Compile-time schema checks |
| **Debugging** | Hard (cross-language) | gRPC interceptors, tracing |

**Architecture:**

```python
# OLD (Polars - PyO3)
import polars as pl
df = pl.read_parquet("data.parquet")  # Direct FFI call

# NEW (Polaroid - gRPC)
import polaroid as pd
df = pd.read_parquet("data.parquet")  # gRPC call to Rust server
```

Under the hood:
```
Python Client
    ↓ (gRPC protobuf)
Polaroid gRPC Server (Rust)
    ↓ (Arrow IPC stream)
Python Client receives Arrow RecordBatch
```

### 3. **Time-Series First Design**

**Built-in Time-Series Operations:**

```rust
// Core time-series types
pub enum TimeSeriesFrequency {
    Tick,           // Microsecond/nanosecond
    Second(u32),    // 1s, 5s, 30s
    Minute(u32),    // 1m, 5m, 15m, 30m
    Hour(u32),      // 1h, 4h
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

pub struct TimeSeriesFrame {
    data: DataFrame,
    timestamp_col: String,
    frequency: Option<TimeSeriesFrequency>,
    is_sorted: bool,
}

// Time-series operations
impl TimeSeriesFrame {
    fn resample(&self, freq: TimeSeriesFrequency) -> Result<Self>;
    fn rolling_window(&self, window: Duration) -> Result<Self>;
    fn lag(&self, periods: i64) -> Result<Self>;
    fn diff(&self, periods: i64) -> Result<Self>;
    fn ffill(&self) -> Result<Self>;
    fn align(&self, other: &Self) -> Result<(Self, Self)>;
}
```

### 4. **Monadic Error Handling**

**No Panics - Only Results:**

```rust
// OLD (Polars - can panic)
let df = df.filter(col("price").gt(100))?;  // Panics on missing column

// NEW (Polaroid - pure Result)
let df: Result<DataFrame, PolaroidError> = df
    .and_then(|df| df.filter(col("price").gt(100)))
    .and_then(|df| df.select(&["symbol", "price"]))
    .map(|df| df.sort("symbol", false));

// Or using monadic chaining
let result = monad! {
    let filtered <- df.filter(col("price").gt(100));
    let selected <- filtered.select(&["symbol", "price"]);
    selected.sort("symbol", false)
};
```

### 5. **Stream Processing Architecture**

**True Streaming with Tokio:**

```rust
use futures::stream::{Stream, StreamExt};
use arrow::record_batch::RecordBatch;

pub trait DataSource: Send + Sync {
    fn stream(&self) -> Pin<Box<dyn Stream<Item = Result<RecordBatch>> + Send>>;
}

// WebSocket source
pub struct WebSocketSource {
    url: String,
    schema: Schema,
}

impl DataSource for WebSocketSource {
    fn stream(&self) -> Pin<Box<dyn Stream<Item = Result<RecordBatch>> + Send>> {
        let stream = async_stream::stream! {
            let ws = connect_websocket(&self.url).await?;
            while let Some(msg) = ws.next().await {
                let batch = parse_to_record_batch(msg?, &self.schema)?;
                yield Ok(batch);
            }
        };
        Box::pin(stream)
    }
}

// REST API source with pagination
pub struct RestApiSource {
    base_url: String,
    endpoint: String,
    pagination: PaginationStrategy,
}

// gRPC streaming source
pub struct GrpcStreamSource {
    endpoint: String,
    method: String,
}
```

## Core Components

### 1. **polaroid-core** (Rust)

Functional core with immutable operations:

```rust
// Immutable operations return new instances
pub trait DataFrameOps {
    fn filter(self, predicate: Expr) -> Result<Self, PolaroidError>;
    fn select(self, columns: &[&str]) -> Result<Self, PolaroidError>;
    fn with_column(self, name: &str, expr: Expr) -> Result<Self, PolaroidError>;
}

// Lazy evaluation with monadic composition
pub enum LazyExecution<T> {
    Pure(T),
    Map(Box<LazyExecution<T>>, Box<dyn Fn(T) -> T>),
    Bind(Box<LazyExecution<T>>, Box<dyn Fn(T) -> LazyExecution<T>>),
}

impl<T> LazyExecution<T> {
    pub fn execute(self) -> Result<T, PolaroidError> {
        // Optimize and execute the computation graph
    }
}
```

### 2. **polaroid-grpc** (Rust)

gRPC service layer with Arrow Flight integration:

```rust
use tonic::{Request, Response, Status};
use arrow_flight::{FlightData, FlightDescriptor};

#[tonic::async_trait]
impl FlightService for PolaroidFlightService {
    async fn do_get(
        &self,
        request: Request<Ticket>,
    ) -> Result<Response<Self::DoGetStream>, Status> {
        let ticket = request.into_inner();
        let query = decode_query(&ticket)?;
        
        // Execute query and stream results
        let stream = self.engine
            .execute_query(query)
            .await?
            .map(|batch| encode_flight_data(batch));
        
        Ok(Response::new(Box::pin(stream)))
    }
    
    async fn do_put(
        &self,
        request: Request<Self::DoPutStream>,
    ) -> Result<Response<PutResult>, Status> {
        // Stream ingestion
    }
}
```

### 3. **polaroid-python** (Python Client)

Thin Python client that communicates via gRPC:

```python
import grpc
from polaroid.proto import polaroid_pb2, polaroid_pb2_grpc
import pyarrow as pa

class DataFrame:
    def __init__(self, stub: polaroid_pb2_grpc.DataFrameServiceStub, handle: str):
        self._stub = stub
        self._handle = handle  # Server-side handle
    
    def filter(self, predicate: str) -> 'DataFrame':
        """Apply filter - returns new DataFrame handle"""
        request = polaroid_pb2.FilterRequest(
            handle=self._handle,
            predicate=predicate
        )
        response = self._stub.Filter(request)
        return DataFrame(self._stub, response.handle)
    
    def collect(self) -> pa.Table:
        """Execute and collect results via Arrow IPC"""
        request = polaroid_pb2.CollectRequest(handle=self._handle)
        
        # Stream Arrow batches
        batches = []
        for response in self._stub.Collect(request):
            batch = pa.ipc.read_record_batch(response.arrow_data)
            batches.append(batch)
        
        return pa.Table.from_batches(batches)
    
    @classmethod
    def read_parquet(cls, path: str) -> 'DataFrame':
        stub = get_default_stub()
        request = polaroid_pb2.ReadParquetRequest(path=path)
        response = stub.ReadParquet(request)
        return cls(stub, response.handle)
    
    @classmethod
    def from_websocket(cls, url: str, schema: dict) -> 'StreamingDataFrame':
        """Create streaming dataframe from WebSocket"""
        stub = get_default_stub()
        request = polaroid_pb2.WebSocketSourceRequest(
            url=url,
            schema=schema
        )
        stream = stub.StreamWebSocket(request)
        return StreamingDataFrame(stub, stream)
```

### 4. **polaroid-sources** (Rust)

Network-native data sources:

```rust
// REST API with automatic pagination
pub struct RestApiLoader {
    client: reqwest::Client,
    rate_limiter: RateLimiter,
}

impl RestApiLoader {
    pub async fn load_paginated(
        &self,
        endpoint: &str,
        pagination: PaginationConfig,
    ) -> Result<impl Stream<Item = Result<RecordBatch>>> {
        // Handle different pagination strategies
        match pagination.strategy {
            PaginationStrategy::Offset { limit, offset_field } => {
                self.load_offset_pagination(endpoint, limit, offset_field).await
            }
            PaginationStrategy::Cursor { cursor_field } => {
                self.load_cursor_pagination(endpoint, cursor_field).await
            }
            PaginationStrategy::LinkHeader => {
                self.load_link_header_pagination(endpoint).await
            }
        }
    }
}

// WebSocket with reconnection and buffering
pub struct WebSocketLoader {
    url: String,
    reconnect_policy: ReconnectPolicy,
    buffer_size: usize,
}

impl WebSocketLoader {
    pub async fn connect(
        &self,
    ) -> Result<impl Stream<Item = Result<RecordBatch>>> {
        let (tx, rx) = mpsc::channel(self.buffer_size);
        
        // Spawn reconnecting websocket task
        tokio::spawn(async move {
            loop {
                match connect_with_retry(&self.url, &self.reconnect_policy).await {
                    Ok(ws) => {
                        // Process messages
                    }
                    Err(e) => {
                        // Backoff and retry
                    }
                }
            }
        });
        
        Ok(ReceiverStream::new(rx))
    }
}

// gRPC streaming source (for chaining Polaroid instances)
pub struct GrpcStreamLoader {
    client: DataFrameServiceClient<Channel>,
}
```

### 5. **polaroid-timeseries** (Rust)

Specialized time-series operations:

```rust
pub struct TimeSeriesOps;

impl TimeSeriesOps {
    // OHLCV resampling
    pub fn resample_ohlcv(
        df: &DataFrame,
        frequency: TimeSeriesFrequency,
    ) -> Result<DataFrame> {
        // Efficient resampling using Arrow's compute kernels
    }
    
    // Rolling windows with custom aggregations
    pub fn rolling_apply<F>(
        df: &DataFrame,
        window: Duration,
        func: F,
    ) -> Result<DataFrame>
    where
        F: Fn(&[f64]) -> f64 + Send + Sync,
    {
        // Parallel rolling window computation
    }
    
    // Time-series alignment (joins on nearest timestamp)
    pub fn asof_join(
        left: &DataFrame,
        right: &DataFrame,
        tolerance: Duration,
    ) -> Result<DataFrame> {
        // Efficient as-of join for time-series
    }
    
    // Cointegration and stationarity tests
    pub fn adf_test(series: &[f64]) -> Result<ADFResult> {
        // Augmented Dickey-Fuller test
    }
    
    pub fn johansen_test(
        series: Vec<&[f64]>,
    ) -> Result<JohansenResult> {
        // Johansen cointegration test
    }
}
```

## Protocol Buffer Definitions

### Core DataFrame Operations

```protobuf
syntax = "proto3";

package polaroid.v1;

import "arrow_flight.proto";

// Main DataFrame service
service DataFrameService {
    // File I/O
    rpc ReadParquet(ReadParquetRequest) returns (DataFrameHandle);
    rpc ReadCsv(ReadCsvRequest) returns (DataFrameHandle);
    rpc WriteParquet(WriteParquetRequest) returns (WriteResponse);
    
    // Network sources
    rpc ReadWebSocket(WebSocketSourceRequest) returns (stream ArrowBatch);
    rpc ReadRestApi(RestApiRequest) returns (stream ArrowBatch);
    rpc ReadGrpcStream(GrpcStreamRequest) returns (stream ArrowBatch);
    
    // DataFrame operations (all return new handles)
    rpc Filter(FilterRequest) returns (DataFrameHandle);
    rpc Select(SelectRequest) returns (DataFrameHandle);
    rpc GroupBy(GroupByRequest) returns (DataFrameHandle);
    rpc Join(JoinRequest) returns (DataFrameHandle);
    rpc Sort(SortRequest) returns (DataFrameHandle);
    rpc WithColumn(WithColumnRequest) returns (DataFrameHandle);
    
    // Time-series operations
    rpc Resample(ResampleRequest) returns (DataFrameHandle);
    rpc RollingWindow(RollingWindowRequest) returns (DataFrameHandle);
    rpc Lag(LagRequest) returns (DataFrameHandle);
    rpc AsofJoin(AsofJoinRequest) returns (DataFrameHandle);
    
    // Execution
    rpc Collect(CollectRequest) returns (stream ArrowBatch);
    rpc CollectLazy(CollectLazyRequest) returns (stream ArrowBatch);
    rpc Explain(ExplainRequest) returns (ExplainResponse);
    
    // Metadata
    rpc GetSchema(GetSchemaRequest) returns (SchemaResponse);
    rpc GetStats(GetStatsRequest) returns (StatsResponse);
    
    // Cleanup
    rpc DropHandle(DropHandleRequest) returns (DropHandleResponse);
}

message DataFrameHandle {
    string handle = 1;  // Unique identifier for server-side DataFrame
    string error = 2;
}

message ArrowBatch {
    bytes arrow_ipc = 1;  // Arrow IPC format
    string error = 2;
}

message ReadParquetRequest {
    string path = 1;
    repeated string columns = 2;  // Projection pushdown
    string predicate = 3;  // Filter pushdown (SQL-like)
}

message WebSocketSourceRequest {
    string url = 1;
    map<string, string> headers = 2;
    string schema_json = 3;  // Arrow schema as JSON
    ReconnectPolicy reconnect_policy = 4;
}

message ReconnectPolicy {
    uint32 max_retries = 1;
    uint32 initial_delay_ms = 2;
    uint32 max_delay_ms = 3;
    float backoff_multiplier = 4;
}

message RestApiRequest {
    string url = 1;
    map<string, string> headers = 2;
    string method = 3;  // GET, POST
    string body = 4;
    PaginationConfig pagination = 5;
    string schema_json = 6;
}

message PaginationConfig {
    oneof strategy {
        OffsetPagination offset = 1;
        CursorPagination cursor = 2;
        LinkHeaderPagination link_header = 3;
    }
}

message OffsetPagination {
    uint32 limit = 1;
    string offset_field = 2;
}

message CursorPagination {
    string cursor_field = 1;
    string next_cursor_field = 2;
}

message LinkHeaderPagination {
    string rel = 1;  // usually "next"
}

message FilterRequest {
    string handle = 1;
    string predicate = 2;  // SQL-like expression
}

message ResampleRequest {
    string handle = 1;
    string frequency = 2;  // "1m", "5m", "1h", "1d"
    repeated Aggregation aggregations = 3;
}

message Aggregation {
    string column = 1;
    string function = 2;  // "sum", "mean", "first", "last", "ohlc"
}

message CollectRequest {
    string handle = 1;
    bool streaming = 2;  // Use streaming execution
}

message ExplainRequest {
    string handle = 1;
    bool optimized = 2;  // Show optimized plan
}

message ExplainResponse {
    string logical_plan = 1;
    string physical_plan = 2;
    string optimizations_applied = 3;
}
```

## Implementation Roadmap

### Phase 1: Foundation (Weeks 1-4)
- [x] Fork Polars and rename to Polaroid
- [ ] Set up gRPC server structure
- [ ] Define core .proto files
- [ ] Implement basic DataFrame handle management
- [ ] Create Python client stub library
- [ ] Add Arrow Flight integration

### Phase 2: Core Operations (Weeks 5-8)
- [ ] Migrate filter, select, groupby to gRPC
- [ ] Implement monadic error handling patterns
- [ ] Add streaming execution engine
- [ ] Optimize Arrow IPC serialization
- [ ] Create benchmarks vs Polars PyO3

### Phase 3: Time-Series Features (Weeks 9-12)
- [ ] Implement TimeSeriesFrame type
- [ ] Add resample, rolling, lag operations
- [ ] Integrate financial math from rust-arblab
- [ ] Add OHLCV-specific operations
- [ ] Implement as-of joins

### Phase 4: Network Sources (Weeks 13-16)
- [ ] Implement WebSocket data source
- [ ] Add REST API loader with pagination
- [ ] Create gRPC streaming source
- [ ] Add connection pooling and retry logic
- [ ] Implement rate limiting

### Phase 5: Advanced Features (Weeks 17-20)
- [ ] Add distributed query execution
- [ ] Implement query caching layer
- [ ] Add SQL interface via DataFusion
- [ ] Create monitoring/observability
- [ ] Performance tuning and optimization

### Phase 6: Legacy Compatibility (Weeks 21-24)
- [ ] Create Polars compatibility layer
- [ ] Add PyO3 optional mode for migration
- [ ] Write migration guide
- [ ] Create benchmark suite
- [ ] Documentation and examples

## Performance Targets

| Operation | Polars (PyO3) | Polaroid (gRPC) | Target |
|-----------|---------------|-----------------|--------|
| Read 1GB Parquet | 200ms | 250ms | <300ms |
| Filter + Select (10M rows) | 50ms | 60ms | <80ms |
| gRPC overhead | N/A | 2-5ms | <10ms |
| Streaming throughput | N/A | 1M rows/sec | >500K rows/sec |
| WebSocket ingest | N/A | 100K msgs/sec | >50K msgs/sec |
| Memory overhead (handles) | N/A | <100MB | <200MB |

## Migration Guide (Polars → Polaroid)

### 1. Installation

```bash
# Start Polaroid gRPC server
docker run -p 50051:50051 polaroid/server:latest

# Or locally
cargo build --release -p polaroid-grpc
./target/release/polaroid-grpc

# Install Python client
pip install polaroid
```

### 2. Basic Usage

```python
# OLD (Polars)
import polars as pl
df = pl.read_parquet("data.parquet")
result = df.filter(pl.col("price") > 100).select(["symbol", "price"])

# NEW (Polaroid) - nearly identical API
import polaroid as pd
pd.connect("localhost:50051")  # Connect to gRPC server
df = pd.read_parquet("data.parquet")
result = df.filter(pd.col("price") > 100).select(["symbol", "price"])
```

### 3. Streaming

```python
# NEW: Streaming data sources
df = pd.from_websocket(
    url="wss://api.exchange.com/stream",
    schema={"symbol": pd.Utf8, "price": pd.Float64, "timestamp": pd.Datetime}
)

# Process stream in batches
for batch in df.stream_batches(batch_size=1000):
    processed = batch.filter(pd.col("price") > 100)
    processed.to_parquet("output.parquet", mode="append")
```

### 4. Time-Series

```python
# NEW: Time-series operations
ts_df = pd.read_parquet("ticks.parquet").as_timeseries(
    timestamp_col="ts",
    frequency="tick"
)

# Resample to 5-minute OHLCV
ohlcv = ts_df.resample_ohlcv("5m")

# Rolling operations
sma = ts_df.with_column("sma_20", ts_df["close"].rolling(20).mean())
```

## Testing Strategy

### 1. Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_filter_returns_result() {
        let df = create_test_df();
        let result = df.filter(col("price").gt(100));
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_websocket_stream() {
        let source = WebSocketSource::new("ws://localhost:8080");
        let stream = source.stream();
        let batches: Vec<_> = stream.take(10).collect().await;
        assert_eq!(batches.len(), 10);
    }
}
```

### 2. Integration Tests

```python
import polaroid as pd
import pytest

def test_grpc_roundtrip():
    df = pd.DataFrame({"a": [1, 2, 3], "b": [4, 5, 6]})
    result = df.filter(pd.col("a") > 1).collect()
    assert len(result) == 2

def test_streaming_execution():
    df = pd.read_parquet("large_file.parquet", streaming=True)
    count = 0
    for batch in df.stream_batches():
        count += len(batch)
    assert count > 0
```

### 3. Benchmarks

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_filter(c: &mut Criterion) {
    let df = create_large_df(10_000_000);
    
    c.bench_function("filter_10m_rows", |b| {
        b.iter(|| {
            let result = df.filter(col("price").gt(100));
            black_box(result)
        });
    });
}

criterion_group!(benches, benchmark_filter);
criterion_main!(benches);
```

## Key Differences Summary

| Feature | Polars | Polaroid |
|---------|--------|----------|
| **Python Interface** | PyO3 (FFI) | gRPC (network) |
| **Error Handling** | Panics/Exceptions | Result monads |
| **Streaming** | Limited | First-class |
| **Time-Series** | Basic | Native support |
| **Network Sources** | None | WebSocket, REST, gRPC |
| **Distribution** | Single process | Multi-node capable |
| **Type Safety** | Runtime | Compile-time proto |
| **Pandas Coupling** | High | Minimal |
| **Functional Style** | Partial | Full monadic |

## Next Steps

1. Review this architecture document
2. Validate gRPC approach vs PyO3 for your use case
3. Start with Phase 1: set up gRPC server skeleton
4. Create minimal .proto and test roundtrip
5. Benchmark gRPC overhead vs PyO3

Would you like me to proceed with implementing the Phase 1 components?
