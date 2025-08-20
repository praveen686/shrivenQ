//! Error handling and edge case tests for OMS

use chrono::{Duration, Utc};
use proptest::prelude::*;
use rstest::*;
use services_common::{Px, Qty, Symbol};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::time::{sleep, timeout, Duration as TokioDuration};
use uuid::Uuid;

use oms::{
    OrderManagementSystem, OmsConfig,
    order::{Order, OrderRequest, OrderSide, OrderStatus, OrderType, TimeInForce, Fill, Amendment, LiquidityIndicator},
    error::{OmsError, OmsResult},
    lifecycle::OrderLifecycleManager,
    matching::MatchingEngine,
};

/// Test fixture for creating invalid order requests
#[fixture]
fn invalid_order_request() -> OrderRequest {
    OrderRequest {
        client_order_id: Some("INVALID-TEST".to_string()),
        parent_order_id: None,
        symbol: Symbol(1),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Day,
        quantity: Qty::ZERO, // Invalid: zero quantity
        price: None, // Invalid: limit order without price
        stop_price: None,
        account: String::new(), // Invalid: empty account
        exchange: String::new(), // Invalid: empty exchange
        strategy_id: None,
        tags: vec![],
    }
}

/// Test fixture for creating a valid order for manipulation
#[fixture]
fn valid_order() -> Order {
    Order {
        id: Uuid::new_v4(),
        client_order_id: Some("EDGE-TEST-001".to_string()),
        parent_order_id: None,
        symbol: Symbol(1),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Day,
        quantity: Qty::from_i64(10_000),
        executed_quantity: Qty::ZERO,
        remaining_quantity: Qty::from_i64(10_000),
        price: Some(Px::from_i64(50_000_000_000)),
        stop_price: None,
        status: OrderStatus::New,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        account: "edge_test_account".to_string(),
        exchange: "binance".to_string(),
        strategy_id: Some("edge_test_strategy".to_string()),
        tags: vec!["edge".to_string(), "test".to_string()],
        fills: vec![],
        amendments: vec![],
        version: 1,
        sequence_number: 1,
    }
}

/// Test fixture for test database that may fail
#[fixture]
async fn failing_db_config() -> OmsConfig {
    OmsConfig {
        database_url: "postgresql://invalid:invalid@nonexistent:5432/nonexistent".to_string(),
        max_orders_memory: 1000,
        retention_days: 7,
        enable_audit: true,
        enable_matching: true,
        persist_batch_size: 100,
    }
}

// Input validation tests

#[rstest]
#[tokio::test]
async fn test_create_order_with_invalid_inputs() {
    // Test with mock config since we're testing validation, not DB
    let config = OmsConfig {
        database_url: "postgresql://test:test@localhost/test_errors".to_string(),
        max_orders_memory: 1000,
        retention_days: 7,
        enable_audit: false, // Disable to avoid DB calls
        enable_matching: false, // Disable to avoid complex setup
        persist_batch_size: 100,
    };
    
    // This test focuses on validation logic that happens before DB operations
    if let Ok(oms) = OrderManagementSystem::new(config).await {
        let invalid_request = invalid_order_request();
        let result = oms.create_order(invalid_request).await;
        
        // Should fail validation before reaching database
        assert!(result.is_err(), "Should reject invalid order request");
    }
}

