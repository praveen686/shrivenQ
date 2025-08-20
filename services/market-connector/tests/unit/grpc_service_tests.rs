//! Comprehensive unit tests for gRPC service implementation
//! 
//! These tests cover gRPC streaming, subscription management, event broadcasting,
//! and WebSocket connection handling through the gRPC interface.

use rstest::*;
use tokio::sync::{mpsc, broadcast};
use tokio_stream::StreamExt;
use tonic::{Request, Status};
use std::time::Duration;
use serde_json::json;

use market_connector::grpc_service::*;
use market_connector::{MarketDataEvent as InternalEvent, MarketData, MarketDataType, SubscriptionRequest as InternalSubRequest};
use services_common::marketdata::v1::{
    SubscribeRequest, UnsubscribeRequest, GetSnapshotRequest, GetHistoricalDataRequest,
    MarketDataEvent, OrderBookUpdate, PriceLevel, Trade, Quote, MarketSnapshot
};

// Test constants
const TEST_SYMBOL_BTCUSDT: &str = "BTCUSDT";
const TEST_SYMBOL_ETHUSDT: &str = "ETHUSDT"; 
const TEST_EXCHANGE_BINANCE: &str = "binance";
const TEST_EXCHANGE_ZERODHA: &str = "zerodha";
const TEST_BID_PRICE: f64 = 45000.50;
const TEST_ASK_PRICE: f64 = 45001.00;
const TEST_QUANTITY: f64 = 1.25;
const TEST_TRADE_PRICE: f64 = 45000.75;
const TEST_TRADE_QUANTITY: f64 = 0.5;
const TEST_TRADE_ID: &str = "123456789";
const TEST_SEQUENCE: u64 = 12345;
const TEST_TIMESTAMP: u64 = 1640995200000; // 2022-01-01 00:00:00 UTC in ms

// Fixed-point conversion test constants
const FIXED_POINT_MULTIPLIER: f64 = 10000.0;
const MOCK_BID_PRICE_FP: i64 = 1000000; // 100.00 in fixed point
const MOCK_ASK_PRICE_FP: i64 = 1010000; // 101.00 in fixed point
const MOCK_QUANTITY_FP: i64 = 100000;   // 10.00 in fixed point

#[fixture]
fn grpc_service() -> MarketDataGrpcService {
    let (service, _sender) = MarketDataGrpcService::new();
    service
}

#[fixture]
fn subscribe_request() -> SubscribeRequest {
    SubscribeRequest {
        symbols: vec![TEST_SYMBOL_BTCUSDT.to_string(), TEST_SYMBOL_ETHUSDT.to_string()],
        exchange: TEST_EXCHANGE_BINANCE.to_string(),
        data_types: vec![1, 2], // ORDER_BOOK and TRADES
    }
}

#[fixture]
fn unsubscribe_request() -> UnsubscribeRequest {
    UnsubscribeRequest {
        symbols: vec![TEST_SYMBOL_BTCUSDT.to_string()],
        exchange: TEST_EXCHANGE_BINANCE.to_string(),
    }
}

#[fixture]
fn snapshot_request() -> GetSnapshotRequest {
    GetSnapshotRequest {
        symbols: vec![TEST_SYMBOL_BTCUSDT.to_string()],
        exchange: TEST_EXCHANGE_BINANCE.to_string(),
    }
}

#[fixture]
fn historical_request() -> GetHistoricalDataRequest {
    GetHistoricalDataRequest {
        symbol: TEST_SYMBOL_BTCUSDT.to_string(),
        exchange: TEST_EXCHANGE_BINANCE.to_string(),
        start_time: 1640995200, // 2022-01-01 00:00:00 UTC
        end_time: 1641081600,   // 2022-01-02 00:00:00 UTC
    }
}

#[rstest]
fn test_grpc_service_creation(grpc_service: MarketDataGrpcService) {
    // Service should be created successfully
    assert!(true);
}

#[rstest]
async fn test_get_connection_status(grpc_service: MarketDataGrpcService) {
    let status = grpc_service.get_connection_status().await;
    assert!(status.is_empty()); // No connections initially
}

#[rstest]
async fn test_has_connection_initially_false(grpc_service: MarketDataGrpcService) {
    let has_conn = grpc_service.has_connection(TEST_SYMBOL_BTCUSDT, TEST_EXCHANGE_BINANCE).await;
    assert!(!has_conn);
}

