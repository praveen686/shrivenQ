//! Comprehensive unit tests for Binance WebSocket implementation
//! 
//! These tests cover WebSocket connection handling, message parsing, 
//! order book reconstruction, and error recovery scenarios.

use rstest::*;
use tokio::sync::mpsc;
use serde_json::json;
use std::time::Duration;

use market_connector::exchanges::binance::websocket::*;
use market_connector::connectors::adapter::{FeedAdapter, FeedConfig};
use services_common::{BinanceAuth, BinanceMarket, L2Update, Px, Qty, Side, Symbol, Ts};
use rustc_hash::FxHashMap;

// Test constants
const TEST_SYMBOL_ID: u32 = 1;
const TEST_BTCUSDT_SYMBOL: &str = "BTCUSDT";
const TEST_ETHUSDT_SYMBOL: &str = "ETHUSDT";
const TEST_BID_PRICE: f64 = 45000.50;
const TEST_ASK_PRICE: f64 = 45001.00;
const TEST_QUANTITY: f64 = 1.25;
const TEST_TRADE_PRICE: f64 = 45000.75;
const TEST_TRADE_QUANTITY: f64 = 0.5;
const TEST_EVENT_TIME: u64 = 1640995200000; // 2022-01-01 00:00:00 UTC
const TEST_FIRST_UPDATE_ID: u64 = 12345;
const TEST_FINAL_UPDATE_ID: u64 = 12346;
const TEST_LAST_UPDATE_ID: u64 = 12340;
const TEST_TRADE_ID: &str = "123456789";

#[fixture]
fn test_config() -> FeedConfig {
    let mut symbol_map = FxHashMap::default();
    symbol_map.insert(Symbol::new(TEST_SYMBOL_ID), TEST_BTCUSDT_SYMBOL.to_string());
    symbol_map.insert(Symbol::new(TEST_SYMBOL_ID + 1), TEST_ETHUSDT_SYMBOL.to_string());
    
    FeedConfig {
        name: "test_binance".to_string(),
        ws_url: "wss://stream.binance.com:9443".to_string(),
        api_url: "https://api.binance.com".to_string(),
        symbol_map,
        max_reconnects: 5,
        reconnect_delay_ms: 1000,
    }
}

#[fixture]
fn test_auth() -> BinanceAuth {
    BinanceAuth::new("test_api_key".to_string(), "test_secret_key".to_string())
}

#[fixture]
fn binance_feed(test_config: FeedConfig, test_auth: BinanceAuth) -> BinanceWebSocketFeed {
    BinanceWebSocketFeed::new(test_config, test_auth, BinanceMarket::Spot, false)
}

#[fixture]
fn testnet_feed(test_config: FeedConfig, test_auth: BinanceAuth) -> BinanceWebSocketFeed {
    BinanceWebSocketFeed::new(test_config, test_auth, BinanceMarket::Spot, true)
}

#[rstest]
async fn test_binance_feed_creation(binance_feed: BinanceWebSocketFeed) {
    // Basic creation test
    assert!(true); // Feed created successfully if we reach here
}

#[rstest]
async fn test_connection_initialization(mut binance_feed: BinanceWebSocketFeed) {
    let result = binance_feed.connect().await;
    assert!(result.is_ok());
}

#[rstest]
async fn test_symbol_subscription(mut binance_feed: BinanceWebSocketFeed) {
    let symbols = vec![Symbol::new(TEST_SYMBOL_ID), Symbol::new(TEST_SYMBOL_ID + 1)];
    
    let result = binance_feed.subscribe(symbols).await;
    assert!(result.is_ok());
}

