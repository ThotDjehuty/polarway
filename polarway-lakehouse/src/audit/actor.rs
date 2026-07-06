//! AuditActor — append-only event logging with billing queries
//!
//! The audit actor writes events to a partitioned Delta table
//! and exposes DataFusion SQL queries for billing & analytics.
//!
//! # Usage
//!
//! ```rust,no_run
//! use polarway_lakehouse::audit::{AuditActor, ActionType};
//! use polarway_lakehouse::store::DeltaStore;
//! use polarway_lakehouse::LakehouseConfig;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = LakehouseConfig::new("/data/lakehouse");
//!     let store = Arc::new(DeltaStore::new(config).await?);
//!     let handle = AuditActor::spawn(store).await;
//!
//!     // Log an event
//!     handle.log(
//!         "user-123".into(), "albert".into(),
//!         ActionType::BacktestRun,
//!         Some("strategy-456".into()),
//!         "Backtest on BTC/USD 1m".into(),
//!         None,
//!     ).await;
//!
//!     // Query billing summary
//!     let summary = handle.billing_summary(
//!         "user-123".into(), "2025-01-01".into(), "2025-12-31".into(),
//!     ).await?;
//!
//!     Ok(())
//! }
//! ```

use std::sync::Arc;

use chrono::Utc;
use deltalake::arrow::array::{Array, ArrayRef, RecordBatch, StringArray, UInt64Array};
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};
use uuid::Uuid;

use crate::error::{LakehouseError, Result};
use crate::schema;
use crate::store::DeltaStore;

use super::types::*;

// ─── Messages ───

enum AuditMsg {
    Log {
        user_id: String,
        username: String,
        action: ActionType,
        resource: Option<String>,
        detail: String,
        ip_address: Option<String>,
    },
    GetUserActivity {
        user_id: String,
        limit: usize,
        reply: oneshot::Sender<Vec<AuditEntry>>,
    },
    BillingSummary {
        user_id: String,
        start_date: String,
        end_date: String,
        reply: oneshot::Sender<Result<BillingSummary>>,
    },
    GetRecentEvents {
        limit: usize,
        reply: oneshot::Sender<Vec<AuditEntry>>,
    },
}

// ─── Actor ───

/// Audit actor — append-only event logging
pub struct AuditActor {
    store: Arc<DeltaStore>,
    rx: mpsc::Receiver<AuditMsg>,
}

impl AuditActor {
    /// Spawn the audit actor with a shared DeltaStore
    pub async fn spawn(store: Arc<DeltaStore>) -> AuditHandle {
        let (tx, rx) = mpsc::channel(512);
        let actor = Self { store, rx };
        tokio::spawn(actor.run());
        info!("AuditActor spawned");
        AuditHandle { tx }
    }

