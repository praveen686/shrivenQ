//! Models unit tests

use rstest::*;
use serde_json;
use chrono::Utc;

use api_gateway::models::{
    ApiResponse, ErrorResponse, LoginRequest, LoginResponse, RefreshTokenRequest,
    SubmitOrderRequest, SubmitOrderResponse, CancelOrderRequest, OrderStatusResponse,
    CheckOrderRequest, CheckOrderResponse, RiskMetrics, KillSwitchRequest, KillSwitchResponse,
    WebSocketMessage, HealthCheckResponse, FillInfo, PositionInfo, PositionResponse,
};

#[rstest]
fn test_api_response_success() {
    let data = "test data".to_string();
    let response = ApiResponse::success(data.clone());
    
    assert!(response.success);
    assert_eq!(response.data.unwrap(), data);
    assert!(response.error.is_none());
    assert!(response.timestamp > 0);
}

#[rstest]
fn test_api_response_error() {
    let error = ErrorResponse {
        error: "TEST_ERROR".to_string(),
        message: "Test error message".to_string(),
        details: None,
    };
    
    let response = ApiResponse::<String>::error(error.clone());
    
    assert!(!response.success);
    assert!(response.data.is_none());
    assert!(response.error.is_some());
    assert_eq!(response.error.unwrap().error, error.error);
    assert!(response.timestamp > 0);
}

#[rstest]
fn test_login_request_serialization() {
    let login_request = LoginRequest {
        username: "testuser".to_string(),
        password: "testpass".to_string(),
        exchange: Some("ZERODHA".to_string()),
    };
    
    let serialized = serde_json::to_string(&login_request).unwrap();
    let deserialized: LoginRequest = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(login_request.username, deserialized.username);
    assert_eq!(login_request.password, deserialized.password);
    assert_eq!(login_request.exchange, deserialized.exchange);
}

#[rstest]
fn test_login_request_optional_exchange() {
    let login_request = LoginRequest {
        username: "testuser".to_string(),
        password: "testpass".to_string(),
        exchange: None,
    };
    
    let serialized = serde_json::to_string(&login_request).unwrap();
    let deserialized: LoginRequest = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(login_request.username, deserialized.username);
    assert_eq!(login_request.password, deserialized.password);
    assert!(deserialized.exchange.is_none());
}

#[rstest]
fn test_login_response_serialization() {
    let login_response = LoginResponse {
        token: "jwt-token".to_string(),
        refresh_token: "refresh-token".to_string(),
        expires_at: Utc::now().timestamp(),
        permissions: vec!["PLACE_ORDERS".to_string(), "VIEW_POSITIONS".to_string()],
    };
    
    let serialized = serde_json::to_string(&login_response).unwrap();
    let deserialized: LoginResponse = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(login_response.token, deserialized.token);
    assert_eq!(login_response.refresh_token, deserialized.refresh_token);
    assert_eq!(login_response.expires_at, deserialized.expires_at);
    assert_eq!(login_response.permissions, deserialized.permissions);
}

#[rstest]
fn test_submit_order_request_serialization() {
    let mut params = rustc_hash::FxHashMap::default();
    params.insert("key1".to_string(), "value1".to_string());
    params.insert("key2".to_string(), "value2".to_string());
    
    let order_request = SubmitOrderRequest {
        client_order_id: Some("TEST001".to_string()),
        symbol: "NIFTY2412050000CE".to_string(),
        side: "BUY".to_string(),
        quantity: "100.0000".to_string(),
        order_type: "LIMIT".to_string(),
        limit_price: Some("150.2500".to_string()),
        stop_price: Some("149.0000".to_string()),
        time_in_force: Some("GTC".to_string()),
        venue: Some("NSE".to_string()),
        strategy_id: Some("strategy_1".to_string()),
        params: Some(params.clone()),
    };
    
    let serialized = serde_json::to_string(&order_request).unwrap();
    let deserialized: SubmitOrderRequest = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(order_request.client_order_id, deserialized.client_order_id);
    assert_eq!(order_request.symbol, deserialized.symbol);
    assert_eq!(order_request.side, deserialized.side);
    assert_eq!(order_request.quantity, deserialized.quantity);
    assert_eq!(order_request.order_type, deserialized.order_type);
    assert_eq!(order_request.limit_price, deserialized.limit_price);
    assert_eq!(order_request.stop_price, deserialized.stop_price);
    assert_eq!(order_request.time_in_force, deserialized.time_in_force);
    assert_eq!(order_request.venue, deserialized.venue);
    assert_eq!(order_request.strategy_id, deserialized.strategy_id);
    assert_eq!(order_request.params, deserialized.params);
}

