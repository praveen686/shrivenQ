//! Performance and stress tests for trading gateway
//!
//! These tests verify system performance under extreme conditions:
//! - High-frequency market data processing
//! - Concurrent order processing
//! - Memory usage under load
//! - Latency characteristics
//! - System stability under stress
//! - Resource utilization
//! - Throughput measurements

use anyhow::Result;
use orderbook::{analytics::MicrostructureAnalytics, OrderBook};
use rstest::*;
use services_common::{Px, Qty, Symbol, Ts};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use trading_gateway::{
    GatewayConfig, OrderType, Side, TradingEvent, TradingGateway,
};

/// Test fixture for creating a high-performance gateway configuration
#[fixture]
async fn high_performance_gateway() -> Arc<TradingGateway> {
    let config = GatewayConfig {
        max_position_size: Qty::from_i64(1000000), // 100 units - large positions
        max_daily_loss: 10000000,                   // 1000 USDT - high limit
        risk_check_interval: Duration::from_millis(50), // Fast risk checks
        orderbook_throttle_ms: 1,                   // Minimal throttling
        enable_market_making: true,
        enable_momentum: true,
        enable_arbitrage: true,
        circuit_breaker_threshold: 0.20, // High threshold - 20%
    };
    
    Arc::new(TradingGateway::new(config).await.unwrap())
}

