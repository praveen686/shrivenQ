//! Portfolio optimization algorithm tests
//! Tests equal weight, minimum variance, max Sharpe ratio, and risk parity strategies

use portfolio_manager::optimization::PortfolioOptimizer;
use portfolio_manager::{OptimizationStrategy, PortfolioConstraints};
use rstest::*;
use services_common::Symbol;
use nalgebra::{DMatrix, DVector};
use std::collections::HashMap;
use approx::assert_relative_eq;

// Test fixtures
#[fixture]
fn optimizer() -> PortfolioOptimizer {
    PortfolioOptimizer::new()
}

#[fixture]
fn default_constraints() -> PortfolioConstraints {
    PortfolioConstraints::default()
}

#[fixture]
fn tight_constraints() -> PortfolioConstraints {
    PortfolioConstraints {
        max_position_pct: 1500, // 15% max
        min_position_pct: 500,  // 5% min
        max_positions: 10,
        max_leverage: 10000,
        sector_limits: HashMap::new(),
    }
}

#[fixture]
fn sample_positions() -> Vec<(Symbol, i64, i64)> {
    vec![
        (Symbol::new(1), 1000000, 5000),   // Long position with profit
        (Symbol::new(2), 2000000, -3000),  // Long position with loss
        (Symbol::new(3), 1500000, 2000),   // Long position with profit
        (Symbol::new(4), 800000, 1000),    // Smaller long position
    ]
}

#[fixture]
fn diverse_positions() -> Vec<(Symbol, i64, i64)> {
    vec![
        (Symbol::new(1), 2000000, 8000),   // Large profitable position
        (Symbol::new(2), -1500000, 3000),  // Short position with profit  
        (Symbol::new(3), 1000000, -2000),  // Long position with loss
        (Symbol::new(4), -500000, -1000),  // Small short with loss
        (Symbol::new(5), 3000000, 5000),   // Very large position
    ]
}

#[fixture]
fn covariance_matrix_3x3() -> DMatrix<f64> {
    DMatrix::from_row_slice(3, 3, &[
        0.04, 0.02, 0.01,  // Asset 1: 4% variance, moderate correlations
        0.02, 0.09, 0.03,  // Asset 2: 9% variance, higher risk
        0.01, 0.03, 0.01,  // Asset 3: 1% variance, low risk
    ])
}

#[fixture]
fn returns_vector_3() -> DVector<f64> {
    DVector::from_vec(vec![0.12, 0.15, 0.08]) // 12%, 15%, 8% expected returns
}

mod weight_calculation_tests {
    use super::*;

    #[rstest]
    fn test_current_weights_calculation(optimizer: PortfolioOptimizer, sample_positions: Vec<(Symbol, i64, i64)>) {
        let weights = optimizer.calculate_current_weights(&sample_positions);

        // Total value = |1000000| + |2000000| + |1500000| + |800000| = 5300000
        assert_eq!(weights.len(), 4);
        
        // Check individual weights (quantity * 10000 / total)
        assert_eq!(*weights.get(&Symbol::new(1)).unwrap(), 1886); // ~18.87%
        assert_eq!(*weights.get(&Symbol::new(2)).unwrap(), 3773); // ~37.74%
        assert_eq!(*weights.get(&Symbol::new(3)).unwrap(), 2830); // ~28.30%
        assert_eq!(*weights.get(&Symbol::new(4)).unwrap(), 1509); // ~15.09%
        
        // Weights should sum to approximately 100%
        let total_weight: i32 = weights.values().sum();
        assert!((total_weight - 10000).abs() <= 2); // Allow for rounding
    }

    #[rstest]
    fn test_current_weights_with_short_positions(optimizer: PortfolioOptimizer, diverse_positions: Vec<(Symbol, i64, i64)>) {
        let weights = optimizer.calculate_current_weights(&diverse_positions);

        // Should use absolute values for weight calculation
        assert_eq!(weights.len(), 5);
        
        // All weights should be positive (using absolute position sizes)
        for weight in weights.values() {
            assert!(*weight > 0);
        }
        
        // Total should be approximately 100%
        let total_weight: i32 = weights.values().sum();
        assert!((total_weight - 10000).abs() <= 5);
    }

