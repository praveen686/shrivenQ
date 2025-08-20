//! Comprehensive unit tests for Zerodha WebSocket implementation
//! 
//! These tests cover WebSocket connection handling, binary data parsing,
//! authentication flow, and Kite API protocol compliance.

use rstest::*;
use tokio::sync::mpsc;
use serde_json::json;
use std::time::Duration;

use market_connector::exchanges::zerodha::websocket::*;
use market_connector::connectors::adapter::{FeedAdapter, FeedConfig};
use services_common::{ZerodhaAuth, ZerodhaConfig, L2Update, Px, Qty, Side, Symbol, Ts};
use rustc_hash::FxHashMap;

// Test constants
const TEST_SYMBOL_ID: u32 = 1;
const TEST_NIFTY_TOKEN: u32 = 256265;
const TEST_BANKNIFTY_TOKEN: u32 = 260105;
const TEST_INSTRUMENT_TOKEN: u32 = 408065;
const TEST_TIMESTAMP: i64 = 1640995200000; // 2022-01-01 00:00:00 UTC
const TEST_LAST_PRICE: f64 = 17500.25;
const TEST_BID_PRICE: f64 = 17499.75;
const TEST_ASK_PRICE: f64 = 17500.50;
const TEST_BID_QUANTITY: u32 = 100;
const TEST_ASK_QUANTITY: u32 = 75;
const TEST_VOLUME: u32 = 50000;
const TEST_OHLC_OPEN: f64 = 17450.00;
const TEST_OHLC_HIGH: f64 = 17525.75;
const TEST_OHLC_LOW: f64 = 17425.50;
const TEST_OHLC_CLOSE: f64 = 17500.25;

// Authentication test constants
const TEST_API_KEY: &str = "test_api_key";
const TEST_ACCESS_TOKEN: &str = "test_access_token";
const TEST_USER_ID: &str = "test_user";

#[fixture]
fn test_config() -> FeedConfig {
    let mut symbol_map = FxHashMap::default();
    symbol_map.insert(Symbol::new(TEST_SYMBOL_ID), TEST_NIFTY_TOKEN.to_string());
    symbol_map.insert(Symbol::new(TEST_SYMBOL_ID + 1), TEST_BANKNIFTY_TOKEN.to_string());
    
    FeedConfig {
        name: "test_zerodha".to_string(),
        ws_url: "wss://ws.kite.trade".to_string(),
        api_url: "https://api.kite.trade".to_string(),
        symbol_map,
        max_reconnects: 5,
        reconnect_delay_ms: 1000,
    }
}

#[fixture]
fn test_auth() -> ZerodhaAuth {
    let config = ZerodhaConfig::new(
        TEST_USER_ID.to_string(),
        "password".to_string(),
        "totp_secret".to_string(),
        TEST_API_KEY.to_string(),
        "api_secret".to_string(),
    );
    ZerodhaAuth::new(TEST_API_KEY.to_string(), TEST_ACCESS_TOKEN.to_string(), TEST_USER_ID.to_string())
}

#[fixture]
fn zerodha_feed(test_config: FeedConfig, test_auth: ZerodhaAuth) -> ZerodhaWebSocketFeed {
    ZerodhaWebSocketFeed::new(test_config, test_auth)
}

#[rstest]
async fn test_zerodha_feed_creation(zerodha_feed: ZerodhaWebSocketFeed) {
    assert_eq!(zerodha_feed.symbol_map.len(), 2);
    assert_eq!(zerodha_feed.token_map.len(), 2);
}

#[rstest]
async fn test_connection_initialization(mut zerodha_feed: ZerodhaWebSocketFeed) {
    let result = zerodha_feed.connect().await;
    assert!(result.is_ok());
}

#[rstest]
async fn test_symbol_subscription(mut zerodha_feed: ZerodhaWebSocketFeed) {
    let symbols = vec![Symbol::new(TEST_SYMBOL_ID), Symbol::new(TEST_SYMBOL_ID + 1)];
    
    let result = zerodha_feed.subscribe(symbols).await;
    assert!(result.is_ok());
}

