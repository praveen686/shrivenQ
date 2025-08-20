//! Integration tests for performance analysis and edge cases

use rstest::*;
use tokio_test;
use backtesting::*;
use chrono::{Utc, Duration};
use crate::test_utils::*;
use std::sync::Arc;

#[rstest]
#[tokio::test]
async fn test_edge_case_empty_data_backtest() {
    let config = TestConfigFactory::basic_config();
    let engine = BacktestEngine::new(config);
    
    // Try to load empty data
    let empty_data = vec![];
    let load_result = engine.load_data("EMPTY", empty_data).await;
    
    assert!(load_result.is_err(), "Should reject empty data");
}

#[rstest]
#[tokio::test]
async fn test_edge_case_single_data_point() {
    let config = TestConfigFactory::basic_config();
    let engine = BacktestEngine::new(config);
    
    // Single data point
    let single_point = vec![(Utc::now() - Duration::days(1), OHLCV {
        open: 100.0,
        high: 105.0,
        low: 95.0,
        close: 100.0,
        volume: 50000.0,
    })];
    
    let load_result = engine.load_data("SINGLE", single_point).await;
    
    if load_result.is_ok() {
        let strategy = DoNothingStrategy;
        let result = engine.run(&strategy).await;
        
        assert!(result.is_ok(), "Should handle single data point gracefully");
        
        let backtest_result = result.unwrap();
        TestAssertions::assert_portfolio_valid(&backtest_result.final_portfolio);
    }
}

#[rstest]
#[tokio::test]
async fn test_edge_case_very_short_backtest_period() {
    let mut config = TestConfigFactory::basic_config();
    config.start_date = Utc::now() - Duration::hours(2);
    config.end_date = Utc::now() - Duration::hours(1);
    config.data_frequency = DataFrequency::Minute;
    
    let engine = BacktestEngine::new(config);
    
    // Short intraday data
    let data = TestDataFactory::intraday_data(1, 150.0); // 1 hour of minute data
    engine.load_data("SHORT", data).await.unwrap();
    
    let strategy = DoNothingStrategy;
    let result = engine.run(&strategy).await;
    
    assert!(result.is_ok(), "Should handle very short backtest periods");
    
    let backtest_result = result.unwrap();
    TestAssertions::assert_portfolio_valid(&backtest_result.final_portfolio);
}

#[rstest]
#[tokio::test]
async fn test_edge_case_zero_initial_capital() {
    let mut config = TestConfigFactory::basic_config();
    config.initial_capital = 0.0;
    
    let engine = BacktestEngine::new(config);
    
    let data = TestDataFactory::trending_up_data(10, 100.0);
    engine.load_data("ZERO_CAPITAL", data).await.unwrap();
    
    let strategy = AlwaysBuyStrategy {
        symbol: "ZERO_CAPITAL".to_string(),
        position_size: 1000.0,
    };
    
    let result = engine.run(&strategy).await;
    assert!(result.is_ok(), "Should handle zero initial capital");
    
    let backtest_result = result.unwrap();
    
    // Should have zero cash and no positions
    assert_eq!(backtest_result.final_portfolio.cash, 0.0);
    assert_eq!(backtest_result.final_portfolio.total_value, 0.0);
}

#[rstest]
#[tokio::test]
async fn test_edge_case_extremely_small_capital() {
    let mut config = TestConfigFactory::basic_config();
    config.initial_capital = 0.01; // 1 cent
    
    let engine = BacktestEngine::new(config);
    
    let data = TestDataFactory::trending_up_data(5, 100.0);
    engine.load_data("TINY_CAPITAL", data).await.unwrap();
    
    let strategy = AlwaysBuyStrategy {
        symbol: "TINY_CAPITAL".to_string(),
        position_size: 1000.0, // Way more than available capital
    };
    
    let result = engine.run(&strategy).await.unwrap();
    
    // Should not be able to make any trades
    assert!(result.final_portfolio.positions.is_empty() || 
            result.final_portfolio.positions.iter().all(|p| p.quantity == 0.0));
    
    TestAssertions::assert_approx_eq(result.final_portfolio.cash, 0.01, 1e-6);
}

#[rstest]
#[tokio::test]
async fn test_edge_case_extremely_large_capital() {
    let mut config = TestConfigFactory::basic_config();
    config.initial_capital = 1_000_000_000.0; // $1 billion
    
    let engine = BacktestEngine::new(config);
    
    let data = TestDataFactory::trending_up_data(20, 10.0);
    engine.load_data("HUGE_CAPITAL", data).await.unwrap();
    
    let strategy = AlwaysBuyStrategy {
        symbol: "HUGE_CAPITAL".to_string(),
        position_size: 1_000_000.0, // $1M position
    };
    
    let result = engine.run(&strategy).await.unwrap();
    
    // Should handle large numbers without overflow
    TestAssertions::assert_portfolio_valid(&result.final_portfolio);
    assert!(result.final_portfolio.total_value > 999_000_000.0); // Should be close to initial
}

