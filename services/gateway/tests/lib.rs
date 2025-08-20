//! Test library for API Gateway
//!
//! Common test utilities, fixtures, and helpers used across all test suites.

#![cfg(test)]

pub mod unit;

use anyhow::Result;
use api_gateway::GatewayConfig;
use std::sync::Once;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Ensure tracing is initialized only once across all tests
static INIT: Once = Once::new();

/// Initialize test environment
pub fn init_test_env() {
    INIT.call_once(|| {
        // Initialize tracing for tests
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "gateway=debug,tower_http=debug".into()),
            )
            .with(tracing_subscriber::fmt::layer().with_test_writer())
            .init();

        // Set test-friendly environment variables
        std::env::set_var("RUST_BACKTRACE", "1");
        std::env::set_var("RUST_LOG", "debug");
    });
}

/// Create a test configuration suitable for testing
pub fn create_test_gateway_config() -> GatewayConfig {
    GatewayConfig {
        server: api_gateway::config::ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0, // Random port for tests
            timeout_seconds: 30,
            max_body_size: 1024 * 1024,
            compression: false, // Disable for deterministic testing
            tls: None,
        },
        services: api_gateway::config::ServiceEndpoints {
            auth_service: "http://127.0.0.1:50051".to_string(),
            execution_service: "http://127.0.0.1:50052".to_string(),
            market_data_service: "http://127.0.0.1:50053".to_string(),
            risk_service: "http://127.0.0.1:50054".to_string(),
            portfolio_service: Some("http://127.0.0.1:50055".to_string()),
            reporting_service: Some("http://127.0.0.1:50056".to_string()),
        },
        auth: api_gateway::config::AuthConfig {
            jwt_secret: "test-jwt-secret-key-for-testing-only".to_string(),
            token_expiry_seconds: 3600,
            refresh_token_expiry_seconds: 86400 * 7,
            allowed_algorithms: vec!["HS256".to_string()],
        },
        rate_limiting: api_gateway::config::RateLimitConfig {
            enabled: true,
            requests_per_minute: 1000, // High limit for tests
            burst_size: 100,
            endpoint_limits: rustc_hash::FxHashMap::default(),
        },
        cors: api_gateway::config::CorsConfig {
            enabled: true,
            allowed_origins: vec!["*".to_string()],
            allowed_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
                "OPTIONS".to_string(),
            ],
            allowed_headers: vec![
                "Authorization".to_string(),
                "Content-Type".to_string(),
                "X-Requested-With".to_string(),
            ],
            allow_credentials: true,
            max_age_seconds: 86400,
        },
        monitoring: api_gateway::config::MonitoringConfig {
            metrics_enabled: true,
            metrics_path: "/metrics".to_string(),
            tracing_enabled: false, // Disable for tests to avoid noise
            health_path: "/health".to_string(),
        },
    }
}

/// Test utilities for mocking external dependencies
pub mod mocks {
    use std::sync::Arc;
    use tonic::{Response, Status};
    
    /// Mock gRPC response builder
    pub fn mock_grpc_success<T>(data: T) -> Result<Response<T>, Status> {
        Ok(Response::new(data))
    }

    /// Mock gRPC error builder
    pub fn mock_grpc_error<T>(code: tonic::Code, message: &str) -> Result<Response<T>, Status> {
        Err(Status::new(code, message))
    }
}

/// Performance testing utilities
pub mod performance {
    use std::time::{Duration, Instant};

    /// Measure execution time of an async operation
    pub async fn time_async_operation<F, Fut, T>(operation: F) -> (T, Duration)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let start = Instant::now();
        let result = operation().await;
        let duration = start.elapsed();
        (result, duration)
    }

    /// Assert that an operation completes within expected time
    pub fn assert_performance<T>(
        result_and_duration: (T, Duration),
        max_expected: Duration,
        operation_name: &str,
    ) -> T {
        let (result, actual_duration) = result_and_duration;
        assert!(
            actual_duration <= max_expected,
            "{} took {:?}, expected <= {:?}",
            operation_name,
            actual_duration,
            max_expected
        );
        result
    }

    /// Performance test configuration
    #[derive(Debug, Clone)]
    pub struct PerfTestConfig {
        pub max_latency_p95: Duration,
        pub min_throughput_rps: f64,
        pub test_duration: Duration,
        pub concurrent_users: usize,
    }

    impl Default for PerfTestConfig {
        fn default() -> Self {
            Self {
                max_latency_p95: Duration::from_millis(10), // 10ms for 95th percentile
                min_throughput_rps: 1000.0,                 // 1000 RPS minimum
                test_duration: Duration::from_secs(30),     // 30 second test
                concurrent_users: 100,                      // 100 concurrent users
            }
        }
    }
}