#[rstest]
fn test_depth_update_parsing() {
    let depth_json = json!({
        "e": "depthUpdate",
        "E": TEST_EVENT_TIME,
        "s": TEST_BTCUSDT_SYMBOL,
        "U": TEST_FIRST_UPDATE_ID,
        "u": TEST_FINAL_UPDATE_ID,
        "b": [
            [TEST_BID_PRICE.to_string(), TEST_QUANTITY.to_string()],
            ["44999.50", "2.0"]
        ],
        "a": [
            [TEST_ASK_PRICE.to_string(), TEST_QUANTITY.to_string()],
            ["45002.00", "1.5"]
        ]
    });
    
    let depth_update: DepthUpdate = serde_json::from_value(depth_json)
        .expect("Should parse depth update");
    
    assert_eq!(depth_update.event_type, "depthUpdate");
    assert_eq!(depth_update.event_time, TEST_EVENT_TIME);
    assert_eq!(depth_update.symbol, TEST_BTCUSDT_SYMBOL);
    assert_eq!(depth_update.first_update_id, TEST_FIRST_UPDATE_ID);
    assert_eq!(depth_update.final_update_id, TEST_FINAL_UPDATE_ID);
    assert_eq!(depth_update.bids.len(), 2);
    assert_eq!(depth_update.asks.len(), 2);
    
    // Verify bid data
    assert_eq!(depth_update.bids[0][0], TEST_BID_PRICE.to_string());
    assert_eq!(depth_update.bids[0][1], TEST_QUANTITY.to_string());
    
    // Verify ask data
    assert_eq!(depth_update.asks[0][0], TEST_ASK_PRICE.to_string());
    assert_eq!(depth_update.asks[0][1], TEST_QUANTITY.to_string());
}

#[rstest]
fn test_trade_update_parsing() {
    let trade_json = json!({
        "e": "trade",
        "E": TEST_EVENT_TIME,
        "s": TEST_BTCUSDT_SYMBOL,
        "p": TEST_TRADE_PRICE.to_string(),
        "q": TEST_TRADE_QUANTITY.to_string(),
        "m": false,
        "t": TEST_TRADE_ID
    });
    
    let trade_update: TradeUpdate = serde_json::from_value(trade_json)
        .expect("Should parse trade update");
    
    assert_eq!(trade_update.event_type, "trade");
    assert_eq!(trade_update.event_time, TEST_EVENT_TIME);
    assert_eq!(trade_update.symbol, TEST_BTCUSDT_SYMBOL);
    assert_eq!(trade_update.price, TEST_TRADE_PRICE.to_string());
    assert_eq!(trade_update.quantity, TEST_TRADE_QUANTITY.to_string());
    assert!(!trade_update.is_buyer_maker);
}

#[rstest]
fn test_ticker_update_parsing() {
    let ticker_json = json!({
        "e": "24hrTicker",
        "E": TEST_EVENT_TIME,
        "s": TEST_BTCUSDT_SYMBOL,
        "c": "45000.00",
        "b": TEST_BID_PRICE.to_string(),
        "B": TEST_QUANTITY.to_string(),
        "a": TEST_ASK_PRICE.to_string(),
        "A": "0.75"
    });
    
    let ticker_update: TickerUpdate = serde_json::from_value(ticker_json)
        .expect("Should parse ticker update");
    
    assert_eq!(ticker_update.event_type, "24hrTicker");
    assert_eq!(ticker_update.event_time, TEST_EVENT_TIME);
    assert_eq!(ticker_update.symbol, TEST_BTCUSDT_SYMBOL);
    assert_eq!(ticker_update.best_bid, TEST_BID_PRICE.to_string());
    assert_eq!(ticker_update.best_bid_qty, TEST_QUANTITY.to_string());
    assert_eq!(ticker_update.best_ask, TEST_ASK_PRICE.to_string());
    assert_eq!(ticker_update.best_ask_qty, "0.75");
}

#[rstest]
fn test_stream_message_parsing() {
    let stream_json = json!({
        "stream": "btcusdt@depth@100ms",
        "data": {
            "e": "depthUpdate",
            "E": TEST_EVENT_TIME,
            "s": TEST_BTCUSDT_SYMBOL,
            "U": TEST_FIRST_UPDATE_ID,
            "u": TEST_FINAL_UPDATE_ID,
            "b": [[TEST_BID_PRICE.to_string(), TEST_QUANTITY.to_string()]],
            "a": [[TEST_ASK_PRICE.to_string(), TEST_QUANTITY.to_string()]]
        }
    });
    
    let stream_message: StreamMessage = serde_json::from_value(stream_json)
        .expect("Should parse stream message");
    
    assert_eq!(stream_message.stream, "btcusdt@depth@100ms");
    assert!(stream_message.data.is_object());
}

