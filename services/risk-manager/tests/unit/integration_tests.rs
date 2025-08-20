//! Integration tests for RiskManagerService with complex scenarios

use risk_manager::{RiskLimits, RiskManagerService, RiskManager, RiskCheckResult};
use services_common::{Symbol, Side, Px, Qty};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time;
use rstest::*;

#[fixture]
fn conservative_limits() -> RiskLimits {
    RiskLimits {
        max_position_size: 1000,
        max_position_value: 100_000,
        max_total_exposure: 500_000,
        max_order_size: 200,
        max_order_value: 20_000,
        max_orders_per_minute: 5,
        max_daily_loss: -10_000,
        max_drawdown_pct: 500, // 5%
        circuit_breaker_threshold: 3,
        circuit_breaker_cooldown: 60,
    }
}

#[fixture]
fn aggressive_limits() -> RiskLimits {
    RiskLimits {
        max_position_size: 10_000,
        max_position_value: 1_000_000,
        max_total_exposure: 10_000_000,
        max_order_size: 2000,
        max_order_value: 200_000,
        max_orders_per_minute: 100,
        max_daily_loss: -100_000,
        max_drawdown_pct: 1500, // 15%
        circuit_breaker_threshold: 10,
        circuit_breaker_cooldown: 300,
    }
}

async fn create_risk_manager(limits: RiskLimits) -> RiskManagerService {
    RiskManagerService::new(limits)
}

#[tokio::test]
async fn test_complete_trading_workflow() {
    let risk_manager = create_risk_manager(RiskLimits::default()).await;
    let symbol = Symbol(1);
    
    // 1. Check initial order
    let order_result = risk_manager.check_order(
        symbol,
        Side::Bid,
        Qty::from_qty_i32(100_0000),
        Px::from_price_i32(50_0000),
    ).await;
    assert!(matches!(order_result, RiskCheckResult::Approved));
    
    // 2. Update position after fill
    risk_manager.update_position(
        symbol,
        Side::Bid,
        Qty::from_qty_i32(100_0000),
        Px::from_price_i32(50_0000),
    ).await.unwrap();
    
    // 3. Verify position was created
    let position = risk_manager.get_position(symbol).await.unwrap();
    assert_eq!(position.net_qty, 100_0000);
    assert_eq!(position.avg_price.as_i64(), 50_0000);
    
    // 4. Update mark price
    risk_manager.update_mark_price(symbol, Px::from_price_i32(55_0000)).await.unwrap();
    
    // 5. Check position has unrealized PnL
    let updated_position = risk_manager.get_position(symbol).await.unwrap();
    assert_ne!(updated_position.unrealized_pnl, 0);
    
    // 6. Place opposite order to reduce position
    let close_result = risk_manager.check_order(
        symbol,
        Side::Ask,
        Qty::from_qty_i32(50_0000),
        Px::from_price_i32(55_0000),
    ).await;
    assert!(matches!(close_result, RiskCheckResult::Approved));
    
    // 7. Update position after partial close
    risk_manager.update_position(
        symbol,
        Side::Ask,
        Qty::from_qty_i32(50_0000),
        Px::from_price_i32(55_0000),
    ).await.unwrap();
    
    // 8. Verify final position
    let final_position = risk_manager.get_position(symbol).await.unwrap();
    assert_eq!(final_position.net_qty, 50_0000);
}

#[rstest]
#[tokio::test]
async fn test_position_limit_enforcement(conservative_limits: RiskLimits) {
    let risk_manager = create_risk_manager(conservative_limits).await;
    let symbol = Symbol(1);
    let price = Px::from_price_i32(100_0000);
    
    // Build up position to near limit (limit is 1000)
    for i in 1..=4 {
        let qty = Qty::from_qty_i32(200_0000);
        
        let check_result = risk_manager.check_order(symbol, Side::Bid, qty, price).await;
        if i <= 3 {
            assert!(matches!(check_result, RiskCheckResult::Approved), "Order {} should be approved", i);
            risk_manager.update_position(symbol, Side::Bid, qty, price).await.unwrap();
        } else {
            // 4th order would bring total to 800 + 200 = 1000, which should be approved
            assert!(matches!(check_result, RiskCheckResult::Approved), "Order {} should be approved", i);
            risk_manager.update_position(symbol, Side::Bid, qty, price).await.unwrap();
        }
    }
    
    // Now try to add more - should be rejected
    let excess_result = risk_manager.check_order(
        symbol,
        Side::Bid,
        Qty::from_qty_i32(100_0000),
        price,
    ).await;
    assert!(matches!(excess_result, RiskCheckResult::Rejected(_)));
}

