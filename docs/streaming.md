# Streaming Guide

Polarway supports streaming data processing for larger-than-RAM datasets, real-time pipelines, and per-batch callbacks — all with constant memory usage.

## Overview

Polarway's streaming capabilities work at two levels:

1. **Python-native streaming** via Polars' lazy engine (`collect(engine="streaming")`, `sink_*`)
2. **gRPC streaming** via the Polarway server (Arrow Flight batches over the network)

## Larger-than-RAM Processing

Process datasets of any size with constant memory using Polars' streaming engine:

```python
import polars as pl

# VWAP on 100GB of tick data — constant memory, ~8M rows/sec
vwap = (
    pl.scan_parquet("ticks/**/*.parquet")
    .filter(pl.col("side") == "BUY")
    .group_by("symbol")
    .agg(
        (pl.col("price") * pl.col("volume")).sum() / pl.col("volume").sum()
    )
    .collect(engine="streaming")  # Chunk-by-chunk, bounded RAM
)
```

Key: `collect(engine="streaming")` processes data in chunks without loading the full dataset.

## Streaming File Writes

Transform and write without loading into RAM:

```python
# Streaming ETL: filter + enrich + write
(
    pl.scan_parquet("raw/**/*.parquet")
    .with_columns(
        (pl.col("price") * pl.col("volume")).alias("notional")
    )
    .filter(pl.col("notional") > 5_000)
    .sink_parquet("enriched/output.parquet")
)

# Streaming CSV export
(
    pl.scan_parquet("data/*.parquet")
    .select(["symbol", "price", "timestamp"])
    .sink_csv("export/data.csv")
)

# Streaming NDJSON export
(
    pl.scan_parquet("data/*.parquet")
    .sink_ndjson("export/data.ndjson")
)
```

## Per-Batch Callbacks

Process data chunk-by-chunk with custom callbacks for alerting, forwarding, or aggregation:

```python
# Spike detection on streaming data
def on_batch(batch: pl.DataFrame):
    spikes = batch.filter(pl.col("price").pct_change().abs() > 0.03)
    if len(spikes):
        alert(spikes)

pl.scan_parquet("ticks/*.parquet").sink_batches(on_batch, chunk_size=200_000)
```

## Real-Time Ingestion

### WebSocket Ingestion

```python
import asyncio
import websockets
import polars as pl
import json

async def ingest_websocket(uri: str, output_path: str):
    buffer = []
    async with websockets.connect(uri) as ws:
        async for message in ws:
            tick = json.loads(message)
            buffer.append(tick)

            if len(buffer) >= 10_000:
                df = pl.DataFrame(buffer)
                df.write_parquet(f"{output_path}/{df['timestamp'][0]}.parquet")
                buffer.clear()

asyncio.run(ingest_websocket("wss://stream.exchange.com/btc", "/data/ticks"))
```

### HTTP Polling

```python
import httpx
import polars as pl

async def poll_orderbook(url: str, interval_ms: int = 100):
    async with httpx.AsyncClient() as client:
        while True:
            resp = await client.get(url)
            snapshot = resp.json()
            df = pl.DataFrame(snapshot["bids"] + snapshot["asks"])
            yield df
            await asyncio.sleep(interval_ms / 1000)
```

## gRPC Streaming

The Polarway gRPC server streams Arrow batches over the network for distributed processing:

```python
import polarway as pw

# Connect to remote server
client = pw.Client("grpc://server:50051")

# Server-side lazy scan — data stays on server
handle = client.scan_parquet("hdfs://data/100gb.parquet")

# Server-side filtering + aggregation
result_handle = (
    handle
    .filter(pw.col("price") > 100)
    .group_by("symbol")
    .agg({"price": "mean"})
)

# Only .collect() streams Arrow batches to client
result = result_handle.collect()  # Streams compressed Arrow IPC
```

## Concurrent Operations

Execute multiple operations in parallel for dramatic speedups:

```python
import asyncio
import polarway as pw

async def process_files(files: list[str]) -> list[pl.DataFrame]:
    client = pw.Client("grpc://server:50051")

    # Process 100 files concurrently (vs sequentially)
    tasks = [client.read_parquet_async(f) for f in files]
    return await asyncio.gather(*tasks)

# 100 files: 1.2s (parallel) vs 15s (sequential) = 12.5× speedup
results = asyncio.run(process_files(glob("data/*.parquet")))
```

## Streaming Patterns Summary

| Pattern | Use Case | Memory | Throughput |
|---------|----------|--------|------------|
| `collect(engine="streaming")` | Larger-than-RAM queries | O(1) | ~8M rows/s |
| `sink_parquet` | Streaming file writes | O(1) | ~4M rows/s |
| `sink_batches(fn)` | Per-batch callbacks | O(batch) | ~6M rows/s |
| gRPC streaming | Distributed processing | O(batch) | ~5M rows/s |
| WebSocket ingestion | Real-time data capture | O(buffer) | Market-rate |
| `collect_async()` | Concurrent execution | O(N) | 10-100× speedup |

## Adaptive Streaming

For advanced memory-mapped I/O with adaptive batch sizing, see the [Streaming Adaptive](STREAMING_ADAPTIVE.md) documentation covering:

- Zero-copy memory-mapped I/O
- Automatic batch size tuning based on system resources
- Parallel processing with work-stealing
- 10-50× speed improvement over naive streaming

## Next Steps

- [Time-Series Guide](timeseries.md) — OHLCV resampling, indicators, as-of joins
- [Storage Architecture](storage.md) — Hybrid storage for hot/warm/cold data
- [Distributed Mode](distributed-mode.md) — gRPC server-client setup
- [Examples](examples.md) — Complete working examples
