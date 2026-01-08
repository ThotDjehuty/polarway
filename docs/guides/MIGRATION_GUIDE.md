# Polaroid Migration Guide: From Polars to Polaroid

## Overview

This guide helps you migrate from **Polars** (PyO3-based) to **Polaroid** (gRPC-based). Polaroid maintains API compatibility where possible while adding powerful new features for time-series, streaming, and network data sources.

## Quick Start

### 1. Installation

```bash
# Start Polaroid gRPC server
docker run -d -p 50051:50051 polaroid/server:latest

# Or build from source
cd polaroid
cargo build --release -p polaroid-grpc
./target/release/polaroid-grpc

# Install Python client
pip install polaroid
```

### 2. Basic Migration

```python
# OLD (Polars)
import polars as pl

df = pl.read_parquet("data.parquet")
result = df.filter(pl.col("price") > 100).select(["symbol", "price"])
result.write_parquet("output.parquet")

# NEW (Polaroid) - nearly identical!
import polaroid as pd

pd.connect("localhost:50051")  # Connect to gRPC server
df = pd.read_parquet("data.parquet")
result = df.filter(pd.col("price") > 100).select(["symbol", "price"])
result.write_parquet("output.parquet")
```

## Key Differences

### Architecture Comparison

| Aspect | Polars | Polaroid |
|--------|--------|----------|
| **Interface** | PyO3 (FFI) | gRPC (network) |
| **Execution** | In-process | Client-server |
| **Streaming** | Limited | First-class |
| **Network Sources** | None | Built-in |
| **Time-Series** | Basic | Native support |
| **Error Handling** | Exceptions | Result types |
| **Multi-language** | Python/Rust | Any language |
| **Distribution** | Single node | Multi-node ready |

### Connection Management

```python
# Polaroid requires explicit connection
import polaroid as pd

# Method 1: Default connection
pd.connect("localhost:50051")

# Method 2: Named connections
pd.connect("localhost:50051", name="primary")
pd.connect("remote-server:50051", name="backup")

# Method 3: Context manager (recommended)
with pd.connection("localhost:50051") as conn:
    df = pd.read_parquet("data.parquet")
    result = df.filter(pd.col("price") > 100).collect()
# Connection automatically closed

# Method 4: Configuration file
pd.load_config("~/.polaroid/config.yaml")
```

### Handle-Based Architecture

Polaroid uses server-side handles to avoid copying data:

```python
# Each operation returns a new handle (immutable)
df1 = pd.read_parquet("data.parquet")  # Handle: df1_abc123
df2 = df1.filter(pd.col("price") > 100)  # Handle: df2_def456
df3 = df2.select(["symbol", "price"])    # Handle: df3_ghi789

# Handles are automatically cleaned up
# Or explicitly drop:
df1.drop_handle()

# Check if handle is still valid
df1.is_alive()  # False (dropped)
df2.is_alive()  # True

# Keep handles alive longer (extend TTL)
df2.heartbeat()
```

## API Compatibility Matrix

### Core Operations

| Operation | Polars | Polaroid | Notes |
|-----------|--------|----------|-------|
| `read_parquet` | ✅ | ✅ | Same API |
| `read_csv` | ✅ | ✅ | Same API |
| `filter` | ✅ | ✅ | Same API |
| `select` | ✅ | ✅ | Same API |
| `group_by` | ✅ | ✅ | Same API |
| `join` | ✅ | ✅ | Same API |
| `sort` | ✅ | ✅ | Same API |
| `with_column` | ✅ | ✅ | Same API |
| `collect` | ✅ | ✅ | Returns Arrow Table |
| `lazy` | ✅ | ✅ | Automatic in Polaroid |

### Extended Operations (Polaroid-Specific)

| Operation | Description | Example |
|-----------|-------------|---------|
| `as_timeseries` | Convert to time-series | `df.as_timeseries("timestamp", "1m")` |
| `resample_ohlcv` | Resample to OHLCV | `df.resample_ohlcv("5m")` |
| `from_websocket` | Stream from WebSocket | `pd.from_websocket("wss://...")` |
| `from_rest_api` | Load from REST API | `pd.from_rest_api("https://...", pagination="cursor")` |
| `stream_batches` | Iterate in batches | `for batch in df.stream_batches():` |
| `explain` | Show query plan | `df.explain(optimized=True)` |

## Migration Patterns

### Pattern 1: Simple DataFrame Operations