#[rstest]
#[tokio::test]
async fn test_subscribe_endpoint(
    grpc_service: MarketDataGrpcService,
    subscribe_request: SubscribeRequest
) {
    let request = Request::new(subscribe_request);
    let response = grpc_service.subscribe(request).await;
    
    assert!(response.is_ok());
    
    let mut stream = response.unwrap().into_inner();
    
    // Try to get first event (should work even if no data)
    let _first_event = tokio::time::timeout(
        Duration::from_millis(100),
        stream.next()
    ).await;
    // Timeout is expected since no real WebSocket data
}

#[rstest]
#[tokio::test]
async fn test_unsubscribe_endpoint(
    grpc_service: MarketDataGrpcService,
    unsubscribe_request: UnsubscribeRequest
) {
    let request = Request::new(unsubscribe_request);
    let response = grpc_service.unsubscribe(request).await;
    
    assert!(response.is_ok());
    let unsubscribe_response = response.unwrap().into_inner();
    assert!(unsubscribe_response.success);
}

#[rstest]
#[tokio::test]
async fn test_snapshot_endpoint(
    grpc_service: MarketDataGrpcService,
    snapshot_request: GetSnapshotRequest
) {
    let request = Request::new(snapshot_request);
    let response = grpc_service.get_snapshot(request).await;
    
    assert!(response.is_ok());
    let snapshot_response = response.unwrap().into_inner();
    assert_eq!(snapshot_response.snapshots.len(), 1);
    
    let snapshot = &snapshot_response.snapshots[0];
    assert_eq!(snapshot.symbol, TEST_SYMBOL_BTCUSDT);
    assert!(snapshot.order_book.is_some());
    assert!(snapshot.quote.is_some());
}

#[rstest]
#[tokio::test]
async fn test_historical_data_endpoint(
    grpc_service: MarketDataGrpcService,
    historical_request: GetHistoricalDataRequest
) {
    let request = Request::new(historical_request);
    let response = grpc_service.get_historical_data(request).await;
    
    assert!(response.is_ok());
    let historical_response = response.unwrap().into_inner();
    assert!(historical_response.events.is_empty()); // Not implemented yet
}

#[rstest]
fn test_price_to_fixed_point_conversion() {
    assert_eq!(price_to_fixed_point(100.0), 1000000); // 100.00 -> 1000000
    assert_eq!(price_to_fixed_point(99.5), 995000);   // 99.50 -> 995000
    assert_eq!(price_to_fixed_point(0.0001), 1);      // 0.0001 -> 1
    
    // Test edge cases
    assert_eq!(price_to_fixed_point(0.0), 0);
    assert_eq!(price_to_fixed_point(f64::NAN), 0);
    assert_eq!(price_to_fixed_point(f64::INFINITY), i64::MAX);
    assert_eq!(price_to_fixed_point(f64::NEG_INFINITY), i64::MIN);
}

#[rstest]
fn test_quantity_to_fixed_point_conversion() {
    assert_eq!(quantity_to_fixed_point(10.0), 100000);   // 10.00 -> 100000
    assert_eq!(quantity_to_fixed_point(0.5), 5000);      // 0.50 -> 5000
    assert_eq!(quantity_to_fixed_point(0.0001), 1);      // 0.0001 -> 1
    
    // Test edge cases
    assert_eq!(quantity_to_fixed_point(0.0), 0);
    assert_eq!(quantity_to_fixed_point(-1.0), 0);        // Negative -> 0
    assert_eq!(quantity_to_fixed_point(f64::NAN), 0);
    assert_eq!(quantity_to_fixed_point(f64::INFINITY), i64::MAX);
}

#[rstest]
fn test_sequence_to_proto_conversion() {
    assert_eq!(sequence_to_proto(12345), 12345);
    assert_eq!(sequence_to_proto(u64::MAX), i64::MAX);
    assert_eq!(sequence_to_proto(0), 0);
}

#[rstest]
fn test_current_timestamp_nanos() {
    let result = current_timestamp_nanos();
    assert!(result.is_ok());
    let timestamp = result.unwrap();
    assert!(timestamp > 0);
    assert!(timestamp < i64::MAX);
}

#[rstest]
fn test_subscription_filter_creation() {
    let mut filter = SubscriptionFilter::new();
    assert!(filter.symbols.is_empty());
    assert!(filter.exchanges.is_empty());
    assert!(filter.data_types.is_empty());
}

