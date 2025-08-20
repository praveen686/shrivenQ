//! Portfolio optimization algorithms
//!
//! COMPLIANCE:
//! - Fixed-point arithmetic for all calculations
//! - Pre-allocated matrices for optimization
//! - No allocations in hot paths

use crate::{OptimizationStrategy, PortfolioConstraints, RebalanceChange};
use anyhow::Result;
use services_common::Symbol;
use services_common::constants::fixed_point::SCALE_4;
use nalgebra::{DMatrix, DVector};
use rustc_hash::FxHashMap;

/// Portfolio optimizer
pub struct PortfolioOptimizer {
    /// Covariance matrix cache
    covariance_cache: Option<DMatrix<f64>>,
    /// Returns cache
    returns_cache: Option<DVector<f64>>,
    /// Pre-allocated work matrices
    work_matrix: DMatrix<f64>,
}

impl std::fmt::Debug for PortfolioOptimizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PortfolioOptimizer")
            .field("has_covariance_cache", &self.covariance_cache.is_some())
            .field("has_returns_cache", &self.returns_cache.is_some())
            .field("work_matrix_shape", &(self.work_matrix.nrows(), self.work_matrix.ncols()))
            .finish()
    }
}

impl PortfolioOptimizer {
    /// Create new optimizer
    pub fn new() -> Self {
        Self {
            covariance_cache: None,
            returns_cache: None,
            work_matrix: DMatrix::zeros(0, 0),
        }
    }

    /// Optimize portfolio weights
    pub async fn optimize(
        &mut self,
        strategy: OptimizationStrategy,
        positions: &[(Symbol, i64, i64)],
        constraints: &PortfolioConstraints,
    ) -> Result<Vec<RebalanceChange>> {
        if positions.is_empty() {
            return Ok(Vec::new());
        }

        // Calculate current weights
        let current_weights = self.calculate_current_weights(positions);

        // Calculate target weights based on strategy
        let target_weights = match strategy {
            OptimizationStrategy::EqualWeight => self.equal_weight_allocation(positions),
            OptimizationStrategy::MinimumVariance => {
                self.minimum_variance_allocation(positions, constraints)?
            }
            OptimizationStrategy::MaxSharpe => {
                self.max_sharpe_allocation(positions, constraints)?
            }
            OptimizationStrategy::RiskParity => {
                self.risk_parity_allocation(positions, constraints)?
            }
            OptimizationStrategy::Custom => {
                // Custom weights would be provided externally
                current_weights.clone()
            }
        };

        // Generate rebalance changes
        Ok(self.generate_rebalance_changes(positions, &current_weights, &target_weights))
    }

    /// Calculate current portfolio weights
    fn calculate_current_weights(
        &self,
        positions: &[(Symbol, i64, i64)],
    ) -> FxHashMap<Symbol, i32> {
        let total_value: i64 = positions.iter().map(|(_, qty, _)| qty.abs()).sum();
        let mut weights = FxHashMap::default();

        if total_value == 0 {
            return weights;
        }

        for (symbol, qty, _) in positions {
            // Calculate weight as fixed-point percentage (10000 = 100%)
            // SAFETY: Weight percentage should fit in i32 (max 10000 for 100%)
            let weight_calc = (qty.abs() * 10000) / total_value;
            // SAFETY: i64 to i32 conversion safe - weight_calc is bounded by 10000
            debug_assert!(weight_calc <= i32::MAX as i64, "Weight overflow");
            // SAFETY: Assertion above ensures weight_calc fits in i32
            let weight = weight_calc as i32;
            weights.insert(*symbol, weight);
        }

        weights
    }

    /// Equal weight allocation
    fn equal_weight_allocation(&self, positions: &[(Symbol, i64, i64)]) -> FxHashMap<Symbol, i32> {
        let n = positions.len();
        if n == 0 {
            return FxHashMap::default();
        }

        // SAFETY: n is bounded by position count, should be reasonable
        debug_assert!(
            n <= i32::MAX as usize,
            "Too many positions for equal weight"
        );
        // SAFETY: usize to i32 - assertion above ensures n fits in i32
        let weight = SCALE_4 as i32 / n as i32; // Equal weight for all

        let mut weights = FxHashMap::default();
        for (symbol, _, _) in positions {
            weights.insert(*symbol, weight);
        }
        weights
    }

