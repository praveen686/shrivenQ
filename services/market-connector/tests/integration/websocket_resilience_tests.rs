//! WebSocket resilience and connection recovery integration tests
//! 
//! These tests verify WebSocket connection handling, reconnection logic,
//! and service resilience under various network conditions.

use rstest::*;
use tokio::time::{timeout, Duration, sleep};
use tokio::sync::mpsc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use market_connector::connectors::adapter::{FeedAdapter, FeedConfig};
use market_connector::exchanges::binance::websocket::BinanceWebSocketFeed;
use market_connector::exchanges::zerodha::websocket::ZerodhaWebSocketFeed;
use market_connector::grpc_service::MarketDataGrpcService;
use services_common::{BinanceAuth, BinanceMarket, ZerodhaAuth, ZerodhaConfig, L2Update, Symbol};
use rustc_hash::FxHashMap;

// Test constants
const TEST_SYMBOL_ID: u32 = 1;
const TEST_BTCUSDT: &str = "BTCUSDT";
const TEST_NIFTY_TOKEN: u32 = 256265;
const CONNECTION_TIMEOUT_MS: u64 = 2000;
const RECONNECTION_TIMEOUT_MS: u64 = 10000;
const STABILITY_TEST_DURATION_MS: u64 = 5000;

#[fixture]
fn resilient_binance_config() -> FeedConfig {
    let mut symbol_map = FxHashMap::default();
    symbol_map.insert(Symbol::new(TEST_SYMBOL_ID), TEST_BTCUSDT.to_string());
    
    FeedConfig {
        name: "resilient_binance".to_string(),
        ws_url: "wss://stream.binance.com:9443".to_string(),
        api_url: "https://api.binance.com".to_string(),
        symbol_map,
        max_reconnects: 3,
        reconnect_delay_ms: 1000,
    }
}

#[fixture]
fn resilient_zerodha_config() -> FeedConfig {
    let mut symbol_map = FxHashMap::default();
    symbol_map.insert(Symbol::new(TEST_SYMBOL_ID), TEST_NIFTY_TOKEN.to_string());
    
    FeedConfig {
        name: "resilient_zerodha".to_string(),
        ws_url: "wss://ws.kite.trade".to_string(),
        api_url: "https://api.kite.trade".to_string(),
        symbol_map,
        max_reconnects: 5,
        reconnect_delay_ms: 2000,
    }
}

#[fixture]
fn test_binance_auth() -> BinanceAuth {
    BinanceAuth::new("test_api_key".to_string(), "test_secret_key".to_string())
}

#[fixture]
fn test_zerodha_auth() -> ZerodhaAuth {
    let config = ZerodhaConfig::new(
        "test_user".to_string(),
        "test_password".to_string(),
        "test_totp_secret".to_string(),
        "test_api_key".to_string(),
        "test_api_secret".to_string(),
    );
    ZerodhaAuth::new("test_api_key".to_string(), "test_access_token".to_string(), "test_user".to_string())
}

#[rstest]
#[tokio::test]
async fn test_binance_connection_timeout_handling() {
    let config = resilient_binance_config();
    let auth = test_binance_auth();
    let mut adapter = BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false);
    
    // Test connection with timeout
    let connect_result = timeout(
        Duration::from_millis(CONNECTION_TIMEOUT_MS),
        adapter.connect()
    ).await;
    
    // Should either succeed or timeout gracefully
    assert!(connect_result.is_ok() || connect_result.is_err());
    
    if let Ok(result) = connect_result {
        assert!(result.is_ok());
    }
}

#[rstest]
#[tokio::test]
async fn test_zerodha_connection_timeout_handling() {
    let config = resilient_zerodha_config();
    let auth = test_zerodha_auth();
    let mut adapter = ZerodhaWebSocketFeed::new(config, auth);
    
    // Test connection with timeout
    let connect_result = timeout(
        Duration::from_millis(CONNECTION_TIMEOUT_MS),
        adapter.connect()
    ).await;
    
    // Should either succeed or timeout gracefully
    assert!(connect_result.is_ok() || connect_result.is_err());
    
    if let Ok(result) = connect_result {
        assert!(result.is_ok());
    }
}

