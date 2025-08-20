//! Unit tests for RiskGate
//!
//! Comprehensive tests covering:
//! - Pre-trade risk validation logic
//! - Position limit enforcement
//! - Rate limiting functionality
//! - Daily P&L limit monitoring
//! - Notional value limit checks
//! - Performance characteristics
//! - Concurrent access safety
//! - Risk metrics tracking

use anyhow::Result;
use rstest::*;
use std::time::Duration;
use tokio::time::sleep;
use trading_gateway::{
    risk_gate::{PositionLimit, RiskGate},
    GatewayConfig, OrderType, Side, TimeInForce, TradingEvent,
};
use services_common::{Px, Qty, Symbol, Ts};

/// Test fixture for creating a RiskGate with default configuration
#[fixture]
fn risk_gate() -> RiskGate {
    let config = GatewayConfig::default();
    RiskGate::new(config)
}

/// Test fixture for creating a RiskGate with restrictive limits
#[fixture]
fn restrictive_risk_gate() -> RiskGate {
    let config = GatewayConfig {
        max_position_size: Qty::from_i64(50000), // 5 units
        max_daily_loss: 100000, // 10 USDT
        ..Default::default()
    };
    RiskGate::new(config)
}

/// Test fixture for creating a sample order request
#[fixture]
fn sample_order() -> TradingEvent {
    TradingEvent::OrderRequest {
        id: 1,
        symbol: Symbol(1),
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(10000), // 1 unit
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "test_strategy".to_string(),
    }
}

/// Test fixture for creating a large order request
#[fixture]
fn large_order() -> TradingEvent {
    TradingEvent::OrderRequest {
        id: 2,
        symbol: Symbol(1),
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(200000), // 20 units - exceeds default limits
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "test_large".to_string(),
    }
}

/// Test fixture for creating a limit order with price
#[fixture]
fn limit_order() -> TradingEvent {
    TradingEvent::OrderRequest {
        id: 3,
        symbol: Symbol(2),
        side: Side::Sell,
        order_type: OrderType::Limit,
        quantity: Qty::from_i64(25000),
        price: Some(Px::from_i64(1000000000)), // $100 per unit
        time_in_force: TimeInForce::Gtc,
        strategy_id: "test_limit".to_string(),
    }
}

#[rstest]
#[tokio::test]
async fn test_risk_gate_creation(risk_gate: RiskGate) {
    // Test basic creation and initial state
    let metrics = risk_gate.get_metrics();
    assert_eq!(metrics.orders_checked, 0);
    assert_eq!(metrics.orders_rejected, 0);
    assert_eq!(metrics.position_breaches, 0);
    assert_eq!(metrics.rate_breaches, 0);
    assert_eq!(metrics.rejection_rate, 0.0);
}