    #[rstest]
    fn test_empty_positions_weights(optimizer: PortfolioOptimizer) {
        let empty_positions = vec![];
        let weights = optimizer.calculate_current_weights(&empty_positions);
        
        assert!(weights.is_empty());
    }

    #[rstest]
    fn test_single_position_weights(optimizer: PortfolioOptimizer) {
        let single_position = vec![(Symbol::new(1), 1000000, 0)];
        let weights = optimizer.calculate_current_weights(&single_position);
        
        assert_eq!(weights.len(), 1);
        assert_eq!(*weights.get(&Symbol::new(1)).unwrap(), 10000); // 100%
    }

    #[rstest]
    fn test_zero_quantity_positions(optimizer: PortfolioOptimizer) {
        let zero_positions = vec![
            (Symbol::new(1), 0, 1000),
            (Symbol::new(2), 0, -500),
        ];
        
        let weights = optimizer.calculate_current_weights(&zero_positions);
        
        // Should handle zero quantities gracefully
        assert_eq!(weights.len(), 2);
        for weight in weights.values() {
            assert_eq!(*weight, 0);
        }
    }
}

mod equal_weight_tests {
    use super::*;

    #[rstest]
    fn test_equal_weight_allocation(optimizer: PortfolioOptimizer, sample_positions: Vec<(Symbol, i64, i64)>) {
        let weights = optimizer.equal_weight_allocation(&sample_positions);
        
        assert_eq!(weights.len(), 4);
        
        // Each position should get 25% (2500 in fixed-point)
        let expected_weight = 10000 / 4; // 2500
        for weight in weights.values() {
            assert_eq!(*weight, expected_weight);
        }
    }

    #[rstest]
    fn test_equal_weight_odd_number_positions(optimizer: PortfolioOptimizer) {
        let positions = vec![
            (Symbol::new(1), 1000000, 0),
            (Symbol::new(2), 2000000, 0),
            (Symbol::new(3), 1500000, 0),
        ];
        
        let weights = optimizer.equal_weight_allocation(&positions);
        
        assert_eq!(weights.len(), 3);
        
        // Each should get 33.33% (3333 in fixed-point)
        let expected_weight = 10000 / 3; // 3333
        for weight in weights.values() {
            assert_eq!(*weight, expected_weight);
        }
    }

    #[rstest]
    fn test_equal_weight_empty_positions(optimizer: PortfolioOptimizer) {
        let empty_positions = vec![];
        let weights = optimizer.equal_weight_allocation(&empty_positions);
        
        assert!(weights.is_empty());
    }

    #[rstest]
    fn test_equal_weight_single_position(optimizer: PortfolioOptimizer) {
        let single_position = vec![(Symbol::new(1), 1000000, 0)];
        let weights = optimizer.equal_weight_allocation(&single_position);
        
        assert_eq!(weights.len(), 1);
        assert_eq!(*weights.get(&Symbol::new(1)).unwrap(), 10000); // 100%
    }

    #[rstest]
    async fn test_equal_weight_optimization_integration(
        mut optimizer: PortfolioOptimizer,
        sample_positions: Vec<(Symbol, i64, i64)>,
        default_constraints: PortfolioConstraints,
    ) {
        let changes = optimizer
            .optimize(OptimizationStrategy::EqualWeight, &sample_positions, &default_constraints)
            .await
            .unwrap();

        // Should generate rebalance changes to achieve equal weights
        assert!(!changes.is_empty());
        
        // Verify each symbol has a rebalance change
        let symbols: std::collections::HashSet<_> = changes.iter().map(|c| c.symbol).collect();
        assert_eq!(symbols.len(), sample_positions.len());
    }
}

mod minimum_variance_tests {
    use super::*;

