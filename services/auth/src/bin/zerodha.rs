//! Zerodha authentication and connection management
//!
//! This module handles all Zerodha-related authentication including:
//! - API key management
//! - TOTP-based automated login
//! - Token caching and refresh
//! - gRPC service integration
//! - WebSocket and REST API connectivity

use anyhow::Result;
use auth_service::providers::zerodha::{ZerodhaAuth, ZerodhaConfig};
use services_common::clients::{SecretsClient, SecretsClientBuilder};
use services_common::proto::auth::v1::{ValidateTokenRequest, auth_service_client::AuthServiceClient};
use tonic::transport::Channel;
use tracing::{error, info, warn, Level};

/// Main Zerodha authentication service
pub struct ZerodhaService {
    auth: ZerodhaAuth,
    grpc_client: Option<AuthServiceClient<Channel>>,
}

impl ZerodhaService {
    /// Create new Zerodha service instance
    pub async fn new() -> Result<Self> {
        // Try to connect to secrets manager first
        let mut secrets_client = match SecretsClientBuilder::new("zerodha")
            .endpoint("http://127.0.0.1:50053")
            .build()
            .await
        {
            Ok(client) => {
                info!("Connected to secrets manager");
                Some(client)
            }
            Err(e) => {
                warn!("Could not connect to secrets manager: {}, will use .env file", e);
                None
            }
        };
        
        // Load configuration with fallback
        let config = ZerodhaConfig::load_config(secrets_client.as_mut()).await
            .map_err(|e| {
                error!("Failed to load Zerodha config: {}", e);
                e
            })?;
        
        let auth = ZerodhaAuth::new(config);
        
        // Try to connect to gRPC service
        let grpc_client = match AuthServiceClient::connect("http://localhost:50051").await {
            Ok(client) => {
                info!("Connected to gRPC auth service");
                Some(client)
            }
            Err(e) => {
                warn!("Could not connect to gRPC service: {}", e);
                None
            }
        };
        
        Ok(Self { auth, grpc_client })
    }
    
    /// Authenticate with Zerodha
    pub async fn authenticate(&self) -> Result<String> {
        info!("Authenticating with Zerodha...");
        
        // Try cached token first
        if let Ok(token) = self.auth.authenticate().await {
            info!("Using cached token");
            return Ok(token);
        }
        
        // Perform fresh authentication with TOTP
        info!("Performing fresh authentication with TOTP");
        let token = self.auth.authenticate().await?;
        info!("Authentication successful");
        
        // Token is already validated by the auth provider
        
        Ok(token)
    }
    
    /// Get user profile
    pub async fn get_profile(&self) -> Result<auth_service::providers::zerodha::UserProfile> {
        self.auth.get_profile().await
    }
    
    /// Get margins
    pub async fn get_margins(&self) -> Result<rustc_hash::FxHashMap<String, auth_service::providers::zerodha::MarginData>> {
        self.auth.get_margins().await
    }
    
    /// Get WebSocket URL with authentication
    pub fn get_websocket_url(&self, token: &str) -> String {
        format!(
            "wss://ws.kite.trade?api_key={}&access_token={}",
            self.auth.get_api_key(),
            token
        )
    }
    
    /// Get REST API authorization header
    pub fn get_auth_header(&self, token: &str) -> String {
        format!("token {}:{}", self.auth.get_api_key(), token)
    }
    
    /// Refresh token by re-authenticating
    pub async fn refresh_token(&mut self, _token: &str) -> Result<String> {
        // For Zerodha, we don't have a refresh mechanism, just re-authenticate
        self.authenticate().await
    }
}

/// Command-line interface
#[derive(Debug, clap::Parser)]
#[clap(name = "zerodha", about = "Zerodha authentication and connection management")]
struct Cli {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// Authenticate and get access token
    Auth,
    /// Get user profile
    Profile,
    /// Get account margins
    Margins,
    /// Get WebSocket connection URL
    WebSocket,
    /// Validate token
    Validate {
        #[clap(long)]
        token: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    use clap::Parser;
    
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();
    
    info!("🚀 Zerodha Authentication Service");
    info!("{}", "=".repeat(50));
    
    let cli = Cli::parse();
    let mut service = ZerodhaService::new().await?;
    
    match cli.command {
        Command::Auth => {
            let token = service.authenticate().await?;
            info!("✅ Authenticated successfully");
            info!("Token: {}...", &token[..20.min(token.len())]);
            info!("\nWebSocket URL: {}", service.get_websocket_url(&token));
            info!("Auth Header: {}", service.get_auth_header(&token));
        }
        Command::Profile => {
            let _ = service.authenticate().await?;
            let profile = service.get_profile().await?;
            info!("User Profile:");
            info!("  User ID: {}", profile.user_id);
            info!("  Email: {}", profile.email);
            info!("  Exchanges: {:?}", profile.exchanges);
            info!("  Products: {:?}", profile.products);
        }
        Command::Margins => {
            let _ = service.authenticate().await?;
            let margins = service.get_margins().await?;
            info!("Account Margins:");
            for (segment, margin) in margins {
                info!("\n{}:", segment);
                if let Some(cash) = margin.available.get("cash") {
                    info!("  Available Cash: ₹{:.2}", cash);
                }
                if let Some(collateral) = margin.available.get("collateral") {
                    info!("  Available Collateral: ₹{:.2}", collateral);
                }
                if let Some(total) = margin.total.get("cash") {
                    info!("  Total: ₹{:.2}", total);
                }
            }
        }
        Command::WebSocket => {
            let token = service.authenticate().await?;
            let url = service.get_websocket_url(&token);
            info!("WebSocket URL: {}", url);
        }
        Command::Validate { token } => {
            if service.grpc_client.is_some() {
                let mut client = service.grpc_client.unwrap();
                let validate_request = ValidateTokenRequest {
                    token: token.clone(),
                };
                
                let response = client.validate_token(validate_request).await?;
                let valid = response.into_inner().valid;
                
                if valid {
                    info!("✅ Token is valid");
                } else {
                    error!("❌ Token is invalid");
                }
            } else {
                error!("gRPC service not available");
            }
        }
    }
    
    Ok(())
}