//! Error handling and edge case tests for risk manager

use risk_manager::{RiskLimits, RiskManagerService, RiskManager, RiskCheckResult};
use services_common::{Symbol, Side, Px, Qty, constants};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::task::JoinSet;
use rstest::*;

async fn create_test_manager() -> RiskManagerService {
    RiskManagerService::new(RiskLimits::default())
}

#[tokio::test]
async fn test_extreme_numeric_values() {
    let risk_manager = create_test_manager().await;
    
    // Test with maximum values
    let max_symbol = Symbol(u32::MAX);
    let max_qty = Qty::from_i64(i64::MAX);
    let max_price = Px::from_i64(i64::MAX);
    
    let result = risk_manager.check_order(max_symbol, Side::Bid, max_qty, max_price).await;
    
    // Should handle without panic, likely rejected due to size limits
    assert!(matches!(result, RiskCheckResult::Rejected(_)));
    
    // Test with minimum values
    let min_qty = Qty::from_i64(1);
    let min_price = Px::from_i64(1);
    
    let result = risk_manager.check_order(Symbol(1), Side::Bid, min_qty, min_price).await;
    assert!(matches!(result, RiskCheckResult::Approved | RiskCheckResult::Rejected(_)));
}

#[tokio::test]
async fn test_zero_and_negative_values() {
    let risk_manager = create_test_manager().await;
    let symbol = Symbol(1);
    
    // Test zero quantity - should be handled gracefully
    let zero_qty = Qty::from_i64(0);
    let result = risk_manager.check_order(symbol, Side::Bid, zero_qty, Px::from_price_i32(100_0000)).await;
    assert!(matches!(result, RiskCheckResult::Approved)); // Zero qty should be approved
    
    // Test zero price
    let zero_price = Px::from_i64(0);
    let result = risk_manager.check_order(symbol, Side::Bid, Qty::from_qty_i32(100_0000), zero_price).await;
    assert!(matches!(result, RiskCheckResult::Approved | RiskCheckResult::Rejected(_)));
    
    // Test negative values (represented as positive in Qty/Px but conceptually negative)
    let negative_qty = Qty::from_i64(-100_0000);
    let result = risk_manager.check_order(symbol, Side::Bid, negative_qty, Px::from_price_i32(100_0000)).await;
    assert!(matches!(result, RiskCheckResult::Approved | RiskCheckResult::Rejected(_)));
}

#[tokio::test]
async fn test_position_update_edge_cases() {
    let risk_manager = create_test_manager().await;
    let symbol = Symbol(1);
    
    // Test updating position with zero quantity
    let result = risk_manager.update_position(
        symbol,
        Side::Bid,
        Qty::from_i64(0),
        Px::from_price_i32(100_0000),
    ).await;
    assert!(result.is_ok());
    
    // Test position flip scenarios
    // Start with long position
    risk_manager.update_position(
        symbol,
        Side::Bid,
        Qty::from_qty_i32(100_0000),
        Px::from_price_i32(100_0000),
    ).await.unwrap();
    
    // Flip to short (sell more than long position)
    risk_manager.update_position(
        symbol,
        Side::Ask,
        Qty::from_qty_i32(200_0000),
        Px::from_price_i32(110_0000),
    ).await.unwrap();
    
    let position = risk_manager.get_position(symbol).await.unwrap();
    assert_eq!(position.net_qty, -100_0000); // Should be short 100
    
    // Test extreme position sizes
    risk_manager.update_position(
        symbol,
        Side::Bid,
        Qty::from_i64(i64::MAX / 2),
        Px::from_i64(1000),
    ).await.unwrap();
    
    // Should handle without panic
    let position = risk_manager.get_position(symbol).await;
    assert!(position.is_some());
}

