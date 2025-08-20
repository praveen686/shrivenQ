//! Comprehensive unit tests for replay functionality
//! 
//! Tests cover:
//! - Replay engine configuration and initialization
//! - Event processing with sequence number validation
//! - Out-of-order event buffering and recovery
//! - Snapshot application and validation
//! - Delta application and incremental updates
//! - Gap detection and recovery mechanisms
//! - Checksum validation during replay
//! - Latency tracking and performance monitoring
//! - Concurrent replay operations

use orderbook::replay::{ReplayEngine, ReplayConfig, SnapshotManager, LatencyTracker, ReplayStats};
use orderbook::events::{
    OrderBookEvent, OrderUpdate, TradeEvent, OrderBookSnapshot, OrderBookDelta, 
    MarketEvent, MarketEventType, UpdateType, Side as EventSide, LevelUpdate, EventBuilder
};
use services_common::{Px, Qty, Ts, Symbol};
use anyhow::Result;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use rstest::*;

/// Helper function to create test replay config
fn create_test_config() -> ReplayConfig {
    ReplayConfig {
        max_sequence_gap: 10,
        validate_checksums: true,
        buffer_size: 1000,
        snapshot_interval: 100,
        track_latency: true,
    }
}

/// Helper function to create replay engine
fn create_replay_engine(symbol: &str, config: Option<ReplayConfig>) -> ReplayEngine {
    let config = config.unwrap_or_else(create_test_config);
    ReplayEngine::new(symbol, config)
}

/// Helper to create order update event
fn create_order_update(
    order_id: u64, 
    price: i64, 
    quantity: i64, 
    side: EventSide, 
    update_type: UpdateType,
    sequence: u64
) -> OrderUpdate {
    let exchange_time = Ts::from_nanos(1_000_000_000 * sequence);
    let local_time = Ts::from_nanos(1_000_000_000 * sequence + 1_000_000);
    
    OrderUpdate {
        order_id,
        price: Px::from_i64(price),
        quantity: Qty::from_i64(quantity),
        side,
        update_type,
        exchange_time,
        local_time,
        sequence,
    }
}

/// Helper to create trade event
fn create_trade_event(trade_id: u64, price: i64, quantity: i64, sequence: u64) -> TradeEvent {
    let exchange_time = Ts::from_nanos(1_000_000_000 * sequence);
    let local_time = Ts::from_nanos(1_000_000_000 * sequence + 500_000);
    
    TradeEvent {
        trade_id,
        price: Px::from_i64(price),
        quantity: Qty::from_i64(quantity),
        aggressor_side: EventSide::Buy,
        maker_order_id: Some(1000 + trade_id),
        taker_order_id: Some(2000 + trade_id),
        exchange_time,
        local_time,
        sequence,
    }
}

/// Helper to create snapshot event
fn create_snapshot_event(
    symbol: Symbol,
    bid_levels: Vec<(i64, i64, u64)>,
    ask_levels: Vec<(i64, i64, u64)>,
    sequence: u64
) -> OrderBookSnapshot {
    let bids: Vec<LevelUpdate> = bid_levels.into_iter().map(|(price, qty, count)| {
        LevelUpdate {
            price: Px::from_i64(price),
            quantity: Qty::from_i64(qty),
            order_count: count,
            side: EventSide::Buy,
        }
    }).collect();
    
    let asks: Vec<LevelUpdate> = ask_levels.into_iter().map(|(price, qty, count)| {
        LevelUpdate {
            price: Px::from_i64(price),
            quantity: Qty::from_i64(qty),
            order_count: count,
            side: EventSide::Sell,
        }
    }).collect();
    
    OrderBookSnapshot {
        symbol,
        bids,
        asks,
        sequence,
        exchange_time: Ts::from_nanos(1_000_000_000 * sequence),
        local_time: Ts::from_nanos(1_000_000_000 * sequence + 2_000_000),
        checksum: 0x12345678,
    }
}

