# When NOT to Use Polaroid

## 🎯 Purpose of This Document

Polaroid is a powerful tool for specific use cases, but it's not always the right choice. This guide helps you make informed decisions about when to use Polaroid vs alternatives like Polars, Pandas, DuckDB, or Spark.

## ❌ Don't Use Polaroid When...

### 1. **Single Client, In-Memory Workloads**

**Scenario**: Running notebooks or scripts on your local machine with datasets that fit in RAM.

**Why Not Polaroid**:
- **Network overhead**: gRPC adds 1-10ms latency per operation
- **No benefit**: Single client doesn't need shared memory
- **Complexity**: Client-server architecture is overkill

**Use Instead**:
- ✅ **Polars** - Same engine, zero network overhead, simpler setup
- ✅ **Pandas** - More familiar API for exploratory analysis

**Example**:
```python
# ❌ Don't do this (unnecessary overhead)
import polaroid as pd
df = pd.read_parquet("local_file.parquet").collect()  # Network round-trip for no benefit

# ✅ Do this instead
import polars as pl
df = pl.read_parquet("local_file.parquet")  # Direct, no network overhead
```

### 2. **Datasets Smaller Than 1GB**

**Scenario**: Working with small to medium datasets that load into memory instantly.

**Why Not Polaroid**:
- **Overhead exceeds benefit**: Network serialization takes longer than computation
- **Simpler alternatives**: Pandas/Polars are more straightforward
- **No streaming needed**: Entire dataset fits in RAM

**Use Instead**:
- ✅ **Polars** - Blazing fast for in-memory analytics
- ✅ **Pandas** - Familiar API, good enough for small data
- ✅ **SQLite/DuckDB** - Great for SQL-style queries on small data

**Benchmark**:
```python
# 100MB dataset benchmark
# Polars:   0.8s (load + query)
# Polaroid: 1.2s (network + load + query)
# Winner: Polars ✅
```

### 3. **Exploratory Data Analysis (EDA)**

**Scenario**: Jupyter notebooks with ad-hoc queries, visualizations, and iterative exploration.

**Why Not Polaroid**:
- **Interactive overhead**: Every operation requires server round-trip
- **Debugging harder**: Errors happen on server, not local
- **No notebook magic**: Can't use `df.head()` interactively

**Use Instead**:
- ✅ **Pandas** - Best for exploration, immediate results
- ✅ **Polars** - Fast exploration with lazy API

**Example**:
```python
# ❌ Polaroid in notebooks (slow iteration)
df = polaroid_client.read_parquet("data.parquet")
df.select("price").collect()  # Wait for network
df.filter(price > 100).collect()  # Wait again
df.group_by("symbol").collect()  # And again...

# ✅ Polars in notebooks (instant feedback)
df = pl.read_parquet("data.parquet")
df.select("price")  # Instant
df.filter(pl.col("price") > 100)  # Instant
df.group_by("symbol").agg(pl.col("price").mean())  # Instant
```

### 4. **SQL-First Workflows**

**Scenario**: Teams that prefer SQL over DataFrame APIs.

**Why Not Polaroid**:
- **Limited SQL support**: Polaroid is DataFrame-first
- **Better alternatives**: DuckDB, PostgreSQL have native SQL
- **No JDBC/ODBC**: Can't connect BI tools directly

**Use Instead**:
- ✅ **DuckDB** - Embedded SQL engine, Parquet-native, very fast
- ✅ **PostgreSQL** - Production-ready, ACID compliance, rich ecosystem
- ✅ **ClickHouse** - Columnar database for analytics

**Example**:
```python
# ❌ Polaroid with SQL (limited support)
result = polaroid_client.sql("SELECT * FROM df WHERE price > 100")  # Limited SQL syntax

# ✅ DuckDB with SQL (full support)
import duckdb
result = duckdb.query("SELECT * FROM 'data.parquet' WHERE price > 100")
```

### 5. **Production Web Applications**

**Scenario**: Building REST APIs or web services that need low-latency responses.

**Why Not Polaroid**:
- **Latency**: Network round-trips add 1-10ms overhead
- **Complexity**: Need to manage gRPC server lifecycle
- **Overkill**: Most web apps don't need distributed DataFrames

