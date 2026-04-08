# Time-Series Guide

Polarway provides native time-series support optimized for financial and IoT data: OHLCV resampling, rolling windows, as-of joins, and common indicators.

## TimeSeriesFrame

The `TimeSeriesFrame` wraps a Polars DataFrame with frequency awareness:

```python
import polarway as pw

# Load tick data and create TimeSeriesFrame
ticks = pw.read_parquet("btc_ticks.parquet")
ts = ticks.as_timeseries("timestamp")

# Frequency-aware operations
print(ts.frequency)  # "tick"
print(ts.timespan)   # "2026-01-01 → 2026-02-01"
```

## OHLCV Resampling

Convert tick data to OHLCV bars at any frequency:

```python
# Tick → 1-minute OHLCV bars
ohlcv_1m = ts.resample_ohlcv("1m", price_col="price", volume_col="volume")

# Tick → 5-minute bars
ohlcv_5m = ts.resample_ohlcv("5m", price_col="price", volume_col="volume")

# Multi-timeframe: 1m → 1h → 1d
ohlcv_1h = ohlcv_1m.resample_ohlcv("1h")
ohlcv_1d = ohlcv_1h.resample_ohlcv("1d")
```

**Supported frequencies:** `tick`, `1s`, `5s`, `10s`, `30s`, `1m`, `5m`, `15m`, `30m`, `1h`, `4h`, `1d`, `1w`, `1mo`

## Rolling Windows

Apply rolling computations with configurable window sizes:

```python
import polars as pl

# SMA (Simple Moving Average) over 20 periods
sma_20 = ohlcv_5m.with_columns(
    pl.col("close").rolling_mean(window_size=20).alias("sma_20")
)

# Bollinger Bands (20-period SMA ± 2 std dev)
bollinger = ohlcv_5m.with_columns([
    pl.col("close").rolling_mean(window_size=20).alias("bb_mid"),
    (pl.col("close").rolling_mean(window_size=20)
     + 2 * pl.col("close").rolling_std(window_size=20)).alias("bb_upper"),
    (pl.col("close").rolling_mean(window_size=20)
     - 2 * pl.col("close").rolling_std(window_size=20)).alias("bb_lower"),
])

# EMA (Exponential Moving Average)
ema_12 = ohlcv_5m.with_columns(
    pl.col("close").ewm_mean(span=12).alias("ema_12")
)
```

## Financial Indicators

### VWAP (Volume Weighted Average Price)

```python
vwap = (
    pl.scan_parquet("ticks/*.parquet")
    .group_by("symbol")
    .agg(
        (pl.col("price") * pl.col("volume")).sum() / pl.col("volume").sum()
    )
    .collect(engine="streaming")  # Constant memory
)
```

### Returns and Volatility

```python
# Percentage returns
returns = ohlcv_5m.with_columns(
    pl.col("close").pct_change().alias("returns")
)

# Realized volatility (rolling 20-period std of returns)
realized_vol = returns.with_columns(
    pl.col("returns").rolling_std(window_size=20).alias("realized_vol")
)
```

### RSI (Relative Strength Index)

```python
def compute_rsi(df: pl.DataFrame, period: int = 14) -> pl.DataFrame:
    delta = df.with_columns(pl.col("close").diff().alias("delta"))
    gains = delta.with_columns(
        pl.when(pl.col("delta") > 0).then(pl.col("delta")).otherwise(0).alias("gain")
    )
    losses = delta.with_columns(
        pl.when(pl.col("delta") < 0).then(-pl.col("delta")).otherwise(0).alias("loss")
    )
    avg_gain = gains.with_columns(pl.col("gain").rolling_mean(window_size=period).alias("avg_gain"))
    avg_loss = losses.with_columns(pl.col("loss").rolling_mean(window_size=period).alias("avg_loss"))
    return avg_gain.join(avg_loss, on="timestamp").with_columns(
        (100 - 100 / (1 + pl.col("avg_gain") / pl.col("avg_loss"))).alias("rsi")
    )
```

## As-Of Joins

Align data from different frequencies using point-in-time joins:

```python
# Join 1-minute OHLCV with daily reference data
aligned = ohlcv_1m.join_asof(
    daily_stats,
    on="timestamp",
    strategy="backward"  # Use most recent daily value
)

# Join trades with order book snapshots
trades_with_book = trades.join_asof(
    orderbook_snapshots,
    on="timestamp",
    tolerance="100ms"  # Max 100ms staleness
)
```

## Streaming Time-Series

Process larger-than-RAM time-series with constant memory:

```python
# Stream 100GB of tick data — constant memory
(
    pl.scan_parquet("ticks/**/*.parquet")
    .sort("timestamp")
    .group_by_dynamic("timestamp", every="5m")
    .agg([
        pl.col("price").first().alias("open"),
        pl.col("price").max().alias("high"),
        pl.col("price").min().alias("low"),
        pl.col("price").last().alias("close"),
        pl.col("volume").sum().alias("volume"),
    ])
    .sink_parquet("ohlcv_5m/output.parquet")
)
```

See the [Streaming Guide](streaming.md) for more streaming patterns.

## Next Steps

- [Storage Architecture](storage.md) — Hybrid storage for time-series data
- [Streaming Guide](streaming.md) — Real-time processing patterns
- [Examples](examples.md) — Complete working examples
- [Performance Comparison](PERFORMANCE_COMPARISON.md) — Benchmarks
