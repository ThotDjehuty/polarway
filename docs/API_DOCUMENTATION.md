# Polarway API Documentation

Complete API documentation for both Rust and Python interfaces.

## 🐍 Python API

### Client Connection

#### `connect(address: str) -> Client`

Connect to a Polarway gRPC server.

```python
import polarway as pw

client = pw.connect("localhost:50051")
```

**Parameters:**
- `address`: Server address in format `host:port`

**Returns:** `Client` instance for synchronous operations

#### `AsyncClient(address: str)`

Async client for concurrent operations.

```python
async with pw.AsyncClient("localhost:50051") as client:
    df = await client.read_parquet("data.parquet")
    result = await df.collect()
```

**Methods:**
- `async read_parquet(path)`: Read Parquet file
- `async read_csv(path)`: Read CSV file
- `async read_json(path)`: Read JSON file

---

### DataFrame API

#### `class DataFrame`

Represents a reference (handle) to a DataFrame on the server.

##### Core Methods

**`select(columns: List[str | Expr]) -> DataFrame`**

Select specific columns.

```python
df.select(["col1", "col2"])
df.select([pw.col("price"), pw.col("volume").alias("vol")])
```

**`filter(predicate: Expr) -> DataFrame`**

Filter rows based on condition.

```python
df.filter(pw.col("price") > 100)
df.filter((pw.col("price") > 100) & (pw.col("volume") > 1000))
```

**`with_column(expr: Expr) -> DataFrame`**

Add or modify a single column.

```python
df.with_column((pw.col("price") * 1.1).alias("price_with_tax"))
```

**`with_columns(exprs: List[Expr]) -> DataFrame`**

Add or modify multiple columns.

```python
df.with_columns([
    pw.col("price").cast(pw.Int64).alias("price_int"),
    (pw.col("a") + pw.col("b")).alias("sum")
])
```

**`group_by(columns: List[str]) -> GroupBy`**

Group rows by columns.

```python
df.group_by("symbol").agg({"price": "mean"})
df.group_by(["date", "symbol"]).agg({"price": ["mean", "max"]})
```

**`join(other: DataFrame, on: str | List[str], how: str = "inner") -> DataFrame`**

Join two DataFrames.

```python
df1.join(df2, on="id", how="inner")
df1.join(df2, on=["col1", "col2"], how="left")
```

**Parameters:**
- `how`: `"inner"`, `"left"`, `"right"`, `"outer"`

**`sort(by: str | List[str], descending: bool = False) -> DataFrame`**

Sort DataFrame.

```python
df.sort("price")
df.sort("price", descending=True)
df.sort([("symbol", False), ("price", True)])
```

**`head(n: int = 5) -> DataFrame`**

Take first n rows.

```python
df.head(10)
```

**`tail(n: int = 5) -> DataFrame`**

Take last n rows.

```python
df.tail(10)
```

**`drop(columns: List[str]) -> DataFrame`**

Remove columns.

```python
df.drop(["unwanted_col"])
```

**`rename(mapping: Dict[str, str]) -> DataFrame`**

Rename columns.

```python
df.rename({"old_name": "new_name"})
```

**`drop_nulls(subset: Optional[List[str]] = None) -> DataFrame`**

Remove rows with null values.

```python
df.drop_nulls()
df.drop_nulls(subset=["important_col"])
```

##### Materialization

**`collect() -> Result[pyarrow.Table, Error]`**

Execute operations and retrieve results.

```python
result = df.collect()
if result.is_ok():
    table = result.unwrap()
```

**`lazy() -> LazyFrame`**

Convert to lazy evaluation.

```python
lazy_df = df.lazy()
```

##### Information

**`schema() -> Schema`**

Get DataFrame schema.

```python
schema = df.schema()
print(schema.fields)
```

**`columns() -> List[str]`**

Get column names.

```python
cols = df.columns()
```

**`count() -> int`**

Get row count.

```python
n_rows = df.count()
```

**`describe() -> DataFrame`**

Get summary statistics.

```python
stats = df.describe()
```

##### I/O Operations

**`write_parquet(path: str, compression: str = "snappy", mode: str = "overwrite")`**

Write to Parquet file.

```python
df.write_parquet("output.parquet")
df.write_parquet("output.parquet", compression="zstd", mode="append")
```

**`write_csv(path: str, separator: str = ",", include_header: bool = True)`**

Write to CSV file.

```python
df.write_csv("output.csv")
```

**`write_json(path: str, format: str = "json")`**

