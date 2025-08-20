//! Comprehensive unit tests for performance metrics
//! 
//! Tests cover:
//! - Metrics collection and atomic operations
//! - HDR histogram functionality and percentile calculations
//! - Latency tracking for different operation types
//! - Concurrent metrics updates and thread safety
//! - Spread and depth metrics calculations
//! - Checksum validation tracking
//! - Metrics snapshot generation and reporting

use orderbook::metrics::{
    PerformanceMetrics, LatencyTracker, OperationType, OperationLatency, 
    LatencyStats, MetricsSnapshot
};
use services_common::{Qty, Ts};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use rstest::*;

/// Helper function to create performance metrics
fn create_test_metrics(symbol: &str) -> PerformanceMetrics {
    PerformanceMetrics::new(symbol)
}

/// Helper to simulate latency values in nanoseconds
fn simulate_latencies(operation_type: OperationType, latencies: &[u64]) -> LatencyTracker {
    let tracker = LatencyTracker::new();
    for &latency in latencies {
        tracker.record_operation(operation_type, latency);
    }
    tracker
}

#[rstest]
fn test_performance_metrics_creation() {
    let metrics = create_test_metrics("BTCUSD");
    let snapshot = metrics.get_snapshot();
    
    assert_eq!(snapshot.symbol, "BTCUSD");
    assert_eq!(snapshot.orders_added, 0);
    assert_eq!(snapshot.orders_modified, 0);
    assert_eq!(snapshot.orders_canceled, 0);
    assert_eq!(snapshot.trades_executed, 0);
    assert_eq!(snapshot.total_volume_added, 0);
    assert_eq!(snapshot.total_volume_canceled, 0);
    assert_eq!(snapshot.total_volume_traded, 0);
}

#[rstest]
fn test_order_add_metrics() {
    let metrics = create_test_metrics("ETHUSD");
    
    // Record order adds
    metrics.record_order_add(Qty::from_i64(10_000), 1_500_000); // 1.5ms latency
    metrics.record_order_add(Qty::from_i64(20_000), 2_000_000); // 2.0ms latency
    metrics.record_order_add(Qty::from_i64(15_000), 1_200_000); // 1.2ms latency
    
    let snapshot = metrics.get_snapshot();
    assert_eq!(snapshot.orders_added, 3);
    assert_eq!(snapshot.total_volume_added, 45_000);
    
    // Check latency stats are available
    assert!(snapshot.latency_stats.order_add.is_some());
    let add_stats = snapshot.latency_stats.order_add.unwrap();
    assert_eq!(add_stats.count, 3);
    assert!(add_stats.min > 0);
    assert!(add_stats.max > 0);
    assert!(add_stats.mean > 0);
}

#[rstest]
fn test_order_modify_metrics() {
    let metrics = create_test_metrics("ADAUSD");
    
    // Record order modifications
    for i in 0..5 {
        metrics.record_order_modify(1_000_000 + i * 100_000); // Varying latencies
    }
    
    let snapshot = metrics.get_snapshot();
    assert_eq!(snapshot.orders_modified, 5);
    
    let modify_stats = snapshot.latency_stats.order_modify.unwrap();
    assert_eq!(modify_stats.count, 5);
    assert!(modify_stats.p50 > 0);
    assert!(modify_stats.p90 > 0);
    assert!(modify_stats.p99 > 0);
}

#[rstest]
fn test_order_cancel_metrics() {
    let metrics = create_test_metrics("SOLUSD");
    
    // Record order cancellations
    metrics.record_order_cancel(Qty::from_i64(5_000), 800_000);
    metrics.record_order_cancel(Qty::from_i64(10_000), 900_000);
    metrics.record_order_cancel(Qty::from_i64(7_500), 850_000);
    
    let snapshot = metrics.get_snapshot();
    assert_eq!(snapshot.orders_canceled, 3);
    assert_eq!(snapshot.total_volume_canceled, 22_500);
    
    let cancel_stats = snapshot.latency_stats.order_cancel.unwrap();
    assert_eq!(cancel_stats.count, 3);
    assert!(cancel_stats.mean > 800_000);
    assert!(cancel_stats.mean < 900_000);
}

#[rstest]
fn test_trade_execution_metrics() {
    let metrics = create_test_metrics("LINKUSD");
    
    // Record trade executions
    metrics.record_trade(Qty::from_i64(2_500), 500_000);
    metrics.record_trade(Qty::from_i64(3_500), 600_000);
    metrics.record_trade(Qty::from_i64(1_500), 450_000);
    
    let snapshot = metrics.get_snapshot();
    assert_eq!(snapshot.trades_executed, 3);
    assert_eq!(snapshot.total_volume_traded, 7_500);
    
    let trade_stats = snapshot.latency_stats.trade.unwrap();
    assert_eq!(trade_stats.count, 3);
    assert_eq!(trade_stats.min, 450_000);
    assert_eq!(trade_stats.max, 600_000);
}