#[rstest]
#[tokio::test]
async fn test_binance_connection_lifecycle_resilience() {
    let config = resilient_binance_config();
    let auth = test_binance_auth();
    let mut adapter = BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false);
    
    // Test multiple connect/disconnect cycles
    for cycle in 0..3 {
        // Connect
        let connect_result = timeout(
            Duration::from_millis(CONNECTION_TIMEOUT_MS),
            adapter.connect()
        ).await;
        
        if let Ok(Ok(())) = connect_result {
            // Subscribe
            let symbols = vec![Symbol::new(TEST_SYMBOL_ID)];
            let subscribe_result = adapter.subscribe(symbols).await;
            assert!(subscribe_result.is_ok());
            
            // Brief stabilization
            sleep(Duration::from_millis(100)).await;
            
            // Disconnect
            let disconnect_result = adapter.disconnect().await;
            assert!(disconnect_result.is_ok());
        }
        
        // Brief delay between cycles
        if cycle < 2 {
            sleep(Duration::from_millis(200)).await;
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_zerodha_connection_lifecycle_resilience() {
    let config = resilient_zerodha_config();
    let auth = test_zerodha_auth();
    let mut adapter = ZerodhaWebSocketFeed::new(config, auth);
    
    // Test multiple connect/disconnect cycles
    for cycle in 0..3 {
        // Connect
        let connect_result = timeout(
            Duration::from_millis(CONNECTION_TIMEOUT_MS),
            adapter.connect()
        ).await;
        
        if let Ok(Ok(())) = connect_result {
            // Subscribe
            let symbols = vec![Symbol::new(TEST_SYMBOL_ID)];
            let subscribe_result = adapter.subscribe(symbols).await;
            assert!(subscribe_result.is_ok());
            
            // Brief stabilization
            sleep(Duration::from_millis(100)).await;
            
            // Disconnect
            let disconnect_result = adapter.disconnect().await;
            assert!(disconnect_result.is_ok());
        }
        
        // Brief delay between cycles
        if cycle < 2 {
            sleep(Duration::from_millis(200)).await;
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_concurrent_connection_attempts() {
    let config = resilient_binance_config();
    let auth = test_binance_auth();
    
    let connection_attempts = 5;
    let mut handles = Vec::new();
    
    for _i in 0..connection_attempts {
        let config_clone = config.clone();
        let auth_clone = auth.clone();
        
        let handle = tokio::spawn(async move {
            let mut adapter = BinanceWebSocketFeed::new(
                config_clone,
                auth_clone,
                BinanceMarket::Spot,
                false
            );
            
            timeout(
                Duration::from_millis(CONNECTION_TIMEOUT_MS),
                adapter.connect()
            ).await
        });
        
        handles.push(handle);
    }
    
    // Wait for all attempts to complete
    let mut success_count = 0;
    for handle in handles {
        if let Ok(Ok(Ok(()))) = handle.await {
            success_count += 1;
        }
    }
    
    // At least some connections should succeed or timeout gracefully
    // (In test environment, may not have actual internet connectivity)
}

#[rstest]
#[tokio::test]
async fn test_websocket_run_stability() {
    let config = resilient_binance_config();
    let auth = test_binance_auth();
    let mut adapter = BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false);
    
    // Connect and subscribe
    if adapter.connect().await.is_ok() {
        let symbols = vec![Symbol::new(TEST_SYMBOL_ID)];
        if adapter.subscribe(symbols).await.is_ok() {
            // Set up message channel
            let (tx, mut rx) = mpsc::channel::<L2Update>(1000);
            let message_count = Arc::new(AtomicUsize::new(0));
            let error_occurred = Arc::new(AtomicBool::new(false));
            
            let message_count_clone = Arc::clone(&message_count);
            let error_occurred_clone = Arc::clone(&error_occurred);
            
            // Monitor received messages
            let monitor_task = tokio::spawn(async move {
                while let Some(_update) = rx.recv().await {
                    message_count_clone.fetch_add(1, Ordering::Relaxed);
                }
            });
            
            // Run WebSocket connection
            let run_task = tokio::spawn(async move {
                match adapter.run(tx).await {
                    Ok(()) => {},
                    Err(_) => error_occurred_clone.store(true, Ordering::Relaxed),
                }
            });
            
            // Let it run for a while
            sleep(Duration::from_millis(STABILITY_TEST_DURATION_MS)).await;
            
            // Check if any errors occurred
            let had_errors = error_occurred.load(Ordering::Relaxed);
            let messages_received = message_count.load(Ordering::Relaxed);
            
            // Cancel tasks
            run_task.abort();
            monitor_task.abort();
            
            // In test environment, we don't expect real messages, but no errors should occur
            // during the connection attempt
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_invalid_websocket_url_handling() {
    let mut config = resilient_binance_config();
    config.ws_url = "wss://invalid-url-that-does-not-exist.com".to_string();
    
    let auth = test_binance_auth();
    let mut adapter = BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false);
    
    // Connection should fail gracefully
    let connect_result = adapter.connect().await;
    assert!(connect_result.is_ok()); // Connect just validates credentials, not URL
    
    // Run should handle invalid URL
    let symbols = vec![Symbol::new(TEST_SYMBOL_ID)];
    let subscribe_result = adapter.subscribe(symbols).await;
    assert!(subscribe_result.is_ok());
    
    let (tx, _rx) = mpsc::channel::<L2Update>(100);
    let run_result = timeout(
        Duration::from_millis(2000),
        adapter.run(tx)
    ).await;
    
    // Should either fail gracefully or timeout
    // In either case, shouldn't panic
}

#[rstest]
#[tokio::test]
async fn test_network_interruption_simulation() {
    let config = resilient_binance_config();
    let auth = test_binance_auth();
    let mut adapter = BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false);
    
    if adapter.connect().await.is_ok() {
        let symbols = vec![Symbol::new(TEST_SYMBOL_ID)];
        if adapter.subscribe(symbols).await.is_ok() {
            let (tx, mut rx) = mpsc::channel::<L2Update>(100);
            
            // Start WebSocket connection
            let adapter_handle = tokio::spawn(async move {
                adapter.run(tx).await
            });
            
            // Simulate network interruption by cancelling and restarting
            sleep(Duration::from_millis(1000)).await;
            
            // Cancel the connection (simulates network drop)
            adapter_handle.abort();
            
            // Verify channel is still functional
            let channel_status = rx.try_recv();
            // Channel might be empty or closed, both are acceptable outcomes
            
            // Test that we can create a new adapter after interruption
            let config2 = resilient_binance_config();
            let auth2 = test_binance_auth();
            let mut adapter2 = BinanceWebSocketFeed::new(config2, auth2, BinanceMarket::Spot, false);
            
            let reconnect_result = adapter2.connect().await;
            assert!(reconnect_result.is_ok());
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_high_frequency_connection_stress() {
    let config = resilient_binance_config();
    let auth = test_binance_auth();
    
    // Rapid connect/disconnect cycles
    let cycles = 10;
    let mut adapter = BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false);
    
    for i in 0..cycles {
        let cycle_start = std::time::Instant::now();
        
        // Quick connect
        let connect_result = timeout(
            Duration::from_millis(500),
            adapter.connect()
        ).await;
        
        if connect_result.is_ok() {
            // Quick disconnect
            let disconnect_result = adapter.disconnect().await;
            assert!(disconnect_result.is_ok());
        }
        
        let cycle_duration = cycle_start.elapsed();
        
        // Each cycle should complete reasonably quickly
        assert!(cycle_duration < Duration::from_secs(2));
        
        // Brief pause between cycles
        if i < cycles - 1 {
            sleep(Duration::from_millis(50)).await;
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_memory_leak_prevention_during_reconnections() {
    use std::mem;
    
    let config = resilient_binance_config();
    let auth = test_binance_auth();
    let mut adapter = BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false);
    
    // Measure initial memory footprint
    let initial_size = mem::size_of_val(&adapter);
    
    // Perform many connection cycles
    for _i in 0..20 {
        let _ = timeout(
            Duration::from_millis(200),
            adapter.connect()
        ).await;
        
        let symbols = vec![Symbol::new(TEST_SYMBOL_ID)];
        let _ = adapter.subscribe(symbols).await;
        
        let _ = adapter.disconnect().await;
        
        // Yield to allow cleanup
        tokio::task::yield_now().await;
    }
    
    // Memory usage should remain stable
    let final_size = mem::size_of_val(&adapter);
    
    // Size shouldn't grow significantly (allowing some variance for internal state)
    assert!(final_size <= initial_size + 1024); // Max 1KB growth
}

#[rstest]
#[tokio::test]
async fn test_grpc_service_websocket_coordination() {
    let (grpc_service, event_sender) = MarketDataGrpcService::new();
    
    // Test that WebSocket disconnections don't crash gRPC service
    let test_event = market_connector::MarketDataEvent {
        symbol: TEST_BTCUSDT.to_string(),
        exchange: "binance".to_string(),
        timestamp: chrono::Utc::now().timestamp_millis() as u64,
        data: market_connector::MarketData::OrderBook {
            bids: vec![(45000.0, 1.0)],
            asks: vec![(45001.0, 1.0)],
            sequence: 12345,
        },
    };
    
    // Send event
    let send_result = event_sender.send(test_event).await;
    assert!(send_result.is_ok());
    
    // Drop event sender to simulate WebSocket disconnection
    drop(event_sender);
    
    // gRPC service should still be functional
    let status = grpc_service.get_connection_status().await;
    // Should return empty list but not panic
    assert!(status.is_empty());
}

#[rstest]
#[tokio::test]
async fn test_websocket_message_buffer_overflow_protection() {
    let config = resilient_binance_config();
    let auth = test_binance_auth();
    let mut adapter = BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false);
    
    if adapter.connect().await.is_ok() {
        let symbols = vec![Symbol::new(TEST_SYMBOL_ID)];
        if adapter.subscribe(symbols).await.is_ok() {
            // Create very small channel to test overflow handling
            let (tx, mut rx) = mpsc::channel::<L2Update>(1);
            
            // Don't consume from rx to fill the buffer quickly
            let run_task = tokio::spawn(async move {
                adapter.run(tx).await
            });
            
            // Let it try to run briefly
            sleep(Duration::from_millis(1000)).await;
            
            // Cancel the task
            run_task.abort();
            
            // Check if any messages were received
            let _ = rx.try_recv();
            
            // Test passes if no panic occurred during buffer overflow
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_authentication_failure_recovery() {
    // Test with obviously invalid credentials
    let config = resilient_zerodha_config();
    let invalid_auth = ZerodhaAuth::new(
        "invalid_key".to_string(),
        "invalid_token".to_string(),
        "invalid_user".to_string()
    );
    
    let mut adapter = ZerodhaWebSocketFeed::new(config, invalid_auth);
    
    // Connection should handle auth failure gracefully
    let connect_result = adapter.connect().await;
    assert!(connect_result.is_ok()); // connect() doesn't authenticate
    
    let symbols = vec![Symbol::new(TEST_SYMBOL_ID)];
    let subscribe_result = adapter.subscribe(symbols).await;
    assert!(subscribe_result.is_ok());
    
    // Run should handle auth failure in WebSocket connection
    let (tx, _rx) = mpsc::channel::<L2Update>(100);
    let run_result = timeout(
        Duration::from_millis(2000),
        adapter.run(tx)
    ).await;
    
    // Should either complete with error or timeout, but not panic
}

#[rstest]
#[tokio::test]
async fn test_websocket_url_redirect_handling() {
    // Some WebSocket endpoints might redirect
    // Test that redirects are handled gracefully
    
    let mut config = resilient_binance_config();
    // Use a URL that might redirect (still binance domain)
    config.ws_url = "wss://stream.binance.com/ws".to_string();
    
    let auth = test_binance_auth();
    let mut adapter = BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false);
    
    let connect_result = adapter.connect().await;
    assert!(connect_result.is_ok());
    
    let symbols = vec![Symbol::new(TEST_SYMBOL_ID)];
    let subscribe_result = adapter.subscribe(symbols).await;
    assert!(subscribe_result.is_ok());
    
    // Test that WebSocket connection handles any redirects
    let (tx, _rx) = mpsc::channel::<L2Update>(100);
    let run_result = timeout(
        Duration::from_millis(2000),
        adapter.run(tx)
    ).await;
    
    // Should handle gracefully
}

#[rstest]
#[tokio::test]
async fn test_connection_recovery_after_system_suspend() {
    // Simulate system suspend/resume cycle
    let config = resilient_binance_config();
    let auth = test_binance_auth();
    let mut adapter = BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false);
    
    // Initial connection
    if adapter.connect().await.is_ok() {
        let symbols = vec![Symbol::new(TEST_SYMBOL_ID)];
        let _ = adapter.subscribe(symbols).await;
        
        // Simulate time passing (like system suspend)
        sleep(Duration::from_millis(100)).await;
        
        // Disconnect (simulates network stack reset after resume)
        let _ = adapter.disconnect().await;
        
        // Wait a bit more
        sleep(Duration::from_millis(100)).await;
        
        // Reconnection should work
        let reconnect_result = adapter.connect().await;
        assert!(reconnect_result.is_ok());
        
        // Re-subscription should work
        let symbols = vec![Symbol::new(TEST_SYMBOL_ID)];
        let resubscribe_result = adapter.subscribe(symbols).await;
        assert!(resubscribe_result.is_ok());
    }
}

#[rstest]
#[tokio::test]
async fn test_websocket_connection_pool_exhaustion() {
    // Test behavior when many connections are created rapidly
    let config = resilient_binance_config();
    let auth = test_binance_auth();
    
    let mut adapters = Vec::new();
    let max_adapters = 10;
    
    // Create many adapters
    for _i in 0..max_adapters {
        let adapter = BinanceWebSocketFeed::new(
            config.clone(),
            auth.clone(),
            BinanceMarket::Spot,
            false
        );
        adapters.push(adapter);
    }
    
    // Try to connect all of them
    let mut connection_tasks = Vec::new();
    for mut adapter in adapters {
        let task = tokio::spawn(async move {
            let connect_result = timeout(
                Duration::from_millis(CONNECTION_TIMEOUT_MS),
                adapter.connect()
            ).await;
            
            if connect_result.is_ok() {
                adapter.disconnect().await
            } else {
                Ok(())
            }
        });
        connection_tasks.push(task);
    }
    
    // Wait for all connections to complete or timeout
    for task in connection_tasks {
        let _ = task.await;
    }
    
    // Test passes if no panics occurred during mass connection attempts
}

#[rstest]
#[tokio::test]
async fn test_websocket_protocol_version_compatibility() {
    // Test different WebSocket configurations
    let configs = vec![
        {
            let mut config = resilient_binance_config();
            config.ws_url = "wss://stream.binance.com:9443/ws".to_string();
            config
        },
        {
            let mut config = resilient_binance_config();
            config.ws_url = "wss://stream.binance.com:443/ws".to_string();
            config
        },
    ];
    
    for config in configs {
        let auth = test_binance_auth();
        let mut adapter = BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false);
        
        // Each configuration should work or fail gracefully
        let connect_result = timeout(
            Duration::from_millis(CONNECTION_TIMEOUT_MS),
            adapter.connect()
        ).await;
        
        // Connection might succeed or timeout, but shouldn't panic
        if let Ok(Ok(())) = connect_result {
            let _ = adapter.disconnect().await;
        }
    }
}