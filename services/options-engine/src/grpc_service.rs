use anyhow::Result;
use tonic::{Request, Response, Status};
use std::sync::Arc;
use tokio::sync::RwLock;
use options_engine::{OptionsEngine, ExecutionMode, OptionType, BlackScholes};

#[derive(Debug)]
pub struct OptionsEngineService {
    pub engine: Arc<RwLock<OptionsEngine>>,
}

impl Default for OptionsEngineService {
    fn default() -> Self {
        Self::new()
    }
}

impl OptionsEngineService {
    pub fn new() -> Self {
        Self {
            engine: Arc::new(RwLock::new(OptionsEngine::new(
                ExecutionMode::Paper
            ))),
        }
    }
}

#[tonic::async_trait]
impl crate::pb::options_engine_server::OptionsEngine for OptionsEngineService {
    async fn calculate_price(
        &self,
        request: Request<crate::pb::PricingRequest>,
    ) -> Result<Response<crate::pb::PricingResponse>, Status> {
        let req = request.into_inner();
        
        // Log pricing request through the engine
        {
            let engine = self.engine.read().await;
            tracing::info!("Pricing request received - Engine mode: {:?}", engine.mode);
        }
        
        let option_type = match req.option_type {
            0 => OptionType::Call,
            1 => OptionType::Put,
            _ => return Err(Status::invalid_argument("Invalid option type")),
        };
        
        // Calculate price using Black-Scholes
        let price = BlackScholes::price(
            option_type,
            req.spot,
            req.strike,
            req.rate,
            req.volatility,
            req.time_to_expiry,
            0.0, // dividend yield
        );
        
        // Calculate Greeks
        let greeks = BlackScholes::calculate_greeks(
            option_type,
            req.spot,
            req.strike,
            req.rate,
            req.volatility,
            req.time_to_expiry,
            0.0, // dividend yield
        );
        
        let response = crate::pb::PricingResponse {
            price,
            greeks: Some(crate::pb::Greeks {
                delta: greeks.delta,
                gamma: greeks.gamma,
                theta: greeks.theta,
                vega: greeks.vega,
                rho: greeks.rho,
                lambda: greeks.lambda,
                vanna: greeks.vanna,
                charm: greeks.charm,
            }),
            implied_volatility: req.volatility,
        };
        
        Ok(Response::new(response))
    }
    
    async fn get_implied_volatility(
        &self,
        request: Request<crate::pb::PricingRequest>,
    ) -> Result<Response<crate::pb::PricingResponse>, Status> {
        let req = request.into_inner();
        
        let option_type = match req.option_type {
            0 => OptionType::Call,
            1 => OptionType::Put,
            _ => return Err(Status::invalid_argument("Invalid option type")),
        };
        
        // Calculate implied volatility
        let iv = BlackScholes::implied_volatility(
            option_type,
            req.spot,
            req.strike,
            req.rate,
            req.time_to_expiry,
            req.spot * 0.01, // Approximate premium as 1% of spot
            0.0, // dividend yield
        ).unwrap_or(0.2);
        
        let response = crate::pb::PricingResponse {
            price: 0.0,
            greeks: None,
            implied_volatility: iv,
        };
        
        Ok(Response::new(response))
    }
    
    async fn analyze_strategy(
        &self,
        request: Request<crate::pb::StrategyRequest>,
    ) -> Result<Response<crate::pb::StrategyResponse>, Status> {
        let req = request.into_inner();
        
        let response = crate::pb::StrategyResponse {
            strategy_name: req.strategy_type,
            max_profit: 10000.0,
            max_loss: -5000.0,
            breakeven_points: vec![req.spot * 1.02, req.spot * 0.98],
            aggregate_greeks: Some(crate::pb::Greeks::default()),
            margin_required: 50000.0,
        };
        
        Ok(Response::new(response))
    }
    
    async fn get_option_chain(
        &self,
        request: Request<crate::pb::OptionChainRequest>,
    ) -> Result<Response<crate::pb::OptionChainResponse>, Status> {
        let _req = request.into_inner();
        
        let response = crate::pb::OptionChainResponse {
            options: vec![],
            spot_price: 25000.0,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        
        Ok(Response::new(response))
    }
    
    async fn stream_greeks(
        &self,
        _request: Request<crate::pb::StreamGreeksRequest>,
    ) -> Result<Response<Self::StreamGreeksStream>, Status> {
        Err(Status::unimplemented("Streaming not yet implemented"))
    }
    
    type StreamGreeksStream = tokio_stream::wrappers::ReceiverStream<Result<crate::pb::Greeks, Status>>;
}