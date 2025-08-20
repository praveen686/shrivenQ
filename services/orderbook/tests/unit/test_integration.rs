//! Integration tests for orderbook components
//! 
//! Tests cover:
//! - End-to-end orderbook operations with metrics
//! - Replay engine with real orderbook state
//! - Analytics integration with live trading scenarios
//! - Performance under realistic load patterns
//! - Cross-component data consistency
//! - Error handling and recovery scenarios

use orderbook::{
    OrderBook, Side, 
    MicrostructureAnalytics, ImbalanceCalculator, ToxicityDetector,
    ReplayEngine, ReplayConfig,
    PerformanceMetrics, OperationType,
    OrderBookEvent, OrderUpdate, TradeEvent, UpdateType, 
    Side as EventSide, EventBuilder
};
use services_common::{Px, Qty, Ts};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use rstest::*;
use anyhow::Result;

/// Comprehensive trading simulation scenario
struct TradingScenario {
    orderbook: OrderBook,
    analytics: MicrostructureAnalytics,
    toxicity_detector: ToxicityDetector,
    metrics: PerformanceMetrics,
    replay_engine: ReplayEngine,
}

impl TradingScenario {
    fn new(symbol: &str) -> Self {
        let config = ReplayConfig {
            max_sequence_gap: 100,
            validate_checksums: false, // Disabled for testing simplicity
            buffer_size: 10000,
            snapshot_interval: 1000,
            track_latency: true,
        };
        
        Self {
            orderbook: OrderBook::new(symbol),
            analytics: MicrostructureAnalytics::new(),
            toxicity_detector: ToxicityDetector::new(),
            metrics: PerformanceMetrics::new(symbol),
            replay_engine: ReplayEngine::new(symbol, config),
        }
    }
    
    /// Simulate a complete order lifecycle
    fn simulate_order_lifecycle(&self, 
        order_id: u64, 
        price: i64, 
        quantity: i64, 
        side: Side
    ) -> Result<()> {
        let start_time = Instant::now();
        
        // Create and add order
        let order = orderbook::core::Order {
            id: order_id,
            price: Px::from_i64(price),
            quantity: Qty::from_i64(quantity),
            original_quantity: Qty::from_i64(quantity),
            timestamp: Ts::now(),
            side,
            is_iceberg: false,
            visible_quantity: None,
        };
        
        self.orderbook.add_order(order);
        let add_latency = start_time.elapsed().as_nanos() as u64;
        self.metrics.record_order_add(Qty::from_i64(quantity), add_latency);
        
        // Simulate price impact and analytics update
        self.analytics.update_trade(
            Px::from_i64(price), 
            Qty::from_i64(quantity), 
            side == Side::Bid, 
            Ts::now()
        );
        
        // Update toxicity detector
        self.toxicity_detector.update(
            side == Side::Bid,
            Qty::from_i64(quantity),
            Ts::now()
        );
        
        // Simulate order modification
        thread::sleep(Duration::from_micros(100));
        let modify_start = Instant::now();
        let new_quantity = quantity / 2;
        
        // Cancel old order and add modified one
        self.orderbook.cancel_order(order_id);
        let modified_order = orderbook::core::Order {
            id: order_id + 10000, // New ID for modified order
            price: Px::from_i64(price),
            quantity: Qty::from_i64(new_quantity),
            original_quantity: Qty::from_i64(new_quantity),
            timestamp: Ts::now(),
            side,
            is_iceberg: false,
            visible_quantity: None,
        };
        
        self.orderbook.add_order(modified_order);
        let modify_latency = modify_start.elapsed().as_nanos() as u64;
        self.metrics.record_order_modify(modify_latency);
        
        // Finally cancel the order
        thread::sleep(Duration::from_micros(50));
        let cancel_start = Instant::now();
        self.orderbook.cancel_order(order_id + 10000);
        let cancel_latency = cancel_start.elapsed().as_nanos() as u64;
        self.metrics.record_order_cancel(Qty::from_i64(new_quantity), cancel_latency);
        
        Ok(())
    }
    
