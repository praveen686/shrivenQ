//! ShrivenQuant API Gateway
//!
//! Unified REST API gateway providing HTTP access to all microservices.
//! Features:
//! - REST-to-gRPC translation
//! - JWT authentication middleware
//! - Rate limiting and monitoring
//! - WebSocket streaming for real-time data
//! - Fixed-point precision preservation

use anyhow::Result;

pub mod config;
pub mod grpc_clients;
pub mod handlers;
pub mod metrics;
pub mod middleware;
pub mod models;
pub mod rate_limiter;
pub mod server;
pub mod websocket;

pub use config::GatewayConfig;
pub use server::ApiGatewayServer;

/// Start the API Gateway server
pub async fn start_server(config: GatewayConfig) -> Result<()> {
    let server = ApiGatewayServer::new(config).await?;
    server.start().await
}