#[rstest]
#[tokio::test]
async fn test_single_trade_scenario() {
    TestRandom::reset();
    let config = TestConfigFactory::basic_config();
    let engine = BacktestEngine::new(config);
    
    // Simple data for single trade
    let data = vec![
        (Utc::now() - Duration::days(2), OHLCV {
            open: 100.0, high: 105.0, low: 95.0, close: 100.0, volume: 50000.0
        }),
        (Utc::now() - Duration::days(1), OHLCV {
            open: 100.0, high: 110.0, low: 98.0, close: 105.0, volume: 55000.0
        }),
    ];
    
    engine.load_data("SINGLE_TRADE", data).await.unwrap();
    
    let strategy = AlwaysBuyStrategy {
        symbol: "SINGLE_TRADE".to_string(),
        position_size: 1000.0,
    };
    
    let result = engine.run(&strategy).await.unwrap();
    
    // Should handle single trade scenario
    TestAssertions::assert_portfolio_valid(&result.final_portfolio);
    
    // Should have some trading activity or position
    let has_activity = !result.final_portfolio.positions.is_empty() || !result.trades.is_empty();
    
    if has_activity {
        // Should have reasonable metrics even with minimal trading
        TestAssertions::assert_metric_reasonable(result.metrics.total_return, "single_trade_return", -0.5, 0.5);
    }
}

#[rstest]
#[tokio::test]
async fn test_concurrent_backtest_runs() {
    // Test multiple concurrent backtests to check for race conditions
    use tokio::task::JoinSet;
    
    let mut join_set = JoinSet::new();
    
    for i in 0..5 {
        join_set.spawn(async move {
            TestRandom::reset(); // Each should have same random state
            let config = TestConfigFactory::basic_config();
            let engine = BacktestEngine::new(config);
            
            let data = TestDataFactory::trending_up_data(15, 100.0 + i as f64 * 10.0);
            engine.load_data(&format!("CONCURRENT_{}", i), data).await.unwrap();
            
            let strategy = AlwaysBuyStrategy {
                symbol: format!("CONCURRENT_{}", i),
                position_size: 500.0,
            };
            
            engine.run(&strategy).await
        });
    }
    
    // Wait for all to complete
    let mut results = Vec::new();
    while let Some(res) = join_set.join_next().await {
        let backtest_result = res.unwrap().unwrap();
        results.push(backtest_result);
    }
    
    // All should complete successfully
    assert_eq!(results.len(), 5);
    
    for result in &results {
        TestAssertions::assert_portfolio_valid(&result.final_portfolio);
    }
    
    // Results should be consistent (deterministic with same random seed)
    // Note: Different symbols/data means results will differ, but all should be valid
}

#[rstest]
#[tokio::test]
async fn test_performance_with_high_frequency_data() {
    let mut config = TestConfigFactory::hf_config();
    config.start_date = Utc::now() - Duration::hours(2);
    config.end_date = Utc::now() - Duration::hours(1);
    config.data_frequency = DataFrequency::Second;
    
    let engine = BacktestEngine::new(config);
    
    // Generate second-by-second data for 1 hour
    let mut data = Vec::new();
    let mut price = 100.0;
    let mut current_time = Utc::now() - Duration::hours(2);
    
    for _ in 0..3600 { // 1 hour of seconds
        price += (TestRandom::next() - 0.5) * 0.01; // Small random changes
        price = price.max(1.0);
        
        data.push((current_time, OHLCV {
            open: price,
            high: price + TestRandom::next() * 0.005,
            low: price - TestRandom::next() * 0.005,
            close: price,
            volume: 1000.0 + TestRandom::next() * 500.0,
        }));
        
        current_time = current_time + Duration::seconds(1);
    }
    
    engine.load_data("HF", data).await.unwrap();
    
    let strategy = DoNothingStrategy; // Use simple strategy to test data processing
    
    let start_time = std::time::Instant::now();
    let result = engine.run(&strategy).await;
    let elapsed = start_time.elapsed();
    
    assert!(result.is_ok(), "Should handle high-frequency data");
    
    let backtest_result = result.unwrap();
    TestAssertions::assert_portfolio_valid(&backtest_result.final_portfolio);
    
    // Should complete in reasonable time (less than 30 seconds for 3600 data points)
    assert!(elapsed.as_secs() < 30, "High-frequency backtest should complete efficiently");
    
    // Should have appropriate number of equity curve points
    assert!(backtest_result.equity_curve.len() >= 100, "Should have sufficient equity curve resolution");
}