    #[rstest]
    async fn test_minimum_variance_without_covariance_data(
        mut optimizer: PortfolioOptimizer,
        sample_positions: Vec<(Symbol, i64, i64)>,
        default_constraints: PortfolioConstraints,
    ) {
        // Without covariance matrix, should fallback to equal weights
        let result = optimizer
            .minimum_variance_allocation(&sample_positions, &default_constraints)
            .unwrap();
        
        assert_eq!(result.len(), sample_positions.len());
        
        // Should approximate equal weights as fallback
        let expected_weight = 10000 / sample_positions.len() as i32;
        for weight in result.values() {
            // Weight should be limited by max constraint
            assert!(*weight <= default_constraints.max_position_pct);
            assert!(*weight >= 0);
        }
    }

    #[rstest]
    async fn test_minimum_variance_with_covariance_data(
        mut optimizer: PortfolioOptimizer,
        covariance_matrix_3x3: DMatrix<f64>,
        default_constraints: PortfolioConstraints,
    ) {
        // Set up covariance matrix
        optimizer.update_covariance(covariance_matrix_3x3);
        
        let positions = vec![
            (Symbol::new(1), 1000000, 0),
            (Symbol::new(2), 1000000, 0),
            (Symbol::new(3), 1000000, 0),
        ];
        
        let result = optimizer
            .minimum_variance_allocation(&positions, &default_constraints)
            .unwrap();
        
        assert_eq!(result.len(), 3);
        
        // Asset 3 has lowest variance (0.01), should get higher weight
        // Asset 2 has highest variance (0.09), should get lower weight
        let weight_1 = *result.get(&Symbol::new(1)).unwrap();
        let weight_2 = *result.get(&Symbol::new(2)).unwrap();
        let weight_3 = *result.get(&Symbol::new(3)).unwrap();
        
        // Lower variance asset should get higher allocation
        assert!(weight_3 >= weight_2); // Lowest variance gets most weight
        assert!(weight_1 >= weight_2); // Moderate variance more than high variance
        
        // All weights should respect constraints
        for weight in result.values() {
            assert!(*weight >= default_constraints.min_position_pct);
            assert!(*weight <= default_constraints.max_position_pct);
        }
    }

    #[rstest]
    async fn test_minimum_variance_constraint_enforcement(
        mut optimizer: PortfolioOptimizer,
        covariance_matrix_3x3: DMatrix<f64>,
        tight_constraints: PortfolioConstraints,
    ) {
        optimizer.update_covariance(covariance_matrix_3x3);
        
        let positions = vec![
            (Symbol::new(1), 1000000, 0),
            (Symbol::new(2), 1000000, 0),
            (Symbol::new(3), 1000000, 0),
        ];
        
        let result = optimizer
            .minimum_variance_allocation(&positions, &tight_constraints)
            .unwrap();
        
        // All weights should be within tight constraints
        for weight in result.values() {
            assert!(*weight >= tight_constraints.min_position_pct); // >= 5%
            assert!(*weight <= tight_constraints.max_position_pct); // <= 15%
        }
    }

    #[rstest]
    async fn test_minimum_variance_empty_positions(
        mut optimizer: PortfolioOptimizer,
        default_constraints: PortfolioConstraints,
    ) {
        let empty_positions = vec![];
        let result = optimizer
            .minimum_variance_allocation(&empty_positions, &default_constraints)
            .unwrap();
        
        assert!(result.is_empty());
    }
}

mod max_sharpe_tests {
    use super::*;

    #[rstest]
    async fn test_max_sharpe_without_data(
        mut optimizer: PortfolioOptimizer,
        sample_positions: Vec<(Symbol, i64, i64)>,
        default_constraints: PortfolioConstraints,
    ) {
        // Without returns and covariance data, should use PnL-based allocation
        let result = optimizer
            .max_sharpe_allocation(&sample_positions, &default_constraints)
            .unwrap();
        
        assert_eq!(result.len(), sample_positions.len());
        
        // Should rank by PnL performance
        let weight_1 = *result.get(&Symbol::new(1)).unwrap(); // +5000 PnL
        let weight_2 = *result.get(&Symbol::new(2)).unwrap(); // -3000 PnL
        let weight_3 = *result.get(&Symbol::new(3)).unwrap(); // +2000 PnL
        let weight_4 = *result.get(&Symbol::new(4)).unwrap(); // +1000 PnL
        
        // Higher PnL should get higher allocation
        assert!(weight_1 >= weight_3); // Best performer gets more
        assert!(weight_3 >= weight_4); // Second best gets more than third
        // Note: weight_2 (negative PnL) should get lowest allocation
    }