#[rstest]
#[tokio::test]
async fn test_rate_limiting_with_recovery(conservative_limits: RiskLimits) {
    let risk_manager = create_risk_manager(conservative_limits).await;
    let symbol = Symbol(1);
    let price = Px::from_price_i32(100_0000);
    let qty = Qty::from_qty_i32(50_0000);
    
    // Use up rate limit (5 orders per minute)
    let mut approved_count = 0;
    let mut rejected_count = 0;
    
    for i in 0..8 {
        let result = risk_manager.check_order(symbol, Side::Bid, qty, price).await;
        match result {
            RiskCheckResult::Approved => approved_count += 1,
            RiskCheckResult::Rejected(_) => rejected_count += 1,
            _ => {}
        }
    }
    
    assert_eq!(approved_count, 5, "Should approve exactly 5 orders");
    assert_eq!(rejected_count, 3, "Should reject 3 orders due to rate limit");
    
    // Wait for rate limit window to reset
    time::sleep(Duration::from_secs(61)).await;
    
    // Should be able to place orders again
    let recovery_result = risk_manager.check_order(symbol, Side::Bid, qty, price).await;
    assert!(matches!(recovery_result, RiskCheckResult::Approved));
}

#[tokio::test]
async fn test_multi_symbol_exposure_tracking() {
    let mut limits = RiskLimits::default();
    limits.max_total_exposure = 1_000_000; // $100k total exposure limit
    let risk_manager = create_risk_manager(limits).await;
    
    let symbols = [Symbol(1), Symbol(2), Symbol(3)];
    let price = Px::from_price_i32(100_0000); // $100
    let qty = Qty::from_qty_i32(200_0000);    // 200 units = $20k per position
    
    // Create positions in multiple symbols
    for (i, symbol) in symbols.iter().enumerate() {
        let result = risk_manager.check_order(*symbol, Side::Bid, qty, price).await;
        assert!(matches!(result, RiskCheckResult::Approved), "Order {} should be approved", i);
        
        risk_manager.update_position(*symbol, Side::Bid, qty, price).await.unwrap();
        risk_manager.update_mark_price(*symbol, price).await.unwrap();
    }
    
    // Verify total exposure
    let metrics = risk_manager.get_metrics().await;
    assert_eq!(metrics.open_positions, 3);
    assert!(metrics.total_exposure > 0);
    
    // Try to add position that would exceed total exposure
    let large_qty = Qty::from_qty_i32(5000_0000); // Very large position
    let excess_result = risk_manager.check_order(Symbol(4), Side::Bid, large_qty, price).await;
    
    // Should be rejected due to total exposure limit
    assert!(matches!(excess_result, RiskCheckResult::Rejected(_)));
}

#[tokio::test]
async fn test_kill_switch_blocks_all_orders() {
    let risk_manager = create_risk_manager(RiskLimits::default()).await;
    let symbols = [Symbol(1), Symbol(2), Symbol(3)];
    
    // Verify orders work initially
    let initial_result = risk_manager.check_order(
        symbols[0],
        Side::Bid,
        Qty::from_qty_i32(100_0000),
        Px::from_price_i32(100_0000),
    ).await;
    assert!(matches!(initial_result, RiskCheckResult::Approved));
    
    // Activate kill switch
    risk_manager.activate_kill_switch("Integration test");
    
    // All subsequent orders should be rejected
    for symbol in symbols {
        let result = risk_manager.check_order(
            symbol,
            Side::Bid,
            Qty::from_qty_i32(100_0000),
            Px::from_price_i32(100_0000),
        ).await;
        assert!(matches!(result, RiskCheckResult::Rejected(_)));
    }
    
    // Deactivate kill switch
    risk_manager.deactivate_kill_switch("Integration test deactivation");
    
    // Orders should work again
    let recovery_result = risk_manager.check_order(
        symbols[0],
        Side::Bid,
        Qty::from_qty_i32(100_0000),
        Px::from_price_i32(100_0000),
    ).await;
    assert!(matches!(recovery_result, RiskCheckResult::Approved));
}

