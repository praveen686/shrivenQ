//! REST API models and request/response types

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// Authentication models
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    /// Username for authentication
    pub username: String,
    /// Password for authentication
    pub password: String,
    /// Optional exchange identifier
    pub exchange: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    /// JWT access token
    pub token: String,
    /// Refresh token for obtaining new access tokens
    pub refresh_token: String,
    /// Token expiration timestamp
    pub expires_at: i64,
    /// User permissions
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshTokenRequest {
    /// Refresh token to exchange for new access token
    pub refresh_token: String,
}

/// Order management models
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitOrderRequest {
    /// Optional client-generated order ID
    pub client_order_id: Option<String>,
    /// Trading symbol (e.g., "BTCUSDT")
    pub symbol: String,
    /// Order side: "BUY" or "SELL"
    pub side: String,
    /// Order quantity as fixed-point string
    pub quantity: String,
    /// Order type: "MARKET", "LIMIT", etc.
    pub order_type: String,
    /// Limit price as fixed-point string (for limit orders)
    pub limit_price: Option<String>,
    /// Stop price as fixed-point string (for stop orders)
    pub stop_price: Option<String>,
    /// Time in force: "GTC", "IOC", "FOK", "DAY"
    pub time_in_force: Option<String>,
    /// Target venue/exchange
    pub venue: Option<String>,
    /// Strategy identifier
    pub strategy_id: Option<String>,
    /// Additional order parameters
    pub params: Option<FxHashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitOrderResponse {
    /// Generated order ID
    pub order_id: i64,
    /// Order submission status
    pub status: String,
    /// Status message
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CancelOrderRequest {
    /// Order ID to cancel
    pub order_id: Option<i64>,
    /// Client order ID to cancel
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
    /// Whether to activate the kill switch
    pub activate: bool,
    /// Reason for activation/deactivation
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KillSwitchResponse {
    /// Whether the operation was successful
    pub success: bool,
    /// Current kill switch status
    pub is_active: bool,
}

/// Error response model
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error code identifier
    pub error: String,
    /// Human-readable error message
    pub message: String,
    /// Additional error details
    pub details: Option<FxHashMap<String, String>>,
}

/// Generic API response wrapper
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// Whether the request was successful
    pub success: bool,
    /// Response data (if successful)
    pub data: Option<T>,
    /// Error details (if failed)
    pub error: Option<ErrorResponse>,
    /// Response timestamp
    pub timestamp: i64,
}

impl<T> ApiResponse<T> {
    /// Create a successful API response
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// Create an error API response
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
    /// Message type identifier
    pub message_type: String,
    /// Message payload
    pub data: serde_json::Value,
    /// Message timestamp
    pub timestamp: i64,
}

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    /// Overall health status
    pub status: String,
    /// Service health status map
    pub services: FxHashMap<String, bool>,
    /// Service version
    pub version: String,
    /// Service uptime in seconds
    pub uptime_seconds: u64,
}
