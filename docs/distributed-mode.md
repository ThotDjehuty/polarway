# Distributed Mode: gRPC

Deploy Polarway in distributed mode for remote execution, horizontal scaling, and multi-language client support.

## Overview

Polarway's **distributed mode** uses gRPC to provide:

- 🌐 **Remote Execution**: Execute data operations on a remote server
- 📈 **Horizontal Scaling**: Deploy multiple workers behind a load balancer
- 🔀 **Multi-Language Support**: Python, Rust, Go, TypeScript clients
- 💾 **Shared State**: External handle store for distributed workloads
- 🚀 **High Performance**: Arrow IPC for zero-copy serialization

## Architecture

| Layer | Component | Description |
|-------|-----------|-------------|
| **Clients** | Python, Rust, Go, TypeScript | |
| ↓ | gRPC (HTTP/2) | |
| **Load Balancer** | nginx, Envoy, HAProxy | |
| ↓ | ↓ | ↓ |
| **Polarway Worker 1** | gRPC Server | Independent worker |
| **Polarway Worker 2** | gRPC Server | Independent worker |
| **Polarway Worker 3** | gRPC Server | Independent worker |
| ↓ | ↓ | ↓ |
| **Shared Storage / State Store** | Parquet + DuckDB + Cache + Handles | |

## Server Setup

### 1. Standalone Server (Development)

```bash
# Start single gRPC server
polarway server \
  --grpc-port 50052 \
  --data-dir /data \
  --cache-size 2.0
```

**Environment variables:**

```bash
export POLARWAY_BIND_ADDRESS=0.0.0.0:50052
export POLARWAY_PARQUET_PATH=/data/cold
export POLARWAY_DUCKDB_PATH=/data/analytics.duckdb
export POLARWAY_CACHE_SIZE=2.0
export RUST_LOG=info
```

### 2. Docker Deployment

**Single container:**

```bash
docker run -d \
  --name polarway-server \
  -p 50052:50052 \
  -v /data:/data \
  -e POLARWAY_CACHE_SIZE=2.0 \
  -e RUST_LOG=info \
  polarway/polarway:latest \
  server --grpc-port 50052 --data-dir /data
```

**Docker Compose:**

```yaml
version: '3.8'

services:
  polarway-server:
    image: polarway/polarway:latest
    container_name: polarway-server
    ports:
      - "50052:50052"
    volumes:
      - ./data:/data
    environment:
      - POLARWAY_BIND_ADDRESS=0.0.0.0:50052
      - POLARWAY_CACHE_SIZE=2.0
      - RUST_LOG=info
    command: server --grpc-port 50052 --data-dir /data
    restart: unless-stopped
```

Start:

```bash
docker-compose up -d
```

### 3. Distributed Setup (Production)

**Multiple workers with shared state:**

```bash
# Worker 1
docker run -d \
  --name polarway-worker-1 \
  -p 50052:50052 \
  -v /shared/data:/data \
  -v /shared/state:/state \
  -e POLARWAY_BIND_ADDRESS=0.0.0.0:50052 \
  -e POLARWAY_HANDLE_STORE=external \
  -e POLARWAY_STATE_DIR=/state \
  -e RUST_LOG=info \
  polarway/polarway:latest \
  server --grpc-port 50052 --data-dir /data

# Worker 2
docker run -d \
  --name polarway-worker-2 \
  -p 50053:50052 \
  -v /shared/data:/data \
  -v /shared/state:/state \
  -e POLARWAY_BIND_ADDRESS=0.0.0.0:50052 \
  -e POLARWAY_HANDLE_STORE=external \
  -e POLARWAY_STATE_DIR=/state \
  -e RUST_LOG=info \
  polarway/polarway:latest \
  server --grpc-port 50052 --data-dir /data

# Worker 3
docker run -d \
  --name polarway-worker-3 \
  -p 50054:50052 \
  -v /shared/data:/data \
  -v /shared/state:/state \
  -e POLARWAY_BIND_ADDRESS=0.0.0.0:50052 \
  -e POLARWAY_HANDLE_STORE=external \
  -e POLARWAY_STATE_DIR=/state \
  -e RUST_LOG=info \
  polarway/polarway:latest \
  server --grpc-port 50052 --data-dir /data
```

