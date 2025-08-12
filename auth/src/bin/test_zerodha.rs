//! Test the complete Zerodha authentication implementation

use auth::{ZerodhaAuth, ZerodhaConfig};
use dotenv::dotenv;
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Load environment variables
    dotenv().ok();
    
    println!("🚀 Testing Complete Zerodha Authentication");
    println!("{}", "=".repeat(50));
    
    // Get credentials from environment
    let user_id = env::var("ZERODHA_USER_ID")?;
    let password = env::var("ZERODHA_PASSWORD")?;
    let totp_secret = env::var("ZERODHA_TOTP_SECRET")?;
    let api_key = env::var("ZERODHA_API_KEY")?;
    let api_secret = env::var("ZERODHA_API_SECRET")?;
    
    println!("📋 Configuration:");
    println!("  User ID: {}", user_id);
    println!("  API Key: {}...", &api_key[..8]);
    println!("  Cache Dir: /tmp");
    println!();
    
    // Create configuration
    let config = ZerodhaConfig::new(
        user_id.clone(),
        password,
        totp_secret,
        api_key.clone(),
        api_secret,
    ).with_cache_dir("/tmp".to_string());
    
    // Create auth handler
    let auth = ZerodhaAuth::new(config);
    
    // Test TOTP generation first
    println!("🔐 Testing TOTP Generation:");
    // This is internal, but let's proceed to full auth
    
    // Perform authentication
    println!("🌐 Performing Authentication:");
    println!("  ⏳ Checking cache...");
    println!("  ⏳ Connecting to Zerodha...");
    
    match auth.authenticate().await {
        Ok(access_token) => {
            println!("  ✅ Authentication Successful!");
            println!("  🎫 Access Token: {}...", &access_token[..20.min(access_token.len())]);
            println!();
            
            // Test API call with token
            println!("🧪 Testing API Access:");
            test_api_call(&access_token, &api_key).await?;
            
            // Check cached token
            if let Some(_cached_token) = auth.get_access_token().await {
                println!();
                println!("💾 Token cached for future use");
                println!("  Next run will use cached token if still valid");
            }
        }
        Err(e) => {
            println!("  ❌ Authentication Failed: {}", e);
            println!();
            println!("📝 Troubleshooting:");
            println!("  1. Verify credentials are correct");
            println!("  2. Check TOTP secret matches authenticator app");
            println!("  3. Ensure API key/secret are active");
            println!("  4. Try again if TOTP timing was off");
            return Err(e);
        }
    }
    
    println!();
    println!("✨ Test Complete!");
    Ok(())
}

async fn test_api_call(access_token: &str, api_key: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    
    // Test profile API
    let url = "https://api.kite.trade/user/profile";
    let response = client
        .get(url)
        .header("X-Kite-Version", "3")
        .header("Authorization", format!("token {}:{}", api_key, access_token))
        .send()
        .await?;
    
    if response.status().is_success() {
        println!("  ✅ API Access Verified");
        let profile: serde_json::Value = response.json().await?;
        if let Some(user_name) = profile["data"]["user_name"].as_str() {
            println!("  👤 User: {}", user_name);
        }
        if let Some(email) = profile["data"]["email"].as_str() {
            println!("  📧 Email: {}", email);
        }
        if let Some(broker) = profile["data"]["broker"].as_str() {
            println!("  🏦 Broker: {}", broker);
        }
    } else {
        println!("  ⚠️  API call failed: {}", response.status());
        let error_text = response.text().await?;
        println!("  📝 Response: {}", error_text);
    }
    
    Ok(())
}