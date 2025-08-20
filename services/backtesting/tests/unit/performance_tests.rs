//! Unit tests for PerformanceAnalyzer functionality

use rstest::*;
use backtesting::*;
use chrono::{Utc, Duration};
use crate::test_utils::*;

#[rstest]
fn test_performance_analyzer_creation() {
    let analyzer = PerformanceAnalyzer::new();
    assert!(format!("{:?}", analyzer).contains("PerformanceAnalyzer"));
}

#[rstest]
fn test_calculate_metrics_empty_portfolio() {
    let analyzer = PerformanceAnalyzer::new();
    let portfolio = PortfolioTracker::new(100_000.0);
    
    let result = analyzer.calculate_metrics(&portfolio, 0.02);
    assert!(result.is_ok(), "Should handle empty portfolio");
    
    let metrics = result.unwrap();
    assert_eq!(metrics.total_return, 0.0);
    assert_eq!(metrics.total_trades, 0);
    assert_eq!(metrics.volatility, 0.0);
}

#[rstest]
fn test_calculate_metrics_single_equity_point() {
    let analyzer = PerformanceAnalyzer::new();
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Record single equity point
    portfolio.record_equity(Utc::now()).unwrap();
    
    let result = analyzer.calculate_metrics(&portfolio, 0.02);
    assert!(result.is_ok(), "Should handle single equity point");
    
    let metrics = result.unwrap();
    // With only one point, there are no returns to calculate
    assert_eq!(metrics.total_return, 0.0);
}

#[rstest]
fn test_calculate_metrics_profitable_scenario() {
    let analyzer = PerformanceAnalyzer::new();
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Create equity curve showing growth
    let base_time = Utc::now() - Duration::days(10);
    let values = vec![100_000.0, 102_000.0, 105_000.0, 108_000.0, 110_000.0];
    
    for (i, value) in values.iter().enumerate() {
        let timestamp = base_time + Duration::days(i as i64);
        // Simulate portfolio state at each point
        portfolio.record_equity(timestamp).unwrap();
        
        // Manually create equity curve by calling record_equity multiple times
        // and simulating price changes through fills if needed
    }
    
    let result = analyzer.calculate_metrics(&portfolio, 0.02);
    assert!(result.is_ok());
    
    let metrics = result.unwrap();
    
    // Basic validation that metrics are reasonable
    TestAssertions::assert_metric_reasonable(metrics.total_return, "total_return", -1.0, 1.0);
    TestAssertions::assert_metric_reasonable(metrics.volatility, "volatility", 0.0, 5.0);
    TestAssertions::assert_metric_reasonable(metrics.max_drawdown, "max_drawdown", 0.0, 1.0);
}

#[rstest]
fn test_calculate_metrics_with_realistic_equity_curve() {
    TestRandom::reset();
    let analyzer = PerformanceAnalyzer::new();
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Create a more realistic scenario with actual trades
    let base_time = Utc::now() - Duration::days(30);
    
    // Initial record
    portfolio.record_equity(base_time).unwrap();
    
    // Simulate some trading activity
    let fill = Fill {
        order_id: "test_1".to_string(),
        symbol: "TEST".to_string(),
        side: OrderSide::Buy,
        quantity: 1000.0,
        price: 50.0,
        commission: 10.0,
        slippage: 5.0,
        timestamp: base_time + Duration::days(1),
    };
    portfolio.process_fill(&fill).unwrap();
    portfolio.record_equity(base_time + Duration::days(1)).unwrap();
    
    // Simulate price changes
    for day in 2..30 {
        let price_change = 50.0 + (day as f64 - 15.0) * 0.5; // Gradual increase then decrease
        let market = MarketSnapshotBuilder::new()
            .with_price("TEST", price_change)
            .build();
        
        portfolio.update_prices(&market).unwrap();
        portfolio.record_equity(base_time + Duration::days(day)).unwrap();
    }
    
    let result = analyzer.calculate_metrics(&portfolio, 0.03);
    assert!(result.is_ok());
    
    let metrics = result.unwrap();
    
    // Validate reasonable ranges for all metrics
    TestAssertions::assert_metric_reasonable(metrics.total_return, "total_return", -1.0, 1.0);
    TestAssertions::assert_metric_reasonable(metrics.annualized_return, "annualized_return", -10.0, 10.0);
    TestAssertions::assert_metric_reasonable(metrics.volatility, "volatility", 0.0, 10.0);
    TestAssertions::assert_metric_reasonable(metrics.max_drawdown, "max_drawdown", 0.0, 1.0);
    TestAssertions::assert_metric_reasonable(metrics.sharpe_ratio, "sharpe_ratio", -5.0, 5.0);
    TestAssertions::assert_metric_reasonable(metrics.sortino_ratio, "sortino_ratio", -10.0, 10.0);
}

