//! Risk management handlers

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
};
use rustc_hash::FxHashMap;
use serde::Deserialize;
use std::sync::Arc;
use tracing::{error, info};

use crate::{
    grpc_clients::{GrpcClients, risk},
    middleware::{check_permission, get_user_context},
    models::{
        ApiResponse, CheckOrderRequest, CheckOrderResponse, ErrorResponse, KillSwitchRequest,
        KillSwitchResponse, PositionInfo, PositionResponse, RiskMetrics,
    },
};

/// Query parameters for positions
#[derive(Debug, Deserialize)]
pub struct PositionsQuery {
    /// Optional symbol filter for positions
    pub symbol: Option<String>,
}

/// Risk management handlers
#[derive(Clone)]
pub struct RiskHandlers {
    grpc_clients: Arc<GrpcClients>,
}

impl std::fmt::Debug for RiskHandlers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RiskHandlers")
            .field("grpc_clients", &"Arc<GrpcClients>")
            .finish()
    }
}

impl RiskHandlers {
    pub const fn new(grpc_clients: Arc<GrpcClients>) -> Self {
        Self { grpc_clients }
    }

    /// Check order risk
    pub async fn check_order(
        State(handlers): State<Self>,
        request: axum::extract::Request,
        Json(check_request): Json<CheckOrderRequest>,
    ) -> Result<Json<ApiResponse<CheckOrderResponse>>, StatusCode> {
        // Check permissions
        let user_context = get_user_context(&request);
        if let Some(user) = user_context
            && !check_permission(user, "PLACE_ORDERS") {
                let error_response = ErrorResponse {
                    error: "PERMISSION_DENIED".to_string(),
                    message: "Insufficient permissions to check orders".to_string(),
                    details: None,
                };
                return Ok(Json(ApiResponse::error(error_response)));
            }

        info!("Risk check request for symbol: {}", check_request.symbol);

        let mut client = handlers.grpc_clients.risk.clone();

        let grpc_request = risk::CheckOrderRequest {
            symbol: check_request.symbol.clone(),
            side: string_to_side(&check_request.side),
            quantity: parse_fixed_point(&check_request.quantity).unwrap_or(0),
            price: parse_fixed_point(&check_request.price).unwrap_or(0),
            strategy_id: check_request.strategy_id.unwrap_or_default(),
            exchange: check_request.exchange,
        };

        match client.check_order(grpc_request).await {
            Ok(response) => {
                let grpc_response = response.into_inner();

                let current_metrics = if let Some(metrics) = grpc_response.current_metrics {
                    RiskMetrics {
                        total_exposure: fixed_point_to_string(metrics.total_exposure),
                        current_drawdown: fixed_point_to_string(metrics.current_drawdown.into()),
                        daily_pnl: fixed_point_to_string(metrics.daily_pnl),
                        open_positions: metrics.open_positions,
                        orders_today: metrics.orders_today,
                        circuit_breaker_active: metrics.circuit_breaker_active,
                        kill_switch_active: metrics.kill_switch_active,
                    }
                } else {
                    // Default empty metrics
                    RiskMetrics {
                        total_exposure: "0.0000".to_string(),
                        current_drawdown: "0.0000".to_string(),
                        daily_pnl: "0.0000".to_string(),
                        open_positions: 0,
                        orders_today: 0,
                        circuit_breaker_active: false,
                        kill_switch_active: false,
                    }
                };

                let check_response = CheckOrderResponse {
                    result: check_result_to_string(grpc_response.result),
                    reason: if grpc_response.reason.is_empty() {
                        None
                    } else {
                        Some(grpc_response.reason)
                    },
                    current_metrics,
                };

                Ok(Json(ApiResponse::success(check_response)))
            }
            Err(e) => {
                error!(
                    "Order risk check failed for symbol {}: {}",
                    check_request.symbol, e
                );
                let error_response = ErrorResponse {
                    error: "RISK_CHECK_FAILED".to_string(),
                    message: "Failed to check order risk".to_string(),
                    details: Some(FxHashMap::from_iter([(
                        "symbol".to_string(),
                        check_request.symbol,
                    )])),
                };
                Ok(Json(ApiResponse::error(error_response)))
            }
        }
    }

