//! Enhanced Binance authentication with full trading support
//!
//! Features:
//! - Spot, USD-M Futures, COIN-M Futures support
//! - WebSocket authentication for live data
//! - Order placement and management
//! - Account information and balances
//! - Listen key management for user data streams

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

type HmacSha256 = Hmac<Sha256>;

/// Binance API endpoints
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinanceEndpoint {
    Spot,
    UsdFutures,
    CoinFutures,
}

impl BinanceEndpoint {
    /// Get the base URL for this endpoint
    #[must_use] pub const fn base_url(&self, testnet: bool) -> &str {
        if testnet {
            match self {
                Self::Spot => "https://testnet.binance.vision",
                Self::UsdFutures | Self::CoinFutures => "https://testnet.binancefuture.com",
            }
        } else {
            match self {
                Self::Spot => "https://api.binance.com",
                Self::UsdFutures => "https://fapi.binance.com",
                Self::CoinFutures => "https://dapi.binance.com",
            }
        }
    }

    /// Get WebSocket URL for this endpoint
    #[must_use] pub const fn ws_url(&self, testnet: bool) -> &str {
        if testnet {
            match self {
                Self::Spot => "wss://testnet.binance.vision/ws",
                Self::UsdFutures | Self::CoinFutures => "wss://stream.binancefuture.com/ws",
            }
        } else {
            match self {
                Self::Spot => "wss://stream.binance.com:9443/ws",
                Self::UsdFutures => "wss://fstream.binance.com/ws",
                Self::CoinFutures => "wss://dstream.binance.com/ws",
            }
        }
    }
}

/// Enhanced Binance configuration
#[derive(Debug, Clone)]
pub struct BinanceConfig {
    /// API key for authentication
    pub api_key: String,
    /// API secret for signing requests
    pub api_secret: String,
    /// Endpoint type (Spot/Futures)
    pub endpoint: BinanceEndpoint,
    /// Enable testnet mode
    pub testnet: bool,
    /// Request timeout
    pub timeout: Duration,
    /// Receive window for time sync (milliseconds)
    pub recv_window: u64,
}

impl BinanceConfig {
    /// Create new Binance configuration
    #[must_use] pub const fn new(api_key: String, api_secret: String, endpoint: BinanceEndpoint) -> Self {
        Self {
            api_key,
            api_secret,
            endpoint,
            testnet: false,
            timeout: Duration::from_secs(10),
            recv_window: 5000, // 5 seconds
        }
    }

    /// Load configuration from environment variables
    pub fn from_env_file(endpoint: BinanceEndpoint) -> Result<Self> {
        // Load .env file if it exists
        dotenv::dotenv().ok();

        // Select appropriate env vars based on endpoint
        let (key_var, secret_var) = match endpoint {
            BinanceEndpoint::Spot => ("BINANCE_SPOT_API_KEY", "BINANCE_SPOT_API_SECRET"),
            BinanceEndpoint::UsdFutures => {
                ("BINANCE_FUTURES_API_KEY", "BINANCE_FUTURES_API_SECRET")
            }
            BinanceEndpoint::CoinFutures => (
                "BINANCE_COIN_FUTURES_API_KEY",
                "BINANCE_COIN_FUTURES_API_SECRET",
            ),
        };

        let api_key =
            std::env::var(key_var).map_err(|_| anyhow!("{} not found in .env", key_var))?;
        let api_secret =
            std::env::var(secret_var).map_err(|_| anyhow!("{} not found in .env", secret_var))?;

        // Check if we should use testnet (default to true for now since we have testnet creds)
        let use_testnet = std::env::var("BINANCE_TESTNET")
            .unwrap_or_else(|_| "true".to_string())
            .parse::<bool>()
            .unwrap_or(true);

        Ok(Self::new(api_key, api_secret, endpoint).with_testnet(use_testnet))
    }

    /// Enable testnet mode
    #[must_use] pub const fn with_testnet(mut self, testnet: bool) -> Self {
        self.testnet = testnet;
        self
    }

    /// Set receive window
    #[must_use] pub const fn with_recv_window(mut self, recv_window: u64) -> Self {
        self.recv_window = recv_window;
        self
    }
}

