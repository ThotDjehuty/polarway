//! Auth domain types — UserRole, SubscriptionTier, UserRecord
//!
//! Serializable, cloneable, and cheap to pass around.

use serde::{Deserialize, Serialize};

/// User roles with hierarchical permissions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Guest,
    Pending,
    Registered,
    Trader,
    Admin,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Guest => "guest",
            Self::Pending => "pending",
            Self::Registered => "registered",
            Self::Trader => "trader",
            Self::Admin => "admin",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "guest" => Self::Guest,
            "pending" => Self::Pending,
            "registered" => Self::Registered,
            "trader" => Self::Trader,
            "admin" => Self::Admin,
            _ => Self::Guest,
        }
    }

    /// Permission level (higher = more access)
    pub fn level(&self) -> u8 {
        match self {
            Self::Guest => 0,
            Self::Pending => 1,
            Self::Registered => 2,
            Self::Trader => 3,
            Self::Admin => 4,
        }
    }

    /// Check if this role has at least the permissions of `required`
    pub fn has_permission(&self, required: &UserRole) -> bool {
        self.level() >= required.level()
    }
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Subscription tiers (pricing is deployment-specific configuration)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubscriptionTier {
    Free,
    Hobbyist,
    Pioneer,
    Professional,
}

impl SubscriptionTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Hobbyist => "hobbyist",
            Self::Pioneer => "pioneer",
            Self::Professional => "professional",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "hobbyist" => Self::Hobbyist,
            "pioneer" => Self::Pioneer,
            "professional" => Self::Professional,
            _ => Self::Free,
        }
    }

    /// Map tier to default role after admin approval
    pub fn default_role(&self) -> UserRole {
        match self {
            Self::Free => UserRole::Registered,
            Self::Hobbyist => UserRole::Trader,
            Self::Pioneer => UserRole::Trader,
            Self::Professional => UserRole::Trader,
        }
    }
}

impl std::fmt::Display for SubscriptionTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// User record — full user data as stored in the Delta `users` table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub role: UserRole,
    pub subscription_tier: Option<SubscriptionTier>,
    pub first_name: String,
    pub last_name: String,
    pub is_active: bool,
    pub created_at: String,
    pub last_login: Option<String>,
}

impl UserRecord {
    /// Full display name
    pub fn display_name(&self) -> String {
        if !self.first_name.is_empty() || !self.last_name.is_empty() {
            format!("{} {}", self.first_name, self.last_name).trim().to_string()
        } else {
            self.username.clone()
        }
    }

    /// Check if user has required role
    pub fn has_role(&self, required: &UserRole) -> bool {
        self.role.has_permission(required)
    }
}

/// JWT claims for session tokens
#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Subject (user_id)
    pub sub: String,
    /// Username
    pub username: String,
    /// Role string
    pub role: String,
    /// Expiry (Unix timestamp)
    pub exp: usize,
    /// Issued at (Unix timestamp)
    pub iat: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_hierarchy() {
        assert!(UserRole::Admin.has_permission(&UserRole::Trader));
        assert!(UserRole::Trader.has_permission(&UserRole::Registered));
        assert!(!UserRole::Guest.has_permission(&UserRole::Trader));
        assert!(UserRole::Admin.has_permission(&UserRole::Admin));
    }

    #[test]
    fn test_tier_default_role() {
        assert_eq!(SubscriptionTier::Free.default_role(), UserRole::Registered);
        assert_eq!(SubscriptionTier::Pioneer.default_role(), UserRole::Trader);
    }

    #[test]
    fn test_role_serialization() {
        let role = UserRole::Trader;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"trader\"");
        let parsed: UserRole = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, UserRole::Trader);
    }
}