#[rstest]
fn test_depth_snapshot_parsing() {
    let snapshot_json = json!({
        "lastUpdateId": TEST_LAST_UPDATE_ID,
        "bids": [
            [TEST_BID_PRICE.to_string(), TEST_QUANTITY.to_string()],
            ["44999.50", "2.0"]
        ],
        "asks": [
            [TEST_ASK_PRICE.to_string(), TEST_QUANTITY.to_string()],
            ["45002.00", "1.5"]
        ]
    });
    
    let snapshot: DepthSnapshot = serde_json::from_value(snapshot_json)
        .expect("Should parse depth snapshot");
    
    assert_eq!(snapshot.last_update_id, TEST_LAST_UPDATE_ID);
    assert_eq!(snapshot.bids.len(), 2);
    assert_eq!(snapshot.asks.len(), 2);
    
    // Verify snapshot data
    assert_eq!(snapshot.bids[0][0], TEST_BID_PRICE.to_string());
    assert_eq!(snapshot.bids[0][1], TEST_QUANTITY.to_string());
    assert_eq!(snapshot.asks[0][0], TEST_ASK_PRICE.to_string());
    assert_eq!(snapshot.asks[0][1], TEST_QUANTITY.to_string());
}

#[rstest]
async fn test_websocket_url_generation() {
    // Test mainnet URLs
    let mainnet_spot = BinanceWebSocketFeed::new(
        test_config(),
        test_auth(),
        BinanceMarket::Spot,
        false
    );
    let spot_url = mainnet_spot.get_ws_url();
    assert!(spot_url.contains("stream.binance.com"));
    
    let mainnet_futures = BinanceWebSocketFeed::new(
        test_config(),
        test_auth(),
        BinanceMarket::UsdFutures,
        false
    );
    let futures_url = mainnet_futures.get_ws_url();
    assert!(futures_url.contains("fstream.binance.com"));
    
    // Test testnet URLs
    let testnet_spot = BinanceWebSocketFeed::new(
        test_config(),
        test_auth(),
        BinanceMarket::Spot,
        true
    );
    let testnet_url = testnet_spot.get_ws_url();
    assert!(testnet_url.contains("testnet.binance.vision"));
}

#[rstest]
async fn test_api_url_generation() {
    // Test mainnet API URLs
    let mainnet_spot = BinanceWebSocketFeed::new(
        test_config(),
        test_auth(),
        BinanceMarket::Spot,
        false
    );
    let api_url = mainnet_spot.get_api_url();
    assert!(api_url.contains("api.binance.com"));
    
    // Test testnet API URLs
    let testnet_spot = BinanceWebSocketFeed::new(
        test_config(),
        test_auth(),
        BinanceMarket::Spot,
        true
    );
    let testnet_api_url = testnet_spot.get_api_url();
    assert!(testnet_api_url.contains("testnet.binance.vision"));
}

#[rstest]
fn test_order_book_manager_creation() {
    let symbol = Symbol::new(TEST_SYMBOL_ID);
    let manager = OrderBookManager::new(symbol);
    
    // Test initial state
    assert_eq!(manager.symbol, symbol);
    assert_eq!(manager.bids.len(), 0);
    assert_eq!(manager.asks.len(), 0);
    assert_eq!(manager.last_update_id, 0);
    assert!(!manager.snapshot_received);
}