#[rstest]
fn test_submit_order_request_minimal() {
    let order_request = SubmitOrderRequest {
        client_order_id: None,
        symbol: "BTCUSDT".to_string(),
        side: "BUY".to_string(),
        quantity: "1.0000".to_string(),
        order_type: "MARKET".to_string(),
        limit_price: None,
        stop_price: None,
        time_in_force: None,
        venue: None,
        strategy_id: None,
        params: None,
    };
    
    let serialized = serde_json::to_string(&order_request).unwrap();
    let deserialized: SubmitOrderRequest = serde_json::from_str(&serialized).unwrap();
    
    assert!(deserialized.client_order_id.is_none());
    assert_eq!(deserialized.symbol, "BTCUSDT");
    assert_eq!(deserialized.side, "BUY");
    assert_eq!(deserialized.order_type, "MARKET");
    assert!(deserialized.limit_price.is_none());
    assert!(deserialized.params.is_none());
}

#[rstest]
fn test_cancel_order_request_variants() {
    // Test with order ID
    let cancel_by_id = CancelOrderRequest {
        order_id: Some(12345),
        client_order_id: None,
    };
    
    let serialized = serde_json::to_string(&cancel_by_id).unwrap();
    let deserialized: CancelOrderRequest = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.order_id, Some(12345));
    assert!(deserialized.client_order_id.is_none());
    
    // Test with client order ID
    let cancel_by_client_id = CancelOrderRequest {
        order_id: None,
        client_order_id: Some("TEST001".to_string()),
    };
    
    let serialized = serde_json::to_string(&cancel_by_client_id).unwrap();
    let deserialized: CancelOrderRequest = serde_json::from_str(&serialized).unwrap();
    assert!(deserialized.order_id.is_none());
    assert_eq!(deserialized.client_order_id, Some("TEST001".to_string()));
}

#[rstest]
fn test_order_status_response_with_fills() {
    let fills = vec![
        FillInfo {
            fill_id: "fill1".to_string(),
            quantity: "50.0000".to_string(),
            price: "150.0000".to_string(),
            timestamp: Utc::now().timestamp(),
            is_maker: true,
            commission: "0.0100".to_string(),
            commission_asset: "USDT".to_string(),
        },
        FillInfo {
            fill_id: "fill2".to_string(),
            quantity: "50.0000".to_string(),
            price: "150.5000".to_string(),
            timestamp: Utc::now().timestamp(),
            is_maker: false,
            commission: "0.0100".to_string(),
            commission_asset: "USDT".to_string(),
        },
    ];
    
    let order_status = OrderStatusResponse {
        order_id: 12345,
        client_order_id: "TEST001".to_string(),
        exchange_order_id: "EXCH123".to_string(),
        symbol: "BTCUSDT".to_string(),
        side: "BUY".to_string(),
        quantity: "100.0000".to_string(),
        filled_quantity: "100.0000".to_string(),
        avg_fill_price: "150.2500".to_string(),
        status: "FILLED".to_string(),
        order_type: "LIMIT".to_string(),
        limit_price: Some("151.0000".to_string()),
        stop_price: None,
        time_in_force: "GTC".to_string(),
        venue: "BINANCE".to_string(),
        strategy_id: Some("strategy_1".to_string()),
        created_at: Utc::now().timestamp(),
        updated_at: Utc::now().timestamp(),
        fills: fills.clone(),
    };
    
    let serialized = serde_json::to_string(&order_status).unwrap();
    let deserialized: OrderStatusResponse = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(deserialized.order_id, 12345);
    assert_eq!(deserialized.fills.len(), 2);
    assert_eq!(deserialized.fills[0].fill_id, fills[0].fill_id);
    assert_eq!(deserialized.fills[1].quantity, fills[1].quantity);
}

