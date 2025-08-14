//! Simple example of using Binance authentication
//!
//! This example shows how to:
//! 1. Load credentials from .env
//! 2. Connect to Binance API
//! 3. Get account information
//! 4. Stream market data

use anyhow::Result;
use auth_service::providers::binance_enhanced::{BinanceAuth, BinanceConfig, BinanceEndpoint};
use tracing::{Level, info};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    println!("===========================================");
    println!("Binance Authentication Example");
    println!("===========================================\n");

    // Load configuration from .env file
    let config = BinanceConfig::from_env_file(BinanceEndpoint::Spot)?;

    // Create auth handler
    let auth = BinanceAuth::new(config);

    // Test connectivity
    println!("📡 Testing API connectivity...");
    auth.ping().await?;
    println!("✅ Connected to Binance API\n");

    // Get server time
    let server_time = auth.get_server_time().await?;
    let local_time = chrono::Utc::now().timestamp_millis();
    println!("⏰ Server time: {}", server_time);
    println!("⏰ Local time:  {}", local_time);
    println!("⏰ Difference:  {} ms\n", (server_time - local_time).abs());

    // Validate credentials
    println!("🔐 Validating credentials...");
    if auth.validate_credentials().await? {
        println!("✅ Credentials are valid\n");
    } else {
        println!("❌ Invalid credentials");
        return Ok(());
    }

    // Get account information
    println!("📊 Account Information:");
    let account = auth.get_account_info().await?;
    println!("Can trade:    {}", account.can_trade);
    println!("Can withdraw: {}", account.can_withdraw);
    println!("Can deposit:  {}", account.can_deposit);
    println!("Permissions:  {:?}\n", account.permissions);

    // Show balances
    println!("💰 Non-zero Balances:");
    let mut has_balance = false;
    for balance in &account.balances {
        let free = balance.free.parse::<f64>().unwrap_or(0.0);
        let locked = balance.locked.parse::<f64>().unwrap_or(0.0);
        if free > 0.0 || locked > 0.0 {
            println!(
                "  {}: free={:.8}, locked={:.8}",
                balance.asset, free, locked
            );
            has_balance = true;
        }
    }
    if !has_balance {
        println!("  No non-zero balances");
    }
    println!();

    // Create listen key for user data stream
    println!("🔑 Creating listen key for user data stream...");
    let listen_key = auth.create_listen_key().await?;
    println!("✅ Listen key created: {}...\n", &listen_key[..20]);

    // Generate WebSocket URLs
    println!("🌐 WebSocket URLs:");

    // Market data streams
    let streams = vec!["btcusdt@trade", "ethusdt@trade", "bnbusdt@trade"];
    let market_ws = auth.get_market_ws_url(&streams);
    println!("Market data: {}", &market_ws[..60]);
    println!("Streams: {:?}", streams);

    // User data stream
    let user_ws = auth.get_user_ws_url().await?;
    println!("User data: {}...\n", &user_ws[..60]);

    // Test order validation
    println!("📝 Testing order validation...");
    match auth
        .test_order("BTCUSDT", "BUY", "MARKET", 0.001, None)
        .await
    {
        Ok(_) => println!("✅ Test order validated successfully"),
        Err(e) => println!("⚠️  Test order failed: {}", e),
    }

    // Clean up
    println!("\n🧹 Cleaning up...");
    auth.close_listen_key().await?;
    println!("✅ Listen key closed");

    println!("\n===========================================");
    println!("Example completed successfully!");
    println!("===========================================");

    Ok(())
}
