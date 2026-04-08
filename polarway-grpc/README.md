# polarway-grpc

gRPC server for Polarway Railway-Oriented Data Processing.

## Features

- **Handle-based DataFrames** — persist and reference DataFrames by external handles
- **Hybrid storage** — LRU cache → Parquet (zstd) → DuckDB tiered architecture
- **REST `/exec` endpoint** — QuestDB-compatible SQL execution over HTTP
- **Arrow Flight IPC** — zero-copy serialization
- **Prometheus metrics** — built-in `/metrics` endpoint

## Quick Start

```bash
cargo run -p polarway-grpc
```

The server binds to `0.0.0.0:50051` (gRPC) and `0.0.0.0:9090` (REST + metrics).

## License

MIT
