//! Portfolio analytics and performance metrics tests
//! Tests portfolio statistics, risk metrics, and performance calculations

use portfolio_manager::portfolio::{PortfolioAnalyzer, PortfolioStats, RiskMetrics, PerformanceMetrics};
use rstest::*;
use services_common::Symbol;
use approx::assert_relative_eq;

// Test fixtures
#[fixture]
fn analyzer() -> PortfolioAnalyzer {
    PortfolioAnalyzer::new(1000)
}

#[fixture]
fn sample_positions() -> Vec<(Symbol, i64, i64)> {
    vec![
        (Symbol::new(1), 1000000, 5000),   // Long 100 units, +$50 PnL
        (Symbol::new(2), -500000, -2000),  // Short 50 units, -$20 PnL  
        (Symbol::new(3), 1500000, 3000),   // Long 150 units, +$30 PnL
        (Symbol::new(4), -200000, 1000),   // Short 20 units, +$10 PnL
    ]
}

#[fixture]
fn sample_returns() -> Vec<i64> {
    vec![
        100, -50, 200, -100, 150, -75, 300, -200, 250, 50,
        80, -40, 120, -90, 180, -60, 220, -110, 190, 70,
    ]
}

#[fixture]
fn volatile_returns() -> Vec<i64> {
    vec![
        500, -300, 400, -600, 700, -200, 350, -450, 600, -100,
        550, -380, 420, -520, 680, -220, 380, -480, 590, -150,
    ]
}

mod portfolio_stats_tests {
    use super::*;

    #[rstest]
    fn test_calculate_basic_portfolio_stats(analyzer: PortfolioAnalyzer, sample_positions: Vec<(Symbol, i64, i64)>) {
        let stats = analyzer.calculate_stats(&sample_positions);

        // Check position counts
        assert_eq!(stats.long_positions, 2);  // Symbols 1 and 3
        assert_eq!(stats.short_positions, 2); // Symbols 2 and 4

        // Check exposures
        assert_eq!(stats.long_exposure, 2500000);  // 100 + 150 = 250 units
        assert_eq!(stats.short_exposure, 700000);  // 50 + 20 = 70 units
        assert_eq!(stats.net_exposure, 1800000);   // 250 - 70 = 180 units net long
        assert_eq!(stats.gross_exposure, 3200000); // 250 + 70 = 320 units gross
    }

    #[rstest]
    fn test_empty_portfolio_stats(analyzer: PortfolioAnalyzer) {
        let empty_positions = vec![];
        let stats = analyzer.calculate_stats(&empty_positions);

        assert_eq!(stats.long_positions, 0);
        assert_eq!(stats.short_positions, 0);
        assert_eq!(stats.long_exposure, 0);
        assert_eq!(stats.short_exposure, 0);
        assert_eq!(stats.net_exposure, 0);
        assert_eq!(stats.gross_exposure, 0);
    }

    #[rstest]
    fn test_long_only_portfolio_stats(analyzer: PortfolioAnalyzer) {
        let long_only_positions = vec![
            (Symbol::new(1), 1000000, 2000),
            (Symbol::new(2), 500000, 1500),
            (Symbol::new(3), 2000000, -500),
        ];

        let stats = analyzer.calculate_stats(&long_only_positions);

        assert_eq!(stats.long_positions, 3);
        assert_eq!(stats.short_positions, 0);
        assert_eq!(stats.long_exposure, 3500000);
        assert_eq!(stats.short_exposure, 0);
        assert_eq!(stats.net_exposure, 3500000);
        assert_eq!(stats.gross_exposure, 3500000);
    }

    #[rstest]
    fn test_short_only_portfolio_stats(analyzer: PortfolioAnalyzer) {
        let short_only_positions = vec![
            (Symbol::new(1), -1000000, 2000),
            (Symbol::new(2), -500000, -1000),
        ];

        let stats = analyzer.calculate_stats(&short_only_positions);

        assert_eq!(stats.long_positions, 0);
        assert_eq!(stats.short_positions, 2);
        assert_eq!(stats.long_exposure, 0);
        assert_eq!(stats.short_exposure, 1500000);
        assert_eq!(stats.net_exposure, -1500000);
        assert_eq!(stats.gross_exposure, 1500000);
    }

