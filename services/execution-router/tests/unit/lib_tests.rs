//! Unit tests for execution-router service
//! 
//! These tests have been migrated from inline test modules to maintain
//! clean separation between production and test code.

use execution_router::*;
use rstest::*;
use anyhow::Result;

#[rstest]
#[tokio::test]
async fn test_order_submission() -> Result<()> {
    let mut router = ExecutionRouterService::new(VenueStrategy::Primary);

    let request = OrderRequest {
        client_order_id: "test_001".to_string(),
        symbol: Symbol::new(1),
        side: Side::Bid,
        quantity: Qty::from_qty_i32(100_0000),
        order_type: OrderType::Limit,
        limit_price: Some(Px::from_price_i32(100_0000)),
        stop_price: None,
        time_in_force: TimeInForce::GTC,
        venue: None,
        strategy_id: "test_strategy".to_string(),
        params: FxHashMap::default(),
    };

    let order_id = router.submit_order(request).await?;
    assert!(order_id.as_u64() > 0);

    let order = router.get_order(order_id).await?;
    assert_eq!(order.status, OrderStatus::Pending);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_order_cancellation() -> Result<()> {
    let mut router = ExecutionRouterService::new(VenueStrategy::Primary);

    let request = OrderRequest {
        client_order_id: "test_002".to_string(),
        symbol: Symbol::new(1),
        side: Side::Ask,
        quantity: Qty::from_qty_i32(50_0000),
        order_type: OrderType::Limit,
        limit_price: Some(Px::from_price_i32(101_0000)),
        stop_price: None,
        time_in_force: TimeInForce::DAY,
        venue: None,
        strategy_id: "test_strategy".to_string(),
        params: FxHashMap::default(),
    };

    let order_id = router.submit_order(request).await?;
    router.cancel_order(order_id).await?;

    let order = router.get_order(order_id).await?;
    assert_eq!(order.status, OrderStatus::Cancelled);
    
    Ok(())
}

#[fixture]
fn test_order_request() -> OrderRequest {
    OrderRequest {
        client_order_id: "test_fixture".to_string(),
        symbol: Symbol::new(1),
        side: Side::Bid,
        quantity: Qty::from_qty_i32(100_0000),
        order_type: OrderType::Limit,
        limit_price: Some(Px::from_price_i32(100_0000)),
        stop_price: None,
        time_in_force: TimeInForce::GTC,
        venue: None,
        strategy_id: "test_strategy".to_string(),
        params: FxHashMap::default(),
    }
}

#[rstest]
#[tokio::test]
async fn test_order_with_fixture(test_order_request: OrderRequest) -> Result<()> {
    let mut router = ExecutionRouterService::new(VenueStrategy::Primary);
    let order_id = router.submit_order(test_order_request).await?;
    assert!(order_id.as_u64() > 0);
    Ok(())
}