#[rstest]
fn test_order_book_snapshot_application() {
    let symbol = Symbol::new(TEST_SYMBOL_ID);
    let mut manager = OrderBookManager::new(symbol);
    
    let snapshot = DepthSnapshot {
        last_update_id: TEST_LAST_UPDATE_ID,
        bids: vec![
            [TEST_BID_PRICE.to_string(), TEST_QUANTITY.to_string()],
            ["44999.50".to_string(), "2.0".to_string()]
        ],
        asks: vec![
            [TEST_ASK_PRICE.to_string(), TEST_QUANTITY.to_string()],
            ["45002.00".to_string(), "1.5".to_string()]
        ],
    };
    
    manager.apply_snapshot(snapshot);
    
    // Verify snapshot was applied
    assert!(manager.snapshot_received);
    assert_eq!(manager.last_update_id, TEST_LAST_UPDATE_ID);
    assert_eq!(manager.bids.len(), 2);
    assert_eq!(manager.asks.len(), 2);
    
    // Verify bid ordering (descending)
    assert!(manager.bids[0].0 > manager.bids[1].0);
    
    // Verify ask ordering (ascending)
    assert!(manager.asks[0].0 < manager.asks[1].0);
}

#[rstest]
fn test_order_book_update_without_snapshot() {
    let symbol = Symbol::new(TEST_SYMBOL_ID);
    let mut manager = OrderBookManager::new(symbol);
    
    let update = DepthUpdate {
        event_type: "depthUpdate".to_string(),
        event_time: TEST_EVENT_TIME,
        symbol: TEST_BTCUSDT_SYMBOL.to_string(),
        first_update_id: TEST_FIRST_UPDATE_ID,
        final_update_id: TEST_FINAL_UPDATE_ID,
        bids: vec![[TEST_BID_PRICE.to_string(), TEST_QUANTITY.to_string()]],
        asks: vec![[TEST_ASK_PRICE.to_string(), TEST_QUANTITY.to_string()]],
    };
    
    let updates = manager.apply_update(&update);
    
    // Should return empty updates without snapshot
    assert!(updates.is_empty());
}

#[rstest]
fn test_order_book_update_with_snapshot() {
    let symbol = Symbol::new(TEST_SYMBOL_ID);
    let mut manager = OrderBookManager::new(symbol);
    
    // Apply snapshot first
    let snapshot = DepthSnapshot {
        last_update_id: TEST_LAST_UPDATE_ID,
        bids: vec![[TEST_BID_PRICE.to_string(), TEST_QUANTITY.to_string()]],
        asks: vec![[TEST_ASK_PRICE.to_string(), TEST_QUANTITY.to_string()]],
    };
    manager.apply_snapshot(snapshot);
    
    // Apply update
    let update = DepthUpdate {
        event_type: "depthUpdate".to_string(),
        event_time: TEST_EVENT_TIME,
        symbol: TEST_BTCUSDT_SYMBOL.to_string(),
        first_update_id: TEST_LAST_UPDATE_ID + 1,
        final_update_id: TEST_LAST_UPDATE_ID + 2,
        bids: vec![["45001.00".to_string(), "1.5".to_string()]],
        asks: vec![["45002.00".to_string(), "2.0".to_string()]],
    };
    
    let updates = manager.apply_update(&update);
    
    // Should generate L2Updates
    assert!(!updates.is_empty());
}

#[rstest]
fn test_order_book_update_sequence_gap() {
    let symbol = Symbol::new(TEST_SYMBOL_ID);
    let mut manager = OrderBookManager::new(symbol);
    
    // Apply snapshot
    let snapshot = DepthSnapshot {
        last_update_id: TEST_LAST_UPDATE_ID,
        bids: vec![[TEST_BID_PRICE.to_string(), TEST_QUANTITY.to_string()]],
        asks: vec![[TEST_ASK_PRICE.to_string(), TEST_QUANTITY.to_string()]],
    };
    manager.apply_snapshot(snapshot);
    
    // Apply update with gap in sequence
    let update = DepthUpdate {
        event_type: "depthUpdate".to_string(),
        event_time: TEST_EVENT_TIME,
        symbol: TEST_BTCUSDT_SYMBOL.to_string(),
        first_update_id: TEST_LAST_UPDATE_ID + 10, // Gap in sequence
        final_update_id: TEST_LAST_UPDATE_ID + 11,
        bids: vec![["45001.00".to_string(), "1.5".to_string()]],
        asks: vec![["45002.00".to_string(), "2.0".to_string()]],
    };
    
    let updates = manager.apply_update(&update);
    
    // Should handle sequence gap gracefully
    assert!(updates.is_empty());
    assert!(!manager.snapshot_received); // Should reset snapshot flag
}