/// Account information from Binance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    #[serde(rename = "makerCommission", default)]
    pub maker_commission: i64,
    #[serde(rename = "takerCommission", default)]
    pub taker_commission: i64,
    #[serde(rename = "canTrade")]
    pub can_trade: bool,
    #[serde(rename = "canWithdraw")]
    pub can_withdraw: bool,
    #[serde(rename = "canDeposit")]
    pub can_deposit: bool,
    #[serde(rename = "updateTime", default)]
    pub update_time: i64,
    pub balances: Vec<Balance>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Balance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub asset: String,
    pub free: String,
    pub locked: String,
}

/// Futures account information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesAccountInfo {
    #[serde(rename = "totalInitialMargin")]
    pub total_initial_margin: String,
    #[serde(rename = "totalMaintMargin")]
    pub total_maint_margin: String,
    #[serde(rename = "totalWalletBalance")]
    pub total_wallet_balance: String,
    #[serde(rename = "totalUnrealizedProfit")]
    pub total_unrealized_profit: String,
    #[serde(rename = "totalMarginBalance")]
    pub total_margin_balance: String,
    #[serde(rename = "totalCrossWalletBalance")]
    pub total_cross_wallet_balance: String,
    #[serde(rename = "totalCrossUnPnl")]
    pub total_cross_unpnl: String,
    #[serde(rename = "availableBalance")]
    pub available_balance: String,
    pub assets: Vec<FuturesAsset>,
    pub positions: Vec<FuturesPosition>,
}

/// Futures asset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesAsset {
    pub asset: String,
    #[serde(rename = "walletBalance")]
    pub wallet_balance: String,
    #[serde(rename = "unrealizedProfit")]
    pub unrealized_profit: String,
    #[serde(rename = "marginBalance")]
    pub margin_balance: String,
    #[serde(rename = "maintMargin")]
    pub maint_margin: String,
    #[serde(rename = "initialMargin")]
    pub initial_margin: String,
}

/// Futures position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesPosition {
    pub symbol: String,
    #[serde(rename = "positionAmt")]
    pub position_amt: String,
    #[serde(rename = "entryPrice")]
    pub entry_price: String,
    #[serde(rename = "markPrice")]
    pub mark_price: String,
    #[serde(rename = "unRealizedProfit")]
    pub unrealized_profit: String,
    #[serde(rename = "liquidationPrice")]
    pub liquidation_price: String,
    #[serde(rename = "positionSide")]
    pub position_side: String,
}

/// Listen key for WebSocket user data stream
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListenKeyResponse {
    #[serde(rename = "listenKey")]
    pub listen_key: String,
}

/// Order response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    pub symbol: String,
    #[serde(rename = "orderId")]
    pub order_id: i64,
    #[serde(rename = "clientOrderId")]
    pub client_order_id: String,
    #[serde(rename = "transactTime")]
    pub transact_time: i64,
    pub price: String,
    #[serde(rename = "origQty")]
    pub orig_qty: String,
    #[serde(rename = "executedQty")]
    pub executed_qty: String,
    pub status: String,
    #[serde(rename = "timeInForce")]
    pub time_in_force: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub side: String,
}

/// Session cache for Binance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinanceSession {
    pub api_key: String,
    pub endpoint: String,
    pub testnet: bool,
    pub created_at: DateTime<Utc>,
    pub listen_key: Option<String>,
    pub listen_key_expires_at: Option<DateTime<Utc>>,
}

/// Enhanced Binance authentication handler
pub struct BinanceAuth {
    config: BinanceConfig,
    http_client: Client,
    listen_key: Arc<RwLock<Option<String>>>,
    listen_key_expires_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    account_info_cache: Arc<RwLock<Option<AccountInfo>>>,
    futures_account_cache: Arc<RwLock<Option<FuturesAccountInfo>>>,
}

impl BinanceAuth {
    /// Create new Binance authentication handler
    #[must_use] pub fn new(config: BinanceConfig) -> Self {
        let http_client = Client::builder()
            .timeout(config.timeout)
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            config,
            http_client,
            listen_key: Arc::new(RwLock::new(None)),
            listen_key_expires_at: Arc::new(RwLock::new(None)),
            account_info_cache: Arc::new(RwLock::new(None)),
            futures_account_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Generate HMAC signature for request
    pub fn sign_request(&self, query_string: &str) -> Result<String> {
        let mut mac = HmacSha256::new_from_slice(self.config.api_secret.as_bytes())
            .map_err(|e| anyhow!("HMAC key error: {}", e))?;
        mac.update(query_string.as_bytes());

        let result = mac.finalize();
        Ok(hex::encode(result.into_bytes()))
    }

