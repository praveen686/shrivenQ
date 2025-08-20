//! Unit tests for persistence and recovery functionality

use chrono::{Duration, Utc};
use rstest::*;
use services_common::{Px, Qty, Symbol};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use uuid::Uuid;

use oms::persistence::{PersistenceManager, parse_order_side, parse_order_type, parse_order_status, parse_time_in_force, parse_liquidity};
use oms::recovery::{RecoveryManager, RecoveryStats, OrderDiscrepancy, DiscrepancyType, RecoveryAction};
use oms::order::{Order, OrderSide, OrderStatus, OrderType, TimeInForce, Fill, Amendment, LiquidityIndicator};

/// Test fixture for creating a test database
#[fixture]
async fn test_db() -> PgPool {
    let pool = sqlx::PgPool::connect("postgresql://test:test@localhost/test_oms")
        .await
        .expect("Failed to connect to test database");
    
    // Run migrations to create tables
    oms::persistence::run_migrations(&pool).await.expect("Failed to run migrations");
    
    pool
}

/// Test fixture for persistence manager
#[fixture]
fn persistence_manager(#[future] test_db: PgPool) -> PersistenceManager {
    PersistenceManager::new(test_db)
}

/// Test fixture for recovery manager
#[fixture]
fn recovery_manager(#[future] test_db: PgPool) -> RecoveryManager {
    RecoveryManager::new(test_db)
}

/// Test fixture for creating test orders
#[fixture]
fn test_order() -> Order {
    Order {
        id: Uuid::new_v4(),
        client_order_id: Some("PERSIST-TEST-001".to_string()),
        parent_order_id: None,
        symbol: Symbol(1),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Day,
        quantity: Qty::from_i64(10_000),
        executed_quantity: Qty::ZERO,
        remaining_quantity: Qty::from_i64(10_000),
        price: Some(Px::from_i64(1_000_000)),
        stop_price: None,
        status: OrderStatus::New,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        account: "test_persistence_account".to_string(),
        exchange: "test_exchange".to_string(),
        strategy_id: Some("persistence_test_strategy".to_string()),
        tags: vec!["persistence".to_string(), "test".to_string()],
        fills: vec![],
        amendments: vec![],
        version: 1,
        sequence_number: 1,
    }
}

/// Test fixture for partially filled order
#[fixture]
fn partially_filled_order() -> Order {
    let mut order = test_order();
    order.status = OrderStatus::PartiallyFilled;
    order.executed_quantity = Qty::from_i64(3_000);
    order.remaining_quantity = Qty::from_i64(7_000);
    order.version = 2;
    order
}

/// Test fixture for test fill
#[fixture]
fn test_fill(test_order: Order) -> Fill {
    Fill {
        id: Uuid::new_v4(),
        order_id: test_order.id,
        execution_id: "PERSIST-EXEC-001".to_string(),
        quantity: Qty::from_i64(3_000),
        price: Px::from_i64(1_000_000),
        commission: 30,
        commission_currency: "USDT".to_string(),
        timestamp: Utc::now(),
        liquidity: LiquidityIndicator::Maker,
    }
}

/// Test fixture for test amendment
#[fixture]
fn test_amendment(test_order: Order) -> Amendment {
    Amendment {
        id: Uuid::new_v4(),
        order_id: test_order.id,
        new_quantity: Some(Qty::from_i64(15_000)),
        new_price: Some(Px::from_i64(1_010_000)),
        reason: "Position size adjustment".to_string(),
        timestamp: Utc::now(),
    }
}

// Persistence Manager Tests

#[rstest]
#[tokio::test]
async fn test_save_order(
    #[future] persistence_manager: PersistenceManager,
    test_order: Order,
) {
    let persistence = persistence_manager.await;
    
    let result = persistence.save_order(&test_order).await;
    assert!(result.is_ok(), "Should save order successfully");
    
    // Verify order was saved by loading it back
    let loaded_order = persistence.load_order(test_order.id).await.expect("Should load order").expect("Order should exist");
    
    assert_eq!(loaded_order.id, test_order.id);
    assert_eq!(loaded_order.client_order_id, test_order.client_order_id);
    assert_eq!(loaded_order.symbol, test_order.symbol);
    assert_eq!(loaded_order.side, test_order.side);
    assert_eq!(loaded_order.order_type, test_order.order_type);
    assert_eq!(loaded_order.quantity, test_order.quantity);
    assert_eq!(loaded_order.price, test_order.price);
    assert_eq!(loaded_order.status, test_order.status);
    assert_eq!(loaded_order.account, test_order.account);
    assert_eq!(loaded_order.version, test_order.version);
}

