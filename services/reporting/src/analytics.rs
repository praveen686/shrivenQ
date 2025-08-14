//! Advanced analytics utilities for the reporting service
//!
//! Additional analytical functions and utilities for performance measurement

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Rolling window calculator using VecDeque for efficient FIFO operations
pub struct RollingWindow {
    window: VecDeque<f64>,
    capacity: usize,
    sum: f64,
    sum_sq: f64,
}

impl RollingWindow {
    /// Create new rolling window with fixed capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(capacity),
            capacity,
            sum: 0.0,
            sum_sq: 0.0,
        }
    }

    /// Add value to rolling window with O(1) complexity
    pub fn add(&mut self, value: f64) {
        // Remove oldest value if at capacity
        if self.window.len() >= self.capacity {
            if let Some(old_value) = self.window.pop_front() {
                self.sum -= old_value;
                self.sum_sq -= old_value * old_value;
            }
        }

        // Add new value
        self.window.push_back(value);
        self.sum += value;
        self.sum_sq += value * value;
    }

    /// Get current mean in O(1) time
    #[allow(clippy::cast_precision_loss)]
    pub fn mean(&self) -> f64 {
        if self.window.is_empty() {
            0.0
        } else {
            self.sum / self.window.len() as f64
        }
    }

    /// Get current standard deviation in O(1) time
    #[allow(clippy::cast_precision_loss)]
    pub fn std_dev(&self) -> f64 {
        let len = self.window.len();
        if len < 2 {
            return 0.0;
        }

        let len_f64 = len as f64;
        let mean = self.mean();
        let variance = (self.sum_sq / len_f64) - (mean * mean);
        variance.max(0.0).sqrt() // Handle floating point precision issues
    }

    /// Get current window size
    pub fn len(&self) -> usize {
        self.window.len()
    }

    /// Check if window is empty
    pub fn is_empty(&self) -> bool {
        self.window.is_empty()
    }

    /// Get current values as slice
    pub fn values(&self) -> Vec<f64> {
        self.window.iter().copied().collect()
    }

    /// Clear the window
    pub fn clear(&mut self) {
        self.window.clear();
        self.sum = 0.0;
        self.sum_sq = 0.0;
    }
}

/// Statistical utilities for analytics
pub struct StatisticalAnalyzer;

impl StatisticalAnalyzer {
    /// Calculate rolling statistics
    pub fn rolling_stats(data: &[f64], window: usize) -> Vec<RollingStats> {
        if data.is_empty() || window == 0 {
            return Vec::new();
        }

        let mut results = Vec::with_capacity(data.len().saturating_sub(window - 1));

        for i in window..=data.len() {
            let window_data = &data[i - window..i];
            let stats = Self::calculate_window_stats(window_data);
            results.push(stats);
        }

        results
    }

    /// Calculate statistics for a single window
    #[allow(clippy::cast_precision_loss)]
    fn calculate_window_stats(data: &[f64]) -> RollingStats {
        let count = data.len();
        if count == 0 {
            return RollingStats::default();
        }

        let sum: f64 = data.iter().sum();
        // SAFETY: usize to f64 for statistical calculation
        let mean = sum / count as f64;

        let variance = data
            .iter()
            .map(|x| (x - mean).powi(2))
            // SAFETY: usize to f64 for statistical calculation
            .sum::<f64>()
            / count as f64;

        let std_dev = variance.sqrt();

        let mut sorted_data = data.to_vec();
        // Use total ordering for f64 sorting - NaN values will sort to the end
        sorted_data.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let min = sorted_data[0];
        let max = sorted_data[count - 1];
        let median = if count % 2 == 0 {
            (sorted_data[count / 2 - 1] + sorted_data[count / 2]) / 2.0
        } else {
            sorted_data[count / 2]
        };

        RollingStats {
            count,
            mean,
            std_dev,
            min,
            max,
            median,
            sum,
        }
    }

    /// Calculate correlation between two series
    #[allow(clippy::cast_precision_loss)]
    pub fn correlation(x: &[f64], y: &[f64]) -> f64 {
        if x.len() != y.len() || x.len() < 2 {
            return 0.0;
        }

        // SAFETY: usize to f64 for statistical calculation
        let n = x.len() as f64;
        let mean_x = x.iter().sum::<f64>() / n;
        let mean_y = y.iter().sum::<f64>() / n;

        let mut numerator = 0.0;
        let mut sum_sq_x = 0.0;
        let mut sum_sq_y = 0.0;

        for (xi, yi) in x.iter().zip(y.iter()) {
            let dx = xi - mean_x;
            let dy = yi - mean_y;
            numerator += dx * dy;
            sum_sq_x += dx * dx;
            sum_sq_y += dy * dy;
        }

        let denominator = (sum_sq_x * sum_sq_y).sqrt();
        if denominator > 0.0 {
            numerator / denominator
        } else {
            0.0
        }
    }

