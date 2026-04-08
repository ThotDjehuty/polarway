# Changelog

All notable changes to Polarway will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.0] - 2026-03-29

### 🎉 Major Release — Distributed Horizons

#### ✨ Added
- **`polarway-bus` v0.1.0**: Generic `EventBus<T>` crate — broadcast fan-out, filtered subscribers, sync drain, async recv. 10 unit + 2 doc-tests passing.
- **Distributed Implementation Plan v2**: Rewrote `DISTRIBUTED_IMPLEMENTATION_PLAN.md` with retained solutions — EventBus as coordination layer, Lakehouse as Phase 2+ state store, `BusConnector` trait for pluggable message brokers.
- **Connector architecture**: `BusConnector` trait design for Redis, RabbitMQ, Kafka adapters. Contributor guide with NATS example.
- **Lakehouse ↔ Distributed integration**: `DeltaStore` mapped as `StateStore` backend — ACID handles, time-travel versioning, vacuum GC, z-order optimization.
- **Release notes**: `RELEASE_NOTES_v2.0.0.md` with full feature overview and roadmap.

#### 🧹 Changed
- **Repo cleanup**: Removed deprecated docs (README_OLD.md, README_LEGACY.md), upstream Polars-only directories (polars-cloud, polars-on-premises), duplicate guides, one-time migration scripts, and build artifacts.
- **Workspace Cargo.toml**: `polarway-bus` added to workspace members.
- Superseded `DISTRIBUTED_IMPLEMENTATION_PLAN_v1.md` (original plan archived).

#### 📋 Planned (v2.1.0)
- `polarway-connectors` crate (Redis, RabbitMQ, Kafka)
- `StateStore` trait implementation in `polarway-grpc`
- `WorkerEvent` protocol for inter-worker coordination
- Arrow 54+ upgrade to unblock `polarway-distributed`

---

## [1.0.0] - 2026-02-26

### 🎉 First Stable Release

Polarway v1.0.0 establishes the first stable, production-ready API for the Railway-Oriented DataFrame engine.

### ✨ New in 1.0.0

#### Lakehouse (Delta Lake support)
- ACID transactions with time-travel queries (`read_version`, `read_timestamp`)
- User management with Argon2 password hashing and JWT authentication
- Role-based access control (RBAC) for table-level permissions
- Audit logging for compliance (configurable retention)
- Delta Lake maintenance operations (vacuum, optimize, z-order clustering)
- DataFusion SQL engine integration for complex analytics queries

#### Python Package (`polarway` on PyPI)
- gRPC client library: `from polarway import PolarwayClient, TimeSeriesClient`
- `LakehouseClient` for Delta Lake operations with optional dependencies
- Async iterator streaming API: `async for batch in client.stream_query(...)`
- Arrow IPC zero-copy deserialization
- Automatic proto stub generation at install time
- Full type annotations throughout

#### Functional Programming Enhancements
- Railway-Oriented Programming (ROP) monad: `Result<T, E>`, `Option<T>`
- `Maybe` monad for optional chaining
- `pipe()`, `compose()`, `curry()` higher-order utilities
- Lens-based immutable update patterns
- `validate_dataframe()` with typed schema contracts

#### Notebooks (verified, all cells pass)
- `environment_check.ipynb` — dependency verification (✅ pass)
- `polarway_showcase.ipynb` — full feature tour (✅ pass)
- `polarway_functional_programming.ipynb` — ROP / monad patterns (✅ pass)
- `polarway_functional_programming_v2.ipynb` — advanced composition patterns (✅ pass)
- `storage_demo.ipynb` — LRU cache + Parquet + DuckDB hot/cold storage (✅ pass)
- `polarway_advanced.ipynb` — streaming joins, ETL, partitioning (✅ pass)
- `polarway_cloud_integrations.ipynb` — CSV/JSON/Parquet + SQLite + Pandas (✅ pass)
- `adaptive_streaming_benchmarks.ipynb` — requires private `polars_streaming_adaptive` build (⚠️ internal only)
- Notebooks requiring a running Polarway gRPC server (⚠️ server required):
  `phase2_operations_test.ipynb`, `phase5_streaming_test.ipynb`, `rest_exec_api_demo.ipynb`

#### Documentation
- Switched ReadTheDocs build to MkDocs Material
- Cleaned `mkdocs.yml`: removed Polars-upstream nav (polars-cloud, polars-on-premises)
- Fixed repo URL to `ThotDjehuty/polarway`
- Added 25 reference docs to nav

### 🔧 Changed
- `polarway-grpc` crate: version bumped `0.1.0` → `1.0.0`
- `polarway` Python package: version bumped `0.1.1` → `1.0.0`
- Package description updated to reflect production-ready status
- Development Status classifier: Alpha → Beta
- `docs/source/conf.py`: release `0.53.0` → `1.0.0`

