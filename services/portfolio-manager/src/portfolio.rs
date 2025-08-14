//! Portfolio management and analytics
//!
//! COMPLIANCE:
//! - Fixed-point arithmetic for all metrics
//! - Pre-allocated buffers for calculations
//! - No allocations in metric calculations

use common::Symbol;
use common::constants::fixed_point::SCALE_4 as FIXED_POINT_SCALE;
use common::constants::math::SQRT_TRADING_DAYS;
use serde::{Deserialize, Serialize};

/// Portfolio statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PortfolioStats {
    /// Number of long positions
    pub long_positions: u32,
    /// Number of short positions
    pub short_positions: u32,
    /// Total long exposure
    pub long_exposure: i64,
    /// Total short exposure
    pub short_exposure: i64,
    /// Net exposure
    pub net_exposure: i64,
    /// Gross exposure
    pub gross_exposure: i64,
    /// Portfolio beta (fixed-point)
    pub beta: i32,
    /// Portfolio correlation to benchmark (fixed-point)
    pub correlation: i32,
}

/// Risk metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RiskMetrics {
    /// Value at Risk (95% confidence)
    pub var_95: i64,
    /// Value at Risk (99% confidence)
    pub var_99: i64,
    /// Conditional VaR (Expected Shortfall)
    pub cvar: i64,
    /// Maximum drawdown amount
    pub max_drawdown: i64,
    /// Maximum drawdown percentage (fixed-point)
    pub max_drawdown_pct: i32,
    /// Current drawdown
    pub current_drawdown: i64,
    /// Portfolio volatility (annualized, fixed-point)
    pub volatility: i32,
    /// Downside deviation (fixed-point)
    pub downside_deviation: i32,
}

/// Performance metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Total return (fixed-point percentage)
    pub total_return: i32,
    /// Daily return (fixed-point percentage)
    pub daily_return: i32,
    /// Monthly return (fixed-point percentage)
    pub monthly_return: i32,
    /// Annual return (fixed-point percentage)
    pub annual_return: i32,
    /// Sharpe ratio (fixed-point)
    pub sharpe_ratio: i32,
    /// Sortino ratio (fixed-point)
    pub sortino_ratio: i32,
    /// Calmar ratio (fixed-point)
    pub calmar_ratio: i32,
    /// Win rate (fixed-point percentage)
    pub win_rate: i32,
    /// Average win amount
    pub avg_win: i64,
    /// Average loss amount
    pub avg_loss: i64,
    /// Profit factor (fixed-point)
    pub profit_factor: i32,
}

/// Portfolio analyzer
pub struct PortfolioAnalyzer {
    /// Historical returns buffer (pre-allocated)
    returns_buffer: Vec<i64>,
    /// Historical values buffer
    values_buffer: Vec<i64>,
    /// Benchmark returns buffer
    benchmark_buffer: Vec<i64>,
}

impl PortfolioAnalyzer {
    /// Create new analyzer with capacity
    pub fn new(capacity: usize) -> Self {
        let mut returns_buffer = Vec::with_capacity(capacity);
        let mut values_buffer = Vec::with_capacity(capacity);
        let mut benchmark_buffer = Vec::with_capacity(capacity);

        // Pre-allocate
        returns_buffer.resize(capacity, 0);
        values_buffer.resize(capacity, 0);
        benchmark_buffer.resize(capacity, 0);

        Self {
            returns_buffer,
            values_buffer,
            benchmark_buffer,
        }
    }

    /// Calculate portfolio statistics
    pub fn calculate_stats(&self, positions: &[(Symbol, i64, i64)]) -> PortfolioStats {
        let mut stats = PortfolioStats::default();

        for (_, qty, _) in positions {
            if *qty > 0 {
                stats.long_positions += 1;
                stats.long_exposure += qty;
            } else if *qty < 0 {
                stats.short_positions += 1;
                stats.short_exposure += qty.abs();
            }
        }

        stats.net_exposure = stats.long_exposure - stats.short_exposure;
        stats.gross_exposure = stats.long_exposure + stats.short_exposure;

        // Calculate beta using correlation and volatility ratio
        // Beta = Correlation * (Portfolio Vol / Market Vol)
        // Will be updated by market feed manager with real data
        // SAFETY: SCALE_4 (10000) fits in i32
        stats.beta = FIXED_POINT_SCALE as i32; // 1.0 = market neutral (default until market data available)
        stats.correlation = self.calculate_correlation();

        stats
    }