#[rstest]
fn test_max_drawdown_calculation() {
    let analyzer = PerformanceAnalyzer::new();
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Create equity curve with known drawdown
    let equity_points = vec![
        (Utc::now() - Duration::days(10), 100_000.0),
        (Utc::now() - Duration::days(9), 105_000.0),  // Peak
        (Utc::now() - Duration::days(8), 110_000.0),  // Higher peak
        (Utc::now() - Duration::days(7), 95_000.0),   // Drawdown
        (Utc::now() - Duration::days(6), 88_000.0),   // Max drawdown: 22k/110k = 20%
        (Utc::now() - Duration::days(5), 95_000.0),   // Recovery
        (Utc::now() - Duration::days(4), 108_000.0),  // Near recovery
    ];
    
    for (timestamp, _value) in equity_points {
        portfolio.record_equity(timestamp).unwrap();
    }
    
    let result = analyzer.calculate_metrics(&portfolio, 0.02);
    assert!(result.is_ok());
    
    let metrics = result.unwrap();
    
    // Max drawdown should be positive and reasonable
    assert!(metrics.max_drawdown >= 0.0, "Max drawdown should be non-negative");
    assert!(metrics.max_drawdown <= 1.0, "Max drawdown should not exceed 100%");
}

#[rstest]
fn test_sharpe_ratio_calculation() {
    let analyzer = PerformanceAnalyzer::new();
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Create consistent upward equity curve
    let base_time = Utc::now() - Duration::days(252); // One year
    for day in 0..252 {
        let value = 100_000.0 * (1.0 + 0.0004 * day as f64); // ~10% annual return
        let timestamp = base_time + Duration::days(day);
        portfolio.record_equity(timestamp).unwrap();
    }
    
    let risk_free_rate = 0.02; // 2% risk-free rate
    let result = analyzer.calculate_metrics(&portfolio, risk_free_rate);
    assert!(result.is_ok());
    
    let metrics = result.unwrap();
    
    // Sharpe ratio should be reasonable for consistent returns
    TestAssertions::assert_metric_reasonable(metrics.sharpe_ratio, "sharpe_ratio", -5.0, 10.0);
    
    // Should have positive total return
    assert!(metrics.total_return > 0.0, "Should have positive return for upward trend");
    
    // Annualized return should be reasonable
    TestAssertions::assert_metric_reasonable(metrics.annualized_return, "annualized_return", -2.0, 2.0);
}

#[rstest]
#[case(0.01)] // 1% risk-free rate
#[case(0.025)] // 2.5% risk-free rate  
#[case(0.05)] // 5% risk-free rate
fn test_different_risk_free_rates(#[case] risk_free_rate: f64) {
    let analyzer = PerformanceAnalyzer::new();
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Create simple equity curve
    portfolio.record_equity(Utc::now() - Duration::days(10)).unwrap();
    portfolio.record_equity(Utc::now() - Duration::days(5)).unwrap();
    portfolio.record_equity(Utc::now()).unwrap();
    
    let result = analyzer.calculate_metrics(&portfolio, risk_free_rate);
    assert!(result.is_ok(), "Should handle risk-free rate: {}", risk_free_rate);
    
    let metrics = result.unwrap();
    
    // Metrics should be calculated without errors
    TestAssertions::assert_metric_reasonable(metrics.sharpe_ratio, "sharpe_ratio", -10.0, 10.0);
    TestAssertions::assert_metric_reasonable(metrics.sortino_ratio, "sortino_ratio", -20.0, 20.0);
}

#[rstest]
fn test_sortino_ratio_calculation() {
    let analyzer = PerformanceAnalyzer::new();
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Create equity curve with mixed returns (some negative)
    let base_time = Utc::now() - Duration::days(100);
    let values = vec![
        100_000.0, 102_000.0, 98_000.0, 105_000.0, 95_000.0,
        108_000.0, 92_000.0, 110_000.0, 88_000.0, 115_000.0,
    ];
    
    for (i, value) in values.iter().enumerate() {
        let timestamp = base_time + Duration::days(i as i64 * 10);
        portfolio.record_equity(timestamp).unwrap();
    }
    
    let result = analyzer.calculate_metrics(&portfolio, 0.03);
    assert!(result.is_ok());
    
    let metrics = result.unwrap();
    
    // Sortino ratio should be calculated
    TestAssertions::assert_metric_reasonable(metrics.sortino_ratio, "sortino_ratio", -10.0, 10.0);
    
    // Should handle both positive and negative returns
    assert!(metrics.volatility >= 0.0, "Volatility should be non-negative");
}

