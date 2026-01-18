# Polaroid Multi-Mode Architecture Plan
**Date**: 2026-01-18

---

## 🎯 Goal

Reorganize Polaroid to support 3 deployment modes:
1. **Standalone**: PyO3 Python wheel (local, native)
2. **Distributed**: gRPC polaroid-connect (multi-node)  
3. **Portable**: WASM/serverless (client-side)

---

## 🏗️ Current Polaroid Architecture

```
polaroid/
├── src/
│   ├── lib.rs              # Main library
│   ├── dataframe/          # DataFrame engine
│   ├── grpc_service/       # gRPC server (distributed mode)
│   └── lazy/               # Lazy evaluation
├── Cargo.toml              # Rust dependencies
└── docker-compose.yml      # gRPC deployment
```

**Current Mode**: Distributed only (gRPC)

---

## 🔄 New Architecture

```
polaroid/
├── crates/
│   ├── polaroid-core/      # Core DataFrame engine (shared)
│   │   ├── src/
│   │   │   ├── dataframe.rs
│   │   │   ├── lazy.rs
│   │   │   ├── arrow_ops.rs
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   │
│   ├── polaroid-py/        # 🆕 Standalone: PyO3 bindings
│   │   ├── src/
│   │   │   ├── lib.rs      # PyO3 wrapper
│   │   │   ├── dataframe.rs
│   │   │   └── conversions.rs
│   │   ├── Cargo.toml      # dependencies: pyo3, polaroid-core
│   │   └── pyproject.toml  # Python wheel config
│   │
│   ├── polaroid-connect/   # Distributed: gRPC server
│   │   ├── src/
│   │   │   ├── server.rs
│   │   │   ├── service.rs
│   │   │   └── main.rs
│   │   └── Cargo.toml      # dependencies: tonic, polaroid-core
│   │
│   └── polaroid-wasm/      # 🆕 Portable: WASM module
│       ├── src/
│       │   ├── lib.rs      # wasm-bindgen exports
│       │   └── ops.rs
│       └── Cargo.toml      # dependencies: wasm-bindgen, polaroid-core
│
├── python/                  # Python client library
│   ├── polaroid/
│   │   ├── __init__.py     # Unified API
│   │   ├── standalone.py   # PyO3 import
│   │   ├── distributed.py  # gRPC client
│   │   └── portable.py     # WASM/PyArrow fallback
│   └── setup.py
│
├── Cargo.toml              # Workspace config
├── pyproject.toml          # Python package config
└── docker-compose.yml      # Distributed deployment
```

---

## 📦 Crate Structure

### 1. polaroid-core (Shared Library)

**Purpose**: Core DataFrame engine used by all modes

**Dependencies**:
```toml
[dependencies]
polars = "0.36"
arrow = "49.0"
serde = { version = "1.0", features = ["derive"] }
```

**Exports**:
```rust
pub struct DataFrame { ... }
pub struct LazyFrame { ... }
pub trait DataFrameOps { ... }
```

**Features**: Pure Rust, no Python/gRPC/WASM dependencies

---

### 2. polaroid-py (Standalone Mode)

**Purpose**: PyO3 bindings for pip install polaroid

**Dependencies**:
```toml
[dependencies]
polaroid-core = { path = "../polaroid-core" }
pyo3 = { version = "0.20", features = ["extension-module"] }
numpy = "0.20"

[lib]
crate-type = ["cdylib"]  # Python extension
```

**Python API**:
```python
import polaroid as pl

# Polars-compatible API
df = pl.read_parquet("data.parquet")
df = df.filter(pl.col("price") > 100)
df = df.select(["symbol", "price"])
result = df.collect()  # Execute lazy query
```

**Install**:
```bash
pip install polaroid
```

---

### 3. polaroid-connect (Distributed Mode)

**Purpose**: gRPC server for multi-node deployment

**Dependencies**:
```toml
[dependencies]
polaroid-core = { path = "../polaroid-core" }
tonic = "0.10"
prost = "0.12"
tokio = { version = "1", features = ["full"] }

[[bin]]
name = "polaroid-connect"
```

**Start Server**:
```bash
polaroid-connect --host 0.0.0.0 --port 50052
```

**Docker**:
```bash
docker-compose up polaroid-connect
```

**Python Client**:
```python
import polaroid as pl

# Connect to remote server
pl.connect("grpc://localhost:50052")

df = pl.read_parquet("s3://bucket/data.parquet")
result = df.filter(pl.col("date") > "2024-01-01").collect()
```

---

### 4. polaroid-wasm (Portable Mode)

**Purpose**: WASM module for serverless/browser

**Dependencies**:
```toml
[dependencies]
polaroid-core = { path = "../polaroid-core" }
wasm-bindgen = "0.2"
serde-wasm-bindgen = "0.6"
js-sys = "0.3"

[lib]
crate-type = ["cdylib"]  # WASM output
```

**Build**:
```bash
wasm-pack build --target web --release
```

**Usage (Browser)**:
```javascript
import init, { read_parquet, filter } from './polaroid_wasm.js';

await init();
let df = read_parquet(arrayBuffer);
let filtered = filter(df, "price > 100");
```

**Usage (Python)**:
```python
import polaroid as pl

# Automatically uses WASM backend for small data
df = pl.read_parquet("small_data.parquet")  
```

---

## 🔀 Unified Python API

### python/polaroid/__init__.py

```python
"""
Polaroid - High-performance DataFrame library

Modes:
- Standalone: Native PyO3 (default if installed)
- Distributed: gRPC multi-node
- Portable: PyArrow fallback (always available)
"""

from .router import get_backend, set_backend_mode, BackendMode
from .api import (
    read_parquet,
    read_csv,
    DataFrame,
    LazyFrame,
    col,
)

__all__ = [
    'read_parquet',
    'read_csv',
    'DataFrame',
    'LazyFrame',
    'col',
    'get_backend',
    'set_backend_mode',
    'BackendMode',
]
```