    /// Calculate beta coefficient (relative to benchmark)
    pub fn beta(returns: &[f64], benchmark_returns: &[f64]) -> f64 {
        if returns.len() != benchmark_returns.len() || returns.len() < 2 {
            return 1.0; // Default beta
        }

        let correlation = Self::correlation(returns, benchmark_returns);
        let returns_std = Self::standard_deviation(returns);
        let benchmark_std = Self::standard_deviation(benchmark_returns);

        if benchmark_std > 0.0 {
            correlation * (returns_std / benchmark_std)
        } else {
            1.0
        }
    }

    /// Calculate standard deviation
    #[allow(clippy::cast_precision_loss)]
    fn standard_deviation(data: &[f64]) -> f64 {
        if data.len() < 2 {
            return 0.0;
        }

        // SAFETY: usize to f64 for statistical calculation
        let mean = data.iter().sum::<f64>() / data.len() as f64;
        let variance = data
            .iter()
            .map(|x| (x - mean).powi(2))
            // SAFETY: usize to f64 for statistical calculation
            .sum::<f64>()
            / data.len() as f64;

        variance.sqrt()
    }
}

/// Rolling statistics structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollingStats {
    pub count: usize,
    pub mean: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    pub median: f64,
    pub sum: f64,
}

impl Default for RollingStats {
    fn default() -> Self {
        Self {
            count: 0,
            mean: 0.0,
            std_dev: 0.0,
            min: 0.0,
            max: 0.0,
            median: 0.0,
            sum: 0.0,
        }
    }
}

/// Advanced portfolio metrics calculator
pub struct PortfolioMetricsCalculator;

impl PortfolioMetricsCalculator {
    /// Calculate Information Ratio
    pub fn information_ratio(active_returns: &[f64], benchmark_returns: &[f64]) -> f64 {
        if active_returns.len() != benchmark_returns.len() || active_returns.is_empty() {
            return 0.0;
        }

        let excess_returns: Vec<f64> = active_returns
            .iter()
            .zip(benchmark_returns.iter())
            .map(|(a, b)| a - b)
            .collect();

        // SAFETY: usize to f64 for statistical calculation
        let mean_excess = excess_returns.iter().sum::<f64>() / excess_returns.len() as f64;
        let tracking_error = StatisticalAnalyzer::standard_deviation(&excess_returns);

        if tracking_error > 0.0 {
            mean_excess / tracking_error
        } else {
            0.0
        }
    }

    /// Calculate Treynor Ratio
    #[allow(clippy::cast_precision_loss)]
    pub fn treynor_ratio(returns: &[f64], benchmark_returns: &[f64], risk_free_rate: f64) -> f64 {
        if returns.is_empty() {
            return 0.0;
        }

        // SAFETY: usize to f64 for statistical calculation
        let mean_return = returns.iter().sum::<f64>() / returns.len() as f64;
        let excess_return = mean_return - risk_free_rate;
        let beta = StatisticalAnalyzer::beta(returns, benchmark_returns);

        if beta != 0.0 {
            excess_return / beta
        } else {
            0.0
        }
    }

    /// Calculate Jensen's Alpha
    #[allow(clippy::cast_precision_loss)]
    pub fn jensens_alpha(returns: &[f64], benchmark_returns: &[f64], risk_free_rate: f64) -> f64 {
        if returns.is_empty() || benchmark_returns.is_empty() {
            return 0.0;
        }

        // SAFETY: usize to f64 for statistical calculation
        let mean_return = returns.iter().sum::<f64>() / returns.len() as f64;
        // SAFETY: usize to f64 for statistical calculation
        let mean_benchmark = benchmark_returns.iter().sum::<f64>() / benchmark_returns.len() as f64;
        let beta = StatisticalAnalyzer::beta(returns, benchmark_returns);

        // Jensen's Alpha = Portfolio Return - (Risk Free Rate + Beta * (Benchmark Return - Risk Free Rate))
        let expected_return = risk_free_rate + beta * (mean_benchmark - risk_free_rate);
        mean_return - expected_return
    }

    /// Calculate Maximum Adverse Excursion (MAE)
    pub fn maximum_adverse_excursion(equity_curve: &[f64]) -> f64 {
        if equity_curve.is_empty() {
            return 0.0;
        }

        let mut max_adverse = 0.0;
        let mut peak = equity_curve[0];

        for &value in equity_curve.iter().skip(1) {
            if value > peak {
                peak = value;
            } else {
                let adverse_excursion = peak - value;
                if adverse_excursion > max_adverse {
                    max_adverse = adverse_excursion;
                }
            }
        }

        max_adverse
    }

