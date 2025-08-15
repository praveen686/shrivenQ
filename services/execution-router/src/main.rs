//! Execution Router Service - Production gRPC Server
//!
//! Enterprise-grade execution routing with:
//! - Smart order routing
//! - Venue management
//! - Execution algorithms
//! - Fill management
//! - Health checks and metrics

use anyhow::Result;
use common::constants;
use execution_router::config::ExecutionConfig;
use execution_router::{ExecutionRouterService, grpc_impl::ExecutionServiceImpl};
use shrivenquant_proto::execution::v1::execution_service_server::ExecutionServiceServer;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;
use tonic::transport::Server;
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Constants
const DEFAULT_GRPC_PORT: u16 = 50054;
const SERVICE_NAME: &str = "execution-router";
const HEALTH_CHECK_INTERVAL_SECS: u64 = 10;
const DEFAULT_ORDER_CACHE_SIZE: usize = 10000;
const DEFAULT_VENUE_TIMEOUT_MS: u64 = 5000;
const DEFAULT_MAX_RETRIES: u32 = 3;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    init_tracing()?;
    
    info!("Starting Execution Router Service v{}", env!("CARGO_PKG_VERSION"));
    
    // Load configuration
    let config = load_config()?;
    
    // Create shutdown signal
    let (shutdown_tx, _shutdown_rx) = broadcast::channel::<()>(1);
    
    // Create Execution Router service
    let router_service = Arc::new(ExecutionRouterService::from_config(config));
    
    // Configure gRPC server address
    let addr: SocketAddr = format!("0.0.0.0:{}", DEFAULT_GRPC_PORT)
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid gRPC address: {}", e))?;
    
    info!("Execution Router listening on {}", addr);
    
    // Create gRPC service implementation
    let grpc_service = ExecutionServiceImpl::new(router_service.clone());
    
    // Run health check loop for service monitoring with shutdown support
    let health_check = router_service.clone();
    let mut shutdown_rx = shutdown_tx.subscribe();
    let shutdown_tx_health = shutdown_tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(HEALTH_CHECK_INTERVAL_SECS));
        let mut consecutive_failures = 0;
        const MAX_CONSECUTIVE_FAILURES: u32 = 3;
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if health_check.is_healthy().await {
                        info!("Execution Router health check: OK");
                        consecutive_failures = 0;
                    } else {
                        consecutive_failures += 1;
                        error!("Execution Router health check: FAILED (consecutive failures: {})", consecutive_failures);
                        
                        // Trigger shutdown if too many consecutive failures
                        if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                            error!("Too many consecutive health check failures. Initiating shutdown.");
                            let _ = shutdown_tx_health.send(());
                            break;
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Health check task received shutdown signal");
                    break;
                }
            }
        }
    });
    
    // Start gRPC server with graceful shutdown
    info!("Starting Execution Router gRPC server on {}", addr);
    
    let mut shutdown_rx = shutdown_tx.subscribe();
    
    Server::builder()
        .add_service(ExecutionServiceServer::new(grpc_service))
        .serve_with_shutdown(addr, async move {
            shutdown_rx.recv().await.ok();
            info!("Received shutdown signal, stopping gRPC server gracefully");
        })
        .await?;
    
    info!("Execution Router service shut down successfully");
    Ok(())
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