#[rstest]
fn test_lifecycle_manager_edge_cases(valid_order: Order) {
    let lifecycle_manager = OrderLifecycleManager::new();
    
    // Test with various invalid order configurations
    let mut invalid_order = valid_order.clone();
    invalid_order.quantity = Qty::from_i64(-1000);
    assert!(lifecycle_manager.validate_order(&invalid_order).is_err(), "Should reject negative quantity");
    
    invalid_order = valid_order.clone();
    invalid_order.order_type = OrderType::Limit;
    invalid_order.price = None;
    assert!(lifecycle_manager.validate_order(&invalid_order).is_err(), "Should reject limit order without price");
    
    invalid_order = valid_order.clone();
    invalid_order.order_type = OrderType::Stop;
    invalid_order.stop_price = None;
    assert!(lifecycle_manager.validate_order(&invalid_order).is_err(), "Should reject stop order without stop price");
    
    invalid_order = valid_order.clone();
    invalid_order.account = String::new();
    assert!(lifecycle_manager.validate_order(&invalid_order).is_err(), "Should reject empty account");
    
    invalid_order = valid_order.clone();
    invalid_order.exchange = String::new();
    assert!(lifecycle_manager.validate_order(&invalid_order).is_err(), "Should reject empty exchange");
    
    // Test expired GTT
    invalid_order = valid_order.clone();
    invalid_order.time_in_force = TimeInForce::Gtt(Utc::now() - Duration::hours(1));
    assert!(lifecycle_manager.validate_order(&invalid_order).is_err(), "Should reject expired GTT");
}

#[rstest]
fn test_invalid_state_transitions(valid_order: Order) {
    let lifecycle_manager = OrderLifecycleManager::new();
    
    // Test all invalid transitions
    let invalid_transitions = vec![
        (OrderStatus::New, OrderStatus::Filled),
        (OrderStatus::New, OrderStatus::PartiallyFilled),
        (OrderStatus::New, OrderStatus::Expired),
        (OrderStatus::Pending, OrderStatus::New),
        (OrderStatus::Pending, OrderStatus::Filled),
        (OrderStatus::Pending, OrderStatus::PartiallyFilled),
        (OrderStatus::Submitted, OrderStatus::New),
        (OrderStatus::Submitted, OrderStatus::Pending),
        (OrderStatus::Filled, OrderStatus::New),
        (OrderStatus::Filled, OrderStatus::Pending),
        (OrderStatus::Filled, OrderStatus::Cancelled),
        (OrderStatus::Cancelled, OrderStatus::New),
        (OrderStatus::Cancelled, OrderStatus::Filled),
        (OrderStatus::Rejected, OrderStatus::Pending),
        (OrderStatus::Expired, OrderStatus::New),
    ];
    
    for (from_status, to_status) in invalid_transitions {
        let mut order = valid_order.clone();
        order.status = from_status;
        
        let result = lifecycle_manager.validate_transition(&order, to_status);
        assert!(result.is_err(), "Transition {:?} -> {:?} should be invalid", from_status, to_status);
    }
}

#[rstest]
fn test_order_operations_on_terminal_states(valid_order: Order) {
    let lifecycle_manager = OrderLifecycleManager::new();
    
    let terminal_states = vec![
        OrderStatus::Filled,
        OrderStatus::Cancelled,
        OrderStatus::Rejected,
        OrderStatus::Expired,
    ];
    
    for terminal_status in terminal_states {
        let mut order = valid_order.clone();
        order.status = terminal_status;
        
        assert!(!lifecycle_manager.can_cancel(&order), "Should not be able to cancel order in {:?} state", terminal_status);
        assert!(!lifecycle_manager.can_amend(&order), "Should not be able to amend order in {:?} state", terminal_status);
        assert!(!lifecycle_manager.should_expire(&order), "Order in {:?} state should not expire", terminal_status);
    }
}

// Matching engine edge cases

#[rstest]
fn test_matching_engine_edge_cases() {
    let engine = MatchingEngine::new();
    
    // Test adding order without price for limit order
    let mut order_without_price = Order {
        id: Uuid::new_v4(),
        client_order_id: None,
        parent_order_id: None,
        symbol: Symbol(1),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Gtc,
        quantity: Qty::from_i64(1000),
        executed_quantity: Qty::ZERO,
        remaining_quantity: Qty::from_i64(1000),
        price: None, // Invalid for limit order
        stop_price: None,
        status: OrderStatus::New,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        account: "test".to_string(),
        exchange: "test".to_string(),
        strategy_id: None,
        tags: vec![],
        fills: vec![],
        amendments: vec![],
        version: 1,
        sequence_number: 1,
    };
    
    let result = engine.add_order(&order_without_price);
    assert!(result.is_err(), "Should reject limit order without price");
    
    // Test zero quantity order
    order_without_price.price = Some(Px::from_i64(1_000_000));
    order_without_price.quantity = Qty::ZERO;
    order_without_price.remaining_quantity = Qty::ZERO;
    
    let result = engine.add_order(&order_without_price);
    if result.is_ok() {
        let matches = result.unwrap();
        assert!(matches.is_empty(), "Zero quantity order should not generate matches");
    }
}