    async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                AuditMsg::Log { user_id, username, action, resource, detail, ip_address } => {
                    if let Err(e) = self.handle_log(user_id, username, action, resource, detail, ip_address).await {
                        warn!(error = ?e, "Failed to write audit log");
                    }
                }
                AuditMsg::GetUserActivity { user_id, limit, reply } => {
                    let _ = reply.send(self.handle_user_activity(&user_id, limit).await);
                }
                AuditMsg::BillingSummary { user_id, start_date, end_date, reply } => {
                    let _ = reply.send(self.handle_billing_summary(&user_id, &start_date, &end_date).await);
                }
                AuditMsg::GetRecentEvents { limit, reply } => {
                    let _ = reply.send(self.handle_recent_events(limit).await);
                }
            }
        }
        info!("AuditActor stopped");
    }

    async fn handle_log(
        &self,
        user_id: String,
        username: String,
        action: ActionType,
        resource: Option<String>,
        detail: String,
        ip_address: Option<String>,
    ) -> Result<()> {
        let event_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let timestamp = now.to_rfc3339();
        let date_partition = now.format("%Y-%m-%d").to_string();

        let batch = RecordBatch::try_new(
            Arc::new(schema::audit_log_arrow_schema()),
            vec![
                Arc::new(StringArray::from(vec![event_id.as_str()])) as ArrayRef,
                Arc::new(StringArray::from(vec![user_id.as_str()])),
                Arc::new(StringArray::from(vec![username.as_str()])),
                Arc::new(StringArray::from(vec![action.as_str()])),
                Arc::new(StringArray::from(vec![resource.as_deref()])),
                Arc::new(StringArray::from(vec![detail.as_str()])),
                Arc::new(StringArray::from(vec![ip_address.as_deref()])),
                Arc::new(StringArray::from(vec![timestamp.as_str()])),
                Arc::new(StringArray::from(vec![date_partition.as_str()])),
            ],
        )?;

        self.store.append(schema::TABLE_AUDIT_LOG, batch).await?;
        Ok(())
    }

    async fn handle_user_activity(&self, user_id: &str, limit: usize) -> Vec<AuditEntry> {
        let sql = format!(
            "SELECT * FROM audit_log WHERE user_id = '{}' ORDER BY timestamp DESC LIMIT {}",
            user_id, limit
        );
        self.query_entries_sql(&sql).await.unwrap_or_default()
    }

    async fn handle_recent_events(&self, limit: usize) -> Vec<AuditEntry> {
        let sql = format!(
            "SELECT * FROM audit_log ORDER BY timestamp DESC LIMIT {}",
            limit
        );
        self.query_entries_sql(&sql).await.unwrap_or_default()
    }

    async fn handle_billing_summary(
        &self,
        user_id: &str,
        start_date: &str,
        end_date: &str,
    ) -> Result<BillingSummary> {
        // Query counts per action type using DataFusion SQL
        let sql = format!(
            r#"SELECT
                action,
                COUNT(*) as cnt
            FROM audit_log
            WHERE user_id = '{user_id}'
                AND date_partition >= '{start_date}'
                AND date_partition <= '{end_date}'
            GROUP BY action"#,
        );

        let batches = self.store.sql(schema::TABLE_AUDIT_LOG, &sql).await?;

        let mut summary = BillingSummary {
            user_id: user_id.to_string(),
            period_start: start_date.to_string(),
            period_end: end_date.to_string(),
            total_queries: 0,
            total_uploads: 0,
            total_exports: 0,
            total_backtests: 0,
            total_live_trades: 0,
            total_actions: 0,
        };

        for batch in &batches {
            let actions = batch.column(0)
                .as_any()
                .downcast_ref::<StringArray>();
            let counts = batch.column(1)
                .as_any()
                .downcast_ref::<UInt64Array>();

            if let (Some(actions), Some(counts)) = (actions, counts) {
                for i in 0..batch.num_rows() {
                    let action = actions.value(i);
                    let count = counts.value(i);
                    summary.total_actions += count;

                    match action {
                        "query_executed" => summary.total_queries += count,
                        "data_upload" => summary.total_uploads += count,
                        "data_export" => summary.total_exports += count,
                        "backtest_run" => summary.total_backtests += count,
                        "live_trade_start" => summary.total_live_trades += count,
                        _ => {}
                    }
                }
            }
        }

        Ok(summary)
    }

    fn extract_entry_from_batch(batch: &RecordBatch, i: usize) -> Option<AuditEntry> {
        let get_str = |col: usize| -> &str {
            batch.column(col)
                .as_any()
                .downcast_ref::<StringArray>()
                .map(|a| a.value(i))
                .unwrap_or("")
        };

        let get_opt = |col: usize| -> Option<String> {
            batch.column(col)
                .as_any()
                .downcast_ref::<StringArray>()
                .and_then(|a| {
                    if a.is_null(i) { None } else { Some(a.value(i).to_string()) }
                })
        };

        Some(AuditEntry {
            event_id: get_str(0).to_string(),
            user_id: get_str(1).to_string(),
            username: get_str(2).to_string(),
            action: ActionType::from_str(get_str(3)),
            resource: get_opt(4),
            detail: get_str(5).to_string(),
            ip_address: get_opt(6),
            timestamp: get_str(7).to_string(),
            date_partition: get_str(8).to_string(),
        })
    }

    async fn query_entries_sql(&self, sql: &str) -> Result<Vec<AuditEntry>> {
        let batches = self.store.sql(schema::TABLE_AUDIT_LOG, sql).await?;
        let mut entries = Vec::new();
        for batch in &batches {
            for i in 0..batch.num_rows() {
                if let Some(entry) = Self::extract_entry_from_batch(batch, i) {
                    entries.push(entry);
                }
            }
        }
        Ok(entries)
    }
}

// ─── Handle ───

/// Thread-safe handle to communicate with the AuditActor
#[derive(Clone)]
pub struct AuditHandle {
    tx: mpsc::Sender<AuditMsg>,
}

impl AuditHandle {
    /// Log an audit event (fire-and-forget — does not block)
    pub async fn log(
        &self,
        user_id: String,
        username: String,
        action: ActionType,
        resource: Option<String>,
        detail: String,
        ip_address: Option<String>,
    ) {
        let _ = self.tx.send(AuditMsg::Log {
            user_id, username, action, resource, detail, ip_address,
        }).await;
    }

    /// Get recent activity for a user
    pub async fn get_user_activity(&self, user_id: String, limit: usize) -> Vec<AuditEntry> {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(AuditMsg::GetUserActivity { user_id, limit, reply }).await.is_err() {
            return vec![];
        }
        rx.await.unwrap_or_default()
    }

    /// Get billing summary for a user over a date range (YYYY-MM-DD)
    pub async fn billing_summary(
        &self,
        user_id: String,
        start_date: String,
        end_date: String,
    ) -> Result<BillingSummary> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AuditMsg::BillingSummary { user_id, start_date, end_date, reply })
            .await
            .map_err(|_| LakehouseError::ActorUnavailable("AuditActor".into()))?;
        rx.await
            .map_err(|_| LakehouseError::ActorUnavailable("AuditActor dropped".into()))?
    }

    /// Get recent events across all users (admin view)
    pub async fn get_recent_events(&self, limit: usize) -> Vec<AuditEntry> {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(AuditMsg::GetRecentEvents { limit, reply }).await.is_err() {
            return vec![];
        }
        rx.await.unwrap_or_default()
    }
}
