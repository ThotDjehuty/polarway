# Polarway Lakehouse

**Delta Lake storage layer** for ACID transactions, time-travel, authentication, and audit logging.

## Overview

`polarway-lakehouse` provides a production-ready storage backend built on [Delta Lake](https://delta.io/) (via [delta-rs](https://github.com/delta-io/delta-rs)). It replaces traditional databases (SQLite, PostgreSQL) with a lakehouse architecture that offers:

- **ACID transactions** on every write
- **Time-travel** to any version or timestamp
- **SQL queries** via Apache DataFusion
- **Append-only audit logs** for compliance
- **GDPR-compliant deletion** with vacuum
- **Automatic optimization** (compaction, Z-ordering)

### Architecture

```text
┌──────────────────────────────────────────────┐
│              Application Layer               │
│     (Streamlit / FastAPI / gRPC / CLI)       │
└──────────────┬───────────────────────────────┘
               │
┌──────────────▼───────────────────────────────┐
│            AuthActor + AuditActor            │
│     (tokio actors, mpsc channels)            │
│     register / login / verify / approve      │
│     log / billing_summary / activity         │
└──────────────┬───────────────────────────────┘
               │
┌──────────────▼───────────────────────────────┐
│              DeltaStore (core)               │
│     append / delete / scan / query / sql     │
│     read_version / read_timestamp / history  │
│     compact / z_order / vacuum               │
└──────────────┬───────────────────────────────┘
               │
┌──────────────▼───────────────────────────────┐
│           Delta Lake (delta-rs 0.30)         │
│     Transaction log + Parquet files          │
│     Apache Arrow columnar format             │
└──────────────────────────────────────────────┘
```

## Getting Started

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
polarway-lakehouse = { path = "polarway-lakehouse", features = ["full"] }
```

### Basic Usage

```rust
use polarway_lakehouse::{DeltaStore, LakehouseConfig};

#[tokio::main]
async fn main() -> polarway_lakehouse::Result<()> {
    let config = LakehouseConfig::new("/data/lakehouse");
    let store = DeltaStore::new(config).await?;

    // Write (ACID)
    let batch = create_record_batch(/* ... */);
    let version = store.append("users", batch).await?;

    // Read (current)
    let users = store.scan("users").await?;

    // Query (SQL)
    let admins = store.query("users", "role = 'admin'").await?;

    Ok(())
}
```

## Time-Travel

Delta Lake maintains a complete version history through its transaction log. Every write creates a new version, and you can read any historical state.

### Read by Version Number

```rust
// Read users at version 5
let old_users = store.read_version("users", 5).await?;

// Compare with current
let current = store.scan("users").await?;
```

**Use case**: Reproduce a bug by reading the exact state of data when the bug occurred.

### Read by Timestamp

```rust
// Read users as of February 1st 2026 at noon
let snapshot = store.read_timestamp("users", "2026-02-01T12:00:00Z").await?;
```

**Use case**: Audit what data was visible to a user at a specific time.

### Version History

```rust
let history = store.history("users", Some(10)).await?;

for entry in &history {
    println!(
        "Version {}: operation={:?}, timestamp={:?}",
        entry.version,
        entry.operation,
        entry.timestamp,
    );
}
```

Output:
```
Version 42: operation=Some("WRITE"), timestamp=Some(1738425600000)
Version 41: operation=Some("DELETE"), timestamp=Some(1738339200000)
Version 40: operation=Some("OPTIMIZE"), timestamp=Some(1738252800000)
...
```

### Time-Travel in Python

```python
from polarway.lakehouse import LakehouseClient

client = LakehouseClient(base_url="http://localhost:8080")

# Read current state
current = client.scan("users")

# Time-travel to version 5
old = client.read_version("users", version=5)

# Time-travel to a timestamp
snapshot = client.read_timestamp("users", "2026-02-01T12:00:00Z")

# Get version history
history = client.history("users", limit=10)
for entry in history:
    print(f"v{entry['version']}: {entry['operation']} at {entry['timestamp']}")
```

## Authentication

The `auth` feature provides a complete user management system built on Delta Lake.

### User Registration Flow

```text
  User                AuthActor              DeltaStore
   │                     │                      │
   │  register(...)      │                      │
   │────────────────────►│                      │
   │                     │  check_existing      │
   │                     │─────────────────────►│
   │                     │                      │
   │                     │  hash_password       │
   │                     │  (Argon2)            │
   │                     │                      │
   │                     │  append(users, ...)  │
   │                     │─────────────────────►│
   │                     │                      │  ACID commit
   │                     │  audit_log(Register) │
   │                     │─────────────────────►│
   │  Ok(UserRecord)     │                      │
   │◄────────────────────│                      │
```

### JWT Token Authentication

```rust
use polarway_lakehouse::auth::{AuthActor, SubscriptionTier};

let (auth, _handle) = AuthActor::new(store.clone(), config.clone()).await?;

// Register
let user = auth
    .register("alice", "alice@example.com", "StrongP@ss123!", SubscriptionTier::Pro)
    .await?;

// Login → JWT token
let token = auth.login("alice@example.com", "StrongP@ss123!").await?;

// Verify token (on every request)
let claims = auth.verify(&token).await?;
println!("User: {}, Role: {:?}", claims.sub, claims.role);

// Admin: approve pending registration
auth.approve(&user.user_id).await?;
```

### Subscription Tiers

| Tier | Features | Rate Limit |
|------|----------|------------|
| **Free** | Basic access, 100 queries/day | 10 req/min |
| **Pro** | Full access, 10K queries/day | 100 req/min |
| **Enterprise** | Unlimited, priority support | Unlimited |

## Audit Logging

All operations are logged to an append-only, partitioned Delta table.

### Automatic Logging

Every auth operation automatically creates an audit entry:
- User registration, login, logout
- Token verification and refresh
- Password changes
- Admin actions (approve, reject, disable)
- GDPR operations

### Manual Logging

```rust
use polarway_lakehouse::audit::{AuditActor, AuditEntry, ActionType};

let (audit, _handle) = AuditActor::new(store.clone()).await?;

audit.log(AuditEntry {
    action: ActionType::DataExport,
    user_id: "user_123".into(),
    details: Some("Exported 1M rows to CSV".into()),
    row_count: Some(1_000_000),
    ..Default::default()
}).await?;
```

### Billing Queries

```rust
// Get billing summary for a user over a date range
let summary = audit
    .billing_summary("user_123", "2026-02-01", "2026-02-28")
    .await?;

println!("Total actions: {}", summary.total_actions);
println!("Total rows: {}", summary.total_rows);
println!("Actions by type: {:?}", summary.actions_by_type);
```

## Storage Optimization

### Compaction

Small files degrade read performance. Compaction merges them:

```rust
// Before: 1000 small Parquet files (1KB each)
let metrics = store.compact("audit_log").await?;
// After: 10 large Parquet files (100KB each)
// Read performance: 10-100× faster
```

### Z-Ordering

Z-ordering colocates data with similar column values, enabling massive data skipping:

```rust
// Z-order audit_log by user_id and action
store.z_order("audit_log", &["user_id", "action"]).await?;

// Now queries filtering on user_id or action skip 90%+ of files:
let user_events = store.query("audit_log", "user_id = 'user_123'").await?;
// Only reads files that might contain user_123 → 10× faster
```

### Vacuum

Remove old files no longer referenced by the transaction log:

```rust
// Dry run: see what would be deleted
let dry = store.vacuum("users", 168, true).await?;
println!("Would delete {} files", dry.files_deleted);

// Actual vacuum with 7-day retention
store.vacuum("users", 168, false).await?;

// GDPR: zero retention for immediate permanent deletion
store.vacuum("users", 0, false).await?;
```

### Automatic Maintenance

```rust
use polarway_lakehouse::maintenance::MaintenanceScheduler;

let scheduler = MaintenanceScheduler::new(store.clone(), config.clone());
scheduler.start().await; // Background tasks:

// ┌─────────────────────┬────────────────┐
// │ Task                │ Frequency      │
// ├─────────────────────┼────────────────┤
// │ Session cleanup     │ Every hour     │
// │ Compaction          │ Every 6 hours  │
// │ Z-ordering          │ Daily          │
// │ Vacuum              │ Weekly         │
// └─────────────────────┴────────────────┘
```

## GDPR Compliance

### Right to Erasure

```rust
// Permanently delete all data for a user
store.gdpr_delete_user("user_id_to_delete").await?;

// This:
// 1. Deletes from users, sessions, audit_log, user_actions
// 2. Vacuums all tables with zero retention
// 3. Physically removes all Parquet files containing the user's data
```

### Right to Access (Data Export)

```rust
// Export all user data
let user_data = store.query("users", &format!("user_id = '{user_id}'")).await?;
let user_sessions = store.query("sessions", &format!("user_id = '{user_id}'")).await?;
let user_audit = store.query("audit_log", &format!("user_id = '{user_id}'")).await?;
let user_actions = store.query("user_actions", &format!("user_id = '{user_id}'")).await?;
```

## Error Handling

All operations follow Railway-Oriented Programming principles:

```rust
use polarway_lakehouse::error::LakehouseError;

let result = store.read_version("users", 999).await;

match result {
    Ok(batches) => { /* process data */ }
    Err(LakehouseError::VersionNotFound { table, version }) => {
        eprintln!("Version {version} doesn't exist for table {table}");
    }
    Err(LakehouseError::AuthenticationFailed(msg)) => {
        eprintln!("Auth failed: {msg}");
    }
    Err(e) => eprintln!("Error: {e}"),
}
```

### Error Categories

| Category | Error Types |
|----------|------------|
| **Storage** | `DeltaTable`, `TableNotFound`, `VersionNotFound`, `SchemaMismatch` |
| **Auth** | `AuthenticationFailed`, `UserNotFound`, `InvalidCredentials`, `TokenExpired` |
| **Audit** | `AuditWriteFailed` |
| **Infrastructure** | `Io`, `Arrow`, `DataFusion`, `Config` |

## Configuration Reference

```rust
use polarway_lakehouse::LakehouseConfig;

let config = LakehouseConfig::builder()
    .base_path("/data/lakehouse")           // Root directory for all Delta tables
    .jwt_secret("your-256-bit-secret")      // JWT signing key
    .session_duration_hours(24)             // Token expiry (default: 24h)
    .vacuum_retention_hours(168)            // Vacuum retention (default: 168h = 7 days)
    .z_order_columns(vec![                  // Columns for Z-ordering
        "user_id".into(),
        "action".into(),
    ])
    .build();
```

## Table Schemas

### users

| Column | Type | Description |
|--------|------|-------------|
| `user_id` | `Utf8` | Unique identifier (UUID) |
| `username` | `Utf8` | Display name |
| `email` | `Utf8` | Email address (unique) |
| `password_hash` | `Utf8` | Argon2 hash |
| `role` | `Utf8` | `user` / `admin` / `superadmin` |
| `subscription_tier` | `Utf8` | `free` / `pro` / `enterprise` |
| `is_active` | `Boolean` | Account enabled |
| `created_at` | `Utf8` | ISO 8601 timestamp |
| `updated_at` | `Utf8` | ISO 8601 timestamp |

### sessions

| Column | Type | Description |
|--------|------|-------------|
| `session_id` | `Utf8` | Unique session ID |
| `user_id` | `Utf8` | Foreign key to users |
| `token_hash` | `Utf8` | SHA-256 of JWT |
| `expires_at` | `Utf8` | ISO 8601 expiry |
| `created_at` | `Utf8` | ISO 8601 timestamp |
| `ip_address` | `Utf8` | Client IP |
| `user_agent` | `Utf8` | Browser/client info |

### audit_log (partitioned by `date_partition`)

| Column | Type | Description |
|--------|------|-------------|
| `audit_id` | `Utf8` | Unique audit entry ID |
| `timestamp` | `Utf8` | ISO 8601 timestamp |
| `user_id` | `Utf8` | Who performed the action |
| `action` | `Utf8` | Action type (see ActionType enum) |
| `details` | `Utf8` | Human-readable description |
| `ip_address` | `Utf8` | Client IP |
| `date_partition` | `Utf8` | `YYYY-MM-DD` for partitioning |