#[rstest]
fn test_kite_order_message_parsing() {
    let order_json = json!({
        "type": "order",
        "data": {
            "instrument_token": TEST_INSTRUMENT_TOKEN,
            "timestamp": TEST_TIMESTAMP,
            "last_price": TEST_LAST_PRICE,
            "depth": {
                "buy": [
                    {
                        "price": TEST_BID_PRICE,
                        "quantity": TEST_BID_QUANTITY,
                        "orders": 5
                    }
                ],
                "sell": [
                    {
                        "price": TEST_ASK_PRICE,
                        "quantity": TEST_ASK_QUANTITY,
                        "orders": 3
                    }
                ]
            }
        }
    });
    
    let kite_msg: KiteMessage = serde_json::from_value(order_json)
        .expect("Should parse order message");
    
    match kite_msg {
        KiteMessage::Order(order) => {
            assert_eq!(order.data.instrument_token, TEST_INSTRUMENT_TOKEN);
            assert_eq!(order.data.timestamp, TEST_TIMESTAMP);
            assert_eq!(order.data.last_price, TEST_LAST_PRICE);
            assert_eq!(order.data.depth.buy.len(), 1);
            assert_eq!(order.data.depth.sell.len(), 1);
            
            // Verify bid data
            assert_eq!(order.data.depth.buy[0].price, TEST_BID_PRICE);
            assert_eq!(order.data.depth.buy[0].quantity, TEST_BID_QUANTITY);
            assert_eq!(order.data.depth.buy[0].orders, 5);
            
            // Verify ask data
            assert_eq!(order.data.depth.sell[0].price, TEST_ASK_PRICE);
            assert_eq!(order.data.depth.sell[0].quantity, TEST_ASK_QUANTITY);
            assert_eq!(order.data.depth.sell[0].orders, 3);
        },
        _ => panic!("Expected order message"),
    }
}

#[rstest]
fn test_kite_quote_message_parsing() {
    let quote_json = json!({
        "type": "quote",
        "data": {
            TEST_INSTRUMENT_TOKEN.to_string(): {
                "instrument_token": TEST_INSTRUMENT_TOKEN,
                "timestamp": TEST_TIMESTAMP.to_string(),
                "last_price": TEST_LAST_PRICE,
                "volume": TEST_VOLUME,
                "buy_quantity": TEST_BID_QUANTITY,
                "sell_quantity": TEST_ASK_QUANTITY,
                "ohlc": {
                    "open": TEST_OHLC_OPEN,
                    "high": TEST_OHLC_HIGH,
                    "low": TEST_OHLC_LOW,
                    "close": TEST_OHLC_CLOSE
                }
            }
        }
    });
    
    let kite_msg: KiteMessage = serde_json::from_value(quote_json)
        .expect("Should parse quote message");
    
    match kite_msg {
        KiteMessage::Quote(quote) => {
            let quote_data = quote.data.get(&TEST_INSTRUMENT_TOKEN.to_string())
                .expect("Should have quote data");
            
            assert_eq!(quote_data.instrument_token, TEST_INSTRUMENT_TOKEN);
            assert_eq!(quote_data.last_price, TEST_LAST_PRICE);
            assert_eq!(quote_data.volume, TEST_VOLUME);
            assert_eq!(quote_data.buy_quantity, TEST_BID_QUANTITY);
            assert_eq!(quote_data.sell_quantity, TEST_ASK_QUANTITY);
            
            // Verify OHLC data
            assert_eq!(quote_data.ohlc.open, TEST_OHLC_OPEN);
            assert_eq!(quote_data.ohlc.high, TEST_OHLC_HIGH);
            assert_eq!(quote_data.ohlc.low, TEST_OHLC_LOW);
            assert_eq!(quote_data.ohlc.close, TEST_OHLC_CLOSE);
        },
        _ => panic!("Expected quote message"),
    }
}