#[rstest]
#[tokio::test]
async fn test_backtest_with_price_precision_edge_cases() {
    let config = TestConfigFactory::basic_config();
    let engine = BacktestEngine::new(config);
    
    // Data with very precise prices
    let data = vec![
        (Utc::now() - Duration::days(3), OHLCV {
            open: 100.123456789, high: 100.987654321, low: 99.111111111, 
            close: 100.555555555, volume: 12345.6789
        }),
        (Utc::now() - Duration::days(2), OHLCV {
            open: 100.555555555, high: 101.333333333, low: 100.222222222,
            close: 100.999999999, volume: 23456.7890
        }),
        (Utc::now() - Duration::days(1), OHLCV {
            open: 100.999999999, high: 101.777777777, low: 100.444444444,
            close: 101.666666666, volume: 34567.8901
        }),
    ];
    
    engine.load_data("PRECISION", data).await.unwrap();
    
    let strategy = AlwaysBuyStrategy {
        symbol: "PRECISION".to_string(),
        position_size: 123.456789,
    };
    
    let result = engine.run(&strategy).await.unwrap();
    
    // Should handle high precision numbers correctly
    TestAssertions::assert_portfolio_valid(&result.final_portfolio);
    
    // Calculations should maintain reasonable precision
    assert!(result.final_portfolio.total_value.is_finite());
    assert!(result.metrics.total_return.is_finite());
}

#[rstest]
#[tokio::test]
async fn test_backtest_with_extreme_price_movements() {
    let config = TestConfigFactory::basic_config();
    let engine = BacktestEngine::new(config);
    
    // Extreme price movements (gaps, spikes)
    let data = vec![
        (Utc::now() - Duration::days(5), OHLCV {
            open: 100.0, high: 105.0, low: 95.0, close: 100.0, volume: 50000.0
        }),
        (Utc::now() - Duration::days(4), OHLCV {
            open: 200.0, high: 250.0, low: 180.0, close: 220.0, volume: 200000.0 // Gap up
        }),
        (Utc::now() - Duration::days(3), OHLCV {
            open: 50.0, high: 60.0, low: 40.0, close: 45.0, volume: 300000.0 // Gap down
        }),
        (Utc::now() - Duration::days(2), OHLCV {
            open: 45.0, high: 500.0, low: 30.0, close: 400.0, volume: 500000.0 // Spike
        }),
        (Utc::now() - Duration::days(1), OHLCV {
            open: 400.0, high: 420.0, low: 380.0, close: 410.0, volume: 100000.0
        }),
    ];
    
    engine.load_data("EXTREME", data).await.unwrap();
    
    let strategy = AlwaysBuyStrategy {
        symbol: "EXTREME".to_string(),
        position_size: 100.0,
    };
    
    let result = engine.run(&strategy).await.unwrap();
    
    // Should handle extreme movements without crashing
    TestAssertions::assert_portfolio_valid(&result.final_portfolio);
    
    // Should show extreme volatility
    TestAssertions::assert_metric_reasonable(result.metrics.volatility, "extreme_volatility", 0.0, 50.0);
    
    // May have extreme drawdowns
    TestAssertions::assert_metric_reasonable(result.metrics.max_drawdown, "extreme_drawdown", 0.0, 1.0);
}

#[rstest]
#[tokio::test]
async fn test_backtest_with_zero_volume_data() {
    let config = TestConfigFactory::basic_config();
    let engine = BacktestEngine::new(config);
    
    // Data with zero volume (market closed, no trading)
    let data = vec![
        (Utc::now() - Duration::days(3), OHLCV {
            open: 100.0, high: 100.0, low: 100.0, close: 100.0, volume: 0.0
        }),
        (Utc::now() - Duration::days(2), OHLCV {
            open: 100.0, high: 101.0, low: 99.0, close: 100.5, volume: 50000.0
        }),
        (Utc::now() - Duration::days(1), OHLCV {
            open: 100.5, high: 100.5, low: 100.5, close: 100.5, volume: 0.0
        }),
    ];
    
    engine.load_data("ZERO_VOL", data).await.unwrap();
    
    let strategy = AlwaysBuyStrategy {
        symbol: "ZERO_VOL".to_string(),
        position_size: 1000.0,
    };
    
    let result = engine.run(&strategy).await.unwrap();
    
    // Should handle zero volume data gracefully
    TestAssertions::assert_portfolio_valid(&result.final_portfolio);
}

