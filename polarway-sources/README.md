# polarway-sources

Network-native data sources for streaming data into Arrow RecordBatches.

## Features

- **WebSocket** — auto-reconnection, configurable backoff
- **REST API** — pagination strategies (offset, cursor, link header)
- **gRPC streaming** — service-to-service communication
- **Connection pooling** — reusable connections with health checks
- **Rate limiting** — governor-based request throttling

## Traits

All sources implement `DataSource` (schema + stream) and optionally `StreamingDataSource` (backpressure + reconnect).

## License

MIT
