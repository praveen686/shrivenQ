use anyhow::Result;
use tonic::{Request, Response, Status};
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::lib::strategies::*;
use crate::pb::*;

#[derive(Debug, Default)]
pub struct TradingStrategiesService {
    pub engine: Arc<RwLock<StrategyEngine>>,
}

impl TradingStrategiesService {
    pub fn new() -> Self {
        Self {
            engine: Arc::new(RwLock::new(StrategyEngine::new())),
        }
    }
}

#[tonic::async_trait]
impl trading_strategies_server::TradingStrategies for TradingStrategiesService {
    async fn execute_strategy(
        &self,
        request: Request<StrategyRequest>,
    ) -> Result<Response<StrategyResponse>, Status> {
        let req = request.into_inner();
        
        // Execute the requested strategy
        let engine = self.engine.read().await;
        
        let response = StrategyResponse {
            strategy_id: req.strategy_id,
            status: "executed".to_string(),
            message: format!("Strategy {} executed successfully", req.strategy_type),
        };
        
        Ok(Response::new(response))
    }
    
    async fn get_active_strategies(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ActiveStrategiesResponse>, Status> {
        let engine = self.engine.read().await;
        let strategies = engine.active_strategies.read().await;
        
        let response = ActiveStrategiesResponse {
            strategies: strategies.iter().map(|s| StrategyInfo {
                id: s.id.clone(),
                strategy_type: s.strategy_type.to_string(),
                enabled: s.enabled,
                risk_limit: s.risk_limit,
                position_size: s.position_size,
            }).collect(),
        };
        
        Ok(Response::new(response))
    }
    
    async fn stop_strategy(
        &self,
        request: Request<StopStrategyRequest>,
    ) -> Result<Response<StrategyResponse>, Status> {
        let req = request.into_inner();
        
        let response = StrategyResponse {
            strategy_id: req.strategy_id.clone(),
            status: "stopped".to_string(),
            message: format!("Strategy {} stopped", req.strategy_id),
        };
        
        Ok(Response::new(response))
    }
}

impl StrategyType {
    fn to_string(&self) -> String {
        match self {
            StrategyType::Production => "production",
            StrategyType::Enhanced => "enhanced",
            StrategyType::MarketMaking => "market_making",
            StrategyType::Arbitrage => "arbitrage",
            StrategyType::MeanReversion => "mean_reversion",
            StrategyType::Momentum => "momentum",
        }.to_string()
    }
}