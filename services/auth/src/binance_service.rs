//! Binance-integrated Authentication Service for gRPC
//!
//! Provides authentication for Binance Spot and Futures trading

use crate::{
    AuthContext, AuthService, Permission,
    providers::binance_enhanced::{BinanceAuth, BinanceConfig, BinanceEndpoint},
};
use anyhow::{Result, anyhow};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Binance auth service with gRPC integration
pub struct BinanceAuthService {
    /// Binance Spot authentication
    spot_auth: Option<Arc<BinanceAuth>>,
    /// Binance USD-M Futures authentication
    futures_auth: Option<Arc<BinanceAuth>>,
    /// JWT secret for token signing
    jwt_secret: String,
    /// Token expiry in seconds
    token_expiry: u64,
    /// Cache of validated tokens
    token_cache: Arc<RwLock<FxHashMap<String, AuthContext>>>,
    /// Revoked tokens
    revoked_tokens: Arc<RwLock<FxHashSet<String>>>,
}

impl BinanceAuthService {
    /// Create new Binance auth service
    pub fn new(jwt_secret: String, token_expiry: u64) -> Result<Self> {
        let mut spot_auth = None;
        let mut futures_auth = None;

        // Try to load Spot credentials
        if std::env::var("BINANCE_SPOT_API_KEY").is_ok() {
            match BinanceConfig::from_env_file(BinanceEndpoint::Spot) {
                Ok(config) => {
                    info!("Loaded Binance Spot configuration");
                    spot_auth = Some(Arc::new(BinanceAuth::new(config)));
                }
                Err(e) => warn!("Failed to load Binance Spot config: {}", e),
            }
        }

        // Try to load Futures credentials
        if std::env::var("BINANCE_FUTURES_API_KEY").is_ok() {
            match BinanceConfig::from_env_file(BinanceEndpoint::UsdFutures) {
                Ok(config) => {
                    info!("Loaded Binance USD-M Futures configuration");
                    futures_auth = Some(Arc::new(BinanceAuth::new(config)));
                }
                Err(e) => warn!("Failed to load Binance Futures config: {}", e),
            }
        }

        if spot_auth.is_none() && futures_auth.is_none() {
            return Err(anyhow!("No Binance credentials found in environment"));
        }

        Ok(Self {
            spot_auth,
            futures_auth,
            jwt_secret,
            token_expiry,
            token_cache: Arc::new(RwLock::new(FxHashMap::default())),
            revoked_tokens: Arc::new(RwLock::new(FxHashSet::default())),
        })
    }

    /// Authenticate with Binance
    async fn authenticate_binance(&self, username: &str, _password: &str) -> Result<AuthContext> {
        // Username format: "binance_spot" or "binance_futures"
        let (exchange, market) = if username.starts_with("binance_") {
            let parts: Vec<&str> = username.split('_').collect();
            if parts.len() == 2 {
                ("binance", parts[1])
            } else {
                ("binance", "spot")
            }
        } else {
            return Err(anyhow!(
                "Invalid username format. Use: binance_spot or binance_futures"
            ));
        };

        // Select appropriate auth handler
        let auth = match market {
            "spot" => self
                .spot_auth
                .as_ref()
                .ok_or_else(|| anyhow!("Binance Spot not configured"))?,
            "futures" | "usd" | "usdfutures" => self
                .futures_auth
                .as_ref()
                .ok_or_else(|| anyhow!("Binance Futures not configured"))?,
            _ => return Err(anyhow!("Unknown market: {}", market)),
        };

        // Validate credentials
        info!("Authenticating with Binance {} market", market);
        if !auth.validate_credentials().await? {
            return Err(anyhow!("Invalid Binance credentials"));
        }

        // Get account information
        let (can_trade, balances) = if market == "spot" {
            let account = auth.get_account_info().await?;
            let balance_str = account
                .balances
                .iter()
                .filter_map(|b| {
                    let free = b.free.parse::<f64>().unwrap_or(0.0);
                    if free > 0.0 {
                        Some(format!("{}:{}", b.asset, free))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            (account.can_trade, balance_str)
        } else {
            let account = auth.get_futures_account_info().await?;
            let balance_str = format!(
                "wallet:{},available:{}",
                account.total_wallet_balance, account.available_balance
            );
            (true, balance_str)
        };

        // Create listen key for WebSocket
        let listen_key = auth.create_listen_key().await?;

        // Build API keys map
        let mut api_keys = FxHashMap::default();
        api_keys.insert(
            format!("binance_{market}_api_key"),
            auth.get_api_key().to_string(),
        );
        api_keys.insert(format!("binance_{market}_listen_key"), listen_key);

        // Build metadata
        let mut metadata = FxHashMap::default();
        metadata.insert("login_time".to_string(), chrono::Utc::now().to_rfc3339());
        metadata.insert("exchange".to_string(), exchange.to_string());
        metadata.insert("market".to_string(), market.to_string());
        metadata.insert("balances".to_string(), balances);

        // Determine permissions
        let mut permissions = vec![Permission::ReadMarketData, Permission::ViewPositions];
        if can_trade {
            permissions.push(Permission::PlaceOrders);
            permissions.push(Permission::CancelOrders);
        }

        // For futures, add risk management permission
        if market != "spot" {
            permissions.push(Permission::ModifyRiskLimits);
        }

        Ok(AuthContext {
            user_id: username.to_string(),
            permissions,
            api_keys,
            metadata,
        })
    }
}

#[tonic::async_trait]
impl AuthService for BinanceAuthService {
    async fn authenticate(&self, username: &str, password: &str) -> Result<AuthContext> {
        self.authenticate_binance(username, password).await
    }

    async fn validate_token(&self, token: &str) -> Result<AuthContext> {
        use jsonwebtoken::{DecodingKey, Validation, decode};

        // Check if token is revoked
        if self.revoked_tokens.read().await.contains(token) {
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

        debug!("Token revoked successfully");
        Ok(())
    }
}

/// Create Binance service with both Spot and Futures
pub async fn create_binance_service() -> Result<BinanceAuthService> {
    dotenv::dotenv().ok();

    let jwt_secret =
        std::env::var("JWT_SECRET").unwrap_or_else(|_| "change-me-in-production".to_string());
    let token_expiry = std::env::var("TOKEN_EXPIRY")
        .unwrap_or_else(|_| "3600".to_string())
        .parse()
        .unwrap_or(3600);

    BinanceAuthService::new(jwt_secret, token_expiry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_binance_service_creation() {
        // This will fail if no credentials, which is expected in tests
        let result = create_binance_service().await;

        if std::env::var("BINANCE_SPOT_API_KEY").is_ok()
            || std::env::var("BINANCE_FUTURES_API_KEY").is_ok()
        {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
        }
    }
}
