//! Integration tests for complete order processing workflow

use chrono::{Duration, Utc};
use rstest::*;
use services_common::{Px, Qty, Symbol};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::time::{sleep, timeout, Duration as TokioDuration};
use uuid::Uuid;

use oms::{
    OrderManagementSystem, OmsConfig, OmsMetricsSnapshot,
    order::{Order, OrderRequest, OrderSide, OrderStatus, OrderType, TimeInForce, Fill, Amendment, LiquidityIndicator},
};

/// Test fixture for creating a test database
#[fixture]
async fn test_db() -> PgPool {
    let pool = sqlx::PgPool::connect("postgresql://test:test@localhost/test_oms_integration")
        .await
        .expect("Failed to connect to test database");
    
    pool
}

/// Test fixture for OMS configuration
#[fixture]
fn oms_config(#[future] test_db: PgPool) -> OmsConfig {
    let db_url = "postgresql://test:test@localhost/test_oms_integration".to_string();
    OmsConfig {
        database_url: db_url,
        max_orders_memory: 10000,
        retention_days: 7,
        enable_audit: true,
        enable_matching: true,
        persist_batch_size: 100,
    }
}

/// Test fixture for OMS instance
#[fixture]
async fn oms_instance(#[future] oms_config: OmsConfig) -> OrderManagementSystem {
    OrderManagementSystem::new(oms_config.await)
        .await
        .expect("Should create OMS instance")
}

/// Test fixture for creating order requests
#[fixture]
fn buy_order_request() -> OrderRequest {
    OrderRequest {
        client_order_id: Some("INTEGRATION-BUY-001".to_string()),
        parent_order_id: None,
        symbol: Symbol(1), // BTC/USDT
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Day,
        quantity: Qty::from_i64(10_000), // 0.0001 BTC
        price: Some(Px::from_i64(50_000_000_000)), // $50,000
        stop_price: None,
        account: "integration_test_account".to_string(),
        exchange: "binance".to_string(),
        strategy_id: Some("integration_test_strategy".to_string()),
        tags: vec!["integration".to_string(), "test".to_string()],
    }
}

#[fixture]
fn sell_order_request() -> OrderRequest {
    OrderRequest {
        client_order_id: Some("INTEGRATION-SELL-001".to_string()),
        parent_order_id: None,
        symbol: Symbol(1),
        side: OrderSide::Sell,
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Day,
        quantity: Qty::from_i64(10_000),
        price: Some(Px::from_i64(50_100_000_000)), // $50,100
        stop_price: None,
        account: "integration_test_account".to_string(),
        exchange: "binance".to_string(),
        strategy_id: Some("integration_test_strategy".to_string()),
        tags: vec!["integration".to_string(), "test".to_string()],
    }
}

#[fixture]
fn market_buy_request() -> OrderRequest {
    OrderRequest {
        client_order_id: Some("INTEGRATION-MARKET-BUY-001".to_string()),
        parent_order_id: None,
        symbol: Symbol(1),
        side: OrderSide::Buy,
        order_type: OrderType::Market,
        time_in_force: TimeInForce::Ioc,
        quantity: Qty::from_i64(5_000),
        price: None,
        stop_price: None,
        account: "integration_test_account".to_string(),
        exchange: "binance".to_string(),
        strategy_id: Some("market_strategy".to_string()),
        tags: vec!["market".to_string(), "integration".to_string()],
    }
}

// Basic workflow tests

