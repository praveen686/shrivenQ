//! Binance authentication and connection management
//!
//! This module handles all Binance-related authentication including:
//! - API key and secret management
//! - Spot, Futures, and Options trading endpoints
//! - Testnet and mainnet support
//! - WebSocket stream management
//! - REST API connectivity

use anyhow::Result;
use auth_service::providers::binance_enhanced::{
    BinanceAuth, BinanceConfig, BinanceEndpoint, AccountInfo, Balance
};
use tokio::time::{Duration, interval};
use tracing::{error, info, warn, Level};

/// Main Binance authentication service
pub struct BinanceService {
    auth: BinanceAuth,
    endpoint: BinanceEndpoint,
    testnet: bool,
}

impl BinanceService {
    /// Create new Binance service instance
    pub async fn new(endpoint: BinanceEndpoint, testnet: bool) -> Result<Self> {
        // Load configuration
        let mut config = BinanceConfig::from_env_file(endpoint.clone())?;
        config.testnet = testnet;
        
        if testnet {
            info!("Initializing Binance TESTNET service");
        } else {
            info!("Initializing Binance MAINNET service");
        }
        
        let auth = BinanceAuth::new(config);
        
        // Test connectivity
        auth.ping().await?;
        info!("Connected to Binance {} API", if testnet { "testnet" } else { "mainnet" });
        
        Ok(Self { auth, endpoint, testnet })
    }
    
    /// Test API connectivity
    pub async fn test_connectivity(&self) -> Result<()> {
        info!("Testing API connectivity...");
        self.auth.ping().await?;
        
        let server_time = self.auth.get_server_time().await?;
        let local_time = chrono::Utc::now().timestamp_millis();
        let diff = (server_time - local_time).abs();
        
        info!("Server time: {}", server_time);
        info!("Local time: {}", local_time);
        info!("Time difference: {} ms", diff);
        
        if diff > 5000 {
            warn!("Time difference exceeds 5 seconds! This may cause authentication issues.");
        }
        
        Ok(())
    }
    
    /// Validate credentials
    pub async fn validate_credentials(&self) -> Result<bool> {
        info!("Validating API credentials...");
        let valid = self.auth.validate_credentials().await?;
        
        if valid {
            info!("✅ Credentials are valid");
        } else {
            error!("❌ Invalid credentials");
        }
        
        Ok(valid)
    }
    
    /// Get account information
    pub async fn get_account_info(&self) -> Result<AccountInfo> {
        info!("Fetching account information...");
        self.auth.get_account_info().await
    }
    
    /// Get balances
    pub async fn get_balances(&self) -> Result<Vec<Balance>> {
        let account = self.get_account_info().await?;
        let non_zero_balances: Vec<Balance> = account.balances
            .into_iter()
            .filter(|b| {
                b.free.parse::<f64>().unwrap_or(0.0) > 0.0 
                || b.locked.parse::<f64>().unwrap_or(0.0) > 0.0
            })
            .collect();
        
        Ok(non_zero_balances)
    }
    
    /// Get WebSocket stream URL
    pub fn get_websocket_url(&self, stream: &str) -> String {
        let base_url = if self.testnet {
            match self.endpoint {
                BinanceEndpoint::Spot => "wss://testnet.binance.vision",
                BinanceEndpoint::UsdFutures => "wss://stream.binancefuture.com",
                BinanceEndpoint::CoinFutures => "wss://stream.binancefuture.com",
            }
        } else {
            match self.endpoint {
                BinanceEndpoint::Spot => "wss://stream.binance.com:9443",
                BinanceEndpoint::UsdFutures => "wss://fstream.binance.com",
                BinanceEndpoint::CoinFutures => "wss://dstream.binance.com",
            }
        };
        
        format!("{}/ws/{}", base_url, stream)
    }
    
