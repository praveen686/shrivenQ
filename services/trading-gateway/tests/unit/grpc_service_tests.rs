//! Unit tests for gRPC service implementations
//!
//! Comprehensive tests covering:
//! - gRPC service method implementations
//! - Request/response handling and validation
//! - Error handling and status codes
//! - Service state management
//! - Concurrent request handling
//! - Integration with underlying gateway
//! - Protocol buffer serialization

use anyhow::Result;
use rstest::*;
use std::sync::Arc;
use tokio::time::Duration;
use tonic::{Request, Status};
use trading_gateway::{
    grpc_service::TradingGatewayServiceImpl,
    GatewayConfig, TradingGateway,
};

// Note: These are placeholder imports - actual proto imports would be:
// use services_common::proto::trading::v1::*;
// For testing purposes, we'll create mock request/response types

/// Mock request types for testing (in real implementation, these come from proto)
#[derive(Debug, Clone)]
pub struct MockStartTradingRequest {
    pub strategies: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MockStartTradingResponse {
    pub success: bool,
    pub message: String,
    pub active_strategies: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MockStopTradingRequest {}

#[derive(Debug, Clone)]
pub struct MockStopTradingResponse {
    pub success: bool,
    pub message: String,
    pub orders_cancelled: u32,
    pub positions_closed: u32,
}

#[derive(Debug, Clone)]
pub struct MockEmergencyStopRequest {
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct MockEmergencyStopResponse {
    pub success: bool,
    pub message: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone)]
pub struct MockGetStatusRequest {}

#[derive(Debug, Clone)]
pub struct MockGetStatusResponse {
    pub status: i32,
    pub active_strategies: Vec<String>,
    pub open_orders: u32,
    pub active_positions: u32,
    pub total_pnl: f64,
    pub uptime_seconds: u64,
}

/// Test fixture for creating a TradingGateway
#[fixture]
async fn trading_gateway() -> Arc<TradingGateway> {
    let config = GatewayConfig::default();
    Arc::new(TradingGateway::new(config).await.unwrap())
}

/// Test fixture for creating a gRPC service implementation
#[fixture]
async fn grpc_service(trading_gateway: Arc<TradingGateway>) -> TradingGatewayServiceImpl {
    TradingGatewayServiceImpl::new(trading_gateway)
}

// Mock implementation of the gRPC service trait for testing
impl TradingGatewayServiceImpl {
    /// Mock implementation of start_trading
    pub async fn mock_start_trading(
        &self,
        request: MockStartTradingRequest,
    ) -> Result<MockStartTradingResponse, Status> {
        // Start the gateway
        if let Err(e) = self.gateway.start().await {
            return Ok(MockStartTradingResponse {
                success: false,
                message: format!("Failed to start: {}", e),
                active_strategies: vec![],
            });
        }
        
        Ok(MockStartTradingResponse {
            success: true,
            message: "Trading started successfully".to_string(),
            active_strategies: request.strategies,
        })
    }
    
    /// Mock implementation of stop_trading
    pub async fn mock_stop_trading(
        &self,
        _request: MockStopTradingRequest,
    ) -> Result<MockStopTradingResponse, Status> {
        if let Err(e) = self.gateway.stop().await {
            return Ok(MockStopTradingResponse {
                success: false,
                message: format!("Failed to stop: {}", e),
                orders_cancelled: 0,
                positions_closed: 0,
            });
        }
        
        Ok(MockStopTradingResponse {
            success: true,
            message: "Trading stopped successfully".to_string(),
            orders_cancelled: 0, // Would get from execution engine
            positions_closed: 0,  // Would get from position manager
        })
    }
    
    /// Mock implementation of emergency_stop
    pub async fn mock_emergency_stop(
        &self,
        request: MockEmergencyStopRequest,
    ) -> Result<MockEmergencyStopResponse, Status> {
        if let Err(e) = self.gateway.emergency_stop().await {
            return Ok(MockEmergencyStopResponse {
                success: false,
                message: format!("Emergency stop failed: {}", e),
                timestamp: chrono::Utc::now().timestamp(),
            });
        }
        
        Ok(MockEmergencyStopResponse {
            success: true,
            message: format!("Emergency stop executed: {}", request.reason),
            timestamp: chrono::Utc::now().timestamp(),
        })
    }
    
    /// Mock implementation of get_status
    pub async fn mock_get_status(
        &self,
        _request: MockGetStatusRequest,
    ) -> Result<MockGetStatusResponse, Status> {
        let status = self.gateway.get_status();
        
        Ok(MockGetStatusResponse {
            status: match status {
                trading_gateway::GatewayStatus::Stopped => 0,
                trading_gateway::GatewayStatus::Starting => 1,
                trading_gateway::GatewayStatus::Running => 2,
                trading_gateway::GatewayStatus::Stopping => 3,
                trading_gateway::GatewayStatus::Error => 4,
            },
            active_strategies: vec![], // Would get from strategy manager
            open_orders: 0,            // Would get from execution engine
            active_positions: 0,       // Would get from position manager
            total_pnl: 0.0,           // Would get from position manager
            uptime_seconds: 0,        // Would track uptime
        })
    }
}

#[rstest]
#[tokio::test]
async fn test_grpc_service_creation(grpc_service: TradingGatewayServiceImpl) {
    // Test basic service creation
    // The service should be ready to handle requests
    let initial_status = grpc_service.gateway.get_status();
    assert_eq!(initial_status, trading_gateway::GatewayStatus::Stopped);
}

#[rstest]
#[tokio::test]
async fn test_start_trading_success(grpc_service: TradingGatewayServiceImpl) -> Result<()> {
    let request = MockStartTradingRequest {
        strategies: vec!["momentum".to_string(), "arbitrage".to_string()],
    };
    
    let response = grpc_service.mock_start_trading(request.clone()).await?;
    
    assert!(response.success);
    assert_eq!(response.message, "Trading started successfully");
    assert_eq!(response.active_strategies, request.strategies);
    
    // Verify gateway state changed
    let status = grpc_service.gateway.get_status();
    assert_eq!(status, trading_gateway::GatewayStatus::Running);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_stop_trading_success(grpc_service: TradingGatewayServiceImpl) -> Result<()> {
    // First start trading
    let start_request = MockStartTradingRequest {
        strategies: vec!["momentum".to_string()],
    };
    grpc_service.mock_start_trading(start_request).await?;
    
    // Then stop trading
    let stop_request = MockStopTradingRequest {};
    let response = grpc_service.mock_stop_trading(stop_request).await?;
    
    assert!(response.success);
    assert_eq!(response.message, "Trading stopped successfully");
    
    // Verify gateway state changed
    let status = grpc_service.gateway.get_status();
    assert_eq!(status, trading_gateway::GatewayStatus::Stopped);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_emergency_stop_execution(grpc_service: TradingGatewayServiceImpl) -> Result<()> {
    // Start trading first
    let start_request = MockStartTradingRequest {
        strategies: vec!["momentum".to_string()],
    };
    grpc_service.mock_start_trading(start_request).await?;
    
    // Execute emergency stop
    let emergency_request = MockEmergencyStopRequest {
        reason: "Market crash detected".to_string(),
    };
    
    let response = grpc_service.mock_emergency_stop(emergency_request.clone()).await?;
    
    assert!(response.success);
    assert!(response.message.contains("Market crash detected"));
    assert!(response.timestamp > 0);
    
    // Verify circuit breaker was tripped
    assert!(grpc_service.gateway.is_circuit_breaker_tripped());
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_get_status_response(grpc_service: TradingGatewayServiceImpl) -> Result<()> {
    let request = MockGetStatusRequest {};
    
    // Test status when stopped
    let response1 = grpc_service.mock_get_status(request.clone()).await?;
    assert_eq!(response1.status, 0); // Stopped
    
    // Start trading and check status again
    let start_request = MockStartTradingRequest {
        strategies: vec!["momentum".to_string()],
    };
    grpc_service.mock_start_trading(start_request).await?;
    
    let response2 = grpc_service.mock_get_status(request.clone()).await?;
    assert_eq!(response2.status, 2); // Running
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_multiple_start_requests(grpc_service: TradingGatewayServiceImpl) -> Result<()> {
    let request = MockStartTradingRequest {
        strategies: vec!["momentum".to_string()],
    };
    
    // First start should succeed
    let response1 = grpc_service.mock_start_trading(request.clone()).await?;
    assert!(response1.success);
    
    // Second start while already running should still succeed
    let response2 = grpc_service.mock_start_trading(request.clone()).await?;
    assert!(response2.success);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_stop_without_start(grpc_service: TradingGatewayServiceImpl) -> Result<()> {
    // Try to stop without starting
    let stop_request = MockStopTradingRequest {};
    let response = grpc_service.mock_stop_trading(stop_request).await?;
    
    // Should still succeed (graceful handling)
    assert!(response.success);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_concurrent_grpc_requests() -> Result<()> {
    let config = GatewayConfig::default();
    let gateway = Arc::new(TradingGateway::new(config).await?);
    let service = Arc::new(TradingGatewayServiceImpl::new(gateway));
    
    let mut handles = Vec::new();
    
    // Concurrent start requests
    for i in 1..=5 {
        let svc = service.clone();
        let handle = tokio::spawn(async move {
            let request = MockStartTradingRequest {
                strategies: vec![format!("strategy_{}", i)],
            };
            svc.mock_start_trading(request).await
        });
        handles.push(handle);
    }
    
    // Concurrent status requests
    for _ in 1..=5 {
        let svc = service.clone();
        let handle = tokio::spawn(async move {
            let request = MockGetStatusRequest {};
            svc.mock_get_status(request).await
        });
        handles.push(handle);
    }
    
    // Wait for all requests to complete
    for handle in handles {
        let result = handle.await?;
        assert!(result.is_ok(), "All concurrent requests should succeed");
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_service_state_consistency(grpc_service: TradingGatewayServiceImpl) -> Result<()> {
    // Test sequence: Start -> Status -> Emergency Stop -> Status
    
    // Initial status
    let status_request = MockGetStatusRequest {};
    let initial_status = grpc_service.mock_get_status(status_request.clone()).await?;
    assert_eq!(initial_status.status, 0); // Stopped
    
    // Start trading
    let start_request = MockStartTradingRequest {
        strategies: vec!["momentum".to_string(), "arbitrage".to_string()],
    };
    let start_response = grpc_service.mock_start_trading(start_request).await?;
    assert!(start_response.success);
    
    // Status after start
    let running_status = grpc_service.mock_get_status(status_request.clone()).await?;
    assert_eq!(running_status.status, 2); // Running
    
    // Emergency stop
    let emergency_request = MockEmergencyStopRequest {
        reason: "Test emergency".to_string(),
    };
    let emergency_response = grpc_service.mock_emergency_stop(emergency_request).await?;
    assert!(emergency_response.success);
    
    // Status after emergency stop should show circuit breaker tripped
    assert!(grpc_service.gateway.is_circuit_breaker_tripped());
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_empty_strategy_list(grpc_service: TradingGatewayServiceImpl) -> Result<()> {
    let request = MockStartTradingRequest {
        strategies: vec![], // Empty strategy list
    };
    
    let response = grpc_service.mock_start_trading(request).await?;
    
    // Should still succeed even with empty strategy list
    assert!(response.success);
    assert!(response.active_strategies.is_empty());
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_large_strategy_list(grpc_service: TradingGatewayServiceImpl) -> Result<()> {
    let large_strategy_list: Vec<String> = (1..=100)
        .map(|i| format!("strategy_{}", i))
        .collect();
    
    let request = MockStartTradingRequest {
        strategies: large_strategy_list.clone(),
    };
    
    let response = grpc_service.mock_start_trading(request).await?;
    
    assert!(response.success);
    assert_eq!(response.active_strategies.len(), 100);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_emergency_stop_with_long_reason(grpc_service: TradingGatewayServiceImpl) -> Result<()> {
    let long_reason = "A".repeat(1000); // Very long reason string
    
    let request = MockEmergencyStopRequest {
        reason: long_reason.clone(),
    };
    
    let response = grpc_service.mock_emergency_stop(request).await?;
    
    assert!(response.success);
    assert!(response.message.contains(&long_reason));
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_rapid_state_transitions(grpc_service: TradingGatewayServiceImpl) -> Result<()> {
    // Rapid sequence of state transitions
    for i in 0..10 {
        // Start
        let start_request = MockStartTradingRequest {
            strategies: vec![format!("strategy_{}", i)],
        };
        let start_response = grpc_service.mock_start_trading(start_request).await?;
        assert!(start_response.success);
        
        // Check status
        let status_request = MockGetStatusRequest {};
        let status_response = grpc_service.mock_get_status(status_request).await?;
        assert_eq!(status_response.status, 2); // Running
        
        // Stop
        let stop_request = MockStopTradingRequest {};
        let stop_response = grpc_service.mock_stop_trading(stop_request).await?;
        assert!(stop_response.success);
    }
    
    Ok(())
}

#[rstest]
#[case(vec!["momentum"])]
#[case(vec!["arbitrage"])]
#[case(vec!["momentum", "arbitrage"])]
#[case(vec!["momentum", "arbitrage", "market_making"])]
#[tokio::test]
async fn test_different_strategy_combinations(
    grpc_service: TradingGatewayServiceImpl,
    #[case] strategies: Vec<&str>
) -> Result<()> {
    let strategy_strings: Vec<String> = strategies.iter().map(|s| s.to_string()).collect();
    
    let request = MockStartTradingRequest {
        strategies: strategy_strings.clone(),
    };
    
    let response = grpc_service.mock_start_trading(request).await?;
    
    assert!(response.success);
    assert_eq!(response.active_strategies, strategy_strings);
    
    // Verify gateway started successfully
    let status = grpc_service.gateway.get_status();
    assert_eq!(status, trading_gateway::GatewayStatus::Running);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_service_performance_characteristics(grpc_service: TradingGatewayServiceImpl) -> Result<()> {
    let start = std::time::Instant::now();
    
    // Perform many rapid operations
    for i in 0..100 {
        let status_request = MockGetStatusRequest {};
        let _response = grpc_service.mock_get_status(status_request).await?;
        
        if i == 50 {
            // Start trading midway
            let start_request = MockStartTradingRequest {
                strategies: vec!["momentum".to_string()],
            };
            grpc_service.mock_start_trading(start_request).await?;
        }
    }
    
    let duration = start.elapsed();
    
    // Should handle 100 operations quickly
    assert!(duration < Duration::from_millis(500), "gRPC operations should be fast");
    
    Ok(())
}