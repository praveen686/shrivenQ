//! Test helpers and utilities

use anyhow::Result;
use axum::http::{HeaderMap, HeaderValue};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, EncodingKey, Header};
use std::sync::Arc;
use tonic::{Request, Response, Status};

use api_gateway::{
    config::GatewayConfig,
    grpc_clients::GrpcClients,
    middleware::{Claims, UserContext},
    models::{LoginRequest, LoginResponse},
};

/// Test configuration factory
pub fn create_test_config() -> GatewayConfig {
    GatewayConfig {
        server: api_gateway::config::ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            timeout_seconds: 30,
            max_body_size: 1024 * 1024,
            compression: false,
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
            jwt_secret: "test-secret-key".to_string(),
            token_expiry_seconds: 3600,
            refresh_token_expiry_seconds: 86400 * 7,
            allowed_algorithms: vec!["HS256".to_string()],
        },
        rate_limiting: api_gateway::config::RateLimitConfig {
            enabled: true,
            requests_per_minute: 100,
            burst_size: 10,
            endpoint_limits: rustc_hash::FxHashMap::default(),
        },
        cors: api_gateway::config::CorsConfig {
            enabled: true,
            allowed_origins: vec!["*".to_string()],
            allowed_methods: vec!["GET".to_string(), "POST".to_string()],
            allowed_headers: vec!["Authorization".to_string(), "Content-Type".to_string()],
            allow_credentials: true,
            max_age_seconds: 3600,
        },
        monitoring: api_gateway::config::MonitoringConfig {
            metrics_enabled: true,
            metrics_path: "/metrics".to_string(),
            tracing_enabled: true,
            health_path: "/health".to_string(),
        },
    }
}

/// Create a valid JWT token for testing
pub fn create_test_jwt(user_id: &str, permissions: Vec<String>) -> Result<String> {
    let claims = Claims {
        sub: user_id.to_string(),
        exp: (Utc::now() + Duration::hours(1)).timestamp() as usize,
        iat: Utc::now().timestamp() as usize,
        permissions,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret("test-secret-key".as_ref()),
    )?;

    Ok(token)
}

/// Create test user context
pub fn create_test_user_context(user_id: &str, permissions: Vec<String>) -> UserContext {
    UserContext {
        user_id: user_id.to_string(),
        permissions,
        expires_at: (Utc::now() + Duration::hours(1)).timestamp() as usize,
    }
}

/// Create authenticated header map with Bearer token
pub fn create_auth_headers(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
    );
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    headers
}

/// Create test login request
pub fn create_test_login_request() -> LoginRequest {
    LoginRequest {
        username: "testuser".to_string(),
        password: "testpass".to_string(),
        exchange: Some("ZERODHA".to_string()),
    }
}

/// Create test login response
pub fn create_test_login_response() -> LoginResponse {
    LoginResponse {
        token: "test-jwt-token".to_string(),
        refresh_token: "test-refresh-token".to_string(),
        expires_at: (Utc::now() + Duration::hours(1)).timestamp(),
        permissions: vec!["PLACE_ORDERS".to_string(), "VIEW_POSITIONS".to_string()],
    }
}

/// Mock gRPC response helper
pub fn mock_grpc_response<T>(data: T) -> Result<Response<T>, Status> {
    Ok(Response::new(data))
}

/// Mock gRPC error helper
pub fn mock_grpc_error<T>(code: tonic::Code, message: &str) -> Result<Response<T>, Status> {
    Err(Status::new(code, message))
}

/// Fixed point conversion helpers for tests
pub fn parse_fixed_point_test(value: &str) -> i64 {
    value.parse::<f64>().unwrap_or(0.0) as i64 * 10000
}

pub fn fixed_point_to_string_test(value: i64) -> String {
    format!("{:.4}", value as f64 / 10000.0)
}