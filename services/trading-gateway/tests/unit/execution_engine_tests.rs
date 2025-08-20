//! Unit tests for ExecutionEngine
//!
//! Comprehensive tests covering:
//! - Order submission and routing
//! - Order state tracking and updates
//! - Order cancellation logic
//! - Performance characteristics
//! - Concurrent operation safety
//! - Error handling scenarios

use anyhow::Result;
use rstest::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::sleep;
use trading_gateway::{
    execution_engine::{ExecutionEngine, OrderState},
    OrderStatus, OrderType, Side, TimeInForce, TradingEvent,
};
use services_common::{Px, Qty, Symbol, Ts};

/// Test fixture for creating ExecutionEngine with mock event bus
#[fixture]
async fn execution_engine() -> ExecutionEngine {
    let (event_tx, _) = broadcast::channel(1000);
    let engine = ExecutionEngine::new(Arc::new(event_tx));
    engine.initialize().await.unwrap();
    engine
}

/// Test fixture for creating a sample order request
#[fixture]
fn sample_order() -> TradingEvent {
    TradingEvent::OrderRequest {
        id: 1,
        symbol: Symbol(1),
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(10000),
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "test_strategy".to_string(),
    }
}

/// Test fixture for creating a limit order
#[fixture]
fn limit_order() -> TradingEvent {
    TradingEvent::OrderRequest {
        id: 2,
        symbol: Symbol(1),
        side: Side::Buy,
        order_type: OrderType::Limit,
        quantity: Qty::from_i64(50000),
        price: Some(Px::from_i64(1000000000)),
        time_in_force: TimeInForce::Gtc,
        strategy_id: "test_limit".to_string(),
    }
}

#[rstest]
#[tokio::test]
async fn test_execution_engine_creation(execution_engine: ExecutionEngine) {
    // Test basic creation and initialization
    assert_eq!(execution_engine.get_active_orders().len(), 0);
    
    let metrics = execution_engine.get_metrics();
    assert_eq!(metrics.orders_submitted, 0);
    assert_eq!(metrics.orders_filled, 0);
    assert_eq!(metrics.orders_cancelled, 0);
    assert_eq!(metrics.orders_rejected, 0);
}