**Use Instead**:
- ✅ **PostgreSQL/MySQL** - Proven, ACID, connection pooling
- ✅ **Redis** - Sub-millisecond latency for hot data
- ✅ **DuckDB** - Embedded, zero-latency queries

**Architecture**:
```python
# ❌ Polaroid in web API (unnecessary complexity)
@app.get("/stats")
async def get_stats():
    df = await polaroid_client.read_parquet("data.parquet")
    stats = await df.describe().collect()  # 5-15ms total latency
    return stats

# ✅ PostgreSQL in web API (simpler, proven)
@app.get("/stats")
async def get_stats():
    stats = await db.query("SELECT AVG(price), COUNT(*) FROM orders")  # 1-3ms
    return stats
```

### 6. **Real-Time OLTP Workloads**

**Scenario**: High-frequency inserts, updates, deletes (e.g., order processing, user sessions).

**Why Not Polaroid**:
- **Read-optimized**: Polaroid is for analytics, not transactions
- **No ACID**: Can't guarantee consistency for concurrent writes
- **Wrong tool**: DataFrames aren't for transactional data

**Use Instead**:
- ✅ **PostgreSQL** - ACID compliance, row-level locking
- ✅ **MySQL/MariaDB** - Proven for OLTP workloads
- ✅ **CockroachDB/YugabyteDB** - Distributed ACID databases

### 7. **Machine Learning Training**

**Scenario**: Training scikit-learn, TensorFlow, or PyTorch models.

**Why Not Polaroid**:
- **No native integration**: ML libraries expect NumPy/Pandas
- **Unnecessary overhead**: Training data usually fits in RAM
- **Simpler pipelines**: Load once, train many times

**Use Instead**:
- ✅ **Polars** - Convert to Pandas/NumPy for ML libraries
- ✅ **Pandas** - Native integration with scikit-learn
- ✅ **Ray Datasets** - Distributed ML data loading

**Example**:
```python
# ❌ Polaroid for ML (extra conversion step)
from sklearn.ensemble import RandomForestClassifier
df = polaroid_client.read_parquet("train.parquet").collect()
X = df.select(features).to_pandas().values  # Extra conversion
y = df.select("label").to_pandas().values
model.fit(X, y)

# ✅ Polars for ML (direct conversion)
df = pl.read_parquet("train.parquet")
X = df.select(features).to_numpy()  # Direct conversion
y = df.select("label").to_numpy()
model.fit(X, y)
```

### 8. **< 10 Concurrent Users**

**Scenario**: Small team or personal projects with few simultaneous users.

**Why Not Polaroid**:
- **Benefit threshold**: Need 10+ concurrent users to justify distributed architecture
- **Operational overhead**: Managing server, monitoring, deployment
- **Cost**: Server costs vs PyO3 embedded

**Use Instead**:
- ✅ **Polars (PyO3)** - Embed directly in application, zero network
- ✅ **Embedded DuckDB** - SQL interface, embedded, fast

**Cost Analysis**:
```
1-10 users:
  PyO3 Polars: $0/month (embedded)
  Polaroid:    $50-100/month (server instance)
  
10-100 users:
  PyO3 Polars: $200/month (each instance loads data)
  Polaroid:    $50-100/month (shared memory)
  
100+ users:
  PyO3 Polars: $2000+/month (memory duplication)
  Polaroid:    $100-300/month (shared memory) ✅
```

### 9. **Cloud Functions / Serverless**

**Scenario**: AWS Lambda, Azure Functions, Google Cloud Functions with short-lived compute.

**Why Not Polaroid**:
- **Cold starts**: gRPC connection adds 100-500ms to first request
- **Complexity**: Need persistent server alongside ephemeral functions
- **Wrong model**: Serverless expects stateless execution

**Use Instead**:
- ✅ **WASM Polars** - Embed compute in function, no network
- ✅ **DuckDB WASM** - SQL queries in browser/function
- ✅ **S3 Select / Athena** - Query Parquet directly in S3