/// Load configuration from environment and defaults
fn load_config() -> Result<ExecutionConfig> {
    use execution_router::config::{VenueConfig, AlgorithmSettings, RiskCheckConfig, RetryConfig};
    use rustc_hash::FxHashMap;
    
    // Load venue configurations
    let mut venues = FxHashMap::default();
    
    // Load Binance configuration
    let binance_enabled = match std::env::var("BINANCE_ENABLED") {
        Ok(val) => val.parse::<bool>()
            .map_err(|e| anyhow::anyhow!("Invalid BINANCE_ENABLED: {}", e))?,
        Err(_) => true,  // Default to enabled
    };
    
    if binance_enabled {
        let api_url = match std::env::var("BINANCE_API_URL") {
            Ok(url) => url,
            Err(_) => "https://api.binance.com".to_string(),
        };
        
        let ws_url = Some(match std::env::var("BINANCE_WS_URL") {
            Ok(url) => url,
            Err(_) => "wss://stream.binance.com:9443/ws".to_string(),
        });
        
        let api_key = match std::env::var("BINANCE_API_KEY") {
            Ok(key) => key,
            Err(_) => String::new(),
        };
        
        let api_secret = match std::env::var("BINANCE_API_SECRET") {
            Ok(secret) => secret,
            Err(_) => String::new(),
        };
        
        let binance_config = VenueConfig {
            name: "binance".to_string(),
            api_url,
            ws_url,
            api_key,
            api_secret,
            max_orders_per_second: match std::env::var("BINANCE_MAX_ORDERS_PER_SEC") {
                Ok(val) => match val.parse() {
                    Ok(num) => num,
                    Err(_) => constants::trading::DEFAULT_MAX_ORDERS_PER_SEC,
                },
                Err(_) => constants::trading::DEFAULT_MAX_ORDERS_PER_SEC,
            },
            max_cancels_per_second: match std::env::var("BINANCE_MAX_CANCELS_PER_SEC") {
                Ok(val) => match val.parse() {
                    Ok(num) => num,
                    Err(_) => constants::trading::DEFAULT_MAX_ORDERS_PER_SEC,
                },
                Err(_) => constants::trading::DEFAULT_MAX_ORDERS_PER_SEC,
            },
            symbols: vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()],
            // Safe conversion: fee basis points are always positive and small
            maker_fee_bps: i32::try_from(constants::trading::MAKER_FEE_BP).unwrap_or(0),
            // Safe conversion: fee basis points are always positive and small
            taker_fee_bps: i32::try_from(constants::trading::TAKER_FEE_BP).unwrap_or(0),
        };
        venues.insert("binance".to_string(), binance_config);
    }
    
    // Load Zerodha configuration
    let zerodha_enabled = match std::env::var("ZERODHA_ENABLED") {
        Ok(val) => match val.parse::<bool>() {
            Ok(enabled) => enabled,
            Err(_) => true,  // Default to enabled if parse fails
        },
        Err(_) => true,  // Default to enabled if not set
    };
    
    if zerodha_enabled {
        let api_url = match std::env::var("ZERODHA_API_URL") {
            Ok(url) => url,
            Err(_) => "https://api.kite.trade".to_string(),
        };
        
        let ws_url = Some(match std::env::var("ZERODHA_WS_URL") {
            Ok(url) => url,
            Err(_) => "wss://ws.kite.trade".to_string(),
        });
        
        let api_key = match std::env::var("ZERODHA_API_KEY") {
            Ok(key) => key,
            Err(_) => String::new(),
        };
        
        let api_secret = match std::env::var("ZERODHA_API_SECRET") {
            Ok(secret) => secret,
            Err(_) => String::new(),
        };
        
        const DEFAULT_ZERODHA_ORDERS_PER_SEC: u32 = 5;
        const DEFAULT_ZERODHA_CANCELS_PER_SEC: u32 = 5;
        
        let zerodha_config = VenueConfig {
            name: "zerodha".to_string(),
            api_url,
            ws_url,
            api_key,
            api_secret,
            max_orders_per_second: match std::env::var("ZERODHA_MAX_ORDERS_PER_SEC") {
                Ok(val) => match val.parse() {
                    Ok(num) => num,
                    Err(_) => DEFAULT_ZERODHA_ORDERS_PER_SEC,
                },
                Err(_) => DEFAULT_ZERODHA_ORDERS_PER_SEC,
            },
            max_cancels_per_second: match std::env::var("ZERODHA_MAX_CANCELS_PER_SEC") {
                Ok(val) => match val.parse() {
                    Ok(num) => num,
                    Err(_) => DEFAULT_ZERODHA_CANCELS_PER_SEC,
                },
                Err(_) => DEFAULT_ZERODHA_CANCELS_PER_SEC,
            },
            symbols: vec!["NIFTY".to_string(), "BANKNIFTY".to_string()],
            maker_fee_bps: 3,  // 0.03% for Zerodha
            taker_fee_bps: 3,  // 0.03% for Zerodha
        };
        venues.insert("zerodha".to_string(), zerodha_config);
    }
    
    // If no venues configured, add default mock venue
    if venues.is_empty() {
        let mock_venue = VenueConfig {
            name: "mock".to_string(),
            api_url: "http://localhost:8080".to_string(),
            ws_url: None,
            api_key: String::new(),
            api_secret: String::new(),
            max_orders_per_second: 100,
            max_cancels_per_second: 100,
            symbols: vec!["TEST".to_string()],
            maker_fee_bps: 10,
            taker_fee_bps: 20,
        };
        venues.insert("mock".to_string(), mock_venue);
    }
    
    let default_venue = match std::env::var("DEFAULT_VENUE") {
        Ok(venue) => venue,
        Err(_) => match venues.keys().next() {
            Some(key) => key.clone(),
            None => "mock".to_string(),
        },
    };
    
    // Load algorithm settings
    const DEFAULT_VWAP_LOOKBACK_MINUTES: u32 = 30;
    
    let algorithm_settings = AlgorithmSettings {
        default_slice_duration: match std::env::var("ALGO_SLICE_DURATION") {
            Ok(val) => match val.parse() {
                Ok(num) => num,
                Err(_) => constants::time::SECS_PER_MINUTE,
            },
            Err(_) => constants::time::SECS_PER_MINUTE,
        },
        max_participation_rate: match std::env::var("ALGO_MAX_PARTICIPATION_RATE") {
            Ok(val) => match val.parse() {
                Ok(num) => num,
                // Safe conversion: participation rate percentage
                Err(_) => i32::try_from(constants::fixed_point::SCALE_3 / 10).unwrap_or(100), // 10%
            },
            // Safe conversion: participation rate percentage
            Err(_) => i32::try_from(constants::fixed_point::SCALE_3 / 10).unwrap_or(100),
        },
        min_order_size: match std::env::var("ALGO_MIN_ORDER_SIZE") {
            Ok(val) => match val.parse() {
                Ok(num) => num,
                Err(_) => constants::trading::MIN_ORDER_QTY,
            },
            Err(_) => constants::trading::MIN_ORDER_QTY,
        },
        max_order_size: match std::env::var("ALGO_MAX_ORDER_SIZE") {
            Ok(val) => match val.parse() {
                Ok(num) => num,
                Err(_) => constants::trading::MIN_ORDER_QTY * 1000,
            },
            Err(_) => constants::trading::MIN_ORDER_QTY * 1000,
        },
        vwap_lookback_minutes: match std::env::var("ALGO_VWAP_LOOKBACK") {
            Ok(val) => match val.parse() {
                Ok(num) => num,
                Err(_) => DEFAULT_VWAP_LOOKBACK_MINUTES,
            },
            Err(_) => DEFAULT_VWAP_LOOKBACK_MINUTES,
        },
        iceberg_display_pct: match std::env::var("ALGO_ICEBERG_DISPLAY_PCT") {
            Ok(val) => match val.parse() {
                Ok(num) => num,
                // Safe conversion: iceberg display percentage
                Err(_) => i32::try_from(constants::fixed_point::SCALE_3 / 5).unwrap_or(200), // 20%
            },
            // Safe conversion: iceberg display percentage
            Err(_) => i32::try_from(constants::fixed_point::SCALE_3 / 5).unwrap_or(200),
        },
    };
    
    // Load risk check configuration
    const DEFAULT_MAX_ORDER_VALUE: i64 = 1000000_0000; // 100K
    const DEFAULT_MAX_POSITION_VALUE: i64 = 10000000_0000; // 1M
    
    let risk_checks = RiskCheckConfig {
        enable_pretrade_checks: match std::env::var("ENABLE_PRETRADE_CHECKS") {
            Ok(val) => match val.parse() {
                Ok(enabled) => enabled,
                Err(_) => true,
            },
            Err(_) => true,
        },
        max_order_value: match std::env::var("MAX_ORDER_VALUE") {
            Ok(val) => match val.parse() {
                Ok(num) => num,
                Err(_) => DEFAULT_MAX_ORDER_VALUE,
            },
            Err(_) => DEFAULT_MAX_ORDER_VALUE,
        },
        max_position_value: match std::env::var("MAX_POSITION_VALUE") {
            Ok(val) => match val.parse() {
                Ok(num) => num,
                Err(_) => DEFAULT_MAX_POSITION_VALUE,
            },
            Err(_) => DEFAULT_MAX_POSITION_VALUE,
        },
        price_tolerance_pct: match std::env::var("PRICE_TOLERANCE_PCT") {
            Ok(val) => match val.parse() {
                Ok(num) => num,
                Err(_) => (constants::fixed_point::SCALE_2 * 5) as i32, // 5%
            },
            Err(_) => (constants::fixed_point::SCALE_2 * 5) as i32,
        },
        check_market_hours: match std::env::var("CHECK_MARKET_HOURS") {
            Ok(val) => match val.parse() {
                Ok(enabled) => enabled,
                Err(_) => true,
            },
            Err(_) => true,
        },
    };
    
    // Load retry configuration
    const DEFAULT_BACKOFF_MULTIPLIER: u32 = 2;
    
    let retry_config = RetryConfig {
        max_retries: match std::env::var("MAX_RETRIES") {
            Ok(val) => match val.parse() {
                Ok(num) => num,
                Err(_) => DEFAULT_MAX_RETRIES,
            },
            Err(_) => DEFAULT_MAX_RETRIES,
        },
        initial_delay_ms: match std::env::var("RETRY_INITIAL_DELAY_MS") {
            Ok(val) => match val.parse() {
                Ok(num) => num,
                Err(_) => constants::network::INITIAL_RETRY_DELAY_MS,
            },
            Err(_) => constants::network::INITIAL_RETRY_DELAY_MS,
        },
        max_delay_ms: match std::env::var("RETRY_MAX_DELAY_MS") {
            Ok(val) => match val.parse() {
                Ok(num) => num,
                Err(_) => constants::network::MAX_RETRY_DELAY_MS,
            },
            Err(_) => constants::network::MAX_RETRY_DELAY_MS,
        },
        backoff_multiplier: match std::env::var("RETRY_BACKOFF_MULTIPLIER") {
            Ok(val) => match val.parse() {
                Ok(num) => num,
                Err(_) => DEFAULT_BACKOFF_MULTIPLIER,
            },
            Err(_) => DEFAULT_BACKOFF_MULTIPLIER,
        },
    };
    
    Ok(ExecutionConfig {
        default_venue,
        venues,
        algorithm_settings,
        risk_checks,
        retry_config,
        order_cache_size: match std::env::var("ORDER_CACHE_SIZE") {
            Ok(val) => match val.parse() {
                Ok(num) => num,
                Err(_) => DEFAULT_ORDER_CACHE_SIZE,
            },
            Err(_) => DEFAULT_ORDER_CACHE_SIZE,
        },
        venue_timeout_ms: match std::env::var("VENUE_TIMEOUT_MS") {
            Ok(val) => match val.parse() {
                Ok(num) => num,
                Err(_) => DEFAULT_VENUE_TIMEOUT_MS,
            },
            Err(_) => DEFAULT_VENUE_TIMEOUT_MS,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_config_loading() {
        let config = load_config();
        assert!(config.is_ok());
    }
}