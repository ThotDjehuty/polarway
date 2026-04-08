# Installation Guide

This guide covers installation of Polarway for Python, Rust, Docker, and from source.

## Python Installation

### Requirements

- Python 3.9 or later
- pip or conda

### Using pip (Recommended)

```bash
pip install polarway
```

### Using conda

```bash
conda install -c conda-forge polarway
```

### Verify Installation

```python
import polarway as pw

print(pw.__version__)  # Should print: 0.53.0
```

### Optional Dependencies

For distributed mode (gRPC):

```bash
pip install polarway[distributed]
```

For development:

```bash
pip install polarway[dev]
```

All extras:

```bash
pip install polarway[all]
```

## Rust Installation

### Requirements

- Rust 1.75 or later
- Cargo (comes with Rust)

### Add to Cargo.toml

```toml
[dependencies]
polarway = "0.53.0"
```

### Feature Flags

```toml
[dependencies]
polarway = { version = "0.53.0", features = ["distributed", "compression"] }
```

Available features:
- `distributed` - gRPC client/server support
- `compression` - Additional compression algorithms
- `sql` - DuckDB SQL backend
- `cloud` - AWS S3, Azure Blob support
- `full` - All features enabled

### Build and Run

```bash
cargo build --release
cargo run --release
```

## Docker Installation

### Pull Official Image

```bash
docker pull polarway/polarway:latest
```

### Run Polarway Server

```bash
docker run -d \
  --name polarway-server \
  -p 50052:50052 \
  -v /data:/data \
  polarway/polarway:latest \
  server --grpc-port 50052 --data-dir /data
```

### Docker Compose

Create `docker-compose.yml`:

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
    command: server --grpc-port 50052 --data-dir /data
    environment:
      - RUST_LOG=info
      - POLARWAY_CACHE_SIZE=2.0
    restart: unless-stopped

  polarway-ui:
    image: polarway/polarway-ui:latest
    container_name: polarway-ui
    ports:
      - "8501:8501"
    environment:
      - POLARWAY_SERVER=polarway-server:50052
    depends_on:
      - polarway-server
    restart: unless-stopped
```

Start services:

```bash
docker-compose up -d
```

## Installation from Source

### Clone Repository

```bash
git clone https://github.com/ThotDjehuty/polarway.git
cd polarway
```

### Build Rust Core

```bash
# Build in release mode
cargo build --release

# Run tests
cargo test --all

# Install binary
cargo install --path .
```

### Build Python Bindings

```bash
# Install maturin
pip install maturin

# Build Python wheel
maturin develop --release

# Or build wheel for distribution
maturin build --release
pip install target/wheels/polarway-*.whl
```

### Development Setup

```bash
# Create virtual environment
python -m venv venv
source venv/bin/activate  # On Windows: venv\Scripts\activate

# Install dev dependencies
pip install -e ".[dev]"

# Install pre-commit hooks
pre-commit install

# Run tests
pytest tests/
cargo test
```

## Configuration

### Environment Variables

Polarway can be configured via environment variables:

```bash
# Storage paths
export POLARWAY_PARQUET_PATH=/data/cold
export POLARWAY_DUCKDB_PATH=/data/analytics.duckdb

# Cache configuration
export POLARWAY_CACHE_SIZE=2.0  # GB

# gRPC server
export POLARWAY_GRPC_HOST=0.0.0.0
export POLARWAY_GRPC_PORT=50052

# Logging
export RUST_LOG=info
export POLARWAY_LOG_LEVEL=INFO
```

### Configuration File

Create `~/.polarway/config.toml`:

```toml
[storage]
parquet_path = "/data/cold"
duckdb_path = "/data/analytics.duckdb"
cache_size_gb = 2.0

[server]
grpc_host = "0.0.0.0"
grpc_port = 50052
max_connections = 100

[compression]
algorithm = "zstd"
level = 19

[logging]
level = "info"
format = "json"
```

Load configuration:

```python
from polarway import load_config

config = load_config("~/.polarway/config.toml")
client = StorageClient.from_config(config)
```

## Cloud Deployment

### AWS

#### Using EC2

```bash
# Launch EC2 instance (Amazon Linux 2)
aws ec2 run-instances \
  --image-id ami-xxxxxxxxx \
  --instance-type t3.medium \
  --key-name your-key \
  --user-data file://install-polarway.sh

