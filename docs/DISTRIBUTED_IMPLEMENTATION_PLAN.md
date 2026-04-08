# Polarway Distributed Workload — Implementation Plan

Date: 2026-01-13 &nbsp;|&nbsp; **Updated: 2026-03-29** (EventBus + Lakehouse + Connectors integration)

Branch note: external (stateless) handle scaffolding exists on `feat/stateless-handles-objectstore`.

---

## 0) Goal (What "distributed Polarway" means)

Polarway becomes horizontally scalable by running **multiple gRPC workers** behind a **load balancer** while keeping **handle state external** (so any worker can serve subsequent requests for a handle).

Minimum viable distributed semantics:
- A request that produces a DataFrame persists it to a shared store and returns an **external handle** (e.g. `ext:fs:<uuid>` today).
- Any subsequent operation that references that handle can be served by any worker by loading the DataFrame from the shared store.

Non-goals (for the minimal-effort path):
- Distributed query planning / DAG scheduling (deferred — `polarway-distributed` crate exists for later)
- Cross-worker shared cache consistency
- Exactly-once execution (we aim for idempotent operations + retries)

---

## 1) Architecture — Retained Solutions

### 1.1) Component Map

```text
┌──────────────────────────────────────────────────────────────────┐
│                         Clients                                  │
│  (Python SDK / notebook / job runner)                            │
└─────────────────────────┬────────────────────────────────────────┘
                          │  gRPC / REST
                          ▼
┌──────────────────────────────────────────────────────────────────┐
│              L7 Load Balancer (HTTP/2 gRPC)                      │
└─────────────────────────┬────────────────────────────────────────┘
                          │
          ┌───────────────┴────────────────┐
          ▼                                ▼
┌──────────────────┐            ┌──────────────────┐
│  Polarway Worker │            │  Polarway Worker │  ... × N
│  (polarway-grpc) │            │  (polarway-grpc) │
│                  │◀──bus──────│                  │
└────────┬─────────┘            └────────┬─────────┘
         │                               │
         │  ┌─────────────────────────┐  │
         └──│    polarway-bus         │──┘
            │  EventBus<WorkerEvent>  │  ← inter-worker coordination
            └─────────┬───────────────┘
                      │ publish / subscribe
                      ▼
         ┌────────────────────────────┐
         │   External State Store     │
         ├────────────────────────────┤
         │ Phase 0–1: shared FS      │  Arrow IPC on NFS/hostPath
         │ Phase 2+ : DeltaStore     │  polarway-lakehouse (ACID)
         └────────────────────────────┘
                      │
         ┌────────────┴───────────────┐
         │   Optional Connectors      │
         │  (polarway-connectors)     │
         │  Redis │ RabbitMQ │ Kafka  │  ← trait-based, pluggable
         └────────────────────────────┘
```

### 1.2) Retained Components (concrete crates)

| Component | Crate | Role |
|---|---|---|
| **Inter-worker event bus** | `polarway-bus` | Generic `EventBus<T>` — broadcast fan-out, filtered subscribers, sync drain. Used for worker heartbeats, handle invalidation, job assignment. |
| **Object store backend** | `polarway-lakehouse` | `DeltaStore` as the Phase 2+ state store — ACID writes, time-travel, vacuum/GC, audit logging. Replaces raw S3/blob approach. |
| **Network sources** | `polarway-sources` | `DataSource` / `StreamingDataSource` traits, connection pooling, rate limiting. Foundation for external connectors. |
| **External connectors** | `polarway-connectors` *(new)* | Trait-based adapters for Redis, RabbitMQ, Kafka. Built on `polarway-bus` + `polarway-sources` traits. |

### 1.3) Data / handle lifecycle (updated)

1. Worker executes operation producing DataFrame.
2. Worker serializes DataFrame to Arrow IPC and writes to shared store.
3. Worker publishes `HandleCreated { id, worker_id, schema_hash }` on the `EventBus`.
4. Worker returns a handle referencing the persisted artifact.
5. Any worker can later load by handle, apply next operation, persist new result, publish `HandleCreated`, and return a new handle.
6. Workers subscribe to `HandleInvalidated` events for cache eviction.