    /// Get current positions
    pub async fn get_positions(
        State(handlers): State<Self>,
        request: axum::extract::Request,
        Query(query): Query<PositionsQuery>,
    ) -> Result<Json<ApiResponse<PositionResponse>>, StatusCode> {
        // Check permissions
        let user_context = get_user_context(&request);
        if let Some(user) = user_context
            && !check_permission(user, "VIEW_POSITIONS") {
                let error_response = ErrorResponse {
                    error: "PERMISSION_DENIED".to_string(),
                    message: "Insufficient permissions to view positions".to_string(),
                    details: None,
                };
                return Ok(Json(ApiResponse::error(error_response)));
            }

        info!("Get positions request");

        let mut client = handlers.grpc_clients.risk.clone();

        let grpc_request = risk::GetPositionsRequest {
            symbol: query.symbol.unwrap_or_default(),
        };

        match client.get_positions(grpc_request).await {
            Ok(response) => {
                let grpc_response = response.into_inner();

                let positions: Vec<PositionInfo> = grpc_response
                    .positions
                    .into_iter()
                    .map(|pos| PositionInfo {
                        symbol: pos.symbol,
                        net_quantity: fixed_point_to_string(pos.net_quantity),
                        avg_price: fixed_point_to_string(pos.avg_price),
                        mark_price: fixed_point_to_string(pos.mark_price),
                        unrealized_pnl: fixed_point_to_string(pos.unrealized_pnl),
                        realized_pnl: fixed_point_to_string(pos.realized_pnl),
                        position_value: fixed_point_to_string(pos.position_value),
                        exchange: pos.exchange,
                    })
                    .collect();

                let position_response = PositionResponse {
                    positions,
                    total_exposure: fixed_point_to_string(grpc_response.total_exposure),
                };

                Ok(Json(ApiResponse::success(position_response)))
            }
            Err(e) => {
                error!("Get positions failed: {}", e);
                let error_response = ErrorResponse {
                    error: "POSITIONS_FAILED".to_string(),
                    message: "Failed to get positions".to_string(),
                    details: None,
                };
                Ok(Json(ApiResponse::error(error_response)))
            }
        }
    }

    /// Get risk metrics
    pub async fn get_metrics(
        State(handlers): State<Self>,
        request: axum::extract::Request,
    ) -> Result<Json<ApiResponse<RiskMetrics>>, StatusCode> {
        // Check permissions
        let user_context = get_user_context(&request);
        if let Some(user) = user_context
            && !check_permission(user, "VIEW_POSITIONS") {
                let error_response = ErrorResponse {
                    error: "PERMISSION_DENIED".to_string(),
                    message: "Insufficient permissions to view risk metrics".to_string(),
                    details: None,
                };
                return Ok(Json(ApiResponse::error(error_response)));
            }

        info!("Get risk metrics request");

        let mut client = handlers.grpc_clients.risk.clone();

        match client.get_metrics(risk::GetMetricsRequest {}).await {
            Ok(response) => {
                let grpc_response = response.into_inner();

                let risk_metrics = if let Some(metrics) = grpc_response.metrics {
                    RiskMetrics {
                        total_exposure: fixed_point_to_string(metrics.total_exposure),
                        current_drawdown: fixed_point_to_string(metrics.current_drawdown.into()),
                        daily_pnl: fixed_point_to_string(metrics.daily_pnl),
                        open_positions: metrics.open_positions,
                        orders_today: metrics.orders_today,
                        circuit_breaker_active: metrics.circuit_breaker_active,
                        kill_switch_active: metrics.kill_switch_active,
                    }
                } else {
                    RiskMetrics {
                        total_exposure: "0.0000".to_string(),
                        current_drawdown: "0.0000".to_string(),
                        daily_pnl: "0.0000".to_string(),
                        open_positions: 0,
                        orders_today: 0,
                        circuit_breaker_active: false,
                        kill_switch_active: false,
                    }
                };

                Ok(Json(ApiResponse::success(risk_metrics)))
            }
            Err(e) => {
                error!("Get risk metrics failed: {}", e);
                let error_response = ErrorResponse {
                    error: "RISK_METRICS_FAILED".to_string(),
                    message: "Failed to get risk metrics".to_string(),
                    details: None,
                };
                Ok(Json(ApiResponse::error(error_response)))
            }
        }
    }