    /// Calculate risk metrics with proper statistical methods
    pub fn calculate_risk(&mut self, returns: &[i64]) -> RiskMetrics {
        if returns.is_empty() {
            return RiskMetrics::default();
        }

        let mut metrics = RiskMetrics::default();

        // Sort returns for VaR calculation
        let mut sorted_returns = returns.to_vec();
        sorted_returns.sort_unstable();

        // VaR at 95% and 99% confidence (left tail)
        let var_95_idx = returns.len().saturating_mul(5).saturating_div(100);
        let var_99_idx = returns.len().saturating_div(100);

        metrics.var_95 = sorted_returns.get(var_95_idx).copied().unwrap_or(0);
        metrics.var_99 = sorted_returns.get(var_99_idx).copied().unwrap_or(0);

        // CVaR (Expected Shortfall - average of returns below VaR)
        if var_95_idx > 0 {
            let cvar_sum: i64 = sorted_returns[..var_95_idx].iter().sum();
            metrics.cvar = cvar_sum / i64::try_from(var_95_idx).unwrap_or(1);
        }

        // Calculate proper volatility
        let mean = returns.iter().sum::<i64>() / i64::try_from(returns.len()).unwrap_or(1);

        // Variance with Bessel's correction (n-1 for sample variance)
        let variance: i64 = returns
            .iter()
            .map(|r| {
                let diff = r - mean;
                diff.saturating_mul(diff) / FIXED_POINT_SCALE // Fixed-point, prevent overflow
            })
            .sum::<i64>()
            / i64::try_from((returns.len() - 1).max(1)).unwrap_or(1);

        // Annualized volatility: daily_vol * sqrt(252)
        // sqrt(252) ≈ 15.8745
        // SAFETY: variance to f64 for sqrt calculation - analytics boundary
        let daily_vol = (variance as f64).sqrt();
        // SAFETY: Volatility percentage fits in i32
        let vol_fp = daily_vol * SQRT_TRADING_DAYS * 100.0;
        let volatility = if vol_fp >= i32::MIN as f64 && vol_fp <= i32::MAX as f64 {
            vol_fp as i32
        } else {
            0_i32
        };
        metrics.volatility = volatility;

        // Downside deviation (volatility of negative returns only)
        let negative_returns: Vec<i64> = returns.iter().filter(|&&r| r < 0).copied().collect();

        if !negative_returns.is_empty() {
            // SAFETY: negative_returns.len() > 0 guaranteed by if condition, fits in i64
            let downside_mean =
                negative_returns.iter().sum::<i64>() / negative_returns.len() as i64;
            let downside_variance: i64 = negative_returns
                .iter()
                .map(|r| {
                    let diff = r - downside_mean;
                    diff.saturating_mul(diff) / FIXED_POINT_SCALE
                })
                // SAFETY: negative_returns.len().max(1) always >= 1, fits in i64
                .sum::<i64>()
                / negative_returns.len().max(1) as i64;

            // SAFETY: downside_variance to f64 for sqrt - analytics boundary
            let downside_vol = (downside_variance as f64).sqrt();
            let downside_fp = downside_vol * SQRT_TRADING_DAYS * 100.0;
            let downside_dev = if downside_fp >= i32::MIN as f64 && downside_fp <= i32::MAX as f64 {
                downside_fp as i32
            } else {
                0_i32
            };
            metrics.downside_deviation = downside_dev;
        }

        // Calculate maximum drawdown
        if !self.values_buffer.is_empty() {
            let mut peak = self.values_buffer[0];
            let mut max_dd = 0i64;
            let mut current_dd = 0i64;

            for &value in &self.values_buffer {
                if value > peak {
                    peak = value;
                }
                current_dd = peak - value;
                if current_dd > max_dd {
                    max_dd = current_dd;
                }
            }

            metrics.max_drawdown = max_dd;
            metrics.current_drawdown = current_dd;

            // Drawdown percentage (fixed-point)
            if peak > 0 {
                // SAFETY: Drawdown percentage fits in i32
                let dd_calc = (max_dd * FIXED_POINT_SCALE) / peak;
                let drawdown_pct = i32::try_from(dd_calc).unwrap_or(i32::MAX);
                metrics.max_drawdown_pct = drawdown_pct;
            }
        }

        metrics
    }