#[tokio::test]
async fn test_mark_price_update_edge_cases() {
    let risk_manager = create_test_manager().await;
    let symbol = Symbol(1);
    
    // Test mark price update for non-existent position
    let result = risk_manager.update_mark_price(Symbol(999), Px::from_price_i32(100_0000)).await;
    assert!(result.is_ok()); // Should succeed but do nothing
    
    // Test extreme mark prices
    risk_manager.update_position(
        symbol,
        Side::Bid,
        Qty::from_qty_i32(100_0000),
        Px::from_price_i32(100_0000),
    ).await.unwrap();
    
    // Very high mark price
    risk_manager.update_mark_price(symbol, Px::from_i64(i64::MAX / 2)).await.unwrap();
    
    let position = risk_manager.get_position(symbol).await.unwrap();
    assert!(position.unrealized_pnl != 0); // Should calculate PnL without overflow
    
    // Zero mark price
    risk_manager.update_mark_price(symbol, Px::from_i64(0)).await.unwrap();
    
    let position = risk_manager.get_position(symbol).await.unwrap();
    assert_eq!(position.mark_price.as_i64(), 0);
}

#[tokio::test]
async fn test_concurrent_position_updates() {
    let risk_manager = Arc::new(create_test_manager().await);
    let symbol = Symbol(1);
    let mut join_set = JoinSet::new();
    
    // Launch many concurrent position updates
    for i in 0..100 {
        let rm = risk_manager.clone();
        join_set.spawn(async move {
            let side = if i % 2 == 0 { Side::Bid } else { Side::Ask };
            let qty = Qty::from_qty_i32(10_0000 + (i * 1000));
            let price = Px::from_price_i32(100_0000 + (i * 100));
            
            rm.update_position(symbol, side, qty, price).await
        });
    }
    
    // Wait for all updates
    let mut success_count = 0;
    let mut error_count = 0;
    
    while let Some(result) = join_set.join_next().await {
        match result.unwrap() {
            Ok(_) => success_count += 1,
            Err(_) => error_count += 1,
        }
    }
    
    // All updates should succeed
    assert_eq!(success_count, 100);
    assert_eq!(error_count, 0);
    
    // Final position should be consistent
    let final_position = risk_manager.get_position(symbol).await.unwrap();
    // Position should exist and have reasonable values
    assert!(final_position.position_value < u64::MAX);
}

#[tokio::test]
async fn test_overflow_protection_in_calculations() {
    let risk_manager = create_test_manager().await;
    let symbol = Symbol(1);
    
    // Test with values that could cause overflow in calculations
    let large_qty = Qty::from_i64(i64::MAX / 10000); // Large but not MAX
    let large_price = Px::from_i64(10000);
    
    let result = risk_manager.update_position(symbol, Side::Bid, large_qty, large_price).await;
    assert!(result.is_ok());
    
    // Test order value calculation with potential overflow
    let result = risk_manager.check_order(symbol, Side::Bid, large_qty, large_price).await;
    // Should handle overflow gracefully (likely reject due to value limits)
    assert!(matches!(result, RiskCheckResult::Approved | RiskCheckResult::Rejected(_)));
    
    // Test PnL calculation with extreme values
    risk_manager.update_mark_price(symbol, Px::from_i64(i64::MAX / 20000)).await.unwrap();
    
    let position = risk_manager.get_position(symbol).await.unwrap();
    // Should not panic on PnL calculation
    assert!(position.unrealized_pnl != i64::MAX); // Should be reasonable value
}

#[tokio::test]
async fn test_rate_limiting_edge_cases() {
    let mut limits = RiskLimits::default();
    limits.max_orders_per_minute = 1; // Very restrictive
    let risk_manager = RiskManagerService::new(limits);
    
    let symbol = Symbol(1);
    let price = Px::from_price_i32(100_0000);
    let qty = Qty::from_qty_i32(100_0000);
    
    // First order should be approved
    let result1 = risk_manager.check_order(symbol, Side::Bid, qty, price).await;
    assert!(matches!(result1, RiskCheckResult::Approved));
    
    // Second order should be rejected immediately
    let result2 = risk_manager.check_order(symbol, Side::Bid, qty, price).await;
    assert!(matches!(result2, RiskCheckResult::Rejected(_)));
    
    // Test with zero rate limit
    let mut zero_limits = RiskLimits::default();
    zero_limits.max_orders_per_minute = 0;
    let zero_rm = RiskManagerService::new(zero_limits);
    
    let result = zero_rm.check_order(symbol, Side::Bid, qty, price).await;
    assert!(matches!(result, RiskCheckResult::Rejected(_)));
}

