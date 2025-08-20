//! Unit tests for order lifecycle management

use chrono::{DateTime, Utc};
use rstest::*;
use services_common::{Px, Qty, Symbol};
use uuid::Uuid;

use oms::lifecycle::OrderLifecycleManager;
use oms::order::{Order, OrderSide, OrderStatus, OrderType, TimeInForce};

/// Test fixture for creating valid test orders
#[fixture]
fn valid_order() -> Order {
    Order {
        id: Uuid::new_v4(),
        client_order_id: Some("TEST-001".to_string()),
        parent_order_id: None,
        symbol: Symbol(1),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Day,
        quantity: Qty::from_i64(10_000),
        executed_quantity: Qty::ZERO,
        remaining_quantity: Qty::from_i64(10_000),
        price: Some(Px::from_i64(1_000_000)), // $100.00
        stop_price: None,
        status: OrderStatus::New,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        account: "test_account".to_string(),
        exchange: "binance".to_string(),
        strategy_id: Some("momentum_v1".to_string()),
        tags: vec!["test".to_string(), "unit_test".to_string()],
        fills: vec![],
        amendments: vec![],
        version: 1,
        sequence_number: 1,
    }
}

/// Test fixture for order lifecycle manager
#[fixture]
fn lifecycle_manager() -> OrderLifecycleManager {
    OrderLifecycleManager::new()
}

/// Test fixture for market order (no price)
#[fixture]
fn market_order() -> Order {
    let mut order = valid_order();
    order.order_type = OrderType::Market;
    order.price = None;
    order
}

/// Test fixture for stop order
#[fixture]
fn stop_order() -> Order {
    let mut order = valid_order();
    order.order_type = OrderType::Stop;
    order.stop_price = Some(Px::from_i64(950_000)); // $95.00
    order
}

/// Test fixture for algo parent order
#[fixture]
fn algo_parent_order() -> Order {
    let mut order = valid_order();
    order.order_type = OrderType::Twap;
    order.quantity = Qty::from_i64(100_000);
    order.remaining_quantity = Qty::from_i64(100_000);
    order
}

#[rstest]
fn test_valid_order_validation(lifecycle_manager: OrderLifecycleManager, valid_order: Order) {
    let result = lifecycle_manager.validate_order(&valid_order);
    assert!(result.is_ok(), "Valid order should pass validation");
}

#[rstest]
fn test_zero_quantity_validation(lifecycle_manager: OrderLifecycleManager) {
    let mut order = valid_order();
    order.quantity = Qty::ZERO;
    
    let result = lifecycle_manager.validate_order(&order);
    assert!(result.is_err(), "Zero quantity should fail validation");
    assert!(result.unwrap_err().to_string().contains("quantity must be positive"));
}

#[rstest]
fn test_negative_quantity_validation(lifecycle_manager: OrderLifecycleManager) {
    let mut order = valid_order();
    order.quantity = Qty::from_i64(-1000);
    
    let result = lifecycle_manager.validate_order(&order);
    assert!(result.is_err(), "Negative quantity should fail validation");
}

#[rstest]
fn test_limit_order_requires_price(lifecycle_manager: OrderLifecycleManager) {
    let mut order = valid_order();
    order.order_type = OrderType::Limit;
    order.price = None;
    
    let result = lifecycle_manager.validate_order(&order);
    assert!(result.is_err(), "Limit order without price should fail");
    assert!(result.unwrap_err().to_string().contains("Limit order requires price"));
}

#[rstest]
fn test_stop_order_requires_stop_price(lifecycle_manager: OrderLifecycleManager) {
    let mut order = valid_order();
    order.order_type = OrderType::Stop;
    order.stop_price = None;
    
    let result = lifecycle_manager.validate_order(&order);
    assert!(result.is_err(), "Stop order without stop price should fail");
    assert!(result.unwrap_err().to_string().contains("Stop order requires stop price"));
}

#[rstest]
fn test_stop_limit_order_requirements(lifecycle_manager: OrderLifecycleManager) {
    let mut order = valid_order();
    order.order_type = OrderType::StopLimit;
    order.price = None;
    order.stop_price = None;
    
    let result = lifecycle_manager.validate_order(&order);
    assert!(result.is_err(), "StopLimit order should require both price and stop price");
}

#[rstest]
fn test_market_order_validation(lifecycle_manager: OrderLifecycleManager, market_order: Order) {
    let result = lifecycle_manager.validate_order(&market_order);
    assert!(result.is_ok(), "Market order should be valid without price");
}