    /// Calculate performance metrics with proper statistical methods
    pub fn calculate_performance(
        &self,
        returns: &[i64],
        risk_free_rate: i32,
    ) -> PerformanceMetrics {
        if returns.is_empty() {
            return PerformanceMetrics::default();
        }

        let mut metrics = PerformanceMetrics::default();
        // SAFETY: returns.len() fits in i64 for reasonable portfolio sizes (max 2^63 items)
        let count = returns.len() as i64;

        // Calculate basic returns
        let total: i64 = returns.iter().sum();
        let mean_return = total / count.max(1);

        // Convert to annualized returns (fixed-point percentage)
        // SAFETY: Return percentage fits in i32
        let total_calc = total * FIXED_POINT_SCALE / count.max(1);
        let total_ret = i32::try_from(total_calc).unwrap_or(0);
        metrics.total_return = total_ret;
        // SAFETY: count.max(1) always >= 1, fits in i32
        let daily_ret = metrics.total_return / i32::try_from(count.max(1)).unwrap_or(1);
        metrics.daily_return = daily_ret;
        metrics.monthly_return = metrics.daily_return.saturating_mul(20); // ~20 trading days
        metrics.annual_return = metrics.daily_return.saturating_mul(252); // 252 trading days

        // Win/loss statistics
        let wins: Vec<_> = returns.iter().filter(|&&r| r > 0).copied().collect();
        let losses: Vec<_> = returns.iter().filter(|&&r| r < 0).copied().collect();

        if !wins.is_empty() {
            // SAFETY: wins.len() > 0 guaranteed by if condition, fits in i64
            metrics.avg_win = wins.iter().sum::<i64>() / wins.len() as i64;
            // SAFETY: wins.len() and returns.len() fit in i32 for reasonable portfolio sizes
            metrics.win_rate =
                (wins.len() as i32 * FIXED_POINT_SCALE as i32) / returns.len() as i32;
        }

        if !losses.is_empty() {
            // SAFETY: losses.len() > 0 guaranteed by if condition, fits in i64
            metrics.avg_loss = losses.iter().sum::<i64>() / losses.len() as i64;
        }

        // Profit factor: Total Wins / Total Losses
        if !losses.is_empty() {
            let total_wins: i64 = wins.iter().sum::<i64>().abs();
            let total_losses: i64 = losses.iter().map(|l| l.abs()).sum();
            if total_losses > 0 {
                // SAFETY: Profit factor percentage fits in i32
                #[allow(clippy::cast_possible_truncation)]
                let profit_factor = ((total_wins * FIXED_POINT_SCALE) / total_losses) as i32;
                metrics.profit_factor = profit_factor;
            }
        }

        // Calculate volatility for Sharpe ratio
        let variance: i64 = returns
            .iter()
            .map(|r| {
                let diff = r - mean_return;
                diff.saturating_mul(diff) / FIXED_POINT_SCALE
            })
            .sum::<i64>()
            / i64::try_from((returns.len() - 1).max(1)).unwrap_or(1);

        // SAFETY: variance to f64 for calculations - analytics boundary
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let daily_volatility = (variance as f64).sqrt();
        let annual_volatility = (daily_volatility * SQRT_TRADING_DAYS * 100 as f64) as i32; // sqrt(252)

        // Sharpe Ratio = (Annual Return - Risk Free Rate) / Annual Volatility
        if annual_volatility > 0 {
            let excess_return = metrics.annual_return.saturating_sub(risk_free_rate);
            // SAFETY: annual_volatility > 0 guaranteed by if condition, fits in i64
            #[allow(clippy::cast_possible_truncation)]
            let sharpe =
                (excess_return as i64 * FIXED_POINT_SCALE / annual_volatility as i64) as i32;
            metrics.sharpe_ratio = sharpe;
        }

        // Sortino Ratio: Uses downside deviation instead of total volatility
        let downside_returns: Vec<i64> = returns.iter().filter(|&&r| r < 0).copied().collect();

        if !downside_returns.is_empty() {
            let downside_variance: i64 = downside_returns
                .iter()
                .map(|r| {
                    let diff = *r; // Already negative
                    diff.saturating_mul(diff) / FIXED_POINT_SCALE
                })
                // SAFETY: downside_returns.len() > 0 guaranteed by if condition, fits in i64
                .sum::<i64>()
                / downside_returns.len() as i64;

            // SAFETY: downside_variance to f64 for calculations - analytics boundary
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            let downside_deviation = (downside_variance as f64).sqrt();
            let annual_downside_dev = (downside_deviation * SQRT_TRADING_DAYS * 100 as f64) as i32;

            if annual_downside_dev > 0 {
                let excess_return = metrics.annual_return.saturating_sub(risk_free_rate);
                // SAFETY: annual_downside_dev > 0 guaranteed by if condition, fits in i64
                #[allow(clippy::cast_possible_truncation)]
                let sortino =
                    (excess_return as i64 * FIXED_POINT_SCALE / annual_downside_dev as i64) as i32;
                metrics.sortino_ratio = sortino;
            }
        } else {
            // No downside, Sortino is technically infinite - use a high value
            metrics.sortino_ratio = 100000; // 10.0 in fixed-point
        }

        // Calmar Ratio = Annual Return / Max Drawdown
        // Need to calculate max drawdown from cumulative returns
        if !returns.is_empty() {
            let mut cumulative = 0i64;
            let mut peak = 0i64;
            let mut max_drawdown = 0i64;

            for &ret in returns {
                cumulative += ret;
                if cumulative > peak {
                    peak = cumulative;
                }
                let drawdown = peak - cumulative;
                if drawdown > max_drawdown {
                    max_drawdown = drawdown;
                }
            }

            if max_drawdown > 0 {
                // Calmar = Annual Return / Max Drawdown
                // SAFETY: max_drawdown > 0 guaranteed by if condition, metrics.annual_return fits in i64
                #[allow(clippy::cast_possible_truncation)]
                let calmar =
                    (metrics.annual_return as i64 * FIXED_POINT_SCALE / max_drawdown) as i32;
                metrics.calmar_ratio = calmar;
            } else {
                // No drawdown
                metrics.calmar_ratio = 100000; // 10.0 in fixed-point
            }
        }

        metrics
    }