#[rstest]
#[tokio::test]
async fn test_create_order_basic_workflow(
    #[future] oms_instance: OrderManagementSystem,
    buy_order_request: OrderRequest,
) {
    let oms = oms_instance.await;
    
    // Create order
    let order = oms.create_order(buy_order_request.clone()).await.expect("Should create order");
    
    // Verify order properties
    assert_eq!(order.client_order_id, buy_order_request.client_order_id);
    assert_eq!(order.symbol, buy_order_request.symbol);
    assert_eq!(order.side, buy_order_request.side);
    assert_eq!(order.order_type, buy_order_request.order_type);
    assert_eq!(order.quantity, buy_order_request.quantity);
    assert_eq!(order.price, buy_order_request.price);
    assert_eq!(order.status, OrderStatus::New);
    assert_eq!(order.executed_quantity, Qty::ZERO);
    assert_eq!(order.remaining_quantity, buy_order_request.quantity);
    assert_eq!(order.version, 1);
    assert!(!order.fills.is_empty() == false); // No fills initially
    
    // Verify order is in active orders
    let active_orders = oms.get_active_orders();
    assert_eq!(active_orders.len(), 1);
    assert_eq!(active_orders[0].id, order.id);
    
    // Verify order can be retrieved by ID
    let retrieved_order = oms.get_order(&order.id).expect("Should retrieve order");
    assert_eq!(retrieved_order.id, order.id);
    
    // Check metrics
    let metrics = oms.get_metrics();
    assert_eq!(metrics.orders_created, 1);
    assert_eq!(metrics.orders_pending, 1);
    assert_eq!(metrics.active_orders, 1);
}

#[rstest]
#[tokio::test]
async fn test_submit_order_workflow(
    #[future] oms_instance: OrderManagementSystem,
    buy_order_request: OrderRequest,
) {
    let oms = oms_instance.await;
    
    // Create and submit order
    let order = oms.create_order(buy_order_request).await.expect("Should create order");
    let submit_result = oms.submit_order(order.id).await;
    
    assert!(submit_result.is_ok(), "Should submit order successfully");
    
    // Verify order status changed
    let updated_order = oms.get_order(&order.id).expect("Should retrieve updated order");
    assert_eq!(updated_order.status, OrderStatus::Pending);
    
    // Check metrics
    let metrics = oms.get_metrics();
    assert_eq!(metrics.orders_pending, 0); // Decremented after submission
}

#[rstest]
#[tokio::test]
async fn test_fill_order_workflow(
    #[future] oms_instance: OrderManagementSystem,
    buy_order_request: OrderRequest,
) {
    let oms = oms_instance.await;
    
    // Create order
    let order = oms.create_order(buy_order_request).await.expect("Should create order");
    
    // Create partial fill
    let fill = Fill {
        id: Uuid::new_v4(),
        order_id: order.id,
        execution_id: "INTEGRATION-FILL-001".to_string(),
        quantity: Qty::from_i64(3_000), // Partial fill
        price: Px::from_i64(50_000_000_000),
        commission: 150_000, // 0.3% of notional
        commission_currency: "USDT".to_string(),
        timestamp: Utc::now(),
        liquidity: LiquidityIndicator::Maker,
    };
    
    // Process fill
    let fill_result = oms.process_fill(order.id, fill.clone()).await;
    assert!(fill_result.is_ok(), "Should process fill successfully");
    
    // Verify order state after fill
    let filled_order = oms.get_order(&order.id).expect("Should retrieve filled order");
    assert_eq!(filled_order.status, OrderStatus::PartiallyFilled);
    assert_eq!(filled_order.executed_quantity.as_i64(), 3_000);
    assert_eq!(filled_order.remaining_quantity.as_i64(), 7_000);
    assert_eq!(filled_order.fills.len(), 1);
    assert_eq!(filled_order.fills[0].id, fill.id);
    
    // Test average fill price calculation
    let avg_price = filled_order.average_fill_price().expect("Should have average price");
    assert_eq!(avg_price.as_i64(), 50_000_000_000);
    
    // Test commission calculation
    assert_eq!(filled_order.total_commission(), 150_000);
    
    // Test fill rate calculation
    assert!((filled_order.fill_rate() - 30.0).abs() < 0.1); // 30% fill rate
    
    // Check metrics
    let metrics = oms.get_metrics();
    assert_eq!(metrics.total_fills, 1);
}

