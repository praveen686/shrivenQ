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

/// JWT Claims structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Claims {
    /// Subject (user_id)
    sub: String,
    /// Expiration time
    exp: i64,
    /// Not before
    nbf: i64,
    /// Issued at
    iat: i64,
    /// Custom auth context
    context: AuthContext,
}

/// Permission types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    
    /// Validate credentials without generating token (useful for health checks)
    async fn validate_credentials(&self, username: &str, password: &str) -> Result<bool> {
        // Default implementation: try to authenticate and return success/failure
        match self.authenticate(username, password).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

/// Service implementation
#[derive(Debug)]
pub struct AuthServiceImpl {
    config: AuthConfig,
}

impl AuthServiceImpl {
    /// Create a new AuthServiceImpl instance with the provided configuration
    #[must_use] pub const fn new(config: AuthConfig) -> Self {
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
        use jsonwebtoken::{DecodingKey, Validation, decode, Algorithm};

        let key = DecodingKey::from_secret(self.config.jwt_secret.as_bytes());
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        validation.validate_nbf = true;

        match decode::<Claims>(token, &key, &validation) {
            Ok(token_data) => Ok(token_data.claims.context),
            Err(e) => Err(anyhow::anyhow!("Invalid token: {}", e)),
        }
    }

    async fn generate_token(&self, context: &AuthContext) -> Result<String> {
        use jsonwebtoken::{EncodingKey, Header, encode};

        let key = EncodingKey::from_secret(self.config.jwt_secret.as_bytes());
        let header = Header::default();
        
        let now = chrono::Utc::now();
        let claims = Claims {
            sub: context.user_id.clone(),
            exp: (now + chrono::Duration::seconds(self.config.token_expiry as i64)).timestamp(),
            nbf: now.timestamp(),
            iat: now.timestamp(),
            context: context.clone(),
        };

        match encode(&header, &claims, &key) {
            Ok(token) => Ok(token),
            Err(e) => Err(anyhow::anyhow!("Failed to generate token: {}", e)),
        }
    }

    async fn check_permission(&self, context: &AuthContext, permission: Permission) -> bool {
        // Admin has all permissions
        if context.permissions.contains(&Permission::Admin) {
            return true;
        }
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