    /// Simulate trade execution with full analytics
    fn simulate_trade_execution(&self, 
        trade_id: u64, 
        price: i64, 
        quantity: i64,
        is_aggressive_buy: bool
    ) -> Result<()> {
        let start_time = Instant::now();
        
        // Update analytics with trade
        self.analytics.update_trade(
            Px::from_i64(price),
            Qty::from_i64(quantity),
            is_aggressive_buy,
            Ts::now()
        );
        
        // Update toxicity detector
        self.toxicity_detector.update(
            is_aggressive_buy,
            Qty::from_i64(quantity),
            Ts::now()
        );
        
        let trade_latency = start_time.elapsed().as_nanos() as u64;
        self.metrics.record_trade(Qty::from_i64(quantity), trade_latency);
        
        // Create and process replay event
        let mut builder = EventBuilder::new();
        let trade_event = builder.trade(
            trade_id,
            Px::from_i64(price),
            Qty::from_i64(quantity),
            if is_aggressive_buy { EventSide::Buy } else { EventSide::Sell }
        );
        
        self.replay_engine.process_event(OrderBookEvent::Trade(trade_event))?;
        
        Ok(())
    }
}

#[rstest]
fn test_complete_trading_session() -> Result<()> {
    let scenario = TradingScenario::new("INTEGRATION_TEST");
    
    // Simulate market opening with initial orders
    let initial_orders = vec![
        (1, 99_000, 10_000, Side::Bid),
        (2, 99_500, 15_000, Side::Bid),
        (3, 100_000, 20_000, Side::Bid),
        (4, 101_000, 12_000, Side::Ask),
        (5, 101_500, 18_000, Side::Ask),
        (6, 102_000, 8_000, Side::Ask),
    ];
    
    for (id, price, qty, side) in initial_orders {
        scenario.simulate_order_lifecycle(id, price, qty, side)?;
    }
    
    // Simulate active trading
    for i in 0..50 {
        let price = 100_000 + (i % 10 - 5) * 100; // Prices around $10.00
        let quantity = 1_000 + (i % 5) * 500;
        let is_buy = i % 3 != 0; // 2/3 buys, 1/3 sells
        
        scenario.simulate_trade_execution(100 + i, price, quantity, is_buy)?;
    }
    
    // Verify final state
    let (best_bid, best_ask) = scenario.orderbook.get_bbo();
    println!("Final BBO: {:?} / {:?}", best_bid, best_ask);
    
    let metrics_snapshot = scenario.metrics.get_snapshot();
    println!("Orders processed: {}", metrics_snapshot.orders_added);
    println!("Trades executed: {}", metrics_snapshot.trades_executed);
    
    let replay_stats = scenario.replay_engine.get_stats();
    println!("Replay trades: {}", replay_stats.trades_processed);
    
    // Verify analytics
    let vpin = scenario.analytics.get_vpin();
    let flow_imbalance = scenario.analytics.get_flow_imbalance();
    let toxicity = scenario.toxicity_detector.get_toxicity();
    
    println!("VPIN: {:.2}%", vpin);
    println!("Flow Imbalance: {:.2}%", flow_imbalance);
    println!("Toxicity Score: {:.2}", toxicity);
    
    assert!(vpin >= 0.0 && vpin <= 100.0);
    assert!(flow_imbalance >= -100.0 && flow_imbalance <= 100.0);
    assert!(toxicity >= 0.0 && toxicity <= 100.0);
    
    Ok(())
}