### Backend Selection Logic

```python
# python/polaroid/router.py

def get_backend():
    """Auto-detect best backend."""
    
    # 1. Try PyO3 (standalone)
    try:
        import polaroid._native  # PyO3 module
        return 'standalone'
    except ImportError:
        pass
    
    # 2. Try gRPC (distributed)
    if _check_grpc_available():
        return 'distributed'
    
    # 3. Fallback to portable (PyArrow)
    return 'portable'
```

---

## 🛠️ Implementation Steps

### Phase 1: Core Refactoring (2-3 days)

1. ✅ Create workspace structure
   ```bash
   cargo new --lib crates/polaroid-core
   cargo new --lib crates/polaroid-py
   cargo new --bin crates/polaroid-connect
   cargo new --lib crates/polaroid-wasm
   ```

2. ✅ Extract core engine to polaroid-core
   - Move DataFrame/LazyFrame to core
   - Remove gRPC-specific code
   - Keep only Arrow/Polars dependencies

3. ✅ Update Cargo.toml workspace
   ```toml
   [workspace]
   members = [
       "crates/polaroid-core",
       "crates/polaroid-py",
       "crates/polaroid-connect",
       "crates/polaroid-wasm",
   ]
   ```

### Phase 2: PyO3 Bindings (2-3 days)

1. Create PyO3 wrapper (polaroid-py)
   ```rust
   use pyo3::prelude::*;
   use polaroid_core::DataFrame as CoreDataFrame;
   
   #[pyclass]
   struct DataFrame {
       inner: CoreDataFrame,
   }
   
   #[pymethods]
   impl DataFrame {
       fn filter(&self, expr: &str) -> PyResult<Self> {
           let filtered = self.inner.filter(parse_expr(expr))?;
           Ok(DataFrame { inner: filtered })
       }
   }
   ```

2. Build Python wheel
   ```bash
   cd crates/polaroid-py
   maturin develop  # For development
   maturin build --release  # For distribution
   ```

3. Test Polars compatibility
   ```python
   import polaroid as pl
   
   # Should work like Polars
   df = pl.read_parquet("data.parquet")
   result = df.filter(pl.col("price") > 100).collect()
   ```

### Phase 3: gRPC Refactoring (1-2 days)

1. Move gRPC server to polaroid-connect
   - Use polaroid-core for engine
   - Keep only gRPC service code

2. Update proto definitions
   ```protobuf
   service PolaroidConnect {
       rpc ReadParquet(ReadRequest) returns (DataFrame);
       rpc Filter(FilterRequest) returns (DataFrame);
       rpc Collect(CollectRequest) returns (ArrowBatch);
   }
   ```

3. Test distributed mode
   ```bash
   # Start server
   cargo run --bin polaroid-connect
   
   # Test from Python
   python -c "import polaroid; pl.connect('grpc://localhost:50052')"
   ```

### Phase 4: WASM Module (2-3 days)

1. Create WASM bindings (polaroid-wasm)
   ```rust
   use wasm_bindgen::prelude::*;
   use polaroid_core::DataFrame;
   
   #[wasm_bindgen]
   pub fn read_parquet(bytes: &[u8]) -> JsValue {
       let df = DataFrame::from_parquet_bytes(bytes).unwrap();
       serde_wasm_bindgen::to_value(&df).unwrap()
   }
   ```

2. Build WASM module
   ```bash
   cd crates/polaroid-wasm
   wasm-pack build --target web --release
   ```

3. Test in browser
   ```javascript
   const df = await polaroid.read_parquet(buffer);
   console.log(df.shape());
   ```

### Phase 5: Python API Unification (2 days)

1. Create unified API (python/polaroid/)
   - Auto-detect backend
   - Consistent API across all modes
   - Smart fallback logic

2. Test all modes
   ```python
   # Test standalone
   os.environ['POLAROID_MODE'] = 'standalone'
   df = pl.read_parquet("data.parquet")
   
   # Test distributed
   os.environ['POLAROID_MODE'] = 'distributed'
   df = pl.read_parquet("data.parquet")
   
   # Test portable
   os.environ['POLAROID_MODE'] = 'portable'
   df = pl.read_parquet("data.parquet")
   ```

---

## 📊 Deployment Guide

### Standalone Mode (Development)

```bash
# Install wheel
pip install polaroid

# Use locally
python -c "import polaroid as pl; df = pl.read_parquet('data.parquet')"
```

### Distributed Mode (Production)

```bash
# Start server
docker-compose up polaroid-connect

# Connect from client
export POLAROID_MODE=distributed
python app.py
```

### Portable Mode (Serverless)

```bash
# No server needed
export POLAROID_MODE=portable
python app.py  # Uses PyArrow
```

---

## 🎯 Success Criteria

- ✅ Single codebase, 3 deployment modes
- ✅ Polars API compatibility (standalone mode)
- ✅ Existing gRPC functionality preserved
- ✅ WASM module <200KB
- ✅ Zero breaking changes to current users
- ✅ Automatic backend selection
- ✅ pip install polaroid (standalone)
- ✅ docker-compose up (distributed)
- ✅ Browser/serverless support (portable)

---

**Timeline**: 2-3 weeks  
**Effort**: High (major refactoring)  
**Impact**: Massive (enables all 3 use cases)  
**Risk**: Medium (careful testing needed)

---

**Next Action**: Start Phase 1 (core refactoring) ✅