#[rstest]
fn test_spread_metrics() {
    let metrics = create_test_metrics("DOTUSD");
    
    // Record different spread values
    let spreads = vec![100, 50, 200, 75, 150, 80, 120];
    for spread in spreads {
        metrics.record_spread(spread);
    }
    
    let snapshot = metrics.get_snapshot();
    assert_eq!(snapshot.min_spread, 50);
    assert_eq!(snapshot.max_spread, 200);
    
    // Average should be reasonable
    assert!(snapshot.avg_spread > 50);
    assert!(snapshot.avg_spread < 200);
}

#[rstest]
fn test_depth_metrics() {
    let metrics = create_test_metrics("AVAXUSD");
    
    // Record depth measurements
    metrics.record_depth(5, 4, 50_000, 40_000); // 5 bid levels, 4 ask levels
    metrics.record_depth(7, 6, 70_000, 60_000); // More levels and volume
    metrics.record_depth(3, 3, 30_000, 30_000); // Fewer levels
    
    let snapshot = metrics.get_snapshot();
    assert_eq!(snapshot.max_bid_levels, 7);
    assert_eq!(snapshot.max_ask_levels, 6);
    
    // Average depths should be reasonable
    assert!(snapshot.avg_bid_depth > 30_000);
    assert!(snapshot.avg_bid_depth < 70_000);
    assert!(snapshot.avg_ask_depth > 30_000);
    assert!(snapshot.avg_ask_depth < 70_000);
}

#[rstest]
fn test_checksum_validation_metrics() {
    let metrics = create_test_metrics("ALGOUSD");
    
    // Record checksum validations
    metrics.record_checksum(true);  // Match
    metrics.record_checksum(true);  // Match
    metrics.record_checksum(false); // Mismatch
    metrics.record_checksum(true);  // Match
    
    let snapshot = metrics.get_snapshot();
    assert_eq!(snapshot.checksum_matches, 3);
    assert_eq!(snapshot.checksum_mismatches, 1);
}

#[rstest]
fn test_metrics_concurrent_updates() {
    let metrics = Arc::new(create_test_metrics("CONCURRENT_TEST"));
    let num_threads = 4;
    let operations_per_thread = 100;
    
    let handles: Vec<_> = (0..num_threads).map(|thread_id| {
        let metrics_clone = Arc::clone(&metrics);
        thread::spawn(move || {
            for i in 0..operations_per_thread {
                let latency = 1_000_000 + (thread_id * 100_000) + (i * 1_000);
                
                match i % 4 {
                    0 => metrics_clone.record_order_add(Qty::from_i64(1000), latency),
                    1 => metrics_clone.record_order_modify(latency),
                    2 => metrics_clone.record_order_cancel(Qty::from_i64(500), latency),
                    3 => metrics_clone.record_trade(Qty::from_i64(250), latency),
                    _ => unreachable!(),
                }
                
                // Record some spread and depth measurements
                if i % 10 == 0 {
                    metrics_clone.record_spread(100 + (i as i64));
                    metrics_clone.record_depth(5, 5, 10_000, 10_000);
                }
            }
        })
    }).collect();
    
    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread should complete successfully");
    }
    
    let snapshot = metrics.get_snapshot();
    
    // Verify that all operations were recorded
    assert_eq!(snapshot.orders_added, num_threads * operations_per_thread / 4);
    assert_eq!(snapshot.orders_modified, num_threads * operations_per_thread / 4);
    assert_eq!(snapshot.orders_canceled, num_threads * operations_per_thread / 4);
    assert_eq!(snapshot.trades_executed, num_threads * operations_per_thread / 4);
    
    // All latency stats should be available
    assert!(snapshot.latency_stats.order_add.is_some());
    assert!(snapshot.latency_stats.order_modify.is_some());
    assert!(snapshot.latency_stats.order_cancel.is_some());
    assert!(snapshot.latency_stats.trade.is_some());
}

#[rstest]
fn test_latency_tracker_creation() {
    let tracker = LatencyTracker::new();
    let stats = tracker.get_stats();
    
    // All operation types should be None initially
    assert!(stats.order_add.is_none());
    assert!(stats.order_modify.is_none());
    assert!(stats.order_cancel.is_none());
    assert!(stats.trade.is_none());
    assert!(stats.snapshot.is_none());
    assert!(stats.checksum.is_none());
    assert!(stats.replay.is_none());
}

