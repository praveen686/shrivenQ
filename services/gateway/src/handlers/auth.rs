//! Authentication handlers

use axum::{extract::State, http::StatusCode, response::Json};
use std::sync::Arc;
use tracing::{error, info};

use crate::{
    grpc_clients::{GrpcClients, auth},
    models::{ApiResponse, ErrorResponse, LoginRequest, LoginResponse, RefreshTokenRequest},
};

/// Authentication handlers
#[derive(Clone)]
pub struct AuthHandlers {
    grpc_clients: Arc<GrpcClients>,
}

impl AuthHandlers {
    pub fn new(grpc_clients: Arc<GrpcClients>) -> Self {
        Self { grpc_clients }
    }

    /// Login endpoint
    pub async fn login(
        State(handlers): State<AuthHandlers>,
        Json(request): Json<LoginRequest>,
    ) -> Result<Json<ApiResponse<LoginResponse>>, StatusCode> {
        info!("Login request for user: {}", request.username);

        let mut client = handlers.grpc_clients.auth.clone();

        let grpc_request = auth::LoginRequest {
            username: request.username.clone(),
            password: request.password,
            exchange: request.exchange.unwrap_or_default(),
        };

        match client.login(grpc_request).await {
            Ok(response) => {
                let grpc_response = response.into_inner();

                // Convert permissions from enum to string
                let permissions: Vec<String> = grpc_response
                    .permissions
                    .iter()
                    .map(|p| permission_to_string(*p))
                    .collect();

                let login_response = LoginResponse {
                    token: grpc_response.token,
                    refresh_token: grpc_response.refresh_token,
                    expires_at: grpc_response.expires_at,
                    permissions,
                };

                info!("Login successful for user: {}", request.username);
                Ok(Json(ApiResponse::success(login_response)))
            }
            Err(e) => {
                error!("Login failed for user {}: {}", request.username, e);
                let error_response = ErrorResponse {
                    error: "LOGIN_FAILED".to_string(),
                    message: "Invalid credentials or service unavailable".to_string(),
                    details: None,
                };
                Ok(Json(ApiResponse::error(error_response)))
            }
        }
    }

    /// Refresh token endpoint
    pub async fn refresh_token(
        State(handlers): State<AuthHandlers>,
        Json(request): Json<RefreshTokenRequest>,
    ) -> Result<Json<ApiResponse<LoginResponse>>, StatusCode> {
        info!("Token refresh request");

        let mut client = handlers.grpc_clients.auth.clone();

        let grpc_request = auth::RefreshTokenRequest {
            refresh_token: request.refresh_token,
        };

        match client.refresh_token(grpc_request).await {
            Ok(response) => {
                let grpc_response = response.into_inner();

                let refresh_response = LoginResponse {
                    token: grpc_response.token,
                    refresh_token: grpc_response.refresh_token,
                    expires_at: grpc_response.expires_at,
                    permissions: vec![], // Permissions not returned in refresh
                };

                info!("Token refresh successful");
                Ok(Json(ApiResponse::success(refresh_response)))
            }
            Err(e) => {
                error!("Token refresh failed: {}", e);
                let error_response = ErrorResponse {
                    error: "REFRESH_FAILED".to_string(),
                    message: "Invalid refresh token or service unavailable".to_string(),
                    details: None,
                };
                Ok(Json(ApiResponse::error(error_response)))
            }
        }
    }

    /// Validate token endpoint (for internal use or debugging)
    pub async fn validate_token(
        State(handlers): State<AuthHandlers>,
        token: String,
    ) -> Result<Json<ApiResponse<bool>>, StatusCode> {
        let mut client = handlers.grpc_clients.auth.clone();

        let grpc_request = auth::ValidateTokenRequest { token };

        match client.validate_token(grpc_request).await {
            Ok(response) => {
                let grpc_response = response.into_inner();
                Ok(Json(ApiResponse::success(grpc_response.valid)))
            }
            Err(e) => {
                error!("Token validation failed: {}", e);
                let error_response = ErrorResponse {
                    error: "VALIDATION_FAILED".to_string(),
                    message: "Token validation service unavailable".to_string(),
                    details: None,
                };
                Ok(Json(ApiResponse::error(error_response)))
            }
        }
    }

    /// Revoke token endpoint
    pub async fn revoke_token(
        State(handlers): State<AuthHandlers>,
        token: String,
    ) -> Result<Json<ApiResponse<bool>>, StatusCode> {
        info!("Token revocation request");

        let mut client = handlers.grpc_clients.auth.clone();

        let grpc_request = auth::RevokeTokenRequest { token };

        match client.revoke_token(grpc_request).await {
            Ok(response) => {
                let grpc_response = response.into_inner();
                info!("Token revoked successfully");
                Ok(Json(ApiResponse::success(grpc_response.success)))
            }
            Err(e) => {
                error!("Token revocation failed: {}", e);
                let error_response = ErrorResponse {
                    error: "REVOCATION_FAILED".to_string(),
                    message: "Token revocation service unavailable".to_string(),
                    details: None,
                };
                Ok(Json(ApiResponse::error(error_response)))
            }
        }
    }
}

/// Convert gRPC permission enum to string
fn permission_to_string(permission: i32) -> String {
    match auth::Permission::try_from(permission) {
        Ok(auth::Permission::ReadMarketData) => "READ_MARKET_DATA".to_string(),
        Ok(auth::Permission::PlaceOrders) => "PLACE_ORDERS".to_string(),
        Ok(auth::Permission::CancelOrders) => "CANCEL_ORDERS".to_string(),
        Ok(auth::Permission::ViewPositions) => "VIEW_POSITIONS".to_string(),
        Ok(auth::Permission::ModifyRiskLimits) => "MODIFY_RISK_LIMITS".to_string(),
        Ok(auth::Permission::Admin) => "ADMIN".to_string(),
        _ => "UNKNOWN".to_string(),
    }
}