Write to JSON file.

```python
df.write_json("output.json")
df.write_json("output.ndjson", format="ndjson")
```

---

### GroupBy API

#### `class GroupBy`

Represents a grouped DataFrame.

**`agg(aggregations: Dict[str, str | List[str]]) -> DataFrame`**

Perform aggregations.

```python
df.group_by("symbol").agg({
    "price": ["mean", "max", "min"],
    "volume": "sum"
})
```

**Supported aggregations:**
- `"mean"`, `"avg"`: Average value
- `"sum"`: Sum of values
- `"min"`: Minimum value
- `"max"`: Maximum value
- `"count"`: Count of values
- `"std"`: Standard deviation
- `"var"`: Variance
- `"first"`: First value
- `"last"`: Last value
- `"median"`: Median value

---

### Expression API

#### `col(name: str) -> Expr`

Reference a column.

```python
pw.col("price")
```

#### `lit(value: Any) -> Expr`

Create a literal value.

```python
pw.lit(100)
pw.lit("text")
pw.lit(True)
```

#### `class Expr`

Expression builder for operations.

##### Arithmetic Operations

```python
pw.col("price") + 10
pw.col("price") - pw.col("cost")
pw.col("price") * pw.col("quantity")
pw.col("total") / pw.col("count")
pw.col("base") ** 2  # Power
```

##### Comparison Operations

```python
pw.col("price") > 100
pw.col("price") >= 100
pw.col("price") < 50
pw.col("price") <= 50
pw.col("status") == "active"
pw.col("status") != "inactive"
```

##### Logical Operations

```python
(pw.col("a") > 5) & (pw.col("b") < 10)  # AND
(pw.col("x") == 1) | (pw.col("y") == 2)  # OR
~pw.col("flag")  # NOT
```

##### Methods

**`alias(name: str) -> Expr`**

Set column name.

```python
pw.col("price").alias("price_usd")
```

**`cast(dtype: DataType) -> Expr`**

Cast to different type.

```python
pw.col("value").cast(pw.Float64)
pw.col("timestamp").cast(pw.Datetime("ms"))
```

**`is_null() -> Expr`**

Check if value is null.

```python
pw.col("value").is_null()
```

**`is_not_null() -> Expr`**

Check if value is not null.

```python
pw.col("value").is_not_null()
```

**`fill_null(value: Any) -> Expr`**

Replace null values.

```python
pw.col("price").fill_null(0)
```

**`between(lower: Any, upper: Any) -> Expr`**

Check if value is in range.

```python
pw.col("price").between(10, 100)
```

##### String Operations

**`str.to_uppercase() -> Expr`**

Convert to uppercase.

```python
pw.col("name").str.to_uppercase()
```

**`str.to_lowercase() -> Expr`**

Convert to lowercase.

```python
pw.col("name").str.to_lowercase()
```

**`str.contains(pattern: str) -> Expr`**

Check if string contains pattern.

```python
pw.col("email").str.contains("@gmail.com")
```

**`str.starts_with(prefix: str) -> Expr`**

Check if string starts with prefix.

```python
pw.col("text").str.starts_with("Hello")
```

**`str.ends_with(suffix: str) -> Expr`**

Check if string ends with suffix.

```python
pw.col("file").str.ends_with(".csv")
```

**`str.replace(old: str, new: str) -> Expr`**

Replace substring.

```python
pw.col("text").str.replace("old", "new")
```

**`str.strip() -> Expr`**

Remove leading/trailing whitespace.

```python
pw.col("text").str.strip()
```

**`str.split(delimiter: str) -> Expr`**

Split string.

```python
pw.col("full_name").str.split(" ")
```

**`str.slice(start: int, length: int) -> Expr`**

Extract substring.

```python
pw.col("text").str.slice(0, 10)
```

##### Datetime Operations

**`dt.year() -> Expr`**

Extract year.

```python
pw.col("timestamp").dt.year()
```

**`dt.month() -> Expr`**

Extract month.

```python
pw.col("timestamp").dt.month()
```

**`dt.day() -> Expr`**

Extract day.

```python
pw.col("timestamp").dt.day()
```

**`dt.hour() -> Expr`**

Extract hour.

```python
pw.col("timestamp").dt.hour()
```

**`dt.minute() -> Expr`**

Extract minute.

```python
pw.col("timestamp").dt.minute()
```

**`dt.second() -> Expr`**

Extract second.

```python
pw.col("timestamp").dt.second()
```

