//! Production-Ready Authentication Client Demo
//!
//! This demonstrates how ShrivenQuant services would use authentication
//! in a real production environment with proper error handling,
//! connection management, and token lifecycle.

use anyhow::{Result, anyhow};
use auth_service::providers::binance_enhanced::{BinanceAuth, BinanceConfig, BinanceEndpoint};
use rustc_hash::FxHashMap;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};

/// Production authentication manager
pub struct ProductionAuthManager {
    binance_spot: Option<BinanceAuth>,
    binance_futures: Option<BinanceAuth>,
    zerodha_auth: Option<String>, // Would be ZerodhaAuth in real implementation
    active_tokens: FxHashMap<String, TokenInfo>,
}

/// Token with metadata for production use
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub token: String,
    pub exchange: String,
    pub market: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub permissions: Vec<String>,
    pub last_validated: chrono::DateTime<chrono::Utc>,
}

/// Service client that uses authentication
pub struct MarketDataClient {
    auth_manager: ProductionAuthManager,
    endpoint: String,
}

impl ProductionAuthManager {
    /// Initialize production auth manager
    pub async fn new() -> Result<Self> {
        info!("🔧 Initializing Production Authentication Manager");

        // Load environment variables
        dotenv::dotenv().ok();

        let mut manager = Self {
            binance_spot: None,
            binance_futures: None,
            zerodha_auth: None,
            active_tokens: FxHashMap::default(),
        };

        // Initialize Binance Spot if configured
        if std::env::var("BINANCE_SPOT_API_KEY").is_ok() {
            match BinanceConfig::from_env_file(BinanceEndpoint::Spot) {
                Ok(config) => {
                    let auth = BinanceAuth::new(config);
                    info!("✅ Binance Spot authentication initialized");
                    manager.binance_spot = Some(auth);
                }
                Err(e) => warn!("⚠️  Failed to initialize Binance Spot: {}", e),
            }
        }

        // Initialize Binance Futures if configured
        if std::env::var("BINANCE_FUTURES_API_KEY").is_ok() {
            match BinanceConfig::from_env_file(BinanceEndpoint::UsdFutures) {
                Ok(config) => {
                    let auth = BinanceAuth::new(config);
                    info!("✅ Binance Futures authentication initialized");
                    manager.binance_futures = Some(auth);
                }
                Err(e) => warn!("⚠️  Failed to initialize Binance Futures: {}", e),
            }
        }

        // Check for Zerodha
        if std::env::var("ZERODHA_API_KEY").is_ok() {
            info!("✅ Zerodha credentials detected");
            manager.zerodha_auth = Some("zerodha-placeholder".to_string());
        }

        Ok(manager)
    }

    /// Authenticate and get token for a specific exchange/market
    pub async fn authenticate(&mut self, exchange: &str, market: &str) -> Result<TokenInfo> {
        info!("🔐 Authenticating with {}/{}", exchange, market);

        match (exchange, market) {
            ("binance", "spot") => {
                if let Some(auth) = &self.binance_spot {
                    // Validate credentials
                    if !auth.validate_credentials().await? {
                        return Err(anyhow!("Binance Spot credentials invalid"));
                    }

                    // Create listen key (acts as our "token")
                    let listen_key = auth.create_listen_key().await?;

                    let token_info = TokenInfo {
                        token: listen_key,
                        exchange: exchange.to_string(),
                        market: market.to_string(),
                        expires_at: chrono::Utc::now() + chrono::Duration::minutes(60),
                        permissions: vec![
                            "read_market_data".to_string(),
                            "place_orders".to_string(),
                        ],
                        last_validated: chrono::Utc::now(),
                    };

                    self.active_tokens
                        .insert(format!("{}_{}", exchange, market), token_info.clone());

                    info!("✅ Binance Spot authentication successful");
                    Ok(token_info)
                } else {
                    Err(anyhow!("Binance Spot not configured"))
                }
            }
            ("binance", "futures") => {
                if let Some(auth) = &self.binance_futures {
                    if !auth.validate_credentials().await? {
                        return Err(anyhow!("Binance Futures credentials invalid"));
                    }

                    let listen_key = auth.create_listen_key().await?;

                    let token_info = TokenInfo {
                        token: listen_key,
                        exchange: exchange.to_string(),
                        market: market.to_string(),
                        expires_at: chrono::Utc::now() + chrono::Duration::minutes(60),
                        permissions: vec![
                            "read_market_data".to_string(),
                            "place_orders".to_string(),
                            "manage_positions".to_string(),
                        ],
                        last_validated: chrono::Utc::now(),
                    };

                    self.active_tokens
                        .insert(format!("{}_{}", exchange, market), token_info.clone());

                    info!("✅ Binance Futures authentication successful");
                    Ok(token_info)
                } else {
                    Err(anyhow!("Binance Futures not configured"))
                }
            }
            ("zerodha", _) => {
                if self.zerodha_auth.is_some() {
                    // In real implementation, would authenticate with Zerodha
                    let token_info = TokenInfo {
                        token: "zerodha-jwt-token-placeholder".to_string(),
                        exchange: exchange.to_string(),
                        market: market.to_string(),
                        expires_at: chrono::Utc::now() + chrono::Duration::hours(12),
                        permissions: vec![
                            "read_market_data".to_string(),
                            "place_orders".to_string(),
                        ],
                        last_validated: chrono::Utc::now(),
                    };

                    self.active_tokens
                        .insert(exchange.to_string(), token_info.clone());

                    info!("✅ Zerodha authentication successful (simulated)");
                    Ok(token_info)
                } else {
                    Err(anyhow!("Zerodha not configured"))
                }
            }
            _ => Err(anyhow!(
                "Unsupported exchange/market: {}/{}",
                exchange,
                market
            )),
        }
    }