```python
### BEFORE (Polars) ###
import polars as pl

df = pl.read_csv("data.csv")
result = (
    df
    .filter(pl.col("age") > 25)
    .select(["name", "age", "city"])
    .sort("age", descending=True)
    .limit(10)
)
print(result)

### AFTER (Polaroid) ###
import polaroid as pd

pd.connect("localhost:50051")
df = pd.read_csv("data.csv")
result = (
    df
    .filter(pd.col("age") > 25)
    .select(["name", "age", "city"])
    .sort("age", descending=True)
    .limit(10)
)
print(result.collect())  # Note: .collect() returns pyarrow.Table
```

### Pattern 2: Lazy Evaluation

```python
### BEFORE (Polars) ###
lazy_df = pl.scan_parquet("large_data.parquet")
result = (
    lazy_df
    .filter(pl.col("value") > 1000)
    .group_by("category")
    .agg([pl.col("value").sum()])
    .collect()  # Execute here
)

### AFTER (Polaroid) ###
# All operations are lazy by default!
df = pd.read_parquet("large_data.parquet")
result = (
    df
    .filter(pd.col("value") > 1000)
    .group_by("category")
    .agg({"value": "sum"})
    .collect()  # Execute here
)

# Or explicitly check plan
plan = df.explain(optimized=True)
print(plan)
```

### Pattern 3: Aggregations

```python
### BEFORE (Polars) ###
result = df.group_by("symbol").agg([
    pl.col("price").mean().alias("avg_price"),
    pl.col("volume").sum().alias("total_volume"),
    pl.col("price").max().alias("max_price"),
])

### AFTER (Polaroid) ###
# Method 1: Dict-based (recommended)
result = df.group_by("symbol").agg({
    "price": ["mean", "max"],
    "volume": "sum"
})

# Method 2: Expression-based (Polars-like)
result = df.group_by("symbol").agg([
    pd.col("price").mean().alias("avg_price"),
    pd.col("volume").sum().alias("total_volume"),
    pd.col("price").max().alias("max_price"),
])
```

### Pattern 4: Joins

```python
### BEFORE (Polars) ###
result = df1.join(df2, on="symbol", how="left")

### AFTER (Polaroid) ###
# Same API!
result = df1.join(df2, on="symbol", how="left")

# Multiple keys
result = df1.join(df2, on=["symbol", "date"], how="inner")

# Different column names
result = df1.join(df2, left_on="symbol", right_on="ticker", how="left")
```

### Pattern 5: Time-Series (New in Polaroid)

```python
### BEFORE (Polars) - manual implementation ###
import polars as pl
from datetime import datetime, timedelta

# Resample manually
df = pl.read_parquet("ticks.parquet")
df = df.with_column(
    pl.col("timestamp").dt.truncate("5m").alias("bucket")
)
ohlcv = df.group_by("bucket").agg([
    pl.col("price").first().alias("open"),
    pl.col("price").max().alias("high"),
    pl.col("price").min().alias("low"),
    pl.col("price").last().alias("close"),
    pl.col("volume").sum().alias("volume"),
])

### AFTER (Polaroid) - native support ###
import polaroid as pd

df = pd.read_parquet("ticks.parquet")
ts_df = df.as_timeseries("timestamp", frequency="tick")
ohlcv = ts_df.resample_ohlcv("5m")  # One line!

# Additional time-series operations
sma_20 = ohlcv.rolling("20m").agg({"close": "mean"})
returns = ohlcv.pct_change(periods=1)
lagged = ohlcv.lag({"close": 1, "volume": 1})
```

### Pattern 6: Streaming Large Files

```python
### BEFORE (Polars) ###
# Limited streaming support
df = pl.scan_parquet("huge_file.parquet")
result = df.filter(pl.col("value") > 100).collect(streaming=True)

### AFTER (Polaroid) ###
# Full streaming support with batching
df = pd.read_parquet("huge_file.parquet", streaming=True)

# Method 1: Stream batches
for batch in df.filter(pd.col("value") > 100).stream_batches(batch_size=10000):
    process_batch(batch)  # batch is pyarrow.Table

# Method 2: Collect with memory limit
result = df.filter(pd.col("value") > 100).collect_streaming(
    batch_size=10000,
    max_memory="8GB"
)
```

### Pattern 7: Network Data Sources (New in Polaroid)

