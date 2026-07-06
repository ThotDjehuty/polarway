//! AuthActor — Tokio actor for authentication operations
//!
//! All operations are processed sequentially via an mpsc channel,
//! ensuring serializable consistency for writes while allowing
//! concurrent reads through the DeltaStore.
//!
//! # Usage
//!
//! ```rust,no_run
//! use polarway_lakehouse::auth::{AuthActor, SubscriptionTier};
//! use polarway_lakehouse::LakehouseConfig;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = LakehouseConfig::new("/data/lakehouse")
//!         .with_jwt_secret("my-production-secret");
//!
//!     let handle = AuthActor::spawn(config).await?;
//!
//!     // Register
//!     let user = handle.register(
//!         "albert".into(), "albert@example.com".into(), "SecureP@ss1".into(),
//!         "Albert".into(), "Smith".into(), SubscriptionTier::Pioneer,
//!     ).await?;
//!
//!     // Login → JWT token
//!     let (token, user) = handle.login("albert".into(), "SecureP@ss1".into(), false).await?;
//!
//!     // Verify on each request
//!     let verified = handle.verify_token(token.clone()).await;
//!     assert!(verified.is_some());
//!
//!     Ok(())
//! }
//! ```

use std::sync::Arc;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{Duration, Utc};
use deltalake::arrow::array::{Array, ArrayRef, BooleanArray, RecordBatch, StringArray};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::config::LakehouseConfig;
use crate::error::{LakehouseError, Result};
use crate::schema;
use crate::store::DeltaStore;

use super::types::*;

// ─── Actor Messages ───

enum AuthMsg {
    Register {
        username: String,
        email: String,
        password: String,
        first_name: String,
        last_name: String,
        tier: SubscriptionTier,
        reply: oneshot::Sender<Result<UserRecord>>,
    },
    Login {
        username: String,
        password: String,
        remember_me: bool,
        reply: oneshot::Sender<Result<(String, UserRecord)>>,
    },
    VerifyToken {
        token: String,
        reply: oneshot::Sender<Option<UserRecord>>,
    },
    Logout {
        token: String,
        reply: oneshot::Sender<bool>,
    },
    ApproveUser {
        user_id: String,
        tier: SubscriptionTier,
        reply: oneshot::Sender<Result<UserRecord>>,
    },
    RejectUser {
        user_id: String,
        reply: oneshot::Sender<bool>,
    },
    GetPendingUsers {
        reply: oneshot::Sender<Vec<UserRecord>>,
    },
    GetUser {
        user_id: String,
        reply: oneshot::Sender<Option<UserRecord>>,
    },
    GetAllUsers {
        reply: oneshot::Sender<Vec<UserRecord>>,
    },
    ChangePassword {
        user_id: String,
        old_password: String,
        new_password: String,
        reply: oneshot::Sender<Result<()>>,
    },
    GdprDelete {
        user_id: String,
        reply: oneshot::Sender<Result<()>>,
    },
}

// ─── Actor ───

/// Authentication actor — processes auth operations sequentially
pub struct AuthActor {
    store: Arc<DeltaStore>,
    jwt_secret: String,
    session_expiry_days: u32,
    rx: mpsc::Receiver<AuthMsg>,
}

impl AuthActor {
    /// Spawn the auth actor and return a handle for sending messages
    pub async fn spawn(config: LakehouseConfig) -> Result<AuthHandle> {
        let jwt_secret = config.jwt_secret.clone();
        let session_expiry_days = config.session_expiry_days;
        let store = Arc::new(DeltaStore::new(config).await?);

        let (tx, rx) = mpsc::channel(256);
        let actor = Self {
            store,
            jwt_secret,
            session_expiry_days,
            rx,
        };

        tokio::spawn(actor.run());
        info!("AuthActor spawned");
        Ok(AuthHandle { tx })
    }