#[rstest]
#[tokio::test]
async fn test_complete_fill_workflow(
    #[future] oms_instance: OrderManagementSystem,
    buy_order_request: OrderRequest,
) {
    let oms = oms_instance.await;
    
    // Create order
    let order = oms.create_order(buy_order_request).await.expect("Should create order");
    
    // Create complete fill
    let fill = Fill {
        id: Uuid::new_v4(),
        order_id: order.id,
        execution_id: "INTEGRATION-COMPLETE-FILL-001".to_string(),
        quantity: Qty::from_i64(10_000), // Complete fill
        price: Px::from_i64(50_000_000_000),
        commission: 500_000,
        commission_currency: "USDT".to_string(),
        timestamp: Utc::now(),
        liquidity: LiquidityIndicator::Taker,
    };
    
    // Process complete fill
    oms.process_fill(order.id, fill).await.expect("Should process complete fill");
    
    // Verify order is fully filled
    let filled_order = oms.get_order(&order.id).expect("Should retrieve filled order");
    assert_eq!(filled_order.status, OrderStatus::Filled);
    assert_eq!(filled_order.executed_quantity, filled_order.quantity);
    assert_eq!(filled_order.remaining_quantity, Qty::ZERO);
    assert_eq!(filled_order.fill_rate(), 100.0);
    
    // Order should no longer be in active orders
    let active_orders = oms.get_active_orders();
    assert!(!active_orders.iter().any(|o| o.id == order.id), "Filled order should not be active");
    
    // Check metrics
    let metrics = oms.get_metrics();
    assert_eq!(metrics.orders_filled, 1);
}

#[rstest]
#[tokio::test]
async fn test_cancel_order_workflow(
    #[future] oms_instance: OrderManagementSystem,
    buy_order_request: OrderRequest,
) {
    let oms = oms_instance.await;
    
    // Create order
    let order = oms.create_order(buy_order_request).await.expect("Should create order");
    
    // Cancel order
    let cancel_result = oms.cancel_order(order.id, "Integration test cancellation".to_string()).await;
    assert!(cancel_result.is_ok(), "Should cancel order successfully");
    
    // Verify order status
    let cancelled_order = oms.get_order(&order.id).expect("Should retrieve cancelled order");
    assert_eq!(cancelled_order.status, OrderStatus::Cancelled);
    
    // Order should no longer be in active orders
    let active_orders = oms.get_active_orders();
    assert!(!active_orders.iter().any(|o| o.id == order.id), "Cancelled order should not be active");
    
    // Check metrics
    let metrics = oms.get_metrics();
    assert_eq!(metrics.orders_cancelled, 1);
}

#[rstest]
#[tokio::test]
async fn test_amend_order_workflow(
    #[future] oms_instance: OrderManagementSystem,
    buy_order_request: OrderRequest,
) {
    let oms = oms_instance.await;
    
    // Create order
    let order = oms.create_order(buy_order_request).await.expect("Should create order");
    
    // Create amendment
    let amendment = Amendment {
        id: Uuid::new_v4(),
        order_id: order.id,
        new_quantity: Some(Qty::from_i64(15_000)), // Increase quantity
        new_price: Some(Px::from_i64(49_500_000_000)), // Better price
        reason: "Market conditions improved".to_string(),
        timestamp: Utc::now(),
    };
    
    // Apply amendment
    let amend_result = oms.amend_order(order.id, amendment.clone()).await;
    assert!(amend_result.is_ok(), "Should amend order successfully");
    
    // Verify amendment applied
    let amended_order = oms.get_order(&order.id).expect("Should retrieve amended order");
    assert_eq!(amended_order.quantity.as_i64(), 15_000);
    assert_eq!(amended_order.remaining_quantity.as_i64(), 15_000);
    assert_eq!(amended_order.price.unwrap().as_i64(), 49_500_000_000);
    assert_eq!(amended_order.version, 2);
    assert_eq!(amended_order.amendments.len(), 1);
    assert_eq!(amended_order.amendments[0].id, amendment.id);
}

// Event subscription tests