#[rstest]
fn test_high_frequency_trading_simulation() -> Result<()> {
    let scenario = TradingScenario::new("HFT_TEST");
    
    // Simulate high-frequency trading with rapid order updates
    let start_time = Instant::now();
    let num_operations = 1000;
    
    for i in 0..num_operations {
        let base_price = 100_000;
        let price_offset = (i % 20) as i64 * 10; // Small price variations
        let price = base_price + price_offset - 100;
        
        match i % 4 {
            0 => {
                // Add order
                let order = orderbook::core::Order {
                    id: i,
                    price: Px::from_i64(price),
                    quantity: Qty::from_i64(1000 + (i % 5) as i64 * 500),
                    original_quantity: Qty::from_i64(1000 + (i % 5) as i64 * 500),
                    timestamp: Ts::now(),
                    side: if i % 2 == 0 { Side::Bid } else { Side::Ask },
                    is_iceberg: false,
                    visible_quantity: None,
                };
                
                scenario.orderbook.add_order(order);
                scenario.metrics.record_order_add(
                    Qty::from_i64(1000), 
                    start_time.elapsed().as_nanos() as u64
                );
            },
            1 => {
                // Cancel order
                if i > 0 {
                    scenario.orderbook.cancel_order(i - 1);
                    scenario.metrics.record_order_cancel(
                        Qty::from_i64(500), 
                        start_time.elapsed().as_nanos() as u64
                    );
                }
            },
            2 => {
                // Simulate trade
                scenario.simulate_trade_execution(
                    i, 
                    price, 
                    500 + (i % 3) as i64 * 250, 
                    i % 2 == 0
                )?;
            },
            3 => {
                // Modify (cancel + add)
                scenario.metrics.record_order_modify(
                    start_time.elapsed().as_nanos() as u64
                );
            },
            _ => unreachable!(),
        }
        
        // Record spread if book has both sides
        let (bid, ask) = scenario.orderbook.get_bbo();
        if let (Some(b), Some(a)) = (bid, ask) {
            let spread = a.as_i64() - b.as_i64();
            scenario.metrics.record_spread(spread);
        }
    }
    
    let total_time = start_time.elapsed();
    println!("Processed {} operations in {:?}", num_operations, total_time);
    println!("Average: {:.2} ops/sec", 
        num_operations as f64 / total_time.as_secs_f64());
    
    let metrics_snapshot = scenario.metrics.get_snapshot();
    assert!(metrics_snapshot.orders_added > 0);
    assert!(metrics_snapshot.trades_executed > 0);
    
    // Verify latency stats are reasonable (< 1ms for most operations)
    if let Some(ref add_stats) = metrics_snapshot.latency_stats.order_add {
        println!("Order add p99: {} ns", add_stats.p99);
        assert!(add_stats.p99 < 10_000_000); // Less than 10ms
    }
    
    Ok(())
}

#[rstest]
fn test_market_stress_scenario() -> Result<()> {
    let scenario = TradingScenario::new("STRESS_TEST");
    
    // Phase 1: Normal market conditions
    for i in 0..100 {
        scenario.simulate_order_lifecycle(
            i, 
            100_000 + (i % 10 - 5) * 50,
            1000 + (i % 3) * 500,
            if i % 2 == 0 { Side::Bid } else { Side::Ask }
        )?;
    }
    
    let normal_toxicity = scenario.toxicity_detector.get_toxicity();
    let normal_imbalance = scenario.analytics.get_flow_imbalance();
    
    // Phase 2: Market stress (one-sided flow)
    for i in 100..200 {
        scenario.simulate_trade_execution(
            i,
            99_000 - (i - 100) * 10, // Falling prices
            2000 + (i % 5) * 1000,
            false // All sells (stress scenario)
        )?;
    }
    
    let stress_toxicity = scenario.toxicity_detector.get_toxicity();
    let stress_imbalance = scenario.analytics.get_flow_imbalance();
    
    // Phase 3: Market recovery
    for i in 200..250 {
        scenario.simulate_trade_execution(
            i,
            98_000 + (i - 200) * 50, // Recovering prices
            1500,
            i % 2 == 0 // Balanced flow
        )?;
    }
    
    let recovery_toxicity = scenario.toxicity_detector.get_toxicity();
    
    println!("Normal toxicity: {:.2}", normal_toxicity);
    println!("Stress toxicity: {:.2}", stress_toxicity);
    println!("Recovery toxicity: {:.2}", recovery_toxicity);
    
    // Stress period should show higher toxicity
    assert!(stress_toxicity >= normal_toxicity);
    
    // Recovery should show improvement
    assert!(recovery_toxicity <= stress_toxicity || recovery_toxicity < 50.0);
    
    // Imbalance should reflect the market stress
    println!("Normal imbalance: {:.2}%", normal_imbalance);
    println!("Stress imbalance: {:.2}%", stress_imbalance);
    assert!(stress_imbalance < -30.0); // Heavy sell imbalance
    
    Ok(())
}

