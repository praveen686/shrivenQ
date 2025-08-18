//! Backtesting Service Main Entry Point
//! 
//! gRPC server for backtesting strategies

use anyhow::Result;
use backtesting::{BacktestEngine, BacktestConfig};
use services_common::proto::{
    BacktestingService as BacktestingTrait, BacktestingServiceServer,
    RunBacktestRequest, RunBacktestResponse,
    GetBacktestStatusRequest, GetBacktestStatusResponse,
    GetBacktestResultsRequest, GetBacktestResultsResponse,
    StopBacktestRequest, StopBacktestResponse,
    ListBacktestsRequest, ListBacktestsResponse,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{info, error};

pub struct BacktestingService {
    engines: Arc<RwLock<std::collections::HashMap<String, Arc<BacktestEngine>>>>,
}

impl BacktestingService {
    pub fn new() -> Self {
        Self {
            engines: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }
}

#[tonic::async_trait]
impl BacktestingTrait for BacktestingService {
    async fn run_backtest(
        &self,
        request: Request<RunBacktestRequest>,
    ) -> Result<Response<RunBacktestResponse>, Status> {
        let req = request.into_inner();
        
        info!("Starting backtest: {}", req.backtest_id);
        
        // Parse configuration
        let config: BacktestConfig = serde_json::from_str(&req.config)
            .map_err(|e| Status::invalid_argument(format!("Invalid config: {}", e)))?;
        
        // Create engine
        let engine = Arc::new(BacktestEngine::new(config));
        
        // Store engine
        {
            let mut engines = self.engines.write().await;
            engines.insert(req.backtest_id.clone(), engine.clone());
        }
        
        // TODO: Load market data from data service
        // TODO: Parse and load strategy
        // TODO: Run backtest asynchronously
        
        Ok(Response::new(RunBacktestResponse {
            backtest_id: req.backtest_id,
            status: "STARTED".to_string(),
            message: "Backtest started successfully".to_string(),
        }))
    }
    
    async fn get_backtest_status(
        &self,
        request: Request<GetBacktestStatusRequest>,
    ) -> Result<Response<GetBacktestStatusResponse>, Status> {
        let req = request.into_inner();
        
        let engines = self.engines.read().await;
        
        if let Some(_engine) = engines.get(&req.backtest_id) {
            Ok(Response::new(GetBacktestStatusResponse {
                backtest_id: req.backtest_id,
                status: "RUNNING".to_string(),
                progress_pct: 50.0,
                message: "Backtest in progress".to_string(),
                started_at: None,
                updated_at: None,
            }))
        } else {
            Err(Status::not_found(format!("Backtest {} not found", req.backtest_id)))
        }
    }
    
    async fn get_backtest_results(
        &self,
        request: Request<GetBacktestResultsRequest>,
    ) -> Result<Response<GetBacktestResultsResponse>, Status> {
        let req = request.into_inner();
        
        info!("Getting results for backtest: {}", req.backtest_id);
        
        let engines = self.engines.read().await;
        
        if engines.get(&req.backtest_id).is_none() {
            error!("Backtest {} not found when retrieving results", req.backtest_id);
        }
        
        Err(Status::unimplemented("Get results not yet implemented"))
    }
    
    async fn stop_backtest(
        &self,
        request: Request<StopBacktestRequest>,
    ) -> Result<Response<StopBacktestResponse>, Status> {
        let req = request.into_inner();
        
        info!("Stopping backtest: {}", req.backtest_id);
        
        let mut engines = self.engines.write().await;
        
        if engines.remove(&req.backtest_id).is_some() {
            Ok(Response::new(StopBacktestResponse {
                backtest_id: req.backtest_id,
                success: true,
                message: "Backtest stopped successfully".to_string(),
            }))
        } else {
            Ok(Response::new(StopBacktestResponse {
                backtest_id: req.backtest_id,
                success: false,
                message: "Backtest not found".to_string(),
            }))
        }
    }
    
    async fn list_backtests(
        &self,
        request: Request<ListBacktestsRequest>,
    ) -> Result<Response<ListBacktestsResponse>, Status> {
        let _req = request.into_inner();
        
        let engines = self.engines.read().await;
        let total = engines.len() as u32;
        
        Ok(Response::new(ListBacktestsResponse {
            backtests: vec![],
            total,
        }))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();
    
    info!("Starting Backtesting Service");
    
    let addr = "[::1]:50060".parse()?;
    let service = BacktestingService::new();
    
    info!("Backtesting service listening on {}", addr);
    
    Server::builder()
        .add_service(BacktestingServiceServer::new(service))
        .serve(addr)
        .await?;
    
    Ok(())
}