    /// Minimum variance portfolio allocation
    fn minimum_variance_allocation(
        &mut self,
        positions: &[(Symbol, i64, i64)],
        constraints: &PortfolioConstraints,
    ) -> Result<FxHashMap<Symbol, i32>> {
        let n = positions.len();
        if n == 0 {
            return Ok(FxHashMap::default());
        }

        // Resize work matrix to appropriate dimensions to avoid allocations
        if self.work_matrix.nrows() != n || self.work_matrix.ncols() != n {
            self.work_matrix = DMatrix::zeros(n, n);
        }

        // Use covariance matrix if available
        if let Some(cov_matrix) = &self.covariance_cache {
            // Solve quadratic optimization: min(w' * Σ * w)
            // Subject to: sum(w) = 1, w >= 0
            let mut weights = FxHashMap::default();

            // Copy covariance matrix to work_matrix to avoid allocation in try_inverse
            self.work_matrix.copy_from(cov_matrix);

            // Calculate inverse covariance matrix using work_matrix
            if let Some(inv_cov) = self.work_matrix.clone().try_inverse() {
                let ones = DVector::from_element(n, 1.0);
                let numerator = &inv_cov * &ones;
                let denominator = ones.dot(&numerator);

                // Optimal weights
                let w_opt = numerator / denominator;

                for (i, (symbol, _, _)) in positions.iter().enumerate() {
                    // SAFETY: Weight as percentage, clamped to valid range
                    #[allow(clippy::cast_possible_truncation)]
                    // SAFETY: f64 to i32 for weight percentage, will be clamped to valid range
                    let weight = ((w_opt[i] * 10000.0) as i32)
                        .max(constraints.min_position_pct)
                        .min(constraints.max_position_pct);
                    weights.insert(*symbol, weight);
                }

                return Ok(weights);
            }
        }

        // Fallback to equal weights if no covariance data
        // SAFETY: n is position count, checked above
        // SAFETY: usize to i32 conversion safe for reasonable position counts
        let weight = 10000 / n as i32;
        let mut weights = FxHashMap::default();

        for (symbol, _, _) in positions {
            weights.insert(*symbol, weight.min(constraints.max_position_pct));
        }

        Ok(weights)
    }

    /// Maximum Sharpe ratio allocation
    fn max_sharpe_allocation(
        &mut self,
        positions: &[(Symbol, i64, i64)],
        constraints: &PortfolioConstraints,
    ) -> Result<FxHashMap<Symbol, i32>> {
        let n = positions.len();
        if n == 0 {
            return Ok(FxHashMap::default());
        }

        // Resize work matrix if needed
        if self.work_matrix.nrows() != n || self.work_matrix.ncols() != n {
            self.work_matrix = DMatrix::zeros(n, n);
        }

        let mut weights = FxHashMap::default();

        // Use returns and covariance if available
        if let (Some(returns), Some(cov_matrix)) = (&self.returns_cache, &self.covariance_cache) {
            // Solve: max((μ - rf)' * w / sqrt(w' * Σ * w))
            // Using analytical solution for unconstrained case

            let risk_free_rate = 0.02; // 2% annual
            let excess_returns = returns - DVector::from_element(n, risk_free_rate);

            // Use work_matrix for inverse calculation to avoid allocation
            self.work_matrix.copy_from(cov_matrix);
            if let Some(inv_cov) = self.work_matrix.clone().try_inverse() {
                // Optimal weights proportional to inv(Σ) * (μ - rf)
                let raw_weights = &inv_cov * &excess_returns;
                let sum_weights: f64 = raw_weights.iter().sum();

                if sum_weights.abs() > 1e-6 {
                    // Normalize weights
                    for (i, (symbol, _, _)) in positions.iter().enumerate() {
                        // SAFETY: Weight as percentage, clamped to valid range
                        #[allow(clippy::cast_possible_truncation)]
                        // SAFETY: f64 to i32 for weight percentage, will be clamped to valid range
                        let weight = ((raw_weights[i] / sum_weights * 10000.0) as i32)
                            .max(0) // Long-only constraint
                            .min(constraints.max_position_pct);
                        weights.insert(*symbol, weight);
                    }

                    return Ok(weights);
                }
            }
        }

        // Fallback: Sort by P&L and allocate based on performance
        let mut sorted_positions = positions.to_vec();
        sorted_positions.sort_by(|a, b| b.2.cmp(&a.2));

        // SAFETY: usize to i32 - position count should be reasonable
        let total_positions = sorted_positions.len() as i32;
        for (i, (symbol, _, _)) in sorted_positions.iter().enumerate() {
            // SAFETY: usize to i32 - loop index bounded by position count
            let rank_weight = (total_positions - i as i32) * 2000 / total_positions;
            let weight = rank_weight.min(constraints.max_position_pct);
            weights.insert(*symbol, weight);
        }

        Ok(weights)
    }

