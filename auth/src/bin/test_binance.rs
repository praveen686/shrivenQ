//! Test Binance authentication implementation

use auth::{BinanceAuth, BinanceConfig, BinanceMarket};
use dotenv::dotenv;
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Load environment variables
    dotenv().ok();
    
    println!("🚀 Testing Binance Authentication");
    println!("{}", "=".repeat(50));
    
    // Get credentials from environment
    let spot_api_key = env::var("BINANCE_SPOT_API_KEY")?;
    let spot_api_secret = env::var("BINANCE_SPOT_API_SECRET")?;
    let futures_api_key = env::var("BINANCE_FUTURES_API_KEY").ok();
    let futures_api_secret = env::var("BINANCE_FUTURES_API_SECRET").ok();
    
    println!("📋 Configuration:");
    println!("  Spot API Key: {}...", &spot_api_key[..8.min(spot_api_key.len())]);
    if futures_api_key.is_some() {
        println!("  Futures API Key: {}...", &futures_api_key.as_ref().unwrap()[..8.min(futures_api_key.as_ref().unwrap().len())]);
    }
    println!();
    
    // Create auth handler
    let mut auth = BinanceAuth::new();
    
    // Add Spot market (TESTNET)
    auth.add_market(BinanceConfig::new_testnet(
        spot_api_key.clone(),
        spot_api_secret.clone(),
        BinanceMarket::Spot,
    ));
    
    // Add Futures market if credentials available (TESTNET)
    if let (Some(key), Some(secret)) = (futures_api_key, futures_api_secret) {
        auth.add_market(BinanceConfig::new_testnet(
            key,
            secret,
            BinanceMarket::UsdFutures,
        ));
    }
    
    println!("🔧 Using TESTNET endpoints");
    println!();
    
    // Test HMAC signing
    println!("🔐 Testing HMAC Signature Generation:");
    let test_query = "symbol=BTCUSDT&timestamp=1234567890000";
    match auth.sign_query(BinanceMarket::Spot, test_query) {
        Ok(signature) => {
            println!("  ✅ Spot market signature: {}...", &signature[..16]);
        }
        Err(e) => {
            println!("  ❌ Failed to sign for Spot: {}", e);
        }
    }
    println!();
    
    // Test API connectivity
    println!("🌐 Testing API Connectivity:");
    println!("  ⏳ Validating Spot credentials...");
    
    match auth.validate_credentials(BinanceMarket::Spot).await {
        Ok(true) => {
            println!("  ✅ Spot credentials are valid!");
        }
        Ok(false) => {
            println!("  ❌ Spot credentials are invalid");
            println!("  📝 Possible reasons:");
            println!("     - API key/secret incorrect");
            println!("     - IP not whitelisted");
            println!("     - API key permissions insufficient");
        }
        Err(e) => {
            println!("  ❌ Error validating Spot credentials: {}", e);
        }
    }
    
    if auth.has_market(BinanceMarket::UsdFutures) {
        println!();
        println!("  ⏳ Validating Futures credentials...");
        match auth.validate_credentials(BinanceMarket::UsdFutures).await {
            Ok(true) => {
                println!("  ✅ Futures credentials are valid!");
            }
            Ok(false) => {
                println!("  ❌ Futures credentials are invalid");
            }
            Err(e) => {
                println!("  ❌ Error validating Futures credentials: {}", e);
            }
        }
    }
    
    println!();
    println!("📊 Market Configuration:");
    let markets = auth.markets();
    for market in markets {
        println!("  - {:?} (TESTNET)", market);
        println!("    API URL: {}", market.api_url(true));
        println!("    WS URL: {}", market.ws_url(true));
    }
    
    println!();
    println!("✨ Test Complete!");
    Ok(())
}