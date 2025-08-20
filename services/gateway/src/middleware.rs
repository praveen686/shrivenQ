//! Middleware for authentication, rate limiting, and monitoring

use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn};

use crate::config::GatewayConfig;
use crate::models::{ApiResponse, ErrorResponse};
use crate::rate_limiter::RateLimiter;
use rustc_hash::FxHashMap;

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
    pub permissions: Vec<String>,
}

/// User context extracted from JWT
#[derive(Debug, Clone)]
pub struct UserContext {
    pub user_id: String,
    pub permissions: Vec<String>,
    pub expires_at: usize,
}

/// Authentication middleware state
#[derive(Clone)]
pub struct AuthState {
    pub config: Arc<GatewayConfig>,
}

impl std::fmt::Debug for AuthState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthState")
            .field("config", &"Arc<GatewayConfig>")
            .finish()
    }
}

/// Rate limiting middleware state
#[derive(Clone)]
pub struct RateLimitState {
    pub limiter: Arc<RateLimiter>,
}

impl std::fmt::Debug for RateLimitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimitState")
            .field("limiter", &"Arc<RateLimiter>")
            .finish()
    }
}

/// Create standardized error response
fn create_error_response(
    error_code: &str,
    message: &str,
    details: Option<FxHashMap<String, String>>,
) -> ErrorResponse {
    ErrorResponse {
        error: error_code.to_string(),
        message: message.to_string(),
        details,
    }
}

/// Authentication middleware
pub async fn auth_middleware(
    State(auth_state): State<AuthState>,
    mut request: Request,
    next: Next,
) -> Result<Response, Response> {
    // Skip authentication for public endpoints
    let path = request.uri().path();
    if is_public_endpoint(path) {
        return Ok(next.run(request).await);
    }

    // Extract Authorization header
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    let token = match auth_header {
        Some(header) if header.starts_with("Bearer ") => &header[7..],
        _ => {
            error!(
                "Authentication attempt without valid Bearer token from IP: {}",
                get_client_ip(&request)
            );
            warn!("Missing or invalid Authorization header");

            let error_response = create_error_response(
                "missing_token",
                "Authorization header missing or invalid format",
                Some(
                    [("client_ip".to_string(), get_client_ip(&request))]
                        .into_iter()
                        .collect(),
                ),
            );

            let json_response = Json(ApiResponse::<()> {
                success: false,
                data: None,
                error: Some(error_response),
                timestamp: chrono::Utc::now().timestamp(),
            });

            return Err((StatusCode::UNAUTHORIZED, json_response).into_response());
        }
    };

    // Validate JWT token
    let validation = Validation::default();
    let decoding_key = DecodingKey::from_secret(auth_state.config.auth.jwt_secret.as_ref());

    match decode::<Claims>(token, &decoding_key, &validation) {
        Ok(token_data) => {
            let user_context = UserContext {
                user_id: token_data.claims.sub,
                permissions: token_data.claims.permissions,
                expires_at: token_data.claims.exp,
            };

            // Add user context to request extensions
            request.extensions_mut().insert(user_context);
            Ok(next.run(request).await)
        }
        Err(e) => {
            error!(
                "JWT validation failed for token from {}: {}",
                get_client_ip(&request),
                e
            );
            warn!("JWT validation failed: {}", e);

            let error_response = create_error_response(
                "authentication_failed",
                "Invalid or expired JWT token",
                Some(
                    [("client_ip".to_string(), get_client_ip(&request))]
                        .into_iter()
                        .collect(),
                ),
            );

            let json_response = Json(ApiResponse::<()> {
                success: false,
                data: None,
                error: Some(error_response),
                timestamp: chrono::Utc::now().timestamp(),
            });

            Err((StatusCode::UNAUTHORIZED, json_response).into_response())
        }
    }
}

/// Rate limiting middleware
pub async fn rate_limit_middleware(
    State(rate_limit_state): State<RateLimitState>,
    request: Request,
    next: Next,
) -> Result<Response, Response> {
    let client_ip = get_client_ip(&request);
    let path = request.uri().path();

    // Check rate limit
    if !rate_limit_state
        .limiter
        .check_rate_limit(&client_ip, path)
        .await
    {
        error!(
            "Rate limit violation - IP: {} exceeded limits on path: {}",
            client_ip, path
        );
        warn!(
            "Rate limit exceeded for IP: {} on path: {}",
            client_ip, path
        );

        let error_response = create_error_response(
            "rate_limit_exceeded",
            "Too many requests - rate limit exceeded",
            Some(
                [
                    ("client_ip".to_string(), client_ip.clone()),
                    ("path".to_string(), path.to_string()),
                ]
                .into_iter()
                .collect(),
            ),
        );

        let json_response = Json(ApiResponse::<()> {
            success: false,
            data: None,
            error: Some(error_response),
            timestamp: chrono::Utc::now().timestamp(),
        });

        return Err((StatusCode::TOO_MANY_REQUESTS, json_response).into_response());
    }

    Ok(next.run(request).await)
}