/// Load testing utilities
pub mod load_testing {
    use futures::future::join_all;
    use std::{sync::Arc, time::Duration};
    use tokio::sync::Semaphore;

    /// Load test configuration
    #[derive(Debug, Clone)]
    pub struct LoadTestConfig {
        pub concurrent_users: usize,
        pub requests_per_user: usize,
        pub ramp_up_duration: Duration,
        pub steady_state_duration: Duration,
        pub ramp_down_duration: Duration,
    }

    impl Default for LoadTestConfig {
        fn default() -> Self {
            Self {
                concurrent_users: 50,
                requests_per_user: 100,
                ramp_up_duration: Duration::from_secs(10),
                steady_state_duration: Duration::from_secs(60),
                ramp_down_duration: Duration::from_secs(10),
            }
        }
    }

    /// Execute a load test with the given configuration
    pub async fn execute_load_test<F, Fut, T>(
        config: LoadTestConfig,
        operation: F,
    ) -> Vec<T>
    where
        F: Fn(usize) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = T> + Send,
        T: Send + 'static,
    {
        let semaphore = Arc::new(Semaphore::new(config.concurrent_users));
        let operation = Arc::new(operation);

        let total_requests = config.concurrent_users * config.requests_per_user;
        let tasks = (0..total_requests).map(|i| {
            let sem = Arc::clone(&semaphore);
            let op = Arc::clone(&operation);

            tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                op(i).await
            })
        });

        let results = join_all(tasks).await;
        results.into_iter().filter_map(|r| r.ok()).collect()
    }
}

/// Assertion utilities for testing
pub mod assertions {
    use api_gateway::models::{ApiResponse, ErrorResponse};
    use serde_json::Value;

    /// Assert that an API response is successful
    pub fn assert_api_success<T>(response: &ApiResponse<T>) {
        assert!(response.success, "API response should be successful");
        assert!(response.data.is_some(), "Successful response should have data");
        assert!(response.error.is_none(), "Successful response should not have error");
        assert!(response.timestamp > 0, "Response should have valid timestamp");
    }

    /// Assert that an API response is an error
    pub fn assert_api_error<T>(response: &ApiResponse<T>, expected_error_code: &str) {
        assert!(!response.success, "API response should be unsuccessful");
        assert!(response.data.is_none(), "Error response should not have data");
        assert!(response.error.is_some(), "Error response should have error");
        
        let error = response.error.as_ref().unwrap();
        assert_eq!(error.error, expected_error_code, "Error code should match expected");
        assert!(!error.message.is_empty(), "Error should have message");
        assert!(response.timestamp > 0, "Response should have valid timestamp");
    }

    /// Assert that JSON contains expected fields
    pub fn assert_json_has_fields(json: &Value, fields: &[&str]) {
        for field in fields {
            assert!(json.get(field).is_some(), "JSON should contain field: {}", field);
        }
    }

    /// Assert response time is within acceptable limits
    pub fn assert_response_time_acceptable(
        duration: std::time::Duration,
        max_acceptable: std::time::Duration,
    ) {
        assert!(
            duration <= max_acceptable,
            "Response time {:?} exceeds acceptable limit {:?}",
            duration,
            max_acceptable
        );
    }
}

/// Test data factories
pub mod factories {
    use api_gateway::models::*;
    use chrono::Utc;

    /// Create a test login request
    pub fn create_login_request(username: &str) -> LoginRequest {
        LoginRequest {
            username: username.to_string(),
            password: format!("{}_password", username),
            exchange: Some("ZERODHA".to_string()),
        }
    }

    /// Create a test order request
    pub fn create_order_request(symbol: &str, side: &str) -> SubmitOrderRequest {
        SubmitOrderRequest {
            client_order_id: Some(format!("TEST_{}", uuid::Uuid::new_v4())),
            symbol: symbol.to_string(),
            side: side.to_string(),
            quantity: "100.0000".to_string(),
            order_type: "LIMIT".to_string(),
            limit_price: Some("150.2500".to_string()),
            stop_price: None,
            time_in_force: Some("GTC".to_string()),
            venue: Some("NSE".to_string()),
            strategy_id: Some("test_strategy".to_string()),
            params: None,
        }
    }

    /// Create a test WebSocket message
    pub fn create_websocket_message(msg_type: &str) -> WebSocketMessage {
        WebSocketMessage {
            message_type: msg_type.to_string(),
            data: serde_json::json!({
                "test": true,
                "timestamp": Utc::now().timestamp()
            }),
            timestamp: Utc::now().timestamp(),
        }
    }

    /// Create a test error response
    pub fn create_error_response(error_code: &str, message: &str) -> ErrorResponse {
        ErrorResponse {
            error: error_code.to_string(),
            message: message.to_string(),
            details: None,
        }
    }
}