**Docker Compose (multi-worker):**

```yaml
version: '3.8'

services:
  polarway-lb:
    image: nginx:alpine
    container_name: polarway-lb
    ports:
      - "50052:50052"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
    depends_on:
      - worker-1
      - worker-2
      - worker-3
    restart: unless-stopped

  worker-1:
    image: polarway/polarway:latest
    volumes:
      - shared-data:/data
      - shared-state:/state
    environment:
      - POLARWAY_BIND_ADDRESS=0.0.0.0:50052
      - POLARWAY_HANDLE_STORE=external
      - POLARWAY_STATE_DIR=/state
      - RUST_LOG=info
    command: server --grpc-port 50052 --data-dir /data

  worker-2:
    image: polarway/polarway:latest
    volumes:
      - shared-data:/data
      - shared-state:/state
    environment:
      - POLARWAY_BIND_ADDRESS=0.0.0.0:50052
      - POLARWAY_HANDLE_STORE=external
      - POLARWAY_STATE_DIR=/state
      - RUST_LOG=info
    command: server --grpc-port 50052 --data-dir /data

  worker-3:
    image: polarway/polarway:latest
    volumes:
      - shared-data:/data
      - shared-state:/state
    environment:
      - POLARWAY_BIND_ADDRESS=0.0.0.0:50052
      - POLARWAY_HANDLE_STORE=external
      - POLARWAY_STATE_DIR=/state
      - RUST_LOG=info
    command: server --grpc-port 50052 --data-dir /data

volumes:
  shared-data:
  shared-state:
```

**nginx.conf (gRPC load balancer):**

```nginx
events {
    worker_connections 1024;
}

http {
    upstream polarway_backend {
        server worker-1:50052;
        server worker-2:50052;
        server worker-3:50052;
    }

    server {
        listen 50052 http2;

        location / {
            grpc_pass grpc://polarway_backend;
            grpc_set_header Host $host;
            grpc_read_timeout 300s;
            grpc_send_timeout 300s;
        }
    }
}
```

## Client Usage

### Python Client

```python
from polarway import DistributedClient

# Connect to server
client = DistributedClient(
    host="polarway-server.example.com",
    port=50052,
    timeout=30
)

# Health check
health = client.health_check()
print(f"Server status: {health}")

# Store data
import polars as pl

df = pl.DataFrame({
    "symbol": ["BTC", "ETH", "SOL"],
    "price": [50000, 3000, 100]
})

client.store("trades_20260203", df)

# Load data
loaded = client.load("trades_20260203")
print(loaded)

# Execute query
result = client.query("""
    SELECT symbol, AVG(price) as avg_price
    FROM parquet_scan('/data/cold/trades_*.parquet')
    WHERE timestamp > '2026-02-01'
    GROUP BY symbol
""")

print(result)
```

### Rust Client

```rust
use polarway::DistributedClient;

#[tokio::main]
async fn main() -> Result<()> {
    // Connect to server
    let client = DistributedClient::connect("http://localhost:50052").await?;
    
    // Health check
    client.health_check().await?;
    
    // Store data
    let df = df! {
        "symbol" => &["BTC", "ETH", "SOL"],
        "price" => &[50000, 3000, 100],
    }?;
    
    client.store("trades_20260203", &df).await?;
    
    // Load data
    let loaded = client.load("trades_20260203").await?;
    println!("{:?}", loaded);
    
    // Execute query
    let result = client.query(
        "SELECT symbol, AVG(price) as avg_price
         FROM parquet_scan('/data/cold/trades_*.parquet')
         WHERE timestamp > '2026-02-01'
         GROUP BY symbol"
    ).await?;
    
    println!("{:?}", result);
    
    Ok(())
}
```

