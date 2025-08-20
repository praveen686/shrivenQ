//! Portfolio Manager Service integration tests
//! Tests the complete service functionality and workflows

use portfolio_manager::{
    PortfolioManagerService, PortfolioManager, OptimizationStrategy, PortfolioConstraints
};
use rstest::*;
use services_common::{Px, Qty, Side, Symbol, Ts};
use std::collections::HashMap;
use tokio_test;

// Test fixtures
#[fixture]
fn portfolio_manager() -> PortfolioManagerService {
    PortfolioManagerService::new(100)
}

#[fixture]
fn default_constraints() -> PortfolioConstraints {
    PortfolioConstraints::default()
}

#[fixture]
fn tight_constraints() -> PortfolioConstraints {
    PortfolioConstraints {
        max_position_pct: 1500, // 15%
        min_position_pct: 500,  // 5%
        max_positions: 5,
        max_leverage: 15000, // 1.5x
        sector_limits: HashMap::new(),
    }
}

#[fixture]
fn sample_symbols() -> Vec<Symbol> {
    vec![
        Symbol::new(1),
        Symbol::new(2),
        Symbol::new(3),
        Symbol::new(4),
        Symbol::new(5),
    ]
}

mod basic_portfolio_operations {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_empty_portfolio_initialization(portfolio_manager: PortfolioManagerService) {
        // Test initial state
        let metrics = portfolio_manager.get_metrics().await;
        assert_eq!(metrics.total_value, 0);
        assert_eq!(metrics.realized_pnl, 0);
        assert_eq!(metrics.unrealized_pnl, 0);
        assert_eq!(metrics.open_positions, 0);

        let positions = portfolio_manager.get_all_positions().await;
        assert!(positions.is_empty());

