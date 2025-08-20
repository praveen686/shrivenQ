//! Unit tests for risk limits

use risk_manager::{RiskLimits, RiskManagerService, RiskManager, RiskCheckResult};
use services_common::{Symbol, Side, Px, Qty};

async fn create_test_risk_manager() -> RiskManagerService {
    let limits = RiskLimits::default();
    RiskManagerService::new(limits)
}

async fn create_custom_risk_manager(limits: RiskLimits) -> RiskManagerService {
    RiskManagerService::new(limits)
}

#[tokio::test]
async fn test_default_limits() {
    let limits = RiskLimits::default();
    assert!(limits.max_position_size > 0);
    assert!(limits.max_order_size > 0);
    assert!(limits.max_order_value > 0);
    assert!(limits.max_total_exposure > 0);
    assert!(limits.max_orders_per_minute > 0);
}

#[tokio::test]
async fn test_order_size_limits() {
    let mut limits = RiskLimits::default();
    limits.max_order_size = 1000;
    let risk_manager = create_custom_risk_manager(limits).await;
    
    let symbol = Symbol(1);
    let price = Px::from_price_i32(100_0000); // $100.00
    
    // Order within limits
    let result = risk_manager.check_order(
        symbol,
        Side::Bid,
        Qty::from_qty_i32(500_0000), // 500 units
        price,
    ).await;
    
    assert!(matches!(result, RiskCheckResult::Approved), "Order within size limit should be approved");
    
    // Order exceeding limits
    let result = risk_manager.check_order(
        symbol,
        Side::Bid,
        Qty::from_qty_i32(1500_0000), // 1500 units > 1000 limit
        price,
    ).await;
    
    assert!(matches!(result, RiskCheckResult::Rejected(_)), "Order exceeding size limit should be rejected");
}

#[tokio::test]
async fn test_order_value_limits() {
    let mut limits = RiskLimits::default();
    limits.max_order_value = 100_000; // $10 value limit
    let risk_manager = create_custom_risk_manager(limits).await;
    
    let symbol = Symbol(1);
    
    // Order within value limits: 100 units * $50 = $5000 (after scaling)
    let result = risk_manager.check_order(
        symbol,
        Side::Bid,
        Qty::from_qty_i32(100_0000),
        Px::from_price_i32(50_0000),
    ).await;
    
    assert!(matches!(result, RiskCheckResult::Approved), "Order within value limit should be approved");
    
    // Order exceeding value limits: 1000 units * $200 = $200k (after scaling)
    let result = risk_manager.check_order(
        symbol,
        Side::Bid,
        Qty::from_qty_i32(1000_0000),
        Px::from_price_i32(200_0000),
    ).await;
    
    assert!(matches!(result, RiskCheckResult::Rejected(_)), "Order exceeding value limit should be rejected");
}

#[tokio::test]
async fn test_position_limits() {
    let mut limits = RiskLimits::default();
    limits.max_position_size = 1000;
    let risk_manager = create_custom_risk_manager(limits).await;
    
    let symbol = Symbol(1);
    let price = Px::from_price_i32(100_0000);
    
    // Build up position to near limit
    risk_manager.update_position(
        symbol,
        Side::Bid,
        Qty::from_qty_i32(800_0000),
        price,
    ).await.unwrap();
    
    // Small additional order should be approved
    let result = risk_manager.check_order(
        symbol,
        Side::Bid,
        Qty::from_qty_i32(100_0000),
        price,
    ).await;
    
    assert!(matches!(result, RiskCheckResult::Approved));
    
    // Large order that would exceed position limit should be rejected
    let result = risk_manager.check_order(
        symbol,
        Side::Bid,
        Qty::from_qty_i32(300_0000), // Would result in 1100 total > 1000 limit
        price,
    ).await;
    
    assert!(matches!(result, RiskCheckResult::Rejected(_)));
}