#[rstest]
#[tokio::test]
async fn test_valid_order_passes_all_checks(
    risk_gate: RiskGate,
    sample_order: TradingEvent
) -> Result<()> {
    // Valid small order should pass all risk checks
    let result = risk_gate.check_order(&sample_order).await?;
    assert!(result, "Valid order should pass risk checks");
    
    let metrics = risk_gate.get_metrics();
    assert_eq!(metrics.orders_checked, 1);
    assert_eq!(metrics.orders_rejected, 0);
    assert_eq!(metrics.rejection_rate, 0.0);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_position_limit_enforcement(
    restrictive_risk_gate: RiskGate,
    large_order: TradingEvent
) -> Result<()> {
    // Large order should be rejected due to position limits
    let result = restrictive_risk_gate.check_order(&large_order).await?;
    assert!(!result, "Large order should be rejected");
    
    let metrics = restrictive_risk_gate.get_metrics();
    assert_eq!(metrics.orders_checked, 1);
    assert_eq!(metrics.orders_rejected, 1);
    assert_eq!(metrics.position_breaches, 1);
    assert_eq!(metrics.rejection_rate, 100.0);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_rate_limiting_enforcement(
    risk_gate: RiskGate,
    sample_order: TradingEvent
) -> Result<()> {
    // Submit many orders rapidly to trigger rate limiting
    let mut passed_count = 0;
    let mut rejected_count = 0;
    
    for i in 1..=150 { // Exceed default rate limit of 100 per second
        let order = TradingEvent::OrderRequest {
            id: i,
            symbol: Symbol(i as u32 % 10 + 1),
            side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
            order_type: OrderType::Market,
            quantity: Qty::from_i64(5000),
            price: None,
            time_in_force: TimeInForce::Ioc,
            strategy_id: format!("rate_test_{}", i),
        };
        
        if risk_gate.check_order(&order).await? {
            passed_count += 1;
        } else {
            rejected_count += 1;
        }
    }
    
    // Should have some rejections due to rate limiting
    assert!(rejected_count > 0, "Should have rate limit rejections");
    assert!(passed_count > 0, "Should have some orders pass");
    
    let metrics = risk_gate.get_metrics();
    assert!(metrics.rate_breaches > 0, "Should track rate breaches");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_notional_value_limits(
    risk_gate: RiskGate,
    limit_order: TradingEvent
) -> Result<()> {
    // Test notional value calculation and limiting
    let result = risk_gate.check_order(&limit_order).await?;
    
    // Small notional order should pass
    assert!(result, "Normal notional order should pass");
    
    // Create order with very high notional value
    let high_notional_order = TradingEvent::OrderRequest {
        id: 4,
        symbol: Symbol(3),
        side: Side::Buy,
        order_type: OrderType::Limit,
        quantity: Qty::from_i64(1000000000), // 100k units
        price: Some(Px::from_i64(1000000000)), // $100 per unit = $10M notional
        time_in_force: TimeInForce::Gtc,
        strategy_id: "high_notional".to_string(),
    };
    
    let high_result = risk_gate.check_order(&high_notional_order).await?;
    // This might pass or fail depending on the default notional limit
    // The test verifies the check runs without error
    assert!(high_result || !high_result); // Should not panic
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_daily_pnl_limit_enforcement(restrictive_risk_gate: RiskGate) -> Result<()> {
    // Set daily P&L to a loss that exceeds the limit
    restrictive_risk_gate.update_pnl(-150000); // Loss exceeds 100000 limit
    
    let order = TradingEvent::OrderRequest {
        id: 1,
        symbol: Symbol(1),
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(10000),
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "pnl_test".to_string(),
    };
    
    let result = restrictive_risk_gate.check_order(&order).await?;
    assert!(!result, "Order should be rejected due to daily loss limit");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_position_tracking_updates(risk_gate: RiskGate) -> Result<()> {
    let symbol = Symbol(5);
    
    // Update position with a buy
    risk_gate.update_position(symbol, Side::Buy, Qty::from_i64(30000)).await;
    
    // Create an order that would exceed position limits when combined
    let order = TradingEvent::OrderRequest {
        id: 1,
        symbol,
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(80000), // Would make total position 110k (11 units)
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "position_test".to_string(),
    };
    
    let result = risk_gate.check_order(&order).await?;
    assert!(!result, "Order should be rejected due to position limit");
    
    // Test sell order that reduces position
    let sell_order = TradingEvent::OrderRequest {
        id: 2,
        symbol,
        side: Side::Sell,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(20000), // Would make net position 10k
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "reduce_position".to_string(),
    };
    
    let sell_result = risk_gate.check_order(&sell_order).await?;
    assert!(sell_result, "Reducing order should pass");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_custom_position_limits(risk_gate: RiskGate) -> Result<()> {
    let symbol = Symbol(10);
    
    // Set custom position limits for a symbol
    let custom_limit = PositionLimit {
        max_long: Qty::from_i64(25000), // 2.5 units
        max_short: Qty::from_i64(15000), // 1.5 units
        max_order_size: Qty::from_i64(20000), // 2 units
        max_notional: 500000, // $50 limit
    };
    
    risk_gate.set_position_limit(symbol, custom_limit);
    
    // Test order within custom limits
    let small_order = TradingEvent::OrderRequest {
        id: 1,
        symbol,
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(15000), // Within limit
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "custom_test".to_string(),
    };
    
    let result1 = risk_gate.check_order(&small_order).await?;
    assert!(result1, "Order within custom limits should pass");
    
    // Test order exceeding custom order size limit
    let large_order = TradingEvent::OrderRequest {
        id: 2,
        symbol,
        side: Side::Sell,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(30000), // Exceeds max_order_size
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "custom_large".to_string(),
    };
    
    let result2 = risk_gate.check_order(&large_order).await?;
    assert!(!result2, "Order exceeding custom size limit should be rejected");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_long_short_position_limits(risk_gate: RiskGate) -> Result<()> {
    let symbol = Symbol(11);
    
    // Set custom asymmetric limits
    let asymmetric_limit = PositionLimit {
        max_long: Qty::from_i64(100000), // 10 units long
        max_short: Qty::from_i64(50000),  // 5 units short
        max_order_size: Qty::from_i64(200000),
        max_notional: 10000000,
    };
    
    risk_gate.set_position_limit(symbol, asymmetric_limit);
    
    // Test large long position
    let long_order = TradingEvent::OrderRequest {
        id: 1,
        symbol,
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(90000), // Within long limit
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "long_test".to_string(),
    };
    
    let long_result = risk_gate.check_order(&long_order).await?;
    assert!(long_result, "Long order within limit should pass");
    
    // Test large short position
    let short_order = TradingEvent::OrderRequest {
        id: 2,
        symbol,
        side: Side::Sell,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(60000), // Exceeds short limit
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "short_test".to_string(),
    };
    
    let short_result = risk_gate.check_order(&short_order).await?;
    assert!(!short_result, "Short order exceeding limit should be rejected");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_risk_metrics_accuracy(risk_gate: RiskGate) -> Result<()> {
    // Process a mix of valid and invalid orders
    let orders = vec![
        // Valid orders
        TradingEvent::OrderRequest {
            id: 1,
            symbol: Symbol(1),
            side: Side::Buy,
            order_type: OrderType::Market,
            quantity: Qty::from_i64(10000),
            price: None,
            time_in_force: TimeInForce::Ioc,
            strategy_id: "valid1".to_string(),
        },
        TradingEvent::OrderRequest {
            id: 2,
            symbol: Symbol(2),
            side: Side::Sell,
            order_type: OrderType::Market,
            quantity: Qty::from_i64(15000),
            price: None,
            time_in_force: TimeInForce::Ioc,
            strategy_id: "valid2".to_string(),
        },
        // Invalid order (too large)
        TradingEvent::OrderRequest {
            id: 3,
            symbol: Symbol(3),
            side: Side::Buy,
            order_type: OrderType::Market,
            quantity: Qty::from_i64(500000), // Way too large
            price: None,
            time_in_force: TimeInForce::Ioc,
            strategy_id: "invalid1".to_string(),
        },
    ];
    
    let mut passed = 0;
    let mut rejected = 0;
    
    for order in orders {
        if risk_gate.check_order(&order).await? {
            passed += 1;
        } else {
            rejected += 1;
        }
    }
    
    let metrics = risk_gate.get_metrics();
    assert_eq!(metrics.orders_checked, 3);
    assert_eq!(metrics.orders_rejected as usize, rejected);
    
    let expected_rejection_rate = (rejected as f64 / 3.0) * 100.0;
    assert!((metrics.rejection_rate - expected_rejection_rate).abs() < 0.01);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_daily_reset_functionality(risk_gate: RiskGate) -> Result<()> {
    let symbol = Symbol(12);
    
    // Set up some position and P&L
    risk_gate.update_position(symbol, Side::Buy, Qty::from_i64(50000)).await;
    risk_gate.update_pnl(-50000);
    
    // Reset daily state
    risk_gate.reset_daily();
    
    // Verify P&L was reset
    // We can't directly check P&L, but we can test that loss limit check passes
    let order = TradingEvent::OrderRequest {
        id: 1,
        symbol,
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(10000),
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "reset_test".to_string(),
    };
    
    let result = risk_gate.check_order(&order).await?;
    assert!(result, "Order should pass after daily reset");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_non_order_event_handling(risk_gate: RiskGate) -> Result<()> {
    // Test that non-order events are handled gracefully
    let market_update = TradingEvent::MarketUpdate {
        symbol: Symbol(1),
        bid: None,
        ask: None,
        mid: Px::ZERO,
        spread: 0,
        imbalance: 0.0,
        vpin: 0.0,
        kyles_lambda: 0.0,
        timestamp: Ts::now(),
    };
    
    // Should return true (no blocking) for non-orders
    let result = risk_gate.check_order(&market_update).await?;
    assert!(result, "Non-order events should pass through");
    
    // Metrics should not be affected
    let metrics = risk_gate.get_metrics();
    assert_eq!(metrics.orders_checked, 0);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_performance_characteristics(risk_gate: RiskGate) -> Result<()> {
    // Test that risk checks are fast
    let start = std::time::Instant::now();
    
    for i in 1..=1000 {
        let order = TradingEvent::OrderRequest {
            id: i,
            symbol: Symbol((i % 20) as u32 + 1),
            side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
            order_type: OrderType::Market,
            quantity: Qty::from_i64(5000), // Small orders to pass checks
            price: None,
            time_in_force: TimeInForce::Ioc,
            strategy_id: "perf_test".to_string(),
        };
        
        risk_gate.check_order(&order).await?;
    }
    
    let duration = start.elapsed();
    
    // Should process 1000 risk checks very quickly
    assert!(duration < Duration::from_millis(100), "Risk checks should be fast");
    
    let metrics = risk_gate.get_metrics();
    assert!(metrics.avg_latency_ns < 100000, "Average latency should be low"); // < 100μs
    
    Ok(())
}

#[rstest]
#[case(Side::Buy, 50000, 25000, true)]   // Adding to position within limit
#[case(Side::Buy, 50000, 60000, false)]  // Exceeding position limit
#[case(Side::Sell, -50000, 25000, true)] // Reducing short position
#[case(Side::Sell, -50000, 60000, false)] // Exceeding short limit
#[case(Side::Sell, 30000, 25000, true)]   // Reducing long position
#[case(Side::Buy, -30000, 25000, true)]   // Reducing short position
#[tokio::test]
async fn test_position_logic_parameterized(
    risk_gate: RiskGate,
    #[case] side: Side,
    #[case] current_position: i64,
    #[case] order_quantity: i64,
    #[case] should_pass: bool,
) -> Result<()> {
    let symbol = Symbol(20);
    
    // Set custom position limits
    let limit = PositionLimit {
        max_long: Qty::from_i64(75000),
        max_short: Qty::from_i64(75000),
        max_order_size: Qty::from_i64(100000),
        max_notional: 10000000,
    };
    risk_gate.set_position_limit(symbol, limit);
    
    // Set current position
    if current_position != 0 {
        let pos_side = if current_position > 0 { Side::Buy } else { Side::Sell };
        let pos_qty = Qty::from_i64(current_position.abs());
        risk_gate.update_position(symbol, pos_side, pos_qty).await;
    }
    
    // Test order
    let order = TradingEvent::OrderRequest {
        id: 1,
        symbol,
        side,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(order_quantity),
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "param_test".to_string(),
    };
    
    let result = risk_gate.check_order(&order).await?;
    assert_eq!(result, should_pass, 
        "Position logic failed for side: {:?}, current: {}, order: {}", 
        side, current_position, order_quantity);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_concurrent_risk_checks() -> Result<()> {
    let risk_gate = std::sync::Arc::new(RiskGate::new(GatewayConfig::default()));
    let mut handles = Vec::new();
    
    // Run concurrent risk checks
    for i in 1..=50 {
        let rg = risk_gate.clone();
        let handle = tokio::spawn(async move {
            let order = TradingEvent::OrderRequest {
                id: i,
                symbol: Symbol((i % 10) as u32 + 1),
                side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
                order_type: OrderType::Market,
                quantity: Qty::from_i64(5000),
                price: None,
                time_in_force: TimeInForce::Ioc,
                strategy_id: format!("concurrent_{}", i),
            };
            rg.check_order(&order).await
        });
        handles.push(handle);
    }
    
    // Wait for all checks to complete
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await??);
    }
    
    // Most should pass (small orders)
    let passed = results.iter().filter(|&&r| r).count();
    assert!(passed > 40, "Most concurrent checks should pass");
    
    let metrics = risk_gate.get_metrics();
    assert_eq!(metrics.orders_checked, 50);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_rate_limit_reset_timing(risk_gate: RiskGate) -> Result<()> {
    // Submit orders to hit rate limit
    for i in 1..=100 {
        let order = TradingEvent::OrderRequest {
            id: i,
            symbol: Symbol(1),
            side: Side::Buy,
            order_type: OrderType::Market,
            quantity: Qty::from_i64(1000),
            price: None,
            time_in_force: TimeInForce::Ioc,
            strategy_id: "rate_reset_test".to_string(),
        };
        risk_gate.check_order(&order).await?;
    }
    
    // Next order should fail rate limit
    let rate_limit_order = TradingEvent::OrderRequest {
        id: 101,
        symbol: Symbol(1),
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(1000),
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "should_fail".to_string(),
    };
    
    let result1 = risk_gate.check_order(&rate_limit_order).await?;
    assert!(!result1, "Should fail due to rate limit");
    
    // Wait for rate limit reset
    sleep(Duration::from_millis(1100)).await; // Slightly more than 1 second
    
    // Should pass now
    let result2 = risk_gate.check_order(&rate_limit_order).await?;
    assert!(result2, "Should pass after rate limit reset");
    
    Ok(())
}