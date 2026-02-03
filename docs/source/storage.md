# Storage Layer Architecture

## Overview

Polarway v0.53.0 introduces a **hybrid storage layer** that combines three backends for optimal performance and cost efficiency:

1. **Cache Backend** (LRU, RAM): Hot data, O(1) access
2. **Parquet Backend** (Cold, Disk): High compression (zstd level 19)
3. **DuckDB Backend** (SQL Analytics): Complex queries on Parquet

This replaces the previous QuestDB integration, providing:
- **18× better compression** (vs 1.07× QuestDB)
- **-20% cost reduction** (24 CHF vs 30 CHF/month)
- **SQL analytics** via DuckDB
- **Fast cache** for frequently accessed data

## Architecture Diagram

```text
┌─────────────────────────────────────────────────────────────────┐
│                        Application Layer                        │
│                                                                 │
│  Python/Rust Client ◄──► Polarway gRPC Server                  │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│                       Storage Layer                              │
│                                                                 │
│  ┌─────────────┐        ┌─────────────┐        ┌─────────────┐ │
│  │   Cache     │        │   Parquet   │        │   DuckDB    │ │
│  │ (LRU, RAM)  │◄──────►│ (Cold, zstd)│◄──────►│(SQL Queries)│ │
│  │             │        │             │        │             │ │
│  │ • Hot data  │        │ • 18× comp  │        │ • Analytics │ │
│  │ • O(1) load │        │ • Durable   │        │ • Joins     │ │
│  │ • 2GB limit │        │ • Append-only│        │ • Aggs     │ │
│  └─────────────┘        └─────────────┘        └─────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

## Smart Loading

The hybrid storage implements a **smart loading strategy**:

1. **Check cache first** (fast path)
   - If found: return immediately (O(1))
   - Cache hit rate: ~85% for typical workloads

2. **Load from Parquet** (cold path)
   - If not in cache: read from Parquet
   - Decompress zstd (fast, ~50ms for 1M rows)
   - **Warm the cache** for next access

3. **SQL queries** (analytics path)
   - For complex queries: use DuckDB
   - DuckDB reads Parquet directly (zero-copy)
   - Supports joins, aggregations, window functions

## Storage Backends

### Cache Backend

**Purpose**: Fast access to frequently used DataFrames

**Features**:
- LRU eviction policy
- Thread-safe (RwLock)
- Configurable size (default: 2GB)
- Hit/miss tracking

**Usage**:
```rust
use polarway_grpc::CacheBackend;

let cache = CacheBackend::new(2.0); // 2GB cache

// Store
cache.store("key", batch)?;

// Load (fast)
let data = cache.load("key")?;

// Stats
let stats = cache.stats()?;
println!("Hit rate: {:.1}%", stats.cache_hits as f64 / (stats.cache_hits + stats.cache_misses) as f64 * 100.0);
```

### Parquet Backend

**Purpose**: Compressed cold storage with high durability

**Features**:
- zstd compression level 19 (15-20× ratio)
- Column-oriented storage
- Atomic writes with fsync
- Schema evolution support

**File Layout**:
```text
/data/cold/
├── BTC_USD_20260203.parquet
├── ETH_USD_20260203.parquet
└── trades_daily_20260203.parquet
```

**Usage**:
```rust
use polarway_grpc::ParquetBackend;

let backend = ParquetBackend::new("/data/cold")?;

// Store with compression
backend.store("trades_20260203", batch)?;

// Load
let data = backend.load("trades_20260203")?;

// Stats
let stats = backend.stats()?;
println!("Compression: {:.1}×", stats.compression_ratio);
```

### DuckDB Backend

**Purpose**: SQL analytics engine for complex queries

**Features**:
- Zero-copy Parquet reading
- Vectorized SIMD execution
- Full SQL support (joins, aggregations, CTEs)
- Window functions

**Usage**:
```rust
use polarway_grpc::DuckDBBackend;

let duckdb = DuckDBBackend::new(":memory:")?;