/// Request logging middleware
pub async fn logging_middleware(request: Request, next: Next) -> Response {
    let start = std::time::Instant::now();
    let method = request.method().clone();
    let uri = request.uri().clone();
    let client_ip = get_client_ip(&request);

    let response = next.run(request).await;

    let duration = start.elapsed();
    let status = response.status();

    info!(
        method = %method,
        uri = %uri,
        status = %status,
        duration_ms = duration.as_millis(),
        client_ip = %client_ip,
        "Request processed"
    );

    response
}

/// CORS layer factory
pub fn create_cors_layer(config: &GatewayConfig) -> CorsLayer {
    let mut cors = CorsLayer::new()
        .allow_credentials(config.cors.allow_credentials)
        .max_age(std::time::Duration::from_secs(config.cors.max_age_seconds));

    // Configure allowed origins
    if config.cors.allowed_origins.contains(&"*".to_string()) {
        cors = cors.allow_origin(tower_http::cors::Any);
    } else {
        for origin in &config.cors.allowed_origins {
            if let Ok(header_value) = HeaderValue::from_str(origin) {
                cors = cors.allow_origin(header_value);
            }
        }
    }

    // Configure allowed methods
    let methods: Result<Vec<_>, _> = config
        .cors
        .allowed_methods
        .iter()
        .map(|method| method.parse())
        .collect();

    if let Ok(methods) = methods {
        cors = cors.allow_methods(methods);
    }

    // Configure allowed headers
    let headers: Result<Vec<_>, _> = config
        .cors
        .allowed_headers
        .iter()
        .map(|header| header.parse())
        .collect();

    if let Ok(headers) = headers {
        cors = cors.allow_headers(headers);
    }

    cors
}

/// Check if endpoint is public (doesn't require authentication)
fn is_public_endpoint(path: &str) -> bool {
    matches!(
        path,
        "/health"
            | "/metrics"
            | "/api/v1/auth/login"
            | "/api/v1/auth/refresh"
            | "/docs"
            | "/swagger-ui"
    )
}

/// Extract client IP from request
fn get_client_ip(request: &Request) -> String {
    // Try X-Forwarded-For first (common in load balancers/proxies)
    if let Some(forwarded_for) = request.headers().get("X-Forwarded-For")
        && let Ok(forwarded_str) = forwarded_for.to_str()
            && let Some(first_ip) = forwarded_str.split(',').next() {
                return first_ip.trim().to_string();
            }

    // Try X-Real-IP
    if let Some(real_ip) = request.headers().get("X-Real-IP")
        && let Ok(real_ip_str) = real_ip.to_str() {
            return real_ip_str.to_string();
        }

    // Fallback to connection info (though this might not be available in Axum)
    "unknown".to_string()
}

/// Permission checking helper
#[must_use] pub fn check_permission(user_context: &UserContext, required_permission: &str) -> bool {
    user_context
        .permissions
        .contains(&"PERMISSION_ADMIN".to_string())
        || user_context
            .permissions
            .contains(&required_permission.to_string())
}

/// Extract user context from request
pub fn get_user_context(request: &Request) -> Option<&UserContext> {
    request.extensions().get::<UserContext>()
}

/// Extract user context from headers (for handlers that don't have access to request extensions)
pub fn get_user_context_from_headers(headers: &HeaderMap) -> Option<UserContext> {
    let auth_header = headers.get(header::AUTHORIZATION)?.to_str().ok()?;

    if !auth_header.starts_with("Bearer ") {
        return None;
    }

    let token = &auth_header[7..];

    // For now, create a dummy validation - in production, use proper JWT validation
    let validation = Validation::default();
    let decoding_key = DecodingKey::from_secret(b"dummy_secret");

    match decode::<Claims>(token, &decoding_key, &validation) {
        Ok(token_data) => {
            let claims = token_data.claims;
            Some(UserContext {
                user_id: claims.sub,
                permissions: claims.permissions,
                expires_at: claims.exp,
            })
        }
        Err(e) => {
            warn!("Failed to decode JWT token from headers: {}", e);
            None
        }
    }
}

// Re-export utility function for tests
pub use crate::utils::parse_fixed_point;
