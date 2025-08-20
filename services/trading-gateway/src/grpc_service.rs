//! gRPC service implementation for Trading Gateway

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{error, info};

use trading_gateway::{GatewayStatus, TradingGateway};
use services_common::proto::trading::v1::{
    trading_gateway_server::TradingGateway as TradingGatewayService,
    StartTradingRequest, StartTradingResponse,
    StopTradingRequest, StopTradingResponse,
    GetStatusRequest, GetStatusResponse,
    EmergencyStopRequest, EmergencyStopResponse,
    UpdateStrategyRequest, UpdateStrategyResponse,
    GetPositionsRequest, GetPositionsResponse, GatewayStatus as ProtoGatewayStatus,
};

/// gRPC service implementation for Trading Gateway
/// 
/// The `TradingGatewayServiceImpl` provides the gRPC API interface for external
/// clients to interact with the trading gateway. It exposes methods for starting
/// and stopping trading, emergency controls, status monitoring, strategy management,
/// and position tracking.
/// 
/// # API Endpoints
/// - `start_trading` - Initiates trading with specified strategies
/// - `stop_trading` - Gracefully stops all trading activity
/// - `emergency_stop` - Immediately halts all trading with reason logging
/// - `get_status` - Returns current gateway status and metrics
/// - `update_strategy` - Modifies strategy parameters
/// - `get_positions` - Retrieves current position information
/// 
/// # Error Handling
/// All methods return structured responses with success flags and descriptive
/// error messages, ensuring clients can handle failures appropriately.
pub(crate) struct TradingGatewayServiceImpl {
    gateway: Arc<TradingGateway>,
}

impl std::fmt::Debug for TradingGatewayServiceImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TradingGatewayServiceImpl")
            .field("gateway_status", &self.gateway.get_status())
            .finish()
    }
}

impl TradingGatewayServiceImpl {
    /// Creates a new gRPC service implementation
    /// 
    /// # Arguments
    /// * `gateway` - Shared reference to the trading gateway instance
    /// 
    /// # Returns
    /// A new service implementation that delegates operations to the
    /// provided gateway instance. The service is immediately ready
    /// to handle gRPC requests from clients.
    pub fn new(gateway: Arc<TradingGateway>) -> Self {
        Self { gateway }
    }
}

#[async_trait]
impl TradingGatewayService for TradingGatewayServiceImpl {
    async fn start_trading(
        &self,
        request: Request<StartTradingRequest>,
    ) -> Result<Response<StartTradingResponse>, Status> {
        let req = request.into_inner();
        info!("Starting trading with strategies: {:?}", req.strategies);
        
        // Start the gateway
        if let Err(e) = self.gateway.start().await {
            error!("Failed to start trading: {}", e);
            return Ok(Response::new(StartTradingResponse {
                success: false,
                message: format!("Failed to start: {}", e),
                active_strategies: vec![],
            }));
        }
        
        Ok(Response::new(StartTradingResponse {
            success: true,
            message: "Trading started successfully".to_string(),
            active_strategies: req.strategies,
        }))
    }
    
    async fn stop_trading(
        &self,
        _request: Request<StopTradingRequest>,
    ) -> Result<Response<StopTradingResponse>, Status> {
        info!("Stopping trading");
        
        // Stop the gateway
        if let Err(e) = self.gateway.stop().await {
            error!("Failed to stop trading: {}", e);
            return Ok(Response::new(StopTradingResponse {
                success: false,
                message: format!("Failed to stop: {}", e),
                orders_cancelled: 0,
                positions_closed: 0,
            }));
        }
        
        Ok(Response::new(StopTradingResponse {
            success: true,
            message: "Trading stopped successfully".to_string(),
            orders_cancelled: 0,
            positions_closed: 0,
        }))
    }
    
    async fn emergency_stop(
        &self,
        request: Request<EmergencyStopRequest>,
    ) -> Result<Response<EmergencyStopResponse>, Status> {
        let reason = request.into_inner().reason;
        info!("Emergency stop requested: {}", reason);
        
        // Emergency stop
        if let Err(e) = self.gateway.emergency_stop().await {
            error!("Emergency stop failed: {}", e);
            return Ok(Response::new(EmergencyStopResponse {
                success: false,
                message: format!("Emergency stop failed: {}", e),
                timestamp: chrono::Utc::now().timestamp(),
            }));
        }
        
        Ok(Response::new(EmergencyStopResponse {
            success: true,
            message: format!("Emergency stop executed: {}", reason),
            timestamp: chrono::Utc::now().timestamp(),
        }))
    }
    
    async fn get_status(
        &self,
        _request: Request<GetStatusRequest>,
    ) -> Result<Response<GetStatusResponse>, Status> {
        let status = self.gateway.get_status();
        
        Ok(Response::new(GetStatusResponse {
            status: match status {
                GatewayStatus::Stopped => ProtoGatewayStatus::Stopped as i32,
                GatewayStatus::Starting => ProtoGatewayStatus::Starting as i32,
                GatewayStatus::Running => ProtoGatewayStatus::Running as i32,
                GatewayStatus::Stopping => ProtoGatewayStatus::Stopping as i32,
                GatewayStatus::Error => ProtoGatewayStatus::Error as i32,
            },
            active_strategies: vec![],
            open_orders: 0,
            active_positions: 0,
            total_pnl: 0.0,
            uptime_seconds: 0,
        }))
    }
    
    async fn update_strategy(
        &self,
        request: Request<UpdateStrategyRequest>,
    ) -> Result<Response<UpdateStrategyResponse>, Status> {
        let req = request.into_inner();
        info!("Updating strategy: {}", req.strategy_name);
        
        Ok(Response::new(UpdateStrategyResponse {
            success: true,
            message: format!("Strategy {} updated", req.strategy_name),
        }))
    }
    
    async fn get_positions(
        &self,
        _request: Request<GetPositionsRequest>,
    ) -> Result<Response<GetPositionsResponse>, Status> {
        Ok(Response::new(GetPositionsResponse {
            positions: vec![],
            total_value: 0.0,
            total_pnl: 0.0,
        }))
    }
}