### 1.4) Routing strategy

- **Default**: "any worker" behind LB (round-robin). External handles make this safe.
- **Optional optimization**: request affinity ("sticky") by consistent hashing of handle → worker to reduce store reads.
- **EventBus-assisted**: workers publish load metrics on the bus; a lightweight coordinator subscriber can rebalance.

---

## 2) Rollout phases (minimal effort → production)

### Phase 0 — Local single node (developer loop)
- Run one worker locally.
- Keep in-memory handles (default) to preserve current behavior.
- Validate API correctness.

Exit criteria: Client can read/transform/write with existing notebooks/tests.

### Phase 1 — Multi-container on ONE host (first distributed semantics)
- Run multiple worker containers on a single machine.
- Enable external handles:
  - `POLARWAY_HANDLE_STORE=external`
  - `POLARWAY_STATE_DIR=/state` (mounted shared dir)
- Enable `EventBus<WorkerEvent>` with default capacity (4096) for inter-worker coordination.
- Put a local gRPC-capable LB in front.

Exit criteria:
- Any request can hit any worker and still resolve handles.
- Handles survive worker restarts (because state is external).
- Workers see each other's `HandleCreated` events via the bus.

### Phase 2 — Multi-VM / multi-host with Lakehouse backend
- Same as Phase 1, but workers span multiple machines.
- **Retained solution**: swap `FileSystemStateStore` for `polarway-lakehouse::DeltaStore`:
  - Writes produce ACID Delta Lake transactions (atomic, no partial artifacts).
  - Each handle maps to a versioned Delta table row — version number is the handle version.
  - Time-travel: clients can request handle state at any previous version.
  - `vacuum()` with configurable retention replaces manual GC.
  - Audit logging tracks all handle writes (who, when, what).
  - Auth (optional feature gate) provides per-tenant isolation out-of-the-box.
- **EventBus** bridges to external message brokers via `polarway-connectors` (see §3.1).

```rust
// Phase 2 state store — DeltaStore as handle backend
use polarway_lakehouse::{DeltaStore, LakehouseConfig};

let config = LakehouseConfig::new("/data/polarway-state"); // or S3 URL
let store = DeltaStore::new(config).await?;

// Write handle artifact
store.write("handles", arrow_batch).await?;

// Read handle by version (time-travel)
let batch = store.read_version("handles", version).await?;

// GC: vacuum old handle artifacts (keep last 7 days)
store.vacuum("handles", chrono::Duration::days(7), false).await?;
```

Exit criteria:
- A handle created on VM A can be read on VM B.
- Failure of a single worker does not break in-flight pipelines.
- Handle artifacts have ACID guarantees and are auditable.

### Phase 3 — Production hardening
- **AuthN/AuthZ**: `polarway-lakehouse` auth feature (JWT + Argon2 + RBAC) for per-tenant isolation.
- **Observability**: tracing + metrics + logs; SLOs. EventBus metrics (event_count, subscriber lag).
- **Backpressure**: concurrency limits, streaming limits, payload limits. EventBus capacity tuning.
- **Cost controls**: Lakehouse `vacuum()` with lifecycle rules, z-order for hot tables.
- **Autoscaling**: workers publish load metrics on EventBus; coordinator subscriber triggers scale events.

Exit criteria:
- Sustained workload with controlled latency, bounded storage growth.
- Audit trail covers all handle mutations.

---

## 3) Connectors Architecture (Redis / RabbitMQ / Kafka)

### 3.1) Design — trait-based, pluggable

The `polarway-connectors` crate provides adapter implementations that bridge `polarway-bus::EventBus` to external message brokers. Each connector implements a common `BusConnector` trait:

```rust
// polarway-connectors/src/traits.rs

/// Trait for bridging EventBus to an external message broker.
///
/// Implementors handle serialization, connection management, and
/// delivery guarantees specific to each broker.
#[async_trait]
pub trait BusConnector: Send + Sync {
    type Config;
    type Error: std::error::Error + Send + Sync;

    /// Connect to the external broker.
    async fn connect(config: Self::Config) -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// Forward events from a local EventBus to the external broker.
    async fn publish_bridge<T>(&self, bus: &EventBus<T>, topic: &str) -> Result<(), Self::Error>
    where
        T: Clone + Send + Sync + serde::Serialize + 'static;

    /// Subscribe to the external broker and publish into a local EventBus.
    async fn subscribe_bridge<T>(&self, bus: &EventBus<T>, topic: &str) -> Result<(), Self::Error>
    where
        T: Clone + Send + Sync + serde::de::DeserializeOwned + 'static;

    /// Health check.
    async fn is_healthy(&self) -> bool;
}
```