    /// Calculate Maximum Favorable Excursion (MFE)
    pub fn maximum_favorable_excursion(equity_curve: &[f64]) -> f64 {
        if equity_curve.is_empty() {
            return 0.0;
        }

        let mut max_favorable = 0.0;
        let mut trough = equity_curve[0];

        for &value in equity_curve.iter().skip(1) {
            if value < trough {
                trough = value;
            } else {
                let favorable_excursion = value - trough;
                if favorable_excursion > max_favorable {
                    max_favorable = favorable_excursion;
                }
            }
        }

        max_favorable
    }
}

/// Risk metrics calculator
pub struct RiskMetricsCalculator;

impl RiskMetricsCalculator {
    /// Calculate Conditional Value at Risk (CVaR) / Expected Shortfall
    pub fn conditional_var(returns: &[f64], confidence_level: f64) -> f64 {
        if returns.is_empty() || confidence_level <= 0.0 || confidence_level >= 1.0 {
            return 0.0;
        }

        let mut sorted_returns = returns.to_vec();
        // Safe sorting for financial returns - handle NaN gracefully
        sorted_returns.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // SAFETY: f64 to usize for array indexing, bounded by returns.len()
        let var_index = ((1.0 - confidence_level) * returns.len() as f64) as usize;

        if var_index == 0 {
            return sorted_returns[0];
        }

        // Average of returns below VaR threshold
        let tail_returns = &sorted_returns[..var_index];
        tail_returns.iter().sum::<f64>() / tail_returns.len() as f64
    }

    /// Calculate downside deviation
    #[allow(clippy::cast_precision_loss)]
    pub fn downside_deviation(returns: &[f64], minimum_acceptable_return: f64) -> f64 {
        if returns.is_empty() {
            return 0.0;
        }

        let downside_returns: Vec<f64> = returns
            .iter()
            .filter_map(|&r| {
                if r < minimum_acceptable_return {
                    Some((r - minimum_acceptable_return).powi(2))
                } else {
                    None
                }
            })
            .collect();

        if downside_returns.is_empty() {
            return 0.0;
        }

        let mean_downside = downside_returns.iter().sum::<f64>() / downside_returns.len() as f64;
        mean_downside.sqrt()
    }

    /// Calculate tail ratio (95th percentile / 5th percentile)
    pub fn tail_ratio(returns: &[f64]) -> f64 {
        if returns.len() < 20 {
            // Need sufficient data
            return 1.0;
        }

        let mut sorted_returns = returns.to_vec();
        // Safe sorting for financial returns - handle NaN gracefully
        sorted_returns.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let len = sorted_returns.len();
        let p5_index = (len as f64 * 0.05) as usize;
        let p95_index = (len as f64 * 0.95) as usize;

        let p5 = sorted_returns[p5_index].abs();
        let p95 = sorted_returns[p95_index.min(len - 1)];

        if p5 > 0.0 { p95 / p5 } else { 1.0 }
    }
}

/// Time series analysis utilities
pub struct TimeSeriesAnalyzer;

impl TimeSeriesAnalyzer {
    /// Calculate autocorrelation at given lag
    #[allow(clippy::cast_precision_loss)]
    pub fn autocorrelation(data: &[f64], lag: usize) -> f64 {
        if data.len() <= lag {
            return 0.0;
        }

        let n = data.len() - lag;
        // SAFETY: usize to f64 for statistical calculation
        let mean = data.iter().sum::<f64>() / data.len() as f64;

        let mut numerator = 0.0;
        let mut denominator = 0.0;

        // Calculate covariance at lag
        for i in 0..n {
            let x_dev = data[i] - mean;
            let y_dev = data[i + lag] - mean;
            numerator += x_dev * y_dev;
        }

        // Calculate variance
        for &value in data {
            let dev = value - mean;
            denominator += dev * dev;
        }

        if denominator > 0.0 {
            numerator / denominator
        } else {
            0.0
        }
    }

    /// Detect regime changes using rolling statistics
    pub fn detect_regime_changes(data: &[f64], window: usize, threshold: f64) -> Vec<usize> {
        if data.len() < 2 * window {
            return Vec::new();
        }

        let mut change_points = Vec::new();
        let rolling_stats = StatisticalAnalyzer::rolling_stats(data, window);

        for i in 1..rolling_stats.len() {
            let prev_mean = rolling_stats[i - 1].mean;
            let curr_mean = rolling_stats[i].mean;
            let prev_std = rolling_stats[i - 1].std_dev;

            if prev_std > 0.0 {
                let normalized_change = (curr_mean - prev_mean).abs() / prev_std;
                if normalized_change > threshold {
                    change_points.push(i + window - 1); // Adjust for window offset
                }
            }
        }

        change_points
    }