#[rstest]
#[tokio::test]
async fn test_save_order_upsert(
    #[future] persistence_manager: PersistenceManager,
    test_order: Order,
) {
    let persistence = persistence_manager.await;
    
    // Save order first time
    persistence.save_order(&test_order).await.expect("Should save order");
    
    // Modify and save again (should update)
    let mut updated_order = test_order.clone();
    updated_order.status = OrderStatus::Pending;
    updated_order.version = 2;
    updated_order.updated_at = Utc::now();
    
    let result = persistence.save_order(&updated_order).await;
    assert!(result.is_ok(), "Should update existing order");
    
    // Verify update
    let loaded_order = persistence.load_order(test_order.id).await.expect("Should load order").expect("Order should exist");
    assert_eq!(loaded_order.status, OrderStatus::Pending);
    assert_eq!(loaded_order.version, 2);
}

#[rstest]
#[tokio::test]
async fn test_update_order_status(
    #[future] persistence_manager: PersistenceManager,
    test_order: Order,
) {
    let persistence = persistence_manager.await;
    
    // Save initial order
    persistence.save_order(&test_order).await.expect("Should save order");
    
    // Update status
    let mut updated_order = test_order.clone();
    updated_order.status = OrderStatus::Submitted;
    updated_order.updated_at = Utc::now();
    
    let result = persistence.update_order_status(&updated_order).await;
    assert!(result.is_ok(), "Should update order status");
    
    // Verify status update
    let loaded_order = persistence.load_order(test_order.id).await.expect("Should load order").expect("Order should exist");
    assert_eq!(loaded_order.status, OrderStatus::Submitted);
}

#[rstest]
#[tokio::test]
async fn test_update_order_quantities(
    #[future] persistence_manager: PersistenceManager,
    partially_filled_order: Order,
) {
    let persistence = persistence_manager.await;
    
    // Save initial order
    persistence.save_order(&partially_filled_order).await.expect("Should save order");
    
    // Update quantities
    let mut updated_order = partially_filled_order.clone();
    updated_order.executed_quantity = Qty::from_i64(5_000);
    updated_order.remaining_quantity = Qty::from_i64(5_000);
    updated_order.updated_at = Utc::now();
    
    let result = persistence.update_order_quantities(&updated_order).await;
    assert!(result.is_ok(), "Should update order quantities");
    
    // Verify quantity updates
    let loaded_order = persistence.load_order(partially_filled_order.id).await.expect("Should load order").expect("Order should exist");
    assert_eq!(loaded_order.executed_quantity.as_i64(), 5_000);
    assert_eq!(loaded_order.remaining_quantity.as_i64(), 5_000);
}

#[rstest]
#[tokio::test]
async fn test_save_fill(
    #[future] persistence_manager: PersistenceManager,
    test_order: Order,
    test_fill: Fill,
) {
    let persistence = persistence_manager.await;
    
    // Save order first
    persistence.save_order(&test_order).await.expect("Should save order");
    
    // Save fill
    let result = persistence.save_fill(&test_fill).await;
    assert!(result.is_ok(), "Should save fill successfully");
}

#[rstest]
#[tokio::test]
async fn test_save_amendment(
    #[future] persistence_manager: PersistenceManager,
    test_order: Order,
    test_amendment: Amendment,
) {
    let persistence = persistence_manager.await;
    
    // Save order first
    persistence.save_order(&test_order).await.expect("Should save order");
    
    // Save amendment
    let result = persistence.save_amendment(&test_amendment).await;
    assert!(result.is_ok(), "Should save amendment successfully");
}

#[rstest]
#[tokio::test]
async fn test_load_active_orders(
    #[future] persistence_manager: PersistenceManager,
) {
    let persistence = persistence_manager.await;
    
    // Create test orders with different statuses
    let active_order = Order {
        id: Uuid::new_v4(),
        status: OrderStatus::Pending,
        ..test_order()
    };
    
    let filled_order = Order {
        id: Uuid::new_v4(),
        status: OrderStatus::Filled,
        ..test_order()
    };
    
    let cancelled_order = Order {
        id: Uuid::new_v4(),
        status: OrderStatus::Cancelled,
        ..test_order()
    };
    
    // Save all orders
    persistence.save_order(&active_order).await.expect("Should save active order");
    persistence.save_order(&filled_order).await.expect("Should save filled order");
    persistence.save_order(&cancelled_order).await.expect("Should save cancelled order");
    
    // Load active orders
    let active_orders = persistence.load_active_orders().await.expect("Should load active orders");
    
    // Should only return non-terminal orders
    assert_eq!(active_orders.len(), 1, "Should load only active orders");
    assert_eq!(active_orders[0].id, active_order.id);
    assert_eq!(active_orders[0].status, OrderStatus::Pending);
}

