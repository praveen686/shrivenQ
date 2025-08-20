//! Performance and stress tests for risk manager

use risk_manager::{RiskLimits, RiskManagerService, RiskManager, RiskCheckResult};
use services_common::{Symbol, Side, Px, Qty};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinSet;
use tokio::sync::Semaphore;
use rstest::*;

#[fixture]
fn high_throughput_limits() -> RiskLimits {
    RiskLimits {
        max_position_size: u64::MAX / 2,
        max_position_value: u64::MAX / 2,
        max_total_exposure: u64::MAX / 2,
        max_order_size: u64::MAX / 2,
        max_order_value: u64::MAX / 2,
        max_orders_per_minute: 100_000, // Very high limit
        max_daily_loss: i64::MIN / 2,
        max_drawdown_pct: i32::MAX,
        circuit_breaker_threshold: u32::MAX,
        circuit_breaker_cooldown: 1,
    }
}

async fn create_high_performance_manager() -> RiskManagerService {
    let limits = RiskLimits {
        max_position_size: 1_000_000,
        max_position_value: 100_000_000,
        max_total_exposure: 1_000_000_000,
        max_order_size: 100_000,
        max_order_value: 10_000_000,
        max_orders_per_minute: 10_000,
        max_daily_loss: -10_000_000,
        max_drawdown_pct: 2000,
        circuit_breaker_threshold: 100,
        circuit_breaker_cooldown: 60,
    };
    RiskManagerService::new(limits)
}

#[tokio::test]
async fn test_order_check_throughput() {
    let risk_manager = Arc::new(create_high_performance_manager().await);
    let order_count = 10_000;
    let start_time = Instant::now();
    
    let mut join_set = JoinSet::new();
    let semaphore = Arc::new(Semaphore::new(100)); // Limit concurrent tasks
    
    for i in 0..order_count {
        let rm = risk_manager.clone();
        let sem = semaphore.clone();
        
        join_set.spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            
            let symbol = Symbol((i % 100) as u32);
            let qty = Qty::from_qty_i32(100_0000 + (i * 100));
            let price = Px::from_price_i32(100_0000 + (i * 10));
            
            let start = Instant::now();
            let result = rm.check_order(symbol, Side::Bid, qty, price).await;
            let duration = start.elapsed();
            
            (i, result, duration)
        });
    }
    
    let mut results = Vec::new();
    let mut total_duration = Duration::ZERO;
    
    while let Some(result) = join_set.join_next().await {
        let (id, check_result, duration) = result.unwrap();
        results.push((id, check_result));
        total_duration += duration;
    }
    
    let total_elapsed = start_time.elapsed();
    let throughput = order_count as f64 / total_elapsed.as_secs_f64();
    let avg_latency = total_duration / order_count as u32;
    
    println!("Order check performance:");
    println!("  Total orders: {}", order_count);
    println!("  Total time: {:?}", total_elapsed);
    println!("  Throughput: {:.2} orders/sec", throughput);
    println!("  Average latency: {:?}", avg_latency);
    
    // Performance assertions
    assert!(throughput > 1000.0, "Throughput should be > 1000 orders/sec, got {:.2}", throughput);
    assert!(avg_latency < Duration::from_millis(10), "Average latency should be < 10ms, got {:?}", avg_latency);
    
    // Verify most orders were processed successfully
    let approved_count = results.iter()
        .filter(|(_, result)| matches!(result, RiskCheckResult::Approved))
        .count();
    
    assert!(approved_count > (order_count as usize) / 2, "Most orders should be approved");
}