### 3.2) Connector implementations

| Connector | Crate/Module | Dependency | Use case |
|---|---|---|---|
| **Redis** | `polarway-connectors/redis` | `redis = { version = "0.27", features = ["tokio-comp", "streams"] }` | Low-latency pub/sub, handle invalidation, lightweight job queues (Redis Streams). Best for ≤ 10 workers. |
| **RabbitMQ** | `polarway-connectors/rabbitmq` | `lapin = "2.5"` | Reliable work distribution with ack/nack, dead-letter queues, routing keys. Best for batch job patterns. |
| **Kafka** | `polarway-connectors/kafka` | `rdkafka = { version = "0.36", features = ["cmake-build"] }` | High-throughput event streaming, durable log, consumer groups. Best for ≥ 10 workers or audit/replay requirements. |

### 3.3) How connectors integrate

```text
                Local Worker Process
┌───────────────────────────────────────────┐
│  EventBus<WorkerEvent>                    │
│     │                                     │
│     ├──subscribers (local)                │
│     │                                     │
│     └──BusConnector::publish_bridge()─────┼──► Redis / RabbitMQ / Kafka
│                                           │
│  BusConnector::subscribe_bridge()─────────┼──◄ Redis / RabbitMQ / Kafka
│     │                                     │
│     └──publishes into local EventBus      │
└───────────────────────────────────────────┘
```

Each worker runs a local `EventBus` for in-process subscribers AND a `BusConnector` that bridges to the external broker. This means:
- **Zero-config single-node**: just the local `EventBus`, no broker needed.
- **Scale-out**: add a connector config and events flow cluster-wide.
- **Contributor-friendly**: implement `BusConnector` for any new broker (NATS, Pulsar, ZeroMQ, etc.).

### 3.4) Configuration

```toml
# polarway.toml (or environment variables)
[distributed]
connector = "redis"  # "redis" | "rabbitmq" | "kafka" | "none" (default)

[distributed.redis]
url = "redis://localhost:6379"
topic_prefix = "polarway:"

[distributed.rabbitmq]
url = "amqp://guest:guest@localhost:5672"
exchange = "polarway"
queue_prefix = "pw."

[distributed.kafka]
bootstrap_servers = "localhost:9092"
topic_prefix = "polarway."
consumer_group = "polarway-workers"
```

---

## 4) Storage strategy (external handles)

### Phase 0–1: shared filesystem (retained)
- Store Arrow IPC at `${POLARWAY_STATE_DIR}/<uuid>.ipc`.
- Ensure the directory is shared across workers.
- Simple, zero-dependency, works for local development and single-host multi-container.

Risks:
- NFS contention at scale.
- No ACID guarantees — partial writes possible on crash.
- GC requires custom cron/cleanup logic.

### Phase 2+: Lakehouse backend (retained solution)

**Replace raw object store with `polarway-lakehouse::DeltaStore`:**

| Capability | Raw S3/Blob | DeltaStore (retained) |
|---|---|---|
| ACID writes | ❌ eventual consistency | ✅ Delta transaction log |
| Time-travel | ❌ manual versioning | ✅ `read_version()` / `read_timestamp()` |
| GC / lifecycle | ❌ bucket lifecycle rules | ✅ `vacuum(retention)` with audit |
| Auth / RBAC | ❌ IAM-only | ✅ JWT + Argon2 + RBAC (feature-gated) |
| Audit trail | ❌ CloudTrail (vendor-specific) | ✅ append-only `audit_log` table |
| Schema evolution | ❌ manual | ✅ Delta schema enforcement |
| Query engine | ❌ download + local | ✅ DataFusion SQL over Delta tables |
| Z-order optimization | ❌ n/a | ✅ `z_order("handles", &["tenant_id"])` |

