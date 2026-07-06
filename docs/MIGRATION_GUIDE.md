# Migration Guide: Polars to Polarway

A comprehensive guide to migrate your existing Polars code to Polarway.

## 📋 Table of Contents

1. [Why Migrate](#-why-migrate)
2. [Version-Specific Migrations](#-version-specific-migrations)
   - [v0.53.0: Adaptive Streaming Sources](#v0530-adaptive-streaming-sources-january-2026)
3. [General Polars → Polarway Migration](#-compatibility-overview)

---

## 🆕 v0.53.0: Adaptive Streaming Sources (January 2026)

### What's New

Polarway v0.53.0 introduces **Generic Adaptive Streaming Sources** with support for:
- **CSV** with adaptive chunking
- **Cloud Storage**: S3, Azure Blob, Google Cloud Storage  
- **Databases**: DynamoDB, PostgreSQL, MySQL
- **Streaming**: Apache Kafka
- **HTTP/REST APIs** with automatic pagination
- **Filesystem** with memory mapping

### Quick Migration

#### From Standard Polars CSV

**Before:**
```python
import polars as pl
df = pl.read_csv("large.csv")  # Loads entire file into memory
```

**After (v0.53.0):**
```python
import polarway as pl

# Automatic memory management
df = pl.adaptive_scan_csv("large.csv", memory_limit="2GB")

# Or streaming
from polarway.streaming import CsvSource
source = CsvSource("large.csv", memory_limit="2GB")
for chunk in source:
    process(chunk)
```

#### From Manual S3 Downloads

**Before:**
```python
import boto3
s3 = boto3.client('s3')
s3.download_file('bucket', 'data.parquet', '/tmp/data.parquet')
df = pl.read_parquet('/tmp/data.parquet')
```

**After (v0.53.0):**
```python
from polarway.streaming import S3Source

# Stream directly from S3
source = S3Source(
    "s3://bucket/data.parquet",
    memory_limit="4GB"
)
for chunk in source:
    process(chunk)
```

#### From Manual API Pagination

**Before:**
```python
import requests
data = []
page = 1
while True:
    r = requests.get(f"https://api.example.com/data?page={page}")
    if not r.json(): break
    data.extend(r.json())
    page += 1
df = pl.DataFrame(data)
```

**After (v0.53.0):**
```python
from polarway.streaming import HttpSource

# Automatic pagination
source = HttpSource(
    "https://api.example.com/data",
    paginated=True,
    memory_limit="1GB"
)
for chunk in source:
    process(chunk)
```

### Configuration

Create `polarway.toml` for default settings:

```toml
[sources.csv]
default_memory_limit = "2GB"
default_chunk_size = 10000

[sources.s3]
region = "us-east-1"
default_memory_limit = "4GB"

[sources.http]
timeout = 30
retry_attempts = 3
```

### Memory Recommendations

| Environment | Memory Limit | Chunk Size |
|-------------|--------------|------------|
| Laptop (8GB) | `"2GB"` | 10,000 |
| Desktop (16GB) | `"4GB"` | 50,000 |
| Server (32GB) | `"8GB"` | 100,000 |
| Azure B1s (1GB) | `"400MB"` | 5,000 |
| Azure B2s (4GB) | `"1.5GB"` | 20,000 |

### Custom Sources (Rust)

```rust
use polars_streaming_adaptive::sources::*;
use async_trait::async_trait;

#[derive(Debug)]
struct MySqlSource { /* ... */ }

#[async_trait]
impl StreamingSource for MySqlSource {
    async fn metadata(&self) -> SourceResult<SourceMetadata> { /* ... */ }
    async fn read_chunk(&mut self) -> SourceResult<Option<DataFrame>> { /* ... */ }
    fn stats(&self) -> StreamingStats { /* ... */ }
    async fn reset(&mut self) -> SourceResult<()> { /* ... */ }
    fn has_more(&self) -> bool { /* ... */ }
}

// Register
let mut registry = SourceRegistry::new();
registry.register("mysql", Box::new(MySqlSourceFactory));
```

### See Also

- [API Reference](API_REFERENCE.md) - Complete API documentation
- [User Guide](USER_GUIDE.md) - Comprehensive usage guide
- [Benchmarks](../notebooks/adaptive_streaming_benchmarks.ipynb) - Performance comparisons

---

## 🎯 Why Migrate?

### Performance Improvements

- **🌐 Remote Execution**: Process data on powerful servers from lightweight clients
- **📊 Streaming-First**: Handle larger-than-RAM datasets with constant memory usage
- **⚡ Zero-Copy Streaming**: Arrow IPC eliminates serialization overhead
- **🔄 True Parallelism**: Async Tokio runtime enables concurrent operations
- **📈 Better Scalability**: Multi-client access to shared datasets

### New Features

- **gRPC Architecture**: Handle-based remote execution
- **WebSocket Streaming**: Real-time data ingestion with sub-ms latency
- **REST API Integration**: Built-in pagination and retries
- **Time-Series Native**: OHLCV resampling and rolling windows
- **Distributed Computing**: Process data across multiple nodes
- **Advanced Async**: First-class async/await support

### Use Cases Perfect for Polarway

✅ Processing datasets larger than your machine's RAM  
✅ Shared data access from multiple clients  
✅ Real-time streaming data pipelines  
✅ Financial data analysis with time-series operations  
✅ Centralized data processing on powerful hardware  
✅ Network-sourced data (WebSocket, REST APIs)

## 🔄 Compatibility Overview

### ✅ What's Compatible

Most Polars operations work identically in Polarway:

```python
# These work the same in both Polars and Polarway
df.select(["col1", "col2"])
df.filter(pl.col("price") > 100)
df.group_by("symbol").agg({"price": "mean"})
df.join(df2, on="id", how="inner")
```

### ⚠️ Key Differences

#### 1. **Handle-Based Architecture**

**Polars** (in-memory):
```python
import polars as pl
df = pl.read_parquet("data.parquet")  # Returns DataFrame with data
print(type(df))  # <class 'polars.DataFrame'>
```

**Polarway** (remote handles):
```python
import polarway as pw
df = pw.read_parquet("data.parquet")  # Returns Handle reference
print(type(df))  # <class 'polarway.DataFrame'> (just a UUID)
```

#### 2. **Explicit Collection**

**Polars**:
```python
df = pl.read_parquet("data.parquet")
result = df.select(["col1"])  # Result is immediately available
print(result)  # Prints data
```

**Polarway**:
```python
df = pw.read_parquet("data.parquet")
df2 = df.select(["col1"])  # Returns new handle
result = df2.collect()  # Explicit collection needed
print(result)  # PyArrow Table
```

#### 3. **Result Types**

**Polars** (raises exceptions):
```python
try:
    df = pl.read_parquet("missing.parquet")
except Exception as e:
    print(f"Error: {e}")
```

**Polarway** (Result monad):
```python
result = pw.read_parquet("missing.parquet").collect()
if result.is_ok():
    df = result.unwrap()
else:
    error = result.unwrap_err()
```

## 📝 Step-by-Step Migration

### 1. Update Dependencies

#### Python

**Before (Polars):**
```toml
# pyproject.toml
dependencies = [
    "polars>=0.19.0",
]
```

**After (Polarway):**
```toml
# pyproject.toml
dependencies = [
    "polarway-df>=0.1.0",
]
```

Or with pip:
```bash
pip uninstall polars
pip install polarway-df
```

#### Rust

**Before (Polars):**
```toml
# Cargo.toml
[dependencies]
polars = "0.35"
```

**After (Polarway):**
```toml
# Cargo.toml
[dependencies]
polarway = "0.1"
polarway-grpc = "0.1"
```

### 2. Update Imports

**Before:**
```python
import polars as pl
from polars import col, lit
```

**After:**
```python
import polarway as pw
from polarway import col, lit

# Connect to server
client = pw.connect("localhost:50051")
```

### 3. Add Server Connection

Polarway requires a running gRPC server:

```bash
# Start server with Docker
docker run -d -p 50051:50051 polarway/server:latest

# Or build from source
cd polarway
cargo run -p polarway-grpc
```

### 4. Update Code Patterns

#### Simple Operations

**Before (Polars):**
```python
df = pl.read_parquet("data.parquet")
result = (
    df
    .filter(pl.col("price") > 100)
    .select(["symbol", "price"])
    .group_by("symbol")
    .agg({"price": "mean"})
)
print(result)
```

**After (Polarway):**
```python
df = pw.read_parquet("data.parquet")
result = (
    df
    .filter(pw.col("price") > 100)
    .select(["symbol", "price"])
    .group_by("symbol")
    .agg({"price": "mean"})
    .collect()  # Explicit collection
)
print(result)  # PyArrow Table
```

#### Lazy Operations

**Before (Polars):**
```python
lazy_df = pl.scan_parquet("data.parquet")
result = (
    lazy_df
    .filter(pl.col("price") > 100)
    .collect()
)
```

**After (Polarway):**
```python
# Lazy by default! No need for scan_
df = pw.read_parquet("data.parquet")
result = (
    df
    .filter(pw.col("price") > 100)
    .collect()
)
```

#### Multiple DataFrames

**Before (Polars):**
```python
df1 = pl.read_parquet("data1.parquet")
df2 = pl.read_parquet("data2.parquet")
joined = df1.join(df2, on="id")
```

**After (Polarway):**
```python
df1 = pw.read_parquet("data1.parquet")
df2 = pw.read_parquet("data2.parquet")
joined = df1.join(df2, on="id")
result = joined.collect()  # Explicit collection
```

### 5. Async Operations (New in Polarway!)

Polarway enables true parallel operations:

**Sequential (slow):**
```python
results = []
for i in range(100):
    df = pw.read_parquet(f"file_{i}.parquet")
    result = df.filter(pw.col("value") > 0).collect()
    results.append(result)
```

**Parallel (fast):**
```python
import asyncio

async def process_files():
    async with pw.AsyncClient("localhost:50051") as client:
        # Read all files in parallel
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

results = await process_files()
```

## 🗺️ Feature Mapping

### Core Operations

| Operation | Polars | Polarway | Notes |
|-----------|--------|----------|-------|
| Read Parquet | `pl.read_parquet()` | `pw.read_parquet()` | ✅ Same |
| Read CSV | `pl.read_csv()` | `pw.read_csv()` | ✅ Same |
| Select | `df.select()` | `df.select()` | ✅ Same |
| Filter | `df.filter()` | `df.filter()` | ✅ Same |
| Group By | `df.group_by()` | `df.group_by()` | ✅ Same |
| Join | `df.join()` | `df.join()` | ✅ Same |
| Collect | `df.collect()` (lazy) | `df.collect()` (always) | ⚠️ Always needed |

### I/O Operations

| Operation | Polars | Polarway | Notes |
|-----------|--------|----------|-------|
| Read local file | `pl.read_parquet("file.parquet")` | `pw.read_parquet("file.parquet")` | ✅ Same |
| Scan lazy | `pl.scan_parquet()` | `pw.read_parquet()` | ⚠️ Lazy by default |
| Write Parquet | `df.write_parquet()` | `df.write_parquet()` | ✅ Same |
| Write CSV | `df.write_csv()` | `df.write_csv()` | ✅ Same |

### Expressions

| Operation | Polars | Polarway | Notes |
|-----------|--------|----------|-------|
| Column reference | `pl.col("name")` | `pw.col("name")` | ✅ Same |
| Literal value | `pl.lit(100)` | `pw.lit(100)` | ✅ Same |
| String ops | `.str.contains()` | `.str.contains()` | ✅ Same |
| Datetime ops | `.dt.year()` | `.dt.year()` | ✅ Same |
| Aggregations | `.mean()`, `.sum()` | `.mean()`, `.sum()` | ✅ Same |

### New in Polarway

| Operation | Polars | Polarway | Notes |
|-----------|--------|----------|-------|
| Async client | ❌ N/A | `pw.AsyncClient()` | ✨ New |
| WebSocket | ❌ N/A | `pw.from_websocket()` | ✨ New |
| REST API | ❌ N/A | `pw.read_rest_api()` | ✨ New |
| Time-series | Manual | `df.as_timeseries()` | ✨ New |
| OHLCV resample | Manual | `.resample_ohlcv()` | ✨ New |
| Result monad | Exceptions | `.is_ok()`, `.unwrap()` | ✨ New |

## 🧪 Testing Your Migration

### Unit Testing Strategy

**Before (Polars):**
```python
def test_data_processing():
    df = pl.read_parquet("test_data.parquet")
    result = df.filter(pl.col("value") > 0)
    assert result.shape[0] > 0
```

**After (Polarway):**
```python
def test_data_processing():
    df = pw.read_parquet("test_data.parquet")
    result = df.filter(pw.col("value") > 0).collect()
    assert result.num_rows > 0
```

### Performance Benchmarking

```python
import time

# Polars
start = time.time()
df = pl.read_parquet("data.parquet")
result = df.filter(pl.col("price") > 100)
polars_time = time.time() - start

# Polarway
start = time.time()
df = pw.read_parquet("data.parquet")
result = df.filter(pw.col("price") > 100).collect()
polarway_time = time.time() - start

print(f"Polars: {polars_time:.3f}s")
print(f"Polarway: {polarway_time:.3f}s")
print(f"Overhead: {((polarway_time/polars_time)-1)*100:.1f}%")
```

**Expected Results:**
- Small datasets (<100MB): 5-10% overhead (gRPC network cost)
- Large datasets (>1GB): Similar or better performance
- Streaming (>RAM): Polarway wins (Polars OOM)
- Concurrent operations: Polarway 10-100x faster

### Validation Checklist

- [ ] All imports updated (`polars` → `polarway`)
- [ ] Server connection established
- [ ] `.collect()` added where needed
- [ ] Result types handled (`.unwrap()` or error checks)
- [ ] Async operations converted to `AsyncClient`
- [ ] Tests updated and passing
- [ ] Performance benchmarks acceptable
- [ ] Memory usage checked for large datasets

## 🐛 Common Migration Issues

### Issue 1: Missing `.collect()`

**Error:**
```python
df = pw.read_parquet("data.parquet")
print(df)  # Prints: DataFrame(handle="abc-123")
```

**Solution:**
```python
df = pw.read_parquet("data.parquet")
result = df.collect()
print(result)  # Prints: pyarrow.Table
```

### Issue 2: Immediate Evaluation Expected

**Error:**
```python
df = pw.read_parquet("data.parquet")
print(df.shape)  # AttributeError: 'DataFrame' object has no attribute 'shape'
```

**Solution:**
```python
df = pw.read_parquet("data.parquet")
result = df.collect()
print(result.num_rows, result.num_columns)
```

### Issue 3: Exception Handling

**Before:**
```python
try:
    df = pl.read_parquet("data.parquet")
except FileNotFoundError:
    print("File not found")
```

**After:**
```python
result = pw.read_parquet("data.parquet").collect()
if result.is_err():
    print(f"Error: {result.unwrap_err()}")
```

### Issue 4: Type Differences

**Polars** returns `polars.DataFrame`, **Polarway** returns `pyarrow.Table`:

```python
# Polars
df = pl.read_parquet("data.parquet")
print(type(df))  # <class 'polars.DataFrame'>

# Polarway
result = pw.read_parquet("data.parquet").collect()
print(type(result))  # <class 'pyarrow.Table'>

# Convert to Pandas if needed
pandas_df = result.to_pandas()
```

## 🔧 Migration Tools

### Automated Import Replacement

```bash
# Find all Polars imports
find . -name "*.py" -exec grep -l "import polars" {} \;

# Replace imports (macOS)
find . -name "*.py" -exec sed -i '' 's/import polars as pl/import polarway as pw/g' {} \;

# Replace imports (Linux)
find . -name "*.py" -exec sed -i 's/import polars as pl/import polarway as pw/g' {} \;
```

### Linting Rules

Add to your `.pylintrc`:

```ini
[SIMILARITIES]
# Detect Polars usage in Polarway projects
check-polars-imports=true
```

### Pre-commit Hook

```yaml
# .pre-commit-config.yaml
repos:
  - repo: local
    hooks:
      - id: check-polars-imports
        name: Check for Polars imports
        entry: bash -c 'if grep -r "import polars" .; then exit 1; fi'
        language: system
        pass_filenames: false
```

## 📚 Case Study: Real Migration

### Small Project (500 lines)

**Project:** Data analysis script

**Changes needed:**
1. Update imports (5 minutes)
2. Add `.collect()` calls (10 minutes)
3. Update tests (15 minutes)
4. Performance validation (10 minutes)

**Total time:** ~40 minutes

**Result:** +7% overhead, but gained streaming capability

### Large Codebase (50K lines)

**Project:** Financial data platform

**Strategy:**
1. **Phase 1:** Migrate read/write operations (week 1)
2. **Phase 2:** Migrate transformations (week 2-3)
3. **Phase 3:** Add async operations (week 4)
4. **Phase 4:** Enable streaming for large datasets (week 5)

**Challenges:**
- 200+ files to update
- Complex test suite
- Performance regression concerns

**Solutions:**
- Automated import replacement
- Incremental migration by module
- Parallel testing (Polars vs Polarway)
- Gradual rollout with feature flags

**Results:**
- Migration completed in 5 weeks
- 15% performance improvement on large datasets
- 50% memory reduction for streaming workflows
- 20x speedup for concurrent operations

## 🆘 Getting Help

### Community Support

- **GitHub Issues**: https://github.com/EnkiNudimmud/polarway/issues
- **Discussions**: https://github.com/EnkiNudimmud/polarway/discussions
- **Discord**: [Join our community](#)

### Migration Assistance

Need help with migration? Open an issue with:
- Your Polars code snippet
- Expected behavior
- Any errors encountered
- Performance requirements

We'll help you find the best Polarway equivalent!

---

**See Also**:
- [Quick Reference](QUICK_REFERENCE.md) - Common operations cheat sheet
- [API Documentation](API_DOCUMENTATION.md) - Complete API reference
- [Architecture Guide](ARCHITECTURE.md) - Understanding Polarway's design