### ✅ Test Results
- Python tests: **27/27 pass** (`pytest polarway-python/tests/`)
- Dependencies: `deltalake==1.4.2`, `pyarrow==18.1.0`, `argon2-cffi`, `PyJWT` installed in rhftlab
- Standalone notebooks: 7 notebooks pass `nbconvert --execute --inplace`
- Rust compilation: known `arrow-arith@53.4.0` × `chrono@0.4.39+` conflict (see ⚠️ Known Issues)

### ⚠️ Known Issues
- **Rust build conflict**: `arrow-arith 53.4.0` introduced a `quarter()` ambiguity with `chrono >= 0.4.39` (chrono added `Datelike::quarter()` which conflicts with `ChronoDateExt::quarter()`). Fix: upgrade arrow to 54+ or use a workspace-level `[patch.crates-io]`. This affects compilation but not the Python package (which uses grpcio, not the Rust gRPC binary).
- **pyarrow compatibility**: `pyarrow >= 21` has a binary incompatibility with `_azurefs` when `deltalake` is installed. Use `pyarrow == 18.1.0` for the Lakehouse optional dependency.

### 🔗 Links
- PyPI: https://pypi.org/project/polarway/2.0.0/
- crates.io: [`polarway-bus`](https://crates.io/crates/polarway-bus) · [`polarway-lakehouse`](https://crates.io/crates/polarway-lakehouse) · [`polarway-sources`](https://crates.io/crates/polarway-sources) · [`polarway-grpc`](https://crates.io/crates/polarway-grpc)
- ReadTheDocs: https://polarway.readthedocs.io
- Repository: https://github.com/ThotDjehuty/polarway

---

## [0.1.1] - 2026-02-11

### 🐛 Bug Fixes

#### Rust Compilation
- **PyO3 Deprecations**: Updated all 26 instances of deprecated `PyObject` to `Py<PyAny>` in polars-python bindings
- **Tracing Macros**: Fixed error logging format from Display (`%e`) to Debug (`?e`) for custom error types in lakehouse, maintenance, auth, and audit modules
- **TableProvider Integration**: Fixed trait object creation for DataFusion `TableProvider` in 5 locations (scan, query, sql, read_version, read_timestamp methods)
- **Code Warnings**: Suppressed false-positive dead code warnings in streaming-adaptive `mmap_reader` and timeseries `vwap` modules
- **Syntax**: Removed redundant trailing semicolons

#### Python Type Checking
- **Pylance Configuration**: Added `pyrightconfig.json` with appropriate settings for optional dependencies
- **Type Safety**: Fixed type mismatch in `approve_user` return value with explicit None check
- **Import Warnings**: Reduced Pylance errors from 36 to 13 by adding file-level type checking directives

### ✅ Testing
- All 8 unit tests pass successfully
- Zero compilation errors with `cargo clippy --all-targets`
- Only 3 style warnings remaining (FromStr trait suggestions)
- Compilation time: ~6.2s

### 📝 Notes
- Remaining Python type errors are false positives for optional dependencies (deltalake, argon2-cffi, PyJWT)
- These dependencies are runtime-checked in `LakehouseClient.__init__`
- Remaining rust-analyzer LSP errors are false positives (cargo compiles successfully)

## [0.1.0] - 2026-01-12

### 🎉 Initial Public Release

First public release of Polarway - a high-performance DataFrame library with gRPC streaming architecture.

### ✨ Features

#### Core Operations
- Read/write Parquet, CSV, JSON with schema inference
- Select, filter, group_by, join, sort, aggregate operations
- Lazy evaluation with automatic query optimization
- Predicate and projection pushdown to data sources
- Zero-copy Apache Arrow serialization
- Handle-based architecture for memory-efficient operations

#### Time-Series Support
- `TimeSeriesFrame` with frequency-aware operations
- OHLCV resampling (tick → 1m → 5m → 1h → 1d)
- Rolling window aggregations (SMA, EMA, Bollinger bands)
- Lag, lead, diff, and pct_change operations
- As-of joins for time-aligned data

#### Streaming & Network Sources
- WebSocket streaming with automatic reconnection
- REST API pagination (cursor-based, offset, link headers)
- Kafka, NATS, Redis Streams integration
- Real-time data pipelines with backpressure handling
- Async Tokio runtime for concurrent operations

#### gRPC Server
- Tonic-based gRPC server on port 50051
- Handle lifecycle management with configurable TTL
- Arrow IPC streaming for efficient data transfer
- Rust-based monadic error handling (Result<T, E>)

### 📚 Documentation
- Comprehensive README with Quick Start guide
- Docker deployment instructions
- Practical examples (time-series, WebSocket, concurrent operations)
- Clear Contributing guidelines
- Performance benchmarks vs Polars

### 🏗️ Architecture
- Client-server architecture with handle-based operations
- DataFrames stay on server, clients hold references
- Language-agnostic gRPC API
- Immutable operations (functional programming principles)
- Built on Polars core with gRPC interface layer

### 🙏 Acknowledgments
- Forked from [Polars](https://github.com/pola-rs/polars) v0.52.0
- Built with Apache Arrow, DataFusion, Tonic, and Tokio

---

[0.1.0]: https://github.com/EnkiNudimmud/polarway/releases/tag/v0.1.0
