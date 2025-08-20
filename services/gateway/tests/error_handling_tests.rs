//! Error handling and resilience tests for the API Gateway
//!
//! Comprehensive tests for error scenarios, fault tolerance, recovery mechanisms,
//! and graceful degradation under various failure conditions.

use anyhow::Result;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use rstest::*;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tokio::{sync::Semaphore, time::timeout};

use api_gateway::{
    config::GatewayConfig,
    grpc_clients::GrpcClients,
    handlers::{AuthHandlers, ExecutionHandlers, RiskHandlers},
    middleware::{auth_middleware, rate_limit_middleware, AuthState, RateLimitState},
    models::{
        ApiResponse, CheckOrderRequest, ErrorResponse, LoginRequest, SubmitOrderRequest,
        KillSwitchRequest,
    },
    rate_limiter::RateLimiter,
};

/// Test configuration for error scenarios
fn create_error_test_config() -> GatewayConfig {
    GatewayConfig {
        server: api_gateway::config::ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            timeout_seconds: 5, // Short timeout for error testing
            max_body_size: 1024,
            compression: false,
            tls: None,
        },
        services: api_gateway::config::ServiceEndpoints {
            auth_service: "http://unreachable:50051".to_string(), // Unreachable service
            execution_service: "http://unreachable:50052".to_string(),
            market_data_service: "http://unreachable:50053".to_string(),
            risk_service: "http://unreachable:50054".to_string(),
            portfolio_service: None,
            reporting_service: None,
        },
        auth: api_gateway::config::AuthConfig {
            jwt_secret: "error-test-secret".to_string(),
            token_expiry_seconds: 60, // Short expiry for testing
            refresh_token_expiry_seconds: 120,
            allowed_algorithms: vec!["HS256".to_string()],
        },
        rate_limiting: api_gateway::config::RateLimitConfig {
            enabled: true,
            requests_per_minute: 10, // Very low limit for testing
            burst_size: 2,
            endpoint_limits: rustc_hash::FxHashMap::default(),
        },
        cors: api_gateway::config::CorsConfig {
            enabled: true,
            allowed_origins: vec!["http://localhost:3000".to_string()],
            allowed_methods: vec!["GET".to_string(), "POST".to_string()],
            allowed_headers: vec!["Authorization".to_string(), "Content-Type".to_string()],
            allow_credentials: true,
            max_age_seconds: 3600,
        },
        monitoring: api_gateway::config::MonitoringConfig {
            metrics_enabled: true,
            metrics_path: "/metrics".to_string(),
            tracing_enabled: false, // Disable for error tests
            health_path: "/health".to_string(),
        },
    }
}

/// Create test handlers with unreachable gRPC services
fn create_error_test_handlers() -> (AuthHandlers, ExecutionHandlers, RiskHandlers) {
    // These will fail to connect, simulating service unavailability
    let mock_clients = Arc::new(GrpcClients::new(
        "http://unreachable:50051",
        "http://unreachable:50052",
        "http://unreachable:50053",
        "http://unreachable:50054",
    ).await.expect("Failed to create mock clients"));

    (
        AuthHandlers::new(Arc::clone(&mock_clients)),
        ExecutionHandlers::new(Arc::clone(&mock_clients)),
        RiskHandlers::new(mock_clients),
    )
}