#[tokio::test]
async fn test_concurrent_order_processing() {
    let risk_manager = Arc::new(create_risk_manager(RiskLimits::default()).await);
    let mut join_set = JoinSet::new();
    
    // Launch multiple concurrent order checks
    for i in 0..20 {
        let rm = risk_manager.clone();
        join_set.spawn(async move {
            let symbol = Symbol(i % 5); // Use 5 different symbols
            let result = rm.check_order(
                symbol,
                Side::Bid,
                Qty::from_qty_i32(50_0000),
                Px::from_price_i32(100_0000),
            ).await;
            
            // Also update position if approved
            if matches!(result, RiskCheckResult::Approved) {
                let _ = rm.update_position(
                    symbol,
                    Side::Bid,
                    Qty::from_qty_i32(50_0000),
                    Px::from_price_i32(100_0000),
                ).await;
            }
            
            (i, result)
        });
    }
    
    // Collect results
    let mut results = Vec::new();
    while let Some(result) = join_set.join_next().await {
        results.push(result.unwrap());
    }
    
    assert_eq!(results.len(), 20);
    
    // Most should be approved (some might be rejected due to rate limits)
    let approved_count = results
        .iter()
        .filter(|(_, result)| matches!(result, RiskCheckResult::Approved))
        .count();
    
    assert!(approved_count > 10, "Most concurrent orders should be approved");
    
    // Verify final state consistency
    let final_metrics = risk_manager.get_metrics().await;
    assert!(final_metrics.open_positions <= 5); // Max 5 symbols
}

#[tokio::test]
async fn test_daily_pnl_and_drawdown_tracking() {
    let limits = RiskLimits {
        max_daily_loss: -50_000, // $5k daily loss limit
        ..RiskLimits::default()
    };
    let max_daily_loss = limits.max_daily_loss;
    let risk_manager = create_risk_manager(limits).await;
    
    let symbol = Symbol(1);
    let entry_price = Px::from_price_i32(100_0000);
    let qty = Qty::from_qty_i32(100_0000);
    
    // Create initial position
    risk_manager.update_position(symbol, Side::Bid, qty, entry_price).await.unwrap();
    
    // Simulate mark price dropping (creating unrealized loss)
    let mark_prices = [
        Px::from_price_i32(95_0000),  // -5% loss
        Px::from_price_i32(90_0000),  // -10% loss
        Px::from_price_i32(85_0000),  // -15% loss
    ];
    
    for mark_price in mark_prices {
        risk_manager.update_mark_price(symbol, mark_price).await.unwrap();
        
        let position = risk_manager.get_position(symbol).await.unwrap();
        let metrics = risk_manager.get_metrics().await;
        
        // Verify unrealized PnL is being calculated
        assert!(position.unrealized_pnl < 0, "Should have unrealized loss");
        
        // Check if we should require approval for new orders
        if metrics.daily_pnl < max_daily_loss {
            let order_result = risk_manager.check_order(
                Symbol(2),
                Side::Bid,
                Qty::from_qty_i32(50_0000),
                Px::from_price_i32(100_0000),
            ).await;
            
            // Should require approval when daily loss limit exceeded
            assert!(matches!(order_result, RiskCheckResult::RequiresApproval(_)));
        }
    }
}

#[tokio::test]
async fn test_position_averaging_and_scaling() {
    let risk_manager = create_risk_manager(RiskLimits::default()).await;
    let symbol = Symbol(1);
    
    // Build position in multiple tranches with different prices
    let trades = [
        (Qty::from_qty_i32(100_0000), Px::from_price_i32(100_0000)), // 100 @ $100
        (Qty::from_qty_i32(200_0000), Px::from_price_i32(110_0000)), // 200 @ $110
        (Qty::from_qty_i32(100_0000), Px::from_price_i32(90_0000)),  // 100 @ $90
    ];
    
    for (qty, price) in trades {
        risk_manager.update_position(symbol, Side::Bid, qty, price).await.unwrap();
        
        let position = risk_manager.get_position(symbol).await.unwrap();
        assert!(position.net_qty > 0);
        assert!(position.avg_price.as_i64() > 0);
    }
    
    let final_position = risk_manager.get_position(symbol).await.unwrap();
    assert_eq!(final_position.net_qty, 400_0000); // 100 + 200 + 100
    
    // Average price should be weighted average
    // (100*100 + 200*110 + 100*90) / 400 = 41000 / 400 = 102.5
    let expected_avg = ((100_0000 * 100_0000) + (200_0000 * 110_0000) + (100_0000 * 90_0000)) / 400_0000;
    assert_eq!(final_position.avg_price.as_i64(), expected_avg);
}