    /// Monitor account status
    pub async fn monitor_account(&self, interval_secs: u64) -> Result<()> {
        let mut ticker = interval(Duration::from_secs(interval_secs));
        
        loop {
            ticker.tick().await;
            
            match self.get_account_info().await {
                Ok(account) => {
                    info!("Account Status Update:");
                    info!("  Can Trade: {}", account.can_trade);
                    info!("  Can Withdraw: {}", account.can_withdraw);
                    info!("  Can Deposit: {}", account.can_deposit);
                    
                    let non_zero_balances: Vec<_> = account.balances
                        .iter()
                        .filter(|b| {
                            b.free.parse::<f64>().unwrap_or(0.0) > 0.0 
                            || b.locked.parse::<f64>().unwrap_or(0.0) > 0.0
                        })
                        .collect();
                    
                    if !non_zero_balances.is_empty() {
                        info!("  Balances:");
                        for balance in non_zero_balances {
                            info!("    {}: free={}, locked={}", 
                                balance.asset, balance.free, balance.locked);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to get account info: {}", e);
                }
            }
        }
    }
}

/// Command-line interface
#[derive(Debug, clap::Parser)]
#[clap(name = "binance", about = "Binance authentication and connection management")]
struct Cli {
    /// Use testnet instead of mainnet
    #[clap(long, short = 't')]
    testnet: bool,
    
    /// Trading endpoint
    #[clap(long, short = 'e', default_value = "spot")]
    endpoint: String,
    
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// Test API connectivity
    Ping,
    /// Validate API credentials
    Validate,
    /// Get account information
    Account,
    /// Get account balances
    Balances,
    /// Get WebSocket URL for a stream
    WebSocket {
        #[clap(long)]
        stream: String,
    },
    /// Monitor account status
    Monitor {
        #[clap(long, default_value = "60")]
        interval: u64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    use clap::Parser;
    
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();
    
    info!("🚀 Binance Authentication Service");
    info!("{}", "=".repeat(50));
    
    let cli = Cli::parse();
    
    // Parse endpoint
    let endpoint = match cli.endpoint.to_lowercase().as_str() {
        "spot" => BinanceEndpoint::Spot,
        "usdfutures" | "usd-futures" => BinanceEndpoint::UsdFutures,
        "coinfutures" | "coin-futures" => BinanceEndpoint::CoinFutures,
        _ => {
            error!("Invalid endpoint: {}. Use 'spot', 'usd-futures', or 'coin-futures'", cli.endpoint);
            return Ok(());
        }
    };
    
    let service = BinanceService::new(endpoint, cli.testnet).await?;
    
    match cli.command {
        Command::Ping => {
            service.test_connectivity().await?;
            info!("✅ API connectivity test passed");
        }
        Command::Validate => {
            let valid = service.validate_credentials().await?;
            if !valid {
                std::process::exit(1);
            }
        }
        Command::Account => {
            if !service.validate_credentials().await? {
                error!("Invalid credentials");
                return Ok(());
            }
            
            let account = service.get_account_info().await?;
            info!("Account Information:");
            info!("  Maker Commission: {}%", account.maker_commission as f64 / 100.0);
            info!("  Taker Commission: {}%", account.taker_commission as f64 / 100.0);
            info!("  Can Trade: {}", account.can_trade);
            info!("  Can Withdraw: {}", account.can_withdraw);
            info!("  Can Deposit: {}", account.can_deposit);
            
            if !account.permissions.is_empty() {
                info!("  Permissions: {:?}", account.permissions);
            }
        }
        Command::Balances => {
            if !service.validate_credentials().await? {
                error!("Invalid credentials");
                return Ok(());
            }
            
            let balances = service.get_balances().await?;
            
            if balances.is_empty() {
                info!("No non-zero balances");
            } else {
                info!("Account Balances:");
                for balance in balances {
                    info!("  {}: free={}, locked={}", 
                        balance.asset, balance.free, balance.locked);
                }
            }
        }
        Command::WebSocket { stream } => {
            let url = service.get_websocket_url(&stream);
            info!("WebSocket URL: {}", url);
        }
        Command::Monitor { interval } => {
            if !service.validate_credentials().await? {
                error!("Invalid credentials");
                return Ok(());
            }
            
            info!("Starting account monitor (interval: {} seconds)", interval);
            service.monitor_account(interval).await?;
        }
    }
    
    Ok(())
}