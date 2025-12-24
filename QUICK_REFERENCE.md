# Polaroid Quick Reference

## Architecture at a Glance

```
┌─────────────────────────────────────────────────┐
│   Python Client (polaroid-python)              │
│   - Thin gRPC client                            │
│   - Handle management                           │
│   - Arrow IPC deserialization                   │
└────────────────────┬────────────────────────────┘
                     │ gRPC (Arrow Flight)
┌────────────────────▼────────────────────────────┐
│   Polaroid gRPC Server (Rust)                  │
│   - Request routing                             │
│   - Authentication                              │
│   - Handle lifecycle management                 │
└────────────────────┬────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────┐
│   Functional Core (Rust)                       │
│   - Immutable DataFrame operations              │
│   - Monadic error handling (Result<T, E>)      │
│   - Streaming execution engine                  │
│   - DataFusion integration                      │
└─────────────────────────────────────────────────┘
```

## Key Concepts

### 1. Handles (Server-Side References)

```python
df = pd.read_parquet("data.parquet")  # Returns handle "df_abc123"
df2 = df.filter(pd.col("price") > 100)  # Returns handle "df_def456"
# No data copied between operations
```

### 2. Immutability

```python
df1 = pd.read_parquet("data.parquet")
df2 = df1.filter(pd.col("price") > 100)  # df1 unchanged
df3 = df1.select(["symbol"])             # df1 still unchanged
```

### 3. Lazy Execution

```python
# Operations are lazy until collect()
df = pd.read_parquet("data.parquet")
df = df.filter(pd.col("price") > 100)  # Not executed yet
df = df.select(["symbol", "price"])    # Not executed yet
result = df.collect()                   # Execute all operations
```

### 4. Streaming

```python
# Stream large datasets in batches
for batch in df.stream_batches(batch_size=10000):
    process(batch)  # Each batch is a pyarrow.Table
```

## Common Operations

### Read/Write

```python
# Parquet
df = pd.read_parquet("data.parquet", columns=["col1", "col2"])
df.write_parquet("output.parquet", compression="zstd")

# CSV
df = pd.read_csv("data.csv", has_header=True)
df.write_csv("output.csv")

# Arrow
import pyarrow as pa
table = pa.Table.from_pandas(pandas_df)
df = pd.from_arrow(table)
```

### Filtering

```python
# Simple filter
df = df.filter(pd.col("price") > 100)

# Multiple conditions
df = df.filter(
    (pd.col("price") > 100) & (pd.col("volume") > 1000)
)

# Using expressions
df = df.filter(pd.col("symbol").is_in(["AAPL", "MSFT"]))
```

### Selection

```python
# Select columns
df = df.select(["symbol", "price"])

# Drop columns
df = df.drop(["unwanted_col"])

# Rename
df = df.rename({"old_name": "new_name"})
```

### Aggregations

```python
# Group by
result = df.group_by("symbol").agg({
    "price": ["mean", "max", "min"],
    "volume": "sum"
})

# Without grouping
result = df.agg({"price": "mean", "volume": "sum"})
```

### Joins

```python
# Inner join
result = df1.join(df2, on="symbol", how="inner")

# Left join with different column names
result = df1.join(df2, left_on="symbol", right_on="ticker", how="left")

# As-of join (time-series)
result = df1.asof_join(df2, on="timestamp", by="symbol")
```

### Sorting

```python
# Sort ascending
df = df.sort("price")

# Sort descending
df = df.sort("price", descending=True)

# Multiple columns
df = df.sort(["symbol", "timestamp"])
```

## Time-Series Operations

### Convert to Time-Series

```python
ts_df = df.as_timeseries(
    timestamp_col="timestamp",
    frequency="1m"  # Auto-detect if not specified
)
```

### Resample

```python
# Resample to 5-minute OHLCV
ohlcv = ts_df.resample_ohlcv("5m")

# Custom aggregations
resampled = ts_df.resample("1h", {
    "price": "mean",
    "volume": "sum"
})
```

### Rolling Windows

```python
# Simple moving average
sma = ts_df.rolling("20m").agg({"close": "mean"})

# Multiple aggregations
rolling = ts_df.rolling("1h").agg({
    "close": ["mean", "std"],
    "volume": "sum"
})
```

### Lag/Lead

```python
# Add lagged columns
df = ts_df.lag({"close": 1, "volume": 1})

# Add leading columns
df = ts_df.lead({"close": 1})
```

### Returns

```python
# Simple returns
returns = ts_df.pct_change(periods=1)

# Log returns
log_returns = ts_df.diff(periods=1).apply(lambda x: np.log(x))
```

## Network Data Sources

### WebSocket

```python
df = pd.from_websocket(
    url="wss://api.exchange.com/stream",
    schema={
        "symbol": pd.Utf8,
        "price": pd.Float64,
        "timestamp": pd.Datetime("ns")
    },
    format="json"
)

# Process stream
for batch in df.stream_batches():
    process(batch)
```

### REST API

```python
df = pd.from_rest_api(
    url="https://api.example.com/data",
    headers={"Authorization": "Bearer TOKEN"},
    pagination={"strategy": "cursor", "cursor_field": "next"},
    schema=schema
)

# Load all data
result = df.collect()
```

