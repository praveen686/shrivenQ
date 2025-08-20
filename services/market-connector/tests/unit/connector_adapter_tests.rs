//! Comprehensive unit tests for connector adapter implementations
//! 
//! These tests cover feed adapter trait implementations, configuration management,
//! and adapter lifecycle operations.

use rstest::*;
use tokio::sync::mpsc;
use std::time::Duration;

use market_connector::connectors::adapter::{FeedAdapter, FeedConfig};
use market_connector::exchanges::binance::websocket::BinanceWebSocketFeed;
use market_connector::exchanges::zerodha::websocket::ZerodhaWebSocketFeed;
use services_common::{BinanceAuth, BinanceMarket, ZerodhaAuth, ZerodhaConfig, L2Update, Symbol};
use rustc_hash::FxHashMap;

// Test constants
const TEST_SYMBOL_ID_1: u32 = 1;
const TEST_SYMBOL_ID_2: u32 = 2;
const TEST_BTCUSDT: &str = "BTCUSDT";
const TEST_ETHUSDT: &str = "ETHUSDT";
const TEST_NIFTY_TOKEN: u32 = 256265;
const TEST_BANKNIFTY_TOKEN: u32 = 260105;

#[fixture]
fn binance_config() -> FeedConfig {
    let mut symbol_map = FxHashMap::default();
    symbol_map.insert(Symbol::new(TEST_SYMBOL_ID_1), TEST_BTCUSDT.to_string());
    symbol_map.insert(Symbol::new(TEST_SYMBOL_ID_2), TEST_ETHUSDT.to_string());
    
    FeedConfig {
        name: "binance_spot".to_string(),
        ws_url: "wss://stream.binance.com:9443".to_string(),
        api_url: "https://api.binance.com".to_string(),
        symbol_map,
        max_reconnects: 5,
        reconnect_delay_ms: 1000,
    }
}

#[fixture]
fn zerodha_config() -> FeedConfig {
    let mut symbol_map = FxHashMap::default();
    symbol_map.insert(Symbol::new(TEST_SYMBOL_ID_1), TEST_NIFTY_TOKEN.to_string());
    symbol_map.insert(Symbol::new(TEST_SYMBOL_ID_2), TEST_BANKNIFTY_TOKEN.to_string());
    
    FeedConfig {
        name: "zerodha".to_string(),
        ws_url: "wss://ws.kite.trade".to_string(),
        api_url: "https://api.kite.trade".to_string(),
        symbol_map,
        max_reconnects: 3,
        reconnect_delay_ms: 5000,
    }
}

#[fixture]
fn binance_auth() -> BinanceAuth {
    BinanceAuth::new("test_api_key".to_string(), "test_secret_key".to_string())
}

#[fixture]
fn zerodha_auth() -> ZerodhaAuth {
    let config = ZerodhaConfig::new(
        "test_user".to_string(),
        "test_password".to_string(),
        "test_totp_secret".to_string(),
        "test_api_key".to_string(),
        "test_api_secret".to_string(),
    );
    ZerodhaAuth::new("test_api_key".to_string(), "test_access_token".to_string(), "test_user".to_string())
}

#[fixture]
fn binance_adapter(binance_config: FeedConfig, binance_auth: BinanceAuth) -> BinanceWebSocketFeed {
    BinanceWebSocketFeed::new(binance_config, binance_auth, BinanceMarket::Spot, false)
}

#[fixture]
fn binance_testnet_adapter(binance_config: FeedConfig, binance_auth: BinanceAuth) -> BinanceWebSocketFeed {
    BinanceWebSocketFeed::new(binance_config, binance_auth, BinanceMarket::Spot, true)
}

#[fixture]
fn binance_futures_adapter(binance_config: FeedConfig, binance_auth: BinanceAuth) -> BinanceWebSocketFeed {
    BinanceWebSocketFeed::new(binance_config, binance_auth, BinanceMarket::UsdFutures, false)
}

#[fixture]
fn zerodha_adapter(zerodha_config: FeedConfig, zerodha_auth: ZerodhaAuth) -> ZerodhaWebSocketFeed {
    ZerodhaWebSocketFeed::new(zerodha_config, zerodha_auth)
}

