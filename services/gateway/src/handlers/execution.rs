//! Execution service handlers

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use rustc_hash::FxHashMap;
use serde::Deserialize;
use std::sync::Arc;
use tracing::{error, info};

use crate::{
    grpc_clients::{GrpcClients, execution},
    middleware::{check_permission, get_user_context_from_headers},
    models::{
        ApiResponse, CancelOrderRequest, ErrorResponse, FillInfo, OrderStatusResponse,
        SubmitOrderRequest, SubmitOrderResponse,
    },
};

/// Query parameters for order lookup
#[derive(Deserialize)]
pub struct OrderQuery {
    pub client_order_id: Option<String>,
}

/// Execution handlers
#[derive(Clone)]
pub struct ExecutionHandlers {
    grpc_clients: Arc<GrpcClients>,
}

impl ExecutionHandlers {
    pub fn new(grpc_clients: Arc<GrpcClients>) -> Self {
        Self { grpc_clients }
    }

    /// Submit order endpoint
    pub async fn submit_order(
        State(handlers): State<ExecutionHandlers>,
        headers: HeaderMap,
        Json(order_request): Json<SubmitOrderRequest>,
    ) -> Result<Json<ApiResponse<SubmitOrderResponse>>, StatusCode> {
        // Check permissions
        let user_context = get_user_context_from_headers(&headers);
        if let Some(user) = user_context {
            if !check_permission(&user, "PLACE_ORDERS") {
                let error_response = ErrorResponse {
                    error: "PERMISSION_DENIED".to_string(),
                    message: "Insufficient permissions to place orders".to_string(),
                    details: None,
                };
                return Ok(Json(ApiResponse::error(error_response)));
            }
        }

        info!("Submit order request for symbol: {}", order_request.symbol);

        let mut client = handlers.grpc_clients.execution.clone();

        // Convert REST request to gRPC
        let grpc_request = execution::SubmitOrderRequest {
            client_order_id: order_request
                .client_order_id
                .unwrap_or_else(|| format!("gateway_{}", uuid::Uuid::new_v4())),
            symbol: order_request.symbol.clone(),
            side: string_to_side(&order_request.side),
            quantity: parse_fixed_point(&order_request.quantity).unwrap_or(0),
            order_type: string_to_order_type(&order_request.order_type),
            limit_price: order_request
                .limit_price
                .map(|p| parse_fixed_point(&p).unwrap_or(0))
                .unwrap_or(0),
            stop_price: order_request
                .stop_price
                .map(|p| parse_fixed_point(&p).unwrap_or(0))
                .unwrap_or(0),
            time_in_force: string_to_time_in_force(
                order_request.time_in_force.as_deref().unwrap_or("GTC"),
            ),
            venue: order_request.venue.unwrap_or_default(),
            strategy_id: order_request.strategy_id.unwrap_or_default(),
            params: order_request
                .params
                .map(|fx_map| fx_map.into_iter().collect())
                .unwrap_or_default(),
        };

        match client.submit_order(grpc_request).await {
            Ok(response) => {
                let grpc_response = response.into_inner();

                let order_response = SubmitOrderResponse {
                    order_id: grpc_response.order_id,
                    status: order_status_to_string(grpc_response.status),
                    message: grpc_response.message,
                };

                info!("Order submitted successfully: {}", grpc_response.order_id);
                Ok(Json(ApiResponse::success(order_response)))
            }
            Err(e) => {
                error!(
                    "Order submission failed for symbol {}: {}",
                    order_request.symbol, e
                );
                let error_response = ErrorResponse {
                    error: "ORDER_SUBMISSION_FAILED".to_string(),
                    message: "Failed to submit order to execution service".to_string(),
                    details: Some(FxHashMap::from_iter([(
                        "symbol".to_string(),
                        order_request.symbol,
                    )])),
                };
                Ok(Json(ApiResponse::error(error_response)))
            }
        }
    }

    /// Cancel order endpoint
    pub async fn cancel_order(
        State(handlers): State<ExecutionHandlers>,
        headers: HeaderMap,
        Json(cancel_request): Json<CancelOrderRequest>,
    ) -> Result<Json<ApiResponse<bool>>, StatusCode> {
        // Check permissions
        let user_context = get_user_context_from_headers(&headers);
        if let Some(user) = user_context {
            if !check_permission(&user, "CANCEL_ORDERS") {
                let error_response = ErrorResponse {
                    error: "PERMISSION_DENIED".to_string(),
                    message: "Insufficient permissions to cancel orders".to_string(),
                    details: None,
                };
                return Ok(Json(ApiResponse::error(error_response)));
            }
        }

        info!("Cancel order request");

        let mut client = handlers.grpc_clients.execution.clone();

        let grpc_request = execution::CancelOrderRequest {
            order_id: cancel_request.order_id.unwrap_or(0),
            client_order_id: cancel_request.client_order_id.unwrap_or_default(),
        };

        match client.cancel_order(grpc_request).await {
            Ok(response) => {
                let grpc_response = response.into_inner();
                info!("Order cancelled successfully");
                Ok(Json(ApiResponse::success(grpc_response.success)))
            }
            Err(e) => {
                error!("Order cancellation failed: {}", e);
                let error_response = ErrorResponse {
                    error: "ORDER_CANCELLATION_FAILED".to_string(),
                    message: "Failed to cancel order".to_string(),
                    details: None,
                };
                Ok(Json(ApiResponse::error(error_response)))
            }
        }
    }