#[rstest]
#[tokio::test]
async fn test_load_nonexistent_order(
    #[future] persistence_manager: PersistenceManager,
) {
    let persistence = persistence_manager.await;
    
    let fake_id = Uuid::new_v4();
    let result = persistence.load_order(fake_id).await.expect("Should handle gracefully");
    
    assert!(result.is_none(), "Should return None for nonexistent order");
}

#[rstest]
#[tokio::test]
async fn test_delete_old_orders(
    #[future] persistence_manager: PersistenceManager,
) {
    let persistence = persistence_manager.await;
    
    // Create old filled order
    let mut old_order = test_order();
    old_order.status = OrderStatus::Filled;
    old_order.updated_at = Utc::now() - Duration::days(10);
    
    // Create recent filled order
    let mut recent_order = Order {
        id: Uuid::new_v4(),
        status: OrderStatus::Filled,
        ..test_order()
    };
    recent_order.updated_at = Utc::now();
    
    // Save both orders
    persistence.save_order(&old_order).await.expect("Should save old order");
    persistence.save_order(&recent_order).await.expect("Should save recent order");
    
    // Delete orders older than 5 days
    let deleted_count = persistence.delete_old_orders(5).await.expect("Should delete old orders");
    
    assert_eq!(deleted_count, 1, "Should delete 1 old order");
    
    // Verify only recent order remains
    let old_result = persistence.load_order(old_order.id).await.expect("Should query").is_none();
    let recent_result = persistence.load_order(recent_order.id).await.expect("Should query").is_some();
    
    assert!(old_result, "Old order should be deleted");
    assert!(recent_result, "Recent order should remain");
}

// Parse function tests

#[rstest]
#[case("Buy", OrderSide::Buy)]
#[case("Sell", OrderSide::Sell)]
fn test_parse_order_side(#[case] input: &str, #[case] expected: OrderSide) {
    let result = parse_order_side(input).expect("Should parse successfully");
    assert_eq!(result, expected);
}

#[rstest]
fn test_parse_invalid_order_side() {
    let result = parse_order_side("Invalid");
    assert!(result.is_err(), "Should fail to parse invalid side");
}

#[rstest]
#[case("Market", OrderType::Market)]
#[case("Limit", OrderType::Limit)]
#[case("Stop", OrderType::Stop)]
#[case("StopLimit", OrderType::StopLimit)]
#[case("Twap", OrderType::Twap)]
#[case("Vwap", OrderType::Vwap)]
#[case("Pov", OrderType::Pov)]
fn test_parse_order_type(#[case] input: &str, #[case] expected: OrderType) {
    let result = parse_order_type(input).expect("Should parse successfully");
    assert_eq!(result, expected);
}

#[rstest]
#[case("New", OrderStatus::New)]
#[case("Pending", OrderStatus::Pending)]
#[case("Filled", OrderStatus::Filled)]
#[case("Cancelled", OrderStatus::Cancelled)]
#[case("PartiallyFilled", OrderStatus::PartiallyFilled)]
fn test_parse_order_status(#[case] input: &str, #[case] expected: OrderStatus) {
    let result = parse_order_status(input).expect("Should parse successfully");
    assert_eq!(result, expected);
}

#[rstest]
#[case("Gtc", TimeInForce::Gtc)]
#[case("Ioc", TimeInForce::Ioc)]
#[case("Fok", TimeInForce::Fok)]
#[case("Day", TimeInForce::Day)]
fn test_parse_time_in_force(#[case] input: &str, #[case] expected: TimeInForce) {
    let result = parse_time_in_force(input).expect("Should parse successfully");
    assert_eq!(result, expected);
}

#[rstest]
#[case("Maker", LiquidityIndicator::Maker)]
#[case("Taker", LiquidityIndicator::Taker)]
fn test_parse_liquidity(#[case] input: &str, #[case] expected: LiquidityIndicator) {
    let result = parse_liquidity(input).expect("Should parse successfully");
    assert_eq!(result, expected);
}

