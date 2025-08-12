//! Test the complete Zerodha authentication implementation

use auth::{ZerodhaAuth, ZerodhaConfig};
use dotenv::dotenv;
use std::env;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Load environment variables
    dotenv().ok();

    info!("🚀 Testing Complete Zerodha Authentication");
    info!("{}", "=".repeat(50));

    // Get credentials from environment
    let user_id = env::var("ZERODHA_USER_ID")?;
    let password = env::var("ZERODHA_PASSWORD")?;
    let totp_secret = env::var("ZERODHA_TOTP_SECRET")?;
    let api_key = env::var("ZERODHA_API_KEY")?;
    let api_secret = env::var("ZERODHA_API_SECRET")?;

    info!("📋 Configuration:");
    info!("  User ID: {user_id}");
    info!("  API Key: {}...", &api_key[..8]);
    info!("  Cache Dir: /tmp");
    info!("");

    // Create configuration
    let config = ZerodhaConfig::new(
        user_id.clone(),
        password,
        totp_secret,
        api_key.clone(),
        api_secret,
    )
    .with_cache_dir("/tmp".to_string());

    // Create auth handler
    let auth = ZerodhaAuth::new(config);

    // Test TOTP generation first
    info!("🔐 Testing TOTP Generation:");
    // This is internal, but let's proceed to full auth

    // Perform authentication
    info!("🌐 Performing Authentication:");
    info!("  ⏳ Checking cache...");
    info!("  ⏳ Connecting to Zerodha...");

    match auth.authenticate().await {
        Ok(access_token) => {
            info!("  ✅ Authentication Successful!");
            info!(
                "  🎫 Access Token: {}...",
                &access_token[..20.min(access_token.len())]
            );
            info!("");

            // Test API call with token
            info!("🧪 Testing API Access:");
            test_api_call(&access_token, &api_key).await?;

            // Check cached token
            if let Some(_cached_token) = auth.get_access_token().await {
                info!("");
                info!("💾 Token cached for future use");
                info!("  Next run will use cached token if still valid");
            }
        }
        Err(e) => {
            info!("  ❌ Authentication Failed: {e}");
            info!("");
            info!("📝 Troubleshooting:");
            info!("  1. Verify credentials are correct");
            info!("  2. Check TOTP secret matches authenticator app");
            info!("  3. Ensure API key/secret are active");
            info!("  4. Try again if TOTP timing was off");
            return Err(e);
        }
    }

    info!("");
    info!("✨ Test Complete!");
    Ok(())
}

async fn test_api_call(access_token: &str, api_key: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::new();

    // Test profile API
    let url = "https://api.kite.trade/user/profile";
    let response = client
        .get(url)
        .header("X-Kite-Version", "3")
        .header("Authorization", format!("token {api_key}:{access_token}"))
        .send()
        .await?;

    if response.status().is_success() {
        info!("  ✅ API Access Verified");
        let profile: serde_json::Value = response.json().await?;
        if let Some(user_name) = profile["data"]["user_name"].as_str() {
            info!("  👤 User: {user_name}");
        }
        if let Some(email) = profile["data"]["email"].as_str() {
            info!("  📧 Email: {email}");
        }
        if let Some(broker) = profile["data"]["broker"].as_str() {
            info!("  🏦 Broker: {broker}");
        }
    } else {
        info!("  ⚠️  API call failed: {}", response.status());
        let error_text = response.text().await?;
        info!("  📝 Response: {error_text}");
    }

    Ok(())
}
