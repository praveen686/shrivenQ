use anyhow::Result;
use tonic::transport::Server;
use tracing::{info, error};
use tracing_subscriber;

mod production_strategy;
mod enhanced_strategy;
mod lib;
mod grpc_service;

pub mod pb {
    tonic::include_proto!("trading");
}

use pb::trading_strategies_server::TradingStrategiesServer;
use grpc_service::TradingStrategiesService;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("trading_strategies=debug,info")
        .init();
    
    let addr = "[::1]:50065".parse()?;
    let service = TradingStrategiesService::new();
    
    info!("🎯 Trading Strategies Service starting on {}", addr);
    info!("📈 Strategies: Production, Enhanced, Market Making");
    info!("💹 Features: Kelly Criterion, Risk Management, Smart Routing");
    
    match Server::builder()
        .add_service(TradingStrategiesServer::new(service))
        .serve(addr)
        .await 
    {
        Ok(_) => info!("Trading Strategies shut down gracefully"),
        Err(e) => error!("Trading Strategies error: {}", e),
    }
    
    Ok(())
}