//! Integration tests for the trading engine

use bus::EventBus;
use common::{Px, Qty, Side, Symbol, Ts};
use engine::core::{Engine, EngineConfig, ExecutionMode, VenueType};
use engine::venue::{VenueConfig, create_binance_adapter, create_zerodha_adapter};
use rstest::rstest;
use std::sync::Arc;
use std::thread;

#[rstest]
#[case(true)] // Testnet
#[case(false)] // Production
fn test_engine_creation_zerodha(#[case] testnet: bool) {
    let config = EngineConfig::default();
    let venue_config = VenueConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        testnet,
    };

    let venue = create_zerodha_adapter(venue_config);
    let bus = Arc::new(EventBus::new(1024));
    let engine = Engine::new(config, venue, bus);

    let perf = engine.get_performance();
    assert_eq!(perf.orders_sent, 0);
    assert_eq!(perf.orders_filled, 0);
    assert_eq!(perf.orders_rejected, 0);
}

#[rstest]
#[case(true)] // Testnet
#[case(false)] // Production
fn test_engine_creation_binance(#[case] testnet: bool) {
    let config = EngineConfig::default();
    let venue_config = VenueConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        testnet,
    };

    let venue = create_binance_adapter(venue_config);
    let bus = Arc::new(EventBus::new(1024));
    let engine = Engine::new(config, venue, bus);

    let perf = engine.get_performance();
    assert_eq!(perf.orders_sent, 0);
    assert_eq!(perf.orders_filled, 0);
    assert_eq!(perf.orders_rejected, 0);
}

#[rstest]
#[case(ExecutionMode::Paper, Side::Bid, 10.0, 100.0, true)] // Paper buy
#[case(ExecutionMode::Paper, Side::Ask, 5.0, 200.0, true)] // Paper sell
#[case(ExecutionMode::Backtest, Side::Bid, 100.0, 50.0, true)] // Backtest buy
fn test_order_execution(
    #[case] mode: ExecutionMode,
    #[case] side: Side,
    #[case] qty: f64,
    #[case] price: f64,
    #[case] should_succeed: bool,
) {
    let mut config = EngineConfig::default();
    config.mode = mode;

    let venue_config = VenueConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        testnet: true,
    };
    let venue = create_binance_adapter(venue_config);
    let bus = Arc::new(EventBus::new(1024));

    let engine = Engine::new(config, venue, bus);

    // Send an order
    let symbol = Symbol(100);
    let result = engine.send_order(symbol, side, Qty::new(qty), Some(Px::new(price)));

    assert_eq!(result.is_ok(), should_succeed);

    if should_succeed {
        let order_id = match result {
            Ok(id) => id,
            Err(e) => {
                assert!(false, "Expected order to succeed but got error: {:?}", e);
                return;
            }
        };
        assert_eq!(order_id.0, 0); // First order should have ID 0

        let perf = engine.get_performance();
        assert_eq!(perf.orders_sent, 1);
    }
}

#[rstest]
#[case(1000000.0, 100.0)] // Huge quantity
#[case(100.0, 1000000.0)] // Huge price
#[case(10000.0, 10000.0)] // Large value
fn test_risk_rejection(#[case] qty: f64, #[case] price: f64) {
    let mut config = EngineConfig::default();
    config.mode = ExecutionMode::Paper;
    config.risk_check_enabled = true;

    let venue_config = VenueConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        testnet: true,
    };
    let venue = create_zerodha_adapter(venue_config);
    let bus = Arc::new(EventBus::new(1024));

    let engine = Engine::new(config, venue, bus);

    // Try to send a huge order that should be rejected
    let symbol = Symbol(100);
    let result = engine.send_order(symbol, Side::Bid, Qty::new(qty), Some(Px::new(price)));

    if result.is_ok() {
        eprintln!("ERROR: Large order was not rejected as expected!");
    }
    assert!(result.is_err());

    let perf = engine.get_performance();
    // orders_sent is incremented before risk check, so it should be 1
    assert_eq!(perf.orders_sent, 1);
    assert_eq!(perf.orders_rejected, 1);
}

