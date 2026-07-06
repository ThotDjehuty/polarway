# Polarway Quick Reference

A comprehensive cheat sheet for common Polarway operations.

## 🚀 Getting Started

```python
import polarway as pw

# Connect to gRPC server
client = pw.connect("localhost:50051")
```

## 📥 Data Loading

### Reading Files

```python
# Parquet (recommended for performance)
df = pw.read_parquet("data.parquet")

# With column selection
df = pw.read_parquet("data.parquet", columns=["col1", "col2"])

# With server-side filtering
df = pw.read_parquet("data.parquet", predicate="price > 100")

# CSV
df = pw.read_csv("data.csv")
df = pw.read_csv("data.csv", separator=";", has_header=True)

# JSON
df = pw.read_json("data.json")
df = pw.read_json("data.ndjson", format="ndjson")  # Newline-delimited
```

### Reading from Streams

```python
# WebSocket
stream = pw.from_websocket(
    url="wss://stream.example.com/ws",
    schema={"price": pw.Float64, "timestamp": pw.Datetime("ms")},
    format="json"
)

# Process in batches
async for batch in stream.batches(size=1000):
    print(batch)

# REST API with pagination
df = pw.read_rest_api(
    url="https://api.example.com/data",
    pagination="cursor",
    page_size=1000
)
```

## 🔍 Data Inspection

```python
# View first rows
df.head(10)

# View last rows
df.tail(10)

# Schema information
df.schema()

# Row count
df.count()

# Summary statistics
df.describe()

# Column names
df.columns()
```

## ✂️ Selecting Columns

```python
# Select specific columns
df.select(["col1", "col2"])

# Select with expressions
df.select([
    pw.col("price"),
    pw.col("volume").alias("vol"),
    (pw.col("price") * pw.col("volume")).alias("notional")
])

# Drop columns
df.drop(["unwanted_col1", "unwanted_col2"])

# Rename columns
df.rename({"old_name": "new_name"})
```

## 🔎 Filtering Rows

```python
# Simple filter
df.filter(pw.col("price") > 100)

# Multiple conditions (AND)
df.filter(
    (pw.col("price") > 100) & 
    (pw.col("volume") > 1000)
)

# Multiple conditions (OR)
df.filter(
    (pw.col("symbol") == "AAPL") | 
    (pw.col("symbol") == "GOOGL")
)

# String operations
df.filter(pw.col("name").str.contains("Apple"))
df.filter(pw.col("email").str.ends_with("@example.com"))

# Null handling
df.filter(pw.col("value").is_not_null())
df.filter(pw.col("optional").is_null())
```

## ➕ Adding/Modifying Columns

```python
# Add new column
df.with_column(
    (pw.col("price") * 1.1).alias("price_with_tax")
)

# Multiple columns at once
df.with_columns([
    (pw.col("price") * pw.col("quantity")).alias("total"),
    pw.col("price").cast(pw.Int64).alias("price_int")
])

# Conditional column
df.with_column(
    pw.when(pw.col("price") > 100)
      .then(pw.lit("expensive"))
      .otherwise(pw.lit("affordable"))
      .alias("price_category")
)
```

## 📊 Grouping and Aggregation

```python
# Simple group by
df.group_by("symbol").agg({"price": "mean"})

# Multiple aggregations
df.group_by("symbol").agg({
    "price": ["mean", "max", "min", "std"],
    "volume": ["sum", "count"]
})

# Custom aggregations
df.group_by("symbol").agg([
    pw.col("price").mean().alias("avg_price"),
    pw.col("price").max().alias("max_price"),
    pw.col("volume").sum().alias("total_volume")
])

# Multiple group-by columns
df.group_by(["date", "symbol"]).agg({"price": "mean"})
```

## 🔗 Joining DataFrames

```python
# Inner join
df1.join(df2, on="id", how="inner")

# Left join
df1.join(df2, on="id", how="left")

# Right join
df1.join(df2, on="id", how="right")

# Outer join
df1.join(df2, on="id", how="outer")

# Join on multiple columns
df1.join(df2, on=["col1", "col2"], how="inner")

# Join with different column names
df1.join(df2, left_on="id", right_on="user_id", how="left")
```