#[tokio::test]
async fn test_position_flattening_scenarios() {
    let risk_manager = create_risk_manager(RiskLimits::default()).await;
    let symbol = Symbol(1);
    let price = Px::from_price_i32(100_0000);
    
    // Create long position
    risk_manager.update_position(
        symbol,
        Side::Bid,
        Qty::from_qty_i32(500_0000),
        price,
    ).await.unwrap();
    
    let position = risk_manager.get_position(symbol).await.unwrap();
    assert_eq!(position.net_qty, 500_0000);
    
    // Partially close position
    risk_manager.update_position(
        symbol,
        Side::Ask,
        Qty::from_qty_i32(200_0000),
        price,
    ).await.unwrap();
    
    let position = risk_manager.get_position(symbol).await.unwrap();
    assert_eq!(position.net_qty, 300_0000);
    
    // Fully close position
    risk_manager.update_position(
        symbol,
        Side::Ask,
        Qty::from_qty_i32(300_0000),
        price,
    ).await.unwrap();
    
    let position = risk_manager.get_position(symbol).await.unwrap();
    assert_eq!(position.net_qty, 0);
    
    // Go short
    risk_manager.update_position(
        symbol,
        Side::Ask,
        Qty::from_qty_i32(100_0000),
        price,
    ).await.unwrap();
    
    let position = risk_manager.get_position(symbol).await.unwrap();
    assert_eq!(position.net_qty, -100_0000);
}

#[tokio::test]
async fn test_metrics_consistency_under_load() {
    let risk_manager = Arc::new(create_risk_manager(RiskLimits::default()).await);
    let mut join_set = JoinSet::new();
    
    // Perform many concurrent operations
    for i in 0..100 {
        let rm = risk_manager.clone();
        join_set.spawn(async move {
            let symbol = Symbol((i % 10) as u32);
            
            match i % 4 {
                0 => {
                    // Check order
                    let _ = rm.check_order(
                        symbol,
                        Side::Bid,
                        Qty::from_qty_i32(10_0000),
                        Px::from_price_i32(100_0000),
                    ).await;
                }
                1 => {
                    // Update position
                    let _ = rm.update_position(
                        symbol,
                        Side::Bid,
                        Qty::from_qty_i32(10_0000),
                        Px::from_price_i32(100_0000),
                    ).await;
                }
                2 => {
                    // Update mark price
                    let _ = rm.update_mark_price(
                        symbol,
                        Px::from_price_i32(105_0000),
                    ).await;
                }
                _ => {
                    // Get metrics
                    let _ = rm.get_metrics().await;
                }
            }
            
            i
        });
    }
    
    // Wait for all operations
    let mut completed = 0;
    while let Some(result) = join_set.join_next().await {
        result.unwrap();
        completed += 1;
    }
    
    assert_eq!(completed, 100);
    
    // Verify final metrics are consistent
    let final_metrics = risk_manager.get_metrics().await;
    let positions = risk_manager.get_all_positions().await;
    
    // Number of open positions should match between metrics and actual positions
    let actual_open_positions = positions.iter().filter(|p| p.net_qty != 0).count() as u32;
    assert!(actual_open_positions <= final_metrics.open_positions);
}

#[rstest]
#[tokio::test]
async fn test_stress_scenario_with_limits(conservative_limits: RiskLimits) {
    let risk_manager = create_risk_manager(conservative_limits).await;
    let symbol = Symbol(1);
    
    // Try to stress test all limits simultaneously
    let mut order_count = 0;
    let mut approved_count = 0;
    let mut rejected_count = 0;
    
    // Rapid fire orders to hit multiple limits
    for i in 0..50 {
        let qty = Qty::from_qty_i32(50_0000);
        let price = Px::from_price_i32(100_0000 + (i * 1000)); // Varying prices
        
        let result = risk_manager.check_order(symbol, Side::Bid, qty, price).await;
        order_count += 1;
        
        match result {
            RiskCheckResult::Approved => {
                approved_count += 1;
                // Actually fill some orders to build up positions
                if approved_count <= 10 {
                    let _ = risk_manager.update_position(symbol, Side::Bid, qty, price).await;
                }
            }
            RiskCheckResult::Rejected(_) => rejected_count += 1,
            RiskCheckResult::RequiresApproval(_) => {},
        }
        
        // Small delay to allow for rate limit window management
        time::sleep(Duration::from_millis(10)).await;
    }
    
    // Should have hit various limits
    assert!(rejected_count > 0, "Should have some rejected orders due to limits");
    assert!(approved_count > 0, "Should have some approved orders");
    assert_eq!(order_count, 50);
    
    // Final state should be consistent
    let metrics = risk_manager.get_metrics().await;
    assert!(metrics.orders_today > 0);
}

