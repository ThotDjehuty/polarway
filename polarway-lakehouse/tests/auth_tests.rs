//! AuthActor integration tests — register, login, verify, approve, GDPR

use tempfile::TempDir;

use polarway_lakehouse::auth::{AuthActor, SubscriptionTier, UserRole};
use polarway_lakehouse::config::LakehouseConfig;

fn test_config(dir: &TempDir) -> LakehouseConfig {
    LakehouseConfig::new(dir.path().to_str().unwrap())
        .with_jwt_secret("test-secret-jwt-key-min-32-chars!!")
}

#[tokio::test]
async fn test_register_and_login() {
    let dir = TempDir::new().unwrap();
    let handle = AuthActor::spawn(test_config(&dir)).await.unwrap();

    // Register
    let user = handle
        .register(
            "bob".into(),
            "bob@example.com".into(),
            "StrongP@ss123".into(),
            "bob".into(),
            "Smith".into(),
            SubscriptionTier::Pioneer,
        )
        .await
        .unwrap();

    assert_eq!(user.username, "bob");
    assert_eq!(user.role, UserRole::Pending);
    assert_eq!(user.subscription_tier, Some(SubscriptionTier::Pioneer));

    // Login fails for pending users? No — login should succeed, just role is pending
    let (token, logged_in) = handle
        .login("bob".into(), "StrongP@ss123".into(), false)
        .await
        .unwrap();

    assert!(!token.is_empty());
    assert_eq!(logged_in.username, "bob");
}

#[tokio::test]
async fn test_verify_token() {
    let dir = TempDir::new().unwrap();
    let handle = AuthActor::spawn(test_config(&dir)).await.unwrap();

    handle
        .register(
            "bob".into(),
            "bob@example.com".into(),
            "SecureP@ss99".into(),
            "Bob".into(),
            "Jones".into(),
            SubscriptionTier::Hobbyist,
        )
        .await
        .unwrap();

    let (token, _) = handle
        .login("bob".into(), "SecureP@ss99".into(), false)
        .await
        .unwrap();

    // Verify valid token
    let user = handle.verify_token(token.clone()).await;
    assert!(user.is_some());
    assert_eq!(user.unwrap().username, "bob");

    // Verify invalid token
    let bad = handle.verify_token("invalid.token.here".into()).await;
    assert!(bad.is_none());
}

#[tokio::test]
async fn test_logout() {
    let dir = TempDir::new().unwrap();
    let handle = AuthActor::spawn(test_config(&dir)).await.unwrap();

    handle
        .register(
            "charlie".into(),
            "charlie@example.com".into(),
            "MyP@ssword1".into(),
            "Charlie".into(),
            "Brown".into(),
            SubscriptionTier::Free,
        )
        .await
        .unwrap();

    let (token, _) = handle
        .login("charlie".into(), "MyP@ssword1".into(), false)
        .await
        .unwrap();

    // Logout
    let ok = handle.logout(token.clone()).await;
    assert!(ok);

    // Token should no longer verify
    let user = handle.verify_token(token).await;
    assert!(user.is_none());
}

#[tokio::test]
async fn test_approve_user() {
    let dir = TempDir::new().unwrap();
    let handle = AuthActor::spawn(test_config(&dir)).await.unwrap();

    let user = handle
        .register(
            "diana".into(),
            "diana@example.com".into(),
            "Tr@derPass1".into(),
            "Diana".into(),
            "Prince".into(),
            SubscriptionTier::Professional,
        )
        .await
        .unwrap();

    assert_eq!(user.role, UserRole::Pending);

    // Approve
    let approved = handle
        .approve_user(user.user_id.clone(), SubscriptionTier::Professional)
        .await
        .unwrap();

    assert_eq!(approved.role, UserRole::Trader);
    assert_eq!(approved.subscription_tier, Some(SubscriptionTier::Professional));

    // Get pending — should be empty now
    let pending = handle.get_pending_users().await;
    assert!(pending.is_empty());
}

#[tokio::test]
async fn test_duplicate_registration() {
    let dir = TempDir::new().unwrap();
    let handle = AuthActor::spawn(test_config(&dir)).await.unwrap();

    handle
        .register(
            "eve".into(),
            "eve@example.com".into(),
            "P@ssword123".into(),
            "Eve".into(),
            "Adams".into(),
            SubscriptionTier::Free,
        )
        .await
        .unwrap();

    // Duplicate username should fail
    let result = handle
        .register(
            "eve".into(),
            "eve2@example.com".into(),
            "P@ssword123".into(),
            "Eve".into(),
            "Adams".into(),
            SubscriptionTier::Free,
        )
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_wrong_password() {
    let dir = TempDir::new().unwrap();
    let handle = AuthActor::spawn(test_config(&dir)).await.unwrap();

    handle
        .register(
            "frank".into(),
            "frank@example.com".into(),
            "Correct!Pass1".into(),
            "Frank".into(),
            "Castle".into(),
            SubscriptionTier::Free,
        )
        .await
        .unwrap();

    let result = handle
        .login("frank".into(), "WrongPassword".into(), false)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_change_password() {
    let dir = TempDir::new().unwrap();
    let handle = AuthActor::spawn(test_config(&dir)).await.unwrap();

    let user = handle
        .register(
            "grace".into(),
            "grace@example.com".into(),
            "OldP@ss1234".into(),
            "Grace".into(),
            "Hopper".into(),
            SubscriptionTier::Pioneer,
        )
        .await
        .unwrap();

    // Change password
    handle
        .change_password(
            user.user_id.clone(),
            "OldP@ss1234".into(),
            "NewP@ss5678".into(),
        )
        .await
        .unwrap();

    // Old password should fail
    let old_login = handle
        .login("grace".into(), "OldP@ss1234".into(), false)
        .await;
    assert!(old_login.is_err());

    // New password should work
    let new_login = handle
        .login("grace".into(), "NewP@ss5678".into(), false)
        .await;
    assert!(new_login.is_ok());
}

#[tokio::test]
async fn test_gdpr_delete() {
    let dir = TempDir::new().unwrap();
    let handle = AuthActor::spawn(test_config(&dir)).await.unwrap();

    let user = handle
        .register(
            "henry".into(),
            "henry@example.com".into(),
            "Gdpr!Delete1".into(),
            "Henry".into(),
            "Jekyll".into(),
            SubscriptionTier::Free,
        )
        .await
        .unwrap();

    // GDPR delete
    handle.gdpr_delete(user.user_id.clone()).await.unwrap();

    // User should no longer exist
    let found = handle.get_user(user.user_id).await;
    assert!(found.is_none());
}

#[tokio::test]
async fn test_get_all_users() {
    let dir = TempDir::new().unwrap();
    let handle = AuthActor::spawn(test_config(&dir)).await.unwrap();

    for (name, email) in [("user1", "u1@e.com"), ("user2", "u2@e.com"), ("user3", "u3@e.com")] {
        handle
            .register(
                name.into(),
                email.into(),
                "TestP@ss123".into(),
                "Test".into(),
                "User".into(),
                SubscriptionTier::Free,
            )
            .await
            .unwrap();
    }

    let all = handle.get_all_users().await;
    assert_eq!(all.len(), 3);
}