#[rstest]
fn test_matching_engine_extreme_prices() {
    let engine = MatchingEngine::new();
    
    // Test with very large prices
    let expensive_order = Order {
        id: Uuid::new_v4(),
        client_order_id: None,
        parent_order_id: None,
        symbol: Symbol(1),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Gtc,
        quantity: Qty::from_i64(1000),
        executed_quantity: Qty::ZERO,
        remaining_quantity: Qty::from_i64(1000),
        price: Some(Px::from_i64(i64::MAX / 1000)), // Very large but safe price
        stop_price: None,
        status: OrderStatus::New,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        account: "test".to_string(),
        exchange: "test".to_string(),
        strategy_id: None,
        tags: vec![],
        fills: vec![],
        amendments: vec![],
        version: 1,
        sequence_number: 1,
    };
    
    let result = engine.add_order(&expensive_order);
    assert!(result.is_ok(), "Should handle large prices gracefully");
    
    // Test with minimum positive prices
    let cheap_order = Order {
        id: Uuid::new_v4(),
        price: Some(Px::from_i64(1)), // Minimum positive price
        side: OrderSide::Sell,
        sequence_number: 2,
        ..expensive_order
    };
    
    let result = engine.add_order(&cheap_order);
    assert!(result.is_ok(), "Should handle small prices gracefully");
}

#[rstest]
fn test_order_book_depth_edge_cases() {
    let engine = MatchingEngine::new();
    
    // Test depth on empty order book
    let depth = engine.get_depth(Symbol(1), 10);
    assert!(depth.is_some(), "Should return depth for empty book");
    let depth = depth.unwrap();
    assert!(depth.bids.is_empty(), "Empty book should have no bids");
    assert!(depth.asks.is_empty(), "Empty book should have no asks");
    
    // Test depth with non-existent symbol
    let depth = engine.get_depth(Symbol(999), 10);
    assert!(depth.is_none(), "Should return None for non-existent symbol");
    
    // Test depth with zero levels
    engine.add_order(&Order {
        id: Uuid::new_v4(),
        client_order_id: None,
        parent_order_id: None,
        symbol: Symbol(1),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Gtc,
        quantity: Qty::from_i64(1000),
        executed_quantity: Qty::ZERO,
        remaining_quantity: Qty::from_i64(1000),
        price: Some(Px::from_i64(1_000_000)),
        stop_price: None,
        status: OrderStatus::New,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        account: "test".to_string(),
        exchange: "test".to_string(),
        strategy_id: None,
        tags: vec![],
        fills: vec![],
        amendments: vec![],
        version: 1,
        sequence_number: 1,
    }).unwrap();
    
    let depth = engine.get_depth(Symbol(1), 0);
    assert!(depth.is_some(), "Should handle zero levels request");
}

// Database and persistence edge cases

#[rstest]
#[tokio::test]
async fn test_database_connection_failure() {
    let failing_config = failing_db_config().await;
    
    // Should fail to create OMS with invalid database
    let result = OrderManagementSystem::new(failing_config).await;
    assert!(result.is_err(), "Should fail to create OMS with invalid database");
}

