//! Demo of Zerodha automated login with TOTP
//!
//! This example shows how the authentication system:
//! 1. Automatically generates TOTP codes for 2FA
//! 2. Caches sessions to avoid repeated logins
//! 3. Handles token expiry and refresh
//! 4. Provides seamless API access

use anyhow::Result;
use auth_service::providers::zerodha::{ZerodhaAuth, ZerodhaConfig};
use tokio::time::{Duration, sleep};
use tracing::{Level, info};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("===========================================");
    info!("Zerodha Automated Authentication Demo");
    info!("===========================================");
    info!("");

    // Load configuration from .env file or use demo values
    let config = match ZerodhaConfig::from_env_file() {
        Ok(config) => {
            info!("✅ Configuration loaded from .env file");
            config
        }
        Err(e) => {
            info!("⚠️  Could not load .env file: {}", e);
            info!("⚠️  Using demo configuration (won't actually connect)");
            demo_config()
        }
    };

    // Initialize the auth service
    info!("Initializing Zerodha auth service...");
    let auth = ZerodhaAuth::new(config);

    // Demonstrate the authentication flow
    info!("");
    info!("📱 Step 1: Automatic TOTP Generation");
    info!("The system will automatically generate TOTP codes");
    info!("No manual intervention needed!");

    sleep(Duration::from_secs(1)).await;

    info!("");
    info!("🔐 Step 2: Automated Login");
    info!("Attempting automated login with TOTP...");

    match auth.authenticate().await {
        Ok(token) => {
            info!("✅ Login successful!");
            info!(
                "Access token obtained: {}...",
                &token[..20.min(token.len())]
            );

            info!("");
            info!("💾 Step 3: Session Caching");
            info!("Token is now cached for future use");

            // Demonstrate cached access
            let start = std::time::Instant::now();
            let cached_token = auth.get_access_token().await.unwrap_or_default();
            let elapsed = start.elapsed();

            info!("✅ Retrieved cached token in {:?}", elapsed);
            assert_eq!(token, cached_token, "Cached token should match");

            info!("");
            info!("📊 Step 4: API Access Ready");
            info!("You can now:");
            info!("  - Stream market data via WebSocket");
            info!("  - Place and manage orders");
            info!("  - Access account information");
            info!("  - Get positions and holdings");

            // Show token info
            if let Ok(age) = auth.get_token_age_hours().await {
                info!("");
                info!("Token age: {:.1} hours", age);
                info!("Token will auto-refresh when expired");
            }

            info!("");
            info!("🎯 Step 5: Profile Access");
            if let Ok(profile) = auth.get_profile().await {
                info!("User ID: {}", profile.user_id);
                info!("Email: {}", profile.email);
                info!("Available exchanges: {:?}", profile.exchanges);
            }

            info!("");
            info!("💰 Step 6: Account Margins");
            if let Ok(margins) = auth.get_margins().await {
                if let Some(equity) = margins.get("equity") {
                    if let Some(cash) = equity.available.get("cash") {
                        info!("Available margin: ₹{:.2}", cash);
                    }
                }
            }
        }
        Err(e) => {
            info!("❌ Login failed: {}", e);
            info!("This is expected in demo mode without real credentials");
        }
    }

    info!("");
    info!("===========================================");
    info!("Demo completed!");
    info!("");
    info!("To use with real credentials:");
    info!("1. Set up your Zerodha API app at https://kite.trade");
    info!("2. Configure environment variables:");
    info!("   - ZERODHA_USER_ID");
    info!("   - ZERODHA_PASSWORD");
    info!("   - ZERODHA_TOTP_SECRET");
    info!("   - ZERODHA_API_KEY");
    info!("   - ZERODHA_API_SECRET");
    info!("3. Run: ./scripts/auth/setup-zerodha-auth.sh");
    info!("===========================================");

    Ok(())
}

fn demo_config() -> ZerodhaConfig {
    ZerodhaConfig::new(
        "DEMO123".to_string(),
        "demo_password".to_string(),
        "JBSWY3DPEHPK3PXP".to_string(), // Example TOTP secret
        "demo_api_key".to_string(),
        "demo_api_secret".to_string(),
    )
}