    /// Activate or deactivate kill switch
    pub async fn kill_switch(
        State(handlers): State<Self>,
        request: axum::extract::Request,
        Json(kill_switch_request): Json<KillSwitchRequest>,
    ) -> Result<Json<ApiResponse<KillSwitchResponse>>, StatusCode> {
        // Check permissions - only admin can control kill switch
        let user_context = get_user_context(&request);
        if let Some(user) = user_context
            && !check_permission(user, "ADMIN") {
                let error_response = ErrorResponse {
                    error: "PERMISSION_DENIED".to_string(),
                    message: "Insufficient permissions to control kill switch".to_string(),
                    details: None,
                };
                return Ok(Json(ApiResponse::error(error_response)));
            }

        info!(
            "Kill switch request: activate={}",
            kill_switch_request.activate
        );

        let mut client = handlers.grpc_clients.risk.clone();

        let grpc_request = risk::KillSwitchRequest {
            activate: kill_switch_request.activate,
            reason: kill_switch_request.reason.unwrap_or_default(),
        };

        match client.activate_kill_switch(grpc_request).await {
            Ok(response) => {
                let grpc_response = response.into_inner();

                let kill_switch_response = KillSwitchResponse {
                    success: grpc_response.success,
                    is_active: grpc_response.is_active,
                };

                info!("Kill switch operation completed successfully");
                Ok(Json(ApiResponse::success(kill_switch_response)))
            }
            Err(e) => {
                error!("Kill switch operation failed: {}", e);
                let error_response = ErrorResponse {
                    error: "KILL_SWITCH_FAILED".to_string(),
                    message: "Failed to control kill switch".to_string(),
                    details: None,
                };
                Ok(Json(ApiResponse::error(error_response)))
            }
        }
    }
}

// Helper conversion functions
fn string_to_side(side: &str) -> i32 {
    match side.to_uppercase().as_str() {
        "BUY" => risk::Side::Buy.into(),
        "SELL" => risk::Side::Sell.into(),
        _ => risk::Side::Unspecified.into(),
    }
}

fn check_result_to_string(result: i32) -> String {
    match risk::CheckResult::try_from(result) {
        Ok(risk::CheckResult::Approved) => "APPROVED".to_string(),
        Ok(risk::CheckResult::Rejected) => "REJECTED".to_string(),
        Ok(risk::CheckResult::RequiresApproval) => "REQUIRES_APPROVAL".to_string(),
        _ => "UNKNOWN".to_string(),
    }
}

fn parse_fixed_point(value: &str) -> Option<i64> {
    // Parse string to fixed-point without using f64
    let parts: Vec<&str> = value.split('.').collect();
    if parts.is_empty() || parts.len() > 2 {
        return None;
    }

    let whole: i64 = parts[0].parse().ok()?;
    let fraction = if parts.len() == 2 {
        // Parse up to 4 decimal places
        let frac_str = if parts[1].len() > 4 {
            &parts[1][..4]
        } else {
            parts[1]
        };
        let frac_val: i64 = frac_str.parse().ok()?;
        // Pad with zeros if needed
        // SAFETY: frac_str.len() is at most 4 (decimal places), fits in u32
        frac_val * 10_i64.pow(4 - frac_str.len() as u32)
    } else {
        0
    };

    if whole < 0 {
        Some(whole * 10000 - fraction)
    } else {
        Some(whole * 10000 + fraction)
    }
}

fn fixed_point_to_string(value: i64) -> String {
    // For display only - using integer arithmetic to avoid casts
    let is_negative = value < 0;
    let abs_value = value.unsigned_abs();
    let whole = abs_value / 10000;
    let fraction = abs_value % 10000;

    if is_negative {
        format!("-{whole}.{fraction:04}")
    } else {
        format!("{whole}.{fraction:04}")
    }
}