#[rstest]
fn test_check_order_request() {
    let check_request = CheckOrderRequest {
        symbol: "NIFTY2412050000CE".to_string(),
        side: "BUY".to_string(),
        quantity: "100.0000".to_string(),
        price: "150.2500".to_string(),
        strategy_id: Some("strategy_1".to_string()),
        exchange: "NSE".to_string(),
    };
    
    let serialized = serde_json::to_string(&check_request).unwrap();
    let deserialized: CheckOrderRequest = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(deserialized.symbol, "NIFTY2412050000CE");
    assert_eq!(deserialized.side, "BUY");
    assert_eq!(deserialized.quantity, "100.0000");
    assert_eq!(deserialized.price, "150.2500");
    assert_eq!(deserialized.strategy_id, Some("strategy_1".to_string()));
    assert_eq!(deserialized.exchange, "NSE");
}

#[rstest]
fn test_risk_metrics_serialization() {
    let risk_metrics = RiskMetrics {
        total_exposure: "1000000.0000".to_string(),
        current_drawdown: "5.2500".to_string(),
        daily_pnl: "-1250.7500".to_string(),
        open_positions: 15,
        orders_today: 123,
        circuit_breaker_active: false,
        kill_switch_active: false,
    };
    
    let serialized = serde_json::to_string(&risk_metrics).unwrap();
    let deserialized: RiskMetrics = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(deserialized.total_exposure, "1000000.0000");
    assert_eq!(deserialized.current_drawdown, "5.2500");
    assert_eq!(deserialized.daily_pnl, "-1250.7500");
    assert_eq!(deserialized.open_positions, 15);
    assert_eq!(deserialized.orders_today, 123);
    assert!(!deserialized.circuit_breaker_active);
    assert!(!deserialized.kill_switch_active);
}

#[rstest]
fn test_kill_switch_request_activate() {
    let kill_switch_request = KillSwitchRequest {
        activate: true,
        reason: Some("Emergency stop".to_string()),
    };
    
    let serialized = serde_json::to_string(&kill_switch_request).unwrap();
    let deserialized: KillSwitchRequest = serde_json::from_str(&serialized).unwrap();
    
    assert!(deserialized.activate);
    assert_eq!(deserialized.reason, Some("Emergency stop".to_string()));
}

#[rstest]
fn test_kill_switch_request_deactivate() {
    let kill_switch_request = KillSwitchRequest {
        activate: false,
        reason: None,
    };
    
    let serialized = serde_json::to_string(&kill_switch_request).unwrap();
    let deserialized: KillSwitchRequest = serde_json::from_str(&serialized).unwrap();
    
    assert!(!deserialized.activate);
    assert!(deserialized.reason.is_none());
}

#[rstest]
fn test_websocket_message_serialization() {
    let ws_message = WebSocketMessage {
        message_type: "market_data_update".to_string(),
        data: serde_json::json!({
            "symbol": "BTCUSDT",
            "price": "50000.0000",
            "volume": "1.2345",
            "timestamp": 1640995200
        }),
        timestamp: Utc::now().timestamp(),
    };
    
    let serialized = serde_json::to_string(&ws_message).unwrap();
    let deserialized: WebSocketMessage = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(deserialized.message_type, "market_data_update");
    assert_eq!(deserialized.data["symbol"], "BTCUSDT");
    assert_eq!(deserialized.data["price"], "50000.0000");
    assert!(deserialized.timestamp > 0);
}

#[rstest]
fn test_health_check_response() {
    let mut services = rustc_hash::FxHashMap::default();
    services.insert("auth".to_string(), true);
    services.insert("execution".to_string(), true);
    services.insert("market_data".to_string(), false);
    
    let health_response = HealthCheckResponse {
        status: "DEGRADED".to_string(),
        services,
        version: "1.0.0".to_string(),
        uptime_seconds: 3600,
    };
    
    let serialized = serde_json::to_string(&health_response).unwrap();
    let deserialized: HealthCheckResponse = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(deserialized.status, "DEGRADED");
    assert_eq!(deserialized.version, "1.0.0");
    assert_eq!(deserialized.uptime_seconds, 3600);
    assert_eq!(deserialized.services.len(), 3);
    assert_eq!(deserialized.services["auth"], true);
    assert_eq!(deserialized.services["market_data"], false);
}

