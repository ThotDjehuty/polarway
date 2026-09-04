# Polarway Changelog

Product updates and releases across the Polarway DataFrame engine, gRPC streaming server, hybrid storage layer, and distributed computing framework, in reverse-chronological order.

---

## August 2026

### Polarway v0.54.0 — Lakehouse & Delta Lake Integration
*Planned release — In Development*

**Lakehouse Module**
- Full Delta Lake integration with ACID-compliant user management and time-travel
- JWT-based authentication with Argon2 password hashing
- Role-based access control (5 roles: guest, pending, registered, trader, admin)
- Session management with automatic token expiration
- Complete audit trail of all user actions
- Time-travel queries at any version or timestamp
- GDPR compliance with right to deletion
- Table optimization: compact, z-order, and vacuum operations
- Billing integration for usage tracking

**Storage & Infrastructure**
- Delta Lake tables: `users`, `sessions`, `audit_log`
- Migration path from SQLite backend via `AUTH_BACKEND=lakehouse`
- Docker environment configuration for deployment

**Developer Experience**
- 37 tests passing (8 unit, 9 auth, 9 store, 11 doc-tests)
- Comprehensive lakehouse documentation with time-travel examples
- GDPR compliance guide

---

## February 2026

### Polarway v0.53.0 — Hybrid Storage Layer (Major Release)
*Released February 3, 2026 — Breaking Changes*

**🔥 New Hybrid Storage Architecture**
- **Three-tier storage system** replacing QuestDB:
  - Cache Backend (LRU, RAM): O(1) hot data access, ~85% hit rate
  - Parquet Backend (Cold, Disk): zstd level 19, **18× compression** (vs 1.07× QuestDB)
  - DuckDB Backend (SQL Analytics): Zero-copy Parquet reading, full SQL support
- **HybridStorage** smart loading: cache → Parquet → warm cache automatically
- **StorageClient** Python interface with simplified API

**Performance & Cost**
- **17-19× better compression** across numeric, mixed, and tick data
- **Cache hits ~1ms**, cache misses ~50ms, DuckDB queries ~25-120ms
- **20% cost reduction**: 24 CHF/month vs 30 CHF/month (50GB storage)

**Breaking Changes**
- QuestDB backend completely removed — migration required
- New `StorageBackend` trait with `ParquetBackend`, `CacheBackend`, `DuckDBBackend`
- HTTP `/exec` endpoint for QuestDB SQL removed

**Developer Experience**
- 42 new storage backend tests + integration tests
- Migration guide with step-by-step examples
- Storage demo notebook with benchmarks
- Architecture documentation with diagrams

---

## January 2026

### Polarway v0.52.0 — Adaptive Streaming & Financial Time Series
*Released January 15, 2026*

**Adaptive Streaming Engine** (`polars-streaming-adaptive`)
- Memory-aware chunk sizing based on available system resources
- Adaptive backpressure preventing OOM on memory-constrained systems
- Zero-copy memory mapping for efficient file reading
- Parallel streaming with dynamic thread pool sizing
- Azure-optimized for 4-8GB cloud VMs
- **3-5× faster** than standard streaming on constrained systems
- **50-70% lower memory footprint**

**Financial Time Series Analysis** (`polars-timeseries`)
- **VWAP** (Volume-Weighted Average Price): O(n) complexity, streaming-compatible
- **TWAP** (Time-Weighted Average Price): Fixed window implementation
- **Typical Price**: (High + Low + Close) / 3 calculation
- Lazy evaluation support for query optimization
- Vectorized operations with proper null handling

**Distributed Computing Framework** (`polarway-distributed`)
- Query planner for intelligent work distribution
- etcd-based coordinator for node management and health monitoring
- Distributed cache layer with configurable TTL
- Parallel execution engine with task prioritization
- Fault tolerance with automatic retry logic

**Enterprise Data Sources** (`polarway-sources`)
- REST APIs: Automatic pagination, rate limiting, OAuth2/JWT
- GraphQL: Query batching, cursor-based pagination
- Kafka: At-least-once/exactly-once semantics, offset management
- Redis: Connection pooling, cluster support

**gRPC Server** (`polarway-grpc`)
- Bidirectional streaming for large result sets
- Protocol buffer schema for type safety
- Connection multiplexing and keepalives
- TLS support for secure communication

---

## December 2025

### Polarway v0.51.0 — Streaming Network Sources
*Released December 20, 2025*

- WebSocket streaming source with automatic reconnection
- REST API source with retry logic and rate limiting
- Server-Sent Events (SSE) support
- Kafka consumer source with offset management
- Generic streaming source architecture with pluggable adapters

---

## November 2025

### Polarway v0.50.0 — Functional Programming Patterns
*Released November 10, 2025*

- Expression expansion and list operations
- User-defined functions with Rust/Python interop
- Advanced window functions for time-series
- Fold operations for aggregations
- Struct and nested data manipulation

---

## Earlier Releases

### Polarway v0.49.0 and before
- Core DataFrame operations based on Polars
- Lazy evaluation and query optimization
- Multi-format I/O (Parquet, CSV, JSON, Cloud Storage)
- Python bindings via PyO3
- Basic gRPC client-server architecture

---

## Migration Guides

| From Version | To Version | Guide |
|--------------|------------|-------|
| QuestDB (any) | v0.53.0+ | [QuestDB → Hybrid Storage Migration](../storage.md#migration-from-questdb) |
| v0.52.x | v0.53.0 | [Storage Layer Migration](RELEASE_NOTES_v0.53.0.md#breaking-changes) |
| SQLite Auth | v0.54.0+ | [Lakehouse Migration](../lakehouse.md#migration-from-sqlite) |

---

## Links

- **GitHub Releases**: https://github.com/ThotDjehuty/polarway/releases
- **Documentation**: https://polarway.readthedocs.io/
- **PyPI**: https://pypi.org/project/polarway/
- **Docker Hub**: https://hub.docker.com/r/polarway/polarway-grpc
- **Issues**: https://github.com/ThotDjehuty/polarway/issues

---

*Last updated: August 2026*