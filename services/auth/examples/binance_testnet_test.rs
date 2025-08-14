//! Test Binance authentication with testnet credentials

use anyhow::Result;
use auth_service::providers::binance_enhanced::{BinanceAuth, BinanceConfig, BinanceEndpoint};
use tracing::{error, info};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🚀 Testing Binance TESTNET Authentication");
    info!("{}", "=".repeat(50));

    // Load config from .env file (will default to testnet)
    let config = BinanceConfig::from_env_file(BinanceEndpoint::Spot)?;
    info!("✅ Loaded configuration from .env file");
    info!(
        "   Mode: {}",
        if config.testnet { "TESTNET" } else { "MAINNET" }
    );
    info!("   Endpoint: {:?}", config.endpoint);

    // Create auth handler
    let auth = BinanceAuth::new(config);

    // Test connectivity
    info!("\n📡 Testing API connectivity...");
    match auth.ping().await {
        Ok(_) => info!("✅ API connectivity test passed"),
        Err(e) => {
            error!("❌ API connectivity test failed: {}", e);
            return Err(e);
        }
    }

    // Get server time
    info!("\n🕒 Getting server time...");
    match auth.get_server_time().await {
        Ok(time) => {
            let datetime = chrono::DateTime::from_timestamp_millis(time)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "Invalid timestamp".to_string());
            info!("✅ Server time: {} ({})", time, datetime);
        }
        Err(e) => {
            error!("❌ Failed to get server time: {}", e);
        }
    }

    // Validate credentials
    info!("\n🔐 Validating credentials...");
    match auth.validate_credentials().await {
        Ok(valid) => {
            if valid {
                info!("✅ Credentials are valid and trading is enabled");
            } else {
                info!("⚠️  Credentials are valid but trading is disabled");
            }
        }
        Err(e) => {
            error!("❌ Credential validation failed: {}", e);
            error!("   This might be because:");
            error!("   1. API key/secret are incorrect");
            error!("   2. Testnet account needs activation at https://testnet.binance.vision/");
            error!("   3. IP restrictions are configured on the API key");
        }
    }

    // Try to get account info
    info!("\n📊 Getting account information...");
    match auth.get_account_info().await {
        Ok(account) => {
            info!("✅ Account information retrieved:");
            info!("   Can trade: {}", account.can_trade);
            info!("   Can withdraw: {}", account.can_withdraw);
            info!("   Can deposit: {}", account.can_deposit);

            // Show non-zero balances
            let non_zero_balances: Vec<_> = account
                .balances
                .iter()
                .filter_map(|b| {
                    let free = b.free.parse::<f64>().unwrap_or(0.0);
                    let locked = b.locked.parse::<f64>().unwrap_or(0.0);
                    if free > 0.0 || locked > 0.0 {
                        Some((b.asset.clone(), free, locked))
                    } else {
                        None
                    }
                })
                .collect();

            if !non_zero_balances.is_empty() {
                info!("   Balances:");
                for (asset, free, locked) in non_zero_balances {
                    info!("     {} - Free: {:.8}, Locked: {:.8}", asset, free, locked);
                }
            } else {
                info!("   No balances (testnet account may need funding)");
            }
        }
        Err(e) => {
            error!("❌ Failed to get account info: {}", e);
            error!("   Make sure your testnet account is activated");
        }
    }

    // Create listen key for WebSocket
    info!("\n🔑 Creating listen key for WebSocket...");
    match auth.create_listen_key().await {
        Ok(key) => {
            info!("✅ Listen key created: {}...", &key[..10.min(key.len())]);

            // Get WebSocket URLs
            let market_ws = auth.get_market_ws_url(&["btcusdt@depth", "btcusdt@trade"]);
            info!("   Market data WebSocket: {}", market_ws);

            match auth.get_user_ws_url().await {
                Ok(user_ws) => info!("   User data WebSocket: {}", user_ws),
                Err(e) => error!("   Failed to get user WebSocket URL: {}", e),
            }
        }
        Err(e) => {
            error!("❌ Failed to create listen key: {}", e);
        }
    }

    // Test order placement (validation only)
    info!("\n📝 Testing order validation...");
    match auth
        .test_order("BTCUSDT", "BUY", "LIMIT", 0.001, Some(30000.0))
        .await
    {
        Ok(_) => info!("✅ Test order validation passed"),
        Err(e) => {
            error!("❌ Test order validation failed: {}", e);
            error!("   Note: This is normal if the testnet account is not funded");
        }
    }

    info!("");
    info!("🎉 Binance TESTNET authentication test complete!");
    info!("{}", "=".repeat(50));

    Ok(())
}