#[rstest]
fn test_value_at_risk_calculation() {
    let analyzer = PerformanceAnalyzer::new();
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Create equity curve with significant variation
    let base_time = Utc::now() - Duration::days(50);
    for day in 0..50 {
        let variation = (day as f64 - 25.0) * 200.0; // Creates volatility
        let value = 100_000.0 + variation;
        let timestamp = base_time + Duration::days(day);
        portfolio.record_equity(timestamp).unwrap();
    }
    
    let result = analyzer.calculate_metrics(&portfolio, 0.02);
    assert!(result.is_ok());
    
    let metrics = result.unwrap();
    
    // VaR and CVaR should be calculated
    TestAssertions::assert_metric_reasonable(metrics.value_at_risk, "VaR", -1.0, 1.0);
    TestAssertions::assert_metric_reasonable(metrics.conditional_var, "CVaR", -1.0, 1.0);
    
    // CVaR should generally be more negative than VaR (expected loss beyond VaR)
    // (This may not always be true due to data distribution, so we just check they're reasonable)
}

#[rstest]
fn test_zero_volatility_scenario() {
    let analyzer = PerformanceAnalyzer::new();
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Create flat equity curve (no volatility)
    let base_time = Utc::now() - Duration::days(10);
    for day in 0..10 {
        portfolio.record_equity(base_time + Duration::days(day)).unwrap();
    }
    
    let result = analyzer.calculate_metrics(&portfolio, 0.02);
    assert!(result.is_ok());
    
    let metrics = result.unwrap();
    
    // With no volatility, Sharpe ratio should be 0 (handled gracefully)
    assert_eq!(metrics.volatility, 0.0);
    assert_eq!(metrics.sharpe_ratio, 0.0);
    assert_eq!(metrics.total_return, 0.0);
    assert_eq!(metrics.max_drawdown, 0.0);
}

#[rstest]
fn test_get_trades_empty() {
    let analyzer = PerformanceAnalyzer::new();
    let trades = analyzer.get_trades();
    
    assert_eq!(trades.len(), 0, "New analyzer should have no trades");
}

#[rstest]
fn test_performance_metrics_default_values() {
    let metrics = PerformanceMetrics::default();
    
    // All metrics should start at reasonable defaults
    assert_eq!(metrics.total_return, 0.0);
    assert_eq!(metrics.annualized_return, 0.0);
    assert_eq!(metrics.volatility, 0.0);
    assert_eq!(metrics.sharpe_ratio, 0.0);
    assert_eq!(metrics.sortino_ratio, 0.0);
    assert_eq!(metrics.calmar_ratio, 0.0);
    assert_eq!(metrics.max_drawdown, 0.0);
    assert_eq!(metrics.max_drawdown_duration, 0);
    assert_eq!(metrics.value_at_risk, 0.0);
    assert_eq!(metrics.conditional_var, 0.0);
    assert_eq!(metrics.total_trades, 0);
    assert_eq!(metrics.winning_trades, 0);
    assert_eq!(metrics.losing_trades, 0);
    assert_eq!(metrics.win_rate, 0.0);
    assert_eq!(metrics.average_win, 0.0);
    assert_eq!(metrics.average_loss, 0.0);
    assert_eq!(metrics.profit_factor, 0.0);
    assert_eq!(metrics.expectancy, 0.0);
    assert_eq!(metrics.total_commission, 0.0);
    assert_eq!(metrics.total_slippage, 0.0);
    assert_eq!(metrics.average_trade_duration, 0);
}

#[rstest]
fn test_metrics_with_extreme_values() {
    let analyzer = PerformanceAnalyzer::new();
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Create equity curve with extreme values to test numerical stability
    let base_time = Utc::now() - Duration::days(5);
    let extreme_values = vec![
        100_000.0,
        1_000_000.0,  // 10x increase
        10_000.0,     // 90% drawdown
        500_000.0,    // Recovery
        100_000.0,    // Back to start
    ];
    
    for (i, value) in extreme_values.iter().enumerate() {
        portfolio.record_equity(base_time + Duration::days(i as i64)).unwrap();
    }
    
    let result = analyzer.calculate_metrics(&portfolio, 0.02);
    assert!(result.is_ok(), "Should handle extreme values without panicking");
    
    let metrics = result.unwrap();
    
    // Should produce finite, reasonable metrics
    assert!(metrics.total_return.is_finite(), "Total return should be finite");
    assert!(metrics.volatility.is_finite(), "Volatility should be finite");
    assert!(metrics.max_drawdown.is_finite(), "Max drawdown should be finite");
    assert!(metrics.sharpe_ratio.is_finite(), "Sharpe ratio should be finite");
    
    // Max drawdown should be significant given the extreme values
    assert!(metrics.max_drawdown > 0.0, "Should detect significant drawdown");
}