#[rstest]
fn test_price_level_update() {
    let symbol = Symbol::new(TEST_SYMBOL_ID);
    let mut levels = Vec::new();
    let initial_price = Px::new(TEST_BID_PRICE);
    let initial_qty = Qty::new(TEST_QUANTITY);
    
    // Add initial level
    OrderBookManager::update_level(&mut levels, initial_price, initial_qty, false);
    assert_eq!(levels.len(), 1);
    assert_eq!(levels[0].0, initial_price);
    assert_eq!(levels[0].1, initial_qty);
    
    // Update quantity
    let new_qty = Qty::new(TEST_QUANTITY * 2.0);
    OrderBookManager::update_level(&mut levels, initial_price, new_qty, false);
    assert_eq!(levels.len(), 1);
    assert_eq!(levels[0].1, new_qty);
    
    // Remove level (zero quantity)
    OrderBookManager::update_level(&mut levels, initial_price, Qty::ZERO, false);
    assert!(levels.is_empty());
}

#[rstest]
fn test_price_level_sorting() {
    let symbol = Symbol::new(TEST_SYMBOL_ID);
    let mut bid_levels = Vec::new();
    let mut ask_levels = Vec::new();
    
    // Add multiple bid levels (should sort descending)
    let prices = [45000.0, 44999.0, 45001.0];
    for &price in &prices {
        OrderBookManager::update_level(
            &mut bid_levels,
            Px::new(price),
            Qty::new(TEST_QUANTITY),
            false
        );
    }
    
    // Verify descending order for bids
    assert!(bid_levels[0].0 > bid_levels[1].0);
    assert!(bid_levels[1].0 > bid_levels[2].0);
    
    // Add multiple ask levels (should sort ascending)
    for &price in &prices {
        OrderBookManager::update_level(
            &mut ask_levels,
            Px::new(price),
            Qty::new(TEST_QUANTITY),
            true
        );
    }
    
    // Verify ascending order for asks
    assert!(ask_levels[0].0 < ask_levels[1].0);
    assert!(ask_levels[1].0 < ask_levels[2].0);
}

#[rstest]
fn test_level_capacity_limit() {
    let symbol = Symbol::new(TEST_SYMBOL_ID);
    let mut levels = Vec::new();
    
    // Add more than 20 levels
    for i in 0..25 {
        let price = Px::new(45000.0 + i as f64);
        OrderBookManager::update_level(&mut levels, price, Qty::new(TEST_QUANTITY), true);
    }
    
    // Should be limited to 20 levels
    assert_eq!(levels.len(), 20);
}

#[rstest]
async fn test_invalid_json_handling() {
    // Test with invalid JSON
    let invalid_json = r#"{"invalid": json"#;
    
    // Should not panic when parsing invalid JSON
    let result = serde_json::from_str::<DepthUpdate>(invalid_json);
    assert!(result.is_err());
}

#[rstest]
async fn test_malformed_price_data() {
    let malformed_json = json!({
        "e": "depthUpdate",
        "E": TEST_EVENT_TIME,
        "s": TEST_BTCUSDT_SYMBOL,
        "U": TEST_FIRST_UPDATE_ID,
        "u": TEST_FINAL_UPDATE_ID,
        "b": [["invalid_price", "invalid_quantity"]],
        "a": [["45001.00", "1.25"]]
    });
    
    let depth_update: DepthUpdate = serde_json::from_value(malformed_json)
        .expect("Should parse despite malformed price data");
    
    // The parsing should succeed, but price conversion will fail during processing
    assert_eq!(depth_update.bids[0][0], "invalid_price");
}