**`dt.strftime(format: str) -> Expr`**

Format as string.

```python
pw.col("timestamp").dt.strftime("%Y-%m-%d")
```

##### Aggregation Methods

**`mean() -> Expr`**

Calculate mean.

```python
pw.col("price").mean()
```

**`sum() -> Expr`**

Calculate sum.

```python
pw.col("volume").sum()
```

**`min() -> Expr`**

Find minimum.

```python
pw.col("price").min()
```

**`max() -> Expr`**

Find maximum.

```python
pw.col("price").max()
```

**`count() -> Expr`**

Count values.

```python
pw.col("id").count()
```

**`std() -> Expr`**

Calculate standard deviation.

```python
pw.col("returns").std()
```

**`var() -> Expr`**

Calculate variance.

```python
pw.col("returns").var()
```

##### Window Functions

**`rolling(window_size: int | str) -> RollingAgg`**

Create rolling window.

```python
pw.col("price").rolling(20).mean()
pw.col("price").rolling("20m").mean()  # Time-based
```

**`rank() -> Expr`**

Rank values.

```python
pw.col("price").rank()
```

**`over(partition_by: str, order_by: str) -> Expr`**

Window partition.

```python
pw.col("price").rank().over(partition_by="symbol", order_by="timestamp")
```

##### Time-Series Methods

**`lag(periods: int) -> Expr`**

Shift values backward.

```python
pw.col("price").lag(1)
```

**`lead(periods: int) -> Expr`**

Shift values forward.

```python
pw.col("price").lead(1)
```

**`diff() -> Expr`**

Calculate difference.

```python
pw.col("price").diff()
```

**`pct_change() -> Expr`**

Calculate percent change.

```python
pw.col("price").pct_change()
```

---

### Conditional Expressions

#### `when(condition: Expr) -> When`

Start conditional expression.

```python
pw.when(pw.col("price") > 100)
  .then(pw.lit("high"))
  .otherwise(pw.lit("low"))
```

#### `class When`

**`then(value: Expr) -> Then`**

Specify value if condition is true.

**`class Then`**

**`when(condition: Expr) -> When`**

Add another condition.

**`otherwise(value: Expr) -> Expr`**

Specify default value.

---

### I/O Functions

#### `read_parquet(path: str, columns: Optional[List[str]] = None, predicate: Optional[str] = None) -> DataFrame`

Read Parquet file.

```python
df = pw.read_parquet("data.parquet")
df = pw.read_parquet("data.parquet", columns=["col1", "col2"])
df = pw.read_parquet("data.parquet", predicate="price > 100")
```

#### `read_csv(path: str, separator: str = ",", has_header: bool = True) -> DataFrame`

Read CSV file.

```python
df = pw.read_csv("data.csv")
df = pw.read_csv("data.csv", separator=";")
```

#### `read_json(path: str, format: str = "json") -> DataFrame`

Read JSON file.

```python
df = pw.read_json("data.json")
df = pw.read_json("data.ndjson", format="ndjson")
```

#### `scan_parquet(path: str) -> LazyFrame`

Lazily scan Parquet file.

```python
lazy_df = pw.scan_parquet("data.parquet")
```

---

### Streaming API

#### `from_websocket(url: str, schema: Dict[str, DataType], format: str = "json") -> Stream`

Connect to WebSocket stream.

```python
stream = pw.from_websocket(
    url="wss://stream.example.com",
    schema={"price": pw.Float64, "timestamp": pw.Datetime("ms")},
    format="json"
)

async for batch in stream.batches(size=1000):
    print(batch)
```

#### `read_rest_api(url: str, pagination: str = "none", page_size: int = 100) -> DataFrame`

Read from REST API.

```python
df = pw.read_rest_api(
    url="https://api.example.com/data",
    pagination="cursor",
    page_size=1000
)
```

---

### Time-Series API

#### `as_timeseries(timestamp_col: str) -> TimeSeriesFrame`

Convert to time-series DataFrame.

```python
ts = df.as_timeseries("timestamp")
```

#### `class TimeSeriesFrame`

**`resample_ohlcv(frequency: str, price_col: str, volume_col: str) -> TimeSeriesFrame`**

Resample to OHLCV bars.

```python
ohlcv = ts.resample_ohlcv("1m", price_col="price", volume_col="volume")
ohlcv = ts.resample_ohlcv("5m", price_col="price", volume_col="volume")
```

