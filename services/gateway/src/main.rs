//! `ShrivenQuant` API Gateway - Main Entry Point

use anyhow::Result;
use clap::{Arg, Command};
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use api_gateway::{GatewayConfig, start_server};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "api_gateway=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse command line arguments
    let matches = Command::new("api-gateway")
        .version(env!("CARGO_PKG_VERSION"))
        .author("ShrivenQuant Team")
        .about("Unified REST API Gateway for ShrivenQuant Trading Platform")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Configuration file path")
                .default_value("gateway.toml"),
        )
        .arg(
            Arg::new("routes")
                .long("routes")
                .help("Print available routes and exit")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    // Print routes if requested
    if matches.get_flag("routes") {
        api_gateway::server::print_routes();
        return Ok(());
    }

    // Load configuration
    let default_config = "gateway.toml".to_string();
    let config_path = matches
        .get_one::<String>("config")
        .unwrap_or(&default_config);
    let config = match GatewayConfig::from_file(config_path) {
        Ok(config) => {
            info!("Loaded configuration from: {}", config_path);
            config
        }
        Err(e) => {
            error!("Failed to load config from {}: {}", config_path, e);
            info!("Using default configuration");
            GatewayConfig::default()
        }
    };

    // Print startup information
    info!(
        "Starting ShrivenQuant API Gateway v{}",
        env!("CARGO_PKG_VERSION")
    );
    info!("Server will bind to: {}", config.server_address());
    info!("gRPC Services:");
    info!("  Auth: {}", config.services.auth_service);
    info!("  Execution: {}", config.services.execution_service);
    info!("  Market Data: {}", config.services.market_data_service);
    info!("  Risk: {}", config.services.risk_service);

    if let Some(portfolio) = &config.services.portfolio_service {
        info!("  Portfolio: {}", portfolio);
    }
    if let Some(reporting) = &config.services.reporting_service {
        info!("  Reporting: {}", reporting);
    }

    info!("Features enabled:");
    info!("  CORS: {}", config.cors.enabled);
    info!("  Rate Limiting: {}", config.rate_limiting.enabled);
    info!("  Metrics: {}", config.monitoring.metrics_enabled);
    info!("  Tracing: {}", config.monitoring.tracing_enabled);
    info!("  Compression: {}", config.server.compression);

    // Start the server
    if let Err(e) = start_server(config).await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