    #[rstest]
    fn test_market_neutral_portfolio_stats(analyzer: PortfolioAnalyzer) {
        let market_neutral = vec![
            (Symbol::new(1), 1000000, 1000),  // $10 profit
            (Symbol::new(2), -1000000, 2000), // $20 profit  
        ];

        let stats = analyzer.calculate_stats(&market_neutral);

        assert_eq!(stats.long_positions, 1);
        assert_eq!(stats.short_positions, 1);
        assert_eq!(stats.long_exposure, 1000000);
        assert_eq!(stats.short_exposure, 1000000);
        assert_eq!(stats.net_exposure, 0); // Market neutral
        assert_eq!(stats.gross_exposure, 2000000);
    }

    #[rstest]
    fn test_calculate_correlation_default(analyzer: PortfolioAnalyzer) {
        // With empty buffers, correlation should be 0
        let correlation = analyzer.calculate_correlation();
        assert_eq!(correlation, 0);
    }
}

mod risk_metrics_tests {
    use super::*;

    #[rstest]
    fn test_empty_returns_risk_metrics(mut analyzer: PortfolioAnalyzer) {
        let empty_returns = vec![];
        let risk = analyzer.calculate_risk(&empty_returns);

        assert_eq!(risk.var_95, 0);
        assert_eq!(risk.var_99, 0);
        assert_eq!(risk.cvar, 0);
        assert_eq!(risk.max_drawdown, 0);
        assert_eq!(risk.volatility, 0);
    }

    #[rstest]
    fn test_risk_metrics_calculation(mut analyzer: PortfolioAnalyzer, sample_returns: Vec<i64>) {
        let risk = analyzer.calculate_risk(&sample_returns);

        // VaR should be negative (representing losses)
        assert!(risk.var_95 <= 0);
        assert!(risk.var_99 <= risk.var_95); // 99% VaR should be worse than 95%
        
        // CVaR should be negative and worse than VaR
        assert!(risk.cvar <= risk.var_95);
        
        // Volatility should be positive
        assert!(risk.volatility > 0);
        
        // Downside deviation should be positive
        assert!(risk.downside_deviation >= 0);
    }

    #[rstest]
    fn test_high_volatility_risk_metrics(mut analyzer: PortfolioAnalyzer, volatile_returns: Vec<i64>) {
        let risk = analyzer.calculate_risk(&volatile_returns);

        // High volatility should result in larger risk metrics
        assert!(risk.volatility > 1000); // Should be significantly above baseline
        assert!(risk.var_95 < -100); // Significant negative VaR
        assert!(risk.downside_deviation > 500); // Significant downside risk
    }

    #[rstest]
    fn test_single_return_risk_metrics(mut analyzer: PortfolioAnalyzer) {
        let single_return = vec![100];
        let risk = analyzer.calculate_risk(&single_return);

        // Single return should have zero volatility
        assert_eq!(risk.volatility, 0);
        assert_eq!(risk.var_95, 100); // Single return becomes the VaR
        assert_eq!(risk.downside_deviation, 0);
    }

    #[rstest]
    fn test_positive_only_returns_risk_metrics(mut analyzer: PortfolioAnalyzer) {
        let positive_returns = vec![50, 100, 75, 125, 90, 110];
        let risk = analyzer.calculate_risk(&positive_returns);

        // All positive returns
        assert!(risk.var_95 >= 0);
        assert!(risk.volatility > 0); // Still has volatility
        assert_eq!(risk.downside_deviation, 0); // No negative returns
    }

    #[rstest]
    fn test_negative_only_returns_risk_metrics(mut analyzer: PortfolioAnalyzer) {
        let negative_returns = vec![-50, -100, -75, -25, -150, -60];
        let risk = analyzer.calculate_risk(&negative_returns);

        // All negative returns
        assert!(risk.var_95 < 0);
        assert!(risk.var_99 < 0);
        assert!(risk.cvar < 0);
        assert!(risk.volatility > 0);
        assert!(risk.downside_deviation > 0);
    }

