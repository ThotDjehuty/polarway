# REST Exec API (QuestDB-like)

Polaroid now exposes a small HTTP API designed to mirror the *shape* and ergonomics of QuestDB’s `/exec` endpoint.

## Why this exists

- Easy integrations with tools that expect a simple REST query endpoint.
- A bridge to the long-term architecture:
  - Time-series and metadata via QuestDB
  - Distributed SQL execution via DataFusion + Ballista

## Server configuration

Environment variables:
- `POLAROID_HTTP_BIND_ADDRESS` (default: `0.0.0.0:9000`)
- `POLAROID_QUESTDB_HTTP_URL` (optional): e.g. `http://questdb:9000`
  - if set, Polaroid will proxy `/exec?query=...` to QuestDB

## Endpoints

### `GET /ping`
Health check.

Response:
- `200 OK` with body `ok`

### `GET /exec`

Two modes:

#### 1) Polaroid handle mode (expose DataFrame)

Request:
- `/exec?handle=<handle>&fmt=json&limit=1000`

Behavior:
- Loads the DataFrame referenced by `handle` from the Polaroid server.
- Returns a QuestDB-like JSON payload with `columns` and `dataset`.

Notes:
- `fmt=json` is currently required.
- `limit` defaults to `1000`.

#### 2) QuestDB proxy mode (SQL)

Request:
- `/exec?query=<sql>&fmt=json`

Behavior:
- If `POLAROID_QUESTDB_HTTP_URL` (or `QUESTDB_HTTP_URL`) is set, Polaroid proxies the request to `${QUESTDB}/exec?query=...&fmt=json`.
- This makes Polaroid a single entrypoint for time-series SQL + Polaroid handles.

If QuestDB is not configured:
- returns `412 Failed Precondition` with a hint to set `POLAROID_QUESTDB_HTTP_URL`.

## Roadmap

- Local SQL execution: DataFusion plan execution on a single node.
- Distributed SQL execution: submit DataFusion plans to Ballista scheduler.
- Output formats: Arrow IPC and CSV.
- Auth: token/mTLS and per-tenant handle namespaces.