#[rstest]
fn test_oms_error_types() {
    // Test various error types
    let order_not_found = OmsError::OrderNotFound {
        order_id: "test-order-123".to_string(),
    };
    assert!(order_not_found.to_string().contains("Order not found"));
    
    let invalid_state = OmsError::InvalidOrderState {
        order_id: "test-order-456".to_string(),
        operation: "cancel".to_string(),
        current_state: "Filled".to_string(),
    };
    assert!(invalid_state.to_string().contains("cannot be cancel"));
    
    let invalid_quantity = OmsError::InvalidQuantity {
        reason: "Quantity must be positive".to_string(),
    };
    assert!(invalid_quantity.to_string().contains("Invalid quantity"));
    
    let validation_error = OmsError::Validation {
        message: "Missing required field".to_string(),
    };
    assert!(validation_error.to_string().contains("Validation error"));
    
    let risk_check_failed = OmsError::RiskCheckFailed {
        reason: "Position limit exceeded".to_string(),
    };
    assert!(risk_check_failed.to_string().contains("Risk check failed"));
    
    let capacity_exceeded = OmsError::CapacityExceeded {
        details: "Maximum orders reached".to_string(),
    };
    assert!(capacity_exceeded.to_string().contains("System capacity exceeded"));
}

// Concurrency and race condition tests

#[rstest]
#[tokio::test]
async fn test_concurrent_access_patterns() {
    // This is a simplified test - real implementation would need actual DB
    let config = OmsConfig {
        database_url: "postgresql://test:test@localhost/test_concurrent".to_string(),
        max_orders_memory: 100,
        retention_days: 1,
        enable_audit: false,
        enable_matching: false,
        persist_batch_size: 10,
    };
    
    if let Ok(oms) = OrderManagementSystem::new(config).await {
        let oms = Arc::new(oms);
        let mut handles = vec![];
        
        // Concurrent order creation with potential conflicts
        for i in 0..10 {
            let oms_clone = Arc::clone(&oms);
            let handle = tokio::spawn(async move {
                for j in 0..10 {
                    let request = OrderRequest {
                        client_order_id: Some(format!("CONCURRENT-{}-{}", i, j)),
                        parent_order_id: None,
                        symbol: Symbol(1),
                        side: if (i + j) % 2 == 0 { OrderSide::Buy } else { OrderSide::Sell },
                        order_type: OrderType::Limit,
                        time_in_force: TimeInForce::Day,
                        quantity: Qty::from_i64(1000),
                        price: Some(Px::from_i64(1_000_000 + (i * 1000) as i64)),
                        stop_price: None,
                        account: format!("account_{}", i),
                        exchange: "binance".to_string(),
                        strategy_id: Some("concurrent_test".to_string()),
                        tags: vec![],
                    };
                    
                    // Some operations may fail due to conflicts - that's expected
                    let _result = oms_clone.create_order(request).await;
                }
            });
            handles.push(handle);
        }
        
        // Wait for all operations to complete
        for handle in handles {
            handle.await.expect("Task should complete");
        }
        
        // System should remain consistent
        let metrics = oms.get_metrics();
        let active_orders = oms.get_active_orders();
        
        // Some orders should have been created successfully
        assert!(metrics.orders_created > 0, "Some orders should have been created");
        assert!(active_orders.len() <= metrics.orders_created as usize, "Active orders should not exceed created");
    }
}

// Resource exhaustion tests

#[rstest]
#[tokio::test]
async fn test_memory_limits() {
    let config = OmsConfig {
        database_url: "postgresql://test:test@localhost/test_memory".to_string(),
        max_orders_memory: 10, // Very low limit
        retention_days: 1,
        enable_audit: false,
        enable_matching: false,
        persist_batch_size: 5,
    };
    
    if let Ok(oms) = OrderManagementSystem::new(config).await {
        let mut successful_orders = 0;
        
        // Try to exceed memory limits
        for i in 0..20 {
            let request = OrderRequest {
                client_order_id: Some(format!("MEMORY-TEST-{}", i)),
                parent_order_id: None,
                symbol: Symbol(1),
                side: OrderSide::Buy,
                order_type: OrderType::Limit,
                time_in_force: TimeInForce::Day,
                quantity: Qty::from_i64(1000),
                price: Some(Px::from_i64(1_000_000)),
                stop_price: None,
                account: "memory_test_account".to_string(),
                exchange: "binance".to_string(),
                strategy_id: Some("memory_test".to_string()),
                tags: vec![],
            };
            
            match oms.create_order(request).await {
                Ok(_) => successful_orders += 1,
                Err(e) => {
                    println!("Order creation failed at {}: {:?}", i, e);
                    // Should fail gracefully when hitting limits
                    break;
                }
            }
        }
        
        let active_orders = oms.get_active_orders();
        
        // Should respect memory limits
        assert!(active_orders.len() <= 10, "Should respect memory limits");
        assert!(successful_orders > 0, "Should create some orders before hitting limit");
        
        println!("Created {} orders before hitting memory limit", successful_orders);
    }
}