    /// Get order status endpoint
    pub async fn get_order_status(
        State(handlers): State<ExecutionHandlers>,
        headers: HeaderMap,
        Path(order_id): Path<i64>,
        Query(query): Query<OrderQuery>,
    ) -> Result<Json<ApiResponse<OrderStatusResponse>>, StatusCode> {
        // Check permissions
        let user_context = get_user_context_from_headers(&headers);
        if let Some(user) = user_context {
            if !check_permission(&user, "VIEW_POSITIONS") {
                let error_response = ErrorResponse {
                    error: "PERMISSION_DENIED".to_string(),
                    message: "Insufficient permissions to view orders".to_string(),
                    details: None,
                };
                return Ok(Json(ApiResponse::error(error_response)));
            }
        }

        info!("Get order status request for order: {}", order_id);

        let mut client = handlers.grpc_clients.execution.clone();

        let grpc_request = execution::GetOrderRequest {
            order_id,
            client_order_id: query.client_order_id.unwrap_or_default(),
        };

        match client.get_order(grpc_request).await {
            Ok(response) => {
                let grpc_response = response.into_inner();

                if let Some(order) = grpc_response.order {
                    let fills: Vec<FillInfo> = order
                        .fills
                        .into_iter()
                        .map(|fill| FillInfo {
                            fill_id: fill.fill_id,
                            quantity: fixed_point_to_string(fill.quantity),
                            price: fixed_point_to_string(fill.price),
                            timestamp: fill.timestamp,
                            is_maker: fill.is_maker,
                            commission: fixed_point_to_string(fill.commission),
                            commission_asset: fill.commission_asset,
                        })
                        .collect();

                    let order_response = OrderStatusResponse {
                        order_id: order.order_id,
                        client_order_id: order.client_order_id,
                        exchange_order_id: order.exchange_order_id,
                        symbol: order.symbol,
                        side: side_to_string(order.side),
                        quantity: fixed_point_to_string(order.quantity),
                        filled_quantity: fixed_point_to_string(order.filled_quantity),
                        avg_fill_price: fixed_point_to_string(order.avg_fill_price),
                        status: order_status_to_string(order.status),
                        order_type: order_type_to_string(order.order_type),
                        limit_price: Some(fixed_point_to_string(order.limit_price)),
                        stop_price: Some(fixed_point_to_string(order.stop_price)),
                        time_in_force: time_in_force_to_string(order.time_in_force),
                        venue: order.venue,
                        strategy_id: Some(order.strategy_id),
                        created_at: order.created_at,
                        updated_at: order.updated_at,
                        fills,
                    };

                    Ok(Json(ApiResponse::success(order_response)))
                } else {
                    let error_response = ErrorResponse {
                        error: "ORDER_NOT_FOUND".to_string(),
                        message: "Order not found".to_string(),
                        details: None,
                    };
                    Ok(Json(ApiResponse::error(error_response)))
                }
            }
            Err(e) => {
                error!("Get order status failed: {}", e);
                let error_response = ErrorResponse {
                    error: "ORDER_STATUS_FAILED".to_string(),
                    message: "Failed to get order status".to_string(),
                    details: None,
                };
                Ok(Json(ApiResponse::error(error_response)))
            }
        }
    }

    /// Get execution metrics endpoint
    pub async fn get_metrics(
        State(handlers): State<ExecutionHandlers>,
        headers: HeaderMap,
    ) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
        // Check permissions
        let user_context = get_user_context_from_headers(&headers);
        if let Some(user) = user_context {
            if !check_permission(&user, "VIEW_POSITIONS") {
                let error_response = ErrorResponse {
                    error: "PERMISSION_DENIED".to_string(),
                    message: "Insufficient permissions to view metrics".to_string(),
                    details: None,
                };
                return Ok(Json(ApiResponse::error(error_response)));
            }
        }

        let mut client = handlers.grpc_clients.execution.clone();

