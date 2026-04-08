# Getting Started with Polarway

![Polarway Logo](../assets/logo.svg)

Welcome to **Polarway** - a revolutionary data engineering platform that brings Railway-Oriented Programming principles to data processing with hybrid storage architecture.

## What is Polarway?

**Polarway** combines the best of:
- 🚆 **Railway-Oriented Programming**: Explicit error handling with `Result<T, E>` types
- 🚀 **High Performance**: Built on Polars and Rust for maximum speed
- 💾 **Hybrid Storage**: Parquet + DuckDB + LRU Cache for optimal cost/performance
- 🌐 **Distributed**: gRPC server-client architecture for remote execution
- 📊 **Streaming**: Handle 100GB+ datasets with constant memory footprint

## Quick Start

### Installation

#### Python Client

```bash
pip install polarway
```

#### Rust

Add to your `Cargo.toml`:

```toml
[dependencies]
polarway = "0.53.0"
```

## Your First Polarway Program

### Python Example

```python
import polarway as pw

# Railway-oriented approach: explicit error handling
result = (
    pw.read_csv("data.csv")
    .and_then(lambda df: df.filter(pw.col("price") > 100))
    .and_then(lambda df: df.group_by("symbol").agg({"price": "mean"}))
    .map_err(lambda e: print(f"Error: {e}"))
)

match result:
    case pw.Ok(data):
        print(f"✅ Success: {data}")
    case pw.Err(error):
        print(f"❌ Failure: {error}")
```

### Rust Example

```rust
use polarway::prelude::*;

fn main() -> Result<()> {
    // Functional pipeline with error handling
    let result = read_csv("data.csv")?
        .filter(col("price").gt(100))?
        .group_by(&["symbol"])?
        .agg(&[col("price").mean()])?
        .collect()?;
    
    println!("Success: {:?}", result);
    Ok(())
}
```

## Key Concepts

### 1. Railway-Oriented Error Handling

Traditional approach (❌ hides errors):
```python
try:
    df = load_data()
    result = process(df)
except Exception as e:
    print(f"Something broke: {e}")  # Where? When? Why?
```

Polarway approach (✅ explicit):
```python
result = (
    pw.load_data()
    .and_then(process)
    .map_err(log_error)
)
# Clear success/failure paths!
```

### 2. Hybrid Storage Architecture

Polarway uses a three-tier storage system:

```
Request → LRU Cache (RAM, <1ms) 
       ↓ (miss)
       → Parquet (Disk, ~50ms, 18× compression)
       ↓
       → DuckDB (SQL analytics, ~45ms)
```

**Benefits:**
- 💰 **-20% cost** vs traditional time-series databases
- 📦 **18× compression** (zstd level 19)
- ⚡ **85%+ cache hit rate** for hot data
- 🔍 **Full SQL support** via DuckDB

### 3. Functional Composition

Build complex pipelines by composing simple operations:

```python
pipeline = (
    pw.read_parquet("data/*.parquet")
    .and_then(lambda df: df.with_columns([
        (pw.col("price") * pw.col("quantity")).alias("value")
    ]))
    .and_then(lambda df: df.filter(pw.col("value") > 1000))
    .and_then(lambda df: df.sort("timestamp"))
    .map(lambda df: df.head(100))
)
```

Each operation is **composable**, **type-safe**, and **error-aware**.

## Storage Modes

Polarway supports three storage modes:

### Standalone Mode (Default)
```python
from polarway import StorageClient

client = StorageClient(
    parquet_path="/data/cold",
    enable_cache=True,
    cache_size_gb=2.0
)

# Local operations
df = client.load("trades_20260203")
```

### Distributed Mode (gRPC)
```python
from polarway import DistributedClient

client = DistributedClient(
    host="polarway-server.example.com",
    port=50052
)

# Remote execution
df = client.load("trades_20260203")
```

### Embedded Mode (In-Process)
```rust
use polarway::HybridStorage;

let storage = HybridStorage::new(
    "/data/cold",
    ":memory:",
    2.0
)?;

let data = storage.smart_load("key")?;
```

## Real-World Example: Time-Series Analytics

```python
import polarway as pw
from datetime import datetime, timedelta

# Connect to Polarway storage
client = pw.StorageClient(
    parquet_path="/data/market",
    enable_cache=True
)

# Load and process trading data
result = (
    client.load_time_range(
        symbol="BTC-USD",
        start=datetime.now() - timedelta(days=7),
        end=datetime.now()
    )
    .and_then(lambda df: df.with_columns([
        pw.col("returns").rolling_mean(window=20).alias("sma_20"),
        pw.col("returns").rolling_std(window=20).alias("vol_20")
    ]))
    .and_then(lambda df: df.filter(
        pw.col("vol_20") > pw.col("vol_20").quantile(0.95)
    ))
    .map(lambda df: df.select(["timestamp", "price", "sma_20", "vol_20"]))
)

match result:
    case pw.Ok(high_vol_periods):
        print(f"Found {len(high_vol_periods)} high-volatility periods")
        high_vol_periods.write_csv("high_vol_analysis.csv")
    case pw.Err(e):
        print(f"Analysis failed: {e}")
```

## Performance Characteristics

| Operation | Polarway | Traditional TSDB | Improvement |
|-----------|----------|------------------|-------------|
| Cache hit (hot data) | <1ms | ~10ms | **10×** faster |
| Cold data load | ~50ms | ~200ms | **4×** faster |
| Compression | 18:1 | 1.07:1 | **17×** better |
| Monthly cost (100GB) | 24 CHF | 30 CHF | **-20%** |
| Memory usage (streaming) | Constant | O(n) | **Unlimited scale** |

## Next Steps

- 📚 **[Core Concepts](concepts.md)** - Deep dive into Railway-Oriented Programming
- 🔧 **[Installation Guide](installation.md)** - Detailed setup for all platforms
- 🐍 **[Python Client](python-client.md)** - Complete Python API reference
- 🦀 **[Rust Client](rust-client.md)** - Complete Rust API reference
- 🌐 **[Distributed Mode](distributed-mode.md)** - gRPC server deployment
- 💡 **[Examples](examples.md)** - Real-world use cases

## Community & Support

- 📖 **Documentation**: https://polarway.readthedocs.io/
- 💬 **Discussions**: https://github.com/ThotDjehuty/polarway/discussions
- 🐛 **Issues**: https://github.com/ThotDjehuty/polarway/issues
- 📧 **Email**: support@polarway.dev

---

**Built with ❤️ by the Polarway team**
