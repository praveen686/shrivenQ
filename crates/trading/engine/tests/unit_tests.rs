//! Unit tests for engine components

mod memory_tests {
    use engine::memory::{Arena, ObjectPool};
    use rstest::rstest;

    #[rstest]
    #[case(1024)] // Standard size
    #[case(4096)] // Larger size
    #[case(256)] // Smaller size
    fn test_arena_allocation(#[case] size: usize) {
        let arena = Arena::new(size);

        // Allocate some memory
        let ptr1: Option<&mut u64> = arena.alloc();
        assert!(ptr1.is_some());

        let ptr2: Option<&mut u32> = arena.alloc();
        assert!(ptr2.is_some());

        // Write values
        if let Some(p1) = ptr1 {
            *p1 = 42;
            assert_eq!(*p1, 42);
        }

        if let Some(p2) = ptr2 {
            *p2 = 100;
            assert_eq!(*p2, 100);
        }
    }

    #[rstest]
    #[case(1024)] // Standard size
    #[case(2048)] // Larger size
    fn test_arena_alignment(#[case] size: usize) {
        let arena = Arena::new(size);

        // Allocate with different alignments
        let ptr1: Option<&mut u8> = arena.alloc();
        let ptr2: Option<&mut u64> = arena.alloc();
        let ptr3: Option<&mut u128> = arena.alloc();

        assert!(ptr1.is_some());
        assert!(ptr2.is_some());
        assert!(ptr3.is_some());

        // Check alignment
        if let Some(p2) = ptr2 {
            let addr = p2 as *mut u64 as usize;
            assert_eq!(addr % 8, 0); // u64 should be 8-byte aligned
        }

        if let Some(p3) = ptr3 {
            let addr = p3 as *mut u128 as usize;
            assert_eq!(addr % 16, 0); // u128 should be 16-byte aligned
        }
    }

    #[rstest]
    #[case(64)] // Very small arena
    #[case(128)] // Small arena
    #[case(32)] // Tiny arena
    fn test_arena_exhaustion(#[case] size: usize) {
        let arena = Arena::new(size);

        // Try to allocate more than available
        let mut allocations = 0;
        loop {
            let ptr: Option<&mut u64> = arena.alloc();
            if ptr.is_none() {
                break;
            }
            allocations += 1;
            if allocations > 100 {
                panic!("Too many allocations, something is wrong");
            }
        }

        assert!(allocations > 0);
        assert!(allocations < size / 8 + 2); // Should run out based on arena size
    }

    #[rstest]
    #[case(10)] // Small pool
    #[case(100)] // Medium pool
    #[case(1000)] // Large pool
    fn test_object_pool(#[case] capacity: usize) {
        #[derive(Debug, Default)]
        struct TestObj {
            _value: u64,
        }

        let pool: ObjectPool<TestObj> = ObjectPool::new(capacity);

        // Acquire objects
        let obj1 = pool.acquire();
        assert!(obj1.is_some());

        let obj2 = pool.acquire();
        assert!(obj2.is_some());

        // Modify objects
        if let Some(o1) = obj1 {
            o1._value = 42;
            assert_eq!(o1._value, 42);

            // Release back to pool
            pool.release(o1);
        }

        // Acquire again - should get the same slot
        let obj3 = pool.acquire();
        assert!(obj3.is_some());
    }

    #[rstest]
    #[case(2)] // Tiny pool
    #[case(5)] // Small pool
    #[case(10)] // Medium pool
    fn test_object_pool_exhaustion(#[case] capacity: usize) {
        #[derive(Debug, Default)]
        struct TestObj {
            _value: u64,
        }

        let pool: ObjectPool<TestObj> = ObjectPool::new(capacity);

        // Acquire all objects
        let mut objects = Vec::new();
        for _ in 0..capacity {
            let obj = pool.acquire();
            assert!(obj.is_some());
            objects.push(obj);
        }

        // Pool should be exhausted
        let extra = pool.acquire();
        assert!(extra.is_none());

        // Release one
        if let Some(o1) = objects.pop().flatten() {
            pool.release(o1);
        }

        // Should be able to acquire again
        let reacquired = pool.acquire();
        assert!(reacquired.is_some());
    }
}

mod risk_tests {
    use common::{Px, Qty, Side, Symbol};
    use engine::core::EngineConfig;
    use engine::risk::RiskEngine;
    use rstest::rstest;
    use std::sync::Arc;

