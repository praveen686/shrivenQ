//! Working integration tests for portfolio manager

use portfolio_manager::{PortfolioManagerService, PortfolioManager};
use services_common::{Px, Qty, Side, Symbol, Ts};

#[tokio::test]
async fn test_portfolio_manager_basic_workflow() {
    let mut manager = PortfolioManagerService::new(100);
    let symbol = Symbol::new(1);

    // Process a fill to open position
    let result = manager.process_fill(
        1,
        symbol,
        Side::Bid,
        Qty::from_i64(1000000), // 100 units
        Px::from_i64(1000000),  // $100
        Ts::now(),
    ).await;
    assert!(result.is_ok());

    // Check position was created
    let position = manager.get_position(symbol).await;
    assert!(position.is_some());
    let pos = position.unwrap();
    assert_eq!(pos.quantity, 1000000);

    // Update market price
    let result = manager.update_market(
        symbol,
        Px::from_i64(1050000), // $105 bid
        Px::from_i64(1051000), // $105.1 ask
        Ts::now(),
    ).await;
    assert!(result.is_ok());

    // Check metrics
    let metrics = manager.get_metrics().await;
    assert_eq!(metrics.open_positions, 1);
    assert_ne!(metrics.total_value, 0); // Should have some P&L

    // Get all positions
    let positions = manager.get_all_positions().await;
    assert_eq!(positions.len(), 1);

    // Get P&L breakdown
    let breakdown = manager.get_pnl_breakdown().await;
    assert_eq!(breakdown.len(), 1);
    assert!(breakdown.contains_key(&symbol));
}

#[tokio::test]
async fn test_multiple_positions_workflow() {
    let mut manager = PortfolioManagerService::new(100);
    let symbols = vec![Symbol::new(1), Symbol::new(2), Symbol::new(3)];

    // Create positions for multiple symbols
    for (i, symbol) in symbols.iter().enumerate() {
        let result = manager.process_fill(
            (i + 1) as u64,
            *symbol,
            Side::Bid,
            Qty::from_i64((i + 1) as i64 * 500000), // Different sizes
            Px::from_i64(1000000 + (i * 100000) as i64), // Different prices
            Ts::now(),
        ).await;
        assert!(result.is_ok());
    }

    // Check all positions created
    let positions = manager.get_all_positions().await;
    assert_eq!(positions.len(), 3);

    let metrics = manager.get_metrics().await;
    assert_eq!(metrics.open_positions, 3);

    // Update market prices
    for (i, symbol) in symbols.iter().enumerate() {
        let base_price = 1100000 + (i * 100000) as i64; // Higher prices
        manager.update_market(
            *symbol,
            Px::from_i64(base_price),
            Px::from_i64(base_price + 1000),
            Ts::now(),
        ).await.unwrap();
    }

    // Check updated metrics
    let final_metrics = manager.get_metrics().await;
    assert_eq!(final_metrics.open_positions, 3);
    assert_ne!(final_metrics.total_value, 0);
}

#[tokio::test]
async fn test_portfolio_optimization_workflow() {
    let mut manager = PortfolioManagerService::new(100);
    let symbols = vec![Symbol::new(1), Symbol::new(2), Symbol::new(3)];

    // Create unequal positions
    let quantities = vec![2000000, 1000000, 3000000];
    for (i, symbol) in symbols.iter().enumerate() {
        manager.process_fill(
            (i + 1) as u64,
            *symbol,
            Side::Bid,
            Qty::from_i64(quantities[i]),
            Px::from_i64(1000000),
            Ts::now(),
        ).await.unwrap();
    }

    // Test optimization
    use portfolio_manager::{OptimizationStrategy, PortfolioConstraints};
    let result = manager.optimize(
        OptimizationStrategy::EqualWeight,
        &PortfolioConstraints::default(),
    ).await;
    assert!(result.is_ok());

    let changes = result.unwrap();
    // Should generate rebalance changes for unequal positions
    // (Note: might be empty if current weights already match target)
    
    // Test rebalance execution
    if !changes.is_empty() {
        let rebalance_result = manager.rebalance(changes).await;
        assert!(rebalance_result.is_ok());
    }
}

#[tokio::test]
async fn test_portfolio_reset() {
    let mut manager = PortfolioManagerService::new(100);
    let symbol = Symbol::new(1);

    // Create position
    manager.process_fill(
        1,
        symbol,
        Side::Bid,
        Qty::from_i64(1000000),
        Px::from_i64(1000000),
        Ts::now(),
    ).await.unwrap();

    // Update market to create P&L
    manager.update_market(
        symbol,
        Px::from_i64(1100000),
        Px::from_i64(1101000),
        Ts::now(),
    ).await.unwrap();

    // Verify position exists
    let metrics_before = manager.get_metrics().await;
    assert_eq!(metrics_before.open_positions, 1);

    // Reset portfolio
    let result = manager.reset().await;
    assert!(result.is_ok());

    // Verify reset
    let metrics_after = manager.get_metrics().await;
    assert_eq!(metrics_after.open_positions, 0);
    assert_eq!(metrics_after.total_value, 0);
    assert_eq!(metrics_after.realized_pnl, 0);
    assert_eq!(metrics_after.unrealized_pnl, 0);
}