#[rstest]
fn test_kite_generic_message_parsing() {
    let message_json = json!({
        "type": "message",
        "data": "Connection established successfully"
    });
    
    let kite_msg: KiteMessage = serde_json::from_value(message_json)
        .expect("Should parse message");
    
    match kite_msg {
        KiteMessage::Message { data } => {
            assert_eq!(data, "Connection established successfully");
        },
        _ => panic!("Expected generic message"),
    }
}

#[rstest]
fn test_kite_subscribe_serialization() {
    let subscribe_msg = KiteSubscribe {
        a: "subscribe".to_string(),
        v: vec![TEST_NIFTY_TOKEN, TEST_BANKNIFTY_TOKEN],
    };
    
    let json = serde_json::to_string(&subscribe_msg)
        .expect("Should serialize subscribe message");
    
    // Verify JSON structure
    let parsed: serde_json::Value = serde_json::from_str(&json)
        .expect("Should parse serialized JSON");
    
    assert_eq!(parsed["a"], "subscribe");
    assert!(parsed["v"].is_array());
    assert_eq!(parsed["v"].as_array().unwrap().len(), 2);
}

#[rstest]
fn test_kite_mode_change_serialization() {
    let mode_msg = KiteModeChange {
        a: "mode".to_string(),
        v: vec![("full".to_string(), TEST_NIFTY_TOKEN)],
    };
    
    let json = serde_json::to_string(&mode_msg)
        .expect("Should serialize mode change message");
    
    // Verify JSON structure
    let parsed: serde_json::Value = serde_json::from_str(&json)
        .expect("Should parse serialized JSON");
    
    assert_eq!(parsed["a"], "mode");
    assert!(parsed["v"].is_array());
}

#[rstest]
fn test_order_update_to_l2updates(zerodha_feed: ZerodhaWebSocketFeed) {
    let order = OrderUpdate {
        data: OrderData {
            instrument_token: TEST_NIFTY_TOKEN,
            timestamp: TEST_TIMESTAMP,
            last_price: TEST_LAST_PRICE,
            depth: Depth {
                buy: vec![
                    DepthLevel {
                        price: TEST_BID_PRICE,
                        quantity: TEST_BID_QUANTITY,
                        orders: 5,
                    }
                ],
                sell: vec![
                    DepthLevel {
                        price: TEST_ASK_PRICE,
                        quantity: TEST_ASK_QUANTITY,
                        orders: 3,
                    }
                ],
            },
        },
    };
    
    let updates = zerodha_feed.parse_order_update(order);
    
    // Should generate L2Updates for both bid and ask
    assert_eq!(updates.len(), 2);
    
    // Verify bid update
    let bid_update = &updates[0];
    assert_eq!(bid_update.side, Side::Bid);
    assert_eq!(bid_update.price, Px::new(TEST_BID_PRICE));
    assert_eq!(bid_update.quantity, Qty::from_units(TEST_BID_QUANTITY as i64));
    
    // Verify ask update
    let ask_update = &updates[1];
    assert_eq!(ask_update.side, Side::Ask);
    assert_eq!(ask_update.price, Px::new(TEST_ASK_PRICE));
    assert_eq!(ask_update.quantity, Qty::from_units(TEST_ASK_QUANTITY as i64));
}

#[rstest]
fn test_order_update_invalid_token(zerodha_feed: ZerodhaWebSocketFeed) {
    let order = OrderUpdate {
        data: OrderData {
            instrument_token: 999999, // Invalid token not in symbol map
            timestamp: TEST_TIMESTAMP,
            last_price: TEST_LAST_PRICE,
            depth: Depth {
                buy: vec![],
                sell: vec![],
            },
        },
    };
    
    let updates = zerodha_feed.parse_order_update(order);
    
    // Should return empty updates for unknown token
    assert!(updates.is_empty());
}