    #[rstest]
    #[case(Side::Bid, 100.0, 100.0, true)] // Small order should pass
    #[case(Side::Bid, 100000.0, 100.0, false)] // Huge order should fail
    #[case(Side::Ask, 100.0, 100.0, true)] // Medium order should pass
    #[case(Side::Ask, 10000.0, 100.0, false)] // Large order should fail
    fn test_risk_order_size_check(
        #[case] side: Side,
        #[case] qty: f64,
        #[case] price: f64,
        #[case] should_pass: bool,
    ) {
        let config = Arc::new(EngineConfig::default());
        let risk = RiskEngine::new(config);
        let symbol = Symbol(100);

        let result = risk.check_order(symbol, side, Qty::new(qty), Some(Px::new(price)));
        assert_eq!(result, should_pass);
    }

    #[rstest]
    #[case(Symbol(100), Side::Bid, 100.0, 100.0, Side::Ask, 50.0, 101.0, true)]
    #[case(Symbol(200), Side::Ask, 200.0, 50.0, Side::Bid, 100.0, 51.0, true)]
    fn test_risk_position_update(
        #[case] symbol: Symbol,
        #[case] pos_side: Side,
        #[case] pos_qty: f64,
        #[case] pos_price: f64,
        #[case] order_side: Side,
        #[case] order_qty: f64,
        #[case] order_price: f64,
        #[case] should_pass: bool,
    ) {
        let config = Arc::new(EngineConfig::default());
        let risk = RiskEngine::new(config);

        // Update position
        risk.update_position(symbol, pos_side, Qty::new(pos_qty), Px::new(pos_price));

        // Check if we can still trade
        let pass = risk.check_order(
            symbol,
            order_side,
            Qty::new(order_qty),
            Some(Px::new(order_price)),
        );
        assert_eq!(pass, should_pass);
    }

    #[rstest]
    #[case(10000, 10000, 0)] // Profit - no drawdown
    #[case(-5000, -5000, 5000)] // Loss - creates drawdown
    #[case(0, 0, 0)] // Break even
    fn test_risk_pnl_tracking(
        #[case] pnl_update: i64,
        #[case] expected_daily_pnl: i64,
        #[case] min_expected_drawdown: i64,
    ) {
        let config = Arc::new(EngineConfig::default());
        let risk = RiskEngine::new(config);

        risk.update_pnl(pnl_update);

        let metrics = risk.get_metrics();
        assert_eq!(metrics.daily_pnl, expected_daily_pnl);
        assert!(metrics.current_drawdown >= min_expected_drawdown);
    }

    #[rstest]
    #[case(Symbol(100), Side::Bid, 10.0, 100.0)]
    #[case(Symbol(200), Side::Ask, 5.0, 200.0)]
    fn test_risk_emergency_stop(
        #[case] symbol: Symbol,
        #[case] side: Side,
        #[case] qty: f64,
        #[case] price: f64,
    ) {
        let config = Arc::new(EngineConfig::default());
        let risk = RiskEngine::new(config);

        // Should pass before emergency stop
        let pass = risk.check_order(symbol, side, Qty::new(qty), Some(Px::new(price)));
        assert!(pass);

        // Trigger emergency stop
        risk.emergency_stop();

        // Should fail after emergency stop
        let fail = risk.check_order(symbol, side, Qty::new(qty), Some(Px::new(price)));
        assert!(!fail);

        // Resume
        risk.resume();

        // Should pass again
        let pass_again = risk.check_order(symbol, side, Qty::new(qty), Some(Px::new(price)));
        assert!(pass_again);
    }

    #[rstest]
    #[case(-10000)] // Loss
    #[case(5000)] // Profit
    #[case(0)] // Break even
    fn test_risk_daily_reset(#[case] initial_pnl: i64) {
        let config = Arc::new(EngineConfig::default());
        let risk = RiskEngine::new(config);

        // Set some PnL
        risk.update_pnl(initial_pnl);

        let metrics = risk.get_metrics();
        assert_eq!(metrics.daily_pnl, initial_pnl);

        // Reset daily counters
        risk.reset_daily();

        let metrics = risk.get_metrics();
        assert_eq!(metrics.daily_pnl, 0);
    }
}

mod venue_tests {
    use common::{Px, Qty, Side, Symbol};
    use engine::venue::{
        OrderStatus, VenueAdapter, VenueConfig, create_binance_adapter, create_zerodha_adapter,
    };
    use rstest::rstest;

    #[rstest]
    #[case("zerodha", false, 100, Side::Bid, 10.0, 100.0)]
    #[case("binance", true, 200, Side::Ask, 5.0, 50000.0)]
    fn test_venue_adapter(
        #[case] venue_type: &str,
        #[case] testnet: bool,
        #[case] symbol_id: u32,
        #[case] side: Side,
        #[case] qty: f64,
        #[case] price: f64,
    ) {
        let config = VenueConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            testnet,
        };

        let adapter: Box<dyn VenueAdapter> = match venue_type {
            "zerodha" => Box::new(create_zerodha_adapter(config)),
            "binance" => Box::new(create_binance_adapter(config)),
            _ => panic!("Unknown venue type"),
        };