#[rstest]
fn test_position_response_with_multiple_positions() {
    let positions = vec![
        PositionInfo {
            symbol: "BTCUSDT".to_string(),
            net_quantity: "1.5000".to_string(),
            avg_price: "45000.0000".to_string(),
            mark_price: "50000.0000".to_string(),
            unrealized_pnl: "7500.0000".to_string(),
            realized_pnl: "1250.0000".to_string(),
            position_value: "75000.0000".to_string(),
            exchange: "BINANCE".to_string(),
        },
        PositionInfo {
            symbol: "ETHUSDT".to_string(),
            net_quantity: "-2.0000".to_string(),
            avg_price: "3200.0000".to_string(),
            mark_price: "3100.0000".to_string(),
            unrealized_pnl: "200.0000".to_string(),
            realized_pnl: "-150.0000".to_string(),
            position_value: "-6200.0000".to_string(),
            exchange: "BINANCE".to_string(),
        },
    ];
    
    let position_response = PositionResponse {
        positions: positions.clone(),
        total_exposure: "81200.0000".to_string(),
    };
    
    let serialized = serde_json::to_string(&position_response).unwrap();
    let deserialized: PositionResponse = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(deserialized.positions.len(), 2);
    assert_eq!(deserialized.positions[0].symbol, "BTCUSDT");
    assert_eq!(deserialized.positions[1].symbol, "ETHUSDT");
    assert_eq!(deserialized.total_exposure, "81200.0000");
}

#[rstest]
fn test_error_response_with_details() {
    let mut details = rustc_hash::FxHashMap::default();
    details.insert("field".to_string(), "symbol".to_string());
    details.insert("value".to_string(), "INVALID_SYMBOL".to_string());
    
    let error_response = ErrorResponse {
        error: "VALIDATION_ERROR".to_string(),
        message: "Invalid symbol provided".to_string(),
        details: Some(details.clone()),
    };
    
    let serialized = serde_json::to_string(&error_response).unwrap();
    let deserialized: ErrorResponse = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(deserialized.error, "VALIDATION_ERROR");
    assert_eq!(deserialized.message, "Invalid symbol provided");
    assert!(deserialized.details.is_some());
    assert_eq!(deserialized.details.unwrap()["field"], "symbol");
}

#[rstest]
fn test_error_response_without_details() {
    let error_response = ErrorResponse {
        error: "INTERNAL_ERROR".to_string(),
        message: "An internal error occurred".to_string(),
        details: None,
    };
    
    let serialized = serde_json::to_string(&error_response).unwrap();
    let deserialized: ErrorResponse = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(deserialized.error, "INTERNAL_ERROR");
    assert_eq!(deserialized.message, "An internal error occurred");
    assert!(deserialized.details.is_none());
}

#[rstest]
fn test_fixed_point_string_formatting() {
    // Test that numeric strings maintain proper formatting
    let order_request = SubmitOrderRequest {
        client_order_id: None,
        symbol: "TEST".to_string(),
        side: "BUY".to_string(),
        quantity: "100.0000".to_string(), // 4 decimal places
        order_type: "LIMIT".to_string(),
        limit_price: Some("150.2500".to_string()), // 4 decimal places
        stop_price: None,
        time_in_force: None,
        venue: None,
        strategy_id: None,
        params: None,
    };
    
    let serialized = serde_json::to_string(&order_request).unwrap();
    
    // Should preserve the exact string formatting
    assert!(serialized.contains("100.0000"));
    assert!(serialized.contains("150.2500"));
    
    let deserialized: SubmitOrderRequest = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.quantity, "100.0000");
    assert_eq!(deserialized.limit_price.unwrap(), "150.2500");
}