### Message Queue

```python
df = pd.from_message_queue(
    type="kafka",
    connection_string="localhost:9092",
    topic="market-data",
    schema=schema
)

# Consume messages
for batch in df.stream_batches():
    process(batch)
```

## Error Handling

```python
try:
    df = pd.read_parquet("data.parquet")
    result = df.filter(pd.col("price") > 100).collect()
except pd.ColumnNotFoundError as e:
    print(f"Column not found: {e}")
except pd.NetworkError as e:
    print(f"Network error: {e}")
except pd.PolaroidError as e:
    print(f"General error: {e}")
```

## Configuration

```python
# Connect to server
pd.connect("localhost:50051")

# Configure
pd.config.set("timeout", 30)  # 30 seconds
pd.config.set("pool_size", 10)
pd.config.set("log_level", "INFO")
pd.config.set("max_memory", "8GB")

# From config file
pd.load_config("~/.polaroid/config.yaml")
```

## Performance Tips

### ✅ DO

```python
# Push filters and projections to source
df = pd.read_parquet(
    "data.parquet",
    columns=["symbol", "price"],
    predicate="price > 100"
)

# Stream large datasets
for batch in df.stream_batches():
    process(batch)

# Reuse connections
pd.connect("localhost:50051", pool_size=10)

# Explain queries
plan = df.explain(optimized=True)
```

### ❌ DON'T

```python
# Load all data then filter
df = pd.read_parquet("data.parquet").collect()
df = df.filter(pd.col("price") > 100)  # Too late!

# Forget to collect
print(df)  # Prints handle, not data

# Create connections repeatedly
for i in range(100):
    with pd.connection("localhost:50051") as conn:
        df = pd.read_parquet(f"data_{i}.parquet")
```

## Cheat Sheet

| Task | Command |
|------|---------|
| **Read Parquet** | `pd.read_parquet("file.parquet")` |
| **Filter rows** | `df.filter(pd.col("x") > 10)` |
| **Select columns** | `df.select(["a", "b"])` |
| **Group by** | `df.group_by("col").agg({"val": "sum"})` |
| **Join** | `df1.join(df2, on="key")` |
| **Sort** | `df.sort("col", descending=True)` |
| **Limit** | `df.limit(100)` |
| **Collect** | `df.collect()` |
| **Stream** | `df.stream_batches(batch_size=1000)` |
| **Time-series** | `df.as_timeseries("timestamp")` |
| **Resample** | `ts_df.resample_ohlcv("5m")` |
| **WebSocket** | `pd.from_websocket(url, schema)` |
| **REST API** | `pd.from_rest_api(url, pagination=...)` |
| **Explain** | `df.explain(optimized=True)` |

## Expression API

```python
# Column reference
pd.col("price")

# Literals
pd.lit(100)
pd.lit("AAPL")

# Comparisons
pd.col("price") > 100
pd.col("symbol") == "AAPL"
pd.col("price").between(100, 200)

# Logical
(pd.col("price") > 100) & (pd.col("volume") > 1000)
(pd.col("symbol") == "AAPL") | (pd.col("symbol") == "MSFT")
~pd.col("flag")

# Null handling
pd.col("price").is_null()
pd.col("price").is_not_null()
pd.col("price").fill_null(0)

# String operations
pd.col("symbol").str.upper()
pd.col("symbol").str.contains("AA")
pd.col("symbol").str.slice(0, 2)

# Math
pd.col("price") + pd.col("fee")
pd.col("price") * 1.1
pd.col("price").abs()
pd.col("price").sqrt()

# Aggregations
pd.col("price").sum()
pd.col("price").mean()
pd.col("price").max()
pd.col("price").min()
pd.col("price").std()
pd.col("price").count()

# Window operations
pd.col("price").rolling(20).mean()
pd.col("price").lag(1)
pd.col("price").lead(1)
```

## Monitoring

```python
# Health check
pd.health_check("localhost:50051")

# Handle status
df.is_alive()
df.heartbeat()

# Query profiling
result = df.collect()
profile = result.profile()
print(f"Execution time: {profile.execution_time}")
print(f"Rows scanned: {profile.rows_scanned}")
print(f"Memory used: {profile.memory_used}")

# Server stats
stats = pd.server_stats()
print(f"Active handles: {stats.active_handles}")
print(f"Total queries: {stats.total_queries}")
print(f"Uptime: {stats.uptime}")
```

## Comparison: Polars vs Polaroid

| Feature | Polars | Polaroid |
|---------|--------|----------|
| Interface | PyO3 (FFI) | gRPC |
| Execution | In-process | Client-server |
| Latency | ~1ms | ~5-10ms |
| Memory | Shared | Separate |
| Streaming | Limited | Full |
| Network | ❌ | ✅ |
| Time-series | Basic | Native |
| Multi-node | ❌ | ✅ |
| Languages | Python/Rust | Any |

## Links

- **Docs**: https://docs.polaroid.rs
- **GitHub**: https://github.com/polaroid-rs/polaroid
- **Discord**: https://discord.gg/polaroid
- **PyPI**: https://pypi.org/project/polaroid/