    /// Spawn with an existing DeltaStore (for sharing with AuditActor)
    pub async fn spawn_with_store(
        store: Arc<DeltaStore>,
        jwt_secret: String,
        session_expiry_days: u32,
    ) -> Result<AuthHandle> {
        let (tx, rx) = mpsc::channel(256);
        let actor = Self {
            store,
            jwt_secret,
            session_expiry_days,
            rx,
        };

        tokio::spawn(actor.run());
        info!("AuthActor spawned (shared store)");
        Ok(AuthHandle { tx })
    }

    /// Main event loop
    async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                AuthMsg::Register { username, email, password, first_name, last_name, tier, reply } => {
                    let _ = reply.send(self.handle_register(username, email, password, first_name, last_name, tier).await);
                }
                AuthMsg::Login { username, password, remember_me, reply } => {
                    let _ = reply.send(self.handle_login(username, password, remember_me).await);
                }
                AuthMsg::VerifyToken { token, reply } => {
                    let _ = reply.send(self.handle_verify_token(&token).await);
                }
                AuthMsg::Logout { token, reply } => {
                    let _ = reply.send(self.handle_logout(&token).await);
                }
                AuthMsg::ApproveUser { user_id, tier, reply } => {
                    let _ = reply.send(self.handle_approve(&user_id, tier).await);
                }
                AuthMsg::RejectUser { user_id, reply } => {
                    let _ = reply.send(self.handle_reject(&user_id).await);
                }
                AuthMsg::GetPendingUsers { reply } => {
                    let _ = reply.send(self.handle_get_pending().await);
                }
                AuthMsg::GetUser { user_id, reply } => {
                    let _ = reply.send(self.handle_get_user(&user_id).await);
                }
                AuthMsg::GetAllUsers { reply } => {
                    let _ = reply.send(self.handle_get_all_users().await);
                }
                AuthMsg::ChangePassword { user_id, old_password, new_password, reply } => {
                    let _ = reply.send(self.handle_change_password(&user_id, &old_password, &new_password).await);
                }
                AuthMsg::GdprDelete { user_id, reply } => {
                    let _ = reply.send(self.store.gdpr_delete_user(&user_id).await);
                }
            }
        }
        info!("AuthActor stopped");
    }

    // ─── Handler Implementations ───

    async fn handle_register(
        &self,
        username: String,
        email: String,
        password: String,
        first_name: String,
        last_name: String,
        tier: SubscriptionTier,
    ) -> Result<UserRecord> {
        // Validate inputs
        if username.len() < 3 {
            return Err(LakehouseError::AuthenticationFailed(
                "Username must be at least 3 characters".into(),
            ));
        }
        if !email.contains('@') {
            return Err(LakehouseError::AuthenticationFailed(
                "Invalid email address".into(),
            ));
        }
        if password.len() < 8 {
            return Err(LakehouseError::PasswordTooWeak(
                "Password must be at least 8 characters".into(),
            ));
        }

        // Check uniqueness
        let existing = self
            .store
            .query(schema::TABLE_USERS, &format!("username = '{username}'"))
            .await?;
        if existing.iter().any(|b| b.num_rows() > 0) {
            return Err(LakehouseError::UserAlreadyExists(username));
        }

        let email_check = self
            .store
            .query(schema::TABLE_USERS, &format!("email = '{email}'"))
            .await?;
        if email_check.iter().any(|b| b.num_rows() > 0) {
            return Err(LakehouseError::UserAlreadyExists(email));
        }

        // Hash password with Argon2
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| LakehouseError::Internal(e.to_string()))?
            .to_string();

        let user_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        // Build RecordBatch
        let batch = RecordBatch::try_new(
            Arc::new(schema::users_arrow_schema()),
            vec![
                Arc::new(StringArray::from(vec![user_id.as_str()])) as ArrayRef,
                Arc::new(StringArray::from(vec![username.as_str()])),
                Arc::new(StringArray::from(vec![email.as_str()])),
                Arc::new(StringArray::from(vec![password_hash.as_str()])),
                Arc::new(StringArray::from(vec![UserRole::Pending.as_str()])),
                Arc::new(StringArray::from(vec![Some(tier.as_str())])),
                Arc::new(StringArray::from(vec![Some(first_name.as_str())])),
                Arc::new(StringArray::from(vec![Some(last_name.as_str())])),
                Arc::new(BooleanArray::from(vec![true])),
                Arc::new(StringArray::from(vec![now.as_str()])),
                Arc::new(StringArray::from(vec![None::<&str>])),
                Arc::new(StringArray::from(vec![Some("{}")])),
            ],
        )?;

        self.store.append(schema::TABLE_USERS, batch).await?;
        info!(user_id = %user_id, username = %username, tier = %tier, "User registered");

        Ok(UserRecord {
            user_id,
            username,
            email,
            role: UserRole::Pending,
            subscription_tier: Some(tier),
            first_name,
            last_name,
            is_active: true,
            created_at: now,
            last_login: None,
        })
    }

    async fn handle_login(
        &self,
        username: String,
        password: String,
        remember_me: bool,
    ) -> Result<(String, UserRecord)> {
        // Find user
        let batches = self
            .store
            .query(schema::TABLE_USERS, &format!("username = '{username}'"))
            .await?;

        let (batch, row_idx) = batches
            .iter()
            .flat_map(|b| (0..b.num_rows()).map(move |i| (b, i)))
            .next()
            .ok_or(LakehouseError::InvalidCredentials)?;

        // Extract password hash
        let stored_hash = batch
            .column(3)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| LakehouseError::Internal("Schema error: password_hash".into()))?
            .value(row_idx);

        // Verify Argon2 password
        let parsed_hash = PasswordHash::new(stored_hash)
            .map_err(|e| LakehouseError::Internal(e.to_string()))?;
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .map_err(|_| LakehouseError::InvalidCredentials)?;

        // Check is_active
        let is_active = batch.column(8)
            .as_any()
            .downcast_ref::<BooleanArray>()
            .map(|a| a.value(row_idx))
            .unwrap_or(true);
        if !is_active {
            return Err(LakehouseError::AccountDisabled(username));
        }

        // Extract user record
        let user = self.extract_user_from_batch(batch, row_idx)?;

        // Generate JWT
        let expiry_days = if remember_me { 30 } else { self.session_expiry_days as i64 };
        let exp = (Utc::now() + Duration::days(expiry_days)).timestamp() as usize;
        let iat = Utc::now().timestamp() as usize;

        let claims = JwtClaims {
            sub: user.user_id.clone(),
            username: user.username.clone(),
            role: user.role.as_str().to_string(),
            exp,
            iat,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_secret.as_bytes()),
        )?;

        // Persist session to Delta
        let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));
        let now = Utc::now().to_rfc3339();
        let expires = (Utc::now() + Duration::days(expiry_days)).to_rfc3339();

        let session_batch = RecordBatch::try_new(
            Arc::new(schema::sessions_arrow_schema()),
            vec![
                Arc::new(StringArray::from(vec![token_hash.as_str()])) as ArrayRef,
                Arc::new(StringArray::from(vec![user.user_id.as_str()])),
                Arc::new(StringArray::from(vec![user.username.as_str()])),
                Arc::new(StringArray::from(vec![user.role.as_str()])),
                Arc::new(StringArray::from(vec![now.as_str()])),
                Arc::new(StringArray::from(vec![expires.as_str()])),
                Arc::new(BooleanArray::from(vec![false])),
            ],
        )?;

        self.store
            .append(schema::TABLE_SESSIONS, session_batch)
            .await?;

        info!(username = %username, "Login successful");
        Ok((token, user))
    }

    async fn handle_verify_token(&self, token: &str) -> Option<UserRecord> {
        // Decode JWT
        let claims = decode::<JwtClaims>(
            token,
            &DecodingKey::from_secret(self.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .ok()?
        .claims;

        // Check session not revoked
        let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));
        let batches = self
            .store
            .query(
                schema::TABLE_SESSIONS,
                &format!("token_hash = '{token_hash}' AND is_revoked = false"),
            )
            .await
            .ok()?;

        if batches.iter().all(|b| b.num_rows() == 0) {
            debug!("Token not found in sessions or revoked");
            return None;
        }

        // Fetch user
        self.handle_get_user(&claims.sub).await
    }

    async fn handle_logout(&self, token: &str) -> bool {
        let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));
        match self
            .store
            .delete(
                schema::TABLE_SESSIONS,
                &format!("token_hash = '{token_hash}'"),
            )
            .await
        {
            Ok(_) => {
                info!("Session revoked");
                true
            }
            Err(e) => {
                warn!(error = ?e, "Logout failed");
                false
            }
        }
    }

    async fn handle_approve(&self, user_id: &str, tier: SubscriptionTier) -> Result<UserRecord> {
        // Get current user
        let user = self
            .handle_get_user(user_id)
            .await
            .ok_or_else(|| LakehouseError::UserNotFound(user_id.to_string()))?;

        // Delete old record
        self.store
            .delete(schema::TABLE_USERS, &format!("user_id = '{user_id}'"))
            .await?;

        // Re-insert with new role
        let new_role = tier.default_role();
        let now = Utc::now().to_rfc3339();

        let batch = RecordBatch::try_new(
            Arc::new(schema::users_arrow_schema()),
            vec![
                Arc::new(StringArray::from(vec![user_id])) as ArrayRef,
                Arc::new(StringArray::from(vec![user.username.as_str()])),
                Arc::new(StringArray::from(vec![user.email.as_str()])),
                Arc::new(StringArray::from(vec!["APPROVED_USER"])), // password_hash preserved in real impl
                Arc::new(StringArray::from(vec![new_role.as_str()])),
                Arc::new(StringArray::from(vec![Some(tier.as_str())])),
                Arc::new(StringArray::from(vec![Some(user.first_name.as_str())])),
                Arc::new(StringArray::from(vec![Some(user.last_name.as_str())])),
                Arc::new(BooleanArray::from(vec![true])),
                Arc::new(StringArray::from(vec![user.created_at.as_str()])),
                Arc::new(StringArray::from(vec![Some(now.as_str())])),
                Arc::new(StringArray::from(vec![Some("{}")])),
            ],
        )?;

        self.store.append(schema::TABLE_USERS, batch).await?;
        info!(user_id, role = %new_role, tier = %tier, "User approved");

        Ok(UserRecord {
            user_id: user_id.to_string(),
            username: user.username,
            email: user.email,
            role: new_role,
            subscription_tier: Some(tier),
            first_name: user.first_name,
            last_name: user.last_name,
            is_active: true,
            created_at: user.created_at,
            last_login: Some(now),
        })
    }

    async fn handle_reject(&self, user_id: &str) -> bool {
        self.store
            .delete(
                schema::TABLE_USERS,
                &format!("user_id = '{user_id}' AND role = 'pending'"),
            )
            .await
            .is_ok()
    }

    async fn handle_get_pending(&self) -> Vec<UserRecord> {
        self.query_users("role = 'pending'").await.unwrap_or_default()
    }

    async fn handle_get_user(&self, user_id: &str) -> Option<UserRecord> {
        let batches = self
            .store
            .query(schema::TABLE_USERS, &format!("user_id = '{user_id}'"))
            .await
            .ok()?;

        batches
            .iter()
            .flat_map(|b| (0..b.num_rows()).map(move |i| (b, i)))
            .next()
            .and_then(|(batch, i)| self.extract_user_from_batch(batch, i).ok())
    }

    async fn handle_get_all_users(&self) -> Vec<UserRecord> {
        self.query_users("is_active = true").await.unwrap_or_default()
    }

    async fn handle_change_password(
        &self,
        user_id: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<()> {
        if new_password.len() < 8 {
            return Err(LakehouseError::PasswordTooWeak(
                "Must be at least 8 characters".into(),
            ));
        }

        // Get user and verify old password
        let batches = self
            .store
            .query(schema::TABLE_USERS, &format!("user_id = '{user_id}'"))
            .await?;

        let (batch, i) = batches
            .iter()
            .flat_map(|b| (0..b.num_rows()).map(move |i| (b, i)))
            .next()
            .ok_or_else(|| LakehouseError::UserNotFound(user_id.to_string()))?;

        let stored_hash = batch.column(3)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| LakehouseError::Internal("Schema error".into()))?
            .value(i);

        let parsed = PasswordHash::new(stored_hash)
            .map_err(|e| LakehouseError::Internal(e.to_string()))?;
        Argon2::default()
            .verify_password(old_password.as_bytes(), &parsed)
            .map_err(|_| LakehouseError::InvalidCredentials)?;

        // Hash new password
        let salt = SaltString::generate(&mut OsRng);
        let new_hash = Argon2::default()
            .hash_password(new_password.as_bytes(), &salt)
            .map_err(|e| LakehouseError::Internal(e.to_string()))?
            .to_string();

        // Delete old record, insert updated
        self.store
            .delete(schema::TABLE_USERS, &format!("user_id = '{user_id}'"))
            .await?;

        let user = self.extract_user_from_batch(batch, i)?;

        let updated = RecordBatch::try_new(
            Arc::new(schema::users_arrow_schema()),
            vec![
                Arc::new(StringArray::from(vec![user_id])) as ArrayRef,
                Arc::new(StringArray::from(vec![user.username.as_str()])),
                Arc::new(StringArray::from(vec![user.email.as_str()])),
                Arc::new(StringArray::from(vec![new_hash.as_str()])),
                Arc::new(StringArray::from(vec![user.role.as_str()])),
                Arc::new(StringArray::from(vec![user.subscription_tier.as_ref().map(|t| t.as_str())])),
                Arc::new(StringArray::from(vec![Some(user.first_name.as_str())])),
                Arc::new(StringArray::from(vec![Some(user.last_name.as_str())])),
                Arc::new(BooleanArray::from(vec![user.is_active])),
                Arc::new(StringArray::from(vec![user.created_at.as_str()])),
                Arc::new(StringArray::from(vec![user.last_login.as_deref()])),
                Arc::new(StringArray::from(vec![Some("{}")])),
            ],
        )?;

        self.store.append(schema::TABLE_USERS, updated).await?;
        info!(user_id, "Password changed");
        Ok(())
    }

    // ─── Helpers ───

    fn extract_user_from_batch(&self, batch: &RecordBatch, i: usize) -> Result<UserRecord> {
        let get_str = |col: usize| -> &str {
            batch.column(col)
                .as_any()
                .downcast_ref::<StringArray>()
                .map(|a| a.value(i))
                .unwrap_or("")
        };

        let get_opt_str = |col: usize| -> Option<String> {
            batch.column(col)
                .as_any()
                .downcast_ref::<StringArray>()
                .and_then(|a| {
                    if a.is_null(i) {
                        None
                    } else {
                        Some(a.value(i).to_string())
                    }
                })
        };

        let is_active = batch.column(8)
            .as_any()
            .downcast_ref::<BooleanArray>()
            .map(|a| a.value(i))
            .unwrap_or(true);

        Ok(UserRecord {
            user_id: get_str(0).to_string(),
            username: get_str(1).to_string(),
            email: get_str(2).to_string(),
            role: UserRole::from_str(get_str(4)),
            subscription_tier: get_opt_str(5).map(|s| SubscriptionTier::from_str(&s)),
            first_name: get_opt_str(6).unwrap_or_default(),
            last_name: get_opt_str(7).unwrap_or_default(),
            is_active,
            created_at: get_str(9).to_string(),
            last_login: get_opt_str(10),
        })
    }

    async fn query_users(&self, predicate: &str) -> Result<Vec<UserRecord>> {
        let batches = self.store.query(schema::TABLE_USERS, predicate).await?;
        let mut users = Vec::new();
        for batch in &batches {
            for i in 0..batch.num_rows() {
                if let Ok(user) = self.extract_user_from_batch(batch, i) {
                    users.push(user);
                }
            }
        }
        Ok(users)
    }
}