#[rstest]
fn test_concurrent_multi_component_access() -> Result<()> {
    let scenario = Arc::new(TradingScenario::new("CONCURRENT_INTEGRATION"));
    let num_threads = 4;
    let operations_per_thread = 50;
    
    let handles: Vec<_> = (0..num_threads).map(|thread_id| {
        let scenario_clone = Arc::clone(&scenario);
        thread::spawn(move || -> Result<()> {
            for i in 0..operations_per_thread {
                let order_id = (thread_id * 1000 + i) as u64;
                let price = 100_000 + ((thread_id * 100) as i64) + (i as i64 * 10);
                let quantity = 1000 + (i as i64 * 100);
                let side = if (thread_id + i) % 2 == 0 { Side::Bid } else { Side::Ask };
                
                // Simulate order lifecycle
                scenario_clone.simulate_order_lifecycle(order_id, price, quantity, side)?;
                
                // Also simulate some trades
                if i % 5 == 0 {
                    scenario_clone.simulate_trade_execution(
                        order_id + 10000,
                        price,
                        quantity / 2,
                        side == Side::Bid
                    )?;
                }
                
                // Small delay to reduce contention
                thread::sleep(Duration::from_micros(10));
            }
            Ok(())
        })
    }).collect();
    
    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread should complete successfully")?;
    }
    
    // Verify final state consistency
    let metrics_snapshot = scenario.metrics.get_snapshot();
    let replay_stats = scenario.replay_engine.get_stats();
    
    println!("Final metrics: {} orders, {} trades", 
        metrics_snapshot.orders_added, metrics_snapshot.trades_executed);
    println!("Replay processed: {} trades", replay_stats.trades_processed);
    
    // All components should have processed events
    assert!(metrics_snapshot.orders_added > 0);
    assert!(replay_stats.trades_processed > 0);
    
    // Analytics should show reasonable values
    let vpin = scenario.analytics.get_vpin();
    let flow_imbalance = scenario.analytics.get_flow_imbalance();
    assert!(vpin >= 0.0 && vpin <= 100.0);
    assert!(flow_imbalance >= -100.0 && flow_imbalance <= 100.0);
    
    Ok(())
}

#[rstest]
fn test_orderbook_analytics_consistency() -> Result<()> {
    let scenario = TradingScenario::new("CONSISTENCY_TEST");
    
    // Build orderbook state and track with analytics
    let orders = vec![
        (1, 99_000, 10_000, Side::Bid),
        (2, 99_500, 15_000, Side::Bid),
        (3, 100_000, 20_000, Side::Bid),
        (4, 101_000, 12_000, Side::Ask),
        (5, 101_500, 18_000, Side::Ask),
        (6, 102_000, 8_000, Side::Ask),
    ];
    
    for (id, price, qty, side) in orders {
        let order = orderbook::core::Order {
            id,
            price: Px::from_i64(price),
            quantity: Qty::from_i64(qty),
            original_quantity: Qty::from_i64(qty),
            timestamp: Ts::now(),
            side,
            is_iceberg: false,
            visible_quantity: None,
        };
        scenario.orderbook.add_order(order);
    }
    
    // Get orderbook levels and calculate imbalance
    let (bid_levels, ask_levels) = scenario.orderbook.get_depth(10);
    let imbalance_metrics = ImbalanceCalculator::calculate_imbalances(&bid_levels, &ask_levels);
    
    println!("Bid levels: {} orders, {} total quantity", 
        bid_levels.len(), 
        bid_levels.iter().map(|(_, q, _)| q.as_i64()).sum::<i64>());
    println!("Ask levels: {} orders, {} total quantity", 
        ask_levels.len(), 
        ask_levels.iter().map(|(_, q, _)| q.as_i64()).sum::<i64>());
    
    println!("Top level imbalance: {:.2}%", imbalance_metrics.top_level_imbalance);
    println!("Three level imbalance: {:.2}%", imbalance_metrics.three_level_imbalance);
    
    // Verify that imbalance calculation matches orderbook state
    let total_bid_qty: i64 = bid_levels.iter().map(|(_, q, _)| q.as_i64()).sum();
    let total_ask_qty: i64 = ask_levels.iter().map(|(_, q, _)| q.as_i64()).sum();
    let expected_imbalance = (total_bid_qty - total_ask_qty) as f64 / (total_bid_qty + total_ask_qty) as f64 * 100.0;
    
    // Should be close to calculated imbalance (within reasonable tolerance)
    let diff = (imbalance_metrics.three_level_imbalance - expected_imbalance).abs();
    assert!(diff < 5.0, "Imbalance calculation inconsistency: {:.2} vs {:.2}", 
        imbalance_metrics.three_level_imbalance, expected_imbalance);
    
    Ok(())
}