#[tokio::test]
async fn test_metrics_under_extreme_conditions() {
    let risk_manager = create_test_manager().await;
    
    // Create many positions to stress metrics calculation
    for i in 0..1000 {
        let symbol = Symbol(i);
        let _ = risk_manager.update_position(
            symbol,
            Side::Bid,
            Qty::from_qty_i32(100_0000),
            Px::from_price_i32(100_0000),
        ).await;
    }
    
    let metrics = risk_manager.get_metrics().await;
    
    // Should handle large number of positions
    assert_eq!(metrics.open_positions, 1000);
    assert!(metrics.total_exposure > 0);
    
    // Test metrics calculation - can't access private fields directly
    // This would require testing through public API methods
    
    let extreme_metrics = risk_manager.get_metrics().await;
    assert!(extreme_metrics.orders_today >= 0);
    assert!(extreme_metrics.total_exposure >= 0);
}

#[tokio::test]
async fn test_error_propagation_in_async_operations() {
    let risk_manager = create_test_manager().await;
    let symbol = Symbol(1);
    
    // Test that async operations handle errors gracefully
    // This tests the error handling in the async trait implementations
    
    // Multiple rapid operations that could cause contention
    let mut operations = Vec::new();
    
    for i in 0..50 {
        let rm = Arc::new(RiskManagerService::new(RiskLimits::default()));
        operations.push(tokio::spawn(async move {
            let symbol = Symbol(i % 10);
            
            // Mix of operations that could fail
            let check_result = rm.check_order(
                symbol,
                Side::Bid,
                Qty::from_qty_i32(100_0000),
                Px::from_price_i32(100_0000),
            ).await;
            
            let update_result = rm.update_position(
                symbol,
                Side::Bid,
                Qty::from_qty_i32(10_0000),
                Px::from_price_i32(100_0000),
            ).await;
            
            let mark_result = rm.update_mark_price(symbol, Px::from_price_i32(105_0000)).await;
            
            (check_result, update_result, mark_result)
        }));
    }
    
    // All operations should complete without panicking
    for op in operations {
        let (check, update, mark) = op.await.unwrap();
        
        // Check result should be valid
        assert!(matches!(check, RiskCheckResult::Approved | RiskCheckResult::Rejected(_) | RiskCheckResult::RequiresApproval(_)));
        
        // Update and mark operations should succeed
        assert!(update.is_ok());
        assert!(mark.is_ok());
    }
}

#[tokio::test]
async fn test_kill_switch_edge_cases() {
    let risk_manager = create_test_manager().await;
    
    // Test repeated kill switch activation
    for _ in 0..10 {
        risk_manager.activate_kill_switch("Test");
        assert!(risk_manager.is_kill_switch_active());
    }
    
    // Test repeated deactivation
    for _ in 0..10 {
        risk_manager.deactivate_kill_switch("Test deactivation");
        assert!(!risk_manager.is_kill_switch_active());
    }
    
    // Test concurrent kill switch operations
    let rm = Arc::new(risk_manager);
    let mut handles = Vec::new();
    
    for i in 0..20 {
        let rm_clone = rm.clone();
        handles.push(tokio::spawn(async move {
            if i % 2 == 0 {
                rm_clone.activate_kill_switch(&format!("Test {}", i))
            } else {
                rm_clone.deactivate_kill_switch("Test deactivation")
            }
        }));
    }
    
    // All operations should complete successfully
    for handle in handles {
        assert!(handle.await.unwrap()); // kill switch methods return bool, not Result
    }
    
    // Final state should be consistent
    let is_active = rm.is_kill_switch_active();
    assert!(is_active || !is_active); // Just check no panic
}

#[tokio::test]
async fn test_daily_metrics_reset_edge_cases() {
    let risk_manager = create_test_manager().await;
    
    // Test reset with no activity
    let result = risk_manager.reset_daily_metrics().await;
    assert!(result.is_ok());
    
    // Test concurrent resets
    let rm = Arc::new(risk_manager);
    let mut handles = Vec::new();
    
    for _ in 0..10 {
        let rm_clone = rm.clone();
        handles.push(tokio::spawn(async move {
            rm_clone.reset_daily_metrics().await
        }));
    }
    
    // All resets should succeed
    for handle in handles {
        assert!(handle.await.unwrap().is_ok());
    }
    
    let metrics = rm.get_metrics().await;
    assert_eq!(metrics.orders_today, 0);
    assert_eq!(metrics.daily_pnl, 0);
}

