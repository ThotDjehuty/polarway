//! DeltaStore integration tests — CRUD, time-travel, optimization

use std::sync::Arc;

use deltalake::arrow::array::{ArrayRef, BooleanArray, RecordBatch, StringArray};
use tempfile::TempDir;

use polarway_lakehouse::config::LakehouseConfig;
use polarway_lakehouse::schema;
use polarway_lakehouse::store::DeltaStore;

fn test_config(dir: &TempDir) -> LakehouseConfig {
    LakehouseConfig::new(dir.path().to_str().unwrap())
        .with_jwt_secret("test-secret-key-for-testing-only")
}

fn make_user_batch(user_id: &str, username: &str, email: &str) -> RecordBatch {
    RecordBatch::try_new(
        Arc::new(schema::users_arrow_schema()),
        vec![
            Arc::new(StringArray::from(vec![user_id])) as ArrayRef,
            Arc::new(StringArray::from(vec![username])),
            Arc::new(StringArray::from(vec![email])),
            Arc::new(StringArray::from(vec!["$argon2id$fake_hash"])),
            Arc::new(StringArray::from(vec!["registered"])),
            Arc::new(StringArray::from(vec![Some("pioneer")])),
            Arc::new(StringArray::from(vec![Some("Test")])),
            Arc::new(StringArray::from(vec![Some("User")])),
            Arc::new(BooleanArray::from(vec![true])),
            Arc::new(StringArray::from(vec!["2025-01-01T00:00:00Z"])),
            Arc::new(StringArray::from(vec![None::<&str>])),
            Arc::new(StringArray::from(vec![Some("{}")])),
        ],
    )
    .unwrap()
}

#[tokio::test]
async fn test_store_init_creates_tables() {
    let dir = TempDir::new().unwrap();
    let config = test_config(&dir);
    let store = DeltaStore::new(config).await.unwrap();

    // All 4 tables should exist
    let version = store.version(schema::TABLE_USERS).await.unwrap();
    assert_eq!(version, 0); // freshly created

    let version = store.version(schema::TABLE_SESSIONS).await.unwrap();
    assert_eq!(version, 0);

    let version = store.version(schema::TABLE_AUDIT_LOG).await.unwrap();
    assert_eq!(version, 0);

    let version = store.version(schema::TABLE_USER_ACTIONS).await.unwrap();
    assert_eq!(version, 0);
}

#[tokio::test]
async fn test_append_and_scan() {
    let dir = TempDir::new().unwrap();
    let store = DeltaStore::new(test_config(&dir)).await.unwrap();

    let batch = make_user_batch("u1", "bob", "bob@example.com");
    let version = store.append(schema::TABLE_USERS, batch).await.unwrap();
    assert!(version > 0);

    let results = store.scan(schema::TABLE_USERS).await.unwrap();
    let total_rows: usize = results.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 1);
}

#[tokio::test]
async fn test_query_with_predicate() {
    let dir = TempDir::new().unwrap();
    let store = DeltaStore::new(test_config(&dir)).await.unwrap();

    // Insert two users
    let b1 = make_user_batch("u1", "bob", "bob@example.com");
    let b2 = make_user_batch("u2", "bob", "bob@example.com");
    store.append(schema::TABLE_USERS, b1).await.unwrap();
    store.append(schema::TABLE_USERS, b2).await.unwrap();

    // Query for bob
    let results = store
        .query(schema::TABLE_USERS, "username = 'bob'")
        .await
        .unwrap();
    let total: usize = results.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total, 1);

    // Query for bob
    let results = store
        .query(schema::TABLE_USERS, "username = 'bob'")
        .await
        .unwrap();
    let total: usize = results.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total, 1);
}

