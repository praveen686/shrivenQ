//! Integration tests for Binance authentication
//!
//! Tests cover:
//! - API connectivity
//! - Credential validation
//! - Account information retrieval
//! - Listen key management
//! - WebSocket URL generation
//! - Order placement (test mode)

use anyhow::Result;
use auth_service::providers::binance_enhanced::{BinanceAuth, BinanceConfig, BinanceEndpoint};
use tracing::{Level, error, info};
use tracing_subscriber;

#[tokio::test]
#[ignore] // Run with: cargo test --ignored test_binance_spot -- --nocapture
async fn test_binance_spot_authentication() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    info!("Starting Binance Spot authentication test");

    // Load configuration from .env file
    let config = BinanceConfig::from_env_file(BinanceEndpoint::Spot)?;

    // Create auth handler
    let auth = BinanceAuth::new(config);

    // Test 1: Ping API
    info!("\n=== Test 1: API Connectivity ===");
    auth.ping().await?;
    info!("✅ API connectivity confirmed");

    // Test 2: Get server time
    info!("\n=== Test 2: Server Time ===");
    let server_time = auth.get_server_time().await?;
    info!("Server time: {}", server_time);
    let local_time = chrono::Utc::now().timestamp_millis();
    let time_diff = (server_time - local_time).abs();
    info!("Time difference: {} ms", time_diff);
    assert!(time_diff < 5000, "Time sync issue - difference too large");

    // Test 3: Validate credentials
    info!("\n=== Test 3: Credential Validation ===");
    let valid = auth.validate_credentials().await?;
    assert!(valid, "Credentials should be valid");
    info!("✅ Credentials validated");

    // Test 4: Get account info
    info!("\n=== Test 4: Account Information ===");
    let account = auth.get_account_info().await?;
    info!("Can trade: {}", account.can_trade);
    info!("Can withdraw: {}", account.can_withdraw);
    info!("Can deposit: {}", account.can_deposit);
    info!("Number of assets: {}", account.balances.len());

    // Show non-zero balances
    for balance in &account.balances {
        let free = balance.free.parse::<f64>().unwrap_or(0.0);
        let locked = balance.locked.parse::<f64>().unwrap_or(0.0);
        if free > 0.0 || locked > 0.0 {
            info!("{}: free={}, locked={}", balance.asset, free, locked);
        }
    }

    // Test 5: Listen key management
    info!("\n=== Test 5: Listen Key Management ===");
    let listen_key = auth.create_listen_key().await?;
    info!("Listen key created: {}...", &listen_key[..10]);

    // Keep-alive the listen key
    auth.keepalive_listen_key().await?;
    info!("✅ Listen key kept alive");

    // Close the listen key
    auth.close_listen_key().await?;
    info!("✅ Listen key closed");

    // Test 6: WebSocket URLs
    info!("\n=== Test 6: WebSocket URLs ===");

    // Market data streams
    let streams = vec!["btcusdt@trade", "btcusdt@depth"];
    let market_ws_url = auth.get_market_ws_url(&streams);
    info!("Market WS URL: {}", market_ws_url);

    // User data stream
    let user_ws_url = auth.get_user_ws_url().await?;
    info!("User WS URL: {}...", &user_ws_url[..50]);

    // Test 7: Test order placement
    info!("\n=== Test 7: Test Order ===");
    match auth
        .test_order("BTCUSDT", "BUY", "MARKET", 0.001, None)
        .await
    {
        Ok(_) => info!("✅ Test order validated"),
        Err(e) => info!(
            "Test order failed (expected if insufficient balance): {}",
            e
        ),
    }

    info!("\n🎉 All Binance Spot tests completed!");

    Ok(())
}