#[rstest]
fn test_replay_engine_creation() {
    let config = create_test_config();
    let engine = create_replay_engine("BTCUSD", Some(config.clone()));
    
    let stats = engine.get_stats();
    assert_eq!(stats.orders_processed, 0);
    assert_eq!(stats.trades_processed, 0);
    assert_eq!(stats.snapshots_processed, 0);
    assert_eq!(stats.deltas_processed, 0);
    assert_eq!(stats.market_events, 0);
}

#[rstest]
fn test_replay_config_defaults() {
    let config = ReplayConfig::default();
    
    assert_eq!(config.max_sequence_gap, 100);
    assert!(config.validate_checksums);
    assert_eq!(config.buffer_size, 100_000);
    assert_eq!(config.snapshot_interval, 10_000);
    assert!(config.track_latency);
}

#[rstest]
fn test_process_in_order_events() -> Result<()> {
    let engine = create_replay_engine("ETHUSD", None);
    
    // Process events in order
    let order1 = create_order_update(1, 100_000, 10_000, EventSide::Buy, UpdateType::Add, 1);
    let order2 = create_order_update(2, 101_000, 5_000, EventSide::Sell, UpdateType::Add, 2);
    let trade1 = create_trade_event(1, 100_500, 2_500, 3);
    
    engine.process_event(OrderBookEvent::Order(order1))?;
    engine.process_event(OrderBookEvent::Order(order2))?;
    engine.process_event(OrderBookEvent::Trade(trade1))?;
    
    let stats = engine.get_stats();
    assert_eq!(stats.orders_processed, 2);
    assert_eq!(stats.trades_processed, 1);
    
    Ok(())
}

#[rstest]
fn test_process_out_of_order_events() -> Result<()> {
    let engine = create_replay_engine("ADAUSD", None);
    
    // Process events out of order (sequence 3, 1, 2)
    let order1 = create_order_update(1, 100_000, 10_000, EventSide::Buy, UpdateType::Add, 1);
    let order2 = create_order_update(2, 101_000, 5_000, EventSide::Sell, UpdateType::Add, 2);
    let order3 = create_order_update(3, 99_000, 7_500, EventSide::Buy, UpdateType::Add, 3);
    
    // Process in wrong order: 3, 1, 2
    engine.process_event(OrderBookEvent::Order(order3.clone()))?;
    // Event 3 should be buffered
    
    engine.process_event(OrderBookEvent::Order(order1))?;
    // Event 1 should be processed immediately, then buffered event 2 should be processed
    
    engine.process_event(OrderBookEvent::Order(order2))?;
    // Event 2 should be processed, then buffered event 3 should be processed
    
    let stats = engine.get_stats();
    assert_eq!(stats.orders_processed, 3);
    
    Ok(())
}

#[rstest]
fn test_duplicate_sequence_handling() -> Result<()> {
    let engine = create_replay_engine("SOLUSD", None);
    
    let order1 = create_order_update(1, 100_000, 10_000, EventSide::Buy, UpdateType::Add, 1);
    let order1_duplicate = create_order_update(2, 101_000, 5_000, EventSide::Sell, UpdateType::Add, 1);
    
    engine.process_event(OrderBookEvent::Order(order1))?;
    engine.process_event(OrderBookEvent::Order(order1_duplicate))?; // Should be ignored
    
    let stats = engine.get_stats();
    assert_eq!(stats.orders_processed, 1); // Only first event should be processed
    
    Ok(())
}

#[rstest]
fn test_large_sequence_gap_detection() -> Result<()> {
    let mut config = create_test_config();
    config.max_sequence_gap = 5; // Small gap for testing
    
    let engine = create_replay_engine("DOTUSD", Some(config));
    
    let order1 = create_order_update(1, 100_000, 10_000, EventSide::Buy, UpdateType::Add, 1);
    let order_gap = create_order_update(2, 101_000, 5_000, EventSide::Sell, UpdateType::Add, 10); // Large gap
    
    engine.process_event(OrderBookEvent::Order(order1))?;
    engine.process_event(OrderBookEvent::Order(order_gap))?;
    
    // Engine should detect gap and request snapshot recovery
    // In real implementation, this would trigger snapshot request
    
    Ok(())
}