        match client.get_metrics(execution::GetMetricsRequest {}).await {
            Ok(response) => {
                let grpc_response = response.into_inner();

                // Convert to JSON for flexible REST API response
                let metrics_json = match grpc_response.metrics {
                    Some(ref metrics) => serde_json::json!({
                        "total_orders": metrics.total_orders,
                        "filled_orders": metrics.filled_orders,
                        "cancelled_orders": metrics.cancelled_orders,
                        "rejected_orders": metrics.rejected_orders,
                        "avg_fill_time_ms": metrics.avg_fill_time_ms,
                        "total_volume": fixed_point_to_string(metrics.total_volume),
                        "total_commission": fixed_point_to_string(metrics.total_commission),
                        // SAFETY: metrics.fill_rate is i32, safely widens to i64
                        "fill_rate": fixed_point_to_string(metrics.fill_rate as i64),
                        "venues_used": &metrics.venues_used,
                    }),
                    None => serde_json::json!({
                        "total_orders": 0,
                        "filled_orders": 0,
                        "cancelled_orders": 0,
                        "rejected_orders": 0,
                        "avg_fill_time_ms": 0,
                        "total_volume": "0",
                        "total_commission": "0",
                        "fill_rate": "0",
                        "venues_used": Vec::<String>::new(),
                    }),
                };

                Ok(Json(ApiResponse::success(metrics_json)))
            }
            Err(e) => {
                error!("Get execution metrics failed: {}", e);
                let error_response = ErrorResponse {
                    error: "METRICS_FAILED".to_string(),
                    message: "Failed to get execution metrics".to_string(),
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
        "BUY" => execution::Side::Buy.into(),
        "SELL" => execution::Side::Sell.into(),
        _ => execution::Side::Unspecified.into(),
    }
}

fn side_to_string(side: i32) -> String {
    match execution::Side::try_from(side) {
        Ok(execution::Side::Buy) => "BUY".to_string(),
        Ok(execution::Side::Sell) => "SELL".to_string(),
        _ => "UNKNOWN".to_string(),
    }
}

fn string_to_order_type(order_type: &str) -> i32 {
    match order_type.to_uppercase().as_str() {
        "MARKET" => execution::OrderType::Market.into(),
        "LIMIT" => execution::OrderType::Limit.into(),
        "STOP" => execution::OrderType::Stop.into(),
        "STOP_LIMIT" => execution::OrderType::StopLimit.into(),
        "ICEBERG" => execution::OrderType::Iceberg.into(),
        _ => execution::OrderType::Unspecified.into(),
    }
}

fn order_type_to_string(order_type: i32) -> String {
    match execution::OrderType::try_from(order_type) {
        Ok(execution::OrderType::Market) => "MARKET".to_string(),
        Ok(execution::OrderType::Limit) => "LIMIT".to_string(),
        Ok(execution::OrderType::Stop) => "STOP".to_string(),
        Ok(execution::OrderType::StopLimit) => "STOP_LIMIT".to_string(),
        Ok(execution::OrderType::Iceberg) => "ICEBERG".to_string(),
        _ => "UNKNOWN".to_string(),
    }
}

fn string_to_time_in_force(tif: &str) -> i32 {
    match tif.to_uppercase().as_str() {
        "GTC" => execution::TimeInForce::Gtc.into(),
        "IOC" => execution::TimeInForce::Ioc.into(),
        "FOK" => execution::TimeInForce::Fok.into(),
        "DAY" => execution::TimeInForce::Day.into(),
        "GTD" => execution::TimeInForce::Gtd.into(),
        _ => execution::TimeInForce::Gtc.into(),
    }
}

fn time_in_force_to_string(tif: i32) -> String {
    match execution::TimeInForce::try_from(tif) {
        Ok(execution::TimeInForce::Gtc) => "GTC".to_string(),
        Ok(execution::TimeInForce::Ioc) => "IOC".to_string(),
        Ok(execution::TimeInForce::Fok) => "FOK".to_string(),
        Ok(execution::TimeInForce::Day) => "DAY".to_string(),
        Ok(execution::TimeInForce::Gtd) => "GTD".to_string(),
        _ => "GTC".to_string(),
    }
}

fn order_status_to_string(status: i32) -> String {
    match execution::OrderStatus::try_from(status) {
        Ok(execution::OrderStatus::Pending) => "PENDING".to_string(),
        Ok(execution::OrderStatus::Sent) => "SENT".to_string(),
        Ok(execution::OrderStatus::Acknowledged) => "ACKNOWLEDGED".to_string(),
        Ok(execution::OrderStatus::PartiallyFilled) => "PARTIALLY_FILLED".to_string(),
        Ok(execution::OrderStatus::Filled) => "FILLED".to_string(),
        Ok(execution::OrderStatus::Cancelled) => "CANCELLED".to_string(),
        Ok(execution::OrderStatus::Rejected) => "REJECTED".to_string(),
        Ok(execution::OrderStatus::Expired) => "EXPIRED".to_string(),
        _ => "UNKNOWN".to_string(),
    }
}

fn parse_fixed_point(value: &str) -> Option<i64> {
    // SAFETY: Conversion from f64 to i64 for fixed-point representation
    #[allow(clippy::cast_possible_truncation)]
    value.parse::<f64>().ok().map(|v| (v * 10000.0) as i64)
}

#[allow(clippy::cast_precision_loss)]
fn fixed_point_to_string(value: i64) -> String {
    // SAFETY: i64 to f64 for display purposes - API boundary
    format!("{:.4}", value as f64 / 10000.0)
}