#[tokio::test]
#[ignore] // Run with: cargo test --ignored test_binance_futures -- --nocapture
async fn test_binance_futures_authentication() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting Binance USD-M Futures authentication test");

    // Load configuration from .env file
    let config = BinanceConfig::from_env_file(BinanceEndpoint::UsdFutures)?;

    // Create auth handler
    let auth = BinanceAuth::new(config);

    // Test connectivity
    info!("Testing API connectivity...");
    auth.ping().await?;
    info!("✅ Connected to Binance Futures");

    // Validate credentials
    info!("Validating credentials...");
    let valid = auth.validate_credentials().await?;
    assert!(valid, "Futures credentials should be valid");

    // Get futures account info
    info!("Getting futures account information...");
    let account = auth.get_futures_account_info().await?;
    info!("Total wallet balance: {}", account.total_wallet_balance);
    info!("Available balance: {}", account.available_balance);
    info!("Total unrealized PnL: {}", account.total_unrealized_profit);
    info!("Total margin balance: {}", account.total_margin_balance);

    // Show positions if any
    if !account.positions.is_empty() {
        info!("Active positions:");
        for pos in &account.positions {
            let amt = pos.position_amt.parse::<f64>().unwrap_or(0.0);
            if amt != 0.0 {
                info!(
                    "  {}: {} @ {}, PnL: {}",
                    pos.symbol, pos.position_amt, pos.entry_price, pos.unrealized_profit
                );
            }
        }
    }

    // Test order validation
    info!("Testing order validation...");
    match auth
        .test_order("BTCUSDT", "BUY", "MARKET", 0.001, None)
        .await
    {
        Ok(_) => info!("✅ Futures test order validated"),
        Err(e) => info!("Test order failed: {}", e),
    }

    info!("\n🎉 All Binance Futures tests completed!");

    Ok(())
}

#[tokio::test]
#[ignore] // Run with: cargo test --ignored test_binance_testnet -- --nocapture
async fn test_binance_testnet() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    info!("Starting Binance Testnet test");

    // Create testnet configuration
    // Note: You need to get testnet API keys from https://testnet.binance.vision/
    let config = BinanceConfig::new(
        "testnet_api_key".to_string(),
        "testnet_api_secret".to_string(),
        BinanceEndpoint::Spot,
    )
    .with_testnet(true);

    let auth = BinanceAuth::new(config);

    // Test connectivity to testnet
    match auth.ping().await {
        Ok(_) => info!("✅ Connected to Binance testnet"),
        Err(e) => {
            info!(
                "❌ Testnet connection failed (expected if no testnet keys): {}",
                e
            );
            return Ok(());
        }
    }

    // If we have valid testnet credentials, test functionality
    if auth.validate_credentials().await? {
        info!("✅ Testnet credentials valid");

        // Get account info
        let account = auth.get_account_info().await?;
        info!("Testnet account retrieved");
        info!("Can trade: {}", account.can_trade);

        // Test order on testnet (safe to place real orders here)
        match auth
            .place_order("BTCUSDT", "BUY", "LIMIT", 0.001, Some(20000.0))
            .await
        {
            Ok(order) => {
                info!("✅ Testnet order placed: {}", order.order_id);
                info!("Status: {}", order.status);
            }
            Err(e) => info!("Testnet order failed: {}", e),
        }
    }

    info!("\n🎉 Binance testnet test completed!");

    Ok(())
}

#[tokio::test]
#[ignore] // Run with: cargo test --ignored test_binance_listen_key_lifecycle -- --nocapture
async fn test_binance_listen_key_lifecycle() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting Binance listen key lifecycle test");

    let config = BinanceConfig::from_env_file(BinanceEndpoint::Spot)?;
    let auth = BinanceAuth::new(config);

    // Test automatic listen key management
    info!("Test 1: Automatic listen key creation");
    let key1 = auth.get_listen_key().await?;
    info!("First key: {}...", &key1[..10]);

    // Call again - should return same key
    info!("Test 2: Reuse existing key");
    let key2 = auth.get_listen_key().await?;
    assert_eq!(key1, key2, "Should reuse existing key");
    info!("✅ Key reused successfully");

    // Test keepalive
    info!("Test 3: Keepalive");
    auth.keepalive_listen_key().await?;
    info!("✅ Keepalive successful");

    // Test close and recreate
    info!("Test 4: Close and recreate");
    auth.close_listen_key().await?;
    let key3 = auth.get_listen_key().await?;
    assert_ne!(key1, key3, "Should create new key after close");
    info!("✅ New key created: {}...", &key3[..10]);

    // Clean up
    auth.close_listen_key().await?;
    info!("✅ Final cleanup complete");

    info!("\n🎉 Listen key lifecycle test completed!");

    Ok(())
}
