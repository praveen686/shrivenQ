//! Zerodha-integrated Authentication Service
//!
//! This module provides a complete authentication service that integrates
//! with Zerodha's `KiteConnect` API for real trading authentication.

use crate::{
    AuthContext, AuthService, Permission,
    providers::zerodha::{ZerodhaAuth, ZerodhaConfig},
};
use anyhow::{Result, anyhow};
use services_common::constants::time::DEFAULT_TOKEN_EXPIRY_SECS;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Enhanced auth service with Zerodha integration
#[derive(Debug)]
pub struct ZerodhaAuthService {
    /// Zerodha authentication handler
    zerodha_auth: Arc<ZerodhaAuth>,
    /// JWT secret for token signing
    jwt_secret: String,
    /// Token expiry in seconds
    token_expiry: u64,
    /// Cache of validated tokens
    token_cache: Arc<RwLock<FxHashMap<String, AuthContext>>>,
    /// Revoked tokens
    revoked_tokens: Arc<RwLock<FxHashSet<String>>>,
}

use rustc_hash::FxHashSet;

impl ZerodhaAuthService {
    /// Create new Zerodha auth service
    pub fn new(jwt_secret: String, token_expiry: u64) -> Result<Self> {
        // Load Zerodha config from .env
        let config = ZerodhaConfig::from_env_file()?;
        let zerodha_auth = Arc::new(ZerodhaAuth::new(config));

        Ok(Self {
            zerodha_auth,
            jwt_secret,
            token_expiry,
            token_cache: Arc::new(RwLock::new(FxHashMap::default())),
            revoked_tokens: Arc::new(RwLock::new(FxHashSet::default())),
        })
    }

    /// Create with custom Zerodha config
    #[must_use] pub fn with_config(config: ZerodhaConfig, jwt_secret: String, token_expiry: u64) -> Self {
        Self {
            zerodha_auth: Arc::new(ZerodhaAuth::new(config)),
            jwt_secret,
            token_expiry,
            token_cache: Arc::new(RwLock::new(FxHashMap::default())),
            revoked_tokens: Arc::new(RwLock::new(FxHashSet::default())),
        }
    }

    /// Authenticate with Zerodha credentials
    async fn authenticate_zerodha(&self, username: &str, _password: &str) -> Result<AuthContext> {
        // Verify username matches configured user
        if username != self.zerodha_auth.get_api_key() {
            return Err(anyhow!("Invalid username"));
        }

        // Perform Zerodha authentication
        info!("Authenticating with Zerodha for user: {}", username);
        let access_token = self.zerodha_auth.authenticate().await?;

        // Get user profile
        let profile = self.zerodha_auth.get_profile().await?;

        // Get margins to determine permissions
        let margins = self.zerodha_auth.get_margins().await?;

        // Build API keys map
        let mut api_keys = FxHashMap::default();
        api_keys.insert("zerodha".to_string(), access_token.clone());
        api_keys.insert(
            "zerodha_api_key".to_string(),
            self.zerodha_auth.get_api_key(),
        );

        // Build metadata
        let mut metadata = FxHashMap::default();
        metadata.insert("login_time".to_string(), chrono::Utc::now().to_rfc3339());
        metadata.insert("email".to_string(), profile.email);
        metadata.insert("exchanges".to_string(), profile.exchanges.join(","));
        metadata.insert("products".to_string(), profile.products.join(","));

        // Determine permissions based on account type
        let mut permissions = vec![Permission::ReadMarketData, Permission::ViewPositions];

        // Check if user has trading permissions
        if profile.products.contains(&"MIS".to_string())
            || profile.products.contains(&"CNC".to_string())
        {
            permissions.push(Permission::PlaceOrders);
            permissions.push(Permission::CancelOrders);
        }

        // Check if user has margin trading
        if let Some(equity) = margins.get("equity") {
            if let Some(available) = equity.available.get("cash") {
                if *available > 0.0 {
                    metadata.insert("available_margin".to_string(), available.to_string());
                }
            }
        }

        Ok(AuthContext {
            user_id: profile.user_id,
            permissions,
            api_keys,
            metadata,
        })
    }
}

#[tonic::async_trait]
impl AuthService for ZerodhaAuthService {
    async fn authenticate(&self, username: &str, password: &str) -> Result<AuthContext> {
        self.authenticate_zerodha(username, password).await
    }