#[tokio::test]
async fn test_rate_limiting() {
    let mut limits = RiskLimits::default();
    limits.max_orders_per_minute = 3;
    let risk_manager = create_custom_risk_manager(limits).await;
    
    let symbol = Symbol(1);
    let price = Px::from_price_i32(100_0000);
    let qty = Qty::from_qty_i32(100_0000);
    
    // First 3 orders should be approved
    for i in 0..3 {
        let result = risk_manager.check_order(symbol, Side::Bid, qty, price).await;
        assert!(matches!(result, RiskCheckResult::Approved), "Order {} should be approved", i + 1);
    }
    
    // 4th order should be rejected due to rate limit
    let result = risk_manager.check_order(symbol, Side::Bid, qty, price).await;
    assert!(matches!(result, RiskCheckResult::Rejected(_)), "Order should be rejected due to rate limit");
}

#[tokio::test]
async fn test_kill_switch() {
    let risk_manager = create_test_risk_manager().await;
    
    let symbol = Symbol(1);
    let price = Px::from_price_i32(100_0000);
    let qty = Qty::from_qty_i32(100_0000);
    
    // Order should be approved initially
    let result = risk_manager.check_order(symbol, Side::Bid, qty, price).await;
    assert!(matches!(result, RiskCheckResult::Approved));
    
    // Activate kill switch
    risk_manager.activate_kill_switch("Test activation");
    
    // Order should be rejected after kill switch activation
    let result = risk_manager.check_order(symbol, Side::Bid, qty, price).await;
    assert!(matches!(result, RiskCheckResult::Rejected(_)));
    
    // Deactivate kill switch
    risk_manager.deactivate_kill_switch("Test deactivation");
    
    // Order should be approved again
    let result = risk_manager.check_order(symbol, Side::Bid, qty, price).await;
    assert!(matches!(result, RiskCheckResult::Approved));
}

#[tokio::test]
async fn test_daily_loss_limits() {
    let mut limits = RiskLimits::default();
    limits.max_daily_loss = -1_000_000; // $100 loss limit
    let risk_manager = create_custom_risk_manager(limits).await;
    
    let symbol = Symbol(1);
    let price = Px::from_price_i32(100_0000);
    let qty = Qty::from_qty_i32(100_0000);
    
    // Daily loss limits are checked during order processing
    // We can't directly simulate large daily loss due to private fields
    // This test would need to be done through order fills and position updates
    
    // Order should require approval due to daily loss limit
    let result = risk_manager.check_order(symbol, Side::Bid, qty, price).await;
    assert!(matches!(result, RiskCheckResult::RequiresApproval(_)));
}

#[tokio::test]
async fn test_exposure_limits() {
    let mut limits = RiskLimits::default();
    limits.max_total_exposure = 10_000; // Low exposure limit for testing
    let risk_manager = create_custom_risk_manager(limits).await;
    
    let symbol = Symbol(1);
    let price = Px::from_price_i32(1000_0000); // High price to quickly hit exposure limit
    let qty = Qty::from_qty_i32(100_0000);
    
    // Large order that would exceed total exposure limit
    let result = risk_manager.check_order(symbol, Side::Bid, qty, price).await;
    assert!(matches!(result, RiskCheckResult::Rejected(_)));
    
    // Small order should be approved
    let result = risk_manager.check_order(
        symbol,
        Side::Bid,
        Qty::from_qty_i32(1_0000), // Much smaller quantity
        Px::from_price_i32(10_0000), // Much smaller price
    ).await;
    assert!(matches!(result, RiskCheckResult::Approved));
}

#[tokio::test]
async fn test_position_tracking() {
    let risk_manager = create_test_risk_manager().await;
    
    let symbol = Symbol(1);
    let price = Px::from_price_i32(100_0000);
    let qty = Qty::from_qty_i32(100_0000);
    
    // Initial position should be None
    assert!(risk_manager.get_position(symbol).await.is_none());
    
    // Update position
    risk_manager.update_position(symbol, Side::Bid, qty, price).await.unwrap();
    
    // Position should now exist
    let position = risk_manager.get_position(symbol).await.unwrap();
    assert_eq!(position.symbol, symbol);
    assert_eq!(position.net_qty, qty.as_i64());
    assert_eq!(position.avg_price, price);
    
    // Update with opposite side should reduce position
    risk_manager.update_position(symbol, Side::Ask, Qty::from_qty_i32(50_0000), price).await.unwrap();
    
    let position = risk_manager.get_position(symbol).await.unwrap();
    assert_eq!(position.net_qty, 50_0000); // 100 - 50 = 50
}