#[rstest]
fn test_gtt_expiry_validation(lifecycle_manager: OrderLifecycleManager) {
    let mut order = valid_order();
    let past_time = Utc::now() - chrono::Duration::hours(1);
    order.time_in_force = TimeInForce::Gtt(past_time);
    
    let result = lifecycle_manager.validate_order(&order);
    assert!(result.is_err(), "GTT with past expiry should fail");
    assert!(result.unwrap_err().to_string().contains("GTT expiry must be in the future"));
}

#[rstest]
fn test_empty_account_validation(lifecycle_manager: OrderLifecycleManager) {
    let mut order = valid_order();
    order.account = String::new();
    
    let result = lifecycle_manager.validate_order(&order);
    assert!(result.is_err(), "Empty account should fail validation");
    assert!(result.unwrap_err().to_string().contains("Account is required"));
}

#[rstest]
fn test_empty_exchange_validation(lifecycle_manager: OrderLifecycleManager) {
    let mut order = valid_order();
    order.exchange = String::new();
    
    let result = lifecycle_manager.validate_order(&order);
    assert!(result.is_err(), "Empty exchange should fail validation");
    assert!(result.unwrap_err().to_string().contains("Exchange is required"));
}

#[rstest]
#[case(OrderStatus::New, OrderStatus::Pending, true)]
#[case(OrderStatus::New, OrderStatus::Cancelled, true)]
#[case(OrderStatus::New, OrderStatus::Rejected, true)]
#[case(OrderStatus::New, OrderStatus::Filled, false)]
#[case(OrderStatus::Pending, OrderStatus::Submitted, true)]
#[case(OrderStatus::Pending, OrderStatus::Cancelled, true)]
#[case(OrderStatus::Pending, OrderStatus::New, false)]
#[case(OrderStatus::Submitted, OrderStatus::Accepted, true)]
#[case(OrderStatus::Submitted, OrderStatus::Cancelled, true)]
#[case(OrderStatus::Accepted, OrderStatus::PartiallyFilled, true)]
#[case(OrderStatus::Accepted, OrderStatus::Filled, true)]
#[case(OrderStatus::PartiallyFilled, OrderStatus::Filled, true)]
#[case(OrderStatus::PartiallyFilled, OrderStatus::Cancelled, true)]
#[case(OrderStatus::Filled, OrderStatus::New, false)]
#[case(OrderStatus::Cancelled, OrderStatus::New, false)]
#[case(OrderStatus::Rejected, OrderStatus::Pending, false)]
fn test_state_transitions(
    lifecycle_manager: OrderLifecycleManager,
    #[case] from_status: OrderStatus,
    #[case] to_status: OrderStatus,
    #[case] expected_valid: bool,
) {
    let mut order = valid_order();
    order.status = from_status;
    
    let result = lifecycle_manager.validate_transition(&order, to_status);
    
    if expected_valid {
        assert!(result.is_ok(), "Transition {:?} -> {:?} should be valid", from_status, to_status);
    } else {
        assert!(result.is_err(), "Transition {:?} -> {:?} should be invalid", from_status, to_status);
    }
}

#[rstest]
#[case(OrderStatus::New, true)]
#[case(OrderStatus::Pending, true)]
#[case(OrderStatus::Submitted, true)]
#[case(OrderStatus::Accepted, true)]
#[case(OrderStatus::PartiallyFilled, true)]
#[case(OrderStatus::Filled, false)]
#[case(OrderStatus::Cancelled, false)]
#[case(OrderStatus::Rejected, false)]
#[case(OrderStatus::Expired, false)]
fn test_can_cancel(
    lifecycle_manager: OrderLifecycleManager,
    #[case] status: OrderStatus,
    #[case] can_cancel: bool,
) {
    let mut order = valid_order();
    order.status = status;
    
    let result = lifecycle_manager.can_cancel(&order);
    assert_eq!(result, can_cancel, "Can cancel check failed for status {:?}", status);
}

#[rstest]
#[case(OrderStatus::New, true)]
#[case(OrderStatus::Pending, true)]
#[case(OrderStatus::Submitted, true)]
#[case(OrderStatus::Accepted, true)]
#[case(OrderStatus::PartiallyFilled, false)]
#[case(OrderStatus::Filled, false)]
#[case(OrderStatus::Cancelled, false)]
#[case(OrderStatus::Rejected, false)]
#[case(OrderStatus::Expired, false)]
fn test_can_amend(
    lifecycle_manager: OrderLifecycleManager,
    #[case] status: OrderStatus,
    #[case] can_amend: bool,
) {
    let mut order = valid_order();
    order.status = status;
    
    let result = lifecycle_manager.can_amend(&order);
    assert_eq!(result, can_amend, "Can amend check failed for status {:?}", status);
}