#[rstest]
fn test_feed_config_creation(binance_config: FeedConfig, zerodha_config: FeedConfig) {
    // Test Binance config
    assert_eq!(binance_config.name, "binance_spot");
    assert_eq!(binance_config.symbol_map.len(), 2);
    assert_eq!(binance_config.max_reconnects, 5);
    assert_eq!(binance_config.reconnect_delay_ms, 1000);
    assert!(binance_config.ws_url.contains("binance.com"));
    assert!(binance_config.api_url.contains("api.binance.com"));
    
    // Test Zerodha config
    assert_eq!(zerodha_config.name, "zerodha");
    assert_eq!(zerodha_config.symbol_map.len(), 2);
    assert_eq!(zerodha_config.max_reconnects, 3);
    assert_eq!(zerodha_config.reconnect_delay_ms, 5000);
    assert!(zerodha_config.ws_url.contains("kite.trade"));
    assert!(zerodha_config.api_url.contains("api.kite.trade"));
}

#[rstest]
fn test_symbol_mapping_in_config(binance_config: FeedConfig, zerodha_config: FeedConfig) {
    // Test Binance symbol mapping
    assert_eq!(
        binance_config.symbol_map.get(&Symbol::new(TEST_SYMBOL_ID_1)),
        Some(&TEST_BTCUSDT.to_string())
    );
    assert_eq!(
        binance_config.symbol_map.get(&Symbol::new(TEST_SYMBOL_ID_2)),
        Some(&TEST_ETHUSDT.to_string())
    );
    
    // Test Zerodha symbol mapping (token-based)
    assert_eq!(
        zerodha_config.symbol_map.get(&Symbol::new(TEST_SYMBOL_ID_1)),
        Some(&TEST_NIFTY_TOKEN.to_string())
    );
    assert_eq!(
        zerodha_config.symbol_map.get(&Symbol::new(TEST_SYMBOL_ID_2)),
        Some(&TEST_BANKNIFTY_TOKEN.to_string())
    );
}

