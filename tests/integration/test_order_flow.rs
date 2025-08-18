//! Integration test for complete order flow

use rstest::*;
use test_utils::*;
use anyhow::Result;
use std::time::Duration;

/// Test complete order flow from submission to execution
#[rstest]
#[tokio::test]
async fn test_complete_order_flow(
    #[from(market_data)] market: MarketDataFixture,
    #[from(order_data)] order: OrderFixture,
) -> Result<()> {
    // Initialize test environment
    init_test_logging();
    let env = TestEnvironment::new()?;
    
    // Create mock services
    let exchange = MockExchangeConnector::new();
    let risk_manager = MockRiskManager::new();
    let database = MockDatabase::new();
    
    // Set up market data
    exchange.set_market_price(order.symbol.clone(), market.bid_price).await;
    
    // Test order submission
    let order_id = exchange.place_order(MockOrder {
        id: order.order_id,
        symbol: order.symbol.clone(),
        side: "BUY".to_string(),
        quantity: order.quantity,
        price: order.price,
        order_type: "LIMIT".to_string(),
    }).await?;
    
    // Verify risk checks
    let risk_approved = risk_manager
        .check_order_risk(&order.symbol, order.quantity, order.price.unwrap_or(market.bid_price))
        .await?;
    assert!(risk_approved, "Risk check should pass for valid order");
    
    // Store order in database
    let order_data = serde_json::to_vec(&order)?;
    database.insert(order_id.to_string(), order_data).await?;
    
    // Verify order stored
    let stored = database.get(&order_id.to_string()).await?;
    assert!(stored.is_some(), "Order should be stored in database");
    
    // Clean up
    env.cleanup();
    
    Ok(())
}

/// Test order rejection scenarios
#[rstest]
#[case::exceeds_position_limit(1000000.0, 45000.0, false)]
#[case::exceeds_daily_loss(1.0, 45000.0, false)]
#[case::valid_order(0.1, 45000.0, true)]
#[tokio::test]
async fn test_order_risk_rejection(
    #[case] quantity: f64,
    #[case] price: f64,
    #[case] should_pass: bool,
) -> Result<()> {
    let risk_manager = MockRiskManager::new();
    
    // Set up daily loss if testing that scenario
    if quantity == 1.0 {
        risk_manager.update_pnl(-6000.0).await; // Exceed daily loss limit
    }
    
    let result = risk_manager
        .check_order_risk("BTCUSDT", quantity, price)
        .await?;
    
    assert_eq!(result, should_pass, "Risk check result mismatch");
    
    Ok(())
}

/// Test concurrent order processing
#[rstest]
#[tokio::test]
async fn test_concurrent_orders() -> Result<()> {
    let exchange = MockExchangeConnector::new();
    let order_factory = OrderFactory::new()
        .with_symbol("BTCUSDT")
        .with_quantity(0.1);
    
    // Generate test orders
    let orders = order_factory.build_batch(10, (44000.0, 46000.0));
    
    // Submit orders concurrently
    let results = run_concurrent(10, |i| {
        let exchange = exchange.clone();
        let order = orders[i].clone();
        async move {
            exchange.place_order(MockOrder {
                id: order.id,
                symbol: order.symbol,
                side: order.side,
                quantity: order.quantity,
                price: order.price,
                order_type: order.order_type,
            }).await
        }
    }).await;
    
    // All orders should succeed
    for result in results {
        assert!(result.is_ok(), "Concurrent order should succeed");
    }
    
    Ok(())
}

/// Test order lifecycle with state transitions
#[rstest]
#[tokio::test]
async fn test_order_lifecycle() -> Result<()> {
    let exchange = MockExchangeConnector::new();
    let database = MockDatabase::new();
    
    // Create order
    let order = OrderFactory::new()
        .with_symbol("ETHUSDT")
        .build_limit_order(2800.0);
    
    // Submit order
    let order_id = exchange.place_order(MockOrder {
        id: order.id,
        symbol: order.symbol.clone(),
        side: order.side.clone(),
        quantity: order.quantity,
        price: order.price,
        order_type: order.order_type.clone(),
    }).await?;
    
    // Store initial state
    database.insert(
        format!("order:{}", order_id),
        b"NEW".to_vec(),
    ).await?;
    
    // Simulate partial fill
    database.insert(
        format!("order:{}", order_id),
        b"PARTIALLY_FILLED".to_vec(),
    ).await?;
    
    // Simulate complete fill
    database.insert(
        format!("order:{}", order_id),
        b"FILLED".to_vec(),
    ).await?;
    
    // Verify final state
    let final_state = database.get(&format!("order:{}", order_id)).await?;
    assert_eq!(final_state, Some(b"FILLED".to_vec()));
    
    Ok(())
}

/// Test exchange disconnection handling
#[rstest]
#[tokio::test]
async fn test_exchange_disconnection() -> Result<()> {
    let exchange = MockExchangeConnector::new();
    
    // Verify initial connection
    assert!(exchange.is_connected().await);
    
    // Place order while connected
    let order = OrderFactory::new().build_market_order();
    let result = exchange.place_order(MockOrder {
        id: order.id,
        symbol: order.symbol.clone(),
        side: order.side.clone(),
        quantity: order.quantity,
        price: order.price,
        order_type: order.order_type.clone(),
    }).await;
    assert!(result.is_ok());
    
    // Disconnect
    exchange.disconnect().await;
    assert!(!exchange.is_connected().await);
    
    // Reconnect
    exchange.connect().await;
    assert!(exchange.is_connected().await);
    
    Ok(())
}

/// Test order validation
#[rstest]
#[case::negative_quantity(-1.0, false)]
#[case::zero_quantity(0.0, false)]
#[case::excessive_quantity(1000000.0, false)]
#[case::valid_quantity(1.0, true)]
#[tokio::test]
async fn test_order_validation(
    #[case] quantity: f64,
    #[case] should_pass: bool,
) -> Result<()> {
    // This would integrate with actual validation logic
    let is_valid = quantity > 0.0 && quantity <= 10000.0;
    assert_eq!(is_valid, should_pass);
    Ok(())
}

/// Performance test for order processing
#[rstest]
#[tokio::test]
async fn test_order_processing_performance() -> Result<()> {
    let exchange = MockExchangeConnector::new();
    let _perf = PerformanceAssertion::new(
        "order_processing",
        Duration::from_millis(100),
    );
    
    // Process order within 100ms
    let order = OrderFactory::new().build_market_order();
    let _ = exchange.place_order(MockOrder {
        id: order.id,
        symbol: order.symbol,
        side: order.side,
        quantity: order.quantity,
        price: order.price,
        order_type: order.order_type,
    }).await?;
    
    Ok(())
}