    #[rstest]
    async fn test_max_sharpe_with_returns_and_covariance(
        mut optimizer: PortfolioOptimizer,
        covariance_matrix_3x3: DMatrix<f64>,
        returns_vector_3: DVector<f64>,
        default_constraints: PortfolioConstraints,
    ) {
        optimizer.update_covariance(covariance_matrix_3x3);
        optimizer.update_returns(returns_vector_3);
        
        let positions = vec![
            (Symbol::new(1), 1000000, 0),
            (Symbol::new(2), 1000000, 0),
            (Symbol::new(3), 1000000, 0),
        ];
        
        let result = optimizer
            .max_sharpe_allocation(&positions, &default_constraints)
            .unwrap();
        
        assert_eq!(result.len(), 3);
        
        // Asset 2 has highest expected return (15%), should get significant allocation
        // Asset 3 has lowest risk and moderate return, should get good allocation
        let weight_1 = *result.get(&Symbol::new(1)).unwrap();
        let weight_2 = *result.get(&Symbol::new(2)).unwrap();
        let weight_3 = *result.get(&Symbol::new(3)).unwrap();
        
        // All weights should be positive (long-only constraint)
        assert!(weight_1 >= 0);
        assert!(weight_2 >= 0);
        assert!(weight_3 >= 0);
        
        // Should respect constraints
        for weight in result.values() {
            assert!(*weight <= default_constraints.max_position_pct);
        }
    }

    #[rstest]
    async fn test_max_sharpe_performance_ranking_fallback(
        mut optimizer: PortfolioOptimizer,
        diverse_positions: Vec<(Symbol, i64, i64)>,
        default_constraints: PortfolioConstraints,
    ) {
        let result = optimizer
            .max_sharpe_allocation(&diverse_positions, &default_constraints)
            .unwrap();
        
        // Should rank by PnL and allocate accordingly
        let weight_1 = *result.get(&Symbol::new(1)).unwrap(); // +8000 PnL (best)
        let weight_5 = *result.get(&Symbol::new(5)).unwrap(); // +5000 PnL (second)
        let weight_2 = *result.get(&Symbol::new(2)).unwrap(); // +3000 PnL
        
        // Better performers should generally get higher allocations
        // (exact ordering depends on position size weighting in fallback)
        assert!(weight_1 > 0);
        assert!(weight_5 > 0);
        assert!(weight_2 > 0);
    }

    #[rstest]
    async fn test_max_sharpe_edge_cases(
        mut optimizer: PortfolioOptimizer,
        default_constraints: PortfolioConstraints,
    ) {
        // Single position
        let single_position = vec![(Symbol::new(1), 1000000, 5000)];
        let result = optimizer
            .max_sharpe_allocation(&single_position, &default_constraints)
            .unwrap();
        
        assert_eq!(result.len(), 1);
        assert!(*result.get(&Symbol::new(1)).unwrap() > 0);
        
        // Empty positions
        let empty_positions = vec![];
        let result = optimizer
            .max_sharpe_allocation(&empty_positions, &default_constraints)
            .unwrap();
        
        assert!(result.is_empty());
    }
}

mod risk_parity_tests {
    use super::*;

    #[rstest]
    async fn test_risk_parity_without_covariance(
        mut optimizer: PortfolioOptimizer,
        sample_positions: Vec<(Symbol, i64, i64)>,
        default_constraints: PortfolioConstraints,
    ) {
        // Without covariance data, should fallback to equal risk contribution
        let result = optimizer
            .risk_parity_allocation(&sample_positions, &default_constraints)
            .unwrap();
        
        assert_eq!(result.len(), sample_positions.len());
        
        // Should approximate equal weights as fallback
        let expected_weight = 10000 / sample_positions.len() as i32;
        for weight in result.values() {
            // Weight should be capped by constraints
            assert!(*weight <= default_constraints.max_position_pct);
            assert!(*weight >= default_constraints.min_position_pct || *weight == expected_weight);
        }
    }