// Recovery Manager Tests

#[rstest]
#[tokio::test]
async fn test_recovery_manager_creation(
    #[future] recovery_manager: RecoveryManager,
) {
    let _recovery = recovery_manager.await;
    // Just verify it can be created without errors
}

#[rstest]
#[tokio::test]
async fn test_full_recovery_empty_database(
    #[future] recovery_manager: RecoveryManager,
) {
    let recovery = recovery_manager.await;
    
    let stats = recovery.recover().await.expect("Should complete recovery");
    
    assert_eq!(stats.orders_recovered, 0, "Should recover 0 orders from empty DB");
    assert_eq!(stats.fills_recovered, 0, "Should recover 0 fills from empty DB");
    assert_eq!(stats.discrepancies_found, 0, "Should find 0 discrepancies in empty DB");
    assert!(stats.recovery_time_ms < 1000, "Recovery should be fast for empty DB");
}

#[rstest]
#[tokio::test]
async fn test_recovery_with_consistent_data(
    #[future] recovery_manager: RecoveryManager,
    #[future] test_db: PgPool,
) {
    let recovery = recovery_manager.await;
    let db = test_db.await;
    
    // Create consistent test data
    let order = Order {
        id: Uuid::new_v4(),
        status: OrderStatus::PartiallyFilled,
        executed_quantity: Qty::from_i64(3_000),
        remaining_quantity: Qty::from_i64(7_000),
        ..test_order()
    };
    
    let fill = Fill {
        id: Uuid::new_v4(),
        order_id: order.id,
        quantity: Qty::from_i64(3_000),
        price: Px::from_i64(1_000_000),
        commission: 30,
        commission_currency: "USDT".to_string(),
        timestamp: Utc::now(),
        liquidity: LiquidityIndicator::Taker,
        execution_id: "RECOVERY-TEST-001".to_string(),
    };
    
    // Save to database
    let persistence = PersistenceManager::new(db);
    persistence.save_order(&order).await.expect("Should save order");
    persistence.save_fill(&fill).await.expect("Should save fill");
    
    // Run recovery
    let stats = recovery.recover().await.expect("Should complete recovery");
    
    assert_eq!(stats.orders_recovered, 1, "Should recover 1 order");
    assert_eq!(stats.fills_recovered, 1, "Should recover 1 fill");
    assert_eq!(stats.discrepancies_found, 0, "Should find no discrepancies with consistent data");
}

#[rstest]
#[tokio::test]
async fn test_recovery_with_quantity_mismatch(
    #[future] recovery_manager: RecoveryManager,
    #[future] test_db: PgPool,
) {
    let recovery = recovery_manager.await;
    let db = test_db.await;
    
    // Create inconsistent data - order shows 5000 executed but fill shows 3000
    let order = Order {
        id: Uuid::new_v4(),
        status: OrderStatus::PartiallyFilled,
        executed_quantity: Qty::from_i64(5_000), // Mismatch here
        remaining_quantity: Qty::from_i64(5_000),
        ..test_order()
    };
    
    let fill = Fill {
        id: Uuid::new_v4(),
        order_id: order.id,
        quantity: Qty::from_i64(3_000), // Actual fill quantity
        price: Px::from_i64(1_000_000),
        commission: 30,
        commission_currency: "USDT".to_string(),
        timestamp: Utc::now(),
        liquidity: LiquidityIndicator::Taker,
        execution_id: "RECOVERY-MISMATCH-001".to_string(),
    };
    
    // Save to database
    let persistence = PersistenceManager::new(db);
    persistence.save_order(&order).await.expect("Should save order");
    persistence.save_fill(&fill).await.expect("Should save fill");
    
    // Run recovery
    let stats = recovery.recover().await.expect("Should complete recovery");
    
    assert_eq!(stats.orders_recovered, 1, "Should recover 1 order");
    assert_eq!(stats.fills_recovered, 1, "Should recover 1 fill");
    assert_eq!(stats.discrepancies_found, 1, "Should find 1 discrepancy");
    assert_eq!(stats.orders_reconciled, 1, "Should reconcile 1 order");
}