#[rstest]
fn test_day_order_expiry(lifecycle_manager: OrderLifecycleManager) {
    let mut order = valid_order();
    order.time_in_force = TimeInForce::Day;
    order.created_at = Utc::now() - chrono::Duration::days(1);
    order.status = OrderStatus::New;
    
    let should_expire = lifecycle_manager.should_expire(&order);
    assert!(should_expire, "Day order from previous day should expire");
}

#[rstest]
fn test_gtt_order_expiry(lifecycle_manager: OrderLifecycleManager) {
    let mut order = valid_order();
    let past_expiry = Utc::now() - chrono::Duration::minutes(5);
    order.time_in_force = TimeInForce::Gtt(past_expiry);
    order.status = OrderStatus::New;
    
    let should_expire = lifecycle_manager.should_expire(&order);
    assert!(should_expire, "GTT order past expiry should expire");
}

#[rstest]
fn test_gtc_order_no_expiry(lifecycle_manager: OrderLifecycleManager) {
    let mut order = valid_order();
    order.time_in_force = TimeInForce::Gtc;
    order.created_at = Utc::now() - chrono::Duration::days(30);
    order.status = OrderStatus::New;
    
    let should_expire = lifecycle_manager.should_expire(&order);
    assert!(!should_expire, "GTC order should never expire based on time");
}

#[rstest]
fn test_terminal_order_no_expiry(lifecycle_manager: OrderLifecycleManager) {
    let mut order = valid_order();
    order.time_in_force = TimeInForce::Day;
    order.created_at = Utc::now() - chrono::Duration::days(1);
    order.status = OrderStatus::Filled;
    
    let should_expire = lifecycle_manager.should_expire(&order);
    assert!(!should_expire, "Terminal orders should not expire");
}

#[rstest]
fn test_expire_order_process(lifecycle_manager: OrderLifecycleManager) {
    let mut order = valid_order();
    order.time_in_force = TimeInForce::Day;
    order.created_at = Utc::now() - chrono::Duration::days(1);
    order.status = OrderStatus::New;
    
    let result = lifecycle_manager.expire_order(&mut order);
    assert!(result.is_ok(), "Order expiry should succeed");
    assert_eq!(order.status, OrderStatus::Expired, "Order should be marked as expired");
    assert!(order.updated_at > order.created_at, "Updated timestamp should be newer");
}

#[rstest]
fn test_expire_order_invalid(lifecycle_manager: OrderLifecycleManager) {
    let mut order = valid_order();
    order.time_in_force = TimeInForce::Gtc;
    order.status = OrderStatus::New;
    
    let result = lifecycle_manager.expire_order(&mut order);
    assert!(result.is_err(), "Should not be able to expire non-expiring order");
}

#[rstest]
fn test_get_valid_transitions(lifecycle_manager: OrderLifecycleManager) {
    let transitions = lifecycle_manager.get_valid_transitions(OrderStatus::New);
    assert!(transitions.contains(&OrderStatus::Pending));
    assert!(transitions.contains(&OrderStatus::Cancelled));
    assert!(transitions.contains(&OrderStatus::Rejected));
    assert!(!transitions.contains(&OrderStatus::Filled));
    
    let terminal_transitions = lifecycle_manager.get_valid_transitions(OrderStatus::Filled);
    assert!(terminal_transitions.is_empty(), "Terminal states should have no valid transitions");
}

#[rstest]
fn test_valid_parent_child_relationship(lifecycle_manager: OrderLifecycleManager, algo_parent_order: Order) {
    let mut child_order = valid_order();
    child_order.parent_order_id = Some(algo_parent_order.id);
    child_order.quantity = Qty::from_i64(10_000); // Less than parent remaining
    
    let result = lifecycle_manager.validate_parent_child(&algo_parent_order, &child_order);
    assert!(result.is_ok(), "Valid parent-child relationship should pass");
}

#[rstest]
fn test_child_without_parent_reference(lifecycle_manager: OrderLifecycleManager, algo_parent_order: Order) {
    let mut child_order = valid_order();
    child_order.parent_order_id = None; // No parent reference
    
    let result = lifecycle_manager.validate_parent_child(&algo_parent_order, &child_order);
    assert!(result.is_err(), "Child without parent reference should fail");
    assert!(result.unwrap_err().to_string().contains("Invalid parent-child relationship"));
}