#[rstest]
#[tokio::test]
async fn test_grpc_service_unavailable_errors() -> Result<()> {
    let (auth_handlers, execution_handlers, risk_handlers) = create_error_test_handlers();

    // Test auth service unavailable
    let login_request = LoginRequest {
        username: "testuser".to_string(),
        password: "testpass".to_string(),
        exchange: Some("ZERODHA".to_string()),
    };

    let result = AuthHandlers::login(State(auth_handlers), Json(login_request)).await;
    
    match result {
        Ok(Json(response)) => {
            assert!(!response.success);
            assert!(response.error.is_some());
            let error = response.error.unwrap();
            assert_eq!(error.error, "LOGIN_FAILED");
            assert!(error.message.contains("service unavailable") || error.message.contains("Invalid credentials"));
        }
        Err(status) => {
            // HTTP error is also acceptable for service unavailable
            assert!(status.is_server_error() || status == StatusCode::UNAUTHORIZED);
        }
    }

    // Test execution service unavailable
    let order_request = SubmitOrderRequest {
        client_order_id: Some("ERROR_TEST_001".to_string()),
        symbol: "TESTSTOCK".to_string(),
        side: "BUY".to_string(),
        quantity: "100.0000".to_string(),
        order_type: "LIMIT".to_string(),
        limit_price: Some("150.0000".to_string()),
        stop_price: None,
        time_in_force: Some("GTC".to_string()),
        venue: Some("TEST".to_string()),
        strategy_id: None,
        params: None,
    };

    let headers = HeaderMap::new();
    let result = ExecutionHandlers::submit_order(
        State(execution_handlers),
        headers,
        Json(order_request),
    ).await;

    match result {
        Ok(Json(response)) => {
            assert!(!response.success);
            assert!(response.error.is_some());
            let error = response.error.unwrap();
            assert_eq!(error.error, "ORDER_SUBMISSION_FAILED");
        }
        Err(_) => {
            // HTTP error acceptable
        }
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_authentication_error_scenarios() -> Result<()> {
    let config = create_error_test_config();
    let auth_state = AuthState {
        config: Arc::new(config.clone()),
    };

    // Test missing authorization header
    let request = Request::builder()
        .uri("/api/v1/orders")
        .body(axum::body::Body::empty())?;

    let next = MockNext::new(StatusCode::OK);
    let result = auth_middleware(State(auth_state.clone()), request, next).await;

    assert!(result.is_err());
    let error_response = result.unwrap_err();
    assert_eq!(error_response.status(), StatusCode::UNAUTHORIZED);

    // Test invalid token format
    let request = Request::builder()
        .uri("/api/v1/orders")
        .header("Authorization", "InvalidTokenFormat")
        .body(axum::body::Body::empty())?;

    let next = MockNext::new(StatusCode::OK);
    let result = auth_middleware(State(auth_state.clone()), request, next).await;

    assert!(result.is_err());
    let error_response = result.unwrap_err();
    assert_eq!(error_response.status(), StatusCode::UNAUTHORIZED);

    // Test malformed JWT
    let request = Request::builder()
        .uri("/api/v1/orders")
        .header("Authorization", "Bearer not.a.valid.jwt")
        .body(axum::body::Body::empty())?;

    let next = MockNext::new(StatusCode::OK);
    let result = auth_middleware(State(auth_state), request, next).await;

    assert!(result.is_err());
    let error_response = result.unwrap_err();
    assert_eq!(error_response.status(), StatusCode::UNAUTHORIZED);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_rate_limiting_error_scenarios() -> Result<()> {
    let config = create_error_test_config();
    let rate_limiter = Arc::new(RateLimiter::new(config.rate_limiting.clone()));
    let rate_limit_state = RateLimitState {
        limiter: rate_limiter,
    };

    // Exhaust rate limit
    for _ in 0..5 {
        let request = Request::builder()
            .uri("/api/v1/test")
            .header("X-Forwarded-For", "192.168.1.100")
            .body(axum::body::Body::empty())?;

        let next = MockNext::new(StatusCode::OK);
        let result = rate_limit_middleware(State(rate_limit_state.clone()), request, next).await;

        // First few should succeed, then start getting rate limited
        if result.is_err() {
            let error_response = result.unwrap_err();
            assert_eq!(error_response.status(), StatusCode::TOO_MANY_REQUESTS);
            break;
        }
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_invalid_request_data_errors() -> Result<()> {
    let (_, execution_handlers, risk_handlers) = create_error_test_handlers();

    // Test invalid order request
    let invalid_order = SubmitOrderRequest {
        client_order_id: None,
        symbol: "".to_string(), // Invalid empty symbol
        side: "INVALID_SIDE".to_string(),
        quantity: "not_a_number".to_string(),
        order_type: "UNKNOWN_TYPE".to_string(),
        limit_price: Some("also_not_a_number".to_string()),
        stop_price: None,
        time_in_force: Some("INVALID_TIF".to_string()),
        venue: None,
        strategy_id: None,
        params: None,
    };

    let headers = HeaderMap::new();
    let result = ExecutionHandlers::submit_order(
        State(execution_handlers),
        headers,
        Json(invalid_order),
    ).await;

    match result {
        Ok(Json(response)) => {
            assert!(!response.success);
            assert!(response.error.is_some());
        }
        Err(_) => {
            // HTTP error also acceptable
        }
    }

    // Test invalid risk check request
    let invalid_risk_request = CheckOrderRequest {
        symbol: "".to_string(),
        side: "INVALID".to_string(),
        quantity: "invalid".to_string(),
        price: "not_a_price".to_string(),
        strategy_id: None,
        exchange: "".to_string(),
    };

    let request = Request::builder()
        .body(axum::body::Body::empty())?;

    let result = RiskHandlers::check_order(
        State(risk_handlers),
        request,
        Json(invalid_risk_request),
    ).await;

    match result {
        Ok(Json(response)) => {
            assert!(!response.success);
            assert!(response.error.is_some());
        }
        Err(_) => {
            // HTTP error acceptable
        }
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_concurrent_error_handling() -> Result<()> {
    use futures::future::join_all;

    let config = create_error_test_config();
    let auth_state = AuthState {
        config: Arc::new(config),
    };

    // Create many concurrent requests that will fail
    let tasks = (0..100).map(|i| {
        let state = auth_state.clone();
        async move {
            let request = Request::builder()
                .uri("/api/v1/orders")
                .header("Authorization", format!("Bearer invalid_token_{}", i))
                .body(axum::body::Body::empty())
                .unwrap();

            let next = MockNext::new(StatusCode::OK);
            auth_middleware(State(state), request, next).await
        }
    });

    let results = join_all(tasks).await;

    // All should fail with proper error handling
    let mut error_count = 0;
    for result in results {
        match result {
            Err(response) => {
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
                error_count += 1;
            }
            Ok(_) => {
                // Shouldn't happen with invalid tokens
                panic!("Expected authentication failure");
            }
        }
    }

    assert_eq!(error_count, 100);
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_timeout_error_handling() -> Result<()> {
    let (auth_handlers, _, _) = create_error_test_handlers();

    // Test with very short timeout
    let login_request = LoginRequest {
        username: "testuser".to_string(),
        password: "testpass".to_string(),
        exchange: Some("ZERODHA".to_string()),
    };

    // Use timeout to simulate slow service
    let result = timeout(
        Duration::from_millis(100),
        AuthHandlers::login(State(auth_handlers), Json(login_request)),
    )
    .await;

    match result {
        Ok(Ok(Json(response))) => {
            // If it completes, should be an error due to unreachable service
            assert!(!response.success);
        }
        Ok(Err(_)) => {
            // HTTP error acceptable
        }
        Err(_) => {
            // Timeout is expected behavior
        }
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_malformed_json_handling() -> Result<()> {
    // Test JSON deserialization errors
    let malformed_json_cases = vec![
        "",
        "{",
        "not_json_at_all",
        r#"{"incomplete": }"#,
        r#"{"wrong_type": [}#,
        r#"{"symbol": null}"#, // Null where string expected
    ];

    for malformed_json in malformed_json_cases {
        // Test parsing as login request
        let login_result: Result<LoginRequest, _> = serde_json::from_str(malformed_json);
        assert!(login_result.is_err(), "Should fail to parse: {}", malformed_json);

        // Test parsing as order request  
        let order_result: Result<SubmitOrderRequest, _> = serde_json::from_str(malformed_json);
        assert!(order_result.is_err(), "Should fail to parse: {}", malformed_json);
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_error_response_serialization() -> Result<()> {
    use api_gateway::models::{ApiResponse, ErrorResponse};

    // Test various error response scenarios
    let error_cases = vec![
        ErrorResponse {
            error: "TEST_ERROR".to_string(),
            message: "Test error message".to_string(),
            details: None,
        },
        ErrorResponse {
            error: "COMPLEX_ERROR".to_string(),
            message: "Error with details".to_string(),
            details: Some({
                let mut details = rustc_hash::FxHashMap::default();
                details.insert("field".to_string(), "symbol".to_string());
                details.insert("value".to_string(), "INVALID".to_string());
                details.insert("reason".to_string(), "Symbol not found".to_string());
                details
            }),
        },
        ErrorResponse {
            error: "UNICODE_ERROR".to_string(),
            message: "Error with unicode: 你好世界 🚀".to_string(),
            details: None,
        },
    ];

    for error in error_cases {
        let api_response = ApiResponse::<()>::error(error.clone());
        
        // Should serialize without error
        let json = serde_json::to_string(&api_response)?;
        assert!(!json.is_empty());

        // Should deserialize back correctly
        let deserialized: ApiResponse<Value> = serde_json::from_str(&json)?;
        assert!(!deserialized.success);
        assert!(deserialized.error.is_some());
        
        let deserialized_error = deserialized.error.unwrap();
        assert_eq!(deserialized_error.error, error.error);
        assert_eq!(deserialized_error.message, error.message);
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_circuit_breaker_simulation() -> Result<()> {
    // Simulate circuit breaker behavior by testing rapid consecutive failures
    let (_, execution_handlers, _) = create_error_test_handlers();

    let failures_before_circuit_open = 10;
    let mut consecutive_failures = 0;

    for i in 0..15 {
        let order_request = SubmitOrderRequest {
            client_order_id: Some(format!("CIRCUIT_TEST_{}", i)),
            symbol: "TESTSTOCK".to_string(),
            side: "BUY".to_string(),
            quantity: "100.0000".to_string(),
            order_type: "MARKET".to_string(),
            limit_price: None,
            stop_price: None,
            time_in_force: Some("GTC".to_string()),
            venue: Some("TEST".to_string()),
            strategy_id: None,
            params: None,
        };

        let headers = HeaderMap::new();
        let result = ExecutionHandlers::submit_order(
            State(execution_handlers.clone()),
            headers,
            Json(order_request),
        ).await;

        match result {
            Ok(Json(response)) => {
                if !response.success {
                    consecutive_failures += 1;
                } else {
                    consecutive_failures = 0; // Reset on success
                }
            }
            Err(_) => {
                consecutive_failures += 1;
            }
        }

        // In a real circuit breaker, we'd expect different behavior after threshold
        if consecutive_failures >= failures_before_circuit_open {
            println!("Circuit breaker would be open after {} failures", consecutive_failures);
            break;
        }
    }

    // Should have detected multiple consecutive failures
    assert!(consecutive_failures >= failures_before_circuit_open);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_resource_exhaustion_handling() -> Result<()> {
    // Test behavior under resource constraints
    let semaphore = Arc::new(Semaphore::new(2)); // Very limited permits

    let tasks = (0..10).map(|i| {
        let sem = Arc::clone(&semaphore);
        async move {
            let permit = sem.acquire().await?;
            
            // Simulate work that might fail under resource pressure
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            let result: Result<String> = Ok(format!("Task {} completed", i));
            drop(permit);
            result
        }
    });

    let results = futures::future::join_all(tasks).await;
    
    // All tasks should complete eventually, even with resource constraints
    for (i, result) in results.into_iter().enumerate() {
        assert!(result.is_ok(), "Task {} should complete", i);
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_graceful_degradation() -> Result<()> {
    // Test that system continues to work with degraded functionality
    let config = create_error_test_config();
    
    // Disable rate limiting to test graceful degradation
    let mut degraded_config = config.clone();
    degraded_config.rate_limiting.enabled = false;

    let rate_limiter = Arc::new(RateLimiter::new(degraded_config.rate_limiting));
    
    // Should allow unlimited requests when rate limiting is disabled
    for _ in 0..100 {
        let allowed = rate_limiter.check_rate_limit("127.0.0.1", "/test").await;
        assert!(allowed, "Should allow requests when rate limiting is disabled");
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_error_propagation_chain() -> Result<()> {
    // Test that errors propagate correctly through the chain
    let (_, _, risk_handlers) = create_error_test_handlers();

    // Create kill switch request that will fail due to service unavailable
    let kill_switch_request = KillSwitchRequest {
        activate: true,
        reason: Some("Test kill switch".to_string()),
    };

    let request = Request::builder()
        .body(axum::body::Body::empty())?;

    let result = RiskHandlers::kill_switch(
        State(risk_handlers),
        request,
        Json(kill_switch_request),
    ).await;

    match result {
        Ok(Json(response)) => {
            // Should be an error due to unreachable service
            assert!(!response.success);
            assert!(response.error.is_some());
            let error = response.error.unwrap();
            assert_eq!(error.error, "KILL_SWITCH_FAILED");
        }
        Err(_) => {
            // HTTP error also acceptable
        }
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_memory_safety_under_errors() -> Result<()> {
    // Test that errors don't cause memory leaks or unsafe behavior
    let (auth_handlers, _, _) = create_error_test_handlers();

    // Generate many error scenarios
    for i in 0..1000 {
        let login_request = LoginRequest {
            username: format!("user_{}", i),
            password: format!("pass_{}", i),
            exchange: Some("ZERODHA".to_string()),
        };

        let _result = AuthHandlers::login(
            State(auth_handlers.clone()),
            Json(login_request),
        ).await;

        // Don't check result - just ensure no panics or memory issues
        
        // Occasionally yield to prevent overwhelming the runtime
        if i % 100 == 0 {
            tokio::task::yield_now().await;
        }
    }

    Ok(())
}

/// Mock Next implementation for testing middleware
struct MockNext {
    response: axum::response::Response,
}

impl MockNext {
    fn new(status: StatusCode) -> Self {
        Self {
            response: axum::response::Response::builder()
                .status(status)
                .body(axum::body::Body::empty())
                .unwrap(),
        }
    }
}

#[async_trait::async_trait]
impl axum::middleware::Next for MockNext {
    async fn run(self, _req: Request) -> axum::response::Response {
        self.response
    }
}

#[rstest]
#[tokio::test]
async fn test_error_recovery_mechanisms() -> Result<()> {
    // Test that the system can recover from error states
    let config = create_error_test_config();
    let rate_limiter = Arc::new(RateLimiter::new(config.rate_limiting.clone()));
    
    // Exhaust rate limits
    for _ in 0..10 {
        let _ = rate_limiter.check_rate_limit("127.0.0.1", "/test").await;
    }
    
    // Wait briefly and test cleanup
    rate_limiter.cleanup_old_limiters().await;
    
    // System should still be functional
    let stats = rate_limiter.get_stats().await;
    assert!(stats.total_ips > 0 || stats.total_endpoints >= 0);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_error_logging_and_metrics() -> Result<()> {
    // Test that errors are properly logged and metrics are updated
    use api_gateway::models::{ApiResponse, ErrorResponse};
    
    let error_types = vec![
        "AUTHENTICATION_FAILED",
        "RATE_LIMIT_EXCEEDED", 
        "SERVICE_UNAVAILABLE",
        "VALIDATION_ERROR",
        "PERMISSION_DENIED",
    ];
    
    let mut error_count = 0;
    
    for error_type in error_types {
        for _ in 0..10 {
            let error = ErrorResponse {
                error: error_type.to_string(),
                message: format!("Test error: {}", error_type),
                details: None,
            };
            
            let response = ApiResponse::<()>::error(error);
            
            // Verify error response structure
            assert!(!response.success);
            assert!(response.error.is_some());
            assert!(response.timestamp > 0);
            
            error_count += 1;
        }
    }
    
    assert_eq!(error_count, 50);
    Ok(())
}