#[rstest]
#[tokio::test]
async fn test_high_frequency_market_data_stress(high_performance_gateway: Arc<TradingGateway>) -> Result<()> {
    high_performance_gateway.start().await?;
    
    let symbol = Symbol(1);
    let updates_per_second = 1000;
    let duration_seconds = 10;
    let total_updates = updates_per_second * duration_seconds;
    
    println!("Starting high-frequency market data stress test:");
    println!("  Target: {} updates/second for {} seconds", updates_per_second, duration_seconds);
    println!("  Total updates: {}", total_updates);
    
    let start_time = Instant::now();
    let mut successful_updates = 0u64;
    
    for i in 1..=total_updates {
        let orderbook = OrderBook::new("HF_STRESS");
        let analytics = MicrostructureAnalytics::new();
        
        // Create realistic but fast-changing market data
        let base_price = 5000000000i64; // $500
        let price_noise = (i * 1234567) % 100000; // Pseudo-random price changes
        let current_price = base_price + price_noise - 50000; // +/- $5
        
        let quantity_noise = (i * 987654) % 5000 + 5000; // 0.5-1.0 BTC
        
        let bid_levels = vec![
            (Px::from_i64(current_price), Qty::from_i64(quantity_noise), 1),
            (Px::from_i64(current_price - 10000), Qty::from_i64(quantity_noise + 2000), 2),
        ];
        
        let ask_levels = vec![
            (Px::from_i64(current_price + 10000), Qty::from_i64(quantity_noise - 1000), 1),
            (Px::from_i64(current_price + 20000), Qty::from_i64(quantity_noise + 1000), 2),
        ];
        
        orderbook.load_snapshot(bid_levels, ask_levels);
        
        // Add trade data for analytics
        analytics.update_trade(
            Px::from_i64(current_price + 5000),
            Qty::from_i64(1000 + (i % 3000)),
            i % 2 == 0,
            Ts::now(),
        );
        
        // Process update
        if high_performance_gateway.process_orderbook_update(symbol, &orderbook, &analytics).await.is_ok() {
            successful_updates += 1;
        }
        
        // Precise timing control
        let target_interval = Duration::from_nanos(1_000_000_000 / updates_per_second as u64);
        let elapsed = start_time.elapsed();
        let expected_time = target_interval * i as u32;
        
        if elapsed < expected_time {
            sleep(expected_time - elapsed).await;
        }
        
        // Progress reporting
        if i % 2000 == 0 {
            let current_rate = successful_updates as f64 / start_time.elapsed().as_secs_f64();
            println!("  Progress: {}/{} updates, current rate: {:.0}/sec", i, total_updates, current_rate);
        }
    }
    
    let total_time = start_time.elapsed();
    let actual_rate = successful_updates as f64 / total_time.as_secs_f64();
    
    // Collect final metrics
    let telemetry = high_performance_gateway.telemetry.get_stats().await;
    let risk_metrics = high_performance_gateway.risk_gate.get_metrics();
    
    println!("\n=== High-Frequency Stress Test Results ===");
    println!("Total time: {:?}", total_time);
    println!("Successful updates: {}/{}", successful_updates, total_updates);
    println!("Actual rate: {:.2} updates/second", actual_rate);
    println!("Target rate: {} updates/second", updates_per_second);
    println!("Success rate: {:.2}%", (successful_updates as f64 / total_updates as f64) * 100.0);
    println!("Telemetry updates: {}", telemetry.orderbook_updates);
    println!("Risk checks: {}", risk_metrics.orders_checked);
    println!("Average risk latency: {}ns", risk_metrics.avg_latency_ns);
    
    // Performance assertions
    assert!(successful_updates >= (total_updates as u64 * 95 / 100), "Should process >95% of updates");
    assert!(actual_rate >= (updates_per_second as f64 * 0.8), "Should achieve >80% of target rate");
    
    high_performance_gateway.stop().await?;
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_concurrent_order_processing_stress(high_performance_gateway: Arc<TradingGateway>) -> Result<()> {
    high_performance_gateway.start().await?;
    
    let concurrent_clients = 50;
    let orders_per_client = 100;
    let total_orders = concurrent_clients * orders_per_client;
    
    println!("Starting concurrent order processing stress test:");
    println!("  {} concurrent clients", concurrent_clients);
    println!("  {} orders per client", orders_per_client);
    println!("  Total orders: {}", total_orders);
    
    let start_time = Instant::now();
    let mut handles = Vec::new();
    
    // Spawn concurrent order submission tasks
    for client_id in 1..=concurrent_clients {
        let gateway = high_performance_gateway.clone();
        let handle = tokio::spawn(async move {
            let mut client_stats = (0u32, 0u32); // (successful, failed)
            
            for order_id in 1..=orders_per_client {
                let symbol = Symbol((client_id % 10 + 1) as u32); // 10 symbols
                let global_order_id = (client_id * 1000 + order_id) as u64;
                
                let order = TradingEvent::OrderRequest {
                    id: global_order_id,
                    symbol,
                    side: if order_id % 2 == 0 { Side::Buy } else { Side::Sell },
                    order_type: if order_id % 3 == 0 { OrderType::Limit } else { OrderType::Market },
                    quantity: Qty::from_i64(1000 + (order_id as i64 * 100)), // Varying sizes
                    price: if order_id % 3 == 0 { 
                        Some(Px::from_i64(1000000000 + (client_id as i64 * 10000)))
                    } else { 
                        None 
                    },
                    time_in_force: trading_gateway::TimeInForce::Ioc,
                    strategy_id: format!("stress_client_{}", client_id),
                };
                
                if gateway.execution_engine.submit_order(order).await.is_ok() {
                    client_stats.0 += 1;
                } else {
                    client_stats.1 += 1;
                }
                
                // Small delay to prevent overwhelming the system
                if order_id % 10 == 0 {
                    tokio::time::sleep(Duration::from_micros(100)).await;
                }
            }
            
            client_stats
        });
        handles.push(handle);
    }
    
    // Wait for all clients to complete
    let mut total_successful = 0u32;
    let mut total_failed = 0u32;
    
    for handle in handles {
        let (successful, failed) = handle.await?;
        total_successful += successful;
        total_failed += failed;
    }
    
    let total_time = start_time.elapsed();
    let order_rate = total_successful as f64 / total_time.as_secs_f64();
    
    // Collect metrics
    let exec_metrics = high_performance_gateway.execution_engine.get_metrics();
    let risk_metrics = high_performance_gateway.risk_gate.get_metrics();
    
    println!("\n=== Concurrent Order Stress Test Results ===");
    println!("Total time: {:?}", total_time);
    println!("Successful orders: {}", total_successful);
    println!("Failed orders: {}", total_failed);
    println!("Success rate: {:.2}%", (total_successful as f64 / total_orders as f64) * 100.0);
    println!("Order rate: {:.2} orders/second", order_rate);
    println!("Execution engine orders: {}", exec_metrics.orders_submitted);
    println!("Risk checks: {}", risk_metrics.orders_checked);
    println!("Risk rejections: {}", risk_metrics.orders_rejected);
    
    // Performance assertions
    assert!(total_successful >= (total_orders * 80 / 100), "Should process >80% of orders successfully");
    assert!(order_rate >= 100.0, "Should process >100 orders/second");
    assert_eq!(exec_metrics.orders_submitted as u32, total_successful, "Metrics should match");
    
    high_performance_gateway.stop().await?;
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_memory_usage_under_load(high_performance_gateway: Arc<TradingGateway>) -> Result<()> {
    high_performance_gateway.start().await?;
    
    let symbols = (1..=100).map(Symbol).collect::<Vec<_>>(); // 100 symbols
    let rounds = 1000;
    
    println!("Starting memory usage stress test:");
    println!("  {} symbols", symbols.len());
    println!("  {} rounds of updates", rounds);
    
    let start_time = Instant::now();
    
    // Initial memory measurement would go here in a real system
    // For testing, we focus on ensuring the system doesn't crash
    
    for round in 1..=rounds {
        for (i, &symbol) in symbols.iter().enumerate() {
            let orderbook = OrderBook::new(&format!("MEM_TEST_{}", i));
            let analytics = MicrostructureAnalytics::new();
            
            let base_price = 1000000000 + (i as i64 * 1000000); // $100 + $1 per symbol
            let price_variation = ((round * (i + 1)) % 1000) as i64 * 100;
            let current_price = base_price + price_variation;
            
            // Create varying market depth
            let mut bid_levels = Vec::new();
            let mut ask_levels = Vec::new();
            
            for level in 1..=(5 + (i % 10)) { // 5-15 levels per symbol
                bid_levels.push((
                    Px::from_i64(current_price - (level as i64 * 1000)),
                    Qty::from_i64(5000 + (round * level) % 20000),
                    level,
                ));
                ask_levels.push((
                    Px::from_i64(current_price + (level as i64 * 1000)),
                    Qty::from_i64(4000 + (round * level) % 18000),
                    level,
                ));
            }
            
            orderbook.load_snapshot(bid_levels, ask_levels);
            
            // Add multiple trades for analytics
            for j in 0..((i % 5) + 1) {
                analytics.update_trade(
                    Px::from_i64(current_price + (j as i64 * 500)),
                    Qty::from_i64(1000 + (round * (j + 1)) % 5000),
                    (round + i + j) % 2 == 0,
                    Ts::now(),
                );
            }
            
            high_performance_gateway.process_orderbook_update(symbol, &orderbook, &analytics).await?;
            
            // Create some positions to test position tracking memory
            if round % 100 == 0 && i < 10 {
                high_performance_gateway.position_manager.update_position(
                    symbol,
                    if i % 2 == 0 { Side::Buy } else { Side::Sell },
                    Qty::from_i64(5000 + (i as i64 * 1000)),
                    Px::from_i64(current_price),
                ).await?;
            }
        }
        
        if round % 100 == 0 {
            let elapsed = start_time.elapsed();
            let rate = (round * symbols.len()) as f64 / elapsed.as_secs_f64();
            println!("  Round {}/{}, rate: {:.0} updates/sec", round, rounds, rate);
            
            // Brief pause to let system catch up
            sleep(Duration::from_millis(10)).await;
        }
    }
    
    let total_time = start_time.elapsed();
    let total_updates = rounds * symbols.len();
    let final_rate = total_updates as f64 / total_time.as_secs_f64();
    
    // Collect final metrics
    let telemetry = high_performance_gateway.telemetry.get_stats().await;
    let position_count = high_performance_gateway.position_manager.get_position_count().await;
    let risk_metrics = high_performance_gateway.risk_gate.get_metrics();
    
    println!("\n=== Memory Usage Stress Test Results ===");
    println!("Total time: {:?}", total_time);
    println!("Total updates: {}", total_updates);
    println!("Final rate: {:.2} updates/second", final_rate);
    println!("Telemetry updates: {}", telemetry.orderbook_updates);
    println!("Positions tracked: {}", position_count);
    println!("Risk checks: {}", risk_metrics.orders_checked);
    
    // System should remain stable
    assert!(final_rate > 500.0, "Should maintain >500 updates/second");
    assert_eq!(telemetry.orderbook_updates as usize, total_updates, "Should process all updates");
    
    high_performance_gateway.stop().await?;
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_latency_characteristics(high_performance_gateway: Arc<TradingGateway>) -> Result<()> {
    high_performance_gateway.start().await?;
    
    let symbol = Symbol(1);
    let test_samples = 1000;
    
    println!("Starting latency characteristics test:");
    println!("  {} samples", test_samples);
    
    let mut latencies = Vec::with_capacity(test_samples);
    
    for i in 1..=test_samples {
        let start = Instant::now();
        
        let orderbook = OrderBook::new("LATENCY_TEST");
        let analytics = MicrostructureAnalytics::new();
        
        let base_price = 3000000000i64; // $300
        let price_noise = (i * 12345) % 50000;
        let current_price = base_price + price_noise;
        
        let bid_levels = vec![
            (Px::from_i64(current_price), Qty::from_i64(10000), 1),
            (Px::from_i64(current_price - 5000), Qty::from_i64(15000), 2),
        ];
        
        let ask_levels = vec![
            (Px::from_i64(current_price + 5000), Qty::from_i64(8000), 1),
            (Px::from_i64(current_price + 10000), Qty::from_i64(12000), 2),
        ];
        
        orderbook.load_snapshot(bid_levels, ask_levels);
        analytics.update_trade(Px::from_i64(current_price + 2500), Qty::from_i64(3000), i % 2 == 0, Ts::now());
        
        // Measure processing latency
        high_performance_gateway.process_orderbook_update(symbol, &orderbook, &analytics).await?;
        
        let latency = start.elapsed();
        latencies.push(latency.as_nanos() as u64);
        
        // Small delay between measurements
        if i % 100 == 0 {
            sleep(Duration::from_millis(1)).await;
        }
    }
    
    // Calculate latency statistics
    latencies.sort();
    let min_latency = latencies[0];
    let max_latency = latencies[latencies.len() - 1];
    let median_latency = latencies[latencies.len() / 2];
    let p95_latency = latencies[latencies.len() * 95 / 100];
    let p99_latency = latencies[latencies.len() * 99 / 100];
    let avg_latency: u64 = latencies.iter().sum::<u64>() / latencies.len() as u64;
    
    println!("\n=== Latency Characteristics Results ===");
    println!("Min latency: {}ns ({:.2}μs)", min_latency, min_latency as f64 / 1000.0);
    println!("Avg latency: {}ns ({:.2}μs)", avg_latency, avg_latency as f64 / 1000.0);
    println!("Median latency: {}ns ({:.2}μs)", median_latency, median_latency as f64 / 1000.0);
    println!("P95 latency: {}ns ({:.2}μs)", p95_latency, p95_latency as f64 / 1000.0);
    println!("P99 latency: {}ns ({:.2}μs)", p99_latency, p99_latency as f64 / 1000.0);
    println!("Max latency: {}ns ({:.2}μs)", max_latency, max_latency as f64 / 1000.0);
    
    // Get risk check latencies for comparison
    let risk_metrics = high_performance_gateway.risk_gate.get_metrics();
    if risk_metrics.orders_checked > 0 {
        println!("Risk check avg latency: {}ns ({:.2}μs)", 
                risk_metrics.avg_latency_ns, 
                risk_metrics.avg_latency_ns as f64 / 1000.0);
    }
    
    // Latency assertions (adjust thresholds based on requirements)
    assert!(avg_latency < 100_000, "Average latency should be <100μs"); // 100 microseconds
    assert!(p95_latency < 500_000, "P95 latency should be <500μs");
    assert!(p99_latency < 1_000_000, "P99 latency should be <1ms");
    assert!(max_latency < 10_000_000, "Max latency should be <10ms");
    
    high_performance_gateway.stop().await?;
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_system_stability_long_running(high_performance_gateway: Arc<TradingGateway>) -> Result<()> {
    high_performance_gateway.start().await?;
    
    let duration = Duration::from_secs(30); // 30-second stability test
    let symbols = vec![Symbol(1), Symbol(2), Symbol(3), Symbol(4), Symbol(5)];
    
    println!("Starting system stability test:");
    println!("  Duration: {:?}", duration);
    println!("  Symbols: {}", symbols.len());
    
    let start_time = Instant::now();
    let mut update_count = 0u64;
    let mut error_count = 0u64;
    
    while start_time.elapsed() < duration {
        for (i, &symbol) in symbols.iter().enumerate() {
            let orderbook = OrderBook::new(&format!("STABILITY_{}", i));
            let analytics = MicrostructureAnalytics::new();
            
            let time_factor = start_time.elapsed().as_millis() as i64;
            let base_price = 2000000000 + (i as i64 * 100000000); // $200, $300, $400, etc.
            let price_drift = (time_factor / 1000) % 200000; // Slow price drift
            let current_price = base_price + price_drift;
            
            let bid_levels = vec![
                (Px::from_i64(current_price), Qty::from_i64(10000 + (time_factor % 10000) as i64), 1),
                (Px::from_i64(current_price - 10000), Qty::from_i64(15000), 2),
            ];
            
            let ask_levels = vec![
                (Px::from_i64(current_price + 10000), Qty::from_i64(8000 + (time_factor % 8000) as i64), 1),
                (Px::from_i64(current_price + 20000), Qty::from_i64(12000), 2),
            ];
            
            orderbook.load_snapshot(bid_levels, ask_levels);
            
            // Continuous trade flow
            analytics.update_trade(
                Px::from_i64(current_price + 5000),
                Qty::from_i64(2000 + (time_factor % 3000) as i64),
                (update_count + i as u64) % 3 != 0,
                Ts::now(),
            );
            
            match high_performance_gateway.process_orderbook_update(symbol, &orderbook, &analytics).await {
                Ok(_) => update_count += 1,
                Err(_) => error_count += 1,
            }
            
            // Occasionally trigger position updates
            if update_count % 100 == 0 {
                let _ = high_performance_gateway.position_manager.update_position(
                    symbol,
                    if update_count % 2 == 0 { Side::Buy } else { Side::Sell },
                    Qty::from_i64(1000 + (i as i64 * 500)),
                    Px::from_i64(current_price),
                ).await;
            }
        }
        
        sleep(Duration::from_millis(10)).await; // 100 Hz update rate
    }
    
    let actual_duration = start_time.elapsed();
    let update_rate = update_count as f64 / actual_duration.as_secs_f64();
    let error_rate = error_count as f64 / (update_count + error_count) as f64 * 100.0;
    
    // Final system check
    let telemetry = high_performance_gateway.telemetry.get_stats().await;
    let risk_metrics = high_performance_gateway.risk_gate.get_metrics();
    let exec_metrics = high_performance_gateway.execution_engine.get_metrics();
    let position_count = high_performance_gateway.position_manager.get_position_count().await;
    let (unrealized_pnl, realized_pnl) = high_performance_gateway.position_manager.get_total_pnl().await;
    
    println!("\n=== System Stability Test Results ===");
    println!("Actual duration: {:?}", actual_duration);
    println!("Successful updates: {}", update_count);
    println!("Errors: {}", error_count);
    println!("Update rate: {:.2} updates/second", update_rate);
    println!("Error rate: {:.4}%", error_rate);
    println!("Telemetry updates: {}", telemetry.orderbook_updates);
    println!("Orders submitted: {}", exec_metrics.orders_submitted);
    println!("Risk checks: {}", risk_metrics.orders_checked);
    println!("Positions: {}", position_count);
    println!("Total P&L: {} unrealized, {} realized", unrealized_pnl, realized_pnl);
    
    // System should remain stable and responsive
    assert!(error_rate < 1.0, "Error rate should be <1%");
    assert!(update_rate > 50.0, "Should maintain >50 updates/second");
    assert!(update_count > 1000, "Should process >1000 updates");
    assert_eq!(high_performance_gateway.get_status(), trading_gateway::GatewayStatus::Running, "Should remain running");
    
    // System should stop gracefully even after stress
    high_performance_gateway.stop().await?;
    assert_eq!(high_performance_gateway.get_status(), trading_gateway::GatewayStatus::Stopped, "Should stop cleanly");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_resource_cleanup_stress(high_performance_gateway: Arc<TradingGateway>) -> Result<()> {
    println!("Starting resource cleanup stress test:");
    
    let cycles = 10;
    
    for cycle in 1..=cycles {
        println!("  Cycle {}/{}", cycle, cycles);
        
        // Start gateway
        high_performance_gateway.start().await?;
        
        // Generate some activity
        let symbol = Symbol(cycle as u32);
        for i in 1..=100 {
            let orderbook = OrderBook::new("CLEANUP_TEST");
            let analytics = MicrostructureAnalytics::new();
            
            let price = 1000000000 + (i * 10000);
            let bid_levels = vec![(Px::from_i64(price), Qty::from_i64(10000), 1)];
            let ask_levels = vec![(Px::from_i64(price + 10000), Qty::from_i64(8000), 1)];
            
            orderbook.load_snapshot(bid_levels, ask_levels);
            analytics.update_trade(Px::from_i64(price + 5000), Qty::from_i64(2000), i % 2 == 0, Ts::now());
            
            high_performance_gateway.process_orderbook_update(symbol, &orderbook, &analytics).await?;
            
            // Create some orders and positions
            if i % 20 == 0 {
                let order = TradingEvent::OrderRequest {
                    id: (cycle * 100 + i) as u64,
                    symbol,
                    side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
                    order_type: OrderType::Market,
                    quantity: Qty::from_i64(5000),
                    price: None,
                    time_in_force: trading_gateway::TimeInForce::Ioc,
                    strategy_id: format!("cleanup_cycle_{}", cycle),
                };
                
                let _ = high_performance_gateway.execution_engine.submit_order(order).await;
            }
        }
        
        // Stop gateway and verify clean shutdown
        high_performance_gateway.stop().await?;
        assert_eq!(high_performance_gateway.get_status(), trading_gateway::GatewayStatus::Stopped);
        
        // Brief pause between cycles
        sleep(Duration::from_millis(100)).await;
    }
    
    println!("  All {} start/stop cycles completed successfully", cycles);
    
    // Final verification
    let final_telemetry = high_performance_gateway.telemetry.get_stats().await;
    println!("Final telemetry updates: {}", final_telemetry.orderbook_updates);
    
    // Should handle all cycles without degradation
    assert!(final_telemetry.orderbook_updates >= (cycles as u64 * 100), "Should accumulate all updates");
    
    Ok(())
}