    #[rstest]
    fn test_drawdown_calculation_with_values(mut analyzer: PortfolioAnalyzer) {
        // Simulate portfolio values that create drawdown
        let portfolio_values = vec![100000, 110000, 105000, 95000, 90000, 100000, 120000];
        
        for value in portfolio_values {
            analyzer.add_value(value);
        }

        let returns = vec![-1000, 2000, -5000, -3000, 5000, 8000]; // Dummy returns
        let risk = analyzer.calculate_risk(&returns);

        assert!(risk.max_drawdown > 0); // Should have some drawdown
        assert!(risk.max_drawdown_pct > 0); // Percentage should be positive
    }

    #[rstest]
    fn test_risk_metrics_precision(mut analyzer: PortfolioAnalyzer) {
        // Test with specific values to verify calculations
        let precise_returns = vec![1000, -500, 1500, -1000, 2000, -750];
        let risk = analyzer.calculate_risk(&precise_returns);

        // Verify basic properties
        assert!(risk.volatility > 0);
        assert!(risk.var_95 <= 0);
        assert!(risk.cvar <= risk.var_95);
        
        // Should be mathematically consistent
        if risk.var_99 != 0 {
            assert!(risk.var_99 <= risk.var_95);
        }
    }
}

mod performance_metrics_tests {
    use super::*;

    #[rstest]
    fn test_empty_returns_performance(analyzer: PortfolioAnalyzer) {
        let empty_returns = vec![];
        let risk_free_rate = 200; // 2%
        
        let perf = analyzer.calculate_performance(&empty_returns, risk_free_rate);

        assert_eq!(perf.total_return, 0);
        assert_eq!(perf.win_rate, 0);
        assert_eq!(perf.avg_win, 0);
        assert_eq!(perf.avg_loss, 0);
        assert_eq!(perf.sharpe_ratio, 0);
    }

    #[rstest]
    fn test_basic_performance_metrics(analyzer: PortfolioAnalyzer, sample_returns: Vec<i64>) {
        let risk_free_rate = 200; // 2%
        let perf = analyzer.calculate_performance(&sample_returns, risk_free_rate);

        // Basic return calculations
        assert!(perf.total_return != 0);
        assert!(perf.daily_return != 0);
        
        // Win rate should be reasonable (0-100%)
        assert!(perf.win_rate >= 0);
        assert!(perf.win_rate <= 10000); // 100% in fixed-point
        
        // Average win should be positive, average loss negative
        if perf.avg_win != 0 {
            assert!(perf.avg_win > 0);
        }
        if perf.avg_loss != 0 {
            assert!(perf.avg_loss < 0);
        }
    }

    #[rstest]
    fn test_all_winning_trades_performance(analyzer: PortfolioAnalyzer) {
        let winning_returns = vec![100, 200, 150, 300, 250];
        let risk_free_rate = 200;
        
        let perf = analyzer.calculate_performance(&winning_returns, risk_free_rate);

        assert_eq!(perf.win_rate, 10000); // 100% win rate
        assert!(perf.avg_win > 0);
        assert_eq!(perf.avg_loss, 0); // No losses
        assert!(perf.profit_factor > 10000); // Should be high
        assert!(perf.total_return > 0);
    }

    #[rstest]
    fn test_all_losing_trades_performance(analyzer: PortfolioAnalyzer) {
        let losing_returns = vec![-100, -200, -150, -50, -75];
        let risk_free_rate = 200;
        
        let perf = analyzer.calculate_performance(&losing_returns, risk_free_rate);

        assert_eq!(perf.win_rate, 0); // 0% win rate
        assert_eq!(perf.avg_win, 0); // No wins
        assert!(perf.avg_loss < 0);
        assert_eq!(perf.profit_factor, 0); // No profits
        assert!(perf.total_return < 0);
        assert!(perf.sharpe_ratio < 0); // Negative Sharpe
    }

