# Polaroid Python Client

Python client library for connecting to Polaroid gRPC server.

## Installation

```bash
# Install from source
cd polaroid-python
pip install -e .

# Generate protocol buffers
./generate_protos.sh
```

## Quick Start

```python
import polaroid as pd

# Connect to server
pd.connect("localhost:50051")

# Read Parquet
df = pd.read_parquet("data.parquet")

# Get info
print(df.shape())  # (1000, 5)
print(df.schema()) # Schema info

# Select columns
df2 = df.select(["col1", "col2"])

# Collect to Arrow Table
table = df2.collect()
print(table)

# Write to Parquet
df2.write_parquet("output.parquet")
```

## Features

- **gRPC Interface**: Network-capable DataFrame operations
- **Arrow IPC**: Zero-copy data transfer
- **Handle-Based**: Efficient server-side operations
- **Streaming**: Process larger-than-memory datasets

## Phase 1 Implementation

Currently implemented:
- ✅ Connection management
- ✅ read_parquet
- ✅ write_parquet
- ✅ select
- ✅ get_schema
- ✅ get_shape
- ✅ collect
- ✅ Handle management (drop, heartbeat)

Coming in Phase 2+:
- filter, group_by, join, sort
- Time-series operations
- Network data sources