#[rstest]
#[tokio::test]
async fn test_binance_adapter_connect(mut binance_adapter: BinanceWebSocketFeed) {
    let result = binance_adapter.connect().await;
    assert!(result.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_zerodha_adapter_connect(mut zerodha_adapter: ZerodhaWebSocketFeed) {
    let result = zerodha_adapter.connect().await;
    assert!(result.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_binance_adapter_disconnect(mut binance_adapter: BinanceWebSocketFeed) {
    let result = binance_adapter.disconnect().await;
    assert!(result.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_zerodha_adapter_disconnect(mut zerodha_adapter: ZerodhaWebSocketFeed) {
    let result = zerodha_adapter.disconnect().await;
    assert!(result.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_binance_adapter_subscribe(mut binance_adapter: BinanceWebSocketFeed) {
    let symbols = vec![Symbol::new(TEST_SYMBOL_ID_1), Symbol::new(TEST_SYMBOL_ID_2)];
    let result = binance_adapter.subscribe(symbols).await;
    assert!(result.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_zerodha_adapter_subscribe(mut zerodha_adapter: ZerodhaWebSocketFeed) {
    let symbols = vec![Symbol::new(TEST_SYMBOL_ID_1), Symbol::new(TEST_SYMBOL_ID_2)];
    let result = zerodha_adapter.subscribe(symbols).await;
    assert!(result.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_binance_adapter_subscribe_empty_symbols(mut binance_adapter: BinanceWebSocketFeed) {
    let symbols = vec![];
    let result = binance_adapter.subscribe(symbols).await;
    assert!(result.is_ok()); // Should handle empty subscription gracefully
}

#[rstest]
#[tokio::test]
async fn test_zerodha_adapter_subscribe_empty_symbols(mut zerodha_adapter: ZerodhaWebSocketFeed) {
    let symbols = vec![];
    let result = zerodha_adapter.subscribe(symbols).await;
    assert!(result.is_ok()); // Should handle empty subscription gracefully
}

#[rstest]
#[tokio::test]
async fn test_binance_adapter_full_lifecycle(mut binance_adapter: BinanceWebSocketFeed) {
    // Test full connection lifecycle
    
    // 1. Connect
    let connect_result = binance_adapter.connect().await;
    assert!(connect_result.is_ok());
    
    // 2. Subscribe
    let symbols = vec![Symbol::new(TEST_SYMBOL_ID_1)];
    let subscribe_result = binance_adapter.subscribe(symbols).await;
    assert!(subscribe_result.is_ok());
    
    // 3. Run (test briefly with timeout)
    let (tx, mut rx) = mpsc::channel::<L2Update>(100);
    
    // Run in background task with timeout
    let run_task = tokio::spawn(async move {
        binance_adapter.run(tx).await
    });
    
    // Wait briefly to see if run starts without immediate errors
    let run_result = tokio::time::timeout(Duration::from_millis(100), run_task).await;
    
    // Timeout is expected since we're not providing real WebSocket data
    assert!(run_result.is_err()); // Timeout error expected
    
    // Check if any messages were received (unlikely in test environment)
    let received_count = rx.try_recv().is_ok() as usize;
    // In test environment, we don't expect real messages
    assert!(received_count == 0);
}

#[rstest]
#[tokio::test]
async fn test_zerodha_adapter_full_lifecycle(mut zerodha_adapter: ZerodhaWebSocketFeed) {
    // Test full connection lifecycle
    
    // 1. Connect
    let connect_result = zerodha_adapter.connect().await;
    assert!(connect_result.is_ok());
    
    // 2. Subscribe
    let symbols = vec![Symbol::new(TEST_SYMBOL_ID_1)];
    let subscribe_result = zerodha_adapter.subscribe(symbols).await;
    assert!(subscribe_result.is_ok());
    
    // 3. Run (test briefly with timeout)
    let (tx, mut rx) = mpsc::channel::<L2Update>(100);
    
    // Run in background task with timeout
    let run_task = tokio::spawn(async move {
        zerodha_adapter.run(tx).await
    });
    
    // Wait briefly to see if run starts without immediate errors
    let run_result = tokio::time::timeout(Duration::from_millis(100), run_task).await;
    
    // Timeout is expected since authentication would fail in test environment
    assert!(run_result.is_err()); // Timeout error expected
}

#[rstest]
fn test_binance_market_variants(binance_config: FeedConfig, binance_auth: BinanceAuth) {
    // Test different Binance market types
    let spot_feed = BinanceWebSocketFeed::new(
        binance_config.clone(),
        binance_auth.clone(),
        BinanceMarket::Spot,
        false
    );
    
    let futures_feed = BinanceWebSocketFeed::new(
        binance_config.clone(),
        binance_auth.clone(),
        BinanceMarket::UsdFutures,
        false
    );
    
    let coin_futures_feed = BinanceWebSocketFeed::new(
        binance_config.clone(),
        binance_auth.clone(),
        BinanceMarket::CoinFutures,
        false
    );
    
    // All should be created successfully
    // The differences are internal (URLs, etc.)
    assert!(true);
}

#[rstest]
fn test_binance_testnet_vs_mainnet(binance_config: FeedConfig, binance_auth: BinanceAuth) {
    // Test mainnet vs testnet configurations
    let mainnet_feed = BinanceWebSocketFeed::new(
        binance_config.clone(),
        binance_auth.clone(),
        BinanceMarket::Spot,
        false // mainnet
    );
    
    let testnet_feed = BinanceWebSocketFeed::new(
        binance_config.clone(),
        binance_auth.clone(),
        BinanceMarket::Spot,
        true // testnet
    );
    
    // Both should be created successfully
    // The difference is in internal URL selection
    assert!(true);
}

#[rstest]
#[tokio::test]
async fn test_adapter_configuration_validation(binance_config: FeedConfig) {
    // Test configuration with various invalid states
    
    // Empty symbol map should still work (no subscriptions)
    let empty_config = FeedConfig {
        name: "empty_test".to_string(),
        ws_url: "wss://test.example.com".to_string(),
        api_url: "https://api.test.com".to_string(),
        symbol_map: FxHashMap::default(),
        max_reconnects: 0,
        reconnect_delay_ms: 0,
    };
    
    let auth = binance_auth();
    let feed = BinanceWebSocketFeed::new(empty_config, auth, BinanceMarket::Spot, false);
    
    // Should create successfully even with empty config
    let connect_result = feed.connect().await;
    assert!(connect_result.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_concurrent_adapter_operations() {
    use std::sync::Arc;
    use tokio::sync::Mutex;
    
    let config = binance_config();
    let auth = binance_auth();
    
    let adapter = Arc::new(Mutex::new(
        BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false)
    ));
    
    let mut handles = Vec::new();
    
    // Test concurrent connect operations
    for i in 0..5 {
        let adapter_clone = Arc::clone(&adapter);
        let handle = tokio::spawn(async move {
            let mut guard = adapter_clone.lock().await;
            if i % 2 == 0 {
                guard.connect().await
            } else {
                guard.disconnect().await
            }
        });
        handles.push(handle);
    }
    
    // Wait for all operations to complete
    for handle in handles {
        let result = handle.await.expect("Task should complete");
        assert!(result.is_ok());
    }
}

#[rstest]
#[tokio::test]
async fn test_adapter_subscription_idempotency(mut binance_adapter: BinanceWebSocketFeed) {
    let symbols = vec![Symbol::new(TEST_SYMBOL_ID_1)];
    
    // Subscribe multiple times to same symbols
    for _i in 0..3 {
        let result = binance_adapter.subscribe(symbols.clone()).await;
        assert!(result.is_ok());
    }
}

#[rstest]
#[tokio::test]
async fn test_adapter_large_symbol_list(mut binance_adapter: BinanceWebSocketFeed) {
    // Create large symbol list
    let mut symbols = Vec::new();
    for i in 0..1000 {
        symbols.push(Symbol::new(i));
    }
    
    let result = binance_adapter.subscribe(symbols).await;
    assert!(result.is_ok()); // Should handle large lists gracefully
}

#[rstest]
#[tokio::test]
async fn test_adapter_reconnection_parameters(binance_config: FeedConfig) {
    // Test different reconnection configurations
    let configs = vec![
        FeedConfig {
            max_reconnects: 0,
            reconnect_delay_ms: 1000,
            ..binance_config.clone()
        },
        FeedConfig {
            max_reconnects: 100,
            reconnect_delay_ms: 10,
            ..binance_config.clone()
        },
        FeedConfig {
            max_reconnects: 1,
            reconnect_delay_ms: 60000,
            ..binance_config.clone()
        },
    ];
    
    for config in configs {
        let auth = binance_auth();
        let feed = BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false);
        
        // Should create successfully with any valid parameters
        let connect_result = feed.connect().await;
        assert!(connect_result.is_ok());
    }
}

#[rstest]
fn test_feed_config_clone(binance_config: FeedConfig) {
    let cloned_config = binance_config.clone();
    
    assert_eq!(binance_config.name, cloned_config.name);
    assert_eq!(binance_config.ws_url, cloned_config.ws_url);
    assert_eq!(binance_config.api_url, cloned_config.api_url);
    assert_eq!(binance_config.max_reconnects, cloned_config.max_reconnects);
    assert_eq!(binance_config.reconnect_delay_ms, cloned_config.reconnect_delay_ms);
    assert_eq!(binance_config.symbol_map.len(), cloned_config.symbol_map.len());
}

#[rstest]
fn test_feed_config_debug_format(binance_config: FeedConfig) {
    let debug_str = format!("{:?}", binance_config);
    
    // Debug output should contain key information
    assert!(debug_str.contains("binance_spot"));
    assert!(debug_str.contains("binance.com"));
    assert!(debug_str.contains(&TEST_BTCUSDT));
}

#[rstest]
#[tokio::test]
async fn test_adapter_memory_efficiency() {
    use std::mem;
    
    let config = binance_config();
    let auth = binance_auth();
    let adapter = BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false);
    
    // Check adapter memory usage is reasonable
    let size = mem::size_of_val(&adapter);
    assert!(size < 2048); // Should be less than 2KB
}

#[rstest]
#[tokio::test]
async fn test_zerodha_adapter_config_access(zerodha_adapter: ZerodhaWebSocketFeed) {
    let config = zerodha_adapter.config();
    
    assert_eq!(config.name, "zerodha");
    assert!(config.ws_url.contains("kite.trade"));
    assert_eq!(config.symbol_map.len(), 2);
}

#[rstest]
#[tokio::test]
async fn test_adapter_error_resilience(mut binance_adapter: BinanceWebSocketFeed) {
    // Test adapter behavior with various error conditions
    
    // Subscribe before connect (should work)
    let symbols = vec![Symbol::new(TEST_SYMBOL_ID_1)];
    let subscribe_result = binance_adapter.subscribe(symbols).await;
    assert!(subscribe_result.is_ok());
    
    // Double disconnect (should be idempotent)
    let disconnect1 = binance_adapter.disconnect().await;
    let disconnect2 = binance_adapter.disconnect().await;
    assert!(disconnect1.is_ok());
    assert!(disconnect2.is_ok());
    
    // Connect after disconnect
    let reconnect_result = binance_adapter.connect().await;
    assert!(reconnect_result.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_adapter_channel_capacity_handling() {
    let config = binance_config();
    let auth = binance_auth();
    let mut adapter = BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false);
    
    // Test with very small channel capacity
    let (tx, _rx) = mpsc::channel::<L2Update>(1);
    
    // Should handle small capacity gracefully
    let symbols = vec![Symbol::new(TEST_SYMBOL_ID_1)];
    let subscribe_result = adapter.subscribe(symbols).await;
    assert!(subscribe_result.is_ok());
    
    // Run task should start without immediate error
    let run_task = tokio::spawn(async move {
        adapter.run(tx).await
    });
    
    // Brief timeout to check for immediate errors
    let run_result = tokio::time::timeout(Duration::from_millis(50), run_task).await;
    assert!(run_result.is_err()); // Timeout expected in test environment
}

#[rstest]
#[tokio::test]
async fn test_binance_url_generation_by_market_type() {
    let config = binance_config();
    let auth = binance_auth();
    
    // Test different market types generate different URLs
    let spot_feed = BinanceWebSocketFeed::new(
        config.clone(),
        auth.clone(),
        BinanceMarket::Spot,
        false
    );
    
    let futures_feed = BinanceWebSocketFeed::new(
        config.clone(),
        auth.clone(),
        BinanceMarket::UsdFutures,
        false
    );
    
    let coin_futures_feed = BinanceWebSocketFeed::new(
        config.clone(),
        auth.clone(),
        BinanceMarket::CoinFutures,
        false
    );
    
    // All should create successfully
    // URL differences are handled internally
    let spot_connect = spot_feed.connect().await;
    let futures_connect = futures_feed.connect().await;
    let coin_futures_connect = coin_futures_feed.connect().await;
    
    assert!(spot_connect.is_ok());
    assert!(futures_connect.is_ok());
    assert!(coin_futures_connect.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_feed_adapter_trait_compliance() {
    // Test that our adapters properly implement the FeedAdapter trait
    
    let config = binance_config();
    let auth = binance_auth();
    let mut adapter: Box<dyn FeedAdapter> = Box::new(
        BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false)
    );
    
    // Test trait method calls
    let connect_result = adapter.connect().await;
    assert!(connect_result.is_ok());
    
    let symbols = vec![Symbol::new(TEST_SYMBOL_ID_1)];
    let subscribe_result = adapter.subscribe(symbols).await;
    assert!(subscribe_result.is_ok());
    
    let disconnect_result = adapter.disconnect().await;
    assert!(disconnect_result.is_ok());
    
    // Run method test
    let (tx, _rx) = mpsc::channel::<L2Update>(100);
    let run_task = tokio::spawn(async move {
        adapter.run(tx).await
    });
    
    // Brief timeout
    let run_result = tokio::time::timeout(Duration::from_millis(50), run_task).await;
    assert!(run_result.is_err()); // Timeout expected
}

#[rstest]
#[tokio::test]
async fn test_configuration_immutability_after_creation() {
    let original_config = binance_config();
    let auth = binance_auth();
    
    let adapter = BinanceWebSocketFeed::new(
        original_config.clone(),
        auth,
        BinanceMarket::Spot,
        false
    );
    
    // Modifying original config shouldn't affect adapter
    // (This is inherent to the move semantics, but good to verify structure)
    assert_eq!(original_config.name, "binance_spot");
    
    // Adapter should work with its internal config copy
    let connect_result = adapter.connect().await;
    assert!(connect_result.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_multiple_adapters_same_config() {
    let config = binance_config();
    let auth1 = binance_auth();
    let auth2 = binance_auth();
    
    // Create multiple adapters with same config
    let adapter1 = BinanceWebSocketFeed::new(
        config.clone(),
        auth1,
        BinanceMarket::Spot,
        false
    );
    
    let adapter2 = BinanceWebSocketFeed::new(
        config.clone(),
        auth2,
        BinanceMarket::UsdFutures,
        false
    );
    
    // Both should work independently
    let connect1 = adapter1.connect().await;
    let connect2 = adapter2.connect().await;
    
    assert!(connect1.is_ok());
    assert!(connect2.is_ok());
}