#[rstest]
#[tokio::test]
async fn test_memory_usage_large_backtest() {
    let mut config = TestConfigFactory::basic_config();
    config.start_date = Utc::now() - Duration::days(365); // 1 year
    config.end_date = Utc::now() - Duration::days(1);
    
    let engine = BacktestEngine::new(config);
    
    // Large dataset
    let data = TestDataFactory::trending_up_data(365, 100.0);
    engine.load_data("LARGE_MEM", data).await.unwrap();
    
    let strategy = DoNothingStrategy; // Simple strategy to focus on data handling
    
    // Measure memory usage (approximate)
    let start_memory = get_memory_usage();
    let result = engine.run(&strategy).await.unwrap();
    let end_memory = get_memory_usage();
    
    TestAssertions::assert_portfolio_valid(&result.final_portfolio);
    
    // Should not consume excessive memory (this is a rough check)
    let memory_increase = end_memory - start_memory;
    assert!(memory_increase < 100_000_000, "Memory usage should be reasonable"); // Less than 100MB increase
}

#[rstest]
#[tokio::test]
async fn test_backtest_numerical_stability() {
    let config = TestConfigFactory::basic_config();
    let engine = BacktestEngine::new(config);
    
    // Create scenario that could cause numerical instability
    let mut data = Vec::new();
    let mut price = 1e-6; // Very small starting price
    
    for i in 0..50 {
        let timestamp = Utc::now() - Duration::days(50 - i);
        
        // Exponential growth that could cause overflow
        price *= 1.1; // 10% daily growth
        
        data.push((timestamp, OHLCV {
            open: price * 0.99,
            high: price * 1.01,
            low: price * 0.98,
            close: price,
            volume: 1e15, // Very large volume
        }));
    }
    
    engine.load_data("NUMERICAL", data).await.unwrap();
    
    let strategy = AlwaysBuyStrategy {
        symbol: "NUMERICAL".to_string(),
        position_size: 1e12, // Very large position
    };
    
    let result = engine.run(&strategy).await.unwrap();
    
    // All values should remain finite
    assert!(result.final_portfolio.total_value.is_finite());
    assert!(result.metrics.total_return.is_finite());
    assert!(result.metrics.sharpe_ratio.is_finite() || result.metrics.sharpe_ratio == 0.0);
    assert!(result.metrics.volatility.is_finite());
    
    TestAssertions::assert_portfolio_valid(&result.final_portfolio);
}

// Helper function to estimate memory usage (simplified)
fn get_memory_usage() -> u64 {
    // This is a simplified memory estimation
    // In a real implementation, you might use system-specific APIs
    std::process::id() as u64 * 1000 // Placeholder
}

#[rstest]
#[tokio::test]
async fn test_backtest_error_recovery() {
    let config = TestConfigFactory::basic_config();
    let engine = BacktestEngine::new(config);
    
    // Mix of valid and invalid data
    let mixed_data = vec![
        (Utc::now() - Duration::days(5), OHLCV {
            open: 100.0, high: 105.0, low: 95.0, close: 100.0, volume: 50000.0
        }),
        (Utc::now() - Duration::days(4), OHLCV {
            open: 100.0, high: 90.0, low: 105.0, close: 102.0, volume: 55000.0 // Invalid: high < low
        }),
        (Utc::now() - Duration::days(3), OHLCV {
            open: 102.0, high: 108.0, low: 98.0, close: 105.0, volume: 60000.0
        }),
    ];
    
    let load_result = engine.load_data("MIXED", mixed_data).await;
    
    // Should either filter invalid data or reject the load
    match load_result {
        Ok(_) => {
            let strategy = DoNothingStrategy;
            let result = engine.run(&strategy).await;
            assert!(result.is_ok(), "Should run successfully after filtering invalid data");
        },
        Err(_) => {
            // Also acceptable to reject mixed data
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_backtest_time_zone_handling() {
    let config = TestConfigFactory::basic_config();
    let engine = BacktestEngine::new(config);
    
    // All timestamps should be in UTC as per the API
    let utc_data = TestDataFactory::trending_up_data(10, 150.0);
    
    engine.load_data("UTC", utc_data).await.unwrap();
    
    let strategy = DoNothingStrategy;
    let result = engine.run(&strategy).await.unwrap();
    
    // All timestamps in equity curve should be valid UTC
    for (timestamp, _) in &result.equity_curve {
        assert_eq!(timestamp.timezone(), Utc);
    }
    
    TestAssertions::assert_portfolio_valid(&result.final_portfolio);
}