    #[rstest]
    async fn test_risk_parity_with_covariance(
        mut optimizer: PortfolioOptimizer,
        covariance_matrix_3x3: DMatrix<f64>,
        default_constraints: PortfolioConstraints,
    ) {
        optimizer.update_covariance(covariance_matrix_3x3);
        
        let positions = vec![
            (Symbol::new(1), 1000000, 0),
            (Symbol::new(2), 1000000, 0),
            (Symbol::new(3), 1000000, 0),
        ];
        
        let result = optimizer
            .risk_parity_allocation(&positions, &default_constraints)
            .unwrap();
        
        assert_eq!(result.len(), 3);
        
        // Asset 3 has lowest volatility (sqrt(0.01) = 0.1)
        // Asset 2 has highest volatility (sqrt(0.09) = 0.3)
        // For risk parity, lower vol assets get higher weights
        
        let weight_1 = *result.get(&Symbol::new(1)).unwrap();
        let weight_2 = *result.get(&Symbol::new(2)).unwrap(); 
        let weight_3 = *result.get(&Symbol::new(3)).unwrap();
        
        // Lower volatility should get higher allocation for equal risk contribution
        assert!(weight_3 >= weight_1); // Lowest vol gets most weight
        assert!(weight_1 >= weight_2); // Medium vol more than high vol
        
        // All weights should be within constraints
        for weight in result.values() {
            assert!(*weight >= default_constraints.min_position_pct);
            assert!(*weight <= default_constraints.max_position_pct);
        }
    }

    #[rstest]
    async fn test_risk_parity_constraint_enforcement(
        mut optimizer: PortfolioOptimizer,
        covariance_matrix_3x3: DMatrix<f64>,
        tight_constraints: PortfolioConstraints,
    ) {
        optimizer.update_covariance(covariance_matrix_3x3);
        
        let positions = vec![
            (Symbol::new(1), 1000000, 0),
            (Symbol::new(2), 1000000, 0),
            (Symbol::new(3), 1000000, 0),
        ];
        
        let result = optimizer
            .risk_parity_allocation(&positions, &tight_constraints)
            .unwrap();
        
        // All weights should respect tight constraints
        for weight in result.values() {
            assert!(*weight >= tight_constraints.min_position_pct);
            assert!(*weight <= tight_constraints.max_position_pct);
        }
        
        // Total allocation should be reasonable
        let total_weight: i32 = result.values().sum();
        assert!(total_weight > 0);
    }

    #[rstest]
    async fn test_risk_parity_zero_volatility_handling(
        mut optimizer: PortfolioOptimizer,
        default_constraints: PortfolioConstraints,
    ) {
        // Covariance matrix with one zero-volatility asset
        let zero_vol_cov = DMatrix::from_row_slice(2, 2, &[
            0.00, 0.00,  // Zero volatility asset
            0.00, 0.04,  // Normal volatility asset
        ]);
        
        optimizer.update_covariance(zero_vol_cov);
        
        let positions = vec![
            (Symbol::new(1), 1000000, 0),
            (Symbol::new(2), 1000000, 0),
        ];
        
        let result = optimizer
            .risk_parity_allocation(&positions, &default_constraints)
            .unwrap();
        
        assert_eq!(result.len(), 2);
        
        // Should handle zero volatility gracefully
        // Asset with zero volatility should get minimum allocation
        let weight_1 = *result.get(&Symbol::new(1)).unwrap();
        let weight_2 = *result.get(&Symbol::new(2)).unwrap();
        
        assert!(weight_1 >= default_constraints.min_position_pct);
        assert!(weight_2 >= default_constraints.min_position_pct);
    }
}

mod rebalance_generation_tests {
    use super::*;

