//! Risk Manager Service - Production gRPC Server
//!
//! Enterprise-grade risk management with:
//! - Health checks and readiness probes
//! - Prometheus metrics
//! - Circuit breakers
//! - Rate limiting
//! - Graceful shutdown
//! - Distributed tracing support

use anyhow::Result;
use services_common::constants;
use prometheus::{Encoder, TextEncoder};
use risk_manager::grpc_service::{RiskManagerGrpcService, RiskEvent};
use risk_manager::RiskLimits;
use services_common::risk::v1::risk_service_server::RiskServiceServer;
use std::net::SocketAddr;
use tokio::sync::broadcast;
use tonic::transport::Server;
use tonic_health::server::HealthReporter;
use tracing::{info, error, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use warp::Filter;

// Constants
const DEFAULT_GRPC_PORT: u16 = 50053;
const DEFAULT_METRICS_PORT: u16 = 9053;
const SERVICE_NAME: &str = "risk-manager";
const HEALTH_CHECK_MAX_DAILY_LOSS: i64 = -50_000_000;  // $5M loss threshold
const HEALTH_CHECK_MAX_DRAWDOWN: i64 = 5000;  // 50% drawdown in fixed-point (100 = 1%)
const DEFAULT_MAX_POSITION_VALUE: u64 = 100_000_000;  // $10M default
const DEFAULT_MAX_DAILY_LOSS: i64 = -5_000_000;  // $500K default loss limit
const DEFAULT_MAX_TOTAL_EXPOSURE: u64 = 1_000_000_000;  // $100M total
const DEFAULT_MAX_ORDER_SIZE: u64 = 1_000_000;  // 100 units in fixed-point
const DEFAULT_MAX_ORDERS_PER_MINUTE: u32 = 100;
const DEFAULT_MAX_DRAWDOWN_PCT: i32 = 1000;  // 10% in fixed-point
const DEFAULT_CIRCUIT_BREAKER_THRESHOLD: u32 = 5;
const DEFAULT_CIRCUIT_BREAKER_COOLDOWN: u64 = 300;  // 5 minutes
const MAX_ORDER_VALUE_DIVISOR: u64 = 10;  // Divide max position by 10 for max order value

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing with OpenTelemetry support
    init_tracing()?;
    
    info!("Starting Risk Manager Service v{}", env!("CARGO_PKG_VERSION"));
    
    // Load configuration
    let config = load_config()?;
    
    // Create shutdown signal
    let (shutdown_tx, _shutdown_rx) = broadcast::channel(1);
    
    // Initialize health reporter
    let (health_reporter, health_grpc_service) = tonic_health::server::health_reporter();
    
    // Create Risk Manager gRPC service
    let (risk_service, event_rx) = RiskManagerGrpcService::new(config)?;
    
    // Clone service for health checker (cheap since all fields are Arc)
    let health_check_service = risk_service.clone();
    
    // Start health check updater with access to risk service
    tokio::spawn(async move {
        update_health_status(health_reporter, health_check_service).await;
    });
    
    // Start Prometheus metrics server
    let metrics_addr: SocketAddr = format!("0.0.0.0:{DEFAULT_METRICS_PORT}")
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid metrics address: {}", e))?;
    
    tokio::spawn(async move {
        info!("Prometheus metrics server listening on {}", metrics_addr);
        serve_metrics(metrics_addr).await;
    });
    
    // Start event processor
    tokio::spawn(async move {
        process_risk_events(event_rx).await;
    });
    
    // Configure gRPC server address
    let grpc_addr: SocketAddr = format!("0.0.0.0:{DEFAULT_GRPC_PORT}")
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid gRPC address: {}", e))?;
    
    info!("Risk Manager gRPC server listening on {}", grpc_addr);
    
    // Build gRPC server with all services
    let server = Server::builder()
        .trace_fn(|_| tracing::info_span!("risk_grpc_request"))
        .add_service(health_grpc_service)
        .add_service(RiskServiceServer::new(risk_service))
        .serve_with_shutdown(grpc_addr, shutdown_signal(shutdown_tx.clone()));
    
    // Start server
    match server.await {
        Ok(()) => {
            info!("Risk Manager Service shutdown complete");
            Ok(())
        }
        Err(e) => {
            error!("gRPC server error: {}", e);
            Err(anyhow::anyhow!("Server failed: {}", e))
        }
    }
}