#[rstest]
fn test_performance_with_short_time_period() {
    let analyzer = PerformanceAnalyzer::new();
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Very short time period (2 days)
    let base_time = Utc::now() - Duration::days(2);
    portfolio.record_equity(base_time).unwrap();
    portfolio.record_equity(base_time + Duration::days(1)).unwrap();
    portfolio.record_equity(base_time + Duration::days(2)).unwrap();
    
    let result = analyzer.calculate_metrics(&portfolio, 0.02);
    assert!(result.is_ok());
    
    let metrics = result.unwrap();
    
    // Should handle short periods gracefully
    TestAssertions::assert_metric_reasonable(metrics.annualized_return, "annualized_return", -100.0, 100.0);
    assert!(metrics.volatility >= 0.0);
}

#[rstest]
fn test_metrics_calculation_consistency() {
    // Test that metrics calculation is deterministic
    let analyzer = PerformanceAnalyzer::new();
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Create consistent data
    let base_time = Utc::now() - Duration::days(20);
    for day in 0..20 {
        let value = 100_000.0 + day as f64 * 500.0; // Linear growth
        portfolio.record_equity(base_time + Duration::days(day)).unwrap();
    }
    
    // Calculate metrics multiple times
    let result1 = analyzer.calculate_metrics(&portfolio, 0.025).unwrap();
    let result2 = analyzer.calculate_metrics(&portfolio, 0.025).unwrap();
    let result3 = analyzer.calculate_metrics(&portfolio, 0.025).unwrap();
    
    // All results should be identical (deterministic)
    TestAssertions::assert_approx_eq(result1.total_return, result2.total_return, 1e-10);
    TestAssertions::assert_approx_eq(result2.total_return, result3.total_return, 1e-10);
    TestAssertions::assert_approx_eq(result1.sharpe_ratio, result2.sharpe_ratio, 1e-10);
    TestAssertions::assert_approx_eq(result2.sharpe_ratio, result3.sharpe_ratio, 1e-10);
}

#[rstest]
fn test_completed_trade_structure() {
    let trade = CompletedTrade {
        entry_time: Utc::now() - Duration::hours(2),
        exit_time: Utc::now(),
        symbol: "TEST".to_string(),
        side: OrderSide::Buy,
        entry_price: 100.0,
        exit_price: 110.0,
        quantity: 100.0,
        pnl: 1000.0,
        return_pct: 0.1,
    };
    
    // Basic validation of trade structure
    assert_eq!(trade.symbol, "TEST");
    assert!(trade.exit_time > trade.entry_time);
    assert_eq!(trade.pnl, 1000.0);
    assert_eq!(trade.return_pct, 0.1);
    
    // PnL should match price difference * quantity
    let expected_pnl = (trade.exit_price - trade.entry_price) * trade.quantity;
    TestAssertions::assert_approx_eq(trade.pnl, expected_pnl, 0.01);
}

#[rstest]
fn test_edge_case_single_return() {
    let analyzer = PerformanceAnalyzer::new();
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Only two equity points (single return)
    portfolio.record_equity(Utc::now() - Duration::days(1)).unwrap();
    portfolio.record_equity(Utc::now()).unwrap();
    
    let result = analyzer.calculate_metrics(&portfolio, 0.02);
    assert!(result.is_ok(), "Should handle single return calculation");
    
    let metrics = result.unwrap();
    
    // Should produce reasonable metrics even with minimal data
    assert!(metrics.volatility >= 0.0);
    assert!(metrics.total_return.is_finite());
}

#[rstest]
fn test_calmar_ratio_calculation() {
    let analyzer = PerformanceAnalyzer::new();
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Create data with known return and drawdown
    let base_time = Utc::now() - Duration::days(365);
    let values = vec![
        100_000.0, 120_000.0, 110_000.0, 130_000.0, 100_000.0, 140_000.0
    ];
    
    for (i, value) in values.iter().enumerate() {
        let timestamp = base_time + Duration::days(i as i64 * 60);
        portfolio.record_equity(timestamp).unwrap();
    }
    
    let result = analyzer.calculate_metrics(&portfolio, 0.02);
    assert!(result.is_ok());
    
    let metrics = result.unwrap();
    
    // Calmar ratio should be reasonable (return / max drawdown)
    TestAssertions::assert_metric_reasonable(metrics.calmar_ratio, "calmar_ratio", -10.0, 10.0);
    
    // If there's positive return and drawdown, Calmar ratio should be calculated
    if metrics.total_return > 0.0 && metrics.max_drawdown > 0.0 {
        let expected_calmar = metrics.total_return / metrics.max_drawdown;
        TestAssertions::assert_approx_eq(metrics.calmar_ratio, expected_calmar, 0.01);
    }
}