#[rstest]
fn test_non_algo_parent(lifecycle_manager: OrderLifecycleManager) {
    let parent_order = valid_order(); // Limit order, not algo
    let mut child_order = valid_order();
    child_order.parent_order_id = Some(parent_order.id);
    
    let result = lifecycle_manager.validate_parent_child(&parent_order, &child_order);
    assert!(result.is_err(), "Non-algo parent should fail validation");
    assert!(result.unwrap_err().to_string().contains("Parent must be an algorithmic order"));
}

#[rstest]
fn test_child_quantity_exceeds_parent(lifecycle_manager: OrderLifecycleManager, algo_parent_order: Order) {
    let mut child_order = valid_order();
    child_order.parent_order_id = Some(algo_parent_order.id);
    child_order.quantity = Qty::from_i64(200_000); // More than parent remaining
    
    let result = lifecycle_manager.validate_parent_child(&algo_parent_order, &child_order);
    assert!(result.is_err(), "Child quantity exceeding parent should fail");
    assert!(result.unwrap_err().to_string().contains("Child quantity exceeds parent remaining quantity"));
}

#[rstest]
#[case(OrderType::Twap)]
#[case(OrderType::Vwap)]
#[case(OrderType::Pov)]
fn test_algo_order_types_as_parents(
    lifecycle_manager: OrderLifecycleManager,
    #[case] algo_type: OrderType,
) {
    let mut parent_order = valid_order();
    parent_order.order_type = algo_type;
    parent_order.quantity = Qty::from_i64(100_000);
    parent_order.remaining_quantity = Qty::from_i64(100_000);
    
    let mut child_order = valid_order();
    child_order.parent_order_id = Some(parent_order.id);
    child_order.quantity = Qty::from_i64(10_000);
    
    let result = lifecycle_manager.validate_parent_child(&parent_order, &child_order);
    assert!(result.is_ok(), "Algo order type {:?} should be valid parent", algo_type);
}

// Property-based tests using proptest
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;
    
    proptest! {
        #[test]
        fn test_positive_quantities_always_valid(qty in 1i64..1_000_000_000i64) {
            let lifecycle_manager = OrderLifecycleManager::new();
            let mut order = valid_order();
            order.quantity = Qty::from_i64(qty);
            order.remaining_quantity = Qty::from_i64(qty);
            
            let result = lifecycle_manager.validate_order(&order);
            prop_assert!(result.is_ok());
        }
        
        #[test]
        fn test_non_positive_quantities_always_invalid(qty in i64::MIN..=0i64) {
            let lifecycle_manager = OrderLifecycleManager::new();
            let mut order = valid_order();
            order.quantity = Qty::from_i64(qty);
            
            let result = lifecycle_manager.validate_order(&order);
            prop_assert!(result.is_err());
        }
        
        #[test]
        fn test_active_orders_can_be_cancelled(status in prop::sample::select(vec![
            OrderStatus::New,
            OrderStatus::Pending,
            OrderStatus::Submitted,
            OrderStatus::Accepted,
            OrderStatus::PartiallyFilled,
        ])) {
            let lifecycle_manager = OrderLifecycleManager::new();
            let mut order = valid_order();
            order.status = status;
            
            prop_assert!(lifecycle_manager.can_cancel(&order));
        }
        
        #[test]
        fn test_terminal_orders_cannot_be_cancelled(status in prop::sample::select(vec![
            OrderStatus::Filled,
            OrderStatus::Cancelled,
            OrderStatus::Rejected,
            OrderStatus::Expired,
        ])) {
            let lifecycle_manager = OrderLifecycleManager::new();
            let mut order = valid_order();
            order.status = status;
            
            prop_assert!(!lifecycle_manager.can_cancel(&order));
        }
    }
}

// Performance tests
#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;
    
    #[test]
    fn test_validation_performance() {
        let lifecycle_manager = OrderLifecycleManager::new();
        let order = valid_order();
        
        let start = Instant::now();
        for _ in 0..10_000 {
            let _ = lifecycle_manager.validate_order(&order);
        }
        let duration = start.elapsed();
        
        // Should validate 10k orders in less than 100ms
        assert!(duration.as_millis() < 100, "Validation too slow: {}ms", duration.as_millis());
    }
    
    #[test]
    fn test_transition_validation_performance() {
        let lifecycle_manager = OrderLifecycleManager::new();
        let order = valid_order();
        
        let start = Instant::now();
        for _ in 0..10_000 {
            let _ = lifecycle_manager.validate_transition(&order, OrderStatus::Pending);
        }
        let duration = start.elapsed();
        
        // Should validate 10k transitions in less than 50ms
        assert!(duration.as_millis() < 50, "Transition validation too slow: {}ms", duration.as_millis());
    }
}