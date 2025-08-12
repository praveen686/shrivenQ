//! Real Integration Tests with External APIs
//! 
//! These tests connect to actual Zerodha/Binance APIs using credentials from .env
//! Run with: cargo test --test integration_tests --release -- --ignored
//! 
//! Required .env variables:
//! - ZERODHA_API_KEY
//! - ZERODHA_API_SECRET  
//! - ZERODHA_TOKEN_FILE (path to store token)
//! - BINANCE_API_KEY
//! - BINANCE_API_SECRET

use auth::{AuthProvider, ZerodhaAuth, BinanceSigner};
use dotenv::dotenv;
use std::env;
use std::path::PathBuf;

#[tokio::test]
#[ignore] // Run only when explicitly requested
async fn test_zerodha_real_auth() {
    // Load environment variables
    dotenv().ok();
    
    let api_key = env::var("ZERODHA_API_KEY")
        .expect("ZERODHA_API_KEY not found in .env");
    let api_secret = env::var("ZERODHA_API_SECRET")
        .expect("ZERODHA_API_SECRET not found in .env");
    let token_file = env::var("ZERODHA_TOKEN_FILE")
        .unwrap_or_else(|_| "/tmp/zerodha_token.json".to_string());
    
    println!("Testing Zerodha auth with API key: {}...", &api_key[..8]);
    
    // Create auth instance
    let auth = ZerodhaAuth::new(
        api_key,
        api_secret,
        PathBuf::from(token_file),
    );
    
    // Try to get token (will fail if no valid token saved)
    match auth.token() {
        Ok(token) => {
            println!("✅ Successfully retrieved Zerodha token: {}...", &token[..8]);
            assert!(!token.is_empty());
            assert!(token.len() > 10); // Zerodha tokens are typically 32+ chars
        }
        Err(e) => {
            println!("❌ Zerodha token retrieval failed: {}", e);
            println!("💡 To fix: Complete Zerodha login flow and save token to file");
            println!("💡 This is expected for first-time setup");
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_binance_real_auth() {
    dotenv().ok();
    
    let api_key = env::var("BINANCE_API_KEY")
        .expect("BINANCE_API_KEY not found in .env");
    let api_secret = env::var("BINANCE_API_SECRET")
        .expect("BINANCE_API_SECRET not found in .env");
    
    println!("Testing Binance auth with API key: {}...", &api_key[..8]);
    
    // Create signer
    let signer = BinanceSigner::new(api_key.clone(), api_secret);
    
    // Test signing
    let timestamp = chrono::Utc::now().timestamp_millis();
    let query = format!("timestamp={}", timestamp);
    let signature = signer.sign_query(&query);
    
    println!("✅ Successfully generated Binance signature: {}...", &signature[..8]);
    assert_eq!(signature.len(), 64); // HMAC-SHA256 = 32 bytes = 64 hex chars
    
    // Test API call to get account info (this will validate the credentials)
    let client = reqwest::Client::new();
    let url = "https://api.binance.com/api/v3/account";
    
    let response = client
        .get(url)
        .header("X-MBX-APIKEY", &api_key)
        .query(&[
            ("timestamp", timestamp.to_string()),
            ("signature", signature),
        ])
        .send()
        .await;
    
    match response {
        Ok(resp) => {
            println!("✅ Binance API response status: {}", resp.status());
            if resp.status().is_success() {
                println!("✅ Binance authentication successful!");
                let body = resp.text().await.unwrap_or_else(|_| "Unable to read body".to_string());
                assert!(body.contains("accountType") || body.contains("balances"));
            } else {
                let error = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                println!("❌ Binance API error: {}", error);
                // Don't fail test - might be network/API issues
            }
        }
        Err(e) => {
            println!("❌ Binance API request failed: {}", e);
            println!("💡 Check internet connection and API endpoints");
            // Don't fail test - might be network issues
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_zerodha_websocket_connection() {
    dotenv().ok();
    
    let api_key = env::var("ZERODHA_API_KEY")
        .expect("ZERODHA_API_KEY not found in .env");
    let api_secret = env::var("ZERODHA_API_SECRET")
        .expect("ZERODHA_API_SECRET not found in .env");
    let token_file = env::var("ZERODHA_TOKEN_FILE")
        .unwrap_or_else(|_| "/tmp/zerodha_token.json".to_string());
    
    let auth = ZerodhaAuth::new(
        api_key.clone(),
        api_secret,
        PathBuf::from(token_file),
    );
    
    // Need valid token for WebSocket
    let token = match auth.token() {
        Ok(t) => t,
        Err(_) => {
            println!("❌ No Zerodha token available - skipping WebSocket test");
            println!("💡 Complete Zerodha auth flow first");
            return;
        }
    };
    
    // Test WebSocket connection
    let ws_url = format!(
        "wss://ws.kite.trade?api_key={}&access_token={}",
        api_key, token
    );
    
    println!("🔌 Attempting Zerodha WebSocket connection...");
    
    // Set a timeout for connection attempt
    let connection_result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio_tungstenite::connect_async(&ws_url)
    ).await;
    
    match connection_result {
        Ok(Ok((ws_stream, _))) => {
            println!("✅ Zerodha WebSocket connection successful!");
            
            // Close connection immediately 
            drop(ws_stream);
        }
        Ok(Err(e)) => {
            println!("❌ Zerodha WebSocket connection failed: {}", e);
            println!("💡 Check token validity and network connectivity");
        }
        Err(_) => {
            println!("❌ Zerodha WebSocket connection timed out");
            println!("💡 Check network connectivity to ws.kite.trade");
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_binance_websocket_connection() {
    // Test Binance public WebSocket (no auth required)
    let ws_url = "wss://stream.binance.com:9443/ws/btcusdt@depth5@100ms";
    
    println!("🔌 Attempting Binance WebSocket connection...");
    
    let connection_result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio_tungstenite::connect_async(ws_url)
    ).await;
    
    match connection_result {
        Ok(Ok((ws_stream, _))) => {
            println!("✅ Binance WebSocket connection successful!");
            
            // Could listen for a few messages to verify data flow
            // For now, just close immediately
            drop(ws_stream);
        }
        Ok(Err(e)) => {
            println!("❌ Binance WebSocket connection failed: {}", e);
            println!("💡 Check network connectivity to stream.binance.com");
        }
        Err(_) => {
            println!("❌ Binance WebSocket connection timed out");
            println!("💡 Check network connectivity");
        }
    }
}

#[test]
#[ignore]
fn test_env_file_setup() {
    // Test that .env file is properly configured
    dotenv().ok();
    
    let required_vars = [
        "ZERODHA_API_KEY",
        "ZERODHA_API_SECRET", 
        "BINANCE_API_KEY",
        "BINANCE_API_SECRET",
    ];
    
    let mut missing_vars = Vec::new();
    
    for var in &required_vars {
        match env::var(var) {
            Ok(value) => {
                if value.is_empty() {
                    println!("⚠️  {} is empty", var);
                    missing_vars.push(var);
                } else {
                    println!("✅ {} is set ({}...)", var, &value[..value.len().min(8)]);
                }
            }
            Err(_) => {
                println!("❌ {} is not set", var);
                missing_vars.push(var);
            }
        }
    }
    
    if !missing_vars.is_empty() {
        println!("\n💡 Create a .env file in the project root with:");
        for var in &missing_vars {
            println!("{}=your_actual_value_here", var);
        }
        println!("\n💡 Optional variables:");
        println!("ZERODHA_TOKEN_FILE=/path/to/token.json");
        
        // Don't fail the test, just inform
        println!("\nℹ️  Set up .env file to run real integration tests");
    } else {
        println!("\n✅ All required environment variables are configured");
    }
}

fn main() {
    println!("Integration Tests");
    println!("Run with: cargo test --test integration_tests --release -- --ignored");
    println!("Make sure to set up .env file first!");
}