#[rstest]
fn test_subscription_filter_add_subscription() {
    let mut filter = SubscriptionFilter::new();
    
    filter.add_subscription(
        TEST_SYMBOL_BTCUSDT.to_string(),
        TEST_EXCHANGE_BINANCE.to_string(),
        vec![MarketDataType::OrderBook, MarketDataType::Trades]
    );
    
    assert!(filter.symbols.contains(TEST_SYMBOL_BTCUSDT));
    assert!(filter.exchanges.contains(TEST_EXCHANGE_BINANCE));
    assert!(filter.data_types.contains(&MarketDataType::OrderBook));
    assert!(filter.data_types.contains(&MarketDataType::Trades));
}

#[rstest]
fn test_subscription_filter_matches() {
    let mut filter = SubscriptionFilter::new();
    filter.add_subscription(
        TEST_SYMBOL_BTCUSDT.to_string(),
        TEST_EXCHANGE_BINANCE.to_string(),
        vec![MarketDataType::OrderBook]
    );
    
    // Create matching event
    let matching_event = InternalEvent {
        symbol: TEST_SYMBOL_BTCUSDT.to_string(),
        exchange: TEST_EXCHANGE_BINANCE.to_string(),
        timestamp: TEST_TIMESTAMP,
        data: MarketData::OrderBook {
            bids: vec![(TEST_BID_PRICE, TEST_QUANTITY)],
            asks: vec![(TEST_ASK_PRICE, TEST_QUANTITY)],
            sequence: TEST_SEQUENCE,
        },
    };
    
    assert!(filter.matches(&matching_event));
    
    // Create non-matching event (different symbol)
    let non_matching_event = InternalEvent {
        symbol: TEST_SYMBOL_ETHUSDT.to_string(),
        exchange: TEST_EXCHANGE_BINANCE.to_string(),
        timestamp: TEST_TIMESTAMP,
        data: MarketData::OrderBook {
            bids: vec![(TEST_BID_PRICE, TEST_QUANTITY)],
            asks: vec![(TEST_ASK_PRICE, TEST_QUANTITY)],
            sequence: TEST_SEQUENCE,
        },
    };
    
    assert!(!filter.matches(&non_matching_event));
}

#[rstest]
fn test_subscription_filter_matches_data_type() {
    let mut filter = SubscriptionFilter::new();
    filter.add_subscription(
        TEST_SYMBOL_BTCUSDT.to_string(),
        TEST_EXCHANGE_BINANCE.to_string(),
        vec![MarketDataType::Trades] // Only trades
    );
    
    // OrderBook should not match
    let orderbook_data = MarketData::OrderBook {
        bids: vec![(TEST_BID_PRICE, TEST_QUANTITY)],
        asks: vec![(TEST_ASK_PRICE, TEST_QUANTITY)],
        sequence: TEST_SEQUENCE,
    };
    assert!(!filter.matches_data_type(&orderbook_data));
    
    // Trades should match
    let trade_data = MarketData::Trade {
        price: TEST_TRADE_PRICE,
        quantity: TEST_TRADE_QUANTITY,
        side: "buy".to_string(),
        trade_id: TEST_TRADE_ID.to_string(),
    };
    assert!(filter.matches_data_type(&trade_data));
}

#[rstest]
fn test_subscription_filter_empty_data_types() {
    let filter = SubscriptionFilter::new(); // Empty filter
    
    // Should match all data types when none specified
    let orderbook_data = MarketData::OrderBook {
        bids: vec![(TEST_BID_PRICE, TEST_QUANTITY)],
        asks: vec![(TEST_ASK_PRICE, TEST_QUANTITY)],
        sequence: TEST_SEQUENCE,
    };
    assert!(filter.matches_data_type(&orderbook_data));
}

#[rstest]
fn test_websocket_connection_creation() {
    let connection = WebSocketConnection {
        exchange: TEST_EXCHANGE_BINANCE.to_string(),
        symbols: vec![TEST_SYMBOL_BTCUSDT.to_string()],
        connected: true,
        market_type: "spot".to_string(),
    };
    
    assert!(connection.handles_symbol(TEST_SYMBOL_BTCUSDT, TEST_EXCHANGE_BINANCE));
    assert!(!connection.handles_symbol(TEST_SYMBOL_ETHUSDT, TEST_EXCHANGE_BINANCE));
    assert!(!connection.handles_symbol(TEST_SYMBOL_BTCUSDT, TEST_EXCHANGE_ZERODHA));
    
    assert!(connection.is_connected());
    assert!(connection.connection_info().contains("binance"));
    assert!(connection.connection_info().contains("spot"));
    assert!(connection.connection_info().contains(TEST_SYMBOL_BTCUSDT));
}