#[tokio::test]
async fn test_daily_metrics_reset_functionality() {
    let risk_manager = create_risk_manager(RiskLimits::default()).await;
    let symbol = Symbol(1);
    
    // Generate some activity
    for _ in 0..5 {
        let _ = risk_manager.check_order(
            symbol,
            Side::Bid,
            Qty::from_qty_i32(100_0000),
            Px::from_price_i32(100_0000),
        ).await;
    }
    
    let initial_metrics = risk_manager.get_metrics().await;
    assert!(initial_metrics.orders_today > 0);
    
    // Reset daily metrics
    risk_manager.reset_daily_metrics().await.unwrap();
    
    let reset_metrics = risk_manager.get_metrics().await;
    assert_eq!(reset_metrics.orders_today, 0);
    assert_eq!(reset_metrics.daily_pnl, 0);
    
    // Should be able to place orders again (rate limit reset)
    for _ in 0..3 {
        let result = risk_manager.check_order(
            symbol,
            Side::Bid,
            Qty::from_qty_i32(100_0000),
            Px::from_price_i32(100_0000),
        ).await;
        assert!(matches!(result, RiskCheckResult::Approved));
    }
}

#[tokio::test]
async fn test_complex_multi_asset_portfolio() {
    let mut limits = RiskLimits::default();
    limits.max_total_exposure = 10_000_000; // $1M total
    let risk_manager = create_risk_manager(limits).await;
    
    // Create a diverse portfolio
    let portfolio = [
        (Symbol(1), Qty::from_qty_i32(1000_0000), Px::from_price_i32(100_0000)), // $100k
        (Symbol(2), Qty::from_qty_i32(500_0000), Px::from_price_i32(200_0000)),  // $100k  
        (Symbol(3), Qty::from_qty_i32(2000_0000), Px::from_price_i32(50_0000)),  // $100k
        (Symbol(4), Qty::from_qty_i32(100_0000), Px::from_price_i32(1000_0000)), // $100k
    ];
    
    // Build portfolio
    for (symbol, qty, price) in portfolio {
        let result = risk_manager.check_order(symbol, Side::Bid, qty, price).await;
        assert!(matches!(result, RiskCheckResult::Approved));
        
        risk_manager.update_position(symbol, Side::Bid, qty, price).await.unwrap();
        risk_manager.update_mark_price(symbol, price).await.unwrap();
    }
    
    // Verify portfolio metrics
    let metrics = risk_manager.get_metrics().await;
    assert_eq!(metrics.open_positions, 4);
    
    let all_positions = risk_manager.get_all_positions().await;
    assert_eq!(all_positions.len(), 4);
    
    // Each position should have correct values
    for position in all_positions {
        assert!(position.net_qty > 0);
        assert!(position.position_value > 0);
        assert_eq!(position.avg_price, position.mark_price);
    }
    
    // Test portfolio-wide operations
    // Update all mark prices simultaneously
    let price_changes = [
        (Symbol(1), Px::from_price_i32(105_0000)), // +5%
        (Symbol(2), Px::from_price_i32(190_0000)), // -5%
        (Symbol(3), Px::from_price_i32(52_0000)),  // +4%
        (Symbol(4), Px::from_price_i32(900_0000)), // -10%
    ];
    
    for (symbol, new_price) in price_changes {
        risk_manager.update_mark_price(symbol, new_price).await.unwrap();
    }
    
    // Verify unrealized PnL is tracked correctly
    let updated_positions = risk_manager.get_all_positions().await;
    let total_unrealized_pnl: i64 = updated_positions.iter().map(|p| p.unrealized_pnl).sum();
    
    // Should have mix of gains and losses
    assert_ne!(total_unrealized_pnl, 0);
}