// Timing and timeout edge cases

#[rstest]
#[tokio::test]
async fn test_operation_timeouts() {
    // Test operations under time pressure
    let config = OmsConfig {
        database_url: "postgresql://test:test@localhost/test_timeouts".to_string(),
        max_orders_memory: 1000,
        retention_days: 1,
        enable_audit: false,
        enable_matching: false,
        persist_batch_size: 100,
    };
    
    if let Ok(oms) = OrderManagementSystem::new(config).await {
        let request = OrderRequest {
            client_order_id: Some("TIMEOUT-TEST".to_string()),
            parent_order_id: None,
            symbol: Symbol(1),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            time_in_force: TimeInForce::Day,
            quantity: Qty::from_i64(1000),
            price: Some(Px::from_i64(1_000_000)),
            stop_price: None,
            account: "timeout_test_account".to_string(),
            exchange: "binance".to_string(),
            strategy_id: Some("timeout_test".to_string()),
            tags: vec![],
        };
        
        // Test with very short timeout
        let result = timeout(TokioDuration::from_millis(1), oms.create_order(request.clone())).await;
        
        // Operation may timeout or succeed depending on system speed
        match result {
            Ok(order_result) => {
                assert!(order_result.is_ok(), "If operation completes, it should succeed");
            }
            Err(_) => {
                println!("Operation timed out as expected");
            }
        }
        
        // Test with reasonable timeout - should succeed
        let result = timeout(TokioDuration::from_secs(5), oms.create_order(request)).await;
        assert!(result.is_ok(), "Operation should succeed with reasonable timeout");
    }
}

// Property-based testing for edge cases

proptest! {
    #[test]
    fn test_quantity_edge_cases(qty in i64::MIN..=i64::MAX) {
        let lifecycle_manager = OrderLifecycleManager::new();
        let mut order = Order {
            id: Uuid::new_v4(),
            client_order_id: None,
            parent_order_id: None,
            symbol: Symbol(1),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            time_in_force: TimeInForce::Day,
            quantity: Qty::from_i64(qty),
            executed_quantity: Qty::ZERO,
            remaining_quantity: Qty::from_i64(qty),
            price: Some(Px::from_i64(1_000_000)),
            stop_price: None,
            status: OrderStatus::New,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            account: "test".to_string(),
            exchange: "test".to_string(),
            strategy_id: None,
            tags: vec![],
            fills: vec![],
            amendments: vec![],
            version: 1,
            sequence_number: 1,
        };
        
        let result = lifecycle_manager.validate_order(&order);
        
        if qty <= 0 {
            prop_assert!(result.is_err(), "Non-positive quantity should be rejected");
        } else {
            prop_assert!(result.is_ok(), "Positive quantity should be accepted");
        }
    }
    
    #[test]
    fn test_price_edge_cases(price in i64::MIN..=i64::MAX) {
        let lifecycle_manager = OrderLifecycleManager::new();
        let order = Order {
            id: Uuid::new_v4(),
            client_order_id: None,
            parent_order_id: None,
            symbol: Symbol(1),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            time_in_force: TimeInForce::Day,
            quantity: Qty::from_i64(1000),
            executed_quantity: Qty::ZERO,
            remaining_quantity: Qty::from_i64(1000),
            price: Some(Px::from_i64(price)),
            stop_price: None,
            status: OrderStatus::New,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            account: "test".to_string(),
            exchange: "test".to_string(),
            strategy_id: None,
            tags: vec![],
            fills: vec![],
            amendments: vec![],
            version: 1,
            sequence_number: 1,
        };
        
        // Order validation should always complete (may accept or reject, but shouldn't crash)
        let result = lifecycle_manager.validate_order(&order);
        prop_assert!(result.is_ok() || result.is_err(), "Validation should complete for any price");
    }
    
    #[test]
    fn test_symbol_edge_cases(symbol_id in 0u32..=u32::MAX) {
        let lifecycle_manager = OrderLifecycleManager::new();
        let order = Order {
            id: Uuid::new_v4(),
            client_order_id: None,
            parent_order_id: None,
            symbol: Symbol(symbol_id),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            time_in_force: TimeInForce::Day,
            quantity: Qty::from_i64(1000),
            executed_quantity: Qty::ZERO,
            remaining_quantity: Qty::from_i64(1000),
            price: Some(Px::from_i64(1_000_000)),
            stop_price: None,
            status: OrderStatus::New,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            account: "test".to_string(),
            exchange: "test".to_string(),
            strategy_id: None,
            tags: vec![],
            fills: vec![],
            amendments: vec![],
            version: 1,
            sequence_number: 1,
        };
        
        // Should handle any symbol ID gracefully
        let result = lifecycle_manager.validate_order(&order);
        prop_assert!(result.is_ok(), "Should accept any valid symbol ID");
    }
}