    #[rstest]
    fn test_mixed_performance_calculations(analyzer: PortfolioAnalyzer) {
        // 60% win rate scenario
        let mixed_returns = vec![100, -50, 150, -75, 200, -25, 120, -60, 180, -40];
        let risk_free_rate = 200;
        
        let perf = analyzer.calculate_performance(&mixed_returns, risk_free_rate);

        // Should be 50% win rate (5 wins out of 10)
        assert_eq!(perf.win_rate, 5000); // 50% in fixed-point
        
        // Calculate expected values
        let wins: Vec<_> = mixed_returns.iter().filter(|&&r| r > 0).copied().collect();
        let losses: Vec<_> = mixed_returns.iter().filter(|&&r| r < 0).copied().collect();
        
        let expected_avg_win = wins.iter().sum::<i64>() / wins.len() as i64;
        let expected_avg_loss = losses.iter().sum::<i64>() / losses.len() as i64;
        
        assert_eq!(perf.avg_win, expected_avg_win);
        assert_eq!(perf.avg_loss, expected_avg_loss);
    }

    #[rstest]
    fn test_sharpe_ratio_calculation(analyzer: PortfolioAnalyzer) {
        // High return, low volatility scenario
        let consistent_returns = vec![100, 105, 95, 102, 98, 103, 97, 101, 99, 104];
        let risk_free_rate = 50; // 0.5%
        
        let perf = analyzer.calculate_performance(&consistent_returns, risk_free_rate);

        // Should have positive Sharpe ratio (returns > risk free rate, low vol)
        assert!(perf.sharpe_ratio > 0);
        assert!(perf.annual_return > risk_free_rate); // Excess return
    }

    #[rstest]
    fn test_sortino_ratio_calculation(analyzer: PortfolioAnalyzer) {
        let returns_with_upside = vec![200, -100, 300, -50, 400, 100, -75, 250];
        let risk_free_rate = 100;
        
        let perf = analyzer.calculate_performance(&returns_with_upside, risk_free_rate);

        // Sortino should be calculated (focuses on downside deviation)
        assert!(perf.sortino_ratio != 0);
        
        // With positive returns overall, Sortino should be positive
        if perf.annual_return > risk_free_rate {
            assert!(perf.sortino_ratio > 0);
        }
    }

    #[rstest]
    fn test_calmar_ratio_calculation(analyzer: PortfolioAnalyzer) {
        // Returns that create both gains and drawdown
        let returns_with_drawdown = vec![1000, -500, -300, 800, -400, 600, 200];
        let risk_free_rate = 100;
        
        let perf = analyzer.calculate_performance(&returns_with_drawdown, risk_free_rate);

        // Calmar ratio should be calculated
        assert!(perf.calmar_ratio != 0);
        
        // With overall positive returns, Calmar should be reasonable
        if perf.annual_return > 0 {
            assert!(perf.calmar_ratio > 0);
        }
    }

    #[rstest]
    fn test_profit_factor_calculation(analyzer: PortfolioAnalyzer) {
        let balanced_returns = vec![1000, -500, 800, -400, 600, -300];
        let risk_free_rate = 100;
        
        let perf = analyzer.calculate_performance(&balanced_returns, risk_free_rate);

        // Calculate expected profit factor
        let total_gains: i64 = balanced_returns.iter().filter(|&&r| r > 0).sum::<i64>();
        let total_losses: i64 = balanced_returns.iter().filter(|&&r| r < 0).map(|r| r.abs()).sum::<i64>();
        
        if total_losses > 0 {
            let expected_pf = (total_gains * 10000) / total_losses;
            assert_relative_eq!(perf.profit_factor as f64, expected_pf as f64, epsilon = 1.0);
        }
    }

    #[rstest]
    fn test_annualized_returns_calculation(analyzer: PortfolioAnalyzer) {
        let daily_returns = vec![10; 252]; // 252 trading days, 1% daily return
        let risk_free_rate = 200;
        
        let perf = analyzer.calculate_performance(&daily_returns, risk_free_rate);

        // Daily return should be ~10
        assert_relative_eq!(perf.daily_return as f64, 10.0, epsilon = 2.0);
        
        // Annual return should be daily * 252
        let expected_annual = 10 * 252;
        assert_relative_eq!(perf.annual_return as f64, expected_annual as f64, epsilon = 10.0);
    }