    /// Get valid token for exchange/market
    pub async fn get_valid_token(&mut self, exchange: &str, market: &str) -> Result<String> {
        let key = if market.is_empty() {
            exchange.to_string()
        } else {
            format!("{}_{}", exchange, market)
        };

        // Check if we have a cached token
        if let Some(token_info) = self.active_tokens.get(&key) {
            // Check if token is still valid (not expired)
            if token_info.expires_at > chrono::Utc::now() {
                // Check if we need to refresh soon (within 5 minutes)
                if token_info.expires_at - chrono::Utc::now() < chrono::Duration::minutes(5) {
                    warn!("🔄 Token expires soon, consider refreshing");
                }
                return Ok(token_info.token.clone());
            } else {
                warn!("⏰ Token expired, re-authenticating");
                self.active_tokens.remove(&key);
            }
        }

        // No valid token, authenticate
        let token_info = self.authenticate(exchange, market).await?;
        Ok(token_info.token)
    }

    /// Refresh all tokens proactively
    pub async fn refresh_tokens(&mut self) -> Result<()> {
        info!("🔄 Refreshing all authentication tokens");

        let keys: Vec<String> = self.active_tokens.keys().cloned().collect();

        for key in keys {
            if let Some(token_info) = self.active_tokens.get(&key) {
                // Refresh tokens that expire within 10 minutes
                if token_info.expires_at - chrono::Utc::now() < chrono::Duration::minutes(10) {
                    info!("🔄 Refreshing token for {}", key);

                    let parts: Vec<&str> = key.split('_').collect();
                    let (exchange, market) = if parts.len() == 2 {
                        (parts[0], parts[1])
                    } else {
                        (parts[0], "")
                    };

                    match self.authenticate(exchange, market).await {
                        Ok(_) => info!("✅ Refreshed token for {}", key),
                        Err(e) => error!("❌ Failed to refresh token for {}: {}", key, e),
                    }
                }
            }
        }

        Ok(())
    }

    /// Get authentication summary
    pub fn get_auth_summary(&self) -> FxHashMap<String, String> {
        let mut summary = FxHashMap::default();

        if self.binance_spot.is_some() {
            summary.insert("binance_spot".to_string(), "configured".to_string());
        }

        if self.binance_futures.is_some() {
            summary.insert("binance_futures".to_string(), "configured".to_string());
        }

        if self.zerodha_auth.is_some() {
            summary.insert("zerodha".to_string(), "configured".to_string());
        }

        summary.insert(
            "active_tokens".to_string(),
            self.active_tokens.len().to_string(),
        );

        summary
    }
}

impl MarketDataClient {
    /// Create market data client with authentication
    pub fn new(auth_manager: ProductionAuthManager, endpoint: String) -> Self {
        Self {
            auth_manager,
            endpoint,
        }
    }