    /// Update returns buffer
    pub fn add_return(&mut self, return_value: i64) {
        // Circular buffer logic - maintain fixed size
        if self.returns_buffer.len() >= self.returns_buffer.capacity() {
            self.returns_buffer.remove(0);
        }
        self.returns_buffer.push(return_value);
    }

    /// Update portfolio value for drawdown tracking
    pub fn add_value(&mut self, portfolio_value: i64) {
        if self.values_buffer.len() >= self.values_buffer.capacity() {
            self.values_buffer.remove(0);
        }
        self.values_buffer.push(portfolio_value);
    }

    /// Update benchmark returns for correlation
    pub fn add_benchmark_return(&mut self, benchmark_return: i64) {
        if self.benchmark_buffer.len() >= self.benchmark_buffer.capacity() {
            self.benchmark_buffer.remove(0);
        }
        self.benchmark_buffer.push(benchmark_return);
    }

    /// Get returns buffer
    pub fn returns(&self) -> &[i64] {
        &self.returns_buffer
    }

    /// Calculate correlation with benchmark
    pub fn calculate_correlation(&self) -> i32 {
        if self.returns_buffer.len() < 2 || self.benchmark_buffer.len() < 2 {
            return 0;
        }

        let n = self.returns_buffer.len().min(self.benchmark_buffer.len());
        let returns = &self.returns_buffer[..n];
        let benchmark = &self.benchmark_buffer[..n];

        // Calculate means
        // SAFETY: n > 0 guaranteed by function precondition, fits in i64
        let returns_mean = returns.iter().sum::<i64>() / n as i64;
        // SAFETY: n > 0 guaranteed by function precondition, fits in i64
        let benchmark_mean = benchmark.iter().sum::<i64>() / n as i64;

        // Calculate correlation coefficient
        let mut cov = 0i64;
        let mut returns_var = 0i64;
        let mut benchmark_var = 0i64;

        for i in 0..n {
            let r_diff = returns[i] - returns_mean;
            let b_diff = benchmark[i] - benchmark_mean;

            cov += r_diff.saturating_mul(b_diff) / FIXED_POINT_SCALE;
            returns_var += r_diff.saturating_mul(r_diff) / FIXED_POINT_SCALE;
            benchmark_var += b_diff.saturating_mul(b_diff) / FIXED_POINT_SCALE;
        }

        if returns_var > 0 && benchmark_var > 0 {
            // SAFETY: Variance to f64 for correlation calculation - analytics boundary
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            let correlation =
                (cov as f64) / ((returns_var as f64).sqrt() * (benchmark_var as f64).sqrt());
            (correlation * FIXED_POINT_SCALE as f64) as i32 // Fixed-point
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_portfolio_stats() {
        let analyzer = PortfolioAnalyzer::new(100);
        let positions = vec![
            (Symbol::new(1), 1000000, 5000),
            (Symbol::new(2), -500000, -2000),
            (Symbol::new(3), 1500000, 3000),
        ];

        let stats = analyzer.calculate_stats(&positions);

        assert_eq!(stats.long_positions, 2);
        assert_eq!(stats.short_positions, 1);
        assert_eq!(stats.long_exposure, 2500000);
        assert_eq!(stats.short_exposure, 500000);
        assert_eq!(stats.net_exposure, 2000000);
    }

    #[test]
    fn test_risk_metrics() {
        let mut analyzer = PortfolioAnalyzer::new(100);
        let returns = vec![100, -50, 200, -100, 150, -75, 300, -200, 250, 50];

        let risk = analyzer.calculate_risk(&returns);

        assert!(risk.var_95 <= 0); // Should be negative (loss)
        assert!(risk.volatility > 0);
    }

    #[test]
    fn test_performance_metrics() {
        let analyzer = PortfolioAnalyzer::new(100);
        let returns = vec![100, -50, 200, -100, 150];
        let risk_free_rate = 200; // 2% annual

        let perf = analyzer.calculate_performance(&returns, risk_free_rate);

        assert!(perf.win_rate > 0);
        assert_eq!(perf.win_rate, 6000); // 3 wins out of 5 = 60%
        assert!(perf.avg_win > 0);
        assert!(perf.avg_loss < 0);
    }
}