### TypeScript Client

```typescript
import { PolarwayClient } from '@polarway/client';

// Connect to server
const client = new PolarwayClient({
  host: 'localhost',
  port: 50052,
  timeout: 30000
});

// Health check
const health = await client.healthCheck();
console.log(`Server status: ${health}`);

// Store data
await client.store('trades_20260203', {
  symbol: ['BTC', 'ETH', 'SOL'],
  price: [50000, 3000, 100]
});

// Load data
const loaded = await client.load('trades_20260203');
console.log(loaded);

// Execute query
const result = await client.query(`
  SELECT symbol, AVG(price) as avg_price
  FROM parquet_scan('/data/cold/trades_*.parquet')
  WHERE timestamp > '2026-02-01'
  GROUP BY symbol
`);

console.log(result);
```

## Handle Store Modes

### In-Memory Mode (Default)

**Use case:** Single server, non-distributed

```bash
export POLARWAY_HANDLE_STORE=memory
```

- Fast: handles stored in RAM
- Not distributed-safe: operations must complete on same worker
- Suitable for development and single-node deployments

### External Mode (Distributed)

**Use case:** Multiple workers, load balancing

```bash
export POLARWAY_HANDLE_STORE=external
export POLARWAY_STATE_DIR=/shared/state
```

- Distributed-safe: any worker can serve follow-up operations
- Handles persisted to shared storage
- Required for multi-worker deployments

**Shared storage options:**

1. **Network filesystem (NFS/SMB):**
```bash
# Mount on all workers
mount -t nfs server:/export/polarway-state /state
```

2. **Object store (S3/Azure Blob):**
```bash
export POLARWAY_HANDLE_STORE=s3
export POLARWAY_S3_BUCKET=polarway-state
export AWS_REGION=us-east-1
```

3. **Docker volume (single-host):**
```yaml
volumes:
  shared-state:
    driver: local
```

## Load Balancing Strategies

### Round Robin (Default)

nginx configuration:

```nginx
upstream polarway_backend {
    server worker-1:50052;
    server worker-2:50052;
    server worker-3:50052;
}
```

**Pros:**
- Simple
- Even distribution

**Cons:**
- No affinity
- Repeated reads from external store

### Least Connections

```nginx
upstream polarway_backend {
    least_conn;
    server worker-1:50052;
    server worker-2:50052;
    server worker-3:50052;
}
```

**Pros:**
- Better for variable workloads
- Balances load automatically

### IP Hash (Sticky Sessions)

```nginx
upstream polarway_backend {
    ip_hash;
    server worker-1:50052;
    server worker-2:50052;
    server worker-3:50052;
}
```

**Pros:**
- Client affinity
- Reduces external store reads (cache locality)

**Cons:**
- Uneven distribution if clients are few

## Performance Optimization

### 1. Caching Strategy

**Worker-local cache:**

```bash
export POLARWAY_CACHE_SIZE=2.0  # 2GB per worker
```

Benefits:
- Fast cache hits (<1ms)
- Reduced external store reads
- Better with sticky sessions

### 2. Parallel Requests

**Python:**

```python
import asyncio
from polarway import AsyncDistributedClient

async def load_all():
    client = AsyncDistributedClient(host="localhost", port=50052)
    
    # Parallel loads
    results = await asyncio.gather(
        client.load("trades_20260203"),
        client.load("quotes_20260203"),
        client.load("orders_20260203")
    )
    
    return results

results = asyncio.run(load_all())
```

**Rust:**

```rust
use tokio::try_join;

let (trades, quotes, orders) = try_join!(
    client.load("trades_20260203"),
    client.load("quotes_20260203"),
    client.load("orders_20260203")
)?;
```