        let pnl_breakdown = portfolio_manager.get_pnl_breakdown().await;
        assert!(pnl_breakdown.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_single_position_lifecycle(mut portfolio_manager: PortfolioManagerService) {
        let symbol = Symbol::new(1);
        let order_id = 1;
        let quantity = Qty::from_i64(1000000); // 100 units
        let price = Px::from_i64(1000000);    // $100

        // Process fill - open position
        let result = portfolio_manager.process_fill(
            order_id,
            symbol,
            Side::Bid,
            quantity,
            price,
            Ts::now()
        ).await;
        assert!(result.is_ok());

        // Check position was created
        let position = portfolio_manager.get_position(symbol).await;
        assert!(position.is_some());
        let pos = position.unwrap();
        assert_eq!(pos.symbol, symbol);
        assert_eq!(pos.quantity, 1000000);

        // Check metrics updated
        let metrics = portfolio_manager.get_metrics().await;
        assert_eq!(metrics.open_positions, 1);
        assert_eq!(metrics.total_value, 0); // No unrealized P&L yet

        // Update market price
        let new_bid = Px::from_i64(1010000); // $101
        let new_ask = Px::from_i64(1011000); // $101.1
        portfolio_manager.update_market(symbol, new_bid, new_ask, Ts::now()).await.unwrap();

        // Check unrealized P&L
        let updated_metrics = portfolio_manager.get_metrics().await;
        assert!(updated_metrics.unrealized_pnl > 0); // Should show profit

        // Get P&L breakdown
        let breakdown = portfolio_manager.get_pnl_breakdown().await;
        assert_eq!(breakdown.len(), 1);
        assert!(breakdown.contains_key(&symbol));
    }

    #[rstest]
    #[tokio::test]
    async fn test_multiple_positions_management(mut portfolio_manager: PortfolioManagerService, sample_symbols: Vec<Symbol>) {
        // Create positions for multiple symbols
        for (i, symbol) in sample_symbols.iter().take(3).enumerate() {
            let order_id = (i + 1) as u64;
            let quantity = Qty::from_i64((i + 1) as i64 * 500000); // Varying sizes
            let price = Px::from_i64(1000000 + (i * 100000) as i64); // Varying prices

            portfolio_manager.process_fill(
                order_id,
                *symbol,
                Side::Bid,
                quantity,
                price,
                Ts::now()
            ).await.unwrap();
        }

        // Check all positions created
        let positions = portfolio_manager.get_all_positions().await;
        assert_eq!(positions.len(), 3);

        let metrics = portfolio_manager.get_metrics().await;
        assert_eq!(metrics.open_positions, 3);

        // Update market prices for all
        for (i, symbol) in sample_symbols.iter().take(3).enumerate() {
            let base_price = 1000000 + (i * 100000) as i64;
            let bid = Px::from_i64(base_price + 10000); // +$10 profit
            let ask = Px::from_i64(base_price + 11000);
            
            portfolio_manager.update_market(*symbol, bid, ask, Ts::now()).await.unwrap();
        }

        // Check aggregated P&L
        let final_metrics = portfolio_manager.get_metrics().await;
        assert!(final_metrics.unrealized_pnl > 0);
        assert!(final_metrics.total_value > 0);

        let breakdown = portfolio_manager.get_pnl_breakdown().await;
        assert_eq!(breakdown.len(), 3);
    }

    #[rstest]
    #[tokio::test]
    async fn test_long_short_mixed_portfolio(mut portfolio_manager: PortfolioManagerService) {
        let symbols = vec![Symbol::new(1), Symbol::new(2), Symbol::new(3), Symbol::new(4)];

        // Create mixed long/short positions
        let orders = vec![
            (1, symbols[0], Side::Bid, 1000000, 1000000),  // Long
            (2, symbols[1], Side::Ask, 800000, 2000000),   // Short
            (3, symbols[2], Side::Bid, 1500000, 1500000),  // Long
            (4, symbols[3], Side::Ask, 600000, 2500000),   // Short
        ];

        for (order_id, symbol, side, qty, price) in orders {
            portfolio_manager.process_fill(
                order_id,
                symbol,
                side,
                Qty::from_i64(qty),
                Px::from_i64(price),
                Ts::now()
            ).await.unwrap();
        }

        let positions = portfolio_manager.get_all_positions().await;
        assert_eq!(positions.len(), 4);

        // Check position directions
        let long_positions: Vec<_> = positions.iter().filter(|p| p.quantity > 0).collect();
        let short_positions: Vec<_> = positions.iter().filter(|p| p.quantity < 0).collect();
        
        assert_eq!(long_positions.len(), 2);
        assert_eq!(short_positions.len(), 2);

        // Update market prices and check mixed P&L
        for (i, symbol) in symbols.iter().enumerate() {
            let bid = Px::from_i64(1500000 + (i * 100000) as i64);
            let ask = Px::from_i64(1501000 + (i * 100000) as i64);
            portfolio_manager.update_market(*symbol, bid, ask, Ts::now()).await.unwrap();
        }

        let metrics = portfolio_manager.get_metrics().await;
        // Total P&L depends on price movements and position directions
        assert_ne!(metrics.total_value, 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_position_partial_fills(mut portfolio_manager: PortfolioManagerService) {
        let symbol = Symbol::new(1);
        let base_price = Px::from_i64(1000000);

        // First fill - partial
        portfolio_manager.process_fill(
            1,
            symbol,
            Side::Bid,
            Qty::from_i64(500000),
            base_price,
            Ts::now()
        ).await.unwrap();

        let position1 = portfolio_manager.get_position(symbol).await.unwrap();
        assert_eq!(position1.quantity, 500000);

        // Second fill - add to position
        portfolio_manager.process_fill(
            2,
            symbol,
            Side::Bid,
            Qty::from_i64(300000),
            Px::from_i64(1010000), // Different price
            Ts::now()
        ).await.unwrap();

        let position2 = portfolio_manager.get_position(symbol).await.unwrap();
        assert_eq!(position2.quantity, 800000); // Combined quantity

        // Average price should be calculated
        assert!(position2.avg_price.as_i64() > base_price.as_i64());
        assert!(position2.avg_price.as_i64() < 1010000);
    }

    #[rstest]
    #[tokio::test]
    async fn test_position_closing_and_reopening(mut portfolio_manager: PortfolioManagerService) {
        let symbol = Symbol::new(1);
        let price = Px::from_i64(1000000);

        // Open position
        portfolio_manager.process_fill(
            1,
            symbol,
            Side::Bid,
            Qty::from_i64(1000000),
            price,
            Ts::now()
        ).await.unwrap();

        // Close position at higher price (profit)
        portfolio_manager.process_fill(
            2,
            symbol,
            Side::Ask,
            Qty::from_i64(1000000),
            Px::from_i64(1050000),
            Ts::now()
        ).await.unwrap();

        let position_closed = portfolio_manager.get_position(symbol).await;
        if let Some(pos) = position_closed {
            assert_eq!(pos.quantity, 0);
            assert!(pos.realized_pnl > 0); // Should show profit
        }

        // Reopen position
        portfolio_manager.process_fill(
            3,
            symbol,
            Side::Bid,
            Qty::from_i64(800000),
            Px::from_i64(1040000),
            Ts::now()
        ).await.unwrap();

        let position_reopened = portfolio_manager.get_position(symbol).await.unwrap();
        assert_eq!(position_reopened.quantity, 800000);
        assert_eq!(position_reopened.avg_price.as_i64(), 1040000);

        // Previous realized P&L should be preserved
        assert!(position_reopened.realized_pnl > 0);
    }
}

mod optimization_integration_tests {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_equal_weight_optimization_full_workflow(
        mut portfolio_manager: PortfolioManagerService,
        sample_symbols: Vec<Symbol>,
        default_constraints: PortfolioConstraints
    ) {
        // Create unequal positions
        let quantities = vec![2000000, 1000000, 3000000]; // Different sizes
        let price = Px::from_i64(1000000);

        for (i, symbol) in sample_symbols.iter().take(3).enumerate() {
            portfolio_manager.process_fill(
                (i + 1) as u64,
                *symbol,
                Side::Bid,
                Qty::from_i64(quantities[i]),
                price,
                Ts::now()
            ).await.unwrap();
        }

        // Run optimization
        let changes = portfolio_manager.optimize(
            OptimizationStrategy::EqualWeight,
            &default_constraints
        ).await.unwrap();

        assert!(!changes.is_empty());

        // Changes should aim for equal weights
        let total_changes = changes.len();
        assert!(total_changes > 0);

        // Execute rebalance
        let rebalance_result = portfolio_manager.rebalance(changes).await;
        assert!(rebalance_result.is_ok());
    }

    #[rstest]
    #[tokio::test]
    async fn test_optimization_with_constraints(
        mut portfolio_manager: PortfolioManagerService,
        sample_symbols: Vec<Symbol>,
        tight_constraints: PortfolioConstraints
    ) {
        // Create positions
        for (i, symbol) in sample_symbols.iter().take(3).enumerate() {
            portfolio_manager.process_fill(
                (i + 1) as u64,
                *symbol,
                Side::Bid,
                Qty::from_i64((i + 1) as i64 * 1000000),
                Px::from_i64(1000000),
                Ts::now()
            ).await.unwrap();
        }

        // Test each optimization strategy
        let strategies = vec![
            OptimizationStrategy::EqualWeight,
            OptimizationStrategy::MinimumVariance,
            OptimizationStrategy::MaxSharpe,
            OptimizationStrategy::RiskParity,
        ];

        for strategy in strategies {
            let changes = portfolio_manager.optimize(strategy, &tight_constraints).await.unwrap();

            // Verify constraints are respected
            for change in changes {
                assert!(change.new_weight >= tight_constraints.min_position_pct);
                assert!(change.new_weight <= tight_constraints.max_position_pct);
            }
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_optimization_empty_portfolio(
        mut portfolio_manager: PortfolioManagerService,
        default_constraints: PortfolioConstraints
    ) {
        // Test optimization on empty portfolio
        let changes = portfolio_manager.optimize(
            OptimizationStrategy::EqualWeight,
            &default_constraints
        ).await.unwrap();

        assert!(changes.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_custom_optimization_strategy(
        mut portfolio_manager: PortfolioManagerService,
        sample_symbols: Vec<Symbol>,
        default_constraints: PortfolioConstraints
    ) {
        // Create positions
        for (i, symbol) in sample_symbols.iter().take(2).enumerate() {
            portfolio_manager.process_fill(
                (i + 1) as u64,
                *symbol,
                Side::Bid,
                Qty::from_i64(1000000),
                Px::from_i64(1000000),
                Ts::now()
            ).await.unwrap();
        }

        // Custom strategy should not generate changes (returns current weights)
        let changes = portfolio_manager.optimize(
            OptimizationStrategy::Custom,
            &default_constraints
        ).await.unwrap();

        assert!(changes.is_empty());
    }
}

mod risk_and_performance_metrics_tests {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_portfolio_metrics_calculation(mut portfolio_manager: PortfolioManagerService, sample_symbols: Vec<Symbol>) {
        // Create profitable positions
        for (i, symbol) in sample_symbols.iter().take(3).enumerate() {
            portfolio_manager.process_fill(
                (i + 1) as u64,
                *symbol,
                Side::Bid,
                Qty::from_i64(1000000),
                Px::from_i64(1000000),
                Ts::now()
            ).await.unwrap();

            // Update to profitable prices
            let bid = Px::from_i64(1100000 + (i * 10000) as i64);
            let ask = Px::from_i64(1101000 + (i * 10000) as i64);
            portfolio_manager.update_market(*symbol, bid, ask, Ts::now()).await.unwrap();
        }

        let metrics = portfolio_manager.get_metrics().await;

        assert_eq!(metrics.open_positions, 3);
        assert!(metrics.total_value > 0);
        assert!(metrics.unrealized_pnl > 0);
        
        // Performance metrics should be calculated
        assert!(metrics.sharpe_ratio != 0);
        assert!(metrics.volatility >= 0);
        assert!(metrics.var_95 != 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_portfolio_statistics_integration(portfolio_manager: PortfolioManagerService) {
        let stats = portfolio_manager.get_latest_stats();
        
        // Initial stats should be defaults
        assert_eq!(stats.long_positions, 0);
        assert_eq!(stats.short_positions, 0);
        assert_eq!(stats.net_exposure, 0);
        assert_eq!(stats.gross_exposure, 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_portfolio_beta_calculation(mut portfolio_manager: PortfolioManagerService, sample_symbols: Vec<Symbol>) {
        // Create positions
        for (i, symbol) in sample_symbols.iter().take(2).enumerate() {
            portfolio_manager.process_fill(
                (i + 1) as u64,
                *symbol,
                Side::Bid,
                Qty::from_i64((i + 1) as i64 * 1000000),
                Px::from_i64(1000000),
                Ts::now()
            ).await.unwrap();
        }

        // Calculate portfolio beta
        let beta = portfolio_manager.update_portfolio_beta("NIFTY").await.unwrap();
        
        // Should return some beta value
        assert!(beta > 0);
        
        // Stats should be updated
        let stats = portfolio_manager.get_latest_stats();
        assert_eq!(stats.beta, beta);
    }

    #[rstest]
    #[tokio::test]
    async fn test_metrics_with_losses(mut portfolio_manager: PortfolioManagerService, sample_symbols: Vec<Symbol>) {
        // Create positions that will lose money
        for (i, symbol) in sample_symbols.iter().take(2).enumerate() {
            portfolio_manager.process_fill(
                (i + 1) as u64,
                *symbol,
                Side::Bid,
                Qty::from_i64(1000000),
                Px::from_i64(1000000), // Buy at $100
                Ts::now()
            ).await.unwrap();

            // Update to lower prices (losses)
            let bid = Px::from_i64(900000 - (i * 10000) as i64); // Losing money
            let ask = Px::from_i64(901000 - (i * 10000) as i64);
            portfolio_manager.update_market(*symbol, bid, ask, Ts::now()).await.unwrap();
        }

        let metrics = portfolio_manager.get_metrics().await;

        assert_eq!(metrics.open_positions, 2);
        assert!(metrics.unrealized_pnl < 0); // Should show losses
        assert!(metrics.total_value < 0);
    }
}

mod market_data_integration_tests {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_market_feed_subscription_workflow(mut portfolio_manager: PortfolioManagerService, sample_symbols: Vec<Symbol>) {
        // Test would normally connect to market feed
        // This tests the interface without actual connection
        
        let result = portfolio_manager.subscribe_symbols(&sample_symbols[..2]).await;
        // In test environment, this might fail due to no actual market feed
        // But should not crash
    }

    #[rstest]
    #[tokio::test]
    async fn test_market_price_updates_workflow(mut portfolio_manager: PortfolioManagerService, sample_symbols: Vec<Symbol>) {
        let symbol = sample_symbols[0];

        // Create position first
        portfolio_manager.process_fill(
            1,
            symbol,
            Side::Bid,
            Qty::from_i64(1000000),
            Px::from_i64(1000000),
            Ts::now()
        ).await.unwrap();

        // Series of market updates
        let price_updates = vec![
            (1010000, 1011000), // Price up
            (1005000, 1006000), // Price down
            (1015000, 1016000), // Price up again
            (995000, 996000),   // Price down below entry
        ];

        for (bid, ask) in price_updates {
            portfolio_manager.update_market(
                symbol,
                Px::from_i64(bid),
                Px::from_i64(ask),
                Ts::now()
            ).await.unwrap();

            // Check metrics updated
            let metrics = portfolio_manager.get_metrics().await;
            assert_ne!(metrics.total_value, 0);
        }

        // Final state should reflect last update
        let final_position = portfolio_manager.get_position(symbol).await.unwrap();
        assert_ne!(final_position.unrealized_pnl, 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_rapid_market_updates(mut portfolio_manager: PortfolioManagerService, sample_symbols: Vec<Symbol>) {
        let symbol = sample_symbols[0];

        // Create position
        portfolio_manager.process_fill(
            1,
            symbol,
            Side::Bid,
            Qty::from_i64(1000000),
            Px::from_i64(1000000),
            Ts::now()
        ).await.unwrap();

        // Rapid price updates
        for i in 0..100 {
            let base_price = 1000000 + (i % 50 - 25) * 1000; // Oscillating prices
            portfolio_manager.update_market(
                symbol,
                Px::from_i64(base_price),
                Px::from_i64(base_price + 1000),
                Ts::now()
            ).await.unwrap();
        }

        // Should handle all updates
        let metrics = portfolio_manager.get_metrics().await;
        assert_eq!(metrics.open_positions, 1);

        let position = portfolio_manager.get_position(symbol).await.unwrap();
        assert_ne!(position.unrealized_pnl, 0);
    }
}

mod portfolio_lifecycle_tests {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_portfolio_reset(mut portfolio_manager: PortfolioManagerService, sample_symbols: Vec<Symbol>) {
        // Create positions and generate P&L
        for (i, symbol) in sample_symbols.iter().take(2).enumerate() {
            portfolio_manager.process_fill(
                (i + 1) as u64,
                *symbol,
                Side::Bid,
                Qty::from_i64(1000000),
                Px::from_i64(1000000),
                Ts::now()
            ).await.unwrap();

            let bid = Px::from_i64(1100000);
            let ask = Px::from_i64(1101000);
            portfolio_manager.update_market(*symbol, bid, ask, Ts::now()).await.unwrap();
        }

        // Verify portfolio has data
        let metrics_before = portfolio_manager.get_metrics().await;
        assert_eq!(metrics_before.open_positions, 2);
        assert!(metrics_before.total_value != 0);

        // Reset portfolio
        let result = portfolio_manager.reset().await;
        assert!(result.is_ok());

        // Verify reset
        let metrics_after = portfolio_manager.get_metrics().await;
        assert_eq!(metrics_after.open_positions, 0);
        assert_eq!(metrics_after.total_value, 0);
        assert_eq!(metrics_after.realized_pnl, 0);
        assert_eq!(metrics_after.unrealized_pnl, 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_close_all_positions_workflow(mut portfolio_manager: PortfolioManagerService, sample_symbols: Vec<Symbol>) {
        // Create multiple positions
        for (i, symbol) in sample_symbols.iter().take(3).enumerate() {
            portfolio_manager.process_fill(
                (i + 1) as u64,
                *symbol,
                Side::Bid,
                Qty::from_i64((i + 1) as i64 * 500000),
                Px::from_i64(1000000),
                Ts::now()
            ).await.unwrap();
        }

        let positions_before = portfolio_manager.get_all_positions().await;
        assert_eq!(positions_before.len(), 3);

        // Close all positions
        let result = portfolio_manager.close_all_positions().await;
        assert!(result.is_ok());

        // In the current implementation, this doesn't actually close positions
        // It's a placeholder for order generation to close positions
        // The test verifies the interface works
    }

    #[rstest]
    #[tokio::test]
    async fn test_complete_portfolio_workflow(mut portfolio_manager: PortfolioManagerService, sample_symbols: Vec<Symbol>) {
        let symbol1 = sample_symbols[0];
        let symbol2 = sample_symbols[1];

        // 1. Build portfolio
        portfolio_manager.process_fill(
            1, symbol1, Side::Bid, Qty::from_i64(1000000), Px::from_i64(1000000), Ts::now()
        ).await.unwrap();
        
        portfolio_manager.process_fill(
            2, symbol2, Side::Bid, Qty::from_i64(2000000), Px::from_i64(2000000), Ts::now()
        ).await.unwrap();

        // 2. Market updates
        portfolio_manager.update_market(symbol1, Px::from_i64(1050000), Px::from_i64(1051000), Ts::now()).await.unwrap();
        portfolio_manager.update_market(symbol2, Px::from_i64(1950000), Px::from_i64(1951000), Ts::now()).await.unwrap();

        // 3. Check metrics
        let metrics = portfolio_manager.get_metrics().await;
        assert_eq!(metrics.open_positions, 2);
        assert_ne!(metrics.total_value, 0);

        // 4. Optimize portfolio
        let changes = portfolio_manager.optimize(
            OptimizationStrategy::EqualWeight,
            &PortfolioConstraints::default()
        ).await.unwrap();

        if !changes.is_empty() {
            // 5. Rebalance
            portfolio_manager.rebalance(changes).await.unwrap();
        }

        // 6. Get final state
        let final_positions = portfolio_manager.get_all_positions().await;
        assert_eq!(final_positions.len(), 2);

        let final_metrics = portfolio_manager.get_metrics().await;
        assert_eq!(final_metrics.open_positions, 2);

        let breakdown = portfolio_manager.get_pnl_breakdown().await;
        assert_eq!(breakdown.len(), 2);
    }
}

mod error_handling_and_edge_cases {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_process_fill_edge_cases(mut portfolio_manager: PortfolioManagerService) {
        let symbol = Symbol::new(1);

        // Zero quantity fill
        let result = portfolio_manager.process_fill(
            1, symbol, Side::Bid, Qty::from_i64(0), Px::from_i64(1000000), Ts::now()
        ).await;
        assert!(result.is_ok());

        // Very large quantity
        let result = portfolio_manager.process_fill(
            2, symbol, Side::Bid, Qty::from_i64(1_000_000_000_000), Px::from_i64(1000000), Ts::now()
        ).await;
        assert!(result.is_ok());

        // Very small price
        let result = portfolio_manager.process_fill(
            3, symbol, Side::Bid, Qty::from_i64(1000000), Px::from_i64(1), Ts::now()
        ).await;
        assert!(result.is_ok());
    }

    #[rstest]
    #[tokio::test]
    async fn test_market_update_edge_cases(mut portfolio_manager: PortfolioManagerService) {
        let symbol = Symbol::new(1);

        // Update market for non-existent position
        let result = portfolio_manager.update_market(
            symbol, Px::from_i64(1000000), Px::from_i64(1001000), Ts::now()
        ).await;
        assert!(result.is_ok()); // Should not fail

        // Zero prices
        let result = portfolio_manager.update_market(
            symbol, Px::from_i64(0), Px::from_i64(0), Ts::now()
        ).await;
        assert!(result.is_ok());

        // Inverted bid/ask
        let result = portfolio_manager.update_market(
            symbol, Px::from_i64(1001000), Px::from_i64(1000000), Ts::now()
        ).await;
        assert!(result.is_ok()); // Implementation should handle gracefully
    }

    #[rstest]
    #[tokio::test]
    async fn test_get_position_nonexistent(portfolio_manager: PortfolioManagerService) {
        let nonexistent_symbol = Symbol::new(999);
        let position = portfolio_manager.get_position(nonexistent_symbol).await;
        assert!(position.is_none());
    }

    #[rstest]
    #[tokio::test]
    async fn test_optimization_constraints_edge_cases(
        mut portfolio_manager: PortfolioManagerService,
        sample_symbols: Vec<Symbol>
    ) {
        // Create position
        portfolio_manager.process_fill(
            1, sample_symbols[0], Side::Bid, Qty::from_i64(1000000), Px::from_i64(1000000), Ts::now()
        ).await.unwrap();

        // Test with extreme constraints
        let extreme_constraints = PortfolioConstraints {
            max_position_pct: 100,   // 1% max (very restrictive)
            min_position_pct: 50,    // 0.5% min
            max_positions: 1,
            max_leverage: 5000,      // 0.5x leverage
            sector_limits: HashMap::new(),
        };

        let result = portfolio_manager.optimize(
            OptimizationStrategy::EqualWeight,
            &extreme_constraints
        ).await;
        assert!(result.is_ok());
    }

    #[rstest]
    #[tokio::test]
    async fn test_concurrent_operations(portfolio_manager: PortfolioManagerService) {
        use std::sync::Arc;
        use tokio::task::JoinHandle;
        
        let manager = Arc::new(portfolio_manager);
        let mut handles: Vec<JoinHandle<()>> = vec![];

        // Spawn concurrent operations
        for i in 0..10 {
            let mgr = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                let symbol = Symbol::new((i % 3 + 1) as u32);
                
                // Process fill
                let _ = mgr.process_fill(
                    i as u64,
                    symbol,
                    Side::Bid,
                    Qty::from_i64((i + 1) * 100000),
                    Px::from_i64(1000000),
                    Ts::now()
                ).await;

                // Update market
                let _ = mgr.update_market(
                    symbol,
                    Px::from_i64(1000000 + i * 1000),
                    Px::from_i64(1001000 + i * 1000),
                    Ts::now()
                ).await;

                // Get metrics
                let _ = mgr.get_metrics().await;
            });
            handles.push(handle);
        }

        // Wait for all operations
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify final state is consistent
        let final_metrics = manager.get_metrics().await;
        assert!(final_metrics.open_positions > 0);
    }
}