#[rstest]
#[tokio::test]
async fn test_order_events_workflow(
    #[future] oms_instance: OrderManagementSystem,
    buy_order_request: OrderRequest,
) {
    let oms = Arc::new(oms_instance.await);
    
    // Subscribe to events
    let mut event_receiver = oms.subscribe();
    
    // Create order in separate task
    let oms_clone = Arc::clone(&oms);
    let order_request_clone = buy_order_request.clone();
    let create_task = tokio::spawn(async move {
        oms_clone.create_order(order_request_clone).await.expect("Should create order")
    });
    
    // Wait for order creation event
    let event = timeout(TokioDuration::from_secs(1), event_receiver.recv())
        .await
        .expect("Should receive event within timeout")
        .expect("Should receive order creation event");
    
    match event {
        oms::OrderEvent::OrderCreated(created_order) => {
            assert_eq!(created_order.client_order_id, buy_order_request.client_order_id);
            assert_eq!(created_order.status, OrderStatus::New);
        }
        _ => panic!("Expected OrderCreated event"),
    }
    
    // Get the created order
    let order = create_task.await.expect("Create task should complete");
    
    // Submit order and wait for status change event
    let oms_clone = Arc::clone(&oms);
    let order_id = order.id;
    let submit_task = tokio::spawn(async move {
        oms_clone.submit_order(order_id).await.expect("Should submit order")
    });
    
    let status_event = timeout(TokioDuration::from_secs(1), event_receiver.recv())
        .await
        .expect("Should receive status change event within timeout")
        .expect("Should receive status change event");
    
    match status_event {
        oms::OrderEvent::OrderStatusChanged { order_id: event_order_id, old_status, new_status, .. } => {
            assert_eq!(event_order_id, order.id);
            assert_eq!(old_status, OrderStatus::New);
            assert_eq!(new_status, OrderStatus::Pending);
        }
        _ => panic!("Expected OrderStatusChanged event"),
    }
    
    submit_task.await.expect("Submit task should complete");
}

// Parent-child order tests

#[rstest]
#[tokio::test]
async fn test_parent_child_order_workflow(
    #[future] oms_instance: OrderManagementSystem,
) {
    let oms = oms_instance.await;
    
    // Create parent TWAP order
    let parent_request = OrderRequest {
        client_order_id: Some("TWAP-PARENT-001".to_string()),
        parent_order_id: None,
        symbol: Symbol(1),
        side: OrderSide::Buy,
        order_type: OrderType::Twap,
        time_in_force: TimeInForce::Day,
        quantity: Qty::from_i64(100_000), // Large order
        price: Some(Px::from_i64(50_000_000_000)),
        stop_price: None,
        account: "algo_account".to_string(),
        exchange: "binance".to_string(),
        strategy_id: Some("twap_strategy".to_string()),
        tags: vec!["algo".to_string(), "parent".to_string()],
    };
    
    let parent_order = oms.create_order(parent_request).await.expect("Should create parent order");
    
    // Create child orders
    let mut child_orders = Vec::new();
    for i in 0..5 {
        let child_request = OrderRequest {
            client_order_id: Some(format!("TWAP-CHILD-{:03}", i)),
            parent_order_id: Some(parent_order.id),
            symbol: Symbol(1),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            time_in_force: TimeInForce::Day,
            quantity: Qty::from_i64(10_000), // 1/10th of parent
            price: Some(Px::from_i64(50_000_000_000 - (i as i64 * 1_000_000))), // Slightly different prices
            stop_price: None,
            account: "algo_account".to_string(),
            exchange: "binance".to_string(),
            strategy_id: Some("twap_strategy".to_string()),
            tags: vec!["algo".to_string(), "child".to_string()],
        };
        
        let child_order = oms.create_order(child_request).await.expect("Should create child order");
        child_orders.push(child_order);
    }
    
    // Verify parent-child relationships
    let retrieved_children = oms.get_child_orders(&parent_order.id);
    assert_eq!(retrieved_children.len(), 5, "Should have 5 child orders");
    
    for child in &retrieved_children {
        assert_eq!(child.parent_order_id, Some(parent_order.id));
        assert_eq!(child.symbol, parent_order.symbol);
        assert_eq!(child.side, parent_order.side);
    }
    
    // Test filling a child order
    let first_child = &child_orders[0];
    let child_fill = Fill {
        id: Uuid::new_v4(),
        order_id: first_child.id,
        execution_id: "CHILD-FILL-001".to_string(),
        quantity: first_child.quantity,
        price: first_child.price.unwrap(),
        commission: 250_000,
        commission_currency: "USDT".to_string(),
        timestamp: Utc::now(),
        liquidity: LiquidityIndicator::Maker,
    };
    
    oms.process_fill(first_child.id, child_fill).await.expect("Should fill child order");
    
    // Verify child order is filled
    let filled_child = oms.get_order(&first_child.id).expect("Should retrieve filled child");
    assert_eq!(filled_child.status, OrderStatus::Filled);
    
    // Parent order should still be active
    let parent_status = oms.get_order(&parent_order.id).expect("Should retrieve parent");
    assert!(parent_status.is_active(), "Parent should still be active");
}

