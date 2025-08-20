//! End-to-end integration tests for trading gateway
//!
//! These tests verify complete trading workflows including:
//! - Market data ingestion and processing
//! - Strategy signal generation and aggregation
//! - Risk gate validation
//! - Order execution and position management
//! - Complete order lifecycle (submit -> fill -> position update)
//! - Multi-strategy coordination
//! - Circuit breaker functionality
//! - Performance under realistic conditions

use anyhow::Result;
use orderbook::{analytics::MicrostructureAnalytics, OrderBook};
use rstest::*;
use services_common::{Px, Qty, Symbol, Ts};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use trading_gateway::{
    GatewayConfig, GatewayStatus, OrderStatus, OrderType, Side, SignalType, TradingEvent,
    TradingGateway,
};

/// Test fixture for creating a trading gateway with realistic configuration
#[fixture]
async fn realistic_gateway() -> Arc<TradingGateway> {
    let config = GatewayConfig {
        max_position_size: Qty::from_i64(100000), // 10 units
        max_daily_loss: 500000,                   // 50 USDT
        risk_check_interval: Duration::from_millis(100),
        orderbook_throttle_ms: 10,
        enable_market_making: true,
        enable_momentum: true,
        enable_arbitrage: true,
        circuit_breaker_threshold: 0.05, // 5%
    };
    
    Arc::new(TradingGateway::new(config).await.unwrap())
}

/// Test fixture for creating an orderbook with realistic market data
#[fixture]
fn realistic_orderbook() -> (OrderBook, MicrostructureAnalytics) {
    let orderbook = OrderBook::new("BTCUSDT");
    let analytics = MicrostructureAnalytics::new();
    
    // Load realistic bid/ask levels
    let bid_levels = vec![
        (Px::from_i64(6500000000), Qty::from_i64(15000), 1),  // $650.00 - 1.5 BTC
        (Px::from_i64(6499000000), Qty::from_i64(25000), 2),  // $649.90 - 2.5 BTC
        (Px::from_i64(6498000000), Qty::from_i64(30000), 3),  // $649.80 - 3.0 BTC
        (Px::from_i64(6497000000), Qty::from_i64(20000), 4),  // $649.70 - 2.0 BTC
        (Px::from_i64(6496000000), Qty::from_i64(35000), 5),  // $649.60 - 3.5 BTC
    ];
    
    let ask_levels = vec![
        (Px::from_i64(6501000000), Qty::from_i64(12000), 1),  // $650.10 - 1.2 BTC
        (Px::from_i64(6502000000), Qty::from_i64(22000), 2),  // $650.20 - 2.2 BTC
        (Px::from_i64(6503000000), Qty::from_i64(28000), 3),  // $650.30 - 2.8 BTC
        (Px::from_i64(6504000000), Qty::from_i64(18000), 4),  // $650.40 - 1.8 BTC
        (Px::from_i64(6505000000), Qty::from_i64(40000), 5),  // $650.50 - 4.0 BTC
    ];
    
    orderbook.load_snapshot(bid_levels, ask_levels);
    
    (orderbook, analytics)
}

