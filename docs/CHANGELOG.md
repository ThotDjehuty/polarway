# Changelog

All notable changes to Polaroid will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-01-12

### 🎉 Initial Public Release

First public release of Polaroid - a high-performance DataFrame library with gRPC streaming architecture.

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

[0.1.0]: https://github.com/EnkiNudimmud/polaroid/releases/tag/v0.1.0