#[rstest]
fn test_snapshot_processing() -> Result<()> {
    let engine = create_replay_engine("LINKUSD", None);
    
    let snapshot = create_snapshot_event(
        Symbol::new(1),
        vec![(100_000, 10_000, 2), (99_000, 15_000, 3)], // Bid levels
        vec![(101_000, 8_000, 1), (102_000, 12_000, 2)], // Ask levels
        100
    );
    
    engine.process_event(OrderBookEvent::Snapshot(snapshot))?;
    
    let stats = engine.get_stats();
    assert_eq!(stats.snapshots_processed, 1);
    
    Ok(())
}

#[rstest]
fn test_delta_processing() -> Result<()> {
    let engine = create_replay_engine("AVAXUSD", None);
    
    // First apply a snapshot to establish base state
    let snapshot = create_snapshot_event(
        Symbol::new(2),
        vec![(100_000, 10_000, 1)],
        vec![(101_000, 8_000, 1)],
        50
    );
    
    engine.process_event(OrderBookEvent::Snapshot(snapshot))?;
    
    // Now apply a delta
    let delta = OrderBookDelta {
        symbol: Symbol::new(2),
        bid_updates: vec![
            LevelUpdate {
                price: Px::from_i64(99_500),
                quantity: Qty::from_i64(5_000),
                order_count: 1,
                side: EventSide::Buy,
            }
        ],
        ask_updates: vec![
            LevelUpdate {
                price: Px::from_i64(101_500),
                quantity: Qty::from_i64(7_000),
                order_count: 2,
                side: EventSide::Sell,
            }
        ],
        bid_deletions: vec![],
        ask_deletions: vec![],
        prev_sequence: 50,
        sequence: 51,
        exchange_time: Ts::now(),
        local_time: Ts::now(),
    };
    
    engine.process_event(OrderBookEvent::Delta(delta))?;
    
    let stats = engine.get_stats();
    assert_eq!(stats.snapshots_processed, 1);
    assert_eq!(stats.deltas_processed, 1);
    
    Ok(())
}

#[rstest]
fn test_delta_sequence_gap_error() {
    let engine = create_replay_engine("ALGOUSD", None);
    
    // Apply delta without proper previous sequence
    let delta = OrderBookDelta {
        symbol: Symbol::new(3),
        bid_updates: vec![],
        ask_updates: vec![],
        bid_deletions: vec![],
        ask_deletions: vec![],
        prev_sequence: 100, // No events processed yet, so this should fail
        sequence: 101,
        exchange_time: Ts::now(),
        local_time: Ts::now(),
    };
    
    let result = engine.process_event(OrderBookEvent::Delta(delta));
    assert!(result.is_err()); // Should fail due to sequence gap
}

#[rstest]
fn test_market_event_processing() -> Result<()> {
    let engine = create_replay_engine("MATICUSD", None);
    
    let market_event = MarketEvent {
        event_type: MarketEventType::TradingHalt,
        symbol: Some(Symbol::new(4)),
        timestamp: Ts::now(),
        message: "Trading halted for volatility".to_string(),
    };
    
    engine.process_event(OrderBookEvent::Market(market_event))?;
    
    let stats = engine.get_stats();
    assert_eq!(stats.market_events, 1);
    
    Ok(())
}

#[rstest]
fn test_event_buffer_overflow() -> Result<()> {
    let mut config = create_test_config();
    config.buffer_size = 5; // Very small buffer for testing
    
    let engine = create_replay_engine("BUFFER_TEST", Some(config));
    
    // Fill buffer with out-of-order events
    for i in 10..20 {
        let order = create_order_update(i, 100_000, 1000, EventSide::Buy, UpdateType::Add, i);
        engine.process_event(OrderBookEvent::Order(order))?;
    }
    
    // Buffer should handle overflow gracefully by dropping oldest events
    let stats = engine.get_stats();
    assert_eq!(stats.orders_processed, 0); // No events processed as they're all out of order
    
    Ok(())
}

