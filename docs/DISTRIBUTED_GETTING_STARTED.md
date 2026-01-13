# Getting Started — Running Polaroid Across Multiple Containers/VMs

Date: 2026-01-13

This guide shows the simplest way to distribute Polaroid workloads across multiple containers/VMs.

## 1) Quick concept

You scale Polaroid by:
- Running **N identical gRPC workers** behind a **gRPC-capable load balancer**.
- Turning on **external handles** so any worker can serve follow-up operations.

External handles mean:
- The worker persists the DataFrame result into a shared store.
- The handle returned to the client references that persisted artifact.

## 2) Prerequisites

- Docker (for container path) or systemd (for VM path)
- A shared storage option accessible by all workers:
  - Development: shared host path
  - Multi-host: network filesystem (NFS/SMB)
  - Production: object store (recommended)

## 3) Environment variables

On each worker:
- `POLAROID_BIND_ADDRESS=0.0.0.0:50051`

Handle store modes:
- In-memory (default):
  - `POLAROID_HANDLE_STORE=memory`
- External (distributed-safe):
  - `POLAROID_HANDLE_STORE=external`
  - `POLAROID_STATE_DIR=/state` (must be shared across workers)

## 4) Single-host multi-container (Phase 1)

### Topology
- 1 machine
- 1 shared directory mounted into every worker container
- 1 load balancer container in front

### What to run
- Worker containers (replicas): `polaroid-grpc`
- Load balancer: any HTTP/2 gRPC-capable LB

### Key idea
All workers must see the same `POLAROID_STATE_DIR` contents.

## 5) Multi-VM / multi-host (Phase 2)

### Topology
- Multiple VMs
- Each VM runs one or more worker containers
- A load balancer routes traffic to all worker endpoints
- Shared external store:
  - network filesystem mounted at the same path on all nodes (e.g. `/state`), OR
  - object store backend (recommended next step)

### Path consistency
If you use a filesystem store, ensure the mount path is identical on all machines:
- Good: every worker uses `/state`
- Risky: different paths per VM (handles will still work if key-to-path mapping is consistent, but ops/debugging becomes painful)

## 6) Client usage patterns

### Pattern A: Interactive pipelines (simplest)
- Client calls gRPC operations sequentially.
- Each call can land on any worker via LB.

Recommended client settings:
- request timeout (e.g. 30–120s depending on operation)
- retries on transient errors

### Pattern B: Batch jobs
- Client enqueues jobs to a queue.
- Workers pull and execute.

This is the right approach for long-running workloads where client timeouts are unacceptable.

## 7) Good practices (production-minded)

### Make operations retry-safe
- Assume clients will retry.
- Ensure “create handle” is atomic:
  - filesystem: write temp file then rename
  - object store: upload then return handle

### Bound resources
- Set maximum payload sizes (streaming batches, Arrow IPC bytes).
- Limit concurrency per worker (CPU and memory are the bottleneck with DataFrames).
- Add guardrails: max rows, max columns, max string lengths (if applicable).

### Handle lifecycle and storage growth
- External state grows without cleanup.
- Plan one of:
  - TTL-based GC (delete objects older than N hours/days)
  - “pinning” important artifacts and expiring the rest

### Observability
At minimum, track:
- requests/sec and latency per RPC method
- error rate by status code
- bytes written/read to state store
- number of handles created per minute

### Network and security
- Run workers on a private network.
- Put auth in front (mTLS or token auth).
- Separate tenants/namespaces in handle keys.

### Performance tips
- Prefer Arrow IPC for fast serialization.
- Consider request affinity (sticky routing) to reduce repeated reads from external store.
- Add a local cache (best-effort) per worker if repeated reads are common.

## 8) Suggested next steps

- Implement atomic writes + GC for the filesystem state store.
- Add an object-store backed `StateStore` (same interface as filesystem).
- Add a small client-side “router” helper for:
  - retries
  - optional handle-affinity routing
  - standardized timeouts

## 9) Reference docs

- Implementation plan: [DISTRIBUTED_IMPLEMENTATION_PLAN.md](DISTRIBUTED_IMPLEMENTATION_PLAN.md)