    /// Build signed request URL
    fn build_signed_request(
        &self,
        endpoint: &str,
        params: &mut BTreeMap<String, String>,
    ) -> Result<String> {
        // Add timestamp
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| anyhow!("System time error: {}", e))?
            .as_millis()
            .to_string();
        params.insert("timestamp".to_string(), timestamp);

        // Add receive window
        params.insert(
            "recvWindow".to_string(),
            self.config.recv_window.to_string(),
        );

        // Build query string
        let query_string: String = params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");

        // Generate signature
        let signature = self.sign_request(&query_string)?;

        // Build final URL
        Ok(format!(
            "{}{}?{}&signature={}",
            self.config.endpoint.base_url(self.config.testnet),
            endpoint,
            query_string,
            signature
        ))
    }

    /// Test connectivity to Binance API
    pub async fn ping(&self) -> Result<()> {
        let url = format!(
            "{}/api/v3/ping",
            self.config.endpoint.base_url(self.config.testnet)
        );

        let mode = if self.config.testnet {
            "TESTNET"
        } else {
            "MAINNET"
        };
        info!("Connecting to Binance {} at {}", mode, url);

        let response = self.http_client.get(&url).send().await?;

        if response.status().is_success() {
            info!("✅ Binance {} API connectivity test successful", mode);
            Ok(())
        } else {
            Err(anyhow!("Binance API ping failed: {}", response.status()))
        }
    }

    /// Get server time
    pub async fn get_server_time(&self) -> Result<i64> {
        let url = format!(
            "{}/api/v3/time",
            self.config.endpoint.base_url(self.config.testnet)
        );

        let response = self.http_client.get(&url).send().await?;

        if response.status().is_success() {
            let data: Value = response.json().await?;
            let server_time = data["serverTime"]
                .as_i64()
                .ok_or_else(|| anyhow!("Invalid server time response"))?;
            Ok(server_time)
        } else {
            Err(anyhow!("Failed to get server time: {}", response.status()))
        }
    }

    /// Get account information
    pub async fn get_account_info(&self) -> Result<AccountInfo> {
        // Check cache first
        if let Some(cached) = self.account_info_cache.read().await.clone() {
            debug!("Using cached account info");
            return Ok(cached);
        }

        let endpoint = match self.config.endpoint {
            BinanceEndpoint::Spot => "/api/v3/account",
            _ => {
                // For futures, use futures-specific method
                return Err(anyhow!("Use get_futures_account_info() for futures"));
            }
        };

        let mut params = BTreeMap::new();
        let url = self.build_signed_request(endpoint, &mut params)?;

        let response = self
            .http_client
            .get(&url)
            .header("X-MBX-APIKEY", &self.config.api_key)
            .send()
            .await?;

        if response.status().is_success() {
            let account_info: AccountInfo = response.json().await?;

            // Cache the account info
            *self.account_info_cache.write().await = Some(account_info.clone());

            info!("✅ Binance account info retrieved successfully");
            info!(
                "Can trade: {}, Can withdraw: {}, Can deposit: {}",
                account_info.can_trade, account_info.can_withdraw, account_info.can_deposit
            );

            Ok(account_info)
        } else {
            let error_text = response.text().await?;
            Err(anyhow!("Failed to get account info: {}", error_text))
        }
    }

    /// Get futures account information
    pub async fn get_futures_account_info(&self) -> Result<FuturesAccountInfo> {
        // Check cache first
        if let Some(cached) = self.futures_account_cache.read().await.clone() {
            debug!("Using cached futures account info");
            return Ok(cached);
        }

        let endpoint = match self.config.endpoint {
            BinanceEndpoint::UsdFutures => "/fapi/v2/account",
            BinanceEndpoint::CoinFutures => "/dapi/v1/account",
            _ => return Err(anyhow!("Use get_account_info() for spot")),
        };

        let mut params = BTreeMap::new();
        let url = self.build_signed_request(endpoint, &mut params)?;

        let response = self
            .http_client
            .get(&url)
            .header("X-MBX-APIKEY", &self.config.api_key)
            .send()
            .await?;

        if response.status().is_success() {
            let account_info: FuturesAccountInfo = response.json().await?;

            // Cache the account info
            *self.futures_account_cache.write().await = Some(account_info.clone());

            info!("✅ Binance futures account info retrieved");
            info!(
                "Wallet balance: {}, Available: {}, Unrealized PnL: {}",
                account_info.total_wallet_balance,
                account_info.available_balance,
                account_info.total_unrealized_profit
            );

            Ok(account_info)
        } else {
            let error_text = response.text().await?;
            Err(anyhow!(
                "Failed to get futures account info: {}",
                error_text
            ))
        }
    }

    /// Create listen key for user data stream
    pub async fn create_listen_key(&self) -> Result<String> {
        let endpoint = match self.config.endpoint {
            BinanceEndpoint::Spot => "/api/v3/userDataStream",
            BinanceEndpoint::UsdFutures => "/fapi/v1/listenKey",
            BinanceEndpoint::CoinFutures => "/dapi/v1/listenKey",
        };

        let url = format!(
            "{}{}",
            self.config.endpoint.base_url(self.config.testnet),
            endpoint
        );

        let response = self
            .http_client
            .post(&url)
            .header("X-MBX-APIKEY", &self.config.api_key)
            .send()
            .await?;

        if response.status().is_success() {
            let data: ListenKeyResponse = response.json().await?;

            // Store the listen key with expiry (valid for 60 minutes)
            *self.listen_key.write().await = Some(data.listen_key.clone());
            *self.listen_key_expires_at.write().await =
                Some(Utc::now() + chrono::Duration::minutes(60));

            info!(
                "✅ Binance listen key created: {}...",
                &data.listen_key[..10]
            );
            Ok(data.listen_key)
        } else {
            let error_text = response.text().await?;
            Err(anyhow!("Failed to create listen key: {}", error_text))
        }
    }

    /// Keep-alive a listen key (extends by 60 minutes)
    pub async fn keepalive_listen_key(&self) -> Result<()> {
        let listen_key = self
            .listen_key
            .read()
            .await
            .clone()
            .ok_or_else(|| anyhow!("No listen key to keepalive"))?;

        let endpoint = match self.config.endpoint {
            BinanceEndpoint::Spot => "/api/v3/userDataStream",
            BinanceEndpoint::UsdFutures => "/fapi/v1/listenKey",
            BinanceEndpoint::CoinFutures => "/dapi/v1/listenKey",
        };

        let url = format!(
            "{}{}?listenKey={}",
            self.config.endpoint.base_url(self.config.testnet),
            endpoint,
            listen_key
        );

        let response = self
            .http_client
            .put(&url)
            .header("X-MBX-APIKEY", &self.config.api_key)
            .send()
            .await?;

        if response.status().is_success() {
            // Update expiry
            *self.listen_key_expires_at.write().await =
                Some(Utc::now() + chrono::Duration::minutes(60));
            debug!("Listen key keepalive successful");
            Ok(())
        } else {
            Err(anyhow!("Failed to keepalive listen key"))
        }
    }

    /// Close listen key
    pub async fn close_listen_key(&self) -> Result<()> {
        let listen_key = self
            .listen_key
            .read()
            .await
            .clone()
            .ok_or_else(|| anyhow!("No listen key to close"))?;

        let endpoint = match self.config.endpoint {
            BinanceEndpoint::Spot => "/api/v3/userDataStream",
            BinanceEndpoint::UsdFutures => "/fapi/v1/listenKey",
            BinanceEndpoint::CoinFutures => "/dapi/v1/listenKey",
        };

        let url = format!(
            "{}{}?listenKey={}",
            self.config.endpoint.base_url(self.config.testnet),
            endpoint,
            listen_key
        );

        let response = self
            .http_client
            .delete(&url)
            .header("X-MBX-APIKEY", &self.config.api_key)
            .send()
            .await?;

        if response.status().is_success() {
            *self.listen_key.write().await = None;
            *self.listen_key_expires_at.write().await = None;
            debug!("Listen key closed");
            Ok(())
        } else {
            Err(anyhow!("Failed to close listen key"))
        }
    }

    /// Get or create listen key (with auto-renewal)
    pub async fn get_listen_key(&self) -> Result<String> {
        // Check if we have a valid listen key
        let needs_renewal = {
            let expires_at = self.listen_key_expires_at.read().await;
            match *expires_at {
                Some(exp) => exp <= Utc::now() + chrono::Duration::minutes(5), // Renew 5 min before expiry
                None => true,
            }
        };

        if needs_renewal {
            if self.listen_key.read().await.is_some() {
                // Try to keepalive existing key
                match self.keepalive_listen_key().await {
                    Ok(()) => info!("Listen key renewed"),
                    Err(e) => {
                        // Create new key if keepalive fails
                        debug!("Listen key keepalive failed: {}, creating new key", e);
                        self.create_listen_key().await?;
                    }
                }
            } else {
                // Create new key
                self.create_listen_key().await?;
            }
        }

        self.listen_key
            .read()
            .await
            .clone()
            .ok_or_else(|| anyhow!("Failed to get listen key"))
    }

    /// Get WebSocket URL for market data
    #[must_use] pub fn get_market_ws_url(&self, streams: &[&str]) -> String {
        format!(
            "{}/{}",
            self.config.endpoint.ws_url(self.config.testnet),
            streams.join("/")
        )
    }

    /// Get WebSocket URL for user data
    pub async fn get_user_ws_url(&self) -> Result<String> {
        let listen_key = self.get_listen_key().await?;

        Ok(format!(
            "{}/{}",
            self.config.endpoint.ws_url(self.config.testnet),
            listen_key
        ))
    }

    /// Place a test order (validates but doesn't execute)
    pub async fn test_order(
        &self,
        symbol: &str,
        side: &str,
        order_type: &str,
        quantity: f64,
        price: Option<f64>,
    ) -> Result<()> {
        let endpoint = match self.config.endpoint {
            BinanceEndpoint::Spot => "/api/v3/order/test",
            BinanceEndpoint::UsdFutures => "/fapi/v1/order/test",
            BinanceEndpoint::CoinFutures => "/dapi/v1/order/test",
        };

        let mut params = BTreeMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("side".to_string(), side.to_string());
        params.insert("type".to_string(), order_type.to_string());
        params.insert("quantity".to_string(), quantity.to_string());

        if let Some(p) = price {
            params.insert("price".to_string(), p.to_string());
            params.insert("timeInForce".to_string(), "GTC".to_string());
        }

        let url = self.build_signed_request(endpoint, &mut params)?;

        let response = self
            .http_client
            .post(&url)
            .header("X-MBX-APIKEY", &self.config.api_key)
            .send()
            .await?;

        if response.status().is_success() {
            info!("✅ Test order validated successfully");
            Ok(())
        } else {
            let error_text = response.text().await?;
            Err(anyhow!("Test order failed: {}", error_text))
        }
    }

    /// Place a real order
    pub async fn place_order(
        &self,
        symbol: &str,
        side: &str,
        order_type: &str,
        quantity: f64,
        price: Option<f64>,
    ) -> Result<OrderResponse> {
        let endpoint = match self.config.endpoint {
            BinanceEndpoint::Spot => "/api/v3/order",
            BinanceEndpoint::UsdFutures => "/fapi/v1/order",
            BinanceEndpoint::CoinFutures => "/dapi/v1/order",
        };

        let mut params = BTreeMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("side".to_string(), side.to_string());
        params.insert("type".to_string(), order_type.to_string());
        params.insert("quantity".to_string(), quantity.to_string());

        if let Some(p) = price {
            params.insert("price".to_string(), p.to_string());
            params.insert("timeInForce".to_string(), "GTC".to_string());
        }

        let url = self.build_signed_request(endpoint, &mut params)?;

        let response = self
            .http_client
            .post(&url)
            .header("X-MBX-APIKEY", &self.config.api_key)
            .send()
            .await?;

        if response.status().is_success() {
            let order: OrderResponse = response.json().await?;
            info!("✅ Order placed successfully: {}", order.order_id);
            Ok(order)
        } else {
            let error_text = response.text().await?;
            Err(anyhow!("Order placement failed: {}", error_text))
        }
    }

    /// Get API key
    #[must_use] pub fn get_api_key(&self) -> &str {
        &self.config.api_key
    }

    /// Get API secret
    #[must_use] pub fn get_api_secret(&self) -> &str {
        &self.config.api_secret
    }

    /// Validate credentials
    pub async fn validate_credentials(&self) -> Result<bool> {
        match self.config.endpoint {
            BinanceEndpoint::Spot => match self.get_account_info().await {
                Ok(info) => {
                    info!("✅ Binance Spot credentials validated");
                    Ok(info.can_trade)
                }
                Err(e) => {
                    error!("❌ Binance credential validation failed: {}", e);
                    Ok(false)
                }
            },
            _ => match self.get_futures_account_info().await {
                Ok(_) => {
                    info!("✅ Binance Futures credentials validated");
                    Ok(true)
                }
                Err(e) => {
                    error!("❌ Binance credential validation failed: {}", e);
                    Ok(false)
                }
            },
        }
    }
}
