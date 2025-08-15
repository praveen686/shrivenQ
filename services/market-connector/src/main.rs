//! Market Connector Service - gRPC Server
//!
//! Provides real-time market data streaming via gRPC for all exchanges.
//! Handles WebSocket connections to exchanges and normalizes data format.

use anyhow::Result;
use market_connector::grpc_service::MarketDataGrpcService;
use shrivenquant_proto::marketdata::v1::market_data_service_server::MarketDataServiceServer;
use std::net::SocketAddr;
use tonic::transport::Server;
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Default port for Market Connector service
const DEFAULT_PORT: u16 = 50052;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "market_connector=info,tonic=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Market Connector Service");

    // Create gRPC service
    let (market_data_service, _event_sender) = MarketDataGrpcService::new();
    
    // Configure server address
    let addr: SocketAddr = format!("0.0.0.0:{}", DEFAULT_PORT)
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid socket address: {}", e))?;

    info!("Market Connector Service listening on {}", addr);

    // Start gRPC server
    Server::builder()
        .add_service(MarketDataServiceServer::new(market_data_service))
        .serve(addr)
        .await
        .map_err(|e| {
            error!("gRPC server error: {}", e);
            anyhow::anyhow!("Failed to start gRPC server: {}", e)
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_port() {
        assert_eq!(DEFAULT_PORT, 50052);
    }

    #[tokio::test]
    async fn test_service_creation() {
        let (service, _sender) = MarketDataGrpcService::new();
        // Service creation should not fail
        assert!(true);
    }
}