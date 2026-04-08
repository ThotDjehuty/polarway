# Examples

Real-world examples of using Polarway for data engineering tasks.

## Basic Operations

### Creating DataFrames

```python
import polarway as pw
import polars as pl

# From dictionary
df = pw.DataFrame({
    "symbol": ["BTC", "ETH", "SOL"],
    "price": [50000, 3000, 100],
    "volume": [1.5, 10.0, 50.0]
})

# From CSV
df = pw.read_csv("data.csv")

# From Parquet
df = pw.read_parquet("data.parquet")

# From storage
client = pw.StorageClient(parquet_path="/data/cold")
df = client.load("trades_20260203")
```

### Filtering and Selecting

```python
# Filter rows
filtered = df.filter(pw.col("price") > 1000)

# Select columns
selected = df.select(["symbol", "price"])

# Chaining operations
result = (
    df
    .filter(pw.col("price") > 1000)
    .select(["symbol", "price"])
    .sort("price", descending=True)
)
```

### Aggregations

```python
# Group by and aggregate
result = df.group_by("symbol").agg({
    "price": "mean",
    "volume": "sum"
})

# Multiple aggregations
result = df.group_by("symbol").agg([
    pw.col("price").mean().alias("avg_price"),
    pw.col("price").min().alias("min_price"),
    pw.col("price").max().alias("max_price"),
    pw.col("volume").sum().alias("total_volume")
])
```

## Storage Operations

### Storing Data

```python
from polarway import StorageClient

client = StorageClient(
    parquet_path="/data/cold",
    enable_cache=True,
    cache_size_gb=2.0
)

# Basic store
client.store("trades_20260203", df)

# Smart store (Parquet + Cache)
client.smart_store("trades_20260203", df)

# Batch store
client.store_batch({
    "trades_20260203": trades_df,
    "quotes_20260203": quotes_df,
    "orders_20260203": orders_df
})
```

### Loading Data

```python
# Basic load
df = client.load("trades_20260203")

# Smart load (Cache → Parquet)
df = client.smart_load("trades_20260203")  # <1ms after first load

# Time-range load
from datetime import datetime, timedelta

df = client.load_time_range(
    symbol="BTC",
    start=datetime(2026, 2, 1),
    end=datetime(2026, 2, 7)
)

# Pattern load
dfs = client.load_pattern("trades_2026*")

# Lazy load (streaming)
lazy_df = client.scan("trades_*.parquet")
result = (
    lazy_df
    .filter(pw.col("price") > 100)
    .collect()
)
```

### SQL Queries

```python
# Execute SQL
result = client.query("""
    SELECT 
        symbol,
        AVG(price) as avg_price,
        COUNT(*) as count
    FROM read_parquet('/data/cold/trades_*.parquet')
    WHERE timestamp > '2026-02-01'
    GROUP BY symbol
    ORDER BY avg_price DESC
""")

# Simplified query builder
result = client.query_simple(
    pattern="trades_*.parquet",
    select="symbol, avg(price) as avg_price",
    where="timestamp > '2026-02-01'",
    group_by="symbol"
)

# Parameterized query
result = client.query_params("""
    SELECT * FROM read_parquet('/data/cold/trades_*.parquet')
    WHERE symbol = $1 AND price > $2
""", params=["BTC", 40000])
```

## Time-Series Operations

### OHLCV Resampling

```python
import polars as pl

# Resample tick data to 1-minute bars
ohlcv = (
    tick_data
    .sort("timestamp")
    .group_by_dynamic("timestamp", every="1m")
    .agg([
        pl.col("price").first().alias("open"),
        pl.col("price").max().alias("high"),
        pl.col("price").min().alias("low"),
        pl.col("price").last().alias("close"),
        pl.col("volume").sum().alias("volume")
    ])
)

# Store result
client.store("ohlcv_1m_20260203", ohlcv)
```

### Rolling Windows

