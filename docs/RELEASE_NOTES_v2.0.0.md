# Polarway v2.0.0 — Release Notes

**Release Date:** March 2026  
**Codename:** *Distributed Horizons*  
**Based on:** Polars 0.52.0 (edition 2024)

---

## Highlights

Polarway v2.0.0 is a **major release** that introduces the distributed coordination layer, bridges the gap between single-node and multi-host deployments, and lays the groundwork for pluggable message broker connectors.

### What's new

| Feature | Crate | Status |
|---|---|---|
| Generic Event Bus | `polarway-bus` v0.1.0 | ✅ Shipped |
| Distributed Implementation Plan v2 | `docs/` | ✅ Shipped |
| Connector architecture (Redis, RabbitMQ, Kafka) | `polarway-connectors` | 📋 Planned |
| Lakehouse as distributed state store | `polarway-lakehouse` + `polarway-grpc` | 📋 Planned (Phase 2) |

---

## ✨ New: `polarway-bus` — Generic Event Fan-Out

A domain-agnostic, zero-dependency event bus for inter-component and inter-worker coordination.

### Features

- **Broadcast fan-out** — every subscriber receives every event (via `tokio::sync::broadcast`)
- **Filtered subscribers** — register closure predicates to drop irrelevant events before delivery
- **Sync drain** — batch-drain all pending events without async (`sub.drain()`)
- **Async recv** — `sub.recv().await` for event-loop integration
- **Generic over `T: Clone + Send + Sync + 'static`** — no finance-specific code
- **Railway-oriented** — all fallible operations return `BusResult<T>`

### API

```rust
use polarway_bus::{EventBus, Subscriber, FilteredSubscriber};

// Create a bus with 4096-slot capacity
let bus: EventBus<String> = EventBus::new(4096);

// Unfiltered subscriber
let mut sub = bus.subscribe();

// Filtered subscriber (only events containing "error")
let mut err_sub = bus.subscribe_filtered(|msg: &String| msg.contains("error"));

// Publish
bus.publish("hello world".into())?;
bus.publish("an error occurred".into())?;

// Drain (sync, non-blocking)
let all = sub.drain();        // ["hello world", "an error occurred"]
let errs = err_sub.drain();   // ["an error occurred"]

// Metrics
bus.event_count();       // 2
bus.subscriber_count();  // 2
```

### Tests

- 10 unit tests + 2 doc-tests — all passing
- Covers: publish/subscribe, filtered delivery, drain semantics, no-subscriber error, concurrent access

---

## 📐 Updated: Distributed Implementation Plan

The [DISTRIBUTED_IMPLEMENTATION_PLAN.md](../docs/DISTRIBUTED_IMPLEMENTATION_PLAN.md) has been rewritten to integrate the new crates as **retained solutions**:

### Key decisions

1. **`polarway-bus` replaces "Optional Coordinator"** — the EventBus is now the concrete inter-worker coordination primitive (heartbeats, handle invalidation, job assignment)

2. **`polarway-lakehouse::DeltaStore` replaces raw S3/blob for Phase 2+** — ACID writes, time-travel, vacuum/GC, audit logging, auth/RBAC all come for free via the existing Lakehouse crate

3. **`polarway-connectors` (new crate, planned)** — trait-based `BusConnector` adapters bridge the local EventBus to Redis, RabbitMQ, or Kafka for multi-host event distribution

4. **Contributor-friendly extensibility** — adding a new broker requires implementing a single 4-method trait (`BusConnector`). A full example (NATS) is provided in the plan.

### Phase roadmap

| Phase | Scope | Key deliverable |
|---|---|---|
| 0 | Single node (dev loop) | Validate API correctness |
| 1 | Multi-container, single host | `EventBus<WorkerEvent>` + filesystem state store |
| 2 | Multi-host with Lakehouse | `DeltaStore` backend + `polarway-connectors` |
| 3 | Production hardening | Auth, metrics, autoscaling, lifecycle |

---

## 🏗️ Existing Stable Components (unchanged)

| Crate | Version | Description |
|---|---|---|
| `polarway-grpc` | 1.0.0 | gRPC server, handle-based DataFrames, hybrid LRU→Parquet→DuckDB storage, REST `/exec`, Prometheus metrics |
| `polarway-lakehouse` | 0.1.1 | Delta Lake ACID, time-travel, JWT auth, Argon2, RBAC, audit, vacuum, z-order |
| `polarway-sources` | 0.1.0 | WebSocket (auto-reconnect), REST (pagination), gRPC streaming, connection pooling, rate limiting |
| `polarway-distributed` | 0.1.0 | Query planner, etcd coordinator, executor, cache *(excluded from default build — awaits arrow 54+)* |
| `polars-streaming-adaptive` | 0.1.0 | Memory-mapped Parquet, adaptive chunk sizing, parallel streaming (3-5x faster) |
| `polars-timeseries` | 0.1.0 | VWAP, TWAP, typical price, multi-frequency resampling |

---

## ⚠️ Known Issues

- **`polarway-distributed` excluded**: `arrow-arith@53.4.0` × `chrono@0.4.39+` conflict. Fix: upgrade arrow to 54+ when DataFusion supports it.
- **crates.io publication deferred**: path dependencies on local Polars fork prevent `cargo publish`. Architecture change planned for v2.1.0.
- **pyarrow compatibility**: `pyarrow >= 21` binary incompatibility with `deltalake`. Use `pyarrow == 18.1.0`.

---

## 📋 Roadmap (v2.1.0+)

1. **`polarway-connectors` crate** — Redis, RabbitMQ, Kafka `BusConnector` implementations
2. **`StateStore` trait** — abstract handle storage; `DeltaStore` adapter in `polarway-lakehouse`
3. **`WorkerEvent` protocol** — `HandleCreated`, `HandleInvalidated`, `WorkerHeartbeat`, `JobAssigned`
4. **Arrow 54+ upgrade** — unblocks `polarway-distributed` and crates.io publishing
5. **Integration tests** — multi-worker + connector + lakehouse end-to-end

---

## 🔗 Links

- Repository: https://github.com/ThotDjehuty/polarway
- Documentation: https://polarway.readthedocs.io
- PyPI: https://pypi.org/project/polarway/ (release pending)
- License: MIT