#[rstest]
#[tokio::test]
async fn test_recovery_with_missing_fills(
    #[future] recovery_manager: RecoveryManager,
    #[future] test_db: PgPool,
) {
    let recovery = recovery_manager.await;
    let db = test_db.await;
    
    // Create order that claims to be executed but has no fills
    let order = Order {
        id: Uuid::new_v4(),
        status: OrderStatus::PartiallyFilled,
        executed_quantity: Qty::from_i64(3_000), // Claims executed
        remaining_quantity: Qty::from_i64(7_000),
        ..test_order()
    };
    
    // Save order but no fills
    let persistence = PersistenceManager::new(db);
    persistence.save_order(&order).await.expect("Should save order");
    
    // Run recovery
    let stats = recovery.recover().await.expect("Should complete recovery");
    
    assert_eq!(stats.orders_recovered, 1, "Should recover 1 order");
    assert_eq!(stats.fills_recovered, 0, "Should recover 0 fills");
    assert_eq!(stats.discrepancies_found, 1, "Should find 1 discrepancy for missing fills");
}

#[rstest]
#[tokio::test]
async fn test_recovery_with_status_inconsistency(
    #[future] recovery_manager: RecoveryManager,
    #[future] test_db: PgPool,
) {
    let recovery = recovery_manager.await;
    let db = test_db.await;
    
    // Create order with fills but wrong status
    let order = Order {
        id: Uuid::new_v4(),
        status: OrderStatus::New, // Wrong status - should be PartiallyFilled
        executed_quantity: Qty::ZERO, // Also wrong
        remaining_quantity: Qty::from_i64(10_000),
        ..test_order()
    };
    
    let fill = Fill {
        id: Uuid::new_v4(),
        order_id: order.id,
        quantity: Qty::from_i64(3_000),
        price: Px::from_i64(1_000_000),
        commission: 30,
        commission_currency: "USDT".to_string(),
        timestamp: Utc::now(),
        liquidity: LiquidityIndicator::Taker,
        execution_id: "RECOVERY-STATUS-001".to_string(),
    };
    
    // Save to database
    let persistence = PersistenceManager::new(db);
    persistence.save_order(&order).await.expect("Should save order");
    persistence.save_fill(&fill).await.expect("Should save fill");
    
    // Run recovery
    let stats = recovery.recover().await.expect("Should complete recovery");
    
    assert!(stats.discrepancies_found >= 1, "Should find discrepancies for status inconsistency");
}

#[rstest]
#[tokio::test]
async fn test_recovery_validation(
    #[future] recovery_manager: RecoveryManager,
    #[future] test_db: PgPool,
) {
    let recovery = recovery_manager.await;
    let db = test_db.await;
    
    // Create consistent test data
    let order = Order {
        id: Uuid::new_v4(),
        status: OrderStatus::Pending,
        ..test_order()
    };
    
    let persistence = PersistenceManager::new(db);
    persistence.save_order(&order).await.expect("Should save order");
    
    // Create recovered orders list
    let recovered_orders = vec![order];
    
    let validation_result = recovery.validate_recovery(&recovered_orders).await.expect("Should validate");
    assert!(validation_result, "Recovery validation should pass for consistent data");
}

#[rstest]
#[tokio::test]
async fn test_recovery_validation_failure(
    #[future] recovery_manager: RecoveryManager,
) {
    let recovery = recovery_manager.await;
    
    // Create order that doesn't exist in persistence
    let fake_order = Order {
        id: Uuid::new_v4(),
        ..test_order()
    };
    
    let validation_result = recovery.validate_recovery(&vec![fake_order]).await.expect("Should validate");
    assert!(!validation_result, "Recovery validation should fail for inconsistent data");
}

#[rstest]
#[tokio::test]
async fn test_create_recovery_checkpoint(
    #[future] recovery_manager: RecoveryManager,
    #[future] test_db: PgPool,
) {
    let recovery = recovery_manager.await;
    let db = test_db.await;
    
    // Create test data
    let persistence = PersistenceManager::new(db.clone());
    persistence.save_order(&test_order()).await.expect("Should save order");
    
    let checkpoint_id = recovery.create_checkpoint().await.expect("Should create checkpoint");
    
    assert!(!checkpoint_id.is_empty(), "Checkpoint ID should not be empty");
    
    // Verify checkpoint was saved (check if table exists and has data)
    // This would require creating the recovery_checkpoints table in migrations
}

// Error handling tests