#[tokio::test]
async fn test_delete() {
    let dir = TempDir::new().unwrap();
    let store = DeltaStore::new(test_config(&dir)).await.unwrap();

    let batch = make_user_batch("u1", "bob", "bob@example.com");
    store.append(schema::TABLE_USERS, batch).await.unwrap();

    // Delete bob
    let metrics = store
        .delete(schema::TABLE_USERS, "user_id = 'u1'")
        .await
        .unwrap();
    assert!(metrics.num_deleted_rows > 0);

    // Verify empty
    let results = store.scan(schema::TABLE_USERS).await.unwrap();
    let total: usize = results.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total, 0);
}

#[tokio::test]
async fn test_time_travel_by_version() {
    let dir = TempDir::new().unwrap();
    let store = DeltaStore::new(test_config(&dir)).await.unwrap();

    // Version 0: empty table after creation
    // Version 1: insert bob
    let b1 = make_user_batch("u1", "bob", "bob@example.com");
    store.append(schema::TABLE_USERS, b1).await.unwrap();

    // Version 2: insert bob
    let b2 = make_user_batch("u2", "bob", "bob@example.com");
    store.append(schema::TABLE_USERS, b2).await.unwrap();

    // Current: 2 users
    let current = store.scan(schema::TABLE_USERS).await.unwrap();
    let total: usize = current.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total, 2);

    // Version 1: only bob
    let v1 = store.read_version(schema::TABLE_USERS, 1).await.unwrap();
    let total_v1: usize = v1.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_v1, 1);
}

#[tokio::test]
async fn test_history() {
    let dir = TempDir::new().unwrap();
    let store = DeltaStore::new(test_config(&dir)).await.unwrap();

    let b1 = make_user_batch("u1", "bob", "bob@example.com");
    store.append(schema::TABLE_USERS, b1).await.unwrap();

    let b2 = make_user_batch("u2", "bob", "bob@example.com");
    store.append(schema::TABLE_USERS, b2).await.unwrap();

    let history = store.history(schema::TABLE_USERS, Some(10)).await.unwrap();
    assert!(history.len() >= 3); // create + 2 appends
}

#[tokio::test]
async fn test_compact_and_vacuum() {
    let dir = TempDir::new().unwrap();
    let store = DeltaStore::new(test_config(&dir)).await.unwrap();

    // Insert several small batches
    for i in 0..5 {
        let batch = make_user_batch(
            &format!("u{i}"),
            &format!("user{i}"),
            &format!("user{i}@example.com"),
        );
        store.append(schema::TABLE_USERS, batch).await.unwrap();
    }

    // Compact should succeed
    let _metrics = store.compact(schema::TABLE_USERS).await.unwrap();

    // Vacuum (dry run) should succeed
    let vacuum_metrics = store.vacuum(schema::TABLE_USERS, 0, true).await.unwrap();
    assert!(vacuum_metrics.dry_run);
}

#[tokio::test]
async fn test_sql_query() {
    let dir = TempDir::new().unwrap();
    let store = DeltaStore::new(test_config(&dir)).await.unwrap();

    let b1 = make_user_batch("u1", "bob", "bob@example.com");
    let b2 = make_user_batch("u2", "bob", "bob@example.com");
    store.append(schema::TABLE_USERS, b1).await.unwrap();
    store.append(schema::TABLE_USERS, b2).await.unwrap();

    let results = store
        .sql(
            schema::TABLE_USERS,
            "SELECT username, email FROM t WHERE role = 'registered' ORDER BY username",
        )
        .await
        .unwrap();

    let total: usize = results.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total, 2);
}

#[tokio::test]
async fn test_gdpr_delete() {
    let dir = TempDir::new().unwrap();
    let store = DeltaStore::new(test_config(&dir)).await.unwrap();

    let batch = make_user_batch("u1", "bob", "bob@example.com");
    store.append(schema::TABLE_USERS, batch).await.unwrap();

    // GDPR delete
    store.gdpr_delete_user("u1").await.unwrap();

    // Verify: no trace of user in users table
    let r = store.scan(schema::TABLE_USERS).await.unwrap();
    let total: usize = r.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total, 0);
}