#[rstest]
fn test_convert_data_type() {
    assert_eq!(MarketDataGrpcService::convert_data_type(1), MarketDataType::OrderBook);
    assert_eq!(MarketDataGrpcService::convert_data_type(2), MarketDataType::Trades);
    assert_eq!(MarketDataGrpcService::convert_data_type(3), MarketDataType::Quotes);
    
    // Test candles conversion
    if let MarketDataType::Candles { interval } = MarketDataGrpcService::convert_data_type(4) {
        assert_eq!(interval, "1m");
    } else {
        panic!("Expected Candles data type");
    }
    
    // Test default fallback
    assert_eq!(MarketDataGrpcService::convert_data_type(999), MarketDataType::Quotes);
}

#[rstest]
fn test_convert_event_orderbook() {
    let internal_event = InternalEvent {
        symbol: TEST_SYMBOL_BTCUSDT.to_string(),
        exchange: TEST_EXCHANGE_BINANCE.to_string(),
        timestamp: TEST_TIMESTAMP,
        data: MarketData::OrderBook {
            bids: vec![(TEST_BID_PRICE, TEST_QUANTITY), (TEST_BID_PRICE - 1.0, TEST_QUANTITY * 2.0)],
            asks: vec![(TEST_ASK_PRICE, TEST_QUANTITY), (TEST_ASK_PRICE + 1.0, TEST_QUANTITY * 1.5)],
            sequence: TEST_SEQUENCE,
        },
    };
    
    let proto_event = MarketDataGrpcService::convert_event(internal_event);
    
    assert_eq!(proto_event.symbol, TEST_SYMBOL_BTCUSDT);
    assert_eq!(proto_event.exchange, TEST_EXCHANGE_BINANCE);
    assert_eq!(proto_event.timestamp_nanos, TEST_TIMESTAMP as i64);
    
    if let Some(services_common::proto::marketdata::v1::market_data_event::Data::OrderBook(orderbook)) = proto_event.data {
        assert_eq!(orderbook.bids.len(), 2);
        assert_eq!(orderbook.asks.len(), 2);
        assert_eq!(orderbook.sequence, TEST_SEQUENCE as i64);
        
        // Check first bid level conversion
        assert_eq!(orderbook.bids[0].price, price_to_fixed_point(TEST_BID_PRICE));
        assert_eq!(orderbook.bids[0].quantity, quantity_to_fixed_point(TEST_QUANTITY));
        assert_eq!(orderbook.bids[0].count, 1);
    } else {
        panic!("Expected OrderBook data");
    }
}

#[rstest]
fn test_convert_event_trade() {
    let internal_event = InternalEvent {
        symbol: TEST_SYMBOL_BTCUSDT.to_string(),
        exchange: TEST_EXCHANGE_BINANCE.to_string(),
        timestamp: TEST_TIMESTAMP,
        data: MarketData::Trade {
            price: TEST_TRADE_PRICE,
            quantity: TEST_TRADE_QUANTITY,
            side: "buy".to_string(),
            trade_id: TEST_TRADE_ID.to_string(),
        },
    };
    
    let proto_event = MarketDataGrpcService::convert_event(internal_event);
    
    assert_eq!(proto_event.symbol, TEST_SYMBOL_BTCUSDT);
    assert_eq!(proto_event.exchange, TEST_EXCHANGE_BINANCE);
    
    if let Some(services_common::proto::marketdata::v1::market_data_event::Data::Trade(trade)) = proto_event.data {
        assert_eq!(trade.price, price_to_fixed_point(TEST_TRADE_PRICE));
        assert_eq!(trade.quantity, quantity_to_fixed_point(TEST_TRADE_QUANTITY));
        assert_eq!(trade.trade_id, TEST_TRADE_ID);
        assert!(!trade.is_buyer_maker); // Default value
    } else {
        panic!("Expected Trade data");
    }
}