#[rstest]
#[tokio::test]
async fn test_concurrent_order_book_updates() {
    use std::sync::Arc;
    use tokio::sync::Mutex;
    
    let symbol = Symbol::new(TEST_SYMBOL_ID);
    let manager = Arc::new(Mutex::new(OrderBookManager::new(symbol)));
    
    // Apply initial snapshot
    {
        let mut mgr = manager.lock().await;
        let snapshot = DepthSnapshot {
            last_update_id: TEST_LAST_UPDATE_ID,
            bids: vec![[TEST_BID_PRICE.to_string(), TEST_QUANTITY.to_string()]],
            asks: vec![[TEST_ASK_PRICE.to_string(), TEST_QUANTITY.to_string()]],
        };
        mgr.apply_snapshot(snapshot);
    }
    
    // Spawn multiple tasks updating the order book
    let mut handles = Vec::new();
    for i in 0..10 {
        let mgr_clone = Arc::clone(&manager);
        let handle = tokio::spawn(async move {
            let update = DepthUpdate {
                event_type: "depthUpdate".to_string(),
                event_time: TEST_EVENT_TIME + i,
                symbol: TEST_BTCUSDT_SYMBOL.to_string(),
                first_update_id: TEST_LAST_UPDATE_ID + i + 1,
                final_update_id: TEST_LAST_UPDATE_ID + i + 2,
                bids: vec![[format!("{:.2}", TEST_BID_PRICE + i as f64), TEST_QUANTITY.to_string()]],
                asks: vec![[format!("{:.2}", TEST_ASK_PRICE + i as f64), TEST_QUANTITY.to_string()]],
            };
            
            let mut mgr = mgr_clone.lock().await;
            mgr.apply_update(&update)
        });
        handles.push(handle);
    }
    
    // Wait for all updates to complete
    for handle in handles {
        let _updates = handle.await.expect("Task should complete");
    }
    
    // Verify order book is in consistent state
    let mgr = manager.lock().await;
    assert!(mgr.snapshot_received);
}

#[rstest]
#[tokio::test]
async fn test_memory_efficiency() {
    use std::mem;
    
    let symbol = Symbol::new(TEST_SYMBOL_ID);
    let manager = OrderBookManager::new(symbol);
    
    // Verify reasonable memory usage
    let size = mem::size_of_val(&manager);
    assert!(size < 1024); // Should be less than 1KB
    
    // Check vector capacities are reasonable
    assert_eq!(manager.bids.capacity(), 20);
    assert_eq!(manager.asks.capacity(), 20);
}

#[rstest]
#[tokio::test(flavor = "multi_thread")]
async fn test_high_frequency_updates() {
    let symbol = Symbol::new(TEST_SYMBOL_ID);
    let mut manager = OrderBookManager::new(symbol);
    
    // Apply initial snapshot
    let snapshot = DepthSnapshot {
        last_update_id: 0,
        bids: vec![[TEST_BID_PRICE.to_string(), TEST_QUANTITY.to_string()]],
        asks: vec![[TEST_ASK_PRICE.to_string(), TEST_QUANTITY.to_string()]],
    };
    manager.apply_snapshot(snapshot);
    
    let start = std::time::Instant::now();
    let update_count = 10000;
    
    // Apply many updates rapidly
    for i in 1..=update_count {
        let update = DepthUpdate {
            event_type: "depthUpdate".to_string(),
            event_time: TEST_EVENT_TIME + i,
            symbol: TEST_BTCUSDT_SYMBOL.to_string(),
            first_update_id: i,
            final_update_id: i + 1,
            bids: vec![[format!("{:.2}", TEST_BID_PRICE + (i % 100) as f64 * 0.01), TEST_QUANTITY.to_string()]],
            asks: vec![[format!("{:.2}", TEST_ASK_PRICE + (i % 100) as f64 * 0.01), TEST_QUANTITY.to_string()]],
        };
        
        let _updates = manager.apply_update(&update);
    }
    
    let elapsed = start.elapsed();
    let updates_per_sec = update_count as f64 / elapsed.as_secs_f64();
    
    // Should handle at least 1000 updates per second
    assert!(updates_per_sec > 1000.0, "Only processed {} updates/sec", updates_per_sec);
}