#[tokio::test]
async fn test_position_update_throughput() {
    let risk_manager = Arc::new(create_high_performance_manager().await);
    let update_count = 5_000;
    let start_time = Instant::now();
    
    let mut join_set = JoinSet::new();
    let semaphore = Arc::new(Semaphore::new(50));
    
    for i in 0..update_count {
        let rm = risk_manager.clone();
        let sem = semaphore.clone();
        
        join_set.spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            
            let symbol = Symbol((i % 50) as u32);
            let side = if i % 2 == 0 { Side::Bid } else { Side::Ask };
            let qty = Qty::from_qty_i32(10_0000 + (i * 10));
            let price = Px::from_price_i32(100_0000 + (i * 5));
            
            let start = Instant::now();
            let result = rm.update_position(symbol, side, qty, price).await;
            let duration = start.elapsed();
            
            (i, result, duration)
        });
    }
    
    let mut success_count = 0;
    let mut total_duration = Duration::ZERO;
    
    while let Some(result) = join_set.join_next().await {
        let (_, update_result, duration) = result.unwrap();
        if update_result.is_ok() {
            success_count += 1;
        }
        total_duration += duration;
    }
    
    let total_elapsed = start_time.elapsed();
    let throughput = update_count as f64 / total_elapsed.as_secs_f64();
    let avg_latency = total_duration / update_count as u32;
    
    println!("Position update performance:");
    println!("  Total updates: {}", update_count);
    println!("  Successful updates: {}", success_count);
    println!("  Total time: {:?}", total_elapsed);
    println!("  Throughput: {:.2} updates/sec", throughput);
    println!("  Average latency: {:?}", avg_latency);
    
    // Performance assertions
    assert!(throughput > 500.0, "Update throughput should be > 500/sec, got {:.2}", throughput);
    assert!(avg_latency < Duration::from_millis(20), "Update latency should be < 20ms, got {:?}", avg_latency);
    assert_eq!(success_count, update_count, "All position updates should succeed");
}

#[tokio::test]
async fn test_mixed_operation_performance() {
    let risk_manager = Arc::new(create_high_performance_manager().await);
    let operation_count = 10_000;
    let start_time = Instant::now();
    
    let mut join_set = JoinSet::new();
    let semaphore = Arc::new(Semaphore::new(100));
    
    for i in 0..operation_count {
        let rm = risk_manager.clone();
        let sem = semaphore.clone();
        
        join_set.spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let symbol = Symbol((i % 200) as u32);
            
            let start = Instant::now();
            
            let operation_result = match i % 5 {
                0 => {
                    // Order check
                    let result = rm.check_order(
                        symbol,
                        Side::Bid,
                        Qty::from_qty_i32(100_0000),
                        Px::from_price_i32(100_0000),
                    ).await;
                    matches!(result, RiskCheckResult::Approved | RiskCheckResult::Rejected(_))
                }
                1 => {
                    // Position update
                    let result = rm.update_position(
                        symbol,
                        Side::Bid,
                        Qty::from_qty_i32(10_0000),
                        Px::from_price_i32(100_0000),
                    ).await;
                    result.is_ok()
                }
                2 => {
                    // Mark price update
                    let result = rm.update_mark_price(
                        symbol,
                        Px::from_price_i32(105_0000),
                    ).await;
                    result.is_ok()
                }
                3 => {
                    // Get metrics
                    let _metrics = rm.get_metrics().await;
                    true
                }
                _ => {
                    // Get position
                    let _position = rm.get_position(symbol).await;
                    true
                }
            };
            
            let duration = start.elapsed();
            (i, operation_result, duration)
        });
    }
    
    let mut success_count = 0;
    let mut total_duration = Duration::ZERO;
    let mut max_latency = Duration::ZERO;
    
    while let Some(result) = join_set.join_next().await {
        let (_, success, duration) = result.unwrap();
        if success {
            success_count += 1;
        }
        total_duration += duration;
        if duration > max_latency {
            max_latency = duration;
        }
    }
    
    let total_elapsed = start_time.elapsed();
    let throughput = operation_count as f64 / total_elapsed.as_secs_f64();
    let avg_latency = total_duration / operation_count as u32;
    
    println!("Mixed operations performance:");
    println!("  Total operations: {}", operation_count);
    println!("  Successful operations: {}", success_count);
    println!("  Total time: {:?}", total_elapsed);
    println!("  Throughput: {:.2} ops/sec", throughput);
    println!("  Average latency: {:?}", avg_latency);
    println!("  Max latency: {:?}", max_latency);
    
    // Performance assertions
    assert!(throughput > 800.0, "Mixed ops throughput should be > 800/sec, got {:.2}", throughput);
    assert!(avg_latency < Duration::from_millis(15), "Avg latency should be < 15ms, got {:?}", avg_latency);
    assert!(max_latency < Duration::from_millis(100), "Max latency should be < 100ms, got {:?}", max_latency);
    assert!(success_count > operation_count * 95 / 100, "Success rate should be > 95%");
}