#[rstest]
fn test_latency_tracker_single_operation_type() {
    let tracker = simulate_latencies(OperationType::OrderAdd, &[1000, 2000, 1500, 3000, 2500]);
    let stats = tracker.get_stats();
    
    let add_stats = stats.order_add.unwrap();
    assert_eq!(add_stats.count, 5);
    assert_eq!(add_stats.min, 1000);
    assert_eq!(add_stats.max, 3000);
    assert!(add_stats.mean >= 1000 && add_stats.mean <= 3000);
    assert!(add_stats.p50 >= 1000 && add_stats.p50 <= 3000);
    assert!(add_stats.p90 >= add_stats.p50);
    assert!(add_stats.p99 >= add_stats.p90);
}

#[rstest]
fn test_latency_tracker_multiple_operation_types() {
    let tracker = LatencyTracker::new();
    
    // Record different operation types
    tracker.record_operation(OperationType::OrderAdd, 1_000_000);
    tracker.record_operation(OperationType::OrderModify, 800_000);
    tracker.record_operation(OperationType::OrderCancel, 600_000);
    tracker.record_operation(OperationType::Trade, 400_000);
    tracker.record_operation(OperationType::Snapshot, 5_000_000);
    tracker.record_operation(OperationType::Checksum, 200_000);
    tracker.record_operation(OperationType::Replay, 10_000_000);
    
    let stats = tracker.get_stats();
    
    // All operation types should have stats
    assert!(stats.order_add.is_some());
    assert!(stats.order_modify.is_some());
    assert!(stats.order_cancel.is_some());
    assert!(stats.trade.is_some());
    assert!(stats.snapshot.is_some());
    assert!(stats.checksum.is_some());
    assert!(stats.replay.is_some());
    
    // Verify individual latencies
    assert_eq!(stats.order_add.unwrap().min, 1_000_000);
    assert_eq!(stats.trade.unwrap().min, 400_000);
    assert_eq!(stats.snapshot.unwrap().min, 5_000_000);
}

#[rstest]
fn test_latency_tracker_percentile_calculations() {
    // Create a dataset where percentiles are predictable
    let mut latencies = Vec::new();
    for i in 0..1000 {
        latencies.push((i + 1) * 1000); // 1000, 2000, 3000, ..., 1000000
    }
    
    let tracker = simulate_latencies(OperationType::Trade, &latencies);
    let stats = tracker.get_stats();
    let trade_stats = stats.trade.unwrap();
    
    assert_eq!(trade_stats.count, 1000);
    assert_eq!(trade_stats.min, 1000);
    assert_eq!(trade_stats.max, 1_000_000);
    
    // With uniform distribution, percentiles should be predictable
    assert!(trade_stats.p50 >= 400_000 && trade_stats.p50 <= 600_000);
    assert!(trade_stats.p90 >= 800_000 && trade_stats.p90 <= 950_000);
    assert!(trade_stats.p99 >= 950_000);
}

#[rstest]
fn test_latency_tracker_reset() {
    let tracker = LatencyTracker::new();
    
    // Record some data
    tracker.record_operation(OperationType::OrderAdd, 1_000_000);
    tracker.record_operation(OperationType::Trade, 500_000);
    
    let stats_before = tracker.get_stats();
    assert!(stats_before.order_add.is_some());
    assert!(stats_before.trade.is_some());
    
    // Reset the tracker
    tracker.reset();
    
    let stats_after = tracker.get_stats();
    assert!(stats_after.order_add.is_none());
    assert!(stats_after.trade.is_none());
}

#[rstest]
fn test_metrics_snapshot_formatting() {
    let metrics = create_test_metrics("MATICUSD");
    
    // Add some data
    metrics.record_order_add(Qty::from_i64(10_000), 1_500_000);
    metrics.record_trade(Qty::from_i64(5_000), 800_000);
    metrics.record_spread(150);
    
    let snapshot = metrics.get_snapshot();
    let report = snapshot.format_report();
    
    // Verify report contains expected sections
    assert!(report.contains("=== Orderbook Metrics: MATICUSD ==="));
    assert!(report.contains("Orders: 1 added"));
    assert!(report.contains("Trades: 1 executed"));
    assert!(report.contains("Volume: 10000 added"));
    assert!(report.contains("Volume: 5000 traded"));
    assert!(report.contains("Spread: min 150"));
    
    // Should contain latency sections if data exists
    if snapshot.latency_stats.order_add.is_some() {
        assert!(report.contains("Order Add Latency"));
    }
    if snapshot.latency_stats.trade.is_some() {
        assert!(report.contains("Trade Latency"));
    }
}

