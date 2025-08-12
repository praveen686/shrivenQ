//! Binance authentication module with multi-market support

use anyhow::{Result, anyhow};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::HashMap;
use reqwest::Client;
use serde::{Deserialize, Serialize};
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
    pub fn api_url(&self, testnet: bool) -> &'static str {
        if testnet {
            match self {
                BinanceMarket::Spot => "https://testnet.binance.vision",
                BinanceMarket::UsdFutures => "https://testnet.binancefuture.com",
                BinanceMarket::CoinFutures => "https://testnet.binancefuture.com", // Same as USD futures for testnet
            }
        } else {
            match self {
                BinanceMarket::Spot => "https://api.binance.com",
                BinanceMarket::UsdFutures => "https://fapi.binance.com",
                BinanceMarket::CoinFutures => "https://dapi.binance.com",
            }
        }
    }
    
    /// Get the WebSocket URL for this market
    pub fn ws_url(&self, testnet: bool) -> &'static str {
        if testnet {
            match self {
                BinanceMarket::Spot => "wss://testnet.binance.vision/ws",
                BinanceMarket::UsdFutures => "wss://stream.binancefuture.com/ws",
                BinanceMarket::CoinFutures => "wss://stream.binancefuture.com/ws", // Same as USD futures for testnet
            }
        } else {
            match self {
                BinanceMarket::Spot => "wss://stream.binance.com:9443/ws",
                BinanceMarket::UsdFutures => "wss://fstream.binance.com/ws",
                BinanceMarket::CoinFutures => "wss://dstream.binance.com/ws",
            }
        }
    }
    
    /// Get the account endpoint for this market
    pub fn account_endpoint(&self) -> &'static str {
        match self {
            BinanceMarket::Spot => "/api/v3/account",
            BinanceMarket::UsdFutures => "/fapi/v2/account",
            BinanceMarket::CoinFutures => "/dapi/v1/account",
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
    pub fn new(api_key: String, api_secret: String, market: BinanceMarket) -> Self {
        Self {
            api_key,
            api_secret,
            market,
            testnet: false,
        }
    }
    
    /// Create new Binance configuration for testnet
    pub fn new_testnet(api_key: String, api_secret: String, market: BinanceMarket) -> Self {
        Self {
            api_key,
            api_secret,
            market,
            testnet: true,
        }
    }
    
    /// Set testnet flag
    pub fn with_testnet(mut self, testnet: bool) -> Self {
        self.testnet = testnet;
        self
    }
}

/// Binance authentication handler supporting multiple markets
pub struct BinanceAuth {
    /// Configurations for each market
    configs: HashMap<BinanceMarket, BinanceConfig>,
    /// HTTP client
    client: Client,
}

impl BinanceAuth {
    /// Create new Binance auth handler
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
            client: Client::new(),
        }
    }
    
    /// Add configuration for a specific market
    pub fn add_market(&mut self, config: BinanceConfig) -> &mut Self {
        let network = if config.testnet { "testnet" } else { "mainnet" };
        info!("Added Binance {:?} configuration for {}", config.market, network);
        self.configs.insert(config.market, config);
        self
    }
    
    /// Sign a query string for a specific market
    pub fn sign_query(&self, market: BinanceMarket, query: &str) -> Result<String> {
        let config = self.configs.get(&market)
            .ok_or_else(|| anyhow!("No configuration for {:?} market", market))?;
        
        let mut mac = HmacSha256::new_from_slice(config.api_secret.as_bytes())?;
        mac.update(query.as_bytes());
        let signature = mac.finalize().into_bytes();
        Ok(hex::encode(signature))
    }
    
    /// Get API key for a specific market
    pub fn get_api_key(&self, market: BinanceMarket) -> Result<&str> {
        self.configs.get(&market)
            .map(|c| c.api_key.as_str())
            .ok_or_else(|| anyhow!("No configuration for {:?} market", market))
    }
    
    /// Validate credentials for a specific market
    pub async fn validate_credentials(&self, market: BinanceMarket) -> Result<bool> {
        let config = self.configs.get(&market)
            .ok_or_else(|| anyhow!("No configuration for {:?} market", market))?;
        
        let timestamp = chrono::Utc::now().timestamp_millis();
        let query = format!("timestamp={}", timestamp);
        let signature = self.sign_query(market, &query)?;
        
        let url = format!("{}{}", market.api_url(config.testnet), market.account_endpoint());
        
        let response = self.client
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
            let error_body = response.text().await.unwrap_or_else(|_| "No error body".to_string());
            warn!("✗ Binance {:?} credentials are invalid - Status: {}, Error: {}", market, status, error_body);
        }
        
        Ok(is_valid)
    }
    
    /// Get all configured markets
    pub fn markets(&self) -> Vec<BinanceMarket> {
        self.configs.keys().copied().collect()
    }
    
    /// Check if a market is configured
    pub fn has_market(&self, market: BinanceMarket) -> bool {
        self.configs.contains_key(&market)
    }
}

impl Default for BinanceAuth {
    fn default() -> Self {
        Self::new()
    }
}