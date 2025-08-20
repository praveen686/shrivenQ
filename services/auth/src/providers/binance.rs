//! Binance authentication module with multi-market support

use anyhow::{Result, anyhow};
use hmac::{Hmac, Mac};
use reqwest::Client;
use rustc_hash::{FxBuildHasher, FxHashMap};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::{info, warn};

type HmacSha256 = Hmac<Sha256>;

/// Binance market types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BinanceMarket {
    /// Spot trading market
    Spot,
    /// USD-M Futures market
    UsdFutures,
    /// COIN-M Futures market
    CoinFutures,
}

impl BinanceMarket {
    /// Get the base API URL for this market
    #[must_use]
    pub const fn api_url(&self, testnet: bool) -> &'static str {
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

    /// Get the WebSocket URL for this market
    #[must_use]
    pub const fn ws_url(&self, testnet: bool) -> &'static str {
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

    /// Get the account endpoint for this market
    #[must_use]
    pub const fn account_endpoint(&self) -> &'static str {
        match self {
            Self::Spot => "/api/v3/account",
            Self::UsdFutures => "/fapi/v2/account",
            Self::CoinFutures => "/dapi/v1/account",
        }
    }
}

/// Binance configuration for a specific market
#[derive(Debug, Clone)]
pub struct BinanceConfig {
    /// API key
    pub api_key: String,
    /// API secret
    pub api_secret: String,
    /// Market type
    pub market: BinanceMarket,
    /// Use testnet instead of mainnet
    pub testnet: bool,
}

impl BinanceConfig {
    /// Create new Binance configuration for mainnet
    #[must_use]
    pub const fn new(api_key: String, api_secret: String, market: BinanceMarket) -> Self {
        Self {
            api_key,
            api_secret,
            market,
            testnet: false,
        }
    }

    /// Create new Binance configuration for testnet
    #[must_use]
    pub const fn new_testnet(api_key: String, api_secret: String, market: BinanceMarket) -> Self {
        Self {
            api_key,
            api_secret,
            market,
            testnet: true,
        }
    }

    /// Set testnet flag
    #[must_use]
    pub const fn with_testnet(mut self, testnet: bool) -> Self {
        self.testnet = testnet;
        self
    }
}

/// Binance authentication handler supporting multiple markets
#[derive(Debug)]
pub struct BinanceAuth {
    /// Configurations for each market
    configs: FxHashMap<BinanceMarket, BinanceConfig>,
    /// HTTP client
    client: Client,
}

impl BinanceAuth {
    /// Create new Binance auth handler
    #[must_use]
    pub fn new() -> Self {
        Self {
            configs: FxHashMap::with_capacity_and_hasher(3, FxBuildHasher), // Spot, UsdFutures, CoinFutures
            client: Client::new(),
        }
    }

    /// Add configuration for a specific market
    #[must_use]
    pub fn add_market(&mut self, config: BinanceConfig) -> &mut Self {
        let network = if config.testnet { "testnet" } else { "mainnet" };
        info!(
            "Added Binance {:?} configuration for {}",
            config.market, network
        );
        self.configs.insert(config.market, config);
        self
    }

    /// Sign a query string for a specific market
    ///
    /// # Errors
    /// Returns an error if the market is not configured or HMAC signing fails
    pub fn sign_query(&self, market: BinanceMarket, query: &str) -> Result<String> {
        let config = self
            .configs
            .get(&market)
            .ok_or_else(|| anyhow!("No configuration for {:?} market", market))?;

        let mut mac = HmacSha256::new_from_slice(config.api_secret.as_bytes())?;
        mac.update(query.as_bytes());
        let signature = mac.finalize().into_bytes();
        Ok(hex::encode(signature))
    }

    /// Get API key for a specific market
    ///
    /// # Errors
    /// Returns an error if the market is not configured
    pub fn get_api_key(&self, market: BinanceMarket) -> Result<&str> {
        self.configs
            .get(&market)
            .map(|c| c.api_key.as_str())
            .ok_or_else(|| anyhow!("No configuration for {:?} market", market))
    }

    /// Validate credentials for a specific market
    ///
    /// # Errors
    /// Returns an error if the market is not configured or API call fails
    pub async fn validate_credentials(&self, market: BinanceMarket) -> Result<bool> {
        let config = self
            .configs
            .get(&market)
            .ok_or_else(|| anyhow!("No configuration for {:?} market", market))?;

        let timestamp = chrono::Utc::now().timestamp_millis();
        let query = format!("timestamp={timestamp}");
        let signature = self.sign_query(market, &query)?;

        let url = format!(
            "{}{}",
            market.api_url(config.testnet),
            market.account_endpoint()
        );

        let response = self
            .client
            .get(&url)
            .header("X-MBX-APIKEY", &config.api_key)
            .query(&[
                ("timestamp", timestamp.to_string()),
                ("signature", signature),
            ])
            .send()
            .await?;

        let is_valid = response.status().is_success();

        if is_valid {
            info!("✓ Binance {:?} credentials are valid", market);
        } else {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "No error body".to_string());
            warn!(
                "✗ Binance {:?} credentials are invalid - Status: {}, Error: {}",
                market, status, error_body
            );
        }

        Ok(is_valid)
    }

    /// Get all configured markets
    #[must_use]
    pub fn markets(&self) -> Vec<BinanceMarket> {
        self.configs.keys().copied().collect()
    }

    /// Check if a market is configured
    #[must_use]
    pub fn has_market(&self, market: BinanceMarket) -> bool {
        self.configs.contains_key(&market)
    }
}

impl Default for BinanceAuth {
    fn default() -> Self {
        Self::new()
    }
}