**Supported frequencies:**
- `"1s"`, `"5s"`, `"10s"`, `"30s"`: Seconds
- `"1m"`, `"5m"`, `"15m"`, `"30m"`: Minutes
- `"1h"`, `"4h"`, `"12h"`: Hours
- `"1d"`, `"1w"`, `"1M"`: Days, weeks, months

---

### Data Types

```python
# Integer types
pw.Int8, pw.Int16, pw.Int32, pw.Int64
pw.UInt8, pw.UInt16, pw.UInt32, pw.UInt64

# Float types
pw.Float32, pw.Float64

# Other types
pw.Boolean
pw.Utf8  # String
pw.Date
pw.Datetime("ms")  # Timestamp with millisecond precision
pw.Datetime("us")  # Timestamp with microsecond precision
pw.Datetime("ns")  # Timestamp with nanosecond precision
pw.Duration
pw.Time

# Complex types
pw.List(pw.Int64)
pw.Struct({"name": pw.Utf8, "age": pw.Int64})
pw.Categorical
```

---

### Error Handling

#### `class Result[T, E]`

Rust-style Result type.

**`is_ok() -> bool`**

Check if result is success.

**`is_err() -> bool`**

Check if result is error.

**`unwrap() -> T`**

Get value (raises if error).

**`unwrap_err() -> E`**

Get error (raises if success).

**`map(fn: Callable[[T], U]) -> Result[U, E]`**

Transform success value.

**`map_err(fn: Callable[[E], F]) -> Result[T, F]`**

Transform error value.

```python
result = df.collect()

if result.is_ok():
    table = result.unwrap()
else:
    error = result.unwrap_err()
    print(f"Error: {error}")

# Or use monadic operations
result.map(lambda t: process(t)).map_err(lambda e: log_error(e))
```

---

## 🦀 Rust API

### Client Connection

```rust
use polarway_grpc::client::PolarwayClient;

let mut client = PolarwayClient::connect("http://localhost:50051").await?;
```

### DataFrame Operations

```rust
use polarway::prelude::*;

// Read Parquet
let df = client.read_parquet("data.parquet").await?;

// Select columns
let df = df.select(&["col1", "col2"])?;

// Filter rows
let df = df.filter(col("price").gt(lit(100)))?;

// Collect results
let result: DataFrame = df.collect().await?;
```

### Core Types

#### `struct DataFrame`

**Methods:**
- `select(&[&str]) -> Result<DataFrame>`
- `filter(Expr) -> Result<DataFrame>`
- `with_column(Expr) -> Result<DataFrame>`
- `group_by(&[&str]) -> GroupBy`
- `join(DataFrame, JoinArgs) -> Result<DataFrame>`
- `sort(&[&str], &[bool]) -> Result<DataFrame>`
- `collect() -> Result<DataFrame>`

#### `struct LazyFrame`

Lazy evaluation interface.

**Methods:**
- `filter(Expr) -> LazyFrame`
- `select(&[Expr]) -> LazyFrame`
- `group_by(&[&str]) -> LazyGroupBy`
- `join(LazyFrame, &[&str], JoinType) -> LazyFrame`
- `collect() -> Result<DataFrame>`

#### `enum Expr`

Expression builder.

**Constructors:**
- `col(name: &str) -> Expr`
- `lit<T: Literal>(value: T) -> Expr`

**Methods:**
- `alias(name: &str) -> Expr`
- `cast(dtype: DataType) -> Expr`
- `gt(rhs: Expr) -> Expr`
- `lt(rhs: Expr) -> Expr`
- `eq(rhs: Expr) -> Expr`
- `and(rhs: Expr) -> Expr`
- `or(rhs: Expr) -> Expr`

---

### Module Reference

#### `polarway::prelude`

Common imports.

```rust
use polarway::prelude::*;
```

Exports: `DataFrame`, `LazyFrame`, `Series`, `col`, `lit`, `DataType`

#### `polarway::io`

I/O operations.

```rust
use polarway::io::{ParquetReader, CsvReader};

let df = ParquetReader::new("data.parquet").finish()?;
```

#### `polarway::lazy`

Lazy evaluation.

```rust
use polarway::lazy::dsl::*;

let lazy_df = scan_parquet("data.parquet")?;
```

---

**See Also**:
- [Quick Reference](QUICK_REFERENCE.md) - Common operations cheat sheet
- [Migration Guide](MIGRATION_GUIDE.md) - Moving from Polars
- [Architecture Guide](ARCHITECTURE.md) - Design deep dive