// SQL query on Parquet files
let result = duckdb.query(r#"
    SELECT time_bucket(INTERVAL '5m', timestamp) as bucket,
           symbol,
           avg(price) as avg_price,
           stddev(price) as volatility
    FROM read_parquet('/data/cold/trades_*.parquet')
    WHERE timestamp > now() - INTERVAL '7 days'
    GROUP BY bucket, symbol
    ORDER BY bucket DESC
"#)?;
```

## Hybrid Storage

The **HybridStorage** combines all three backends:

```rust
use polarway_grpc::HybridStorage;

let storage = HybridStorage::new(
    "/data/cold".to_string(),   // Parquet path
    ":memory:".to_string(),      // DuckDB path
    2.0,                         // Cache size (GB)
)?;

// Smart load: cache → Parquet → warm cache
let data = storage.smart_load("key")?;

// Store: cache + Parquet
storage.store("key", batch)?;

// Query: DuckDB analytics
let result = storage.query("SELECT * FROM read_parquet('/data/cold/*.parquet')")?;

// Statistics
let stats = storage.stats()?;
println!("Total keys: {}", stats.total_keys);
println!("Total size: {} MB", stats.total_size_bytes / 1_000_000);
println!("Cache hit rate: {:.1}%", stats.cache_hits as f64 / (stats.cache_hits + stats.cache_misses) as f64 * 100.0);
println!("Compression: {:.1}×", stats.compression_ratio);
```

## Python Client

The Python client provides a simplified interface:

```python
from polarway import StorageClient

# Connect
client = StorageClient(
    parquet_path="/data/cold",
    enable_cache=True
)

# Store DataFrame
client.store("trades_20260203", df)

# Smart load (cache → Parquet)
df = client.load("trades_20260203")

# SQL query
df = client.query("""
    SELECT symbol, avg(price) as avg_price
    FROM read_parquet('/data/cold/*.parquet')
    WHERE timestamp > '2026-01-01'
    GROUP BY symbol
""")

# Simplified query builder
df = client.query_simple(
    pattern="trades_*.parquet",
    select="symbol, avg(price) as avg_price",
    where="timestamp > '2026-01-01'",
    group_by="symbol"
)

# Statistics
stats = client.stats()
print(f"Cache hit rate: {stats['cache_hit_rate']:.1%}")
print(f"Compression: {stats['compression_ratio']:.1f}×")
```

## Performance Characteristics

### Compression

| Data Type | Original Size | Compressed Size | Ratio |
|-----------|--------------|-----------------|-------|
| OHLCV (numeric) | 1 GB | 55 MB | 18.2× |
| Trades (mixed) | 1 GB | 65 MB | 15.4× |
| Tick data | 1 GB | 50 MB | 20.0× |

### Latency

| Operation | Cache Hit | Cache Miss (Parquet) | DuckDB Query |
|-----------|-----------|---------------------|--------------|
| Load 1M rows | ~1ms | ~50ms | ~45ms (with agg) |
| Store 1M rows | ~2ms | ~150ms | N/A |
| Query (simple) | ~1ms | N/A | ~25ms |
| Query (complex join) | N/A | N/A | ~120ms |

### Cost Comparison

| Component | QuestDB (Old) | Parquet+DuckDB (New) | Savings |
|-----------|--------------|---------------------|---------|
| VPS (4GB RAM) | 20 CHF | 20 CHF | - |
| Storage (50GB) | 10 CHF | 4 CHF (20GB) | -6 CHF |
| **Total/month** | **30 CHF** | **24 CHF** | **-20%** |
| **Total/year** | **360 CHF** | **288 CHF** | **-72 CHF** |

## Migration Guide

### From QuestDB

1. **Export QuestDB data**:
```sql
-- QuestDB
COPY trades TO '/export/trades.csv';
```

2. **Convert to Parquet**:
```python
import polars as pl

df = pl.read_csv("/export/trades.csv")
df.write_parquet(
    "/data/cold/trades_20260203.parquet",
    compression="zstd",
    compression_level=19
)
```

3. **Update client code**:
```python
# Old (QuestDB)
from questdb.ingress import Sender
sender = Sender(host='localhost', port=9009)
sender.dataframe(df, table_name='trades')

# New (Polarway storage)
from polarway import StorageClient
client = StorageClient(parquet_path="/data/cold")
client.store("trades_20260203", df)
```

## Best Practices

### Cache Sizing

- **Recommended**: 1-2 GB for typical workloads
- **Rule of thumb**: Cache 10-20% of hot data
- **Monitor**: Track hit rate (target: >80%)

### Parquet Partitioning

- **Time-based**: Daily or hourly partitions
- **Size target**: 100-500 MB per file (optimal for DuckDB)
- **Naming**: Use date suffixes (`trades_20260203.parquet`)

### DuckDB Queries

- **Use wildcards**: `read_parquet('/data/cold/*.parquet')`
- **Filter early**: WHERE clauses pushed down to Parquet
- **Partition pruning**: Name files by date for efficient scans

### Ingestion

- **Buffer size**: 10,000 rows (balance latency/throughput)
- **Compression**: Always use zstd level 19
- **Atomic writes**: Ensure fsync after flush

## Troubleshooting

### Low cache hit rate

```python
# Check cache size
stats = client.stats()
print(f"Cache size: {stats['total_size_mb']} MB")

# Increase cache if needed
client = StorageClient(
    parquet_path="/data/cold",
    cache_size_gb=4.0  # Increase from 2GB to 4GB
)
```

### High query latency

```python
# Check compression ratio
stats = client.stats()
if stats['compression_ratio'] < 10.0:
    print("⚠️ Low compression - check data types")

# Enable DuckDB query profiling
result = client.query("EXPLAIN ANALYZE SELECT ...")
print(result)
```

### Storage growth

```bash
# Check storage usage
make storage-stats

# Clean old partitions
find /data/cold -name "*.parquet" -mtime +30 -delete
```

## References

- [Parquet Documentation](https://parquet.apache.org/docs/)
- [DuckDB Documentation](https://duckdb.org/docs/)
- [Arrow Rust Guide](https://arrow.apache.org/rust/)
- [LRU Cache Wikipedia](https://en.wikipedia.org/wiki/Cache_replacement_policies#Least_recently_used_(LRU))