```python
# Calculate technical indicators
indicators = df.with_columns([
    # Simple Moving Average
    pl.col("price").rolling_mean(window=20).alias("sma_20"),
    pl.col("price").rolling_mean(window=50).alias("sma_50"),
    
    # Volatility
    pl.col("returns").rolling_std(window=20).alias("vol_20"),
    
    # Momentum
    pl.col("returns").rolling_sum(window=5).alias("momentum_5")
])

# Bollinger Bands
bollinger = df.with_columns([
    pl.col("price").rolling_mean(window=20).alias("middle"),
])
.with_columns([
    (pl.col("middle") + 2 * pl.col("price").rolling_std(window=20)).alias("upper"),
    (pl.col("middle") - 2 * pl.col("price").rolling_std(window=20)).alias("lower")
])
```

### As-Of Joins

```python
# Join trades with quotes at trade time
enriched = trades.join_asof(
    quotes,
    left_on="trade_time",
    right_on="quote_time",
    by="symbol",
    strategy="backward"
)

# Railway-Oriented approach
def join_trades_and_quotes(date: str) -> Result[pl.DataFrame, str]:
    trades_key = f"trades_{date}"
    quotes_key = f"quotes_{date}"
    
    return (
        client.load(trades_key)
        .and_then(lambda trades: 
            client.load(quotes_key).map(lambda quotes: (trades, quotes))
        )
        .map(lambda data: data[0].join_asof(
            data[1],
            left_on="trade_time",
            right_on="quote_time",
            by="symbol"
        ))
    )
```

## Streaming Large Datasets

### Lazy Evaluation

```python
# Process 100GB dataset on 16GB RAM
result = (
    pw.scan_parquet("/data/huge/*.parquet")  # Lazy scan
    .filter(pw.col("price") > 100)           # Lazy filter
    .with_columns([
        (pw.col("price") * pw.col("quantity")).alias("value")
    ])
    .group_by("symbol")
    .agg([
        pw.col("value").sum().alias("total_value"),
        pw.col("value").mean().alias("avg_value")
    ])
    .collect()  # Execute now (streaming mode)
)
```

### Chunked Processing

```python
# Process in batches
for batch in pw.scan_parquet("data/*.parquet").iter_slices(batch_size=100000):
    # Process batch
    processed = process_batch(batch)
    
    # Write result
    client.store(f"output_{batch_id}", processed)
    
    # Memory freed after each iteration
```

### Streaming Sink

```python
# Stream to Parquet
(
    pw.scan_parquet("input/*.parquet")
    .filter(pw.col("price") > 100)
    .with_columns([
        (pw.col("price") * pw.col("quantity")).alias("value")
    ])
    .sink_parquet("output.parquet")  # Stream to disk
)
```

## Railway-Oriented Patterns

### Basic Error Handling

```python
from polarway import Result, Ok, Err

def load_and_process(key: str) -> Result[pl.DataFrame, str]:
    return (
        client.load(key)
        .and_then(validate_schema)
        .and_then(transform_data)
        .and_then(aggregate)
        .map_err(lambda e: f"Pipeline failed: {e}")
    )

# Use it
match load_and_process("trades_20260203"):
    case Ok(data):
        print(f"Success: {len(data)} rows")
        client.store("output", data)
    case Err(error):
        print(f"Error: {error}")
```

### Error Recovery

```python
# Try primary, fallback to backup, create empty if both fail
result = (
    client.load("trades_20260203")
    .or_else(lambda _: client.load("trades_backup"))
    .or_else(lambda _: Ok(create_empty_df()))
)
```

### Validation Pipeline

```python
def validate_schema(df: pl.DataFrame) -> Result[pl.DataFrame, str]:
    required = ["timestamp", "symbol", "price", "volume"]
    missing = [col for col in required if col not in df.columns]
    
    if missing:
        return Err(f"Missing columns: {missing}")
    return Ok(df)

def validate_data(df: pl.DataFrame) -> Result[pl.DataFrame, str]:
    if df.filter(pw.col("price") <= 0).height > 0:
        return Err("Invalid prices found (price <= 0)")
    return Ok(df)

def validate_pipeline(key: str) -> Result[pl.DataFrame, str]:
    return (
        client.load(key)
        .and_then(validate_schema)
        .and_then(validate_data)
    )
```