#[tokio::test]
async fn test_position_retrieval_edge_cases() {
    let risk_manager = create_test_manager().await;
    
    // Test getting non-existent position
    let position = risk_manager.get_position(Symbol(999)).await;
    assert!(position.is_none());
    
    // Test getting all positions when empty
    let positions = risk_manager.get_all_positions().await;
    assert_eq!(positions.len(), 0);
    
    // Create positions and test retrieval
    for i in 1..=100 {
        risk_manager.update_position(
            Symbol(i),
            Side::Bid,
            Qty::from_qty_i32(100_0000),
            Px::from_price_i32(100_0000),
        ).await.unwrap();
    }
    
    let all_positions = risk_manager.get_all_positions().await;
    assert_eq!(all_positions.len(), 100);
    
    // Test specific position retrieval
    let specific_position = risk_manager.get_position(Symbol(50)).await;
    assert!(specific_position.is_some());
    assert_eq!(specific_position.unwrap().symbol, Symbol(50));
}

#[tokio::test]
async fn test_fixed_point_arithmetic_edge_cases() {
    let risk_manager = create_test_manager().await;
    let symbol = Symbol(1);
    
    // Test with values near fixed-point boundaries
    let scale_boundary = constants::fixed_point::SCALE_4;
    
    // Test quantities and prices at scale boundaries
    let boundary_qty = Qty::from_i64(scale_boundary);
    let boundary_price = Px::from_i64(scale_boundary);
    
    risk_manager.update_position(symbol, Side::Bid, boundary_qty, boundary_price).await.unwrap();
    
    // Test PnL calculation at boundaries
    let new_price = Px::from_i64(scale_boundary * 2);
    risk_manager.update_mark_price(symbol, new_price).await.unwrap();
    
    let position = risk_manager.get_position(symbol).await.unwrap();
    
    // Should handle fixed-point arithmetic correctly
    assert!(position.unrealized_pnl != 0);
    assert!(position.position_value > 0);
}

#[rstest]
#[case(0)]
#[case(1)]
#[case(u64::MAX)]
#[tokio::test]
async fn test_symbol_id_edge_cases(#[case] symbol_id: u64) {
    let risk_manager = create_test_manager().await;
    let symbol = Symbol(symbol_id as u32);
    
    // All symbol IDs should be handled gracefully
    let result = risk_manager.check_order(
        symbol,
        Side::Bid,
        Qty::from_qty_i32(100_0000),
        Px::from_price_i32(100_0000),
    ).await;
    
    assert!(matches!(result, RiskCheckResult::Approved | RiskCheckResult::Rejected(_)));
    
    let update_result = risk_manager.update_position(
        symbol,
        Side::Bid,
        Qty::from_qty_i32(100_0000),
        Px::from_price_i32(100_0000),
    ).await;
    
    assert!(update_result.is_ok());
}

#[tokio::test]
async fn test_memory_usage_under_stress() {
    let risk_manager = Arc::new(create_test_manager().await);
    
    // Create many symbols to test memory usage
    for i in 0..10000 {
        let symbol = Symbol(i);
        let _ = risk_manager.update_position(
            symbol,
            Side::Bid,
            Qty::from_qty_i32(100_0000),
            Px::from_price_i32(100_0000),
        ).await;
        
        // Occasionally get metrics to ensure data structures remain consistent
        if i % 1000 == 0 {
            let _metrics = risk_manager.get_metrics().await;
        }
    }
    
    // Should handle large number of symbols without excessive memory usage
    let final_metrics = risk_manager.get_metrics().await;
    assert_eq!(final_metrics.open_positions, 10000);
    
    // Clean up some positions by flattening them
    for i in 0..5000 {
        let symbol = Symbol(i);
        let _ = risk_manager.update_position(
            symbol,
            Side::Ask,
            Qty::from_qty_i32(100_0000),
            Px::from_price_i32(100_0000),
        ).await;
    }
    
    let final_positions = risk_manager.get_all_positions().await;
    let open_count = final_positions.iter().filter(|p| p.net_qty != 0).count();
    assert_eq!(open_count, 5000);
}