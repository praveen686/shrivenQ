//! REST API models and request/response types

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// Authentication models
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub exchange: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

/// Order management models
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitOrderRequest {
    pub client_order_id: Option<String>,
    pub symbol: String,
    pub side: String,                  // "BUY" or "SELL"
    pub quantity: String,              // Fixed-point as string to preserve precision
    pub order_type: String,            // "MARKET", "LIMIT", etc.
    pub limit_price: Option<String>,   // Fixed-point as string
    pub stop_price: Option<String>,    // Fixed-point as string
    pub time_in_force: Option<String>, // "GTC", "IOC", "FOK", "DAY"
    pub venue: Option<String>,
    pub strategy_id: Option<String>,
    pub params: Option<FxHashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitOrderResponse {
    pub order_id: i64,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CancelOrderRequest {
    pub order_id: Option<i64>,
    pub client_order_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderStatusResponse {
    pub order_id: i64,
    pub client_order_id: String,
    pub exchange_order_id: String,
    pub symbol: String,
    pub side: String,
    pub quantity: String,        // Fixed-point as string
    pub filled_quantity: String, // Fixed-point as string
    pub avg_fill_price: String,  // Fixed-point as string
    pub status: String,
    pub order_type: String,
    pub limit_price: Option<String>, // Fixed-point as string
    pub stop_price: Option<String>,  // Fixed-point as string
    pub time_in_force: String,
    pub venue: String,
    pub strategy_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub fills: Vec<FillInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FillInfo {
    pub fill_id: String,
    pub quantity: String, // Fixed-point as string
    pub price: String,    // Fixed-point as string
    pub timestamp: i64,
    pub is_maker: bool,
    pub commission: String, // Fixed-point as string
    pub commission_asset: String,
}

/// Market data models
#[derive(Debug, Serialize, Deserialize)]
pub struct MarketDataSubscriptionRequest {
    pub symbols: Vec<String>,
    pub data_types: Vec<String>, // "ORDER_BOOK", "TRADES", "QUOTES", "CANDLES"
    pub exchange: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetSnapshotRequest {
    pub symbols: Vec<String>,
    pub exchange: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MarketSnapshot {
    pub symbol: String,
    pub timestamp_nanos: i64,
    pub order_book: Option<OrderBookSnapshot>,
    pub quote: Option<QuoteSnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub sequence: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: String,    // Fixed-point as string
    pub quantity: String, // Fixed-point as string
    pub count: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QuoteSnapshot {
    pub bid_price: String, // Fixed-point as string
    pub bid_size: String,  // Fixed-point as string
    pub ask_price: String, // Fixed-point as string
    pub ask_size: String,  // Fixed-point as string
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetHistoricalDataRequest {
    pub symbol: String,
    pub exchange: String,
    pub start_time: i64,
    pub end_time: i64,
    pub data_type: String,
    pub interval: Option<String>,
}

/// Risk management models
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckOrderRequest {
    pub symbol: String,
    pub side: String,
    pub quantity: String, // Fixed-point as string
    pub price: String,    // Fixed-point as string
    pub strategy_id: Option<String>,
    pub exchange: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckOrderResponse {
    pub result: String, // "APPROVED", "REJECTED", "REQUIRES_APPROVAL"
    pub reason: Option<String>,
    pub current_metrics: RiskMetrics,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RiskMetrics {
    pub total_exposure: String,   // Fixed-point as string
    pub current_drawdown: String, // Fixed-point percentage as string
    pub daily_pnl: String,        // Fixed-point as string
    pub open_positions: i32,
    pub orders_today: i32,
    pub circuit_breaker_active: bool,
    pub kill_switch_active: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PositionResponse {
    pub positions: Vec<PositionInfo>,
    pub total_exposure: String, // Fixed-point as string
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PositionInfo {
    pub symbol: String,
    pub net_quantity: String,   // Fixed-point as string
    pub avg_price: String,      // Fixed-point as string
    pub mark_price: String,     // Fixed-point as string
    pub unrealized_pnl: String, // Fixed-point as string
    pub realized_pnl: String,   // Fixed-point as string
    pub position_value: String, // Fixed-point as string
    pub exchange: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KillSwitchRequest {
    pub activate: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KillSwitchResponse {
    pub success: bool,
    pub is_active: bool,
}

/// Error response model
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
    pub details: Option<FxHashMap<String, String>>,
}

/// Generic API response wrapper
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ErrorResponse>,
    pub timestamp: i64,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    #[must_use] pub fn error(error: ErrorResponse) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketMessage {
    pub message_type: String,
    pub data: serde_json::Value,
    pub timestamp: i64,
}

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub status: String,
    pub services: FxHashMap<String, bool>,
    pub version: String,
    pub uptime_seconds: u64,
}
