# Polaroid Docker Deployment

## Quick Start

### 1. Build and Run Polaroid gRPC Server
```bash
# Build the Docker image
docker-compose -f docker-compose.polaroid.yml build

# Start the server
docker-compose -f docker-compose.polaroid.yml up -d

# Check logs
docker-compose -f docker-compose.polaroid.yml logs -f polaroid-grpc

# Check health
docker ps | grep polaroid
```

### 2. Install Polaroid Python Client
```bash
# In your notebook environment
pip install -e polaroid-python/
```

### 3. Run Tests
Open `notebooks/phase2_operations_test.ipynb` and run all cells.

The notebook will connect to the Dockerized server at `localhost:50051`.

## Architecture

```
┌──────────────────────┐
│  Jupyter Notebook    │
│  (Your Machine)      │
│                      │
│  import polaroid     │
│  pld.connect()       │
└──────────┬───────────┘
           │ gRPC :50051
           │ Arrow IPC
           ▼
┌──────────────────────┐
│  Docker Container    │
│  polaroid-grpc       │
│                      │
│  Rust + Polars       │
│  gRPC Server         │
└──────────────────────┘
```

## Environment Variables

```bash
export POLAROID_GRPC_HOST=localhost  # or Docker service name
export POLAROID_GRPC_PORT=50051
export TEST_DATA_DIR=/tmp
```

## Docker Network (Multi-Container)

If running Jupyter in Docker:

```yaml
services:
  jupyter:
    image: jupyter/scipy-notebook
    networks:
      - polaroid-net
    environment:
      - POLAROID_GRPC_HOST=polaroid-grpc  # Use service name
  
  polaroid-grpc:
    # ... same as docker-compose.polaroid.yml
    networks:
      - polaroid-net

networks:
  polaroid-net:
    driver: bridge
```

## Production Deployment

```bash
# Build optimized image
docker build -f Dockerfile.polaroid -t polaroid-grpc:latest .

# Run with resource limits
docker run -d \
  --name polaroid-grpc \
  -p 50051:50051 \
  --memory=4g \
  --cpus=2 \
  -e RUST_LOG=info \
  polaroid-grpc:latest
```

## Troubleshooting

### Server not starting
```bash
# Check logs
docker logs polaroid-grpc

# Restart
docker-compose -f docker-compose.polaroid.yml restart
```

### Connection refused
```bash
# Check server is listening
docker exec polaroid-grpc netcat -zv localhost 50051

# Check port mapping
docker port polaroid-grpc

# Check from host
nc -zv localhost 50051
```

### Import error
```bash
# Ensure polaroid client is installed
pip install -e polaroid-python/

# Verify
python -c "import polaroid; print(polaroid.__version__)"
```

## Stop and Clean Up

```bash
# Stop server
docker-compose -f docker-compose.polaroid.yml down

# Remove volumes
docker-compose -f docker-compose.polaroid.yml down -v
```