#[tokio::test]
async fn test_memory_usage_under_high_load() {
    let risk_manager = Arc::new(create_high_performance_manager().await);
    let symbol_count = 10_000;
    
    // Create positions for many symbols
    let start_time = Instant::now();
    let mut join_set = JoinSet::new();
    let semaphore = Arc::new(Semaphore::new(50));
    
    for i in 0..symbol_count {
        let rm = risk_manager.clone();
        let sem = semaphore.clone();
        
        join_set.spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let symbol = Symbol(i as u32);
            
            // Create position
            let _ = rm.update_position(
                symbol,
                Side::Bid,
                Qty::from_qty_i32(100_0000),
                Px::from_price_i32(100_0000),
            ).await;
            
            // Update mark price
            let _ = rm.update_mark_price(
                symbol,
                Px::from_price_i32(105_0000),
            ).await;
            
            i
        });
    }
    
    // Wait for all position creations
    let mut created_count = 0;
    while let Some(result) = join_set.join_next().await {
        result.unwrap();
        created_count += 1;
    }
    
    let creation_time = start_time.elapsed();
    println!("Memory usage test:");
    println!("  Created {} positions in {:?}", created_count, creation_time);
    
    // Test metrics retrieval performance with many positions
    let metrics_start = Instant::now();
    let metrics = risk_manager.get_metrics().await;
    let metrics_time = metrics_start.elapsed();
    
    println!("  Metrics retrieval time: {:?}", metrics_time);
    println!("  Open positions: {}", metrics.open_positions);
    
    assert_eq!(metrics.open_positions, symbol_count as u32);
    assert!(metrics_time < Duration::from_millis(100), "Metrics retrieval should be fast");
    
    // Test bulk position retrieval
    let positions_start = Instant::now();
    let positions = risk_manager.get_all_positions().await;
    let positions_time = positions_start.elapsed();
    
    println!("  All positions retrieval time: {:?}", positions_time);
    println!("  Retrieved {} positions", positions.len());
    
    assert_eq!(positions.len(), symbol_count);
    assert!(positions_time < Duration::from_millis(500), "Position retrieval should be reasonable");
}

#[tokio::test] 
async fn test_concurrent_access_scalability() {
    let risk_manager = Arc::new(create_high_performance_manager().await);
    
    // Test with increasing concurrency levels
    let concurrency_levels = [10, 50, 100, 200, 500];
    
    for &concurrency in &concurrency_levels {
        let operations_per_thread = 100;
        let start_time = Instant::now();
        
        let mut join_set = JoinSet::new();
        
        for thread_id in 0..concurrency {
            let rm = risk_manager.clone();
            
            join_set.spawn(async move {
                let mut thread_success = 0;
                
                for i in 0..operations_per_thread {
                    let symbol = Symbol(((thread_id * operations_per_thread + i) % 1000) as u32);
                    
                    match i % 3 {
                        0 => {
                            let result = rm.check_order(
                                symbol,
                                Side::Bid,
                                Qty::from_qty_i32(100_0000),
                                Px::from_price_i32(100_0000),
                            ).await;
                            if matches!(result, RiskCheckResult::Approved) {
                                thread_success += 1;
                            }
                        }
                        1 => {
                            if rm.update_position(
                                symbol,
                                Side::Bid,
                                Qty::from_qty_i32(10_0000),
                                Px::from_price_i32(100_0000),
                            ).await.is_ok() {
                                thread_success += 1;
                            }
                        }
                        _ => {
                            let _metrics = rm.get_metrics().await;
                            thread_success += 1;
                        }
                    }
                }
                
                (thread_id, thread_success)
            });
        }
        
        // Collect results
        let mut total_success = 0;
        while let Some(result) = join_set.join_next().await {
            let (_, success) = result.unwrap();
            total_success += success;
        }
        
        let elapsed = start_time.elapsed();
        let total_operations = concurrency * operations_per_thread;
        let throughput = total_operations as f64 / elapsed.as_secs_f64();
        
        println!("Concurrency level {}: {} ops in {:?} = {:.2} ops/sec ({}% success)",
                 concurrency, total_operations, elapsed, throughput,
                 (total_success * 100) / total_operations);
        
        // Throughput should scale reasonably with concurrency
        assert!(throughput > (concurrency as f64 * 5.0), 
                "Throughput should scale with concurrency");
        
        // Success rate should remain high
        assert!(total_success > total_operations * 80 / 100,
                "Success rate should remain > 80% even under high concurrency");
    }
}

