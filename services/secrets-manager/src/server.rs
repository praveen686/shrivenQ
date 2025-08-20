//! gRPC server for secrets management service

use anyhow::Result;
use secrets_manager::grpc_service;
use std::net::SocketAddr;
use tonic::transport::Server;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("secrets_manager=info,tonic=info")
        .init();
    
    info!("Starting Secrets Manager gRPC server");
    
    // Get master password from environment
    let master_password = std::env::var("MASTER_PASSWORD")
        .unwrap_or_else(|_| {
            error!("MASTER_PASSWORD not set, using default (INSECURE!)");
            "development_password_change_me".to_string()
        });
    
    // Create the gRPC service
    let service = grpc_service::create_server(&master_password)?;
    
    // Bind to address
    let addr: SocketAddr = "127.0.0.1:50053".parse()?;
    
    info!("Secrets Manager gRPC server listening on {}", addr);
    
    // Start the server
    Server::builder()
        .add_service(service)
        .serve(addr)
        .await?;
    
    Ok(())
}