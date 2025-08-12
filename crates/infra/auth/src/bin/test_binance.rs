//! Test Binance authentication implementation

use auth::{BinanceAuth, BinanceConfig, BinanceMarket};
use dotenv::dotenv;
use std::env;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Load environment variables
    dotenv().ok();

    info!("🚀 Testing Binance Authentication");
    info!("{}", "=".repeat(50));

    // Get credentials from environment
    let spot_api_key = env::var("BINANCE_SPOT_API_KEY")?;
    let spot_api_secret = env::var("BINANCE_SPOT_API_SECRET")?;
    let futures_api_key = env::var("BINANCE_FUTURES_API_KEY").ok();
    let futures_api_secret = env::var("BINANCE_FUTURES_API_SECRET").ok();

    info!("📋 Configuration:");
    info!(
        "  Spot API Key: {}...",
        &spot_api_key[..8.min(spot_api_key.len())]
    );
    if let Some(ref key) = futures_api_key {
        info!("  Futures API Key: {}...", &key[..8.min(key.len())]);
    }

    // Create auth handler
    let mut auth = BinanceAuth::new();

    // Add Spot market (TESTNET)
    let _ = auth.add_market(BinanceConfig::new_testnet(
        spot_api_key.clone(),
        spot_api_secret.clone(),
        BinanceMarket::Spot,
    ));

    // Add Futures market if credentials available (TESTNET)
    if let (Some(key), Some(secret)) = (futures_api_key, futures_api_secret) {
        let _ = auth.add_market(BinanceConfig::new_testnet(
            key,
            secret,
            BinanceMarket::UsdFutures,
        ));
    }

    info!("🔧 Using TESTNET endpoints");
    info!("");

    // Test HMAC signing
    info!("🔐 Testing HMAC Signature Generation:");
    let test_query = "symbol=BTCUSDT&timestamp=1234567890000";
    match auth.sign_query(BinanceMarket::Spot, test_query) {
        Ok(signature) => {
            info!("  ✅ Spot market signature: {}...", &signature[..16]);
        }
        Err(e) => {
            info!("  ❌ Failed to sign for Spot: {e}");
        }
    }
    info!("");

    // Test API connectivity
    info!("🌐 Testing API Connectivity:");
    info!("  ⏳ Validating Spot credentials...");

    match auth.validate_credentials(BinanceMarket::Spot).await {
        Ok(true) => {
            info!("  ✅ Spot credentials are valid!");
        }
        Ok(false) => {
            info!("  ❌ Spot credentials are invalid");
            info!("  📝 Possible reasons:");
            info!("     - API key/secret incorrect");
            info!("     - IP not whitelisted");
            info!("     - API key permissions insufficient");
        }
        Err(e) => {
            info!("  ❌ Error validating Spot credentials: {e}");
        }
    }

    if auth.has_market(BinanceMarket::UsdFutures) {
        info!("");
        info!("  ⏳ Validating Futures credentials...");
        match auth.validate_credentials(BinanceMarket::UsdFutures).await {
            Ok(true) => {
                info!("  ✅ Futures credentials are valid!");
            }
            Ok(false) => {
                info!("  ❌ Futures credentials are invalid");
            }
            Err(e) => {
                info!("  ❌ Error validating Futures credentials: {e}");
            }
        }
    }

    info!("");
    info!("📊 Market Configuration:");
    let markets = auth.markets();
    for market in markets {
        info!("  - {market:?} (TESTNET)");
        info!("    API URL: {}", market.api_url(true));
        info!("    WS URL: {}", market.ws_url(true));
    }

    info!("");
    info!("✨ Test Complete!");
    Ok(())
}