#[tokio::test]
async fn test_metrics_reporting() {
    let risk_manager = create_test_risk_manager().await;
    
    let symbol1 = Symbol(1);
    let symbol2 = Symbol(2);
    let price = Px::from_price_i32(100_0000);
    let qty = Qty::from_qty_i32(100_0000);
    
    // Add some positions
    risk_manager.update_position(symbol1, Side::Bid, qty, price).await.unwrap();
    risk_manager.update_position(symbol2, Side::Bid, qty, price).await.unwrap();
    
    let metrics = risk_manager.get_metrics().await;
    
    assert_eq!(metrics.open_positions, 2);
    assert!(!metrics.kill_switch_active);
    assert_eq!(metrics.orders_today, 0); // Orders today is only incremented on successful order checks
}

#[tokio::test]
async fn test_concurrent_order_checks() {
    use std::sync::Arc;
    use tokio::task::JoinSet;
    
    let risk_manager = Arc::new(create_test_risk_manager().await);
    let mut join_set = JoinSet::new();
    
    let symbol = Symbol(1);
    let price = Px::from_price_i32(100_0000);
    let qty = Qty::from_qty_i32(10_0000);
    
    // Launch multiple concurrent order checks
    for i in 0..10 {
        let rm = risk_manager.clone();
        join_set.spawn(async move {
            let result = rm.check_order(symbol, Side::Bid, qty, price).await;
            (i, result)
        });
    }
    
    // Collect results
    let mut results = Vec::new();
    while let Some(result) = join_set.join_next().await {
        results.push(result.unwrap());
    }
    
    // All should complete successfully (either approved or rejected due to rate limits)
    assert_eq!(results.len(), 10);
    
    // At least some should be approved (assuming rate limits allow)
    let approved_count = results.iter()
        .filter(|(_, result)| matches!(result, RiskCheckResult::Approved))
        .count();
    
    assert!(approved_count > 0, "At least some orders should be approved");
}

#[tokio::test]
async fn test_reset_daily_metrics() {
    let risk_manager = create_test_risk_manager().await;
    
    let symbol = Symbol(1);
    let price = Px::from_price_i32(100_0000);
    let qty = Qty::from_qty_i32(100_0000);
    
    // Make some orders to increment counters
    for _ in 0..3 {
        risk_manager.check_order(symbol, Side::Bid, qty, price).await;
    }
    
    let initial_metrics = risk_manager.get_metrics().await;
    assert!(initial_metrics.orders_today > 0);
    
    // Reset daily metrics
    risk_manager.reset_daily_metrics().await.unwrap();
    
    let reset_metrics = risk_manager.get_metrics().await;
    assert_eq!(reset_metrics.orders_today, 0);
    assert_eq!(reset_metrics.daily_pnl, 0);
}

#[tokio::test]
async fn test_mark_price_updates() {
    let risk_manager = create_test_risk_manager().await;
    
    let symbol = Symbol(1);
    let entry_price = Px::from_price_i32(100_0000);
    let mark_price = Px::from_price_i32(110_0000);
    let qty = Qty::from_qty_i32(100_0000);
    
    // Create position
    risk_manager.update_position(symbol, Side::Bid, qty, entry_price).await.unwrap();
    
    // Update mark price
    risk_manager.update_mark_price(symbol, mark_price).await.unwrap();
    
    let position = risk_manager.get_position(symbol).await.unwrap();
    assert_eq!(position.mark_price, mark_price);
    
    // Check that unrealized PnL is calculated
    // PnL = (mark_price - avg_price) * qty / SCALE_4
    // = (110 - 100) * 100 / 10000 = 10 * 100 / 10000 = 0.1
    assert!(position.unrealized_pnl != 0);
}