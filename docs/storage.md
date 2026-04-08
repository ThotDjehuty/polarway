# Storage Architecture

Polarway's **Hybrid Storage Layer** is a three-tier architecture that balances performance, cost, and scalability.

## Overview

```
Request ──► LRU Cache (2GB RAM) ──► Cache Hit (~1ms) ──► Return
                 │
            Cache Miss
                 ▼
            Parquet (zstd lvl19, 18× compression) ──► Load + Warm (~50ms) ──► Return
                 │
            Complex Query
                 ▼
            DuckDB (SQL Analytics) ──► Query (~45ms) ──► Return
```

**Results:** -20% cost (24 CHF vs 30 CHF/month for 100GB), 18× compression, 85%+ cache hit rate.

## Storage Backends

### CacheBackend (LRU)

In-memory LRU cache for hot data with configurable capacity:

```rust
use polarway_grpc::storage::CacheBackend;

let cache = CacheBackend::new(2 * 1024 * 1024 * 1024); // 2GB
cache.store("trades_today", df)?;

// Sub-millisecond reads for cached data
let df = cache.load("trades_today")?; // ~0.5ms
```

- **Capacity:** Configurable (default 2GB)
- **Eviction:** Least-Recently-Used
- **Hit Rate:** 85%+ for typical time-series workloads
- **Latency:** < 1ms

### ParquetBackend

Compressed columnar storage with predicate pushdown:

```rust
use polarway_grpc::storage::ParquetBackend;

let backend = ParquetBackend::new("/data/cold");

// Store with zstd level 19 compression (18× ratio)
backend.store("btc_ticks_20260203", df)?;

// Load with column pruning and predicate pushdown
let df = backend.load("btc_ticks_20260203")?; // ~50ms
```

- **Compression:** zstd level 19 (18:1 ratio)
- **Format:** Apache Parquet (columnar)
- **Features:** Column pruning, predicate pushdown, row group skipping
- **Latency:** ~50ms for typical scans

### DuckDB Backend

SQL analytics engine for complex queries on cold data:

```rust
use polarway_grpc::storage::DuckDBBackend;

let db = DuckDBBackend::new(":memory:");

let result = db.query("
    SELECT symbol, AVG(price) as avg_price, COUNT(*) as trades
    FROM read_parquet('/data/cold/*.parquet')
    WHERE timestamp > '2026-01-01'
    GROUP BY symbol
    ORDER BY trades DESC
")?;
```

- **Engine:** DuckDB (embedded OLAP)
- **Features:** Full SQL, window functions, CTEs, joins
- **Latency:** ~45ms for analytical queries

## HybridStorage Router

The `HybridStorage` router automatically selects the optimal backend:

```python
from polarway import StorageClient

client = StorageClient(
    parquet_path="/data/cold",
    enable_cache=True,
    cache_size_gb=2.0
)

# Automatic routing: Cache → Parquet → DuckDB
df = client.load("trades_20260203")       # Cache hit: ~1ms
df = client.load("trades_20250101")       # Cache miss → Parquet: ~50ms

# SQL queries go directly to DuckDB
result = client.query("""
    SELECT symbol, AVG(price) as avg_price
    FROM read_parquet('/data/cold/*.parquet')
    WHERE timestamp > '2026-01-01'
    GROUP BY symbol
""")
```

## Cost Comparison

| Metric | Polarway | QuestDB | TimescaleDB |
|--------|----------|---------|-------------|
| **Storage (100GB/mo)** | 24 CHF | 30 CHF | 45 CHF |
| **Compression** | 18:1 | 1.07:1 | 3:1 |
| **Cache Layer** | Built-in LRU | Limited | None |
| **SQL Analytics** | Full (DuckDB) | InfluxQL | PostgreSQL |

## Performance

| Operation | Latency | Throughput |
|-----------|---------|------------|
| Cache hit | < 1ms | ~10M rows/s |
| Parquet scan | ~50ms | ~8M rows/s |
| DuckDB query | ~45ms | ~5M rows/s |
| Store (zstd) | ~100ms | ~4M rows/s |

## Next Steps

- [Getting Started](getting-started.md) — Install and run your first pipeline
- [Lakehouse Guide](lakehouse.md) — ACID transactions, time-travel, GDPR
- [Performance Comparison](PERFORMANCE_COMPARISON.md) — Detailed benchmarks