### 3. Batching

```python
# Batch store (single RPC)
client.store_batch({
    "trades_20260203": trades_df,
    "quotes_20260203": quotes_df,
    "orders_20260203": orders_df
})
```

### 4. Streaming Large Results

```python
# Stream large result in chunks
for batch in client.load_stream("huge_dataset"):
    process_batch(batch)
    # Memory freed after each iteration
```

## Security

### TLS/SSL Configuration

**Server:**

```bash
polarway server \
  --grpc-port 50052 \
  --tls-cert /certs/server.crt \
  --tls-key /certs/server.key \
  --tls-ca /certs/ca.crt
```

**Client (Python):**

```python
import grpc

credentials = grpc.ssl_channel_credentials(
    root_certificates=open('/certs/ca.crt', 'rb').read(),
    private_key=open('/certs/client.key', 'rb').read(),
    certificate_chain=open('/certs/client.crt', 'rb').read()
)

client = DistributedClient(
    host="polarway-server.example.com",
    port=50052,
    credentials=credentials
)
```

### Token Authentication

**Server:**

```bash
export POLARWAY_AUTH_TOKEN=your-secret-token
```

**Client:**

```python
client = DistributedClient(
    host="polarway-server.example.com",
    port=50052,
    auth_token="your-secret-token"
)
```

## Monitoring & Observability

### Health Checks

```python
# Check server health
health = client.health_check()
print(health)  # "OK" or error
```

### Metrics

Key metrics to track:

- **Requests/sec**: `polarway_grpc_requests_total`
- **Latency**: `polarway_grpc_request_duration_seconds`
- **Error rate**: `polarway_grpc_errors_total`
- **Cache hit rate**: `polarway_cache_hit_rate`
- **External store I/O**: `polarway_state_store_bytes_read/written`

**Prometheus export:**

```bash
export POLARWAY_METRICS_ENABLED=true
export POLARWAY_METRICS_PORT=9090
```

**Grafana dashboard:**

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: polarway-dashboard
data:
  dashboard.json: |
    {
      "dashboard": {
        "title": "Polarway gRPC Metrics",
        "panels": [
          {
            "title": "Requests/sec",
            "targets": [
              {
                "expr": "rate(polarway_grpc_requests_total[1m])"
              }
            ]
          },
          {
            "title": "P95 Latency",
            "targets": [
              {
                "expr": "histogram_quantile(0.95, polarway_grpc_request_duration_seconds_bucket)"
              }
            ]
          }
        ]
      }
    }
```

## Troubleshooting

### Connection Refused

```bash
# Check server is running
docker ps | grep polarway

# Check logs
docker logs polarway-server

# Test connectivity
grpcurl -plaintext localhost:50052 list
```

### Slow Queries

```python
# Enable query timing
client = DistributedClient(host="localhost", port=50052, timeout=300)

# Check cache stats
stats = client.cache_stats()
print(f"Hit rate: {stats['hit_rate']:.1%}")
```

### External Store Errors

```bash
# Verify shared storage is mounted
df -h | grep /state

# Check permissions
ls -la /state

# Test write access
touch /state/test && rm /state/test
```

## Best Practices

1. **Use external handle store** for multi-worker deployments
2. **Enable sticky sessions** to maximize cache hit rate
3. **Set appropriate timeouts** (30-300s depending on workload)
4. **Monitor cache hit rate** (target 85%+)
5. **Implement retry logic** in clients for transient errors
6. **Use TLS in production** for secure communication
7. **Track metrics** for performance and debugging
8. **Set resource limits** (max payload size, concurrency)

## Next Steps

- 💡 [Examples](examples.md) - Real-world distributed use cases
- 🐍 [Python Client](python-client.md) - Python API reference
- 🦀 [Rust Client](rust-client.md) - Rust API reference

---

**Need help?** Open an [issue](https://github.com/ThotDjehuty/polarway/issues)
