# Polaroid Distributed Workload — Implementation Plan

Date: 2026-01-13
Branch note: external (stateless) handle scaffolding exists on `feat/stateless-handles-objectstore`.

## 0) Goal (What “distributed Polaroid” means)

Polaroid becomes horizontally scalable by running **multiple gRPC workers** behind a **load balancer** while keeping **handle state external** (so any worker can serve subsequent requests for a handle).

Minimum viable distributed semantics:
- A request that produces a DataFrame persists it to a shared store and returns an **external handle** (e.g. `ext:fs:<uuid>` today).
- Any subsequent operation that references that handle can be served by any worker by loading the DataFrame from the shared store.

Non-goals (for the minimal-effort path):
- Distributed query planning / DAG scheduling
- Cross-worker shared cache consistency
- Exactly-once execution (we aim for idempotent operations + retries)

## 1) Recommended architecture (simple + works)

### Components
- **Client(s)** (Python SDK / notebook / job runner)
- **L7 Load Balancer** (HTTP/2 gRPC capable) or service discovery
- **Polaroid Worker Pool** (N containers/VMs, identical)
- **External State Store**
  - Phase 0/1: shared filesystem (NFS/SMB/hostPath) for Arrow IPC artifacts
  - Phase 2+: object store (S3-compatible / blob storage) with lifecycle rules
- **Optional Coordinator (later)**
  - Queue-driven work distribution (Redis/RabbitMQ/Kafka) for long-running jobs

### Data / handle lifecycle
1. Worker executes operation producing DataFrame.
2. Worker serializes DataFrame to Arrow IPC and writes to shared store.
3. Worker returns a handle referencing the persisted artifact.
4. Any worker can later load by handle, apply next operation, persist new result, and return a new handle.

### Routing strategy
- **Default**: “any worker” behind LB (round-robin). External handles make this safe.
- **Optional optimization**: request affinity (“sticky”) by consistent hashing of handle → worker to reduce store reads.

## 2) Rollout phases (minimal effort → production)

### Phase 0 — Local single node (developer loop)
- Run one worker locally.
- Keep in-memory handles (default) to preserve current behavior.
- Validate API correctness.

Exit criteria:
- Client can read/transform/write with existing notebooks/tests.

### Phase 1 — Multi-container on ONE host (first distributed semantics)
- Run multiple worker containers on a single machine.
- Enable external handles:
  - `POLAROID_HANDLE_STORE=external`
  - `POLAROID_STATE_DIR=/state` (mounted shared dir)
- Put a local gRPC-capable LB in front.

Exit criteria:
- Any request can hit any worker and still resolve handles.
- Handles survive worker restarts (because state is external).

### Phase 2 — Multi-VM / multi-host (real horizontal scale)
- Same as Phase 1, but workers span multiple machines.
- Swap shared filesystem for:
  - A properly shared network filesystem, or
  - An object store backend (recommended) via a `StateStore` implementation.

Exit criteria:
- A handle created on VM A can be read on VM B.
- Failure of a single worker does not break in-flight pipelines (beyond retryable errors).

### Phase 3 — Production hardening
- **AuthN/AuthZ**: mTLS or token auth; per-tenant isolation.
- **Observability**: tracing + metrics + logs; SLOs.
- **Backpressure**: concurrency limits, streaming limits, payload limits.
- **Cost controls**: storage lifecycle policies, GC of old handles.
- **Autoscaling**: scale workers on CPU/memory/queue depth.

Exit criteria:
- Can run sustained workload with controlled latency, bounded storage growth.

## 3) Work distribution patterns (choose per workload)

### Pattern A — Client-driven pipelining (simplest)
Clients call gRPC operations directly. Work distribution is “free” via the LB.

Pros: minimal new infra.
Cons: client must orchestrate; long jobs can time out.

### Pattern B — Job queue + workers (batch workloads)
Client submits a job spec; workers pull from queue and produce outputs.

Pros: great for long-running workloads, retries, prioritization.
Cons: requires coordinator + job storage.

### Pattern C — Hybrid
Interactive requests go through LB; batch jobs go through queue.

## 4) Storage strategy (external handles)

### Near-term: shared filesystem
- Store Arrow IPC at `${POLAROID_STATE_DIR}/<uuid>.ipc`.
- Ensure the directory is shared across workers.

Risks:
- NFS contention at scale
- GC complexity

### Target: object store backend
- Implement `ObjectStoreStateStore` with methods matching `FileSystemStateStore`.
- Use content-addressed keys (optional) to dedupe repeated results.

Good practices:
- Add metadata (schema hash, row count, created_at) sidecar or object tags.
- Lifecycle policy: expire objects older than N days unless pinned.

## 5) Correctness and failure model

### Idempotency
- Prefer deterministic operations.
- Handle creation should be safe to retry:
  - store write is atomic (write temp + rename / object put with unique key)
  - if a retry creates a second object, it’s acceptable unless you require dedupe.

### Timeouts and retries
- Client: exponential backoff for retryable codes.
- Server: bounded concurrency and reasonable request timeouts.

### Partial failures
- If a worker dies mid-write, object must not be referenced by a returned handle.
- Use write-then-commit semantics.

## 6) Observability / operations

Minimum signals:
- Request rate, latency (p50/p95/p99), error rate
- Bytes read/write to state store
- Handle creation rate, handle read rate
- Per-method concurrency and queue time

Tracing:
- Include handle id and operation id in spans.

## 7) Implementation checklist (concrete tasks)

### Core (already partially implemented)
- [x] External handle codec `ext:fs:<uuid>`
- [x] FileSystem-backed state store using Arrow IPC
- [x] Service wired to a handle provider

### Next (recommended)
- [ ] Make external store writes atomic (temp file + rename).
- [ ] Add GC policy for filesystem store (age-based cleanup) OR “pinning” support.
- [ ] Add basic request limits (max rows, max bytes, max columns) to prevent OOM.
- [ ] Add an object-store backend with the same interface.
- [ ] Add a small “router” client helper that does retries + optional affinity.

## 8) Configuration knobs

Environment variables:
- `POLAROID_BIND_ADDRESS` (already exists): e.g. `0.0.0.0:50051`
- `POLAROID_HANDLE_STORE`:
  - `memory` (default)
  - `external`
- `POLAROID_STATE_DIR`:
  - for filesystem store (e.g. `/state`)

## 9) Security notes (don’t skip in multi-tenant)

- Never trust `POLAROID_STATE_DIR` from clients (server-side only).
- Consider per-tenant namespaces in handle keys.
- Require auth before allowing read by handle.
- Add quotas per tenant.