    async fn validate_token(&self, token: &str) -> Result<AuthContext> {
        use jsonwebtoken::{DecodingKey, Validation, decode};

        // Check if token is revoked
        let is_revoked = self.revoked_tokens.read().await.contains(token);

        if is_revoked {
            return Err(anyhow!("Token has been revoked"));
        }

        // Check cache first
        if let Some(context) = self.token_cache.read().await.get(token).cloned() {
            return Ok(context);
        }

        // Validate JWT
        let key = DecodingKey::from_secret(self.jwt_secret.as_bytes());
        let validation = Validation::default();

        match decode::<AuthContext>(token, &key, &validation) {
            Ok(token_data) => {
                let context = token_data.claims;

                // Cache the validated token
                self.token_cache
                    .write()
                    .await
                    .insert(token.to_string(), context.clone());

                Ok(context)
            }
            Err(e) => Err(anyhow!("Invalid token: {}", e)),
        }
    }

    async fn generate_token(&self, context: &AuthContext) -> Result<String> {
        use jsonwebtoken::{EncodingKey, Header, encode};

        let key = EncodingKey::from_secret(self.jwt_secret.as_bytes());
        let header = Header::default();

        // Add expiry to claims
        let mut enriched_context = context.clone();
        enriched_context.metadata.insert(
            "exp".to_string(),
            // SAFETY: Explicit bounds check ensures token_expiry fits in i64
            (chrono::Utc::now().timestamp()
                + if i64::try_from(self.token_expiry).is_ok() {
                    // SAFETY: Bounds checked above
                    self.token_expiry as i64
                } else {
                    i64::MAX // Cap at maximum safe value
                })
            .to_string(),
        );

        match encode(&header, &enriched_context, &key) {
            Ok(token) => {
                // Cache the token
                self.token_cache
                    .write()
                    .await
                    .insert(token.clone(), enriched_context);

                Ok(token)
            }
            Err(e) => Err(anyhow!("Failed to generate token: {}", e)),
        }
    }

    async fn check_permission(&self, context: &AuthContext, permission: Permission) -> bool {
        context.permissions.contains(&permission)
            || context.permissions.contains(&Permission::Admin)
    }

    async fn revoke_token(&self, token: &str) -> Result<()> {
        // Add to revoked set
        self.revoked_tokens.write().await.insert(token.to_string());

        // Remove from cache
        self.token_cache.write().await.remove(token);

        info!("Token revoked successfully");
        Ok(())
    }
}

/// Factory function to create auth service based on configuration
pub fn create_auth_service() -> Result<Arc<dyn AuthService>> {
    // Check if credentials are available
    dotenv::dotenv().ok();

    // Check for Zerodha credentials
    if std::env::var("ZERODHA_API_KEY").is_ok() {
        info!("Using Zerodha authentication service");
        let jwt_secret =
            std::env::var("JWT_SECRET").unwrap_or_else(|_| "change-me-in-production".to_string());
        let token_expiry = std::env::var("TOKEN_EXPIRY")
            .unwrap_or_else(|_| DEFAULT_TOKEN_EXPIRY_SECS.to_string())
            .parse()
            .unwrap_or(DEFAULT_TOKEN_EXPIRY_SECS);

        Ok(Arc::new(ZerodhaAuthService::new(jwt_secret, token_expiry)?))
    }
    // Check for Binance credentials
    else if std::env::var("BINANCE_SPOT_API_KEY").is_ok()
        || std::env::var("BINANCE_FUTURES_API_KEY").is_ok()
    {
        info!("Using Binance authentication service");
        use super::binance_service::BinanceAuthService;

        let jwt_secret =
            std::env::var("JWT_SECRET").unwrap_or_else(|_| "change-me-in-production".to_string());
        let token_expiry = std::env::var("TOKEN_EXPIRY")
            .unwrap_or_else(|_| DEFAULT_TOKEN_EXPIRY_SECS.to_string())
            .parse()
            .unwrap_or(DEFAULT_TOKEN_EXPIRY_SECS);

        Ok(Arc::new(BinanceAuthService::new(jwt_secret, token_expiry)?))
    } else {
        warn!("No exchange credentials found, using demo auth service");
        use crate::{AuthConfig, AuthServiceImpl};

        let mut rate_limits = FxHashMap::default();
        rate_limits.insert("default".to_string(), 100);

        let config = AuthConfig {
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "change-me-in-production".to_string()),
            token_expiry: 3600,
            rate_limits,
        };

        Ok(Arc::new(AuthServiceImpl::new(config)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_zerodha_service_creation() {
        // This will use demo service if no credentials
        let service = create_auth_service();
        assert!(service.is_ok());
    }
}