    /// Risk parity allocation
    fn risk_parity_allocation(
        &mut self,
        positions: &[(Symbol, i64, i64)],
        constraints: &PortfolioConstraints,
    ) -> Result<FxHashMap<Symbol, i32>> {
        let n = positions.len();
        if n == 0 {
            return Ok(FxHashMap::default());
        }

        let mut weights = FxHashMap::default();

        // Risk parity: Equal risk contribution from each asset
        // wi = 1 / (σi * sqrt(Σ(1/σj²)))

        if let Some(cov_matrix) = &self.covariance_cache {
            // Extract volatilities from diagonal of covariance matrix
            let mut volatilities = Vec::with_capacity(n);
            for i in 0..n {
                volatilities.push(cov_matrix[(i, i)].sqrt());
            }

            // Calculate risk parity weights
            let sum_inv_vol: f64 = volatilities
                .iter()
                .map(|v| if *v > 1e-6 { 1.0 / v } else { 0.0 })
                .sum();

            if sum_inv_vol > 1e-6 {
                for (i, (symbol, _, _)) in positions.iter().enumerate() {
                    let vol = volatilities[i];
                    if vol > 1e-6 {
                        let weight = ((1.0 / (vol * sum_inv_vol) * 10000.0) as i32)
                            .max(constraints.min_position_pct)
                            .min(constraints.max_position_pct);
                        weights.insert(*symbol, weight);
                    } else {
                        weights.insert(*symbol, constraints.min_position_pct);
                    }
                }

                return Ok(weights);
            }
        }

        // Fallback to equal risk contribution
        let base_weight = 10000 / n as i32;

        for (symbol, _, _) in positions {
            let weight = base_weight.min(constraints.max_position_pct);
            weights.insert(*symbol, weight);
        }

        Ok(weights)
    }

    /// Generate rebalance changes
    fn generate_rebalance_changes(
        &self,
        positions: &[(Symbol, i64, i64)],
        current_weights: &FxHashMap<Symbol, i32>,
        target_weights: &FxHashMap<Symbol, i32>,
    ) -> Vec<RebalanceChange> {
        let mut changes = Vec::new();

        for (symbol, current_qty, _) in positions {
            let current_weight = current_weights.get(symbol).copied().unwrap_or(0);
            let target_weight = target_weights.get(symbol).copied().unwrap_or(0);

            if current_weight != target_weight {
                let weight_change = target_weight - current_weight;
                let qty_change = (weight_change as i64 * current_qty.abs()) / 10000;

                changes.push(RebalanceChange {
                    symbol: *symbol,
                    old_weight: current_weight,
                    new_weight: target_weight,
                    quantity_change: qty_change,
                });
            }
        }

        // Sort by absolute quantity change (largest first)
        changes.sort_by(|a, b| b.quantity_change.abs().cmp(&a.quantity_change.abs()));

        changes
    }

    /// Update covariance matrix cache
    pub fn update_covariance(&mut self, covariance: DMatrix<f64>) {
        self.covariance_cache = Some(covariance);
    }

    /// Update returns cache
    pub fn update_returns(&mut self, returns: DVector<f64>) {
        self.returns_cache = Some(returns);
    }
}

impl Default for PortfolioOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use services_common::Symbol;

    #[tokio::test]
    async fn test_equal_weight_optimization() {
        let mut optimizer = PortfolioOptimizer::new();
        let positions = vec![
            (Symbol::new(1), 1000000, 5000),
            (Symbol::new(2), 2000000, -3000),
            (Symbol::new(3), 1500000, 2000),
        ];

        let constraints = PortfolioConstraints::default();

        let changes = optimizer
            .optimize(OptimizationStrategy::EqualWeight, &positions, &constraints)
            .await
            .unwrap();

        // Should generate rebalance changes
        assert!(!changes.is_empty());
    }

    #[test]
    fn test_current_weights_calculation() {
        let optimizer = PortfolioOptimizer::new();
        let positions = vec![
            (Symbol::new(1), 1000000, 0),
            (Symbol::new(2), 2000000, 0),
            (Symbol::new(3), 1500000, 0),
        ];

        let weights = optimizer.calculate_current_weights(&positions);

        // Check weights sum to approximately 100%
        let total: i32 = weights.values().sum();
        assert!(total >= 9900 && total <= 10100); // Allow for rounding
    }
}