## 📈 Sorting

```python
# Sort ascending
df.sort("price")

# Sort descending
df.sort("price", descending=True)

# Multiple columns
df.sort(["symbol", "timestamp"])
df.sort([("symbol", False), ("price", True)])  # symbol asc, price desc
```

## 🧮 Expressions

### Column Operations

```python
# Arithmetic
pw.col("price") + 10
pw.col("price") * pw.col("quantity")
pw.col("value") / 100

# Comparisons
pw.col("price") > 100
pw.col("status") == "active"
pw.col("amount").between(10, 100)

# Logical operations
(pw.col("a") > 5) & (pw.col("b") < 10)
(pw.col("status") == "A") | (pw.col("status") == "B")
~pw.col("flag")  # NOT
```

### String Operations

```python
# Basic string methods
pw.col("name").str.to_uppercase()
pw.col("name").str.to_lowercase()
pw.col("text").str.strip()

# Pattern matching
pw.col("email").str.contains("@gmail.com")
pw.col("text").str.starts_with("Hello")
pw.col("file").str.ends_with(".csv")

# String manipulation
pw.col("text").str.replace("old", "new")
pw.col("full_name").str.split(" ")
pw.col("values").str.slice(0, 10)
```

### Datetime Operations

```python
# Extract components
pw.col("timestamp").dt.year()
pw.col("timestamp").dt.month()
pw.col("timestamp").dt.day()
pw.col("timestamp").dt.hour()
pw.col("timestamp").dt.minute()

# Date arithmetic
pw.col("date") + pw.duration(days=7)
pw.col("end_date") - pw.col("start_date")

# Formatting
pw.col("timestamp").dt.strftime("%Y-%m-%d")

# Timezone conversion
pw.col("timestamp").dt.convert_timezone("UTC")
```

### Null Handling

```python
# Check for nulls
pw.col("value").is_null()
pw.col("value").is_not_null()

# Fill nulls
pw.col("value").fill_null(0)
pw.col("value").fill_null_with_strategy("forward")
pw.col("value").fill_null_with_strategy("backward")

# Drop nulls
df.drop_nulls()
df.drop_nulls(subset=["important_col"])
```

### Conditional Logic

```python
# Simple when-then-otherwise
pw.when(pw.col("price") > 100)
  .then(pw.lit("high"))
  .otherwise(pw.lit("low"))

# Multiple conditions
pw.when(pw.col("price") > 100)
  .then(pw.lit("high"))
  .when(pw.col("price") > 50)
  .then(pw.lit("medium"))
  .otherwise(pw.lit("low"))
```

## ⚡ Performance Operations

### Lazy Evaluation

```python
# Build lazy query
lazy_df = df.lazy()

# Chain operations
result = (
    lazy_df
    .filter(pw.col("price") > 100)
    .select(["symbol", "price"])
    .group_by("symbol")
    .agg({"price": "mean"})
    .collect()  # Execute query
)
```

### Streaming (for large datasets)

```python
# Stream large file
for batch in pw.scan_parquet("huge_file.parquet").iter_batches():
    # Process each batch
    processed = batch.filter(pw.col("value") > 0)
    processed.write_parquet("output.parquet", mode="append")
```

### Parallel Operations

```python
import asyncio

async def process_files_parallel():
    async with pw.AsyncClient("localhost:50051") as client:
        # Read 100 files in parallel
        handles = await asyncio.gather(*[
            client.read_parquet(f"file_{i}.parquet")
            for i in range(100)
        ])
        
        # Process all in parallel
        results = await asyncio.gather(*[
            h.filter(pw.col("value") > 0).collect()
            for h in handles
        ])
    
    return results

results = await process_files_parallel()
```

## 📤 Data Output

### Writing Files

```python
# Parquet (recommended)
df.write_parquet("output.parquet")
df.write_parquet("output.parquet", compression="snappy")

# CSV
df.write_csv("output.csv")
df.write_csv("output.csv", separator=";", include_header=True)

# JSON
df.write_json("output.json")
df.write_json("output.ndjson", format="ndjson")

# Append mode
df.write_parquet("output.parquet", mode="append")
```

### Export to Other Formats