## Performance Optimization

### Caching Hot Data

```python
# First load: ~50ms (Parquet)
df1 = client.smart_load("hot_key")

# Second load: <1ms (Cache hit!)
df2 = client.smart_load("hot_key")

# Check cache stats
stats = client.cache_stats()
print(f"Hit rate: {stats['hit_rate']:.1%}")
print(f"Size: {stats['size_gb']:.2f} GB")
```

### Parallel Processing

**Python (asyncio):**

```python
import asyncio
from polarway import AsyncDistributedClient

async def process_all_symbols(symbols):
    client = AsyncDistributedClient(host="localhost", port=50052)
    
    tasks = [process_symbol(client, symbol) for symbol in symbols]
    results = await asyncio.gather(*tasks)
    
    return results

async def process_symbol(client, symbol):
    df = await client.load_async(f"trades_{symbol}")
    processed = df.filter(pw.col("price") > 100)
    await client.store_async(f"output_{symbol}", processed)
    return len(processed)

symbols = ["BTC", "ETH", "SOL", "AVAX"]
results = asyncio.run(process_all_symbols(symbols))
```

**Rust (tokio):**

```rust
use tokio::try_join;

let (btc, eth, sol) = try_join!(
    process_symbol("BTC"),
    process_symbol("ETH"),
    process_symbol("SOL")
)?;
```

### Batch Operations

```python
# Batch store (faster than individual stores)
data = {
    "trades_20260203": trades_df,
    "quotes_20260203": quotes_df,
    "orders_20260203": orders_df
}

client.store_batch(data)

# Batch load
keys = ["trades_20260203", "quotes_20260203", "orders_20260203"]
dfs = [client.load(key) for key in keys]
```

## Real-World Use Cases

### Use Case 1: Market Data Pipeline

```python
from datetime import datetime, timedelta
import polars as pl
from polarway import StorageClient, Result, Ok, Err

client = StorageClient(parquet_path="/data/market", enable_cache=True)

def market_data_pipeline(symbol: str, date: str) -> Result[dict, str]:
    """Complete market data processing pipeline."""
    
    key = f"trades_{symbol}_{date}"
    
    return (
        client.load(key)
        # 1. Data cleaning
        .map(lambda df: df.filter(
            (pw.col("price") > 0) &
            (pw.col("volume") > 0)
        ))
        # 2. Feature engineering
        .map(lambda df: df.with_columns([
            pw.col("price").pct_change().alias("returns"),
            (pw.col("price") * pw.col("volume")).alias("value")
        ]))
        # 3. Technical indicators
        .map(lambda df: df.with_columns([
            pw.col("price").rolling_mean(window=20).alias("sma_20"),
            pw.col("returns").rolling_std(window=20).alias("vol_20")
        ]))
        # 4. OHLCV resampling
        .map(lambda df: df.group_by_dynamic("timestamp", every="1m").agg([
            pw.col("price").first().alias("open"),
            pw.col("price").max().alias("high"),
            pw.col("price").min().alias("low"),
            pw.col("price").last().alias("close"),
            pw.col("volume").sum().alias("volume")
        ]))
        # 5. Summary statistics
        .map(lambda df: {
            "symbol": symbol,
            "date": date,
            "bars": len(df),
            "avg_price": df["close"].mean(),
            "total_volume": df["volume"].sum(),
            "price_range": df["high"].max() - df["low"].min(),
            "volatility": df["close"].std()
        })
    )

# Run pipeline
result = market_data_pipeline("BTC", "20260203")

match result:
    case Ok(stats):
        print(f"✅ {stats['symbol']} on {stats['date']}")
        print(f"   Bars: {stats['bars']}")
        print(f"   Avg Price: ${stats['avg_price']:.2f}")
        print(f"   Volume: {stats['total_volume']:.2f}")
        print(f"   Volatility: {stats['volatility']:.2%}")
    case Err(e):
        print(f"❌ Pipeline failed: {e}")
```