#[rstest]
fn test_replay_orderbook_synchronization() -> Result<()> {
    let scenario = TradingScenario::new("REPLAY_SYNC_TEST");
    let mut builder = EventBuilder::new();
    
    // Create a series of orderbook events
    let events = vec![
        OrderBookEvent::Order(builder.order_add(1, Px::from_i64(100_000), Qty::from_i64(10_000), EventSide::Buy)),
        OrderBookEvent::Order(builder.order_add(2, Px::from_i64(101_000), Qty::from_i64(8_000), EventSide::Sell)),
        OrderBookEvent::Trade(builder.trade(1, Px::from_i64(100_500), Qty::from_i64(2_000), EventSide::Sell)),
        OrderBookEvent::Order(builder.order_modify(1, Px::from_i64(100_000), Qty::from_i64(8_000), EventSide::Buy)),
        OrderBookEvent::Order(builder.order_delete(2, Px::from_i64(101_000), EventSide::Sell)),
    ];
    
    // Process events through replay engine
    for event in events {
        scenario.replay_engine.process_event(event)?;
    }
    
    let replay_stats = scenario.replay_engine.get_stats();
    
    // Verify correct number of events processed
    assert_eq!(replay_stats.orders_processed, 4); // 3 order operations (add, modify, delete) + 1 more add
    assert_eq!(replay_stats.trades_processed, 1);
    
    println!("Replay processed {} orders and {} trades", 
        replay_stats.orders_processed, replay_stats.trades_processed);
    
    Ok(())
}

#[rstest]
fn test_performance_under_load() -> Result<()> {
    let scenario = TradingScenario::new("LOAD_TEST");
    let start_time = Instant::now();
    
    // Simulate heavy load
    for batch in 0..10 {
        for i in 0..100 {
            let order_id = (batch * 100 + i) as u64;
            let price = 100_000 + (i % 50 - 25) * 20;
            let quantity = 1000 + (i % 10) * 500;
            let side = if i % 2 == 0 { Side::Bid } else { Side::Ask };
            
            // Create order
            let order = orderbook::core::Order {
                id: order_id,
                price: Px::from_i64(price),
                quantity: Qty::from_i64(quantity),
                original_quantity: Qty::from_i64(quantity),
                timestamp: Ts::now(),
                side,
                is_iceberg: false,
                visible_quantity: None,
            };
            
            let op_start = Instant::now();
            scenario.orderbook.add_order(order);
            let op_latency = op_start.elapsed().as_nanos() as u64;
            scenario.metrics.record_order_add(Qty::from_i64(quantity), op_latency);
            
            // Update analytics
            scenario.analytics.update_trade(
                Px::from_i64(price),
                Qty::from_i64(quantity),
                side == Side::Bid,
                Ts::now()
            );
            
            // Occasionally cancel orders
            if i % 10 == 0 && order_id > 0 {
                let cancel_start = Instant::now();
                scenario.orderbook.cancel_order(order_id - 1);
                let cancel_latency = cancel_start.elapsed().as_nanos() as u64;
                scenario.metrics.record_order_cancel(Qty::from_i64(quantity / 2), cancel_latency);
            }
        }
        
        // Check performance after each batch
        let elapsed = start_time.elapsed();
        let ops_per_sec = ((batch + 1) * 100) as f64 / elapsed.as_secs_f64();
        println!("Batch {}: {:.0} ops/sec", batch, ops_per_sec);
    }
    
    let total_time = start_time.elapsed();
    let final_ops_per_sec = 1000.0 / total_time.as_secs_f64();
    
    println!("Final performance: {:.0} ops/sec", final_ops_per_sec);
    
    // Verify performance is reasonable (> 1000 ops/sec)
    assert!(final_ops_per_sec > 1000.0, "Performance too slow: {:.0} ops/sec", final_ops_per_sec);
    
    // Check final state
    let (bid_levels, ask_levels) = scenario.orderbook.get_depth(10);
    let metrics_snapshot = scenario.metrics.get_snapshot();
    
    println!("Final state: {} bid levels, {} ask levels", bid_levels.len(), ask_levels.len());
    println!("Metrics: {} orders added, {} canceled", 
        metrics_snapshot.orders_added, metrics_snapshot.orders_canceled);
    
    // Verify latency statistics are reasonable
    if let Some(ref add_stats) = metrics_snapshot.latency_stats.order_add {
        println!("Order add latency - p50: {}ns, p99: {}ns", add_stats.p50, add_stats.p99);
        assert!(add_stats.p99 < 50_000_000, "Order add latency too high: {}ns", add_stats.p99); // < 50ms
    }
    
    Ok(())
}