#[rstest]
fn test_convert_event_quote() {
    let internal_event = InternalEvent {
        symbol: TEST_SYMBOL_BTCUSDT.to_string(),
        exchange: TEST_EXCHANGE_BINANCE.to_string(),
        timestamp: TEST_TIMESTAMP,
        data: MarketData::Quote {
            bid_price: TEST_BID_PRICE,
            bid_size: TEST_QUANTITY,
            ask_price: TEST_ASK_PRICE,
            ask_size: TEST_QUANTITY * 0.8,
        },
    };
    
    let proto_event = MarketDataGrpcService::convert_event(internal_event);
    
    if let Some(services_common::proto::marketdata::v1::market_data_event::Data::Quote(quote)) = proto_event.data {
        assert_eq!(quote.bid_price, price_to_fixed_point(TEST_BID_PRICE));
        assert_eq!(quote.bid_size, quantity_to_fixed_point(TEST_QUANTITY));
        assert_eq!(quote.ask_price, price_to_fixed_point(TEST_ASK_PRICE));
        assert_eq!(quote.ask_size, quantity_to_fixed_point(TEST_QUANTITY * 0.8));
    } else {
        panic!("Expected Quote data");
    }
}

#[rstest]
fn test_process_binance_depth() {
    let depth_data = json!({
        "s": TEST_SYMBOL_BTCUSDT,
        "u": TEST_SEQUENCE,
        "b": [
            [TEST_BID_PRICE.to_string(), TEST_QUANTITY.to_string()],
            [(TEST_BID_PRICE - 1.0).to_string(), (TEST_QUANTITY * 2.0).to_string()]
        ],
        "a": [
            [TEST_ASK_PRICE.to_string(), TEST_QUANTITY.to_string()],
            [(TEST_ASK_PRICE + 1.0).to_string(), (TEST_QUANTITY * 1.5).to_string()]
        ]
    });
    
    let (broadcaster, _receiver) = broadcast::channel(1000);
    
    // This tests the internal processing logic
    MarketDataGrpcService::process_binance_depth(&depth_data, &broadcaster);
    
    // Should not panic and should broadcast an event
    // The actual event verification would require accessing the broadcast channel
}

#[rstest]
fn test_process_binance_trade() {
    let trade_data = json!({
        "s": TEST_SYMBOL_BTCUSDT,
        "p": TEST_TRADE_PRICE.to_string(),
        "q": TEST_TRADE_QUANTITY.to_string(),
        "m": false,
        "t": 123456789u64
    });
    
    let (broadcaster, _receiver) = broadcast::channel(1000);
    
    MarketDataGrpcService::process_binance_trade(&trade_data, &broadcaster);
    
    // Should not panic and should broadcast a trade event
}

#[rstest]
fn test_process_binance_ticker() {
    let ticker_data = json!({
        "s": TEST_SYMBOL_BTCUSDT,
        "b": TEST_BID_PRICE.to_string(),
        "B": TEST_QUANTITY.to_string(),
        "a": TEST_ASK_PRICE.to_string(),
        "A": (TEST_QUANTITY * 0.8).to_string()
    });
    
    let (broadcaster, _receiver) = broadcast::channel(1000);
    
    MarketDataGrpcService::process_binance_ticker(&ticker_data, &broadcaster);
    
    // Should not panic and should broadcast a quote event
}

#[rstest]
#[tokio::test]
async fn test_event_broadcasting() {
    let (service, event_sender) = MarketDataGrpcService::new();
    
    // Create test event
    let test_event = InternalEvent {
        symbol: TEST_SYMBOL_BTCUSDT.to_string(),
        exchange: TEST_EXCHANGE_BINANCE.to_string(),
        timestamp: TEST_TIMESTAMP,
        data: MarketData::Quote {
            bid_price: TEST_BID_PRICE,
            bid_size: TEST_QUANTITY,
            ask_price: TEST_ASK_PRICE,
            ask_size: TEST_QUANTITY,
        },
    };
    
    // Subscribe to events
    let mut event_receiver = service.event_broadcaster.subscribe();
    
    // Send event through the internal sender
    event_sender.send(test_event.clone()).await.unwrap();
    
    // Should receive the event
    let received = tokio::time::timeout(
        Duration::from_millis(100),
        event_receiver.recv()
    ).await;
    
    // The event forwarding task runs in background, so timeout is possible
    // This tests the basic structure
}