```python
### WebSocket Streaming ###
import polaroid as pd

# Define schema
schema = {
    "symbol": pd.Utf8,
    "price": pd.Float64,
    "timestamp": pd.Datetime("ns"),
}

# Connect to WebSocket
df = pd.from_websocket(
    url="wss://api.exchange.com/stream",
    schema=schema,
    format="json",
    reconnect_policy={
        "max_retries": 10,
        "initial_delay_ms": 1000,
        "backoff_multiplier": 2.0
    }
)

# Process stream
for batch in df.stream_batches(batch_size=1000):
    processed = batch.filter(pd.col("price") > 100)
    processed.to_parquet("output.parquet", mode="append")

### REST API with Pagination ###
df = pd.from_rest_api(
    url="https://api.example.com/data",
    headers={"Authorization": "Bearer TOKEN"},
    pagination={
        "strategy": "cursor",
        "cursor_field": "next_cursor",
        "cursor_param": "cursor"
    },
    schema=schema,
    rate_limit={"requests_per_second": 10}
)

# Load all pages
result = df.collect()

### Kafka/Message Queue ###
df = pd.from_message_queue(
    type="kafka",
    connection_string="localhost:9092",
    topic="market-data",
    consumer_group="polaroid-consumer",
    schema=schema,
    format="json"
)

# Real-time processing
for batch in df.stream_batches():
    # Process and forward
    pass
```

## Error Handling

### Polars (Exceptions)

```python
import polars as pl

try:
    df = pl.read_parquet("data.parquet")
    result = df.filter(pl.col("missing_column") > 100)
except pl.ColumnNotFoundError as e:
    print(f"Column error: {e}")
except Exception as e:
    print(f"Unknown error: {e}")
```

### Polaroid (Results)

```python
import polaroid as pd

# Method 1: Exception-based (default for compatibility)
try:
    df = pd.read_parquet("data.parquet")
    result = df.filter(pd.col("missing_column") > 100).collect()
except pd.ColumnNotFoundError as e:
    print(f"Column error: {e}")
    print(f"Available columns: {e.available_columns}")
except pd.PolaroidError as e:
    print(f"Polaroid error: {e}")
    print(f"Error context: {e.context}")

# Method 2: Result-based (functional style)
result = (
    pd.read_parquet("data.parquet")
    .and_then(lambda df: df.filter(pd.col("missing_column") > 100))
    .and_then(lambda df: df.collect())
)

if result.is_ok():
    df = result.unwrap()
else:
    error = result.unwrap_err()
    print(f"Error: {error}")
```

## Performance Optimization

### 1. Predicate Pushdown

```python
# GOOD: Filter pushdown to Parquet
df = pd.read_parquet(
    "data.parquet",
    predicate="price > 100 AND symbol = 'AAPL'"  # Executed during scan
)

# LESS EFFICIENT: Filter after loading
df = pd.read_parquet("data.parquet")
df = df.filter((pd.col("price") > 100) & (pd.col("symbol") == "AAPL"))
```

### 2. Projection Pushdown

```python
# GOOD: Only read needed columns
df = pd.read_parquet("data.parquet", columns=["symbol", "price", "timestamp"])

# LESS EFFICIENT: Read all then select
df = pd.read_parquet("data.parquet")
df = df.select(["symbol", "price", "timestamp"])
```

### 3. Streaming for Large Data

```python
# GOOD: Stream processing
for batch in pd.read_parquet("huge.parquet", streaming=True).stream_batches():
    process_batch(batch)

# BAD: Load everything into memory
df = pd.read_parquet("huge.parquet").collect()  # OOM!
```

### 4. Connection Pooling

```python
# GOOD: Reuse connections
pd.connect("localhost:50051", pool_size=10)

# LESS EFFICIENT: Create connection per request
for i in range(100):
    with pd.connection("localhost:50051") as conn:
        df = pd.read_parquet(f"data_{i}.parquet")
```

## Common Pitfalls

### Pitfall 1: Forgetting to Collect

```python
# WRONG: df is a handle, not data
df = pd.read_parquet("data.parquet")
print(df)  # Prints: <DataFrame handle: abc123>

# CORRECT: Call collect() to get data
df = pd.read_parquet("data.parquet").collect()
print(df)  # Prints: pyarrow.Table
```

### Pitfall 2: Handle Expiration

```python
# WRONG: Handle may expire after TTL
df = pd.read_parquet("data.parquet")
time.sleep(3600)  # Wait 1 hour
result = df.filter(pd.col("price") > 100).collect()  # Error: handle expired!

# CORRECT: Keep handle alive or collect early
df = pd.read_parquet("data.parquet")
df.heartbeat()  # Extend TTL
# Or
result = pd.read_parquet("data.parquet").filter(pd.col("price") > 100).collect()
```

### Pitfall 3: Mixing Polars and Polaroid