```python
# To PyArrow Table
table = df.collect()  # Returns pyarrow.Table

# To Pandas DataFrame
pandas_df = df.collect().to_pandas()

# To Python dictionaries
records = df.collect().to_pylist()

# To NumPy arrays
arrays = df.collect().to_pydict()
```

## 🎯 Common Patterns

### Method Chaining

```python
result = (
    df
    .filter(pw.col("date") >= "2024-01-01")
    .select(["symbol", "price", "volume"])
    .with_column((pw.col("price") * pw.col("volume")).alias("notional"))
    .group_by("symbol")
    .agg({
        "price": ["mean", "max", "min"],
        "volume": "sum",
        "notional": "sum"
    })
    .sort("notional", descending=True)
    .head(10)
    .collect()
)
```

### Window Functions

```python
# Rolling window
df.with_column(
    pw.col("price")
      .rolling(window_size=20)
      .mean()
      .alias("sma_20")
)

# Partition by group
df.with_column(
    pw.col("price")
      .rank()
      .over(partition_by="symbol", order_by="timestamp")
      .alias("price_rank")
)
```

### Pivot Operations

```python
# Pivot table
df.pivot(
    values="price",
    index="date",
    columns="symbol",
    aggregate_fn="mean"
)

# Unpivot (melt)
df.melt(
    id_vars=["date", "symbol"],
    value_vars=["open", "high", "low", "close"],
    variable_name="price_type",
    value_name="price"
)
```

## ⏱️ Time-Series Operations

### OHLCV Resampling

```python
# Load tick data
ticks = pw.read_parquet("ticks.parquet")

# Convert to time-series
ts = ticks.as_timeseries("timestamp")

# Resample to OHLCV bars
ohlcv_1m = ts.resample_ohlcv(
    "1m",
    price_col="price",
    volume_col="volume"
)

ohlcv_5m = ts.resample_ohlcv("5m", price_col="price", volume_col="volume")
ohlcv_1h = ts.resample_ohlcv("1h", price_col="price", volume_col="volume")
```

### Rolling Window Operations

```python
# Simple moving average
df.with_column(
    pw.col("close").rolling("20m").mean().alias("sma_20")
)

# Multiple aggregations
df.with_columns([
    pw.col("close").rolling("20m").mean().alias("sma_20"),
    pw.col("close").rolling("50m").mean().alias("sma_50"),
    pw.col("volume").rolling("20m").sum().alias("vol_20")
])
```

### Time-Based Operations

```python
# Lag/Lead
df.with_column(pw.col("price").lag(1).alias("prev_price"))
df.with_column(pw.col("price").lead(1).alias("next_price"))

# Difference
df.with_column(pw.col("price").diff().alias("price_change"))

# Percent change
df.with_column(pw.col("price").pct_change().alias("return"))
```

## 🛡️ Error Handling

```python
# Result type pattern
result = df.collect()

if result.is_ok():
    table = result.unwrap()
    print(table)
else:
    error = result.unwrap_err()
    print(f"Error: {error}")

# Monadic operations
result.map(lambda t: print(t)).map_err(lambda e: log_error(e))

# Try-except pattern
try:
    df = pw.read_parquet("file.parquet")
    result = df.collect()
except PolarwayError as e:
    print(f"Operation failed: {e}")
```

## 📊 Type System

```python
# Basic types
pw.Int8, pw.Int16, pw.Int32, pw.Int64
pw.UInt8, pw.UInt16, pw.UInt32, pw.UInt64
pw.Float32, pw.Float64
pw.Boolean
pw.Utf8  # String

# Temporal types
pw.Date
pw.Datetime("ms")  # millisecond precision
pw.Datetime("us")  # microsecond precision
pw.Datetime("ns")  # nanosecond precision
pw.Duration
pw.Time

# Complex types
pw.List(pw.Int64)
pw.Struct({"name": pw.Utf8, "age": pw.Int64})
pw.Categorical

# Type casting
df.with_column(pw.col("value").cast(pw.Float64))
```

---

**See Also**:
- [API Documentation](API_DOCUMENTATION.md) - Full API reference
- [Migration Guide](MIGRATION_GUIDE.md) - Moving from Polars
- [Architecture Guide](ARCHITECTURE.md) - Design deep dive