#[rstest]
#[tokio::test]
async fn test_persistence_with_invalid_data() {
    // Test with invalid database connection
    let invalid_pool = PgPool::connect("postgresql://invalid:invalid@localhost/nonexistent").await;
    
    if invalid_pool.is_err() {
        // Expected - connection should fail
        return;
    }
    
    let persistence = PersistenceManager::new(invalid_pool.unwrap());
    let result = persistence.save_order(&test_order()).await;
    
    // Should handle database errors gracefully
    assert!(result.is_err(), "Should fail with database error");
}

#[rstest]
fn test_parse_functions_error_handling() {
    // Test all parse functions with invalid input
    assert!(parse_order_side("Invalid").is_err());
    assert!(parse_order_type("Invalid").is_err());
    assert!(parse_order_status("Invalid").is_err());
    assert!(parse_time_in_force("Invalid").is_err());
    assert!(parse_liquidity("Invalid").is_err());
}

// Performance tests

#[rstest]
#[tokio::test]
async fn test_bulk_order_persistence(
    #[future] persistence_manager: PersistenceManager,
) {
    let persistence = persistence_manager.await;
    
    let start = std::time::Instant::now();
    
    // Save 1000 orders
    for i in 0..1000 {
        let mut order = test_order();
        order.id = Uuid::new_v4();
        order.sequence_number = i;
        order.client_order_id = Some(format!("BULK-{}", i));
        
        persistence.save_order(&order).await.expect("Should save order");
    }
    
    let duration = start.elapsed();
    println!("Saved 1000 orders in {}ms", duration.as_millis());
    
    // Should save 1000 orders in reasonable time
    assert!(duration.as_secs() < 30, "Bulk save should complete in under 30 seconds");
}

#[rstest]
#[tokio::test]
async fn test_bulk_order_loading(
    #[future] persistence_manager: PersistenceManager,
) {
    let persistence = persistence_manager.await;
    
    // Create many active orders
    for i in 0..100 {
        let mut order = test_order();
        order.id = Uuid::new_v4();
        order.status = OrderStatus::Pending;
        order.sequence_number = i;
        order.client_order_id = Some(format!("LOAD-{}", i));
        
        persistence.save_order(&order).await.expect("Should save order");
    }
    
    let start = std::time::Instant::now();
    let active_orders = persistence.load_active_orders().await.expect("Should load active orders");
    let duration = start.elapsed();
    
    println!("Loaded {} active orders in {}ms", active_orders.len(), duration.as_millis());
    
    assert_eq!(active_orders.len(), 100, "Should load all active orders");
    assert!(duration.as_millis() < 1000, "Loading should be fast");
}

#[rstest]
#[tokio::test]
async fn test_recovery_performance(
    #[future] recovery_manager: RecoveryManager,
    #[future] test_db: PgPool,
) {
    let recovery = recovery_manager.await;
    let db = test_db.await;
    let persistence = PersistenceManager::new(db);
    
    // Create many consistent orders and fills
    for i in 0..50 {
        let order = Order {
            id: Uuid::new_v4(),
            status: OrderStatus::PartiallyFilled,
            executed_quantity: Qty::from_i64(1_000),
            remaining_quantity: Qty::from_i64(9_000),
            sequence_number: i,
            client_order_id: Some(format!("RECOVERY-PERF-{}", i)),
            ..test_order()
        };
        
        let fill = Fill {
            id: Uuid::new_v4(),
            order_id: order.id,
            quantity: Qty::from_i64(1_000),
            price: Px::from_i64(1_000_000),
            commission: 10,
            commission_currency: "USDT".to_string(),
            timestamp: Utc::now(),
            liquidity: LiquidityIndicator::Taker,
            execution_id: format!("RECOVERY-PERF-EXEC-{}", i),
        };
        
        persistence.save_order(&order).await.expect("Should save order");
        persistence.save_fill(&fill).await.expect("Should save fill");
    }
    
    let start = std::time::Instant::now();
    let stats = recovery.recover().await.expect("Should complete recovery");
    let duration = start.elapsed();
    
    println!("Recovered {} orders and {} fills in {}ms", 
             stats.orders_recovered, stats.fills_recovered, duration.as_millis());
    
    assert_eq!(stats.orders_recovered, 50, "Should recover all orders");
    assert_eq!(stats.fills_recovered, 50, "Should recover all fills");
    assert_eq!(stats.discrepancies_found, 0, "Should find no discrepancies");
    assert!(duration.as_millis() < 2000, "Recovery should be fast");
}