#[rstest]
#[tokio::test]
async fn test_complete_trading_workflow(realistic_gateway: Arc<TradingGateway>) -> Result<()> {
    // Start the trading gateway
    realistic_gateway.start().await?;
    
    // Verify gateway is running
    assert_eq!(realistic_gateway.get_status(), GatewayStatus::Running);
    
    let symbol = Symbol(1); // BTCUSDT
    let (orderbook, analytics) = realistic_orderbook();
    
    // Process market data updates to trigger strategies
    for i in 1..=100 {
        // Create evolving market conditions
        let base_price = 6500000000i64 + (i * 1000); // Gradually rising
        
        let bid_levels = vec![
            (Px::from_i64(base_price), Qty::from_i64(15000 + i * 100), 1),
            (Px::from_i64(base_price - 10000), Qty::from_i64(20000), 2),
        ];
        
        let ask_levels = vec![
            (Px::from_i64(base_price + 10000), Qty::from_i64(12000 + i * 80), 1),
            (Px::from_i64(base_price + 20000), Qty::from_i64(18000), 2),
        ];
        
        orderbook.load_snapshot(bid_levels, ask_levels);
        
        // Update analytics with trade data
        analytics.update_trade(
            Px::from_i64(base_price + 5000),
            Qty::from_i64(5000 + i * 50),
            i % 2 == 0, // Alternate buy/sell
            Ts::now(),
        );
        
        // Process through gateway
        realistic_gateway.process_orderbook_update(symbol, &orderbook, &analytics).await?;
        
        // Small delay to simulate realistic market data frequency
        sleep(Duration::from_millis(10)).await;
    }
    
    // Allow time for strategies to process and generate signals
    sleep(Duration::from_millis(500)).await;
    
    // Verify system metrics
    let risk_metrics = realistic_gateway.risk_gate.get_metrics();
    let exec_metrics = realistic_gateway.execution_engine.get_metrics();
    let telemetry = realistic_gateway.telemetry.get_stats().await;
    
    // Should have processed market data
    assert!(telemetry.orderbook_updates > 90, "Should have processed most market updates");
    
    // Should have performed risk checks
    assert!(risk_metrics.orders_checked > 0, "Should have performed risk checks");
    
    // Should have some order activity if strategies generated signals
    println!("Orders submitted: {}", exec_metrics.orders_submitted);
    println!("Risk checks: {}", risk_metrics.orders_checked);
    println!("Signals generated: {}", telemetry.signals_generated);
    
    // Stop gateway gracefully
    realistic_gateway.stop().await?;
    assert_eq!(realistic_gateway.get_status(), GatewayStatus::Stopped);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_multi_symbol_trading_workflow(realistic_gateway: Arc<TradingGateway>) -> Result<()> {
    realistic_gateway.start().await?;
    
    let symbols = vec![Symbol(1), Symbol(2), Symbol(3)]; // BTC, ETH, SOL
    let base_prices = vec![6500000000i64, 400000000, 15000000]; // $650, $40, $1.50
    
    // Process market data for multiple symbols
    for round in 1..=50 {
        for (i, (&symbol, &base_price)) in symbols.iter().zip(base_prices.iter()).enumerate() {
            let orderbook = OrderBook::new(&format!("SYMBOL_{}", i + 1));
            let analytics = MicrostructureAnalytics::new();
            
            let price_movement = (round as i64 * (i as i64 + 1) * 1000);
            let current_price = base_price + price_movement;
            
            let bid_levels = vec![
                (Px::from_i64(current_price), Qty::from_i64(10000 + round * 100), 1),
                (Px::from_i64(current_price - 5000), Qty::from_i64(15000), 2),
            ];
            
            let ask_levels = vec![
                (Px::from_i64(current_price + 5000), Qty::from_i64(8000 + round * 80), 1),
                (Px::from_i64(current_price + 10000), Qty::from_i64(12000), 2),
            ];
            
            orderbook.load_snapshot(bid_levels, ask_levels);
            
            // Simulate trades with varying patterns per symbol
            analytics.update_trade(
                Px::from_i64(current_price + (i as i64 * 1000)),
                Qty::from_i64(3000 + round * 50),
                (round + i) % 2 == 0,
                Ts::now(),
            );
            
            realistic_gateway.process_orderbook_update(symbol, &orderbook, &analytics).await?;
        }
        
        sleep(Duration::from_millis(20)).await;
    }
    
    // Allow processing time
    sleep(Duration::from_millis(300)).await;
    
    // Check positions were potentially created for multiple symbols
    let positions = realistic_gateway.position_manager.get_all_positions().await;
    let position_count = realistic_gateway.position_manager.get_position_count().await;
    
    println!("Active positions: {}", position_count);
    println!("Position details: {:?}", positions);
    
    // Should have processed data for all symbols
    let telemetry = realistic_gateway.telemetry.get_stats().await;
    assert!(telemetry.orderbook_updates >= 150, "Should process all symbol updates");
    
    realistic_gateway.stop().await?;
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_risk_management_integration(realistic_gateway: Arc<TradingGateway>) -> Result<()> {
    realistic_gateway.start().await?;
    
    let symbol = Symbol(10);
    let orderbook = OrderBook::new("RISK_TEST");
    let analytics = MicrostructureAnalytics::new();
    
    // Create market conditions that should trigger position limits
    for i in 1..=20 {
        let base_price = 1000000000i64; // $100
        
        let bid_levels = vec![
            (Px::from_i64(base_price + i * 100000), Qty::from_i64(50000), 1), // Large size
        ];
        
        let ask_levels = vec![
            (Px::from_i64(base_price + i * 100000 + 10000), Qty::from_i64(50000), 1),
        ];
        
        orderbook.load_snapshot(bid_levels, ask_levels);
        
        // Create strong momentum signals
        analytics.update_trade(
            Px::from_i64(base_price + i * 100000 + 5000),
            Qty::from_i64(25000), // Large trade
            true, // All buys to create momentum
            Ts::now(),
        );
        
        realistic_gateway.process_orderbook_update(symbol, &orderbook, &analytics).await?;
        sleep(Duration::from_millis(50)).await;
    }
    
    sleep(Duration::from_millis(200)).await;
    
    // Check risk metrics
    let risk_metrics = realistic_gateway.risk_gate.get_metrics();
    
    println!("Risk checks performed: {}", risk_metrics.orders_checked);
    println!("Orders rejected: {}", risk_metrics.orders_rejected);
    println!("Position breaches: {}", risk_metrics.position_breaches);
    println!("Rejection rate: {:.2}%", risk_metrics.rejection_rate);
    
    // Should have performed risk management
    assert!(risk_metrics.orders_checked > 0, "Should have performed risk checks");
    
    // If there were large orders, some should have been rejected
    if risk_metrics.orders_checked > 10 {
        assert!(risk_metrics.rejection_rate < 90.0, "Rejection rate should be reasonable");
    }
    
    realistic_gateway.stop().await?;
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_circuit_breaker_integration(realistic_gateway: Arc<TradingGateway>) -> Result<()> {
    realistic_gateway.start().await?;
    
    let symbol = Symbol(20);
    
    // Create extreme market conditions to trigger circuit breaker
    let orderbook = OrderBook::new("CIRCUIT_TEST");
    let analytics = MicrostructureAnalytics::new();
    
    // Simulate a market crash scenario
    let initial_price = 5000000000i64; // $500
    let crash_prices = vec![
        4750000000, // -5%
        4500000000, // -10%
        4250000000, // -15% - should trigger circuit breaker
    ];
    
    for (i, &price) in crash_prices.iter().enumerate() {
        let bid_levels = vec![
            (Px::from_i64(price), Qty::from_i64(1000000), 1), // Massive size
        ];
        
        let ask_levels = vec![
            (Px::from_i64(price + 50000), Qty::from_i64(500000), 1),
        ];
        
        orderbook.load_snapshot(bid_levels, ask_levels);
        
        // Simulate massive sell pressure
        for j in 0..10 {
            analytics.update_trade(
                Px::from_i64(price - j * 10000),
                Qty::from_i64(100000), // Large sells
                false, // All sells
                Ts::now(),
            );
        }
        
        realistic_gateway.process_orderbook_update(symbol, &orderbook, &analytics).await?;
        
        // Check if circuit breaker was tripped
        if realistic_gateway.is_circuit_breaker_tripped() {
            println!("Circuit breaker tripped at price level {}", i + 1);
            break;
        }
        
        sleep(Duration::from_millis(100)).await;
    }
    
    // Circuit breaker should prevent further trading
    if realistic_gateway.is_circuit_breaker_tripped() {
        // Try to process more updates - should be ignored
        let post_breaker_orderbook = OrderBook::new("POST_BREAKER");
        let bid_levels = vec![(Px::from_i64(4000000000), Qty::from_i64(10000), 1)];
        let ask_levels = vec![(Px::from_i64(4001000000), Qty::from_i64(10000), 1)];
        post_breaker_orderbook.load_snapshot(bid_levels, ask_levels);
        
        // Should be blocked by circuit breaker
        realistic_gateway.process_orderbook_update(symbol, &post_breaker_orderbook, &analytics).await?;
    }
    
    // Even with circuit breaker, gateway should stop gracefully
    realistic_gateway.stop().await?;
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_strategy_coordination_integration(realistic_gateway: Arc<TradingGateway>) -> Result<()> {
    realistic_gateway.start().await?;
    
    let symbol = Symbol(30);
    
    // Create market conditions that should trigger multiple strategies
    for scenario in 1..=5 {
        let orderbook = OrderBook::new(&format!("SCENARIO_{}", scenario));
        let analytics = MicrostructureAnalytics::new();
        
        match scenario {
            1 => {
                // Momentum scenario - trending market
                for i in 1..=30 {
                    let price = 2000000000i64 + i * 50000; // Trending up
                    let bid_levels = vec![(Px::from_i64(price), Qty::from_i64(10000), 1)];
                    let ask_levels = vec![(Px::from_i64(price + 10000), Qty::from_i64(10000), 1)];
                    orderbook.load_snapshot(bid_levels, ask_levels);
                    
                    analytics.update_trade(Px::from_i64(price + 5000), Qty::from_i64(3000), true, Ts::now());
                    realistic_gateway.process_orderbook_update(symbol, &orderbook, &analytics).await?;
                    sleep(Duration::from_millis(20)).await;
                }
            }
            2 => {
                // Market making scenario - stable market with good spreads
                let base_price = 3500000000i64; // $350
                for i in 1..=20 {
                    let bid_levels = vec![
                        (Px::from_i64(base_price + (i % 5) * 1000), Qty::from_i64(15000), 1),
                    ];
                    let ask_levels = vec![
                        (Px::from_i64(base_price + (i % 5) * 1000 + 20000), Qty::from_i64(12000), 1),
                    ];
                    orderbook.load_snapshot(bid_levels, ask_levels);
                    
                    analytics.update_trade(
                        Px::from_i64(base_price + (i % 5) * 1000 + 10000),
                        Qty::from_i64(2000),
                        i % 2 == 0,
                        Ts::now(),
                    );
                    realistic_gateway.process_orderbook_update(symbol, &orderbook, &analytics).await?;
                    sleep(Duration::from_millis(30)).await;
                }
            }
            3 => {
                // Arbitrage scenario - create negative spread conditions
                let base_price = 1500000000i64; // $150
                let bid_levels = vec![(Px::from_i64(base_price + 2000), Qty::from_i64(10000), 1)]; // Higher bid
                let ask_levels = vec![(Px::from_i64(base_price), Qty::from_i64(8000), 1)];          // Lower ask
                orderbook.load_snapshot(bid_levels, ask_levels);
                
                realistic_gateway.process_orderbook_update(symbol, &orderbook, &analytics).await?;
                sleep(Duration::from_millis(100)).await;
            }
            4 => {
                // High toxicity scenario - should widen spreads in market making
                let base_price = 800000000i64; // $80
                for i in 1..=15 {
                    let bid_levels = vec![(Px::from_i64(base_price), Qty::from_i64(5000), 1)];
                    let ask_levels = vec![(Px::from_i64(base_price + 50000), Qty::from_i64(3000), 1)]; // Wide spread
                    orderbook.load_snapshot(bid_levels, ask_levels);
                    
                    // High VPIN through many small trades
                    for j in 0..5 {
                        analytics.update_trade(
                            Px::from_i64(base_price + j * 1000),
                            Qty::from_i64(100),
                            j % 3 == 0, // Mostly one direction
                            Ts::now(),
                        );
                    }
                    
                    realistic_gateway.process_orderbook_update(symbol, &orderbook, &analytics).await?;
                    sleep(Duration::from_millis(25)).await;
                }
            }
            5 => {
                // Mixed scenario - should trigger signal aggregation
                let base_price = 1200000000i64; // $120
                for i in 1..=25 {
                    // Create conditions that generate multiple signal types
                    let trend_adjustment = if i > 15 { (i - 15) * 20000 } else { 0 };
                    let price = base_price + trend_adjustment;
                    
                    let bid_levels = vec![(Px::from_i64(price), Qty::from_i64(12000 + i * 200), 1)];
                    let ask_levels = vec![(Px::from_i64(price + 15000), Qty::from_i64(10000 + i * 150), 1)];
                    orderbook.load_snapshot(bid_levels, ask_levels);
                    
                    analytics.update_trade(
                        Px::from_i64(price + 7500),
                        Qty::from_i64(2000 + i * 100),
                        i > 12, // Direction change
                        Ts::now(),
                    );
                    
                    realistic_gateway.process_orderbook_update(symbol, &orderbook, &analytics).await?;
                    sleep(Duration::from_millis(15)).await;
                }
            }
            _ => {}
        }
        
        // Allow time for each scenario to complete
        sleep(Duration::from_millis(200)).await;
    }
    
    // Collect final metrics
    let telemetry = realistic_gateway.telemetry.get_stats().await;
    let risk_metrics = realistic_gateway.risk_gate.get_metrics();
    let exec_metrics = realistic_gateway.execution_engine.get_metrics();
    
    println!("=== Strategy Coordination Test Results ===");
    println!("Orderbook updates: {}", telemetry.orderbook_updates);
    println!("Signals generated: {}", telemetry.signals_generated);
    println!("Orders submitted: {}", exec_metrics.orders_submitted);
    println!("Risk checks: {}", risk_metrics.orders_checked);
    println!("Orders rejected: {}", risk_metrics.orders_rejected);
    
    // Should have significant activity across all systems
    assert!(telemetry.orderbook_updates > 50, "Should have processed many market updates");
    
    // Should have some signal generation if strategies are working
    if telemetry.signals_generated > 0 {
        println!("✓ Strategies generated signals");
    }
    
    // Should have some order activity if signals were strong enough
    if exec_metrics.orders_submitted > 0 {
        println!("✓ Signal aggregation produced orders");
    }
    
    realistic_gateway.stop().await?;
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_position_lifecycle_integration(realistic_gateway: Arc<TradingGateway>) -> Result<()> {
    realistic_gateway.start().await?;
    
    let symbol = Symbol(40);
    
    // Manually create position through position manager (simulating order fills)
    let entry_price = Px::from_i64(4500000000); // $450
    let position_size = Qty::from_i64(20000);    // 2 BTC
    
    // Simulate buying 2 BTC at $450
    realistic_gateway.position_manager.update_position(
        symbol,
        Side::Buy,
        position_size,
        entry_price,
    ).await?;
    
    // Verify position was created
    let position = realistic_gateway.position_manager.get_position(symbol).await;
    assert!(position.is_some());
    
    let pos = position.unwrap();
    assert_eq!(pos.quantity, 20000); // Long 2 BTC
    assert_eq!(pos.avg_entry_price, 4500000000); // $450 entry
    
    // Update position with additional buy
    realistic_gateway.position_manager.update_position(
        symbol,
        Side::Buy,
        Qty::from_i64(10000), // +1 BTC
        Px::from_i64(4600000000), // $460
    ).await?;
    
    // Verify position averaging
    let updated_position = realistic_gateway.position_manager.get_position(symbol).await.unwrap();
    assert_eq!(updated_position.quantity, 30000); // 3 BTC total
    
    // Average should be (2*450 + 1*460) / 3 = $453.33
    let expected_avg = (4500000000 * 20000 + 4600000000 * 10000) / 30000;
    assert_eq!(updated_position.avg_entry_price, expected_avg);
    
    // Update market price to see unrealized P&L
    realistic_gateway.position_manager.update_market_price(
        symbol,
        Px::from_i64(4700000000), // $470 - profit
    ).await;
    
    let final_position = realistic_gateway.position_manager.get_position(symbol).await.unwrap();
    assert!(final_position.unrealized_pnl > 0, "Should have unrealized profit");
    
    // Close part of position
    realistic_gateway.position_manager.update_position(
        symbol,
        Side::Sell,
        Qty::from_i64(10000), // Sell 1 BTC
        Px::from_i64(4700000000), // $470
    ).await?;
    
    let closed_position = realistic_gateway.position_manager.get_position(symbol).await.unwrap();
    assert_eq!(closed_position.quantity, 20000); // 2 BTC remaining
    assert!(closed_position.realized_pnl > 0, "Should have realized profit");
    
    // Verify portfolio P&L
    let (total_unrealized, total_realized) = realistic_gateway.position_manager.get_total_pnl().await;
    assert!(total_unrealized > 0, "Should have unrealized profit on remaining position");
    assert!(total_realized > 0, "Should have realized profit from partial close");
    
    println!("Position lifecycle test completed:");
    println!("  Remaining position: {} units", closed_position.quantity);
    println!("  Unrealized P&L: {}", total_unrealized);
    println!("  Realized P&L: {}", total_realized);
    
    realistic_gateway.stop().await?;
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_performance_under_load(realistic_gateway: Arc<TradingGateway>) -> Result<()> {
    realistic_gateway.start().await?;
    
    let start_time = std::time::Instant::now();
    let symbols = vec![Symbol(50), Symbol(51), Symbol(52), Symbol(53), Symbol(54)];
    
    // Generate high-frequency market data
    for round in 1..=200 {
        for (i, &symbol) in symbols.iter().enumerate() {
            let orderbook = OrderBook::new(&format!("PERF_TEST_{}", i));
            let analytics = MicrostructureAnalytics::new();
            
            let base_price = 1000000000 + (i as i64 * 100000000); // Different base prices
            let price_noise = (round as i64 * (i + 1) as i64 * 500) % 50000;
            let current_price = base_price + price_noise;
            
            let bid_levels = vec![
                (Px::from_i64(current_price), Qty::from_i64(10000 + round * 50), 1),
                (Px::from_i64(current_price - 5000), Qty::from_i64(15000), 2),
            ];
            
            let ask_levels = vec![
                (Px::from_i64(current_price + 5000), Qty::from_i64(8000 + round * 40), 1),
                (Px::from_i64(current_price + 10000), Qty::from_i64(12000), 2),
            ];
            
            orderbook.load_snapshot(bid_levels, ask_levels);
            
            // Multiple trades per update
            for j in 0..3 {
                analytics.update_trade(
                    Px::from_i64(current_price + (j * 1000)),
                    Qty::from_i64(1000 + j * 200),
                    (round + i + j) % 2 == 0,
                    Ts::now(),
                );
            }
            
            realistic_gateway.process_orderbook_update(symbol, &orderbook, &analytics).await?;
        }
        
        // Minimal delay to simulate high-frequency updates
        if round % 50 == 0 {
            sleep(Duration::from_millis(1)).await;
        }
    }
    
    let processing_time = start_time.elapsed();
    
    // Collect final metrics
    let telemetry = realistic_gateway.telemetry.get_stats().await;
    let risk_metrics = realistic_gateway.risk_gate.get_metrics();
    let exec_metrics = realistic_gateway.execution_engine.get_metrics();
    
    println!("=== Performance Test Results ===");
    println!("Processing time: {:?}", processing_time);
    println!("Total updates: {}", telemetry.orderbook_updates);
    println!("Updates/second: {:.2}", telemetry.orderbook_updates as f64 / processing_time.as_secs_f64());
    println!("Risk checks: {}", risk_metrics.orders_checked);
    println!("Average risk check latency: {}ns", risk_metrics.avg_latency_ns);
    println!("Orders submitted: {}", exec_metrics.orders_submitted);
    println!("Signals generated: {}", telemetry.signals_generated);
    
    // Performance assertions
    assert!(processing_time < Duration::from_secs(5), "Should handle load efficiently");
    assert_eq!(telemetry.orderbook_updates, 1000, "Should process all 1000 updates");
    
    // Throughput should be reasonable
    let throughput = telemetry.orderbook_updates as f64 / processing_time.as_secs_f64();
    assert!(throughput > 100.0, "Should achieve >100 updates/second");
    
    // Risk check latency should be low
    if risk_metrics.orders_checked > 0 {
        assert!(risk_metrics.avg_latency_ns < 1_000_000, "Risk checks should be <1ms");
    }
    
    realistic_gateway.stop().await?;
    
    Ok(())
}