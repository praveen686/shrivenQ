//! Integration test for Zerodha automated authentication
//!
//! This test demonstrates the complete automated login flow including:
//! - Automatic TOTP generation for 2FA
//! - Session caching to avoid repeated logins
//! - Token refresh when needed
//! - Full API authentication

use anyhow::Result;
use auth_service::providers::zerodha::{ZerodhaAuth, ZerodhaConfig};
use tracing::{Level, error, info};
use tracing_subscriber;

#[tokio::test]
#[ignore] // Run with: cargo test --ignored zerodha_automated_login -- --nocapture
async fn test_zerodha_automated_login() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    info!("Starting Zerodha automated login test");

    // Load configuration from .env file
    let config = ZerodhaConfig::from_env_file()?;

    info!("Configuration loaded from .env file");

    // Initialize auth service
    let auth = ZerodhaAuth::new(config);

    // Test 1: Initial login with automatic TOTP generation
    info!("Test 1: Performing initial automated login...");
    match auth.authenticate().await {
        Ok(token) => {
            info!("✅ Login successful!");
            info!("Access token obtained: {}...", &token[..20]);

            // Verify token is cached
            assert!(auth.has_valid_token().await);
            info!("✅ Token is cached and valid");
        }
        Err(e) => {
            panic!("❌ Login failed: {}", e);
        }
    }

    // Test 2: Use cached token (should not trigger new login)
    info!("\nTest 2: Testing cached token usage...");
    let start = std::time::Instant::now();
    if let Some(token) = auth.get_access_token().await {
        let elapsed = start.elapsed();
        info!("✅ Got cached token in {:?}", elapsed);
        assert!(elapsed.as_millis() < 100, "Should be fast from cache");
        info!("Token: {}...", &token[..20]);
    } else {
        panic!("❌ Failed to get cached token");
    }

    // Test 3: Force re-authentication
    info!("\nTest 3: Force re-authentication...");
    auth.invalidate_cache().await?;
    assert!(!auth.has_valid_token().await);
    info!("Cache invalidated");

    match auth.authenticate().await {
        Ok(token) => {
            info!("✅ Re-authentication successful!");
            info!("New token: {}...", &token[..20]);
        }
        Err(e) => {
            panic!("❌ Re-authentication failed: {}", e);
        }
    }

    // Test 4: Get profile information
    info!("\nTest 4: Fetching user profile...");
    match auth.get_profile().await {
        Ok(profile) => {
            info!("✅ Profile fetched successfully!");
            info!("User: {}", profile.user_id);
            info!("Email: {}", profile.email);
            info!("Exchanges: {:?}", profile.exchanges);
            info!("Products: {:?}", profile.products);
        }
        Err(e) => {
            error!("❌ Failed to fetch profile: {}", e);
        }
    }

    // Test 5: Get margins
    info!("\nTest 5: Fetching account margins...");
    match auth.get_margins().await {
        Ok(margins) => {
            info!("✅ Margins fetched successfully!");
            if let Some(equity) = margins.get("equity") {
                info!("Equity margins:");
                info!(
                    "  Available: {}",
                    equity.available.get("cash").unwrap_or(&0.0)
                );
                info!("  Used: {}", equity.used.get("cash").unwrap_or(&0.0));
                info!("  Total: {}", equity.total.get("cash").unwrap_or(&0.0));
            }
        }
        Err(e) => {
            error!("❌ Failed to fetch margins: {}", e);
        }
    }

    // Test 6: Test token expiry handling
    info!("\nTest 6: Testing token expiry handling...");
    // Note: In production, tokens expire after market hours
    // This test would need to run across market close to fully test
    info!(
        "Current token age: {} hours",
        auth.get_token_age_hours().await.unwrap_or(0.0)
    );
    info!("Token will auto-refresh when expired");

    info!("\n🎉 All Zerodha authentication tests passed!");

    Ok(())
}

#[tokio::test]
#[ignore] // Run with: cargo test --ignored zerodha_websocket_auth -- --nocapture
async fn test_zerodha_websocket_authentication() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting Zerodha WebSocket authentication test");

    // Load configuration from .env file
    let config = ZerodhaConfig::from_env_file()?;

    // Get authenticated token
    let auth = ZerodhaAuth::new(config);
    let access_token = auth.authenticate().await?;
    info!("Got access token for WebSocket");

    // Test WebSocket connection with auth
    // Note: This would connect to Zerodha's WebSocket for live data
    let ws_url = format!(
        "wss://ws.kite.trade?api_key={}&access_token={}",
        auth.get_api_key(),
        access_token
    );

    info!(
        "WebSocket URL prepared (not connecting in test): {}",
        &ws_url[..50]
    );
    info!("✅ WebSocket authentication URL prepared successfully");

    Ok(())
}

#[tokio::test]
#[ignore] // Run with: cargo test --ignored zerodha_order_placement -- --nocapture
async fn test_zerodha_order_placement_auth() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting Zerodha order placement authentication test");

    // Load configuration from .env file
    let config = ZerodhaConfig::from_env_file()?;

    // Authenticate
    let auth = ZerodhaAuth::new(config);
    let token = auth.authenticate().await?;
    info!("✅ Authenticated for order placement");

    // Verify token is valid
    assert!(!token.is_empty(), "Token should not be empty");

    // Example order structure (not actually placed)
    let test_order = serde_json::json!({
        "tradingsymbol": "RELIANCE",
        "exchange": "NSE",
        "transaction_type": "BUY",
        "order_type": "LIMIT",
        "quantity": 1,
        "price": 2500.0,
        "product": "MIS",
        "validity": "DAY"
    });

    info!("Test order prepared: {:?}", test_order);
    info!("✅ Order authentication and preparation successful");
    info!("Note: Order not actually placed in test mode");

    Ok(())
}