// Concurrent access tests

#[rstest]
#[tokio::test]
async fn test_concurrent_order_persistence(
    #[future] test_db: PgPool,
) {
    let db = std::sync::Arc::new(test_db.await);
    let mut handles = vec![];
    
    // Spawn multiple tasks persisting orders concurrently
    for i in 0..10 {
        let db_clone = std::sync::Arc::clone(&db);
        let handle = tokio::spawn(async move {
            let persistence = PersistenceManager::new((**db_clone).clone());
            
            for j in 0..10 {
                let mut order = test_order();
                order.id = Uuid::new_v4();
                order.sequence_number = (i * 10 + j) as u64;
                order.client_order_id = Some(format!("CONCURRENT-{}-{}", i, j));
                
                persistence.save_order(&order).await.expect("Should save order");
            }
        });
        handles.push(handle);
    }
    
    // Wait for all tasks to complete
    for handle in handles {
        handle.await.expect("Task should complete successfully");
    }
    
    // Verify all orders were saved
    let persistence = PersistenceManager::new((**db).clone());
    let active_orders = persistence.load_active_orders().await.expect("Should load orders");
    
    assert_eq!(active_orders.len(), 100, "Should have saved 100 orders concurrently");
}

// Integration-style tests combining persistence and recovery

#[rstest]
#[tokio::test]
async fn test_end_to_end_persistence_recovery(
    #[future] test_db: PgPool,
) {
    let db = test_db.await;
    let persistence = PersistenceManager::new(db.clone());
    let recovery = RecoveryManager::new(db.clone());
    
    // Create complete order lifecycle
    let mut order = test_order();
    order.status = OrderStatus::New;
    persistence.save_order(&order).await.expect("Should save initial order");
    
    // Update to pending
    order.status = OrderStatus::Pending;
    persistence.update_order_status(&order).await.expect("Should update status");
    
    // Add partial fill
    let fill = Fill {
        id: Uuid::new_v4(),
        order_id: order.id,
        quantity: Qty::from_i64(4_000),
        price: Px::from_i64(1_000_000),
        commission: 40,
        commission_currency: "USDT".to_string(),
        timestamp: Utc::now(),
        liquidity: LiquidityIndicator::Maker,
        execution_id: "E2E-FILL-001".to_string(),
    };
    
    persistence.save_fill(&fill).await.expect("Should save fill");
    
    // Update order quantities
    order.status = OrderStatus::PartiallyFilled;
    order.executed_quantity = Qty::from_i64(4_000);
    order.remaining_quantity = Qty::from_i64(6_000);
    persistence.update_order_quantities(&order).await.expect("Should update quantities");
    
    // Add amendment
    let amendment = Amendment {
        id: Uuid::new_v4(),
        order_id: order.id,
        new_quantity: Some(Qty::from_i64(12_000)),
        new_price: Some(Px::from_i64(1_005_000)),
        reason: "Market conditions".to_string(),
        timestamp: Utc::now(),
    };
    
    persistence.save_amendment(&amendment).await.expect("Should save amendment");
    
    // Update order with amendment
    order.quantity = Qty::from_i64(12_000);
    order.remaining_quantity = Qty::from_i64(8_000);
    order.price = Some(Px::from_i64(1_005_000));
    order.version = 2;
    persistence.update_order(&order).await.expect("Should update order");
    
    // Run recovery to verify consistency
    let stats = recovery.recover().await.expect("Should complete recovery");
    
    assert_eq!(stats.orders_recovered, 1, "Should recover the order");
    assert_eq!(stats.fills_recovered, 1, "Should recover the fill");
    assert_eq!(stats.discrepancies_found, 0, "Should find no discrepancies in complete lifecycle");
    
    // Validate recovery
    let recovered_orders = vec![order];
    let validation_result = recovery.validate_recovery(&recovered_orders).await.expect("Should validate");
    assert!(validation_result, "Recovery validation should pass");
    
    // Load and verify final state
    let loaded_order = persistence.load_order(order.id).await.expect("Should load").expect("Order should exist");
    assert_eq!(loaded_order.status, OrderStatus::PartiallyFilled);
    assert_eq!(loaded_order.executed_quantity.as_i64(), 4_000);
    assert_eq!(loaded_order.remaining_quantity.as_i64(), 8_000);
    assert_eq!(loaded_order.quantity.as_i64(), 12_000);
    assert_eq!(loaded_order.version, 2);
}