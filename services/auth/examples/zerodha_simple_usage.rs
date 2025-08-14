//! Simple example of using Zerodha authentication
//!
//! This example shows the minimal code needed to:
//! 1. Load credentials from .env file
//! 2. Authenticate with Zerodha
//! 3. Use the token for API calls

use anyhow::Result;
use auth_service::providers::zerodha::{ZerodhaAuth, ZerodhaConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().init();

    // Load configuration from .env file
    let config = ZerodhaConfig::from_env_file()?;

    // Create auth handler
    let auth = ZerodhaAuth::new(config);

    // Authenticate (will use cached token if available)
    let access_token = auth.authenticate().await?;
    println!("✅ Authenticated! Token: {}...", &access_token[..20]);

    // Get user profile
    let profile = auth.get_profile().await?;
    println!("User: {}", profile.user_id);
    println!("Email: {}", profile.email);

    // Get margins
    let margins = auth.get_margins().await?;
    if let Some(equity) = margins.get("equity") {
        if let Some(cash) = equity.available.get("cash") {
            println!("Available margin: ₹{:.2}", cash);
        }
    }

    // The token can be used for API calls
    println!("\nYou can now use this token for:");
    println!(
        "- WebSocket: wss://ws.kite.trade?api_key={}&access_token={}",
        auth.get_api_key(),
        access_token
    );
    println!(
        "- REST API calls with header: Authorization: token {}:{}",
        auth.get_api_key(),
        access_token
    );

    Ok(())
}
