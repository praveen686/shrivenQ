//! Trading Gateway gRPC Service
//! 
//! Production-ready service that orchestrates all trading components

use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal;
use tonic::transport::Server;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use trading_gateway::{GatewayConfig, TradingGateway};
use services_common::proto::trading::v1::trading_gateway_server::{TradingGateway as TradingGatewayService, TradingGatewayServer};

mod grpc_service;
use grpc_service::TradingGatewayServiceImpl;

const SERVICE_NAME: &str = "trading-gateway";
const DEFAULT_PORT: u16 = 50059;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    init_tracing()?;
    
    info!("🚀 Starting ShrivenQuant Trading Gateway Service v{}", env!("CARGO_PKG_VERSION"));
    
    // Load configuration
    let config = load_config()?;
    
    // Create trading gateway
    let gateway = Arc::new(TradingGateway::new(config).await?);
    
    // Start gateway components
    gateway.start().await?;
    info!("✅ Trading Gateway initialized successfully");
    
    // Create gRPC service
    // Create gRPC service implementation that implements TradingGatewayService trait
    let service: TradingGatewayServiceImpl = TradingGatewayServiceImpl::new(gateway.clone());
    let _: &dyn TradingGatewayService = &service; // Ensure trait is implemented
    
    // Configure server address
    let addr: SocketAddr = format!("0.0.0.0:{}", DEFAULT_PORT)
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid address: {}", e))?;
    
    info!("🔌 Trading Gateway gRPC server listening on {}", addr);
    
    // Start health check endpoint
    start_health_check(gateway.clone());
    
    // Start metrics endpoint
    start_metrics_endpoint();
    
    // Build gRPC server with trait implementation
    let server = Server::builder()
        .add_service(TradingGatewayServer::new(service))
        .serve(addr);
    
    // Handle shutdown signal
    let shutdown = signal::ctrl_c();
    
    tokio::select! {
        res = server => {
            if let Err(e) = res {
                error!("gRPC server error: {}", e);
            }
        }
        _ = shutdown => {
            info!("Received shutdown signal");
            
            // Emergency stop before shutdown
            if let Err(e) = gateway.emergency_stop().await {
                error!("Failed to execute emergency stop: {}", e);
            }
        }
    }
    
    info!("Trading Gateway shut down gracefully");
    Ok(())
}

/// Load configuration from environment or file
fn load_config() -> Result<GatewayConfig> {
    // Check for config file
    if let Ok(config_path) = std::env::var("GATEWAY_CONFIG") {
        info!("Loading config from: {}", config_path);
        let config_str = std::fs::read_to_string(config_path)?;
        let config: GatewayConfig = serde_json::from_str(&config_str)?;
        return Ok(config);
    }
    
    // Use default config with environment overrides
    let mut config = GatewayConfig::default();
    
    if let Ok(max_pos) = std::env::var("MAX_POSITION_SIZE") {
        config.max_position_size = services_common::Qty::from_i64(max_pos.parse()?);
    }
    
    if let Ok(max_loss) = std::env::var("MAX_DAILY_LOSS") {
        config.max_daily_loss = max_loss.parse()?;
    }
    
    if let Ok(threshold) = std::env::var("CIRCUIT_BREAKER_THRESHOLD") {
        config.circuit_breaker_threshold = threshold.parse()?;
    }
    
    // Strategy flags
    config.enable_market_making = std::env::var("ENABLE_MARKET_MAKING")
        .map(|v| v == "true")
        .unwrap_or(true);
        
    config.enable_momentum = std::env::var("ENABLE_MOMENTUM")
        .map(|v| v == "true")
        .unwrap_or(true);
        
    config.enable_arbitrage = std::env::var("ENABLE_ARBITRAGE")
        .map(|v| v == "true")
        .unwrap_or(true);
    
    info!("Loaded configuration: {:?}", config);
    Ok(config)
}

/// Initialize tracing
fn init_tracing() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| {
                    format!(
                        "{}=info,tower=info,tonic=info,h2=info",
                        SERVICE_NAME.replace('-', "_")
                    ).into()
                }),
        )
        .with(tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_thread_names(true))
        .init();
    
    Ok(())
}

/// Start health check endpoint
fn start_health_check(gateway: Arc<TradingGateway>) {
    tokio::spawn(async move {
        let app = axum::Router::new()
            .route("/health", axum::routing::get(move || async move {
                let status = gateway.get_status();
                
                match status {
                    trading_gateway::GatewayStatus::Running => {
                        (axum::http::StatusCode::OK, axum::response::Json(serde_json::json!({
                            "status": "healthy",
                            "service": SERVICE_NAME,
                        })))
                    }
                    _ => {
                        (axum::http::StatusCode::SERVICE_UNAVAILABLE, axum::response::Json(serde_json::json!({
                            "status": "unhealthy",
                            "service": SERVICE_NAME,
                        })))
                    }
                }
            }));
        
        let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
        info!("Health check endpoint available at http://{}/health", addr);
        
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to bind health check server to {}: {}", addr, e);
                return;
            }
        };
        
        if let Err(e) = axum::serve(listener, app).await {
            error!("Health check server error: {}", e);
        }
    });
}

/// Start Prometheus metrics endpoint
fn start_metrics_endpoint() {
    tokio::spawn(async {
        // For now, just create a simple endpoint
        let app = axum::Router::new()
            .route("/metrics", axum::routing::get(|| async {
                "# Metrics endpoint placeholder\n"
            }));
        
        let addr = SocketAddr::from(([0, 0, 0, 0], 9090));
        info!("Metrics endpoint available at http://{}/metrics", addr);
        
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to bind metrics server to {}: {}", addr, e);
                return;
            }
        };
        
        if let Err(e) = axum::serve(listener, app).await {
            error!("Metrics server error: {}", e);
        }
    });
}