#[tokio::test]
async fn test_rate_limiter_performance() {
    let mut limits = RiskLimits::default();
    limits.max_orders_per_minute = 1000; // 1000 orders per minute
    let risk_manager = Arc::new(RiskManagerService::new(limits));
    
    let request_count = 2000; // Request more than limit
    let start_time = Instant::now();
    
    let mut join_set = JoinSet::new();
    let semaphore = Arc::new(Semaphore::new(100));
    
    for i in 0..request_count {
        let rm = risk_manager.clone();
        let sem = semaphore.clone();
        
        join_set.spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            
            let start = Instant::now();
            let result = rm.check_order(
                Symbol((i % 10) as u32),
                Side::Bid,
                Qty::from_qty_i32(100_0000),
                Px::from_price_i32(100_0000),
            ).await;
            let duration = start.elapsed();
            
            (i, result, duration)
        });
    }
    
    let mut approved_count = 0;
    let mut rejected_count = 0;
    let mut rate_limit_rejections = 0;
    let mut total_duration = Duration::ZERO;
    
    while let Some(result) = join_set.join_next().await {
        let (_, check_result, duration) = result.unwrap();
        total_duration += duration;
        
        match check_result {
            RiskCheckResult::Approved => approved_count += 1,
            RiskCheckResult::Rejected(reason) => {
                rejected_count += 1;
                if reason.contains("Rate limit") {
                    rate_limit_rejections += 1;
                }
            }
            _ => {}
        }
    }
    
    let total_elapsed = start_time.elapsed();
    let throughput = request_count as f64 / total_elapsed.as_secs_f64();
    let avg_latency = total_duration / request_count as u32;
    
    println!("Rate limiter performance:");
    println!("  Total requests: {}", request_count);
    println!("  Approved: {}", approved_count);
    println!("  Rejected: {} (rate limited: {})", rejected_count, rate_limit_rejections);
    println!("  Total time: {:?}", total_elapsed);
    println!("  Throughput: {:.2} req/sec", throughput);
    println!("  Average latency: {:?}", avg_latency);
    
    // Rate limiter should work efficiently
    assert!(avg_latency < Duration::from_millis(5), "Rate limiting should be fast");
    assert!(approved_count <= 1000, "Should not exceed rate limit");
    assert!(rate_limit_rejections > 0, "Should see rate limit rejections");
}

#[rstest]
#[tokio::test]
async fn test_stress_with_circuit_breaker(high_throughput_limits: RiskLimits) {
    let mut limits = high_throughput_limits;
    limits.circuit_breaker_threshold = 100; // Low threshold for testing
    limits.circuit_breaker_cooldown = 10;   // Short cooldown
    let risk_manager = Arc::new(RiskManagerService::new(limits));
    
    // Create a scenario that might trigger circuit breaker
    let operation_count = 1000;
    let mut join_set = JoinSet::new();
    let semaphore = Arc::new(Semaphore::new(50));
    
    for i in 0..operation_count {
        let rm = risk_manager.clone();
        let sem = semaphore.clone();
        
        join_set.spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            
            // Mix of operations that could cause failures
            let symbol = Symbol((i % 20) as u32);
            
            let start = Instant::now();
            
            // Rapid sequence of operations
            let check1 = rm.check_order(
                symbol,
                Side::Bid,
                Qty::from_qty_i32(1000_0000), // Large order
                Px::from_price_i32(100_0000),
            ).await;
            
            let update1 = rm.update_position(
                symbol,
                Side::Bid,
                Qty::from_qty_i32(100_0000),
                Px::from_price_i32(100_0000),
            ).await;
            
            let check2 = rm.check_order(
                symbol,
                Side::Ask,
                Qty::from_qty_i32(50_0000),
                Px::from_price_i32(100_0000),
            ).await;
            
            let duration = start.elapsed();
            (i, check1, update1, check2, duration)
        });
    }
    
    let mut results = Vec::new();
    let mut total_duration = Duration::ZERO;
    
    while let Some(result) = join_set.join_next().await {
        let (id, check1, update1, check2, duration) = result.unwrap();
        results.push((id, check1, update1.is_ok(), check2));
        total_duration += duration;
    }
    
    let avg_latency = total_duration / operation_count as u32;
    
    println!("Stress test with circuit breaker:");
    println!("  Operations completed: {}", results.len());
    println!("  Average latency: {:?}", avg_latency);
    
    // Analyze results
    let approved_count = results.iter()
        .filter(|(_, check1, _, check2)| {
            matches!(check1, RiskCheckResult::Approved) || 
            matches!(check2, RiskCheckResult::Approved)
        })
        .count();
    
    let update_success_count = results.iter()
        .filter(|(_, _, update_ok, _)| *update_ok)
        .count();
    
    println!("  Approved orders: {}", approved_count);
    println!("  Successful updates: {}", update_success_count);
    
    // Should handle stress without excessive latency
    assert!(avg_latency < Duration::from_millis(50), 
            "Should maintain reasonable latency under stress");
    
    // Most operations should complete successfully
    assert!(results.len() == operation_count, "All operations should complete");
    assert!(update_success_count > operation_count * 90 / 100, 
            "Most updates should succeed");
}

