//! Auth Service main entry point
//!
//! Starts both REST API and gRPC servers for the authentication service

use anyhow::Result;
use auth_service::{
    AuthService, binance_service::create_binance_service, grpc::AuthServiceGrpc,
    zerodha_service::create_auth_service,
};
use services_common::constants::network::DEFAULT_GRPC_PORT;
use services_common::proto::auth::v1::auth_service_server::AuthServiceServer;
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;
use tracing::{error, info};

/// Create unified authentication service that supports both Binance and Zerodha
async fn create_unified_auth_service() -> Result<Arc<dyn AuthService>> {
    // Check what credentials are available
    let has_binance = std::env::var("BINANCE_SPOT_API_KEY").is_ok()
        || std::env::var("BINANCE_FUTURES_API_KEY").is_ok();
    let has_zerodha = std::env::var("ZERODHA_API_KEY").is_ok();

    match (has_binance, has_zerodha) {
        (true, true) => {
            info!("🔧 Using multi-exchange auth service (Binance + Zerodha)");
            // Create a wrapper that can handle both
            Ok(Arc::new(MultiExchangeAuthService::new().await?))
        }
        (true, false) => {
            info!("🔧 Using Binance authentication service");
            Ok(Arc::new(create_binance_service().await?))
        }
        (false, true) => {
            info!("🔧 Using Zerodha authentication service");
            create_auth_service()
        }
        (false, false) => {
            info!("🔧 No credentials found, using demo authentication service");
            create_auth_service() // Will create demo service
        }
    }
}

/// Multi-exchange authentication service that can handle both Binance and Zerodha
pub struct MultiExchangeAuthService {
    binance_service: Option<Arc<dyn AuthService>>,
    zerodha_service: Option<Arc<dyn AuthService>>,
}

impl MultiExchangeAuthService {
    async fn new() -> Result<Self> {
        let mut binance_service = None;
        let mut zerodha_service = None;

        // Initialize Binance if available
        if std::env::var("BINANCE_SPOT_API_KEY").is_ok()
            || std::env::var("BINANCE_FUTURES_API_KEY").is_ok()
        {
            match create_binance_service().await {
                Ok(service) => {
                    info!("✅ Binance service initialized");
                    binance_service = Some(Arc::new(service) as Arc<dyn AuthService>);
                }
                Err(e) => {
                    error!("❌ Failed to initialize Binance service: {}", e);
                }
            }
        }

        // Initialize Zerodha if available
        if std::env::var("ZERODHA_API_KEY").is_ok() {
            match create_auth_service() {
                Ok(service) => {
                    info!("✅ Zerodha service initialized");
                    zerodha_service = Some(service);
                }
                Err(e) => {
                    error!("❌ Failed to initialize Zerodha service: {}", e);
                }
            }
        }

        Ok(Self {
            binance_service,
            zerodha_service,
        })
    }
}

#[tonic::async_trait]
impl AuthService for MultiExchangeAuthService {
    async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<auth_service::AuthContext> {
        // Route to appropriate service based on username
        if username.starts_with("binance_") {
            if let Some(binance) = &self.binance_service {
                info!("🔄 Routing to Binance service for: {}", username);
                return binance.authenticate(username, password).await;
            }
            return Err(anyhow::anyhow!("Binance service not configured"));
        } else if username == "zerodha" || username.contains("zerodha") {
            if let Some(zerodha) = &self.zerodha_service {
                info!("🔄 Routing to Zerodha service for: {}", username);
                return zerodha.authenticate(username, password).await;
            }
            return Err(anyhow::anyhow!("Zerodha service not configured"));
        }
        // Try Zerodha as default for unknown usernames
        if let Some(zerodha) = &self.zerodha_service {
            info!("🔄 Defaulting to Zerodha service for: {}", username);
            return zerodha.authenticate(username, password).await;
        }

        Err(anyhow::anyhow!(
            "No suitable authentication service found for username: {}",
            username
        ))
    }

    async fn validate_token(&self, token: &str) -> Result<auth_service::AuthContext> {
        // Try both services to validate the token
        if let Some(binance) = &self.binance_service {
            if let Ok(context) = binance.validate_token(token).await {
                return Ok(context);
            }
        }

        if let Some(zerodha) = &self.zerodha_service {
            if let Ok(context) = zerodha.validate_token(token).await {
                return Ok(context);
            }
        }

        Err(anyhow::anyhow!("Token validation failed"))
    }

    async fn generate_token(&self, context: &auth_service::AuthContext) -> Result<String> {
        // Use the exchange from metadata to route to correct service
        if let Some(exchange) = context.metadata.get("exchange") {
            if exchange == "binance" {
                if let Some(binance) = &self.binance_service {
                    return binance.generate_token(context).await;
                }
            } else if exchange == "zerodha" {
                if let Some(zerodha) = &self.zerodha_service {
                    return zerodha.generate_token(context).await;
                }
            }
        }

        // Default to first available service
        if let Some(binance) = &self.binance_service {
            return binance.generate_token(context).await;
        }

        if let Some(zerodha) = &self.zerodha_service {
            return zerodha.generate_token(context).await;
        }

        Err(anyhow::anyhow!("No service available to generate token"))
    }

    async fn check_permission(
        &self,
        context: &auth_service::AuthContext,
        permission: auth_service::Permission,
    ) -> bool {
        // Permission check is exchange-agnostic
        context.permissions.contains(&permission)
    }

    async fn revoke_token(&self, token: &str) -> Result<()> {
        // Try to revoke from all services
        let mut errors = Vec::new();

        if let Some(binance) = &self.binance_service {
            if let Err(e) = binance.revoke_token(token).await {
                errors.push(format!("Binance: {e}"));
            }
        }

        if let Some(zerodha) = &self.zerodha_service {
            if let Err(e) = zerodha.revoke_token(token).await {
                errors.push(format!("Zerodha: {e}"));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Token revocation errors: {}",
                errors.join(", ")
            ))
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("auth=info,tower_http=debug")
        .init();

    info!("Starting ShrivenQuant Auth Service");

    // Create unified auth service that supports both Binance and Zerodha
    dotenv::dotenv().ok();

    let auth_service = create_unified_auth_service().await?;

    // Create gRPC service
    let grpc_service = AuthServiceGrpc::new(auth_service);

    // Start gRPC server
    let grpc_addr: SocketAddr = format!("0.0.0.0:{DEFAULT_GRPC_PORT}").parse()?;

    info!("Starting gRPC server on {}", grpc_addr);

    // Start gRPC server (blocking)
    info!("🚀 gRPC server starting...");

    Server::builder()
        .add_service(AuthServiceServer::new(grpc_service))
        .serve(grpc_addr)
        .await
        .map_err(|e| {
            error!("❌ gRPC server failed: {}", e);
            anyhow::anyhow!("gRPC server error: {}", e)
        })?;

    info!("✅ gRPC server shutdown gracefully");

    Ok(())
}