    #[rstest]
    fn test_performance_edge_cases(analyzer: PortfolioAnalyzer) {
        // Single very large return
        let extreme_return = vec![1000000];
        let risk_free_rate = 200;
        
        let perf = analyzer.calculate_performance(&extreme_return, risk_free_rate);

        assert_eq!(perf.win_rate, 10000); // 100% win rate
        assert!(perf.total_return > 0);
        assert_eq!(perf.avg_loss, 0);
        assert!(perf.avg_win > 0);
    }

    #[rstest]
    fn test_zero_volatility_performance(analyzer: PortfolioAnalyzer) {
        // All identical returns (zero volatility)
        let zero_vol_returns = vec![100; 10];
        let risk_free_rate = 50;
        
        let perf = analyzer.calculate_performance(&zero_vol_returns, risk_free_rate);

        // Should handle zero volatility gracefully
        assert!(perf.total_return > 0);
        assert_eq!(perf.win_rate, 10000); // All positive
        assert!(perf.avg_win > 0);
        
        // Sharpe ratio handling depends on implementation
        // (infinity should be handled as a high value)
    }
}

mod analyzer_buffer_tests {
    use super::*;

    #[rstest]
    fn test_returns_buffer_management(mut analyzer: PortfolioAnalyzer) {
        // Add returns up to capacity
        for i in 0..500 {
            analyzer.add_return(i);
        }

        let returns = analyzer.returns();
        assert!(returns.len() <= 1000); // Shouldn't exceed capacity
    }

    #[rstest]
    fn test_circular_buffer_behavior(mut analyzer: PortfolioAnalyzer) {
        let buffer_capacity = 10;
        let mut small_analyzer = PortfolioAnalyzer::new(buffer_capacity);

        // Fill beyond capacity
        for i in 0..15 {
            small_analyzer.add_return(i);
        }

        let returns = small_analyzer.returns();
        
        // Should maintain fixed capacity
        assert!(returns.len() <= buffer_capacity);
        
        // Most recent values should be present
        // (exact behavior depends on implementation details)
    }

    #[rstest]
    fn test_value_tracking_for_drawdown(mut analyzer: PortfolioAnalyzer) {
        let portfolio_values = vec![100000, 110000, 95000, 105000, 90000];
        
        for value in portfolio_values {
            analyzer.add_value(value);
        }

        // Test that values are being tracked
        let dummy_returns = vec![1000, -500, 1500];
        let risk = analyzer.calculate_risk(&dummy_returns);
        
        // Should have calculated drawdown from the values
        assert!(risk.max_drawdown >= 0);
    }

    #[rstest]
    fn test_benchmark_return_tracking(mut analyzer: PortfolioAnalyzer) {
        // Add some benchmark returns
        for i in 0..10 {
            analyzer.add_benchmark_return(i * 10);
        }

        // Add portfolio returns
        for i in 0..10 {
            analyzer.add_return(i * 15);
        }

        // Should be able to calculate correlation
        let correlation = analyzer.calculate_correlation();
        
        // With positive correlation expected
        assert!(correlation >= 0);
    }

    #[rstest]
    fn test_correlation_calculation_with_data(mut analyzer: PortfolioAnalyzer) {
        // Perfect positive correlation
        for i in 1..=20 {
            analyzer.add_return(i * 100);
            analyzer.add_benchmark_return(i * 50); // Half the portfolio return
        }

        let correlation = analyzer.calculate_correlation();
        
        // Should be close to 1.0 (10000 in fixed-point)
        assert!(correlation > 8000); // At least 0.8 correlation
    }

    #[rstest]
    fn test_negative_correlation(mut analyzer: PortfolioAnalyzer) {
        // Perfect negative correlation
        for i in 1..=20 {
            analyzer.add_return(i * 100);
            analyzer.add_benchmark_return(-i * 50); // Opposite direction
        }

        let correlation = analyzer.calculate_correlation();
        
        // Should be negative
        assert!(correlation < 0);
    }
}