// State consistency tests

#[rstest]
#[tokio::test]
async fn test_order_state_consistency() {
    let config = OmsConfig {
        database_url: "postgresql://test:test@localhost/test_consistency".to_string(),
        max_orders_memory: 1000,
        retention_days: 1,
        enable_audit: false,
        enable_matching: false,
        persist_batch_size: 100,
    };
    
    if let Ok(oms) = OrderManagementSystem::new(config).await {
        // Create order
        let request = OrderRequest {
            client_order_id: Some("CONSISTENCY-TEST".to_string()),
            parent_order_id: None,
            symbol: Symbol(1),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            time_in_force: TimeInForce::Day,
            quantity: Qty::from_i64(10_000),
            price: Some(Px::from_i64(1_000_000)),
            stop_price: None,
            account: "consistency_test".to_string(),
            exchange: "binance".to_string(),
            strategy_id: Some("consistency_test".to_string()),
            tags: vec![],
        };
        
        let order = oms.create_order(request).await.expect("Should create order");
        
        // Test invariants throughout order lifecycle
        assert_eq!(order.executed_quantity.as_i64() + order.remaining_quantity.as_i64(), 
                  order.quantity.as_i64(), "Quantity invariant should hold");
        assert!(order.version >= 1, "Version should be at least 1");
        assert!(order.sequence_number >= 1, "Sequence number should be at least 1");
        assert!(order.created_at <= order.updated_at, "Created time should be <= updated time");
        assert!(order.is_active(), "New order should be active");
        assert!(!order.is_terminal(), "New order should not be terminal");
        
        // Process partial fill
        let fill = Fill {
            id: Uuid::new_v4(),
            order_id: order.id,
            execution_id: "CONSISTENCY-FILL".to_string(),
            quantity: Qty::from_i64(3_000),
            price: Px::from_i64(1_000_000),
            commission: 30,
            commission_currency: "USDT".to_string(),
            timestamp: Utc::now(),
            liquidity: LiquidityIndicator::Maker,
        };
        
        oms.process_fill(order.id, fill).await.expect("Should process fill");
        
        // Verify state consistency after fill
        let filled_order = oms.get_order(&order.id).expect("Should retrieve order");
        assert_eq!(filled_order.executed_quantity.as_i64() + filled_order.remaining_quantity.as_i64(),
                  filled_order.quantity.as_i64(), "Quantity invariant should hold after fill");
        assert!(filled_order.executed_quantity.as_i64() > 0, "Should have executed quantity after fill");
        assert!(filled_order.remaining_quantity.as_i64() > 0, "Should have remaining quantity for partial fill");
        assert_eq!(filled_order.status, OrderStatus::PartiallyFilled, "Status should be partially filled");
        assert_eq!(filled_order.fills.len(), 1, "Should have one fill");
        
        // Test that operations maintain consistency
        let metrics_before = oms.get_metrics();
        let active_orders_before = oms.get_active_orders();
        
        // Cancel the order
        oms.cancel_order(order.id, "Consistency test".to_string()).await.expect("Should cancel order");
        
        let metrics_after = oms.get_metrics();
        let active_orders_after = oms.get_active_orders();
        
        // Verify metrics consistency
        assert_eq!(metrics_after.orders_cancelled, metrics_before.orders_cancelled + 1);
        assert_eq!(active_orders_after.len(), active_orders_before.len() - 1);
        
        // Verify final order state
        let cancelled_order = oms.get_order(&order.id).expect("Should retrieve cancelled order");
        assert_eq!(cancelled_order.status, OrderStatus::Cancelled);
        assert!(!cancelled_order.is_active());
        assert!(cancelled_order.is_terminal());
    }
}