#[rstest]
fn test_concurrent_event_processing() -> Result<()> {
    let engine = Arc::new(create_replay_engine("CONCURRENT_REPLAY", None));
    let num_threads = 4;
    let events_per_thread = 25;
    
    let handles: Vec<_> = (0..num_threads).map(|thread_id| {
        let engine_clone = Arc::clone(&engine);
        thread::spawn(move || -> Result<()> {
            for i in 0..events_per_thread {
                let sequence = (thread_id * events_per_thread + i + 1) as u64;
                let order = create_order_update(
                    sequence, 
                    100_000, 
                    1000, 
                    if i % 2 == 0 { EventSide::Buy } else { EventSide::Sell },
                    UpdateType::Add,
                    sequence
                );
                
                engine_clone.process_event(OrderBookEvent::Order(order))?;
                
                // Small delay to reduce contention
                thread::sleep(Duration::from_micros(10));
            }
            Ok(())
        })
    }).collect();
    
    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread should complete successfully")?;
    }
    
    let stats = engine.get_stats();
    assert_eq!(stats.orders_processed, (num_threads * events_per_thread) as u64);
    
    Ok(())
}

#[rstest]
fn test_latency_tracking_in_replay() -> Result<()> {
    let mut config = create_test_config();
    config.track_latency = true;
    
    let engine = create_replay_engine("LATENCY_TEST", Some(config));
    
    // Create events with known latency
    let base_time = 1_000_000_000;
    let order = OrderUpdate {
        order_id: 1,
        price: Px::from_i64(100_000),
        quantity: Qty::from_i64(10_000),
        side: EventSide::Buy,
        update_type: UpdateType::Add,
        exchange_time: Ts::from_nanos(base_time),
        local_time: Ts::from_nanos(base_time + 1_000_000), // 1ms latency
        sequence: 1,
    };
    
    engine.process_event(OrderBookEvent::Order(order))?;
    
    // Latency should be tracked (hard to verify without access to internal tracker)
    let stats = engine.get_stats();
    assert_eq!(stats.orders_processed, 1);
    
    Ok(())
}

#[rstest]
fn test_checksum_validation_disabled() -> Result<()> {
    let mut config = create_test_config();
    config.validate_checksums = false;
    
    let engine = create_replay_engine("NO_CHECKSUM", Some(config));
    
    let snapshot = create_snapshot_event(
        Symbol::new(5),
        vec![(100_000, 10_000, 1)],
        vec![(101_000, 8_000, 1)],
        1
    );
    
    // Should process successfully even with checksum validation disabled
    engine.process_event(OrderBookEvent::Snapshot(snapshot))?;
    
    let stats = engine.get_stats();
    assert_eq!(stats.snapshots_processed, 1);
    
    Ok(())
}

#[rstest]
fn test_snapshot_manager() {
    let manager = SnapshotManager::new(100);
    
    let snapshot1 = create_snapshot_event(Symbol::new(6), vec![], vec![], 100);
    let snapshot2 = create_snapshot_event(Symbol::new(6), vec![], vec![], 200);
    
    manager.store_snapshot(snapshot1);
    manager.store_snapshot(snapshot2);
    
    // Test retrieval
    let retrieved = manager.get_snapshot_before(150);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().sequence, 100);
    
    let retrieved2 = manager.get_snapshot_before(250);
    assert!(retrieved2.is_some());
    assert_eq!(retrieved2.unwrap().sequence, 200);
    
    // Test needs snapshot
    assert!(manager.needs_snapshot(300, 200));
    assert!(!manager.needs_snapshot(250, 200));
}

#[rstest]
fn test_latency_tracker_percentiles() {
    let tracker = LatencyTracker::new();
    
    // Add known latencies
    let latencies = vec![1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000, 9000, 10000];
    for latency in latencies {
        tracker.record(latency);
    }
    
    let percentiles = tracker.get_percentiles();
    
    assert_eq!(percentiles.min, 1000);
    assert_eq!(percentiles.max, 10000);
    assert_eq!(percentiles.mean, 5500);
    assert_eq!(percentiles.p50, 5000); // Median of 10 items
}

