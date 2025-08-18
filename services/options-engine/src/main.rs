use anyhow::Result;
use tonic::transport::Server;
use tracing::{info, error};
use tracing_subscriber;

mod lib;
mod grpc_service;

pub mod pb {
    tonic::include_proto!("options");
}

use pb::options_engine_server::OptionsEngineServer;
use grpc_service::OptionsEngineService;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("options_engine=debug,info")
        .init();
    
    let addr = "[::1]:50060".parse()?;
    let service = OptionsEngineService::new();
    
    info!("🚀 Options Engine Service starting on {}", addr);
    info!("📊 Features: Black-Scholes, Greeks, IV, Strategies");
    info!("🇮🇳 Supporting: NIFTY50, BANKNIFTY, FINNIFTY, MIDCAPNIFTY");
    
    match Server::builder()
        .add_service(OptionsEngineServer::new(service))
        .serve(addr)
        .await 
    {
        Ok(_) => info!("Options Engine shut down gracefully"),
        Err(e) => error!("Options Engine error: {}", e),
    }
    
    Ok(())
}