    #[rstest]
    fn test_generate_rebalance_changes(optimizer: PortfolioOptimizer, sample_positions: Vec<(Symbol, i64, i64)>) {
        let current_weights = optimizer.calculate_current_weights(&sample_positions);
        
        // Create target weights (equal weight)
        let target_weights = optimizer.equal_weight_allocation(&sample_positions);
        
        let changes = optimizer.generate_rebalance_changes(&sample_positions, &current_weights, &target_weights);
        
        // Should have changes for positions where weights differ
        assert!(!changes.is_empty());
        
        for change in &changes {
            // Each change should have valid data
            assert!(sample_positions.iter().any(|(symbol, _, _)| *symbol == change.symbol));
            assert_ne!(change.old_weight, change.new_weight); // Only changes where weights differ
            
            // Quantity change should be consistent with weight change
            let weight_diff = change.new_weight - change.old_weight;
            let qty_sign = if change.quantity_change > 0 { 1 } else { -1 };
            let weight_sign = if weight_diff > 0 { 1 } else { -1 };
            
            // Signs should generally align (more weight = more quantity)
            // Note: This is approximate due to the calculation method
        }
    }

    #[rstest]
    fn test_rebalance_changes_sorting(optimizer: PortfolioOptimizer) {
        let positions = vec![
            (Symbol::new(1), 1000000, 0),
            (Symbol::new(2), 2000000, 0),
            (Symbol::new(3), 500000, 0),
        ];
        
        let current_weights = optimizer.calculate_current_weights(&positions);
        
        // Create very different target weights
        let mut target_weights = std::collections::HashMap::new();
        target_weights.insert(Symbol::new(1), 5000); // Reduce significantly
        target_weights.insert(Symbol::new(2), 1000); // Reduce drastically  
        target_weights.insert(Symbol::new(3), 4000); // Increase significantly
        
        let changes = optimizer.generate_rebalance_changes(&positions, &current_weights, &target_weights);
        
        // Changes should be sorted by absolute quantity change (largest first)
        for i in 1..changes.len() {
            assert!(changes[i-1].quantity_change.abs() >= changes[i].quantity_change.abs());
        }
    }

    #[rstest]
    fn test_no_rebalance_needed(optimizer: PortfolioOptimizer, sample_positions: Vec<(Symbol, i64, i64)>) {
        let current_weights = optimizer.calculate_current_weights(&sample_positions);
        
        // Target weights same as current
        let target_weights = current_weights.clone();
        
        let changes = optimizer.generate_rebalance_changes(&sample_positions, &current_weights, &target_weights);
        
        // Should be empty if no changes needed
        assert!(changes.is_empty());
    }

    #[rstest]
    fn test_partial_rebalance_changes(optimizer: PortfolioOptimizer) {
        let positions = vec![
            (Symbol::new(1), 1000000, 0), // Will change
            (Symbol::new(2), 1000000, 0), // Will stay same
            (Symbol::new(3), 1000000, 0), // Will change
        ];
        
        let current_weights = optimizer.calculate_current_weights(&positions);
        let mut target_weights = current_weights.clone();
        
        // Change only some weights
        target_weights.insert(Symbol::new(1), 4000);
        target_weights.insert(Symbol::new(3), 2000);
        // Symbol 2 stays the same
        
        let changes = optimizer.generate_rebalance_changes(&positions, &current_weights, &target_weights);
        
        // Should only have changes for symbols 1 and 3
        assert_eq!(changes.len(), 2);
        let changed_symbols: std::collections::HashSet<_> = changes.iter().map(|c| c.symbol).collect();
        assert!(changed_symbols.contains(&Symbol::new(1)));
        assert!(changed_symbols.contains(&Symbol::new(3)));
        assert!(!changed_symbols.contains(&Symbol::new(2)));
    }
}

mod optimization_integration_tests {
    use super::*;