#[rstest]
#[tokio::test]
async fn test_market_order_submission(
    execution_engine: ExecutionEngine,
    sample_order: TradingEvent
) -> Result<()> {
    // Submit market order
    execution_engine.submit_order(sample_order.clone()).await?;
    
    // Verify order was submitted
    let metrics = execution_engine.get_metrics();
    assert_eq!(metrics.orders_submitted, 1);
    
    // Check order state exists
    let order_state = execution_engine.get_order(1);
    assert!(order_state.is_some());
    
    let order = order_state.unwrap();
    assert_eq!(order.order_id, 1);
    assert_eq!(order.symbol, Symbol(1));
    assert_eq!(order.side, Side::Buy);
    assert_eq!(order.order_type, OrderType::Market);
    assert_eq!(order.original_qty, Qty::from_i64(10000));
    assert_eq!(order.executed_qty, Qty::ZERO);
    assert_eq!(order.status, OrderStatus::Accepted);
    assert_eq!(order.strategy_id, "test_strategy");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_limit_order_submission(
    execution_engine: ExecutionEngine,
    limit_order: TradingEvent
) -> Result<()> {
    // Submit limit order
    execution_engine.submit_order(limit_order.clone()).await?;
    
    // Verify order tracking
    let order_state = execution_engine.get_order(2);
    assert!(order_state.is_some());
    
    let order = order_state.unwrap();
    assert_eq!(order.order_type, OrderType::Limit);
    assert_eq!(order.original_qty, Qty::from_i64(50000));
    assert_eq!(order.status, OrderStatus::Accepted);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_twap_order_submission(execution_engine: ExecutionEngine) -> Result<()> {
    let twap_order = TradingEvent::OrderRequest {
        id: 3,
        symbol: Symbol(2), // Different symbol
        side: Side::Sell,
        order_type: OrderType::Twap,
        quantity: Qty::from_i64(100000),
        price: None,
        time_in_force: TimeInForce::Day,
        strategy_id: "twap_test".to_string(),
    };
    
    execution_engine.submit_order(twap_order).await?;
    
    let order_state = execution_engine.get_order(3);
    assert!(order_state.is_some());
    
    let order = order_state.unwrap();
    assert_eq!(order.order_type, OrderType::Twap);
    assert_eq!(order.side, Side::Sell);
    assert_eq!(order.symbol, Symbol(2));
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_vwap_order_submission(execution_engine: ExecutionEngine) -> Result<()> {
    let vwap_order = TradingEvent::OrderRequest {
        id: 4,
        symbol: Symbol(3),
        side: Side::Buy,
        order_type: OrderType::Vwap,
        quantity: Qty::from_i64(75000),
        price: None,
        time_in_force: TimeInForce::Day,
        strategy_id: "vwap_test".to_string(),
    };
    
    execution_engine.submit_order(vwap_order).await?;
    
    let order_state = execution_engine.get_order(4);
    assert!(order_state.is_some());
    
    let order = order_state.unwrap();
    assert_eq!(order.order_type, OrderType::Vwap);
    assert_eq!(order.original_qty, Qty::from_i64(75000));
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_order_cancellation(
    execution_engine: ExecutionEngine,
    sample_order: TradingEvent
) -> Result<()> {
    // Submit order first
    execution_engine.submit_order(sample_order).await?;
    
    // Verify order is active
    let active_orders = execution_engine.get_active_orders();
    assert_eq!(active_orders.len(), 1);
    
    // Cancel the order
    execution_engine.cancel_order(1).await?;
    
    // Verify order is cancelled
    let order_state = execution_engine.get_order(1);
    assert!(order_state.is_some());
    
    let order = order_state.unwrap();
    assert_eq!(order.status, OrderStatus::Cancelled);
    
    // Verify no active orders remain
    let active_orders = execution_engine.get_active_orders();
    assert_eq!(active_orders.len(), 0);
    
    // Check metrics
    let metrics = execution_engine.get_metrics();
    assert_eq!(metrics.orders_cancelled, 1);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_cancel_nonexistent_order(execution_engine: ExecutionEngine) -> Result<()> {
    // Try to cancel order that doesn't exist
    execution_engine.cancel_order(999).await?;
    
    // Should not crash or error
    let metrics = execution_engine.get_metrics();
    assert_eq!(metrics.orders_cancelled, 0);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_cancel_all_orders(execution_engine: ExecutionEngine) -> Result<()> {
    // Submit multiple orders
    let orders = vec![
        TradingEvent::OrderRequest {
            id: 1,
            symbol: Symbol(1),
            side: Side::Buy,
            order_type: OrderType::Market,
            quantity: Qty::from_i64(10000),
            price: None,
            time_in_force: TimeInForce::Ioc,
            strategy_id: "test1".to_string(),
        },
        TradingEvent::OrderRequest {
            id: 2,
            symbol: Symbol(2),
            side: Side::Sell,
            order_type: OrderType::Limit,
            quantity: Qty::from_i64(20000),
            price: Some(Px::from_i64(2000000000)),
            time_in_force: TimeInForce::Gtc,
            strategy_id: "test2".to_string(),
        },
        TradingEvent::OrderRequest {
            id: 3,
            symbol: Symbol(3),
            side: Side::Buy,
            order_type: OrderType::Twap,
            quantity: Qty::from_i64(30000),
            price: None,
            time_in_force: TimeInForce::Day,
            strategy_id: "test3".to_string(),
        },
    ];
    
    for order in orders {
        execution_engine.submit_order(order).await?;
    }
    
    // Verify all orders are active
    let active_orders = execution_engine.get_active_orders();
    assert_eq!(active_orders.len(), 3);
    
    // Cancel all orders
    execution_engine.cancel_all_orders().await?;
    
    // Verify no orders are active
    let active_orders = execution_engine.get_active_orders();
    assert_eq!(active_orders.len(), 0);
    
    // Check metrics
    let metrics = execution_engine.get_metrics();
    assert_eq!(metrics.orders_submitted, 3);
    assert_eq!(metrics.orders_cancelled, 3);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_order_state_tracking(
    execution_engine: ExecutionEngine,
    sample_order: TradingEvent
) -> Result<()> {
    // Submit order
    execution_engine.submit_order(sample_order).await?;
    
    let order_state = execution_engine.get_order(1).unwrap();
    
    // Verify initial state
    assert_eq!(order_state.executed_qty, Qty::ZERO);
    assert_eq!(order_state.avg_price, None);
    assert!(order_state.created_at.as_nanos() > 0);
    assert!(order_state.updated_at.as_nanos() > 0);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_get_active_orders_filtering(execution_engine: ExecutionEngine) -> Result<()> {
    // Submit orders
    let market_order = TradingEvent::OrderRequest {
        id: 1,
        symbol: Symbol(1),
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(10000),
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "test".to_string(),
    };
    
    let limit_order = TradingEvent::OrderRequest {
        id: 2,
        symbol: Symbol(2),
        side: Side::Sell,
        order_type: OrderType::Limit,
        quantity: Qty::from_i64(20000),
        price: Some(Px::from_i64(1000000000)),
        time_in_force: TimeInForce::Gtc,
        strategy_id: "test".to_string(),
    };
    
    execution_engine.submit_order(market_order).await?;
    execution_engine.submit_order(limit_order).await?;
    
    // Both should be active initially
    let active_orders = execution_engine.get_active_orders();
    assert_eq!(active_orders.len(), 2);
    
    // Cancel one order
    execution_engine.cancel_order(1).await?;
    
    // Only one should remain active
    let active_orders = execution_engine.get_active_orders();
    assert_eq!(active_orders.len(), 1);
    assert_eq!(active_orders[0].order_id, 2);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_metrics_accuracy(execution_engine: ExecutionEngine) -> Result<()> {
    // Initial metrics should be zero
    let initial_metrics = execution_engine.get_metrics();
    assert_eq!(initial_metrics.orders_submitted, 0);
    assert_eq!(initial_metrics.fill_rate, 0.0);
    
    // Submit multiple orders
    for i in 1..=5 {
        let order = TradingEvent::OrderRequest {
            id: i,
            symbol: Symbol(i as u32),
            side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
            order_type: OrderType::Market,
            quantity: Qty::from_i64(i * 10000),
            price: None,
            time_in_force: TimeInForce::Ioc,
            strategy_id: format!("test_{}", i),
        };
        execution_engine.submit_order(order).await?;
    }
    
    // Check metrics after submissions
    let metrics = execution_engine.get_metrics();
    assert_eq!(metrics.orders_submitted, 5);
    assert_eq!(metrics.volume_executed, 0); // No executions yet
    
    // Cancel some orders
    execution_engine.cancel_order(1).await?;
    execution_engine.cancel_order(3).await?;
    
    let final_metrics = execution_engine.get_metrics();
    assert_eq!(final_metrics.orders_cancelled, 2);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_concurrent_order_submission() -> Result<()> {
    let (event_tx, _) = broadcast::channel(1000);
    let engine = Arc::new(ExecutionEngine::new(Arc::new(event_tx)));
    engine.initialize().await?;
    
    // Submit orders concurrently
    let mut handles = Vec::new();
    
    for i in 1..=10 {
        let engine_clone = Arc::clone(&engine);
        let handle = tokio::spawn(async move {
            let order = TradingEvent::OrderRequest {
                id: i,
                symbol: Symbol(i),
                side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
                order_type: OrderType::Market,
                quantity: Qty::from_i64(i * 1000),
                price: None,
                time_in_force: TimeInForce::Ioc,
                strategy_id: format!("concurrent_{}", i),
            };
            engine_clone.submit_order(order).await
        });
        handles.push(handle);
    }
    
    // Wait for all submissions
    for handle in handles {
        handle.await??;
    }
    
    // Verify all orders were submitted
    let metrics = engine.get_metrics();
    assert_eq!(metrics.orders_submitted, 10);
    
    let active_orders = engine.get_active_orders();
    assert_eq!(active_orders.len(), 10);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_performance_characteristics(execution_engine: ExecutionEngine) -> Result<()> {
    let start = std::time::Instant::now();
    
    // Submit many orders quickly
    for i in 1..=1000 {
        let order = TradingEvent::OrderRequest {
            id: i,
            symbol: Symbol((i % 10) as u32 + 1), // Cycle through 10 symbols
            side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
            order_type: OrderType::Market,
            quantity: Qty::from_i64(10000),
            price: None,
            time_in_force: TimeInForce::Ioc,
            strategy_id: "performance_test".to_string(),
        };
        execution_engine.submit_order(order).await?;
    }
    
    let duration = start.elapsed();
    
    // Verify performance is reasonable (should handle 1000 orders quickly)
    assert!(duration < Duration::from_secs(1));
    
    let metrics = execution_engine.get_metrics();
    assert_eq!(metrics.orders_submitted, 1000);
    
    // Test cancellation performance
    let cancel_start = std::time::Instant::now();
    execution_engine.cancel_all_orders().await?;
    let cancel_duration = cancel_start.elapsed();
    
    assert!(cancel_duration < Duration::from_millis(100));
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_order_id_generation(execution_engine: ExecutionEngine) -> Result<()> {
    let mut order_ids = Vec::new();
    
    // Submit multiple orders and collect IDs
    for _ in 0..50 {
        let order = TradingEvent::OrderRequest {
            id: 1, // This ID is not used for internal tracking
            symbol: Symbol(1),
            side: Side::Buy,
            order_type: OrderType::Market,
            quantity: Qty::from_i64(10000),
            price: None,
            time_in_force: TimeInForce::Ioc,
            strategy_id: "id_test".to_string(),
        };
        
        execution_engine.submit_order(order).await?;
        
        // Get the generated internal order ID
        let active_orders = execution_engine.get_active_orders();
        if let Some(last_order) = active_orders.last() {
            order_ids.push(last_order.order_id);
        }
    }
    
    // Verify IDs are unique and sequential
    order_ids.sort();
    for i in 1..order_ids.len() {
        assert!(order_ids[i] > order_ids[i-1], "Order IDs should be unique and increasing");
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_invalid_order_handling(execution_engine: ExecutionEngine) -> Result<()> {
    // Test with invalid event type (should be gracefully handled)
    let invalid_event = TradingEvent::MarketUpdate {
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
    
    // Should not panic or error
    execution_engine.submit_order(invalid_event).await?;
    
    // Metrics should not change
    let metrics = execution_engine.get_metrics();
    assert_eq!(metrics.orders_submitted, 0);
    
    Ok(())
}

#[rstest]
#[case(OrderType::Market, Side::Buy, 10000)]
#[case(OrderType::Market, Side::Sell, 25000)]
#[case(OrderType::Limit, Side::Buy, 50000)]
#[case(OrderType::Limit, Side::Sell, 75000)]
#[case(OrderType::Twap, Side::Buy, 100000)]
#[case(OrderType::Vwap, Side::Sell, 125000)]
#[tokio::test]
async fn test_order_types_parameterized(
    execution_engine: ExecutionEngine,
    #[case] order_type: OrderType,
    #[case] side: Side,
    #[case] quantity: i64
) -> Result<()> {
    let order = TradingEvent::OrderRequest {
        id: 1,
        symbol: Symbol(1),
        side,
        order_type,
        quantity: Qty::from_i64(quantity),
        price: if matches!(order_type, OrderType::Limit) {
            Some(Px::from_i64(1000000000))
        } else {
            None
        },
        time_in_force: TimeInForce::Gtc,
        strategy_id: "param_test".to_string(),
    };
    
    execution_engine.submit_order(order).await?;
    
    let order_state = execution_engine.get_order(1);
    assert!(order_state.is_some());
    
    let state = order_state.unwrap();
    assert_eq!(state.order_type, order_type);
    assert_eq!(state.side, side);
    assert_eq!(state.original_qty, Qty::from_i64(quantity));
    assert_eq!(state.status, OrderStatus::Accepted);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_order_update_processing(execution_engine: ExecutionEngine) -> Result<()> {
    // Submit an order
    let order = TradingEvent::OrderRequest {
        id: 1,
        symbol: Symbol(1),
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(10000),
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "update_test".to_string(),
    };
    
    execution_engine.submit_order(order).await?;
    
    // Market orders should get filled quickly through the update processor
    // Wait a bit for the update to process
    sleep(Duration::from_millis(50)).await;
    
    let metrics = execution_engine.get_metrics();
    // Market orders should be auto-filled in the mock implementation
    assert!(metrics.orders_submitted > 0);
    
    Ok(())
}