### Use Case 2: Backtesting Engine

```python
def backtest_strategy(symbol: str, start: datetime, end: datetime) -> Result[dict, str]:
    """Backtest trading strategy with Railway-Oriented error handling."""
    
    return (
        # Load historical data
        client.load_time_range(symbol, start, end)
        
        # Calculate signals
        .map(lambda df: df.with_columns([
            # SMA crossover strategy
            pw.col("price").rolling_mean(window=20).alias("sma_20"),
            pw.col("price").rolling_mean(window=50).alias("sma_50")
        ]))
        .map(lambda df: df.with_columns([
            # Long when SMA20 > SMA50
            (pw.col("sma_20") > pw.col("sma_50")).cast(int).alias("signal")
        ]))
        
        # Calculate returns
        .map(lambda df: df.with_columns([
            pw.col("price").pct_change().alias("returns"),
        ]))
        .map(lambda df: df.with_columns([
            (pw.col("signal").shift(1) * pw.col("returns")).alias("strategy_returns")
        ]))
        
        # Performance metrics
        .map(lambda df: {
            "symbol": symbol,
            "total_return": (1 + df["strategy_returns"]).product() - 1,
            "sharpe_ratio": df["strategy_returns"].mean() / df["strategy_returns"].std() * (252 ** 0.5),
            "max_drawdown": calculate_max_drawdown(df["strategy_returns"]),
            "trades": df["signal"].diff().abs().sum() / 2
        })
    )

def calculate_max_drawdown(returns: pl.Series) -> float:
    """Calculate maximum drawdown from returns series."""
    cumulative = (1 + returns).cumprod()
    running_max = cumulative.cummax()
    drawdown = (cumulative - running_max) / running_max
    return drawdown.min()

# Run backtest
result = backtest_strategy("BTC", datetime(2026, 1, 1), datetime(2026, 2, 1))

match result:
    case Ok(metrics):
        print(f"📊 Backtest Results for {metrics['symbol']}")
        print(f"   Total Return: {metrics['total_return']:.2%}")
        print(f"   Sharpe Ratio: {metrics['sharpe_ratio']:.2f}")
        print(f"   Max Drawdown: {metrics['max_drawdown']:.2%}")
        print(f"   Trades: {int(metrics['trades'])}")
    case Err(e):
        print(f"❌ Backtest failed: {e}")
```

### Use Case 3: Real-Time Streaming

```python
import asyncio
from polarway import AsyncDistributedClient

async def realtime_processor():
    """Process real-time market data stream."""
    
    client = AsyncDistributedClient(host="localhost", port=50052)
    
    while True:
        try:
            # Fetch latest tick data
            latest_key = f"ticks_{datetime.now().strftime('%Y%m%d_%H%M')}"
            ticks = await client.load_async(latest_key)
            
            # Process in real-time
            processed = (
                ticks
                .with_columns([
                    pw.col("price").pct_change().alias("returns"),
                    (pw.col("price") * pw.col("volume")).alias("value")
                ])
                .filter(pw.col("value") > 10000)  # Large trades only
            )
            
            # Detect anomalies
            if len(processed) > 0:
                high_vol = processed.filter(
                    pw.col("returns").abs() > processed["returns"].std() * 3
                )
                
                if len(high_vol) > 0:
                    print(f"⚠️  High volatility detected: {len(high_vol)} large moves")
                    await send_alert(high_vol)
            
            # Wait before next check
            await asyncio.sleep(1)
            
        except Exception as e:
            print(f"Error: {e}")
            await asyncio.sleep(5)

# Run processor
asyncio.run(realtime_processor())
```

## Next Steps

- 📚 [Getting Started](getting-started.md) - Quick introduction
- 🐍 [Python Client](python-client.md) - Complete Python API
- 🦀 [Rust Client](rust-client.md) - Complete Rust API
- 🌐 [Distributed Mode](distributed-mode.md) - Deploy gRPC server

---

**More examples:** [GitHub Examples](https://github.com/ThotDjehuty/polarway/tree/main/examples)