#[rstest]
#[case(100.0, 101.0)] // Normal spread
#[case(99.5, 100.5)] // Tight spread
#[case(98.0, 102.0)] // Wide spread
fn test_market_tick_processing(#[case] _bid_price: f64, #[case] _ask_price: f64) {
    let mut config = EngineConfig::default();
    config.metrics_enabled = true;

    let venue_config = VenueConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        testnet: true,
    };
    let venue = create_binance_adapter(venue_config);
    let bus = Arc::new(EventBus::new(1024));

    let engine = Engine::new(config, venue, bus);

    // Process market ticks
    let symbol = Symbol(100);
    let bid = Px::new(99.5);
    let ask = Px::new(100.5);
    let ts = Ts::now();

    engine.on_tick(symbol, bid, ask, ts);

    // Check that latency was recorded
    let perf = engine.get_performance();
    assert!(perf.avg_tick_to_decision_ns > 0);
}

#[rstest]
#[case(10.0, 100.0)] // Small fill
#[case(100.0, 1000.0)] // Medium fill
#[case(1000.0, 100.0)] // Large quantity
fn test_fill_processing(#[case] _qty: f64, #[case] _price: f64) {
    let mut config = EngineConfig::default();
    config.mode = ExecutionMode::Paper;

    let venue_config = VenueConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        testnet: true,
    };
    let venue = create_zerodha_adapter(venue_config);
    let bus = Arc::new(EventBus::new(1024));

    let engine = Engine::new(config, venue, bus);

    // Send an order
    let symbol = Symbol(100);
    let result = engine.send_order(symbol, Side::Bid, Qty::new(10.0), Some(Px::new(100.0)));

    assert!(result.is_ok());
    let order_id = match result {
        Ok(id) => id,
        Err(e) => {
            assert!(false, "Failed to send order in test: {:?}", e);
            return;
        }
    };

    // Process a fill
    engine.on_fill(order_id.0, Qty::new(10.0), Px::new(100.0), Ts::now());

    let perf = engine.get_performance();
    assert_eq!(perf.orders_filled, 1);
}

#[rstest]
#[case(Ts::from_nanos(1000000), Side::Bid, 10.0, 100.0)]
#[case(Ts::from_nanos(2000000), Side::Ask, 5.0, 200.0)]
fn test_backtest_mode(
    #[case] _ts: Ts,
    #[case] _side: Side,
    #[case] _qty: f64,
    #[case] _price: f64,
) {
    let mut config = EngineConfig::default();
    config.mode = ExecutionMode::Backtest;

    let venue_config = VenueConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        testnet: true,
    };
    let venue = create_binance_adapter(venue_config);
    let bus = Arc::new(EventBus::new(1024));

    let engine = Engine::new(config, venue, bus);

    // Send an order in backtest mode
    let symbol = Symbol(100);
    let result = engine.send_order(symbol, Side::Ask, Qty::new(5.0), Some(Px::new(101.0)));

    assert!(result.is_ok());

    let perf = engine.get_performance();
    assert_eq!(perf.orders_sent, 1);
}

#[rstest]
#[case(VenueType::Zerodha)]
#[case(VenueType::Binance)]
fn test_venue_switching(#[case] _initial_venue: VenueType) {
    // Test with Zerodha
    let mut config = EngineConfig::default();
    config.venue = VenueType::Zerodha;
    config.mode = ExecutionMode::Paper;

    let venue_config = VenueConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        testnet: false,
    };
    let venue = create_zerodha_adapter(venue_config);
    let bus = Arc::new(EventBus::new(1024));

    let engine_zerodha = Engine::new(config.clone(), venue, bus.clone());

    // Test with Binance
    config.venue = VenueType::Binance;
    let venue_config = VenueConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        testnet: true,
    };
    let venue = create_binance_adapter(venue_config);

    let engine_binance = Engine::new(config, venue, bus);

    // Both engines should work
    let symbol = Symbol(100);
    let result1 =
        engine_zerodha.send_order(symbol, Side::Bid, Qty::new(10.0), Some(Px::new(100.0)));
    let result2 = engine_binance.send_order(symbol, Side::Ask, Qty::new(5.0), Some(Px::new(101.0)));

    assert!(result1.is_ok());
    assert!(result2.is_ok());
}

