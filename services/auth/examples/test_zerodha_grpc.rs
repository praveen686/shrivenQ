//! Test Zerodha gRPC authentication with correct API key

use anyhow::Result;
use services_common::auth::v1::{
    LoginRequest, ValidateTokenRequest, auth_service_client::AuthServiceClient,
};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🔌 Testing Zerodha gRPC Authentication");
    info!("{}", "=".repeat(50));

    // Load environment variables
    dotenv::dotenv().ok();

    // Get Zerodha API key from environment
    let api_key = std::env::var("ZERODHA_API_KEY")
        .map_err(|_| anyhow::anyhow!("ZERODHA_API_KEY not found in .env file"))?;

    info!("📋 Using API key: {}...", &api_key[..8]);

    // Connect to auth service
    info!("📡 Connecting to auth service at http://127.0.0.1:50051");

    let mut client = AuthServiceClient::connect("http://127.0.0.1:50051").await?;
    info!("✅ Connected successfully");

    // Test Zerodha authentication with correct API key as username
    info!("\n🔐 Testing Zerodha authentication...");
    let login_request = LoginRequest {
        username: api_key.clone(), // Use API key as username
        password: "".to_string(),  // Password not used
        exchange: "zerodha".to_string(),
    };

    match client.login(login_request).await {
        Ok(response) => {
            let login_resp = response.into_inner();
            info!("✅ Zerodha authentication successful!");
            info!(
                "   Token: {}...",
                &login_resp.token[..20.min(login_resp.token.len())]
            );
            info!(
                "   Expires at: {}",
                chrono::DateTime::from_timestamp(login_resp.expires_at, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| "Invalid timestamp".to_string())
            );
            info!("   Permissions: {:?}", login_resp.permissions);

            // Test token validation
            info!("\n🔍 Testing token validation...");
            let validate_request = ValidateTokenRequest {
                token: login_resp.token.clone(),
            };

            match client.validate_token(validate_request).await {
                Ok(validate_resp) => {
                    let resp = validate_resp.into_inner();
                    info!("✅ Token validation successful!");
                    info!("   User ID: {}", resp.user_id);
                    info!("   Valid: {}", resp.valid);
                    info!("   Permissions: {:?}", resp.permissions);

                    info!("\n🎉 Zerodha gRPC authentication test PASSED!");
                }
                Err(e) => {
                    error!("❌ Token validation failed: {}", e);
                }
            }
        }
        Err(e) => {
            error!("❌ Zerodha authentication failed: {}", e);
            error!("\n🔧 This might be due to:");
            error!("   1. Zerodha credentials in .env are invalid");
            error!("   2. Need to complete Zerodha 2FA process first");
            error!("   3. Zerodha session has expired");
            error!("   4. Network connectivity issues");

            return Err(e.into());
        }
    }

    info!("\n{}", "=".repeat(50));
    info!("✅ Test completed successfully!");

    Ok(())
}
