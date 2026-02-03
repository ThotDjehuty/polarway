.. Polarway documentation master file

📸 Polarway Documentation
==========================

**High-Performance DataFrame Engine with gRPC Streaming and Hybrid Storage**

Polarway is a fast DataFrame library built on Polars, featuring a gRPC client-server architecture
with hybrid storage (Parquet + DuckDB + Cache) for optimal performance and cost efficiency.

.. image:: https://img.shields.io/badge/License-MIT-yellow.svg
   :target: https://opensource.org/licenses/MIT
   :alt: License

.. image:: https://img.shields.io/badge/rust-1.75%2B-orange.svg
   :target: https://www.rust-lang.org/
   :alt: Rust

.. image:: https://img.shields.io/badge/python-3.9%2B-blue.svg
   :target: https://www.python.org/
   :alt: Python

Features
--------

* 🌐 **Remote Execution**: Process data on powerful servers from any client
* 📊 **Streaming-First**: Handle larger-than-RAM datasets with constant memory usage
* 🚀 **High Performance**: Zero-copy Arrow streaming with async Tokio runtime
* 📈 **Time-Series Native**: Built-in OHLCV resampling and rolling window operations
* 💾 **Hybrid Storage**: Parquet + DuckDB + LRU cache (18× compression, -20% cost)
* 🔌 **Network Sources**: Native WebSocket, REST API, and streaming data support
* 🛡️ **Type Safety**: Rust's Result/Option monads for robust error handling
* 🔄 **Language Agnostic**: Any gRPC-capable language (Python, Rust, Go, TypeScript)

Quick Start
-----------

.. code-block:: bash

   # Install Polarway Python client
   pip install polarway-py

   # Start Polarway server
   docker run -p 50051:50051 polarway/polarway-grpc

   # Use in Python
   import polarway as pd

   # Connect to server
   client = pd.connect("localhost:50051")

   # Create DataFrame
   df = client.from_dict({"x": [1, 2, 3], "y": [4, 5, 6]})

   # Operations
   result = df.filter(pd.col("x") > 1).select(["x", "y"])

   # Convert to Python
   print(result.collect())

Storage Architecture
-------------------

Polarway v0.53.0 introduces a **hybrid storage layer** replacing QuestDB:

.. code-block:: text

   ┌─────────────┐
   │   Request   │
   └──────┬──────┘
          │
          ▼
   ┌─────────────┐  Cache Hit
   │    Cache    │──────────────► Return (fast)
   │ (LRU, RAM)  │
   └──────┬──────┘
          │ Cache Miss
          ▼
   ┌─────────────┐
   │   Parquet   │  Load & Warm Cache
   │ (Cold, zstd)│──────────────► Return
   └──────┬──────┘
          │
          ▼
   ┌─────────────┐
   │   DuckDB    │  SQL Analytics
   │  (Queries)  │
   └─────────────┘

**Benefits**:

* **18× Compression**: zstd level 19 (vs 1.07× QuestDB)
* **-20% Cost**: 24 CHF/month vs 30 CHF/month
* **SQL Analytics**: DuckDB for complex queries
* **Fast Access**: LRU cache for hot data

Table of Contents
-----------------

.. toctree::
   :maxdepth: 2
   :caption: Getting Started

   installation
   quickstart
   examples

.. toctree::
   :maxdepth: 2
   :caption: Architecture

   architecture
   storage
   streaming

.. toctree::
   :maxdepth: 2
   :caption: API Reference

   api/python
   api/rust
   api/grpc

.. toctree::
   :maxdepth: 2
   :caption: Guides

   guides/storage_layer
   guides/streaming
   guides/time_series
   guides/functional_programming

.. toctree::
   :maxdepth: 1
   :caption: Development

   contributing
   changelog
   release_notes

Indices and tables
==================

* :ref:`genindex`
* :ref:`modindex`
* :ref:`search`