**Interface mapping:**

```rust
/// StateStore trait that DeltaStore implements for Phase 2+
#[async_trait]
pub trait StateStore: Send + Sync {
    async fn put(&self, handle_id: &str, batch: &RecordBatch) -> Result<HandleMeta>;
    async fn get(&self, handle_id: &str) -> Result<RecordBatch>;
    async fn get_version(&self, handle_id: &str, version: i64) -> Result<RecordBatch>;
    async fn delete(&self, handle_id: &str) -> Result<()>;
    async fn gc(&self, max_age: Duration) -> Result<GcMetrics>;
    async fn list(&self, prefix: &str) -> Result<Vec<HandleMeta>>;
}

/// DeltaStore naturally satisfies this:
/// - put()         → store.write(table, batch)
/// - get()         → store.scan(table)
/// - get_version() → store.read_version(table, version)
/// - delete()      → store.delete_where(table, predicate)
/// - gc()          → store.vacuum(table, retention, dry_run=false)
/// - list()        → store.query(table, "handle_id LIKE '{prefix}%'")
```

Good practices:
- Partition handle tables by tenant_id for multi-tenant isolation.
- Z-order on `(tenant_id, created_at)` for efficient range queries.
- Use `vacuum()` on a schedule (e.g., daily, 7-day retention).
- Enable audit feature gate for compliance workloads.

---

## 5) Correctness and failure model

### Idempotency
- Prefer deterministic operations.
- Handle creation should be safe to retry:
  - **Phase 0–1**: store write is atomic (write temp + rename).
  - **Phase 2+**: Delta transaction log guarantees atomicity — failed writes do not produce visible versions.
  - If a retry succeeds, EventBus publishes `HandleCreated` again (subscribers must be idempotent).

### Timeouts and retries
- Client: exponential backoff for retryable codes (`polarway-sources::backoff`).
- Server: bounded concurrency and reasonable request timeouts.
- Connectors: each broker adapter handles reconnection independently (Redis reconnect, Lapin recovery, rdkafka consumer rebalance).

### Partial failures
- If a worker dies mid-write:
  - **Phase 0–1**: temp file is orphaned, never renamed → handle never returned.
  - **Phase 2+**: Delta transaction aborts → no visible version.
- EventBus: if a worker dies, its subscriber is dropped. Remaining workers continue receiving events.
- Connector recovery: `BusConnector::is_healthy()` + reconnect logic in each adapter.

---

## 6) Observability / operations

Minimum signals:
- Request rate, latency (p50/p95/p99), error rate
- Bytes read/write to state store
- Handle creation rate, handle read rate
- Per-method concurrency and queue time

EventBus signals:
- `event_count()` — total events published since start
- `subscriber_count()` — active subscribers
- Connector lag (broker-specific: Redis stream length, RabbitMQ queue depth, Kafka consumer lag)

Tracing:
- Include handle id and operation id in spans.
- Connector spans: `[connector.redis] publish topic=polarway:handles.created`.

---

## 7) Implementation checklist (concrete tasks)

### Core (already implemented) ✅
- [x] External handle codec `ext:fs:<uuid>`
- [x] FileSystem-backed state store using Arrow IPC
- [x] Service wired to a handle provider
- [x] `polarway-bus` crate — generic `EventBus<T>` with broadcast, filtered subscribers, drain (v0.1.0)
- [x] `polarway-lakehouse` crate — `DeltaStore` with ACID, time-travel, vacuum, auth, audit (v0.1.1)
- [x] `polarway-sources` crate — `DataSource` / `StreamingDataSource` traits, connection pooling (v0.1.0)

### Phase 1 — Multi-container coordination
- [ ] Define `WorkerEvent` enum (`HandleCreated`, `HandleInvalidated`, `WorkerHeartbeat`, `JobAssigned`)
- [ ] Wire `EventBus<WorkerEvent>` into `polarway-grpc` worker startup
- [ ] Publish `HandleCreated` after each successful external store write
- [ ] Subscribe workers to `HandleInvalidated` for LRU cache eviction
- [ ] Make external store writes atomic (temp file + rename)
- [ ] Add GC policy for filesystem store (age-based cleanup) OR "pinning" support
- [ ] Add basic request limits (max rows, max bytes, max columns) to prevent OOM