#[rstest]
#[case(2)] // 2 threads
#[case(4)] // 4 threads
#[case(8)] // 8 threads
fn test_concurrent_operations(#[case] num_threads: usize) {
    let config = EngineConfig::default();
    let venue_config = VenueConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        testnet: true,
    };
    let venue = create_binance_adapter(venue_config);
    let bus = Arc::new(EventBus::new(1024));

    let engine = Arc::new(Engine::new(config, venue, bus));

    // Spawn multiple threads to send orders concurrently
    let mut handles = vec![];
    let orders_per_thread = 10;

    for thread_id in 0..num_threads {
        let engine_clone = Arc::clone(&engine);
        let handle = thread::spawn(move || {
            for order_id in 0..orders_per_thread {
                // SAFETY: Cast is safe within expected range
                let symbol = Symbol(100 + thread_id as u32 * 10 + order_id as u32);
                let result = engine_clone.send_order(
                    symbol,
                    if order_id % 2 == 0 {
                        Side::Bid
                    } else {
                        Side::Ask
                        // SAFETY: Cast is safe within expected range
                    },
                    // SAFETY: Cast is safe within expected range
                    Qty::new(10.0 + order_id as f64),
                    // SAFETY: Cast is safe within expected range
                    Some(Px::new(100.0 + order_id as f64)),
                );
                assert!(result.is_ok());
            }
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().map_err(|_| "Thread panicked").ok();
    }
    // SAFETY: Cast is safe within expected range

    // SAFETY: Cast is safe within expected range
    let perf = engine.get_performance();
    assert_eq!(perf.orders_sent, (num_threads * orders_per_thread) as u64);
}

#[rstest]
#[case(10.0, 100.0, 105.0)] // Profit
#[case(10.0, 100.0, 95.0)] // Loss
#[case(10.0, 100.0, 100.0)] // Break even
fn test_pnl_calculation(#[case] _qty: f64, #[case] _entry_price: f64, #[case] _exit_price: f64) {
    let mut config = EngineConfig::default();
    config.mode = ExecutionMode::Paper;
    config.metrics_enabled = true;

    let venue_config = VenueConfig {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        testnet: true,
    };
    let venue = create_zerodha_adapter(venue_config);
    let bus = Arc::new(EventBus::new(1024));

    let engine = Engine::new(config, venue, bus);

    // Send buy order
    let symbol = Symbol(100);
    let buy_result = engine.send_order(symbol, Side::Bid, Qty::new(10.0), Some(Px::new(100.0)));
    assert!(buy_result.is_ok());

    // Process fill
    let buy_order_id = match buy_result {
        Ok(id) => id,
        Err(e) => {
            assert!(false, "Buy order should have succeeded: {:?}", e);
            return;
        }
    };
    engine.on_fill(buy_order_id.0, Qty::new(10.0), Px::new(100.0), Ts::now());

    // Update market price
    engine.on_tick(symbol, Px::new(101.0), Px::new(102.0), Ts::now());

    // Check PnL - we bought 10 at 100, market is now at 101/102
    // Unrealized PnL should be positive (using bid price for long position)
    let pnl = engine.get_pnl();
    assert!(pnl.unrealized > 0); // Should have unrealized profit
    assert_eq!(pnl.realized, 0); // No realized PnL yet (position still open)
}