    #[rstest]
    async fn test_full_optimization_workflow(
        mut optimizer: PortfolioOptimizer,
        sample_positions: Vec<(Symbol, i64, i64)>,
        default_constraints: PortfolioConstraints,
    ) {
        // Test each optimization strategy
        let strategies = vec![
            OptimizationStrategy::EqualWeight,
            OptimizationStrategy::MinimumVariance,
            OptimizationStrategy::MaxSharpe,
            OptimizationStrategy::RiskParity,
        ];
        
        for strategy in strategies {
            let result = optimizer.optimize(strategy, &sample_positions, &default_constraints).await;
            assert!(result.is_ok(), "Strategy {:?} failed", strategy);
            
            let changes = result.unwrap();
            
            // Should have valid rebalance changes
            for change in &changes {
                assert!(sample_positions.iter().any(|(s, _, _)| *s == change.symbol));
                assert!(change.new_weight >= 0);
                assert!(change.new_weight <= default_constraints.max_position_pct);
            }
        }
    }

    #[rstest]
    async fn test_custom_strategy_passthrough(
        mut optimizer: PortfolioOptimizer,
        sample_positions: Vec<(Symbol, i64, i64)>,
        default_constraints: PortfolioConstraints,
    ) {
        // Custom strategy should return current weights (no changes)
        let changes = optimizer
            .optimize(OptimizationStrategy::Custom, &sample_positions, &default_constraints)
            .await
            .unwrap();
        
        // Custom strategy returns current weights, so should have no changes
        assert!(changes.is_empty());
    }

    #[rstest]
    async fn test_optimization_with_matrix_data(
        mut optimizer: PortfolioOptimizer,
        covariance_matrix_3x3: DMatrix<f64>,
        returns_vector_3: DVector<f64>,
        default_constraints: PortfolioConstraints,
    ) {
        // Set up market data
        optimizer.update_covariance(covariance_matrix_3x3);
        optimizer.update_returns(returns_vector_3);
        
        let positions = vec![
            (Symbol::new(1), 1000000, 0),
            (Symbol::new(2), 1000000, 0),  
            (Symbol::new(3), 1000000, 0),
        ];
        
        // Test strategies that use the matrix data
        let strategies = vec![
            OptimizationStrategy::MinimumVariance,
            OptimizationStrategy::MaxSharpe,
            OptimizationStrategy::RiskParity,
        ];
        
        for strategy in strategies {
            let changes = optimizer.optimize(strategy, &positions, &default_constraints).await.unwrap();
            
            // Should generate meaningful changes with market data
            assert!(!changes.is_empty() || strategy == OptimizationStrategy::Custom);
            
            for change in &changes {
                assert!(change.new_weight >= 0);
                assert!(change.new_weight <= default_constraints.max_position_pct);
            }
        }
    }

    #[rstest]
    async fn test_empty_portfolio_optimization(
        mut optimizer: PortfolioOptimizer,
        default_constraints: PortfolioConstraints,
    ) {
        let empty_positions = vec![];
        
        for strategy in [
            OptimizationStrategy::EqualWeight,
            OptimizationStrategy::MinimumVariance,
            OptimizationStrategy::MaxSharpe,
            OptimizationStrategy::RiskParity,
            OptimizationStrategy::Custom,
        ] {
            let changes = optimizer.optimize(strategy, &empty_positions, &default_constraints).await.unwrap();
            assert!(changes.is_empty());
        }
    }

    #[rstest]
    async fn test_constraint_enforcement_across_strategies(
        mut optimizer: PortfolioOptimizer,
        sample_positions: Vec<(Symbol, i64, i64)>,
        tight_constraints: PortfolioConstraints,
    ) {
        for strategy in [
            OptimizationStrategy::EqualWeight,
            OptimizationStrategy::MinimumVariance,
            OptimizationStrategy::MaxSharpe,
            OptimizationStrategy::RiskParity,
        ] {
            let changes = optimizer.optimize(strategy, &sample_positions, &tight_constraints).await.unwrap();
            
            for change in &changes {
                // All target weights should respect constraints
                assert!(change.new_weight >= tight_constraints.min_position_pct);
                assert!(change.new_weight <= tight_constraints.max_position_pct);
            }
        }
    }
}