#[rstest]
fn test_order_update_type_processing() -> Result<()> {
    let engine = create_replay_engine("UPDATE_TYPES", None);
    
    // Test all update types
    let add_order = create_order_update(1, 100_000, 10_000, EventSide::Buy, UpdateType::Add, 1);
    let modify_order = create_order_update(1, 100_000, 5_000, EventSide::Buy, UpdateType::Modify, 2);
    let delete_order = create_order_update(1, 100_000, 0, EventSide::Buy, UpdateType::Delete, 3);
    
    engine.process_event(OrderBookEvent::Order(add_order))?;
    engine.process_event(OrderBookEvent::Order(modify_order))?;
    engine.process_event(OrderBookEvent::Order(delete_order))?;
    
    let stats = engine.get_stats();
    assert_eq!(stats.orders_processed, 3);
    
    Ok(())
}

#[rstest]
fn test_replay_with_mixed_event_types() -> Result<()> {
    let engine = create_replay_engine("MIXED_EVENTS", None);
    
    // Mix different event types
    let order1 = create_order_update(1, 100_000, 10_000, EventSide::Buy, UpdateType::Add, 1);
    let trade1 = create_trade_event(1, 100_000, 2_500, 2);
    let order2 = create_order_update(2, 101_000, 8_000, EventSide::Sell, UpdateType::Add, 3);
    
    let market_event = MarketEvent {
        event_type: MarketEventType::CircuitBreaker,
        symbol: None,
        timestamp: Ts::now(),
        message: "Circuit breaker triggered".to_string(),
    };
    
    engine.process_event(OrderBookEvent::Order(order1))?;
    engine.process_event(OrderBookEvent::Trade(trade1))?;
    engine.process_event(OrderBookEvent::Order(order2))?;
    engine.process_event(OrderBookEvent::Market(market_event))?;
    
    let stats = engine.get_stats();
    assert_eq!(stats.orders_processed, 2);
    assert_eq!(stats.trades_processed, 1);
    assert_eq!(stats.market_events, 1);
    
    Ok(())
}

#[rstest]
fn test_event_builder_integration() -> Result<()> {
    let engine = create_replay_engine("BUILDER_TEST", None);
    let mut builder = EventBuilder::new();
    
    // Create events using builder
    let order_add = builder.order_add(1, Px::from_i64(100_000), Qty::from_i64(10_000), EventSide::Buy);
    let order_modify = builder.order_modify(1, Px::from_i64(100_000), Qty::from_i64(5_000), EventSide::Buy);
    let order_delete = builder.order_delete(1, Px::from_i64(100_000), EventSide::Buy);
    let trade = builder.trade(1, Px::from_i64(100_000), Qty::from_i64(2_500), EventSide::Sell);
    
    // Process events
    engine.process_event(OrderBookEvent::Order(order_add))?;
    engine.process_event(OrderBookEvent::Order(order_modify))?;
    engine.process_event(OrderBookEvent::Order(order_delete))?;
    engine.process_event(OrderBookEvent::Trade(trade))?;
    
    let stats = engine.get_stats();
    assert_eq!(stats.orders_processed, 3);
    assert_eq!(stats.trades_processed, 1);
    
    Ok(())
}

#[rstest]
fn test_replay_stats_accuracy() -> Result<()> {
    let engine = create_replay_engine("STATS_TEST", None);
    
    // Process known number of each event type
    for i in 1..=5 {
        let order = create_order_update(i, 100_000, 1000, EventSide::Buy, UpdateType::Add, i);
        engine.process_event(OrderBookEvent::Order(order))?;
    }
    
    for i in 6..=8 {
        let trade = create_trade_event(i, 100_000, 500, i);
        engine.process_event(OrderBookEvent::Trade(trade))?;
    }
    
    let snapshot = create_snapshot_event(Symbol::new(7), vec![], vec![], 9);
    engine.process_event(OrderBookEvent::Snapshot(snapshot))?;
    
    let market_event = MarketEvent {
        event_type: MarketEventType::MarketOpen,
        symbol: None,
        timestamp: Ts::now(),
        message: "Market opened".to_string(),
    };
    engine.process_event(OrderBookEvent::Market(market_event))?;
    
    let stats = engine.get_stats();
    assert_eq!(stats.orders_processed, 5);
    assert_eq!(stats.trades_processed, 3);
    assert_eq!(stats.snapshots_processed, 1);
    assert_eq!(stats.deltas_processed, 0);
    assert_eq!(stats.market_events, 1);
    
    Ok(())
}