        // Check market hours
        let is_open = adapter.is_market_open();
        if venue_type == "binance" {
            assert!(is_open); // Binance is 24/7
        }

        // Get latency
        let latency = adapter.get_latency_ns();
        assert!(latency > 0);

        // Send order
        let symbol = Symbol(symbol_id);
        let result = adapter.send_order(symbol, side, Qty::new(qty), Some(Px::new(price)));

        if result.is_ok() {
            let order_id = result.unwrap();

            // Check order status
            let status = adapter.get_order_status(order_id);
            assert!(status.is_some());
            assert_eq!(status.unwrap(), OrderStatus::Accepted);

            // Cancel order
            let cancel_result = adapter.cancel_order(order_id);
            assert!(cancel_result.is_ok());
        }
    }

    #[rstest]
    #[case(Symbol(12345), 12345)]
    #[case(Symbol(99999), 99999)]
    #[case(Symbol(1), 1)]
    fn test_symbol_mapping(#[case] symbol: Symbol, #[case] expected: u32) {
        let config = VenueConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            testnet: false,
        };

        let zerodha = create_zerodha_adapter(config.clone());
        let binance = create_binance_adapter(config);

        let zerodha_id = zerodha.map_symbol(symbol);
        let binance_id = binance.map_symbol(symbol);

        assert_eq!(zerodha_id, expected);
        assert_eq!(binance_id, expected);
    }
}

mod execution_tests {
    use common::{Px, Qty, Side, Symbol, Ts};
    use engine::core::{EngineConfig, ExecutionMode};
    use engine::execution::{ExecutionLayer, Order, OrderPool};
    use engine::venue::{VenueConfig, create_binance_adapter};
    use rstest::rstest;
    use std::sync::Arc;

    #[rstest]
    #[case(10)] // Small pool
    #[case(100)] // Medium pool
    #[case(1000)] // Large pool
    fn test_order_pool(#[case] capacity: usize) {
        let pool = OrderPool::new(capacity);

        // Acquire order
        let order = pool.acquire();
        assert!(order.is_some());

        if let Some(o) = order {
            *o = Order::new(1, Symbol(100), 0, Qty::new(10.0), Some(Px::new(100.0)));
            assert_eq!(o.id, 1);

            pool.release(o);
        }

        // Should be able to acquire again
        let order2 = pool.acquire();
        assert!(order2.is_some());
    }

    #[rstest]
    #[case(ExecutionMode::Paper, Symbol(100), Side::Bid, 10.0, 100.0)]
    #[case(ExecutionMode::Paper, Symbol(200), Side::Ask, 5.0, 200.0)]
    fn test_paper_trading_execution(
        #[case] mode: ExecutionMode,
        #[case] symbol: Symbol,
        #[case] side: Side,
        #[case] qty: f64,
        #[case] price: f64,
    ) {
        let mut config = EngineConfig::default();
        config.mode = mode;
        let config = Arc::new(config);

        let venue_config = VenueConfig {
            api_key: "test".to_string(),
            api_secret: "test".to_string(),
            testnet: true,
        };
        let venue = create_binance_adapter(venue_config);

        let exec = ExecutionLayer::new(config, venue);

        // Simulate order
        let result = exec.simulate_order(1, symbol, side, Qty::new(qty), Some(Px::new(price)));

        assert!(result.is_ok());

        // Check fills
        let fills = exec.get_fills(1);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].order_id, 1);
        assert_eq!(fills[0].quantity, Qty::new(qty));
    }

    #[rstest]
    #[case(Symbol(100), Side::Bid, 10.0, 100.0)]
    #[case(Symbol(200), Side::Ask, 5.0, 50.0)]
    fn test_backtest_execution(
        #[case] symbol: Symbol,
        #[case] side: Side,
        #[case] qty: f64,
        #[case] price: f64,
    ) {
        let mut config = EngineConfig::default();
        config.mode = ExecutionMode::Backtest;
        let config = Arc::new(config);

        let venue_config = VenueConfig {
            api_key: "test".to_string(),
            api_secret: "test".to_string(),
            testnet: true,
        };
        let venue = create_binance_adapter(venue_config);

        let exec = ExecutionLayer::new(config, venue);

        // Set backtest time
        exec.advance_backtest_time(Ts::from_nanos(1000000));

        // Replay order
        let result = exec.replay_order(2, symbol, side, Qty::new(qty), Some(Px::new(price)));

        assert!(result.is_ok());

        // Check fills
        let fills = exec.get_fills(2);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].timestamp.nanos(), 1000000);
    }
}

mod position_tests {
    use common::{Px, Qty, Side, Symbol, Ts};
    use engine::position::PositionTracker;
    use rstest::rstest;