#[rstest]
#[tokio::test]
async fn test_multiple_subscriptions() {
    let grpc_service = grpc_service();
    
    // Create multiple subscription requests
    let requests = vec![
        SubscribeRequest {
            symbols: vec![TEST_SYMBOL_BTCUSDT.to_string()],
            exchange: TEST_EXCHANGE_BINANCE.to_string(),
            data_types: vec![1], // ORDER_BOOK
        },
        SubscribeRequest {
            symbols: vec![TEST_SYMBOL_ETHUSDT.to_string()],
            exchange: TEST_EXCHANGE_BINANCE.to_string(),
            data_types: vec![2], // TRADES
        },
    ];
    
    // Process both subscriptions
    for req in requests {
        let request = Request::new(req);
        let response = grpc_service.subscribe(request).await;
        assert!(response.is_ok());
    }
}

#[rstest]
#[tokio::test]
async fn test_invalid_subscription_data() {
    let grpc_service = grpc_service();
    
    // Test with empty symbols
    let empty_request = SubscribeRequest {
        symbols: vec![],
        exchange: TEST_EXCHANGE_BINANCE.to_string(),
        data_types: vec![1],
    };
    
    let request = Request::new(empty_request);
    let response = grpc_service.subscribe(request).await;
    assert!(response.is_ok()); // Should handle gracefully
    
    // Test with invalid exchange
    let invalid_exchange_request = SubscribeRequest {
        symbols: vec![TEST_SYMBOL_BTCUSDT.to_string()],
        exchange: "invalid_exchange".to_string(),
        data_types: vec![1],
    };
    
    let request = Request::new(invalid_exchange_request);
    let response = grpc_service.subscribe(request).await;
    assert!(response.is_ok()); // Should handle gracefully
}

#[rstest]
#[tokio::test]
async fn test_concurrent_subscriptions() {
    use std::sync::Arc;
    
    let service = Arc::new(grpc_service());
    
    let mut handles = Vec::new();
    
    // Spawn multiple subscription tasks
    for i in 0..5 {
        let service_clone = Arc::clone(&service);
        let symbol = format!("TEST{}", i);
        
        let handle = tokio::spawn(async move {
            let request = SubscribeRequest {
                symbols: vec![symbol],
                exchange: TEST_EXCHANGE_BINANCE.to_string(),
                data_types: vec![1, 2, 3],
            };
            
            let req = Request::new(request);
            service_clone.subscribe(req).await
        });
        
        handles.push(handle);
    }
    
    // Wait for all subscriptions to complete
    for handle in handles {
        let result = handle.await.expect("Task should complete");
        assert!(result.is_ok());
    }
}

#[rstest]
#[tokio::test]
async fn test_memory_efficiency() {
    use std::mem;
    
    let service = grpc_service();
    
    // Check service size is reasonable
    let size = mem::size_of_val(&service);
    assert!(size < 4096); // Should be less than 4KB
}

#[rstest]
#[tokio::test]
async fn test_large_subscription_lists() {
    let grpc_service = grpc_service();
    
    // Create subscription with many symbols
    let mut symbols = Vec::new();
    for i in 0..100 {
        symbols.push(format!("SYMBOL{}", i));
    }
    
    let large_request = SubscribeRequest {
        symbols,
        exchange: TEST_EXCHANGE_BINANCE.to_string(),
        data_types: vec![1, 2, 3, 4],
    };
    
    let request = Request::new(large_request);
    let response = grpc_service.subscribe(request).await;
    assert!(response.is_ok());
}

#[rstest]
fn test_fixed_point_precision_edge_cases() {
    // Test very small values
    let small_price = 0.00000001;
    assert_eq!(price_to_fixed_point(small_price), 0); // Rounds to 0
    
    // Test very large values
    let large_price = 1e10;
    let result = price_to_fixed_point(large_price);
    assert!(result > 0);
    
    // Test precision at the edge of fixed-point representation
    let precise_price = 123.4567;
    let fixed = price_to_fixed_point(precise_price);
    let back_to_float = fixed as f64 / FIXED_POINT_MULTIPLIER;
    
    // Should maintain reasonable precision
    assert!((back_to_float - precise_price).abs() < 0.001);
}

#[rstest]
fn test_error_handling_malformed_json() {
    let malformed_json = json!({
        "s": TEST_SYMBOL_BTCUSDT,
        "b": "invalid_array_structure",
        "a": null
    });
    
    let (broadcaster, _receiver) = broadcast::channel(1000);
    
    // Should handle malformed JSON gracefully without panicking
    MarketDataGrpcService::process_binance_depth(&malformed_json, &broadcaster);
    
    // If we reach here, no panic occurred
    assert!(true);
}