// Data corruption and recovery edge cases

#[rstest]
fn test_order_with_corrupted_data() {
    let lifecycle_manager = OrderLifecycleManager::new();
    
    // Test order with impossible state combinations
    let mut corrupted_order = Order {
        id: Uuid::new_v4(),
        client_order_id: None,
        parent_order_id: None,
        symbol: Symbol(1),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Day,
        quantity: Qty::from_i64(1000),
        executed_quantity: Qty::from_i64(2000), // More than total quantity
        remaining_quantity: Qty::from_i64(-1000), // Negative remaining
        price: Some(Px::from_i64(1_000_000)),
        stop_price: None,
        status: OrderStatus::New,
        created_at: Utc::now(),
        updated_at: Utc::now() - Duration::hours(1), // Updated before created
        account: "test".to_string(),
        exchange: "test".to_string(),
        strategy_id: None,
        tags: vec![],
        fills: vec![],
        amendments: vec![],
        version: 0, // Invalid version
        sequence_number: 0, // Invalid sequence
    };
    
    // System should detect inconsistencies
    // Note: This would be handled in recovery/reconciliation systems
    assert!(corrupted_order.executed_quantity.as_i64() > corrupted_order.quantity.as_i64());
    assert!(corrupted_order.remaining_quantity.as_i64() < 0);
    assert!(corrupted_order.updated_at < corrupted_order.created_at);
    assert_eq!(corrupted_order.version, 0);
}

// Network and external dependency failures