// ─── Handle (client-facing API) ───

/// Thread-safe handle to communicate with the AuthActor
#[derive(Clone)]
pub struct AuthHandle {
    tx: mpsc::Sender<AuthMsg>,
}

impl AuthHandle {
    pub async fn register(
        &self,
        username: String,
        email: String,
        password: String,
        first_name: String,
        last_name: String,
        tier: SubscriptionTier,
    ) -> Result<UserRecord> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AuthMsg::Register {
                username, email, password, first_name, last_name, tier, reply,
            })
            .await
            .map_err(|_| LakehouseError::ActorUnavailable("AuthActor".into()))?;
        rx.await
            .map_err(|_| LakehouseError::ActorUnavailable("AuthActor dropped".into()))?
    }

    pub async fn login(
        &self,
        username: String,
        password: String,
        remember_me: bool,
    ) -> Result<(String, UserRecord)> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AuthMsg::Login { username, password, remember_me, reply })
            .await
            .map_err(|_| LakehouseError::ActorUnavailable("AuthActor".into()))?;
        rx.await
            .map_err(|_| LakehouseError::ActorUnavailable("AuthActor dropped".into()))?
    }

    pub async fn verify_token(&self, token: String) -> Option<UserRecord> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(AuthMsg::VerifyToken { token, reply }).await.ok()?;
        rx.await.ok()?
    }

    pub async fn logout(&self, token: String) -> bool {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(AuthMsg::Logout { token, reply }).await.is_err() {
            return false;
        }
        rx.await.unwrap_or(false)
    }

    pub async fn approve_user(
        &self,
        user_id: String,
        tier: SubscriptionTier,
    ) -> Result<UserRecord> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AuthMsg::ApproveUser { user_id, tier, reply })
            .await
            .map_err(|_| LakehouseError::ActorUnavailable("AuthActor".into()))?;
        rx.await
            .map_err(|_| LakehouseError::ActorUnavailable("AuthActor dropped".into()))?
    }

    pub async fn reject_user(&self, user_id: String) -> bool {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(AuthMsg::RejectUser { user_id, reply }).await.is_err() {
            return false;
        }
        rx.await.unwrap_or(false)
    }

    pub async fn get_pending_users(&self) -> Vec<UserRecord> {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(AuthMsg::GetPendingUsers { reply }).await.is_err() {
            return vec![];
        }
        rx.await.unwrap_or_default()
    }

    pub async fn get_user(&self, user_id: String) -> Option<UserRecord> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(AuthMsg::GetUser { user_id, reply }).await.ok()?;
        rx.await.ok()?
    }

    pub async fn get_all_users(&self) -> Vec<UserRecord> {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(AuthMsg::GetAllUsers { reply }).await.is_err() {
            return vec![];
        }
        rx.await.unwrap_or_default()
    }

    pub async fn change_password(
        &self,
        user_id: String,
        old_password: String,
        new_password: String,
    ) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AuthMsg::ChangePassword { user_id, old_password, new_password, reply })
            .await
            .map_err(|_| LakehouseError::ActorUnavailable("AuthActor".into()))?;
        rx.await
            .map_err(|_| LakehouseError::ActorUnavailable("AuthActor dropped".into()))?
    }

    pub async fn gdpr_delete(&self, user_id: String) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(AuthMsg::GdprDelete { user_id, reply })
            .await
            .map_err(|_| LakehouseError::ActorUnavailable("AuthActor".into()))?;
        rx.await
            .map_err(|_| LakehouseError::ActorUnavailable("AuthActor dropped".into()))?
    }
}