    /// Calculate momentum indicator
    #[allow(clippy::cast_precision_loss)]
    pub fn momentum(data: &[f64], periods: usize) -> Vec<f64> {
        if data.len() <= periods {
            return Vec::new();
        }

        let mut momentum_values = Vec::with_capacity(data.len() - periods);

        for i in periods..data.len() {
            let current = data[i];
            let previous = data[i - periods];
            let momentum = if previous != 0.0 {
                (current - previous) / previous * 100.0
            } else {
                0.0
            };
            momentum_values.push(momentum);
        }

        momentum_values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rolling_stats() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let stats = StatisticalAnalyzer::rolling_stats(&data, 3);

        assert_eq!(stats.len(), 3);
        assert!((stats[0].mean - 2.0).abs() < 0.001); // Mean of [1,2,3]
        assert!((stats[1].mean - 3.0).abs() < 0.001); // Mean of [2,3,4]
        assert!((stats[2].mean - 4.0).abs() < 0.001); // Mean of [3,4,5]
    }

    #[test]
    fn test_correlation() {
        let x = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let y = vec![2.0, 4.0, 6.0, 8.0, 10.0]; // Perfect positive correlation

        let corr = StatisticalAnalyzer::correlation(&x, &y);
        assert!((corr - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_beta_calculation() {
        let returns = vec![0.01, 0.02, -0.01, 0.03, -0.02];
        let benchmark = vec![0.005, 0.01, -0.005, 0.015, -0.01];

        let beta = StatisticalAnalyzer::beta(&returns, &benchmark);
        assert!(beta > 0.0); // Should be positive for correlated assets
    }

    #[test]
    fn test_conditional_var() {
        let returns = vec![-0.05, -0.03, -0.01, 0.01, 0.02, 0.03];
        let cvar = RiskMetricsCalculator::conditional_var(&returns, 0.95);

        // CVaR should be average of worst 5% (in this case, worst return)
        assert!((cvar - (-0.05)).abs() < 0.001);
    }

    #[test]
    fn test_autocorrelation() {
        let data = vec![1.0, 2.0, 1.0, 2.0, 1.0, 2.0]; // Alternating pattern
        let autocorr_1 = TimeSeriesAnalyzer::autocorrelation(&data, 1);
        let autocorr_2 = TimeSeriesAnalyzer::autocorrelation(&data, 2);

        assert!(autocorr_1 < 0.0); // Negative correlation at lag 1
        assert!(autocorr_2 > 0.0); // Positive correlation at lag 2
    }

    #[test]
    fn test_momentum_calculation() {
        let data = vec![100.0, 101.0, 102.0, 103.0, 104.0];
        let momentum = TimeSeriesAnalyzer::momentum(&data, 2);

        assert_eq!(momentum.len(), 3);
        // Each momentum should be approximately 2% (2 point increase over 2 periods)
        for &mom in &momentum {
            assert!((mom - 2.0).abs() < 0.1);
        }
    }

    #[test]
    fn test_rolling_window() {
        let mut window = RollingWindow::new(3);

        // Test empty window
        assert!(window.is_empty());
        assert_eq!(window.len(), 0);
        assert_eq!(window.mean(), 0.0);

        // Add values
        window.add(1.0);
        assert_eq!(window.len(), 1);
        assert_eq!(window.mean(), 1.0);

        window.add(2.0);
        window.add(3.0);
        assert_eq!(window.len(), 3);
        assert_eq!(window.mean(), 2.0); // (1+2+3)/3

        // Test rolling behavior
        window.add(4.0); // Should remove 1.0, now [2,3,4]
        assert_eq!(window.len(), 3);
        assert_eq!(window.mean(), 3.0); // (2+3+4)/3

        // Test standard deviation
        let std_dev = window.std_dev();
        assert!(std_dev > 0.0);

        // Test clear
        window.clear();
        assert!(window.is_empty());
        assert_eq!(window.mean(), 0.0);
    }

    #[test]
    fn test_rolling_window_statistics() {
        let mut window = RollingWindow::new(5);
        let values = [1.0, 2.0, 3.0, 4.0, 5.0];

        for &val in &values {
            window.add(val);
        }

        assert_eq!(window.mean(), 3.0); // (1+2+3+4+5)/5
        assert!((window.std_dev() - 1.4142135623730951).abs() < 1e-10); // Known std dev

        let window_values = window.values();
        assert_eq!(window_values, values.to_vec());
    }
}