# install-polarway.sh:
#!/bin/bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env
cargo install polarway
polarway server --grpc-port 50052
```

#### Using ECS/Fargate

Task definition `polarway-task.json`:

```json
{
  "family": "polarway-server",
  "containerDefinitions": [{
    "name": "polarway",
    "image": "polarway/polarway:latest",
    "memory": 2048,
    "cpu": 1024,
    "portMappings": [{
      "containerPort": 50052,
      "protocol": "tcp"
    }],
    "environment": [
      {"name": "POLARWAY_CACHE_SIZE", "value": "2.0"},
      {"name": "RUST_LOG", "value": "info"}
    ],
    "mountPoints": [{
      "sourceVolume": "polarway-data",
      "containerPath": "/data"
    }]
  }],
  "volumes": [{
    "name": "polarway-data",
    "efsVolumeConfiguration": {
      "fileSystemId": "fs-xxxxxxxxx"
    }
  }]
}
```

Deploy:

```bash
aws ecs register-task-definition --cli-input-json file://polarway-task.json
aws ecs create-service \
  --cluster your-cluster \
  --service-name polarway \
  --task-definition polarway-server \
  --desired-count 2
```

### Azure

#### Using Azure Container Instances

```bash
az container create \
  --resource-group polarway-rg \
  --name polarway-server \
  --image polarway/polarway:latest \
  --ports 50052 \
  --cpu 2 \
  --memory 4 \
  --environment-variables \
    POLARWAY_CACHE_SIZE=2.0 \
    RUST_LOG=info \
  --azure-file-volume-account-name yourstorageaccount \
  --azure-file-volume-account-key yourkey \
  --azure-file-volume-share-name polarway-data \
  --azure-file-volume-mount-path /data
```

### Google Cloud

#### Using Cloud Run

```bash
# Build and push Docker image
gcloud builds submit --tag gcr.io/your-project/polarway

# Deploy to Cloud Run
gcloud run deploy polarway-server \
  --image gcr.io/your-project/polarway \
  --platform managed \
  --region us-central1 \
  --memory 4Gi \
  --cpu 2 \
  --port 50052 \
  --set-env-vars POLARWAY_CACHE_SIZE=2.0,RUST_LOG=info \
  --allow-unauthenticated
```

## Platform-Specific Notes

### macOS

Install Rust via Homebrew (optional):

```bash
brew install rust
```

If using Apple Silicon (M1/M2):

```bash
# Build for ARM architecture
cargo build --release --target aarch64-apple-darwin
```

### Linux

Install system dependencies:

```bash
# Ubuntu/Debian
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev

# CentOS/RHEL
sudo yum groupinstall "Development Tools"
sudo yum install openssl-devel
```

### Windows

Install Visual Studio Build Tools:

1. Download from https://visualstudio.microsoft.com/downloads/
2. Install "Desktop development with C++"

Or use Windows Subsystem for Linux (WSL2):

```bash
# In WSL2 terminal
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
cargo install polarway
```

## Verification

### Test Python Installation

```python
import polarway as pw

# Create test dataframe
df = pw.DataFrame({
    "symbol": ["BTC", "ETH", "SOL"],
    "price": [50000, 3000, 100]
})

# Test operations
result = (
    pw.Ok(df)
    .map(lambda d: d.filter(pw.col("price") > 1000))
    .map(lambda d: d.select(["symbol", "price"]))
)

print(result)
```

### Test Rust Installation

Create `test.rs`:

```rust
use polarway::prelude::*;

fn main() -> Result<()> {
    let df = df! {
        "symbol" => &["BTC", "ETH", "SOL"],
        "price" => &[50000, 3000, 100],
    }?;

    let result = df
        .filter(col("price").gt(1000))?
        .select(&["symbol", "price"])?;

    println!("{:?}", result);
    Ok(())
}
```

Compile and run:

```bash
rustc test.rs -L target/release/deps -l polarway
./test
```

### Test gRPC Server

Start server:

```bash
polarway server --grpc-port 50052
```

Test connection (Python):

```python
from polarway import DistributedClient

client = DistributedClient(host="localhost", port=50052)
print(client.health_check())  # Should print: OK
```

## Troubleshooting

### Python: "No module named 'polarway'"

```bash
# Check pip installation
pip show polarway

# If not found, reinstall
pip install --force-reinstall polarway
```

### Rust: Compilation errors

```bash
# Update Rust
rustup update

# Clean build cache
cargo clean
cargo build --release
```

### Docker: Port already in use

```bash
# Find process using port 50052
lsof -i :50052

# Kill process or use different port
docker run -p 50053:50052 polarway/polarway:latest
```

### gRPC: Connection refused

```bash
# Check server is running
ps aux | grep polarway

# Check firewall
sudo ufw allow 50052/tcp  # Linux
```

## Next Steps

- 📚 [Getting Started](getting-started.md) - Quick introduction
- 🐍 [Python Client Guide](python-client.md) - Complete Python API
- 🦀 [Rust Client Guide](rust-client.md) - Complete Rust API
- 🌐 [Distributed Mode](distributed-mode.md) - gRPC deployment

---

**Need help?** Open an [issue](https://github.com/ThotDjehuty/polarway/issues)