    #[rstest]
    #[case(1, 100, Side::Bid, 10.0, 100.0, 100000, 10000)] // Buy 10 @ 100
    #[case(2, 200, Side::Ask, 5.0, 200.0, -50000, 20000)] // Sell 5 @ 200
    #[case(3, 300, Side::Bid, 100.0, 50.0, 1000000, 5000)] // Buy 100 @ 50
    fn test_position_tracking(
        #[case] order_id: u64,
        #[case] symbol_id: u32,
        #[case] side: Side,
        #[case] qty: f64,
        #[case] price: f64,
        #[case] expected_qty_raw: i64,
        #[case] expected_price_raw: i64,
    ) {
        let tracker = PositionTracker::new(100);
        let symbol = Symbol(symbol_id);

        // Add pending order
        tracker.add_pending(order_id, symbol, side, Qty::new(qty));

        // Apply fill
        tracker.apply_fill(order_id, Qty::new(qty), Px::new(price), Ts::now());

        // Get position
        let pos = tracker.get_position(symbol);
        assert!(pos.is_some());

        if let Some(p) = pos {
            // Check raw values (qty * 10000 for 4 decimals, price * 100 for 2 decimals)
            let actual_qty = p.quantity.load(std::sync::atomic::Ordering::Acquire);
            let actual_price = p.avg_price.load(std::sync::atomic::Ordering::Acquire);

            // For Ask side, quantity should be negative
            let expected_qty = if side == Side::Ask {
                -expected_qty_raw.abs()
            } else {
                expected_qty_raw.abs()
            };
            assert_eq!(actual_qty, expected_qty);
            assert_eq!(actual_price as i64, expected_price_raw);
        }
    }

    #[rstest]
    #[case(100, Side::Bid, 10.0, 100.0, 105.0)] // Buy, price goes up
    #[case(200, Side::Ask, 5.0, 200.0, 195.0)] // Sell, price goes down (profit)
    #[case(300, Side::Bid, 20.0, 50.0, 45.0)] // Buy, price goes down (loss)
    fn test_position_pnl(
        #[case] symbol_id: u32,
        #[case] side: Side,
        #[case] qty: f64,
        #[case] entry_price: f64,
        #[case] exit_price: f64,
    ) {
        let tracker = PositionTracker::new(100);

        let symbol = Symbol(symbol_id);

        // Open position
        tracker.add_pending(1, symbol, side, Qty::new(qty));
        tracker.apply_fill(1, Qty::new(qty), Px::new(entry_price), Ts::now());

        // Update market price
        tracker.update_market(
            symbol,
            Px::new(exit_price),
            Px::new(exit_price + 1.0),
            Ts::now(),
        );

        // Check unrealized PnL
        let pos = tracker.get_position(symbol);
        assert!(pos.is_some());

        if let Some(p) = pos {
            // Unrealized PnL = (105 - 100) * 10 = 50
            assert!(p.unrealized_pnl.load(std::sync::atomic::Ordering::Acquire) > 0);
        }

        // Close position (opposite side)
        let close_side = if side == Side::Bid {
            Side::Ask
        } else {
            Side::Bid
        };
        tracker.add_pending(2, symbol, close_side, Qty::new(qty));
        tracker.apply_fill(2, Qty::new(qty), Px::new(exit_price), Ts::now());

        // Check realized PnL
        let pos = tracker.get_position(symbol);
        if let Some(p) = pos {
            assert_eq!(p.quantity.load(std::sync::atomic::Ordering::Acquire), 0);
            assert!(p.realized_pnl.load(std::sync::atomic::Ordering::Acquire) > 0);
        }
    }

    #[rstest]
    #[case(100, 10)] // 100 max positions, add 10
    #[case(50, 5)] // 50 max positions, add 5
    #[case(200, 20)] // 200 max positions, add 20
    fn test_all_positions(#[case] max_positions: usize, #[case] positions_to_add: usize) {
        let tracker = PositionTracker::new(max_positions);

        // Add multiple positions
        for i in 0..positions_to_add {
            let symbol = Symbol(100 + i as u32);
            let side = if i % 2 == 0 { Side::Bid } else { Side::Ask };
            let qty = 10.0 + i as f64;
            let price = 100.0 + (i as f64 * 10.0);

            tracker.add_pending(i as u64, symbol, side, Qty::new(qty));
            tracker.apply_fill(i as u64, Qty::new(qty), Px::new(price), Ts::now());
        }

        // Get all positions
        let positions = tracker.get_all_positions();
        assert_eq!(positions.len(), positions_to_add);

        // Check positions exist
        for i in 0..positions_to_add {
            assert!(positions.iter().any(|p| p.0 == Symbol(100 + i as u32)));
        }
    }
}