### Phase 2 — Lakehouse backend + connectors
- [ ] Implement `StateStore` trait in `polarway-grpc`
- [ ] Implement `StateStore for DeltaStore` adapter in `polarway-lakehouse`
- [ ] Create `polarway-connectors` crate with `BusConnector` trait
- [ ] Implement Redis connector (`BusConnector for RedisConnector`)
- [ ] Implement RabbitMQ connector (`BusConnector for RabbitMqConnector`)
- [ ] Implement Kafka connector (`BusConnector for KafkaConnector`)
- [ ] Add configuration parsing for `[distributed]` section
- [ ] Add a small "router" client helper that does retries + optional affinity

### Phase 3 — Production hardening
- [ ] Enable `polarway-lakehouse` auth feature for per-tenant isolation
- [ ] Add EventBus metrics export (Prometheus)
- [ ] Add connector health checks to `/health` endpoint
- [ ] Autoscaling triggers from EventBus load metrics
- [ ] Storage lifecycle automation (scheduled vacuum)
- [ ] Integration tests: multi-worker + connector + lakehouse end-to-end

---

## 8) Configuration knobs

Environment variables:
- `POLARWAY_BIND_ADDRESS` (already exists): e.g. `0.0.0.0:50051`
- `POLARWAY_HANDLE_STORE`:
  - `memory` (default)
  - `external` (filesystem)
  - `lakehouse` (DeltaStore, Phase 2+)
- `POLARWAY_STATE_DIR`:
  - for filesystem store (e.g. `/state`)
- `POLARWAY_LAKEHOUSE_PATH`:
  - for DeltaStore (e.g. `/data/lakehouse` or `s3://bucket/polarway`)
- `POLARWAY_BUS_CAPACITY`:
  - EventBus channel capacity (default: `4096`)
- `POLARWAY_CONNECTOR`:
  - `none` (default) | `redis` | `rabbitmq` | `kafka`
- Connector-specific variables: `POLARWAY_REDIS_URL`, `POLARWAY_RABBITMQ_URL`, `POLARWAY_KAFKA_BROKERS`

---

## 9) Security notes (don't skip in multi-tenant)

- Never trust `POLARWAY_STATE_DIR` from clients (server-side only).
- Consider per-tenant namespaces in handle keys (DeltaStore: partition by `tenant_id`).
- Require auth before allowing read by handle (Lakehouse JWT auth).
- Add quotas per tenant (Lakehouse RBAC + audit).
- Connector credentials: use env vars or secret managers, never in config files.
- EventBus is in-process only — connectors must authenticate to external brokers.

---

## 10) Contributor Guide — How to add a new connector

Adding support for a new message broker (e.g., NATS, Pulsar, ZeroMQ) requires:

1. **Create a module** in `polarway-connectors/src/<broker>.rs`
2. **Implement `BusConnector`** for your adapter struct
3. **Add the dependency** as an optional feature in `polarway-connectors/Cargo.toml`
4. **Write integration tests** in `polarway-connectors/tests/<broker>_test.rs`
5. **Document configuration** in `polarway-connectors/README.md`

The `BusConnector` trait is intentionally minimal — only 4 methods. Focus on:
- Reliable reconnection (use `polarway-sources::connection_pool` if applicable)
- Serialization (serde JSON by default, optional Protobuf)
- At-least-once delivery semantics (exactly-once is a non-goal)

```rust
// Minimal example: NATS connector
pub struct NatsConnector { client: async_nats::Client }

#[async_trait]
impl BusConnector for NatsConnector {
    type Config = NatsConfig;
    type Error = NatsError;

    async fn connect(config: Self::Config) -> Result<Self, Self::Error> { ... }
    async fn publish_bridge<T>(&self, bus: &EventBus<T>, topic: &str) -> Result<(), Self::Error> { ... }
    async fn subscribe_bridge<T>(&self, bus: &EventBus<T>, topic: &str) -> Result<(), Self::Error> { ... }
    async fn is_healthy(&self) -> bool { ... }
}
```
