//! Authentication Service
//!
//! Centralized authentication and authorization for all trading operations.
//! Manages API keys, JWT tokens, and permissions across multiple exchanges.

pub mod api;
pub mod binance_service;
pub mod config;
pub mod grpc;
pub mod providers;
pub mod zerodha_service;

use anyhow::Result;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// Authentication service configuration
#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    /// JWT secret for token signing
    pub jwt_secret: String,
    /// Token expiry in seconds
    pub token_expiry: u64,
    /// API rate limits per user
    pub rate_limits: FxHashMap<String, u32>,
}

/// User authentication context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    /// User ID
    pub user_id: String,
    /// Active permissions
    pub permissions: Vec<Permission>,
    /// API keys for different exchanges
    pub api_keys: FxHashMap<String, String>,
    /// Session metadata
    pub metadata: FxHashMap<String, String>,
}

/// Permission types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Permission {
    /// Read market data
    ReadMarketData,
    /// Place orders
    PlaceOrders,
    /// Cancel orders
    CancelOrders,
    /// View positions
    ViewPositions,
    /// Modify risk limits
    ModifyRiskLimits,
    /// Admin access
    Admin,
}

/// Authentication service trait
#[tonic::async_trait]
pub trait AuthService: Send + Sync {
    /// Authenticate user with credentials
    async fn authenticate(&self, username: &str, password: &str) -> Result<AuthContext>;

    /// Validate JWT token
    async fn validate_token(&self, token: &str) -> Result<AuthContext>;

    /// Generate new JWT token
    async fn generate_token(&self, context: &AuthContext) -> Result<String>;

    /// Check permission
    async fn check_permission(&self, context: &AuthContext, permission: Permission) -> bool;

    /// Revoke token
    async fn revoke_token(&self, token: &str) -> Result<()>;
}

/// Service implementation
pub struct AuthServiceImpl {
    config: AuthConfig,
}

impl AuthServiceImpl {
    pub fn new(config: AuthConfig) -> Self {
        Self { config }
    }
}

#[tonic::async_trait]
impl AuthService for AuthServiceImpl {
    async fn authenticate(&self, username: &str, _password: &str) -> Result<AuthContext> {
        // For demo, accept any credentials
        let mut api_keys = FxHashMap::default();
        api_keys.insert("demo".to_string(), "demo-api-key".to_string());

        let mut metadata = FxHashMap::default();
        metadata.insert("login_time".to_string(), chrono::Utc::now().to_rfc3339());

        Ok(AuthContext {
            user_id: username.to_string(),
            permissions: vec![Permission::ReadMarketData, Permission::PlaceOrders],
            api_keys,
            metadata,
        })
    }

    async fn validate_token(&self, token: &str) -> Result<AuthContext> {
        use jsonwebtoken::{DecodingKey, Validation, decode};

        let key = DecodingKey::from_secret(self.config.jwt_secret.as_bytes());
        let validation = Validation::default();

        match decode::<AuthContext>(token, &key, &validation) {
            Ok(token_data) => Ok(token_data.claims),
            Err(e) => Err(anyhow::anyhow!("Invalid token: {}", e)),
        }
    }

    async fn generate_token(&self, context: &AuthContext) -> Result<String> {
        use jsonwebtoken::{EncodingKey, Header, encode};

        let key = EncodingKey::from_secret(self.config.jwt_secret.as_bytes());
        let header = Header::default();

        match encode(&header, context, &key) {
            Ok(token) => Ok(token),
            Err(e) => Err(anyhow::anyhow!("Failed to generate token: {}", e)),
        }
    }

    async fn check_permission(&self, context: &AuthContext, permission: Permission) -> bool {
        context.permissions.contains(&permission)
    }

    async fn revoke_token(&self, _token: &str) -> Result<()> {
        // For demo, just return success
        // In production, would maintain a revocation list
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_check() {
        let context = AuthContext {
            user_id: "test_user".to_string(),
            permissions: vec![Permission::ReadMarketData, Permission::PlaceOrders],
            api_keys: FxHashMap::default(),
            metadata: FxHashMap::default(),
        };

        assert!(context.permissions.contains(&Permission::ReadMarketData));
        assert!(!context.permissions.contains(&Permission::Admin));
    }
}