#[tokio::test]
async fn test_long_running_stability() {
    let risk_manager = Arc::new(create_high_performance_manager().await);
    let duration = Duration::from_secs(30); // 30-second stress test
    let start_time = Instant::now();
    
    let mut join_set = JoinSet::new();
    let operations_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    
    // Launch continuous operations
    for worker_id in 0..20 {
        let rm = risk_manager.clone();
        let counter = operations_counter.clone();
        
        join_set.spawn(async move {
            let mut local_operations = 0;
            let worker_start = Instant::now();
            
            while worker_start.elapsed() < duration {
                let symbol = Symbol((worker_id * 1000 + local_operations % 1000) as u32);
                
                // Perform operation based on cycle
                match local_operations % 4 {
                    0 => {
                        let _ = rm.check_order(
                            symbol,
                            Side::Bid,
                            Qty::from_qty_i32(100_0000),
                            Px::from_price_i32(100_0000),
                        ).await;
                    }
                    1 => {
                        let _ = rm.update_position(
                            symbol,
                            Side::Bid,
                            Qty::from_qty_i32(10_0000),
                            Px::from_price_i32(100_0000),
                        ).await;
                    }
                    2 => {
                        let _ = rm.update_mark_price(symbol, Px::from_price_i32(105_0000)).await;
                    }
                    _ => {
                        let _ = rm.get_metrics().await;
                    }
                }
                
                local_operations += 1;
                counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                
                // Small delay to prevent overwhelming
                tokio::time::sleep(Duration::from_micros(100)).await;
            }
            
            local_operations
        });
    }
    
    // Wait for all workers to complete
    let mut total_worker_operations = 0;
    while let Some(result) = join_set.join_next().await {
        total_worker_operations += result.unwrap();
    }
    
    let total_elapsed = start_time.elapsed();
    let total_operations = operations_counter.load(std::sync::atomic::Ordering::Relaxed);
    let throughput = total_operations as f64 / total_elapsed.as_secs_f64();
    
    println!("Long-running stability test:");
    println!("  Duration: {:?}", total_elapsed);
    println!("  Total operations: {}", total_operations);
    println!("  Worker operations: {}", total_worker_operations);
    println!("  Throughput: {:.2} ops/sec", throughput);
    
    // Final state check
    let final_metrics = risk_manager.get_metrics().await;
    println!("  Final open positions: {}", final_metrics.open_positions);
    println!("  Final orders today: {}", final_metrics.orders_today);
    
    // Stability assertions
    assert!(throughput > 100.0, "Should maintain reasonable throughput over time");
    assert_eq!(total_operations, total_worker_operations as u64, "Operation counts should match");
    assert!(!risk_manager.is_kill_switch_active(), "Kill switch should not be activated");
    
    // Final functionality test
    let final_order_result = risk_manager.check_order(
        Symbol(99999),
        Side::Bid,
        Qty::from_qty_i32(100_0000),
        Px::from_price_i32(100_0000),
    ).await;
    
    assert!(matches!(final_order_result, RiskCheckResult::Approved | RiskCheckResult::Rejected(_)),
            "Should still function normally after stress test");
}