**Architecture**:
```python
# ❌ Serverless function calling Polaroid (cold start penalty)
@azure_function
def process_data(request):
    client = connect_polaroid()  # 200ms cold start
    df = client.read_parquet("data.parquet")  # 50ms network
    return df.sum().collect()  # 30ms compute
    # Total: 280ms (80ms is overhead)

# ✅ Serverless with embedded WASM
@azure_function
def process_data(request):
    df = polars_wasm.read_parquet("data.parquet")  # 10ms
    return df.sum()  # 30ms compute
    # Total: 40ms (no overhead) ✅
```

### 10. **Compliance-Heavy Industries**

**Scenario**: Finance, healthcare, government with strict data residency/privacy laws.

**Why Not Polaroid**:
- **Data leaves machine**: gRPC sends data over network
- **Audit complexity**: Need to track data movement between client/server
- **Compliance risk**: Some regulations forbid network data transfer

**Use Instead**:
- ✅ **Embedded Polars/DuckDB** - Data never leaves machine
- ✅ **On-premises PostgreSQL** - Full control, air-gapped if needed

## ✅ When Polaroid DOES Make Sense

For balance, here's when Polaroid is the right tool:

### 1. **Multi-Client Analytics Platform** ✅
- 10+ concurrent users querying the same datasets
- Memory sharing saves 10-100x RAM costs
- Example: Company-wide analytics dashboard

### 2. **Streaming / Time-Series Pipelines** ✅
- Processing real-time data feeds (WebSocket, Kafka)
- Rolling window operations on unbounded streams
- Example: Real-time trading signals

### 3. **Larger-Than-RAM Datasets** ✅
- Datasets don't fit in memory (10GB+)
- Need to stream and process in batches
- Example: Processing 100GB of historical data on 16GB machine

### 4. **Functional Programming Enthusiasts** ✅
- Want Rust's Result/Option monads in Python
- Value type safety and composable transformations
- Example: Safety-critical data pipelines

### 5. **Language-Agnostic Architecture** ✅
- Need to query from Python, Rust, Go, TypeScript
- gRPC provides consistent API across languages
- Example: Polyglot microservices architecture

## 🎯 Decision Tree

```
Do you have < 1GB of data?
├─ YES → Use Polars or Pandas ❌ Not Polaroid
└─ NO → Continue

Is this a single-user/single-process application?
├─ YES → Use Polars (PyO3) ❌ Not Polaroid
└─ NO → Continue

Do you have 10+ concurrent users?
├─ NO → Use Polars (PyO3) ❌ Not Polaroid
└─ YES → Continue

Do you need streaming or time-series operations?
├─ NO → Consider DuckDB or PostgreSQL
└─ YES → Use Polaroid ✅

Do you value functional programming patterns?
├─ NO → Consider DuckDB or PostgreSQL
└─ YES → Use Polaroid ✅
```

## 📚 Alternatives Comparison

| Use Case | Recommended Tool | Why Not Polaroid? |
|----------|------------------|-------------------|
| EDA in notebooks | Pandas, Polars | Network overhead slows iteration |
| Small data (<1GB) | Polars, DuckDB | Network overhead > compute time |
| SQL-first teams | DuckDB, PostgreSQL | Limited SQL support |
| Single user | Polars (PyO3) | No benefit from distributed architecture |
| OLTP workloads | PostgreSQL, MySQL | Not designed for transactions |
| ML training | Polars → NumPy | Extra conversion step |
| Serverless | WASM Polars, DuckDB | Cold start penalty |
| < 10 users | Polars (PyO3) | Operational overhead not justified |

## 🎓 Summary

**Polaroid is NOT a silver bullet**. It excels at:
- Multi-client analytics (10+ users)
- Streaming time-series data
- Functional programming patterns
- Language-agnostic architectures

But for most common scenarios (EDA, small data, single user), **simpler tools like Polars, Pandas, or DuckDB are better choices**.

**Rule of thumb**: Start with Polars (PyO3). Only add Polaroid when you have:
1. 10+ concurrent users, OR
2. Streaming/real-time requirements, OR
3. Strong preference for functional programming

**Don't prematurely optimize for scale you don't have yet.** 🎯