/// Initialize tracing with OpenTelemetry
fn init_tracing() -> Result<()> {
    // Set up tracing subscriber with multiple layers
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

/// Load configuration from environment and files
fn load_config() -> Result<RiskLimits> {
    // Try to load from environment variables first
    let max_position_value = match std::env::var("RISK_MAX_POSITION_SIZE") {
        Ok(val) => val.parse()
            .map_err(|e| anyhow::anyhow!("Invalid RISK_MAX_POSITION_SIZE: {}", e))?,
        Err(e) => {
            tracing::warn!("MAX_POSITION_VALUE not set, using default: {}", e);
            DEFAULT_MAX_POSITION_VALUE
        }
    };
    
    let max_daily_loss = match std::env::var("RISK_MAX_DAILY_LOSS") {
        Ok(val) => val.parse()
            .map_err(|e| anyhow::anyhow!("Invalid RISK_MAX_DAILY_LOSS: {}", e))?,
        Err(e) => {
            tracing::warn!("MAX_DAILY_LOSS not set, using default: {}", e);
            DEFAULT_MAX_DAILY_LOSS
        }
    };
    
    let circuit_breaker_threshold = match std::env::var("RISK_CIRCUIT_BREAKER_THRESHOLD") {
        Ok(val) => val.parse()
            .map_err(|e| anyhow::anyhow!("Invalid RISK_CIRCUIT_BREAKER_THRESHOLD: {}", e))?,
        Err(e) => {
            tracing::warn!("RISK_CIRCUIT_BREAKER_THRESHOLD not set, using default: {}", e);
            DEFAULT_CIRCUIT_BREAKER_THRESHOLD
        }
    };
    
    Ok(RiskLimits {
        // Safe conversion: MAX_ORDER_SIZE_TICKS is always positive and within u64 range
        max_position_size: u64::try_from(constants::trading::MAX_ORDER_SIZE_TICKS)
            .map_err(|_| anyhow::anyhow!("MAX_ORDER_SIZE_TICKS exceeds u64 range"))?,
        max_position_value,
        max_total_exposure: DEFAULT_MAX_TOTAL_EXPOSURE,
        max_order_size: DEFAULT_MAX_ORDER_SIZE,
        max_order_value: max_position_value / MAX_ORDER_VALUE_DIVISOR,
        max_orders_per_minute: DEFAULT_MAX_ORDERS_PER_MINUTE,
        max_daily_loss,
        max_drawdown_pct: DEFAULT_MAX_DRAWDOWN_PCT,
        circuit_breaker_threshold,
        circuit_breaker_cooldown: DEFAULT_CIRCUIT_BREAKER_COOLDOWN,
    })
}

/// Update health status periodically
async fn update_health_status(
    mut reporter: HealthReporter,
    risk_service: RiskManagerGrpcService
) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
    
    loop {
        interval.tick().await;
        
        // Check various health conditions with actual risk service state
        let is_healthy = check_service_health_with_risk(&risk_service).await;
        
        if is_healthy {
            // Set service as healthy - using the service name directly
            reporter.set_service_status(
                "shrivenquant.risk.v1.RiskService",
                tonic_health::ServingStatus::Serving
            ).await;
        } else {
            // Set service as unhealthy
            reporter.set_service_status(
                "shrivenquant.risk.v1.RiskService",
                tonic_health::ServingStatus::NotServing
            ).await;
        }
    }
}

/// Check service health with risk service context
async fn check_service_health_with_risk(risk_service: &RiskManagerGrpcService) -> bool {
    // Check if kill switch is active
    if risk_service.risk_manager.is_kill_switch_active() {
        return false;
    }
    
    // Check circuit breaker status
    if risk_service.circuit_breaker.is_open() {
        return false;
    }
    
    // Get current metrics and check for critical conditions
    let metrics = risk_service.risk_manager.get_metrics().await;
    
    // Check for excessive losses
    if metrics.daily_pnl < HEALTH_CHECK_MAX_DAILY_LOSS {
        return false;
    }
    
    // Check for high drawdown
    // Safe conversion: comparing drawdown percentages
    let drawdown_i64 = if let Ok(val) = i64::try_from(metrics.current_drawdown) { val } else {
        tracing::warn!("Drawdown {} exceeds i64 range, treating as unhealthy", metrics.current_drawdown);
        return false; // Treat overflow as unhealthy
    };
    if drawdown_i64 > HEALTH_CHECK_MAX_DRAWDOWN {
        return false;
    }
    
    // Check if circuit breaker is active
    if metrics.circuit_breaker_active {
        return false;
    }
    
    true
}

/// Serve Prometheus metrics
async fn serve_metrics(addr: SocketAddr) {
    let metrics_route = warp::path("metrics")
        .map(|| {
            let encoder = TextEncoder::new();
            let metric_families = prometheus::gather();
            let mut buffer = Vec::new();
            match encoder.encode(&metric_families, &mut buffer) {
                Ok(()) => String::from_utf8(buffer).unwrap_or_else(|_| "Error encoding metrics".to_string()),
                Err(_) => "Error gathering metrics".to_string(),
            }
        });
    
    let health_route = warp::path("health")
        .map(|| "OK");
    
    let routes = metrics_route.or(health_route);
    
    warp::serve(routes).run(addr).await;
}

/// Process risk events for monitoring
async fn process_risk_events(mut event_rx: tokio::sync::broadcast::Receiver<RiskEvent>) {
    while let Ok(event) = event_rx.recv().await {
        // Log events
        match event.event_type {
            risk_manager::grpc_service::RiskEventType::OrderRejected => {
                warn!("Order rejected: {}", event.message);
            }
            risk_manager::grpc_service::RiskEventType::LimitBreached => {
                error!("Risk limit breached: {}", event.message);
            }
            risk_manager::grpc_service::RiskEventType::KillSwitchActivated => {
                error!("KILL SWITCH ACTIVATED: {}", event.message);
            }
            _ => {
                info!("Risk event: {:?}", event);
            }
        }
        
        // Could also send to external monitoring systems
    }
}

/// Graceful shutdown signal handler
async fn shutdown_signal(shutdown_tx: broadcast::Sender<()>) {
    // Listen for ctrl-c
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("Failed to install Ctrl+C handler: {}", e);
        }
    };
    
    // Listen for SIGTERM
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(e) => {
                error!("Failed to install SIGTERM handler: {}", e);
            }
        }
    };
    
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    
    tokio::select! {
        () = ctrl_c => {
            info!("Received Ctrl+C, initiating graceful shutdown");
        }
        () = terminate => {
            info!("Received SIGTERM, initiating graceful shutdown");
        }
    }
    
    // Notify all components to shutdown
    let _ = shutdown_tx.send(());
    
    // Give services time to cleanup
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_config_loading() {
        let config = load_config();
        assert!(config.is_ok());
    }
    
    #[tokio::test]
    async fn test_health_check() {
        let is_healthy = check_service_health().await;
        assert!(is_healthy);
    }
}