#[rstest]
fn test_extreme_latency_values() {
    let tracker = LatencyTracker::new();
    
    // Test with extreme latency values
    tracker.record_operation(OperationType::OrderAdd, 1); // 1 nanosecond (extremely fast)
    tracker.record_operation(OperationType::OrderAdd, u64::MAX / 2); // Very large latency
    tracker.record_operation(OperationType::OrderAdd, 1_000_000); // Normal latency
    
    let stats = tracker.get_stats();
    let add_stats = stats.order_add.unwrap();
    
    assert_eq!(add_stats.count, 3);
    assert_eq!(add_stats.min, 1);
    assert_eq!(add_stats.max, u64::MAX / 2);
    assert!(add_stats.mean > 1);
    assert!(add_stats.p50 > 0);
}

#[rstest]
fn test_metrics_edge_cases() {
    let metrics = create_test_metrics("EDGE_CASE_TEST");
    
    // Test with zero quantities
    metrics.record_order_add(Qty::ZERO, 1_000_000);
    metrics.record_order_cancel(Qty::ZERO, 800_000);
    metrics.record_trade(Qty::ZERO, 600_000);
    
    // Test with zero latency
    metrics.record_order_add(Qty::from_i64(1000), 0);
    
    // Test with very large quantities
    let large_qty = Qty::from_i64(i64::MAX / 1000);
    metrics.record_order_add(large_qty, 2_000_000);
    
    // Test with extreme spreads
    metrics.record_spread(0); // Zero spread
    metrics.record_spread(i64::MAX / 2); // Very large spread
    
    let snapshot = metrics.get_snapshot();
    
    // Should handle all edge cases gracefully
    assert!(snapshot.orders_added > 0);
    assert!(snapshot.orders_canceled > 0);
    assert!(snapshot.trades_executed > 0);
    assert!(snapshot.total_volume_added >= 0);
    assert!(snapshot.min_spread == 0);
    assert!(snapshot.max_spread > 0);
}

#[rstest]
fn test_operation_type_values() {
    // Ensure operation type enum values are stable for serialization
    assert_eq!(OperationType::OrderAdd as u8, 0);
    assert_eq!(OperationType::OrderModify as u8, 1);
    assert_eq!(OperationType::OrderCancel as u8, 2);
    assert_eq!(OperationType::Trade as u8, 3);
    assert_eq!(OperationType::Snapshot as u8, 4);
    assert_eq!(OperationType::Checksum as u8, 5);
    assert_eq!(OperationType::Replay as u8, 6);
}

#[rstest]
fn test_frequency_tracking() {
    let metrics = create_test_metrics("FREQUENCY_TEST");
    
    // Record operations in rapid succession
    for i in 0..100 {
        metrics.record_order_add(Qty::from_i64(1000), 1_000_000);
        // Small delay to avoid hitting frequency calculation edge cases
        thread::sleep(Duration::from_micros(1));
    }
    
    let snapshot = metrics.get_snapshot();
    assert_eq!(snapshot.orders_added, 100);
    
    // Updates per second should be reasonable (depends on timing)
    assert!(snapshot.updates_per_second >= 0);
}

#[rstest]
fn test_volume_calculations_accuracy() {
    let metrics = create_test_metrics("VOLUME_TEST");
    
    // Record precise volume calculations
    let add_volumes = vec![1_000, 2_500, 3_750, 4_250];
    let cancel_volumes = vec![500, 1_250, 2_000];
    let trade_volumes = vec![250, 750, 1_500, 3_000];
    
    for vol in add_volumes.iter() {
        metrics.record_order_add(Qty::from_i64(*vol), 1_000_000);
    }
    
    for vol in cancel_volumes.iter() {
        metrics.record_order_cancel(Qty::from_i64(*vol), 800_000);
    }
    
    for vol in trade_volumes.iter() {
        metrics.record_trade(Qty::from_i64(*vol), 600_000);
    }
    
    let snapshot = metrics.get_snapshot();
    
    let expected_add: i64 = add_volumes.iter().sum();
    let expected_cancel: i64 = cancel_volumes.iter().sum();
    let expected_trade: i64 = trade_volumes.iter().sum();
    
    assert_eq!(snapshot.total_volume_added, expected_add);
    assert_eq!(snapshot.total_volume_canceled, expected_cancel);
    assert_eq!(snapshot.total_volume_traded, expected_trade);
}