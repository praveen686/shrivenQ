//! Authentication types for market connectors

use serde::{Deserialize, Serialize};

/// Binance market types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinanceMarket {
    /// Spot trading
    Spot,
    /// USD-M Futures
    UsdFutures,
    /// COIN-M Futures
    CoinFutures,
}

impl BinanceMarket {
    /// Get WebSocket URL
    #[must_use] pub fn ws_url(&self, testnet: bool) -> String {
        if testnet {
            match self {
                Self::Spot => "wss://testnet.binance.vision/ws".to_string(),
                Self::UsdFutures => "wss://stream.binancefuture.com/ws".to_string(),
                Self::CoinFutures => "wss://dstream.binancefuture.com/ws".to_string(),
            }
        } else {
            match self {
                Self::Spot => "wss://stream.binance.com:443/ws".to_string(),
                Self::UsdFutures => "wss://fstream.binance.com/ws".to_string(),
                Self::CoinFutures => "wss://dstream.binance.com/ws".to_string(),
            }
        }
    }

    /// Get API URL
    #[must_use] pub fn api_url(&self, testnet: bool) -> String {
        if testnet {
            match self {
                Self::Spot => "https://testnet.binance.vision".to_string(),
                Self::UsdFutures => "https://testnet.binancefuture.com".to_string(),
                Self::CoinFutures => "https://testnet.binancefuture.com".to_string(),
            }
        } else {
            match self {
                Self::Spot => "https://api.binance.com".to_string(),
                Self::UsdFutures => "https://fapi.binance.com".to_string(),
                Self::CoinFutures => "https://dapi.binance.com".to_string(),
            }
        }
    }
}

/// Binance authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinanceAuth {
    /// API key
    pub api_key: String,
    /// Secret key
    pub secret_key: String,
    /// Market type
    pub market: BinanceMarket,
    /// Use testnet
    pub testnet: bool,
}

impl BinanceAuth {
    /// Create new Binance auth
    #[must_use] pub const fn new(api_key: String, secret_key: String, market: BinanceMarket, testnet: bool) -> Self {
        Self {
            api_key,
            secret_key,
            market,
            testnet,
        }
    }

    /// Check if this auth has access to the given market
    #[must_use] pub fn has_market(&self, market: BinanceMarket) -> bool {
        self.market == market
    }

    /// Validate credentials for the given market
    pub async fn validate_credentials(&self, market: BinanceMarket) -> anyhow::Result<bool> {
        // Basic validation first
        if self.api_key.is_empty() || self.secret_key.is_empty() {
            return Ok(false);
        }

        // Create HTTP client for validation
        let client = reqwest::Client::new();
        
        // Get current timestamp for signature
        let timestamp = chrono::Utc::now().timestamp_millis();
        let query = format!("timestamp={}", timestamp);
        
        // Sign the query
        let signature = self.sign_query(market, &query)?;
        
        // Get the appropriate API URL and endpoint
        let base_url = market.api_url(self.testnet);
        let endpoint = match market {
            BinanceMarket::Spot => "/api/v3/account",
            BinanceMarket::UsdFutures => "/fapi/v2/account", 
            BinanceMarket::CoinFutures => "/dapi/v1/account",
        };
        
        let url = format!("{}{}", base_url, endpoint);
        
        // Make test API call
        let response = client
            .get(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .query(&[
                ("timestamp", timestamp.to_string()),
                ("signature", signature),
            ])
            .send()
            .await?;
            
        Ok(response.status().is_success())
    }

    /// Sign query parameters for the given market using HMAC-SHA256
    pub fn sign_query(&self, _market: BinanceMarket, params: &str) -> anyhow::Result<String> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        
        type HmacSha256 = Hmac<Sha256>;
        
        let mut mac = HmacSha256::new_from_slice(self.secret_key.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to create HMAC: {}", e))?;
            
        mac.update(params.as_bytes());
        let signature = mac.finalize().into_bytes();
        
        Ok(hex::encode(signature))
    }

    /// Get API key for the given market
    pub fn get_api_key(&self, _market: BinanceMarket) -> anyhow::Result<&str> {
        Ok(&self.api_key)
    }
}

/// Zerodha authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerodhaAuth {
    /// API key
    pub api_key: String,
    /// Access token
    pub access_token: String,
    /// User ID
    pub user_id: String,
}

impl ZerodhaAuth {
    /// Create new Zerodha auth
    #[must_use] pub const fn new(api_key: String, access_token: String, user_id: String) -> Self {
        Self {
            api_key,
            access_token,
            user_id,
        }
    }

    /// Create Zerodha auth from config (compatibility constructor)
    #[must_use] pub fn from_config(config: ZerodhaConfig) -> Self {
        Self {
            api_key: config.api_key,
            access_token: String::new(), // Will be obtained through authentication
            user_id: config.user_id,
        }
    }

    /// Authenticate and get access token
    pub async fn authenticate(&self) -> anyhow::Result<String> {
        // If we already have an access token, validate it first
        if !self.access_token.is_empty() {
            // Test the token by making a profile API call
            let client = reqwest::Client::new();
            let response = client
                .get("https://api.kite.trade/user/profile")
                .header("X-Kite-Version", "3")
                .header("Authorization", format!("token {}:{}", self.api_key, self.access_token))
                .send()
                .await?;
                
            if response.status().is_success() {
                return Ok(self.access_token.clone());
            }
        }
        
        // If no valid token, return an error - full authentication requires
        // the complete ZerodhaAuth implementation from the auth service
        Err(anyhow::anyhow!(
            "No valid access token. Use the full ZerodhaAuth service for complete authentication."
        ))
    }

    /// Get API key
    #[must_use] pub fn get_api_key(&self) -> &str {
        &self.api_key
    }
}

/// Zerodha configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerodhaConfig {
    /// User ID
    pub user_id: String,
    /// Password
    pub password: String,
    /// TOTP secret
    pub totp_secret: String,
    /// API key
    pub api_key: String,
    /// API secret
    pub api_secret: String,
}

impl ZerodhaConfig {
    /// Create new Zerodha config
    #[must_use] pub const fn new(user_id: String, password: String, totp_secret: String, api_key: String, api_secret: String) -> Self {
        Self {
            user_id,
            password,
            totp_secret,
            api_key,
            api_secret,
        }
    }
}