    /// Subscribe to market data (demonstrates authenticated service call)
    pub async fn subscribe_to_market_data(&mut self, exchange: &str, symbol: &str) -> Result<()> {
        info!("📊 Subscribing to market data: {}/{}", exchange, symbol);

        // Determine market type from symbol (simplified logic)
        let market = if symbol.contains("PERP") || symbol.contains("USD") {
            "futures"
        } else {
            "spot"
        };

        // Get valid authentication token
        let token = self.auth_manager.get_valid_token(exchange, market).await?;

        info!(
            "🎫 Using authentication token: {}...",
            &token[..10.min(token.len())]
        );

        // In production, this would make the actual gRPC call:
        // let mut request = tonic::Request::new(SubscribeRequest {
        //     symbol: symbol.to_string(),
        //     streams: vec!["depth".to_string(), "trade".to_string()],
        // });
        // request.metadata_mut().insert("authorization", format!("Bearer {}", token).parse()?);
        // let response = self.market_client.subscribe(request).await?;

        info!("📈 Would send request to: {}", self.endpoint);
        info!("   Symbol: {}", symbol);
        info!("   Exchange: {}", exchange);
        info!("   Market: {}", market);
        info!("   Auth: Bearer {}...", &token[..10.min(token.len())]);

        // Simulate streaming response
        for i in 1..=3 {
            sleep(Duration::from_millis(500)).await;
            // SAFETY: Small loop counter to f64 for display
            info!(
                "📊 Market data update {}: {} price={:.2}",
                i,
                symbol,
                100.0 + i as f64
            );
        }

        info!("✅ Market data subscription established");
        Ok(())
    }
}

/// Demonstrate production workflow
async fn production_workflow_demo() -> Result<()> {
    info!("🚀 Production Workflow Demonstration");
    info!("{}", "=".repeat(60));

    // Phase 1: Initialize authentication manager
    info!("\n📋 Phase 1: Authentication Manager Initialization");
    let mut auth_manager = ProductionAuthManager::new().await?;

    let summary = auth_manager.get_auth_summary();
    info!("📊 Authentication Summary:");
    for (service, status) in &summary {
        info!("   {}: {}", service, status);
    }

    // Phase 2: Service authentication
    info!("\n🔐 Phase 2: Service Authentication");

    // Try to authenticate with available exchanges
    let exchanges = vec![("binance", "spot"), ("binance", "futures"), ("zerodha", "")];

    for (exchange, market) in exchanges {
        match auth_manager.authenticate(exchange, market).await {
            Ok(token_info) => {
                info!("✅ {}/{} authenticated successfully", exchange, market);
                info!(
                    "   Token expires: {}",
                    token_info.expires_at.format("%Y-%m-%d %H:%M:%S UTC")
                );
                info!("   Permissions: {:?}", token_info.permissions);
            }
            Err(e) => {
                warn!("⚠️  {}/{} authentication failed: {}", exchange, market, e);
            }
        }
    }

    // Phase 3: Service-to-service communication
    info!("\n📡 Phase 3: Service-to-Service Communication");

    let market_client = MarketDataClient::new(auth_manager, "http://market-data:50052".to_string());

    // Create a local auth manager for the client
    let mut client_auth = ProductionAuthManager::new().await?;
    let mut client = MarketDataClient::new(client_auth, "http://market-data:50052".to_string());

    // Subscribe to market data from different exchanges
    let symbols = vec![("binance", "BTCUSDT"), ("binance", "BTCUSD_PERP")];

    for (exchange, symbol) in symbols {
        match client.subscribe_to_market_data(exchange, symbol).await {
            Ok(_) => info!("✅ Successfully subscribed to {}/{}", exchange, symbol),
            Err(e) => warn!("⚠️  Failed to subscribe to {}/{}: {}", exchange, symbol, e),
        }
    }

    // Phase 4: Token management
    info!("\n🔄 Phase 4: Token Lifecycle Management");
    client.auth_manager.refresh_tokens().await?;

    let final_summary = client.auth_manager.get_auth_summary();
    info!("📊 Final Authentication State:");
    for (service, status) in &final_summary {
        info!("   {}: {}", service, status);
    }

    info!("\n🎉 Production workflow completed successfully!");
    info!("{}", "=".repeat(60));

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize production logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    info!("🏭 ShrivenQuant Production Authentication Demo");

    match production_workflow_demo().await {
        Ok(_) => {
            info!("\n✅ Demo completed successfully!");
            info!("\n🏗️  This demonstrates how ShrivenQuant services:");
            info!("   1. Initialize authentication managers");
            info!("   2. Authenticate with multiple exchanges");
            info!("   3. Manage token lifecycles automatically");
            info!("   4. Make authenticated service calls");
            info!("   5. Handle errors gracefully");
        }
        Err(e) => {
            error!("\n❌ Demo failed: {}", e);
            error!("\n💡 This is expected if exchange credentials are not configured");
            error!("   Add your credentials to .env file to see full functionality");
        }
    }

    Ok(())
}