```python
# WRONG: Can't mix Polars and Polaroid objects
import polars as pl
import polaroid as pd

df_polars = pl.read_parquet("data.parquet")
df_polaroid = pd.read_parquet("data2.parquet")
result = df_polaroid.join(df_polars, on="symbol")  # Error!

# CORRECT: Convert Polars to Polaroid
df_polars = pl.read_parquet("data.parquet")
df_polaroid_1 = pd.from_arrow(df_polars.to_arrow())
df_polaroid_2 = pd.read_parquet("data2.parquet")
result = df_polaroid_2.join(df_polaroid_1, on="symbol")
```

## Testing Your Migration

### Unit Test Template

```python
import unittest
import polaroid as pd
import pyarrow as pa

class TestPolaroidMigration(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        pd.connect("localhost:50051")
    
    def test_read_write_parquet(self):
        # Create test data
        df = pd.DataFrame({
            "a": [1, 2, 3],
            "b": [4, 5, 6]
        })
        
        # Write
        df.write_parquet("test.parquet")
        
        # Read
        df2 = pd.read_parquet("test.parquet").collect()
        
        # Verify
        self.assertEqual(len(df2), 3)
        self.assertEqual(df2.column_names, ["a", "b"])
    
    def test_filter_select(self):
        df = pd.DataFrame({
            "symbol": ["AAPL", "MSFT", "GOOGL"],
            "price": [150, 250, 100]
        })
        
        result = df.filter(pd.col("price") > 100).select(["symbol"]).collect()
        
        self.assertEqual(len(result), 2)
        self.assertIn("AAPL", result["symbol"].to_pylist())
        self.assertIn("MSFT", result["symbol"].to_pylist())

if __name__ == "__main__":
    unittest.main()
```

## Compatibility Layer (Optional)

For gradual migration, use the compatibility layer:

```python
# Install compatibility layer
pip install polaroid[compat]

# Use Polars API with Polaroid backend
import polaroid.compat as pl  # Drop-in replacement

# Existing Polars code works unchanged
df = pl.read_parquet("data.parquet")
result = df.filter(pl.col("price") > 100).select(["symbol", "price"])
result.write_parquet("output.parquet")

# Internally uses Polaroid gRPC backend
```

## Troubleshooting

### Connection Issues

```python
# Check server status
pd.health_check("localhost:50051")

# Test connection with timeout
try:
    pd.connect("localhost:50051", timeout=5)
except pd.ConnectionError as e:
    print(f"Cannot connect: {e}")
```

### Performance Issues

```python
# Enable profiling
pd.config.set("profiling", True)

df = pd.read_parquet("data.parquet")
result = df.filter(pd.col("price") > 100).collect()

# Get profiling info
profile = result.profile()
print(f"Execution time: {profile.execution_time}")
print(f"Network time: {profile.network_time}")
print(f"Rows scanned: {profile.rows_scanned}")
```

### Debug Mode

```python
# Enable debug logging
pd.config.set("log_level", "DEBUG")

# Enable query plan logging
pd.config.set("explain_queries", True)

df = pd.read_parquet("data.parquet")
result = df.filter(pd.col("price") > 100).collect()
# Prints optimized query plan
```

## Feature Comparison Table

| Feature | Polars | Polaroid | Migration Effort |
|---------|--------|----------|------------------|
| Basic operations | ✅ | ✅ | Low - API compatible |
| Lazy evaluation | ✅ | ✅ | Low - automatic |
| Streaming | Partial | ✅ | Medium - new API |
| Time-series | Basic | ✅ Native | Medium - new features |
| Network sources | ❌ | ✅ | High - new capability |
| Distributed | ❌ | ✅ | High - new architecture |
| SQL interface | ✅ | ✅ | Low - compatible |
| Python integration | ✅ PyO3 | ✅ gRPC | Medium - connection setup |

## Migration Checklist

- [ ] Set up Polaroid gRPC server
- [ ] Install Polaroid Python client
- [ ] Update import statements (`polars` → `polaroid`)
- [ ] Add connection initialization
- [ ] Update `.collect()` calls for Arrow tables
- [ ] Test all DataFrame operations
- [ ] Migrate time-series code to native operations
- [ ] Add error handling for network issues
- [ ] Update unit tests
- [ ] Performance testing
- [ ] Update documentation

## Getting Help

- **Documentation**: https://docs.polaroid.rs
- **Discord**: https://discord.gg/polaroid
- **GitHub Issues**: https://github.com/polaroid-rs/polaroid/issues
- **Migration Support**: migration@polaroid.rs

## Next Steps

1. Start with Phase 1: Basic operations migration
2. Test thoroughly with your data
3. Gradually adopt new features (streaming, time-series)
4. Optimize performance
5. Deploy to production

**Welcome to Polaroid!** 🎉