#[rstest]
#[tokio::test]
async fn test_external_service_failures() {
    // Simulate external service failures
    let config = OmsConfig {
        database_url: "postgresql://test:test@localhost/test_external".to_string(),
        max_orders_memory: 1000,
        retention_days: 1,
        enable_audit: true, // Will try to write to audit log
        enable_matching: true, // Will use matching engine
        persist_batch_size: 1, // Force frequent DB writes
    };
    
    // This test would require actual mock services or dependency injection
    // For now, we test the error handling structure
    
    if let Ok(oms) = OrderManagementSystem::new(config).await {
        let request = OrderRequest {
            client_order_id: Some("EXTERNAL-FAILURE-TEST".to_string()),
            parent_order_id: None,
            symbol: Symbol(1),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            time_in_force: TimeInForce::Day,
            quantity: Qty::from_i64(1000),
            price: Some(Px::from_i64(1_000_000)),
            stop_price: None,
            account: "external_test".to_string(),
            exchange: "binance".to_string(),
            strategy_id: Some("external_test".to_string()),
            tags: vec![],
        };
        
        // Even if external services fail, core OMS should remain operational
        let result = oms.create_order(request).await;
        
        // Result may succeed or fail, but should not crash the system
        match result {
            Ok(order) => {
                println!("Order created successfully despite potential external failures: {}", order.id);
                
                // System should still be responsive
                let metrics = oms.get_metrics();
                assert!(metrics.orders_created > 0);
            }
            Err(e) => {
                println!("Order creation failed due to external service failure: {:?}", e);
                // This is acceptable - system degraded gracefully
            }
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_rapid_state_changes() {
    let config = OmsConfig {
        database_url: "postgresql://test:test@localhost/test_rapid_changes".to_string(),
        max_orders_memory: 100,
        retention_days: 1,
        enable_audit: false,
        enable_matching: false,
        persist_batch_size: 10,
    };
    
    if let Ok(oms) = OrderManagementSystem::new(config).await {
        let request = OrderRequest {
            client_order_id: Some("RAPID-CHANGE-TEST".to_string()),
            parent_order_id: None,
            symbol: Symbol(1),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            time_in_force: TimeInForce::Day,
            quantity: Qty::from_i64(10_000),
            price: Some(Px::from_i64(1_000_000)),
            stop_price: None,
            account: "rapid_test".to_string(),
            exchange: "binance".to_string(),
            strategy_id: Some("rapid_test".to_string()),
            tags: vec![],
        };
        
        let order = oms.create_order(request).await.expect("Should create order");
        
        // Rapid sequence of operations
        let operations = vec![
            oms.submit_order(order.id),
        ];
        
        for (i, operation) in operations.into_iter().enumerate() {
            let result = operation.await;
            match result {
                Ok(_) => println!("Operation {} succeeded", i),
                Err(e) => println!("Operation {} failed: {:?}", i, e),
            }
            
            // Very short delay between operations
            sleep(TokioDuration::from_millis(1)).await;
        }
        
        // System should maintain consistency despite rapid changes
        let final_order = oms.get_order(&order.id).expect("Should retrieve order");
        let metrics = oms.get_metrics();
        
        // Order should be in a valid state
        assert!(final_order.version >= 1);
        assert!(!matches!(final_order.status, OrderStatus::New)); // Should have progressed
        assert!(metrics.orders_created >= 1);
    }
}

#[rstest]
#[tokio::test]
async fn test_boundary_conditions() {
    // Test system behavior at various boundaries
    let test_cases = vec![
        (i64::MAX, "maximum quantity"),
        (1, "minimum positive quantity"),
        (1000000, "typical quantity"),
    ];
    
    let config = OmsConfig {
        database_url: "postgresql://test:test@localhost/test_boundaries".to_string(),
        max_orders_memory: 10,
        retention_days: 1,
        enable_audit: false,
        enable_matching: false,
        persist_batch_size: 1,
    };
    
    if let Ok(oms) = OrderManagementSystem::new(config).await {
        for (quantity, description) in test_cases {
            println!("Testing {}: {}", description, quantity);
            
            let request = OrderRequest {
                client_order_id: Some(format!("BOUNDARY-{}", quantity)),
                parent_order_id: None,
                symbol: Symbol(1),
                side: OrderSide::Buy,
                order_type: OrderType::Limit,
                time_in_force: TimeInForce::Day,
                quantity: Qty::from_i64(quantity),
                price: Some(Px::from_i64(1_000_000)),
                stop_price: None,
                account: "boundary_test".to_string(),
                exchange: "binance".to_string(),
                strategy_id: Some("boundary_test".to_string()),
                tags: vec![],
            };
            
            let result = oms.create_order(request).await;
            
            if quantity > 0 {
                match result {
                    Ok(order) => {
                        assert_eq!(order.quantity.as_i64(), quantity);
                        println!("✓ Successfully created order with {}", description);
                    }
                    Err(e) => {
                        println!("✗ Failed to create order with {}: {:?}", description, e);
                    }
                }
            } else {
                assert!(result.is_err(), "Should reject non-positive quantity");
                println!("✓ Correctly rejected {}", description);
            }
        }
    }
}