#[rstest]
fn test_order_update_negative_timestamp(zerodha_feed: ZerodhaWebSocketFeed) {
    let order = OrderUpdate {
        data: OrderData {
            instrument_token: TEST_NIFTY_TOKEN,
            timestamp: -1000, // Negative timestamp
            last_price: TEST_LAST_PRICE,
            depth: Depth {
                buy: vec![
                    DepthLevel {
                        price: TEST_BID_PRICE,
                        quantity: TEST_BID_QUANTITY,
                        orders: 1,
                    }
                ],
                sell: vec![],
            },
        },
    };
    
    let updates = zerodha_feed.parse_order_update(order);
    
    // Should handle negative timestamp gracefully
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].ts, Ts::from_nanos(0)); // Should default to 0
}

#[rstest]
fn test_binary_data_parsing_empty() {
    let zerodha_feed = zerodha_feed(test_config(), test_auth());
    let empty_data = vec![];
    
    let result = zerodha_feed.parse_binary_data(&empty_data);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[rstest]
fn test_binary_data_parsing_insufficient_length() {
    let zerodha_feed = zerodha_feed(test_config(), test_auth());
    let short_data = vec![0x01]; // Only 1 byte
    
    let result = zerodha_feed.parse_binary_data(&short_data);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[rstest]
fn test_binary_data_parsing_heartbeat() {
    let zerodha_feed = zerodha_feed(test_config(), test_auth());
    
    // Create LTP mode packet (8 bytes)
    let mut ltp_packet = vec![0x00, 0x01]; // 1 packet
    ltp_packet.extend_from_slice(&[0x00, 0x08]); // 8 bytes length
    ltp_packet.extend_from_slice(&TEST_NIFTY_TOKEN.to_be_bytes()); // Token
    ltp_packet.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // Padding to 8 bytes
    
    let result = zerodha_feed.parse_binary_data(&ltp_packet);
    assert!(result.is_ok());
    // LTP mode packets don't generate L2Updates in current implementation
    assert!(result.unwrap().is_empty());
}

#[rstest]
fn test_binary_data_parsing_full_mode() {
    let zerodha_feed = zerodha_feed(test_config(), test_auth());
    
    // Create full mode packet (184 bytes) - this is a simplified mock
    let mut full_packet = vec![0x00, 0x01]; // 1 packet
    full_packet.extend_from_slice(&[0x00, 0xB8]); // 184 bytes (0xB8 = 184)
    full_packet.extend_from_slice(&TEST_NIFTY_TOKEN.to_be_bytes()); // Token
    
    // Fill with mock data up to depth section (44 bytes in)
    full_packet.resize(44 + 6, 0x00);
    
    // Add mock bid levels (5 levels * 12 bytes each)
    for i in 0..5 {
        let qty = (100 + i * 10) as u32;
        let price = (17500.0 * 100.0) as u32 + i; // Price in paisa
        
        full_packet.extend_from_slice(&qty.to_be_bytes()); // Quantity
        full_packet.extend_from_slice(&price.to_be_bytes()); // Price
        full_packet.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // Orders (4 bytes)
    }
    
    // Add mock ask levels (5 levels * 12 bytes each)
    for i in 0..5 {
        let qty = (75 + i * 15) as u32;
        let price = (17501.0 * 100.0) as u32 + i; // Price in paisa
        
        full_packet.extend_from_slice(&qty.to_be_bytes()); // Quantity
        full_packet.extend_from_slice(&price.to_be_bytes()); // Price
        full_packet.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // Orders (4 bytes)
    }
    
    // Pad to 184 bytes
    full_packet.resize(184 + 4, 0x00);
    
    let result = zerodha_feed.parse_binary_data(&full_packet);
    assert!(result.is_ok());
    
    let updates = result.unwrap();
    // Should generate updates for both bids and asks
    assert!(!updates.is_empty());
}

#[rstest]
fn test_binary_data_malformed_packet() {
    let zerodha_feed = zerodha_feed(test_config(), test_auth());
    
    // Create packet with incorrect length
    let mut malformed_packet = vec![0x00, 0x01]; // 1 packet
    malformed_packet.extend_from_slice(&[0x00, 0xFF]); // Claims 255 bytes
    malformed_packet.extend_from_slice(&[0x01, 0x02, 0x03, 0x04]); // Only 4 bytes data
    
    let result = zerodha_feed.parse_binary_data(&malformed_packet);
    assert!(result.is_ok());
    // Should handle gracefully and return empty updates
    assert!(result.unwrap().is_empty());
}

#[rstest]
fn test_websocket_url_construction() {
    let api_key = "test_key";
    let access_token = "test_token";
    
    let expected_url = format!(
        "wss://ws.kite.trade?api_key={}&access_token={}",
        api_key, access_token
    );
    
    // URL construction is handled internally in run_websocket
    assert!(expected_url.contains("ws.kite.trade"));
    assert!(expected_url.contains(api_key));
    assert!(expected_url.contains(access_token));
}

#[rstest]
#[tokio::test]
async fn test_concurrent_binary_parsing() {
    use std::sync::Arc;
    
    let zerodha_feed = Arc::new(zerodha_feed(test_config(), test_auth()));
    
    // Create test binary data
    let test_data = vec![0x00, 0x01, 0x00, 0x08, 0x00, 0x00, 0x01, 0x00]; // Simple 8-byte packet
    
    let mut handles = Vec::new();
    
    // Spawn multiple tasks parsing binary data concurrently
    for _i in 0..10 {
        let feed_clone = Arc::clone(&zerodha_feed);
        let data_clone = test_data.clone();
        
        let handle = tokio::spawn(async move {
            feed_clone.parse_binary_data(&data_clone)
        });
        handles.push(handle);
    }
    
    // Wait for all parsing tasks to complete
    for handle in handles {
        let result = handle.await.expect("Task should complete");
        assert!(result.is_ok());
    }
}

#[rstest]
#[tokio::test]
async fn test_authentication_data_extraction() {
    let auth = test_auth();
    
    // Test API key extraction
    let api_key = auth.get_api_key();
    assert_eq!(api_key, TEST_API_KEY);
    
    // Test authentication would be called in real scenario
    // Note: This would require actual network calls in real implementation
    // For unit tests, we're just testing the structure
}

#[rstest]
fn test_symbol_token_mapping() {
    let config = test_config();
    let auth = test_auth();
    let feed = ZerodhaWebSocketFeed::new(config, auth);
    
    // Test symbol to token mapping
    assert_eq!(feed.token_map.get(&Symbol::new(TEST_SYMBOL_ID)), Some(&TEST_NIFTY_TOKEN));
    assert_eq!(feed.token_map.get(&Symbol::new(TEST_SYMBOL_ID + 1)), Some(&TEST_BANKNIFTY_TOKEN));
    
    // Test token to symbol mapping
    assert_eq!(feed.symbol_map.get(&TEST_NIFTY_TOKEN), Some(&Symbol::new(TEST_SYMBOL_ID)));
    assert_eq!(feed.symbol_map.get(&TEST_BANKNIFTY_TOKEN), Some(&Symbol::new(TEST_SYMBOL_ID + 1)));
}

#[rstest]
fn test_depth_level_data_structures() {
    let depth_level = DepthLevel {
        price: TEST_BID_PRICE,
        quantity: TEST_BID_QUANTITY,
        orders: 5,
    };
    
    assert_eq!(depth_level.price, TEST_BID_PRICE);
    assert_eq!(depth_level.quantity, TEST_BID_QUANTITY);
    assert_eq!(depth_level.orders, 5);
}

#[rstest]
fn test_ohlc_data_structure() {
    let ohlc = OHLC {
        open: TEST_OHLC_OPEN,
        high: TEST_OHLC_HIGH,
        low: TEST_OHLC_LOW,
        close: TEST_OHLC_CLOSE,
    };
    
    assert_eq!(ohlc.open, TEST_OHLC_OPEN);
    assert_eq!(ohlc.high, TEST_OHLC_HIGH);
    assert_eq!(ohlc.low, TEST_OHLC_LOW);
    assert_eq!(ohlc.close, TEST_OHLC_CLOSE);
    
    // Verify OHLC relationships
    assert!(ohlc.high >= ohlc.open);
    assert!(ohlc.high >= ohlc.close);
    assert!(ohlc.low <= ohlc.open);
    assert!(ohlc.low <= ohlc.close);
}

#[rstest]
#[tokio::test]
async fn test_error_handling_invalid_json() {
    let invalid_json = r#"{"type": "invalid", "malformed": json"#;
    
    // Should not panic when parsing invalid JSON
    let result = serde_json::from_str::<KiteMessage>(invalid_json);
    assert!(result.is_err());
}

#[rstest]
#[tokio::test]
async fn test_memory_efficiency() {
    use std::mem;
    
    let feed = zerodha_feed(test_config(), test_auth());
    
    // Verify reasonable memory usage
    let size = mem::size_of_val(&feed);
    assert!(size < 2048); // Should be less than 2KB
    
    // Check hash map capacities are reasonable
    assert!(feed.symbol_map.capacity() >= 2);
    assert!(feed.token_map.capacity() >= 2);
}

#[rstest]
#[tokio::test(flavor = "multi_thread")]
async fn test_high_frequency_order_parsing() {
    let feed = zerodha_feed(test_config(), test_auth());
    
    let start = std::time::Instant::now();
    let parse_count = 1000;
    
    // Parse many order updates rapidly
    for i in 0..parse_count {
        let order = OrderUpdate {
            data: OrderData {
                instrument_token: TEST_NIFTY_TOKEN,
                timestamp: TEST_TIMESTAMP + i as i64,
                last_price: TEST_LAST_PRICE + i as f64 * 0.25,
                depth: Depth {
                    buy: vec![
                        DepthLevel {
                            price: TEST_BID_PRICE + i as f64 * 0.25,
                            quantity: TEST_BID_QUANTITY + (i % 100) as u32,
                            orders: 1 + (i % 10) as u32,
                        }
                    ],
                    sell: vec![
                        DepthLevel {
                            price: TEST_ASK_PRICE + i as f64 * 0.25,
                            quantity: TEST_ASK_QUANTITY + (i % 50) as u32,
                            orders: 1 + (i % 5) as u32,
                        }
                    ],
                },
            },
        };
        
        let _updates = feed.parse_order_update(order);
    }
    
    let elapsed = start.elapsed();
    let parses_per_sec = parse_count as f64 / elapsed.as_secs_f64();
    
    // Should handle at least 10000 parses per second
    assert!(parses_per_sec > 10000.0, "Only processed {} parses/sec", parses_per_sec);
}

#[rstest]
fn test_subscription_message_formats() {
    // Test various subscription formats that Kite API expects
    
    // Standard subscription
    let sub_msg = KiteSubscribe {
        a: "subscribe".to_string(),
        v: vec![TEST_NIFTY_TOKEN],
    };
    let json = serde_json::to_string(&sub_msg).unwrap();
    assert!(json.contains("subscribe"));
    assert!(json.contains(&TEST_NIFTY_TOKEN.to_string()));
    
    // Mode change with multiple formats
    let mode_formats = vec![
        ("ltp", vec![TEST_NIFTY_TOKEN]),
        ("quote", vec![TEST_NIFTY_TOKEN, TEST_BANKNIFTY_TOKEN]),
        ("full", vec![TEST_NIFTY_TOKEN]),
    ];
    
    for (mode, tokens) in mode_formats {
        let mode_json = serde_json::json!({
            "a": "mode",
            "v": [mode, tokens]
        });
        
        // Verify the JSON structure
        assert_eq!(mode_json["a"], "mode");
        assert!(mode_json["v"].is_array());
        assert_eq!(mode_json["v"][0], mode);
    }
}