// Multiple symbol tests

#[rstest]
#[tokio::test]
async fn test_multiple_symbols_workflow(
    #[future] oms_instance: OrderManagementSystem,
) {
    let oms = oms_instance.await;
    
    // Create orders for different symbols
    let symbols = vec![Symbol(1), Symbol(2), Symbol(3)]; // BTC, ETH, SOL
    let mut orders = Vec::new();
    
    for (i, symbol) in symbols.iter().enumerate() {
        let request = OrderRequest {
            client_order_id: Some(format!("MULTI-SYMBOL-{}", i)),
            parent_order_id: None,
            symbol: *symbol,
            side: if i % 2 == 0 { OrderSide::Buy } else { OrderSide::Sell },
            order_type: OrderType::Limit,
            time_in_force: TimeInForce::Day,
            quantity: Qty::from_i64((i as i64 + 1) * 5_000),
            price: Some(Px::from_i64(1_000_000_000 * (i as i64 + 1))),
            stop_price: None,
            account: "multi_symbol_account".to_string(),
            exchange: "binance".to_string(),
            strategy_id: Some("multi_symbol_strategy".to_string()),
            tags: vec!["multi".to_string(), format!("symbol_{}", i)],
        };
        
        let order = oms.create_order(request).await.expect("Should create order");
        orders.push(order);
    }
    
    // Verify orders for each symbol
    for (i, symbol) in symbols.iter().enumerate() {
        let symbol_orders = oms.get_orders_by_symbol(*symbol);
        assert_eq!(symbol_orders.len(), 1, "Should have one order for symbol {}", i);
        assert_eq!(symbol_orders[0].symbol, *symbol);
    }
    
    // Verify all active orders
    let active_orders = oms.get_active_orders();
    assert_eq!(active_orders.len(), 3, "Should have 3 active orders");
    
    // Fill one order completely
    let fill = Fill {
        id: Uuid::new_v4(),
        order_id: orders[0].id,
        execution_id: "MULTI-SYMBOL-FILL-001".to_string(),
        quantity: orders[0].quantity,
        price: orders[0].price.unwrap(),
        commission: 500_000,
        commission_currency: "USDT".to_string(),
        timestamp: Utc::now(),
        liquidity: LiquidityIndicator::Taker,
    };
    
    oms.process_fill(orders[0].id, fill).await.expect("Should fill order");
    
    // Verify active orders reduced
    let active_orders_after_fill = oms.get_active_orders();
    assert_eq!(active_orders_after_fill.len(), 2, "Should have 2 active orders after fill");
    
    // Verify specific symbol orders
    let symbol0_orders = oms.get_orders_by_symbol(Symbol(1));
    assert!(symbol0_orders.is_empty() || !symbol0_orders[0].is_active(), "Symbol 0 order should not be active");
}

// Complex workflow tests

#[rstest]
#[tokio::test]
async fn test_complex_order_lifecycle(
    #[future] oms_instance: OrderManagementSystem,
    buy_order_request: OrderRequest,
) {
    let oms = oms_instance.await;
    
    // 1. Create order
    let order = oms.create_order(buy_order_request).await.expect("Should create order");
    assert_eq!(order.status, OrderStatus::New);
    
    // 2. Submit order
    oms.submit_order(order.id).await.expect("Should submit order");
    let submitted_order = oms.get_order(&order.id).expect("Should retrieve order");
    assert_eq!(submitted_order.status, OrderStatus::Pending);
    
    // 3. Amend order (increase quantity and improve price)
    let amendment = Amendment {
        id: Uuid::new_v4(),
        order_id: order.id,
        new_quantity: Some(Qty::from_i64(20_000)),
        new_price: Some(Px::from_i64(49_000_000_000)),
        reason: "Increase position size".to_string(),
        timestamp: Utc::now(),
    };
    
    oms.amend_order(order.id, amendment).await.expect("Should amend order");
    let amended_order = oms.get_order(&order.id).expect("Should retrieve amended order");
    assert_eq!(amended_order.version, 2);
    assert_eq!(amended_order.quantity.as_i64(), 20_000);
    
    // 4. First partial fill
    let fill1 = Fill {
        id: Uuid::new_v4(),
        order_id: order.id,
        execution_id: "COMPLEX-FILL-001".to_string(),
        quantity: Qty::from_i64(8_000),
        price: Px::from_i64(49_000_000_000),
        commission: 392_000,
        commission_currency: "USDT".to_string(),
        timestamp: Utc::now(),
        liquidity: LiquidityIndicator::Maker,
    };
    
    oms.process_fill(order.id, fill1).await.expect("Should process first fill");
    let partially_filled_order = oms.get_order(&order.id).expect("Should retrieve order");
    assert_eq!(partially_filled_order.status, OrderStatus::PartiallyFilled);
    assert_eq!(partially_filled_order.executed_quantity.as_i64(), 8_000);
    assert_eq!(partially_filled_order.remaining_quantity.as_i64(), 12_000);
    
    // 5. Second partial fill
    let fill2 = Fill {
        id: Uuid::new_v4(),
        order_id: order.id,
        execution_id: "COMPLEX-FILL-002".to_string(),
        quantity: Qty::from_i64(7_000),
        price: Px::from_i64(49_100_000_000), // Slightly different price
        commission: 343_700,
        commission_currency: "USDT".to_string(),
        timestamp: Utc::now(),
        liquidity: LiquidityIndicator::Taker,
    };
    
    oms.process_fill(order.id, fill2).await.expect("Should process second fill");
    let more_filled_order = oms.get_order(&order.id).expect("Should retrieve order");
    assert_eq!(more_filled_order.status, OrderStatus::PartiallyFilled);
    assert_eq!(more_filled_order.executed_quantity.as_i64(), 15_000);
    assert_eq!(more_filled_order.remaining_quantity.as_i64(), 5_000);
    assert_eq!(more_filled_order.fills.len(), 2);
    
    // 6. Cancel remaining quantity
    oms.cancel_order(order.id, "Close position early".to_string()).await.expect("Should cancel order");
    let final_order = oms.get_order(&order.id).expect("Should retrieve final order");
    assert_eq!(final_order.status, OrderStatus::Cancelled);
    assert!(!final_order.is_active(), "Cancelled order should not be active");
    
    // Verify final metrics
    let metrics = oms.get_metrics();
    assert_eq!(metrics.orders_created, 1);
    assert_eq!(metrics.orders_cancelled, 1);
    assert_eq!(metrics.total_fills, 2);
    
    // Verify average fill price
    let avg_price = final_order.average_fill_price().expect("Should have average price");
    let expected_avg = (8_000 * 49_000_000_000 + 7_000 * 49_100_000_000) / 15_000;
    assert_eq!(avg_price.as_i64(), expected_avg);
    
    // Verify total commission
    let total_commission = final_order.total_commission();
    assert_eq!(total_commission, 392_000 + 343_700);
}

#[rstest]
#[tokio::test]
async fn test_concurrent_order_operations(
    #[future] oms_instance: OrderManagementSystem,
) {
    let oms = Arc::new(oms_instance.await);
    let mut handles = vec![];
    
    // Concurrently create multiple orders
    for i in 0..10 {
        let oms_clone = Arc::clone(&oms);
        let handle = tokio::spawn(async move {
            let request = OrderRequest {
                client_order_id: Some(format!("CONCURRENT-{}", i)),
                parent_order_id: None,
                symbol: Symbol(1),
                side: if i % 2 == 0 { OrderSide::Buy } else { OrderSide::Sell },
                order_type: OrderType::Limit,
                time_in_force: TimeInForce::Day,
                quantity: Qty::from_i64((i + 1) * 1_000),
                price: Some(Px::from_i64(50_000_000_000 + (i as i64 * 1_000_000))),
                stop_price: None,
                account: format!("concurrent_account_{}", i),
                exchange: "binance".to_string(),
                strategy_id: Some("concurrent_strategy".to_string()),
                tags: vec!["concurrent".to_string()],
            };
            
            oms_clone.create_order(request).await.expect("Should create order")
        });
        handles.push(handle);
    }
    
    // Wait for all orders to be created
    let mut created_orders = Vec::new();
    for handle in handles {
        let order = handle.await.expect("Should create order");
        created_orders.push(order);
    }
    
    assert_eq!(created_orders.len(), 10, "Should create 10 orders concurrently");
    
    // Verify all orders are active
    let active_orders = oms.get_active_orders();
    assert_eq!(active_orders.len(), 10, "All orders should be active");
    
    // Concurrently submit all orders
    let mut submit_handles = vec![];
    for order in &created_orders {
        let oms_clone = Arc::clone(&oms);
        let order_id = order.id;
        let handle = tokio::spawn(async move {
            oms_clone.submit_order(order_id).await.expect("Should submit order")
        });
        submit_handles.push(handle);
    }
    
    // Wait for all submissions
    for handle in submit_handles {
        handle.await.expect("Should submit order");
    }
    
    // Verify all orders are submitted
    for order in &created_orders {
        let submitted_order = oms.get_order(&order.id).expect("Should retrieve order");
        assert_eq!(submitted_order.status, OrderStatus::Pending);
    }
    
    // Check final metrics
    let metrics = oms.get_metrics();
    assert_eq!(metrics.orders_created, 10);
    assert_eq!(metrics.active_orders, 10);
}

// Performance and stress tests

#[rstest]
#[tokio::test]
async fn test_high_throughput_order_creation(
    #[future] oms_instance: OrderManagementSystem,
) {
    let oms = oms_instance.await;
    let start = std::time::Instant::now();
    
    // Create 1000 orders rapidly
    for i in 0..1000 {
        let request = OrderRequest {
            client_order_id: Some(format!("PERF-{:04}", i)),
            parent_order_id: None,
            symbol: Symbol((i % 5) + 1), // Distribute across 5 symbols
            side: if i % 2 == 0 { OrderSide::Buy } else { OrderSide::Sell },
            order_type: OrderType::Limit,
            time_in_force: TimeInForce::Day,
            quantity: Qty::from_i64(1000 + (i % 10000)),
            price: Some(Px::from_i64(1_000_000_000 + (i as i64 * 1000))),
            stop_price: None,
            account: format!("perf_account_{}", i % 10),
            exchange: "binance".to_string(),
            strategy_id: Some("performance_test".to_string()),
            tags: vec!["performance".to_string()],
        };
        
        oms.create_order(request).await.expect("Should create order");
    }
    
    let duration = start.elapsed();
    println!("Created 1000 orders in {}ms", duration.as_millis());
    
    // Should create 1000 orders in reasonable time
    assert!(duration.as_secs() < 30, "Should create 1000 orders in under 30 seconds");
    
    // Verify all orders created
    let active_orders = oms.get_active_orders();
    assert_eq!(active_orders.len(), 1000, "Should have 1000 active orders");
    
    // Check metrics
    let metrics = oms.get_metrics();
    assert_eq!(metrics.orders_created, 1000);
    assert_eq!(metrics.active_orders, 1000);
}

#[rstest]
#[tokio::test]
async fn test_order_recovery_after_restart(
    #[future] oms_config: OmsConfig,
) {
    // Create first OMS instance and some orders
    {
        let oms1 = OrderManagementSystem::new(oms_config.clone()).await.expect("Should create first OMS");
        
        for i in 0..5 {
            let request = OrderRequest {
                client_order_id: Some(format!("RECOVERY-{}", i)),
                parent_order_id: None,
                symbol: Symbol(1),
                side: OrderSide::Buy,
                order_type: OrderType::Limit,
                time_in_force: TimeInForce::Day,
                quantity: Qty::from_i64(10_000),
                price: Some(Px::from_i64(50_000_000_000)),
                stop_price: None,
                account: "recovery_account".to_string(),
                exchange: "binance".to_string(),
                strategy_id: Some("recovery_test".to_string()),
                tags: vec!["recovery".to_string()],
            };
            
            oms1.create_order(request).await.expect("Should create order");
        }
        
        // Verify orders exist
        let active_orders = oms1.get_active_orders();
        assert_eq!(active_orders.len(), 5);
    } // oms1 goes out of scope
    
    // Create second OMS instance (simulating restart)
    let oms2 = OrderManagementSystem::new(oms_config).await.expect("Should create second OMS");
    
    // Should recover orders from database
    let recovered_orders = oms2.get_active_orders();
    assert_eq!(recovered_orders.len(), 5, "Should recover 5 orders after restart");
    
    // Verify order properties are correct
    for order in recovered_orders {
        assert!(order.client_order_id.unwrap().starts_with("RECOVERY-"));
        assert_eq!(order.symbol, Symbol(1));
        assert_eq!(order.side, OrderSide::Buy);
        assert!(order.is_active());
    }
}

// Error handling and edge cases

#[rstest]
#[tokio::test]
async fn test_invalid_order_operations(
    #[future] oms_instance: OrderManagementSystem,
    buy_order_request: OrderRequest,
) {
    let oms = oms_instance.await;
    
    // Create and fill order completely
    let order = oms.create_order(buy_order_request).await.expect("Should create order");
    
    let complete_fill = Fill {
        id: Uuid::new_v4(),
        order_id: order.id,
        execution_id: "COMPLETE-FILL".to_string(),
        quantity: order.quantity,
        price: order.price.unwrap(),
        commission: 500_000,
        commission_currency: "USDT".to_string(),
        timestamp: Utc::now(),
        liquidity: LiquidityIndicator::Maker,
    };
    
    oms.process_fill(order.id, complete_fill).await.expect("Should fill order completely");
    
    // Try to amend filled order (should fail)
    let amendment = Amendment {
        id: Uuid::new_v4(),
        order_id: order.id,
        new_quantity: Some(Qty::from_i64(20_000)),
        new_price: None,
        reason: "Try to amend filled order".to_string(),
        timestamp: Utc::now(),
    };
    
    let amend_result = oms.amend_order(order.id, amendment).await;
    assert!(amend_result.is_err(), "Should not be able to amend filled order");
    
    // Try to cancel filled order (should fail)
    let cancel_result = oms.cancel_order(order.id, "Try to cancel filled order".to_string()).await;
    assert!(cancel_result.is_err(), "Should not be able to cancel filled order");
    
    // Try to submit already filled order (should fail)
    let submit_result = oms.submit_order(order.id).await;
    assert!(submit_result.is_err(), "Should not be able to submit filled order");
}

#[rstest]
#[tokio::test]
async fn test_nonexistent_order_operations(
    #[future] oms_instance: OrderManagementSystem,
) {
    let oms = oms_instance.await;
    
    let fake_order_id = Uuid::new_v4();
    
    // Try operations on non-existent order
    let submit_result = oms.submit_order(fake_order_id).await;
    assert!(submit_result.is_err(), "Should fail to submit non-existent order");
    
    let cancel_result = oms.cancel_order(fake_order_id, "Cancel non-existent".to_string()).await;
    assert!(cancel_result.is_err(), "Should fail to cancel non-existent order");
    
    let amendment = Amendment {
        id: Uuid::new_v4(),
        order_id: fake_order_id,
        new_quantity: Some(Qty::from_i64(1000)),
        new_price: None,
        reason: "Amend non-existent".to_string(),
        timestamp: Utc::now(),
    };
    
    let amend_result = oms.amend_order(fake_order_id, amendment).await;
    assert!(amend_result.is_err(), "Should fail to amend non-existent order");
    
    // Get non-existent order should return None
    let retrieved_order = oms.get_order(&fake_order_id);
    assert!(retrieved_order.is_none(), "Should return None for non-existent order");
}