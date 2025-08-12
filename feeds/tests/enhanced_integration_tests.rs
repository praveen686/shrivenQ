//! Enhanced Integration Tests with Full Authentication
//! 
//! Tests complete auth flow including TOTP, separate Binance markets
//! Run with: cargo test --test enhanced_integration_tests --release -- --ignored

use auth::{EnhancedZerodhaAuth, EnhancedBinanceAuth, BinanceMarket, AuthProvider};
use dotenv::dotenv;
use std::env;
use std::path::PathBuf;

#[tokio::test]
#[ignore]
async fn test_enhanced_zerodha_auth() {
    dotenv().ok();
    
    let user_id = env::var("ZERODHA_USER_ID")
        .expect("ZERODHA_USER_ID not found in .env");
    let password = env::var("ZERODHA_PASSWORD")
        .expect("ZERODHA_PASSWORD not found in .env");
    let totp_secret = env::var("ZERODHA_TOTP_SECRET")
        .expect("ZERODHA_TOTP_SECRET not found in .env");
    let api_key = env::var("ZERODHA_API_KEY")
        .expect("ZERODHA_API_KEY not found in .env");
    let api_secret = env::var("ZERODHA_API_SECRET")
        .expect("ZERODHA_API_SECRET not found in .env");
    let session_file = env::var("ZERODHA_SESSION_FILE")
        .unwrap_or_else(|_| "/tmp/zerodha_session.json".to_string());
    
    println!("Testing enhanced Zerodha auth for user: {}", user_id);
    
    let auth = EnhancedZerodhaAuth::new(
        user_id,
        password,
        totp_secret,
        api_key,
        api_secret,
        PathBuf::from(session_file),
    );
    
    // Test TOTP generation
    match auth.generate_totp() {
        Ok(totp) => {
            println!("✅ Generated TOTP: {}", totp);
            assert_eq!(totp.len(), 6);
            assert!(totp.chars().all(|c| c.is_ascii_digit()));
        }
        Err(e) => {
            println!("❌ TOTP generation failed: {}", e);
            println!("💡 Check TOTP_SECRET format (should be base32)");
        }
    }
    
    // Test token retrieval (from existing session)
    match auth.token() {
        Ok(token) => {
            println!("✅ Found existing session token: {}...", &token[..8]);
        }
        Err(_) => {
            println!("ℹ️  No existing session found");
            println!("💡 Run full login flow to create session");
            
            // Note: Full login flow requires browser interaction
            // In production, this would be handled by CLI tool
            println!("⚠️  Full login flow requires manual browser interaction");
        }
    }
}

#[tokio::test] 
#[ignore]
async fn test_enhanced_binance_auth() {
    dotenv().ok();
    
    let mut auth = EnhancedBinanceAuth::new();
    
    // Add Spot credentials
    if let (Ok(spot_key), Ok(spot_secret)) = (
        env::var("BINANCE_SPOT_API_KEY"),
        env::var("BINANCE_SPOT_API_SECRET")
    ) {
        auth.add_market(BinanceMarket::Spot, spot_key, spot_secret);
        println!("✅ Added Binance Spot credentials");
    }
    
    // Add USD-M Futures credentials
    if let (Ok(futures_key), Ok(futures_secret)) = (
        env::var("BINANCE_FUTURES_API_KEY"),
        env::var("BINANCE_FUTURES_API_SECRET")
    ) {
        auth.add_market(BinanceMarket::UsdFutures, futures_key, futures_secret);
        println!("✅ Added Binance USD-M Futures credentials");
    }
    
    // Add COIN-M Futures credentials
    if let (Ok(coin_key), Ok(coin_secret)) = (
        env::var("BINANCE_COIN_FUTURES_API_KEY"),
        env::var("BINANCE_COIN_FUTURES_API_SECRET")
    ) {
        auth.add_market(BinanceMarket::CoinFutures, coin_key, coin_secret);
        println!("✅ Added Binance COIN-M Futures credentials");
    }
    
    let markets = auth.markets();
    println!("🏢 Configured {} Binance markets: {:?}", markets.len(), markets);
    
    // Test each market
    for market in &markets {
        println!("\n🧪 Testing {} market...", format!("{:?}", market));
        println!("📡 Base URL: {}", market.base_url());
        println!("🔌 WebSocket URL: {}", market.ws_url());
        
        // Test API key retrieval
        match auth.api_key(*market) {
            Ok(key) => println!("✅ API Key: {}...", &key[..8]),
            Err(e) => println!("❌ API Key error: {}", e),
        }
        
        // Test signing
        let test_query = "symbol=BTCUSDT&timestamp=1234567890";
        match auth.sign_query(*market, test_query) {
            Ok(signature) => {
                println!("✅ Signature: {}...", &signature[..16]);
                assert_eq!(signature.len(), 64);
            }
            Err(e) => println!("❌ Signing error: {}", e),
        }
        
        // Test actual credentials (API call)
        println!("🌐 Testing live API credentials...");
        match auth.test_credentials(*market).await {
            Ok(true) => println!("✅ Credentials valid for {:?}", market),
            Ok(false) => println!("❌ Credentials invalid for {:?}", market),
            Err(e) => println!("❌ Credential test failed for {:?}: {}", market, e),
        }
    }
    
    if markets.is_empty() {
        println!("⚠️  No Binance credentials configured");
        println!("💡 Add BINANCE_SPOT_API_KEY, BINANCE_FUTURES_API_KEY to .env");
    }
}

#[test]
#[ignore]
fn test_enhanced_env_setup() {
    dotenv().ok();
    
    let zerodha_vars = [
        "ZERODHA_USER_ID",
        "ZERODHA_PASSWORD", 
        "ZERODHA_TOTP_SECRET",
        "ZERODHA_API_KEY",
        "ZERODHA_API_SECRET",
    ];
    
    let binance_spot_vars = [
        "BINANCE_SPOT_API_KEY",
        "BINANCE_SPOT_API_SECRET",
    ];
    
    let binance_futures_vars = [
        "BINANCE_FUTURES_API_KEY", 
        "BINANCE_FUTURES_API_SECRET",
    ];
    
    println!("🔍 Checking enhanced environment setup...\n");
    
    // Check Zerodha
    println!("📊 Zerodha KiteConnect:");
    let mut zerodha_complete = true;
    for var in &zerodha_vars {
        match env::var(var) {
            Ok(value) if !value.is_empty() => {
                println!("  ✅ {} configured", var);
            }
            _ => {
                println!("  ❌ {} missing", var);
                zerodha_complete = false;
            }
        }
    }
    
    // Check Binance Spot
    println!("\n💰 Binance Spot:");
    let mut spot_complete = true;
    for var in &binance_spot_vars {
        match env::var(var) {
            Ok(value) if !value.is_empty() => {
                println!("  ✅ {} configured", var);
            }
            _ => {
                println!("  ❌ {} missing", var);
                spot_complete = false;
            }
        }
    }
    
    // Check Binance Futures
    println!("\n🚀 Binance USD-M Futures:");
    let mut futures_complete = true;
    for var in &binance_futures_vars {
        match env::var(var) {
            Ok(value) if !value.is_empty() => {
                println!("  ✅ {} configured", var);
            }
            _ => {
                println!("  ❌ {} missing", var);
                futures_complete = false;
            }
        }
    }
    
    println!("\n📋 Summary:");
    println!("  Zerodha: {}", if zerodha_complete { "✅ Complete" } else { "❌ Incomplete" });
    println!("  Binance Spot: {}", if spot_complete { "✅ Complete" } else { "❌ Incomplete" });
    println!("  Binance Futures: {}", if futures_complete { "✅ Complete" } else { "❌ Incomplete" });
    
    if !zerodha_complete || !spot_complete {
        println!("\n💡 Setup Instructions:");
        println!("1. Copy .env.example to .env");
        println!("2. Fill in your credentials:");
        
        if !zerodha_complete {
            println!("   • Zerodha: Get from https://kite.trade/");
            println!("   • TOTP Secret: From authenticator app setup");
        }
        
        if !spot_complete {
            println!("   • Binance: Get from https://binance.com/en/my/settings/api-management");
            println!("   • Use separate keys for Spot vs Futures for security");
        }
        
        println!("3. Run: cargo test --test enhanced_integration_tests -- --ignored");
    }
}

#[test]
fn test_binance_market_urls() {
    // Test market URL configurations
    assert_eq!(BinanceMarket::Spot.base_url(), "https://api.binance.com");
    assert_eq!(BinanceMarket::UsdFutures.base_url(), "https://fapi.binance.com");
    assert_eq!(BinanceMarket::CoinFutures.base_url(), "https://dapi.binance.com");
    
    assert_eq!(BinanceMarket::Spot.ws_url(), "wss://stream.binance.com:9443/ws");
    assert_eq!(BinanceMarket::UsdFutures.ws_url(), "wss://fstream.binance.com/ws");
    assert_eq!(BinanceMarket::CoinFutures.ws_url(), "wss://dstream.binance.com/ws");
}

fn main() {
    println!("Enhanced Integration Tests");
    println!("Features:");
    println!("- Full Zerodha auth with TOTP");
    println!("- Separate Binance Spot/Futures credentials");
    println!("- Live API credential validation");
    println!("");
    println!("Setup .env file then run:");
    println!("cargo test --test enhanced_integration_tests --release -- --ignored");
}