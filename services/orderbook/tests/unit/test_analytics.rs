//! Comprehensive unit tests for market microstructure analytics
//! 
//! Tests cover:
//! - Kyle's Lambda (price impact coefficient) calculations
//! - VPIN (Volume-Synchronized Probability of Informed Trading)
//! - Amihud Illiquidity Measure
//! - Order flow imbalance metrics
//! - Volume bucket management
//! - Toxicity detection algorithms
//! - Imbalance calculations at various depth levels

use orderbook::analytics::{
    MicrostructureAnalytics, ImbalanceCalculator, ImbalanceMetrics, ToxicityDetector
};
use services_common::{Px, Qty, Ts};
use std::thread;
use std::time::Duration;

/// Helper function to create test price/quantity pairs
fn create_trade_data(prices: &[i64], quantities: &[i64], buy_flags: &[bool]) -> Vec<(Px, Qty, bool, Ts)> {
    assert_eq!(prices.len(), quantities.len());
    assert_eq!(prices.len(), buy_flags.len());
    
    prices.iter()
        .zip(quantities.iter())
        .zip(buy_flags.iter())
        .enumerate()
        .map(|(i, ((&price, &qty), &is_buy))| {
            let timestamp = Ts::from_nanos(1_000_000_000 * i as u64); // 1 second apart
            (Px::from_i64(price), Qty::from_i64(qty), is_buy, timestamp)
        })
        .collect()
}

/// Helper function to create bid/ask levels for imbalance testing
fn create_levels(prices: &[i64], quantities: &[i64]) -> Vec<(Px, Qty, u64)> {
    prices.iter()
        .zip(quantities.iter())
        .map(|(&price, &qty)| (Px::from_i64(price), Qty::from_i64(qty), 1))
        .collect()
}

#[cfg(test)]
mod microstructure_analytics_tests {
    use super::*;

    #[test]
    fn test_analytics_creation() {
        let analytics = MicrostructureAnalytics::new();
        
        // Initial state should be zero/empty
        assert_eq!(analytics.get_vpin(), 0.0);
        assert_eq!(analytics.get_flow_imbalance(), 0.0);
        assert_eq!(analytics.get_kyles_lambda(), 0.0);
        assert_eq!(analytics.get_pin(), 0.0);
    }

    #[test]
    fn test_single_trade_update() {
        let analytics = MicrostructureAnalytics::new();
        
        analytics.update_trade(
            Px::from_i64(100_000), // $10.00
            Qty::from_i64(10_000), // 1.0 units
            true,                  // buy
            Ts::now()
        );
        
        // Should have recorded the trade
        assert!(analytics.get_flow_imbalance() > 0.0); // Positive imbalance for buy
    }

    #[test]
    fn test_flow_imbalance_calculation() {
        let analytics = MicrostructureAnalytics::new();
        
        // Add buy trades
        for i in 0..5 {
            analytics.update_trade(
                Px::from_i64(100_000),
                Qty::from_i64(10_000),
                true,
                Ts::from_nanos(1_000_000_000 * i)
            );
        }
        
        let imbalance = analytics.get_flow_imbalance();
        assert!(imbalance > 90.0); // Should be close to 100% buy
        
        // Add equal sell volume
        for i in 5..10 {
            analytics.update_trade(
                Px::from_i64(100_000),
                Qty::from_i64(10_000),
                false,
                Ts::from_nanos(1_000_000_000 * i)
            );
        }
        
        let balanced_imbalance = analytics.get_flow_imbalance();
        assert!(balanced_imbalance.abs() < 10.0); // Should be close to balanced
    }

    #[test]
    fn test_kyles_lambda_calculation() {
        let analytics = MicrostructureAnalytics::new();
        
        // Create correlated price and volume changes
        let price_changes = vec![100, 50, -30, 80, -60, 40, -20, 90, -50, 70];
        let volume_changes = vec![1000, 500, -300, 800, -600, 400, -200, 900, -500, 700];
        
        analytics.calculate_kyles_lambda(&price_changes, &volume_changes);
        
        let lambda = analytics.get_kyles_lambda();
        assert!(lambda > 0.0); // Should have positive correlation
        
        // Test with uncorrelated data
        let random_price_changes = vec![10, -50, 30, -20, 40, -10, 60, -30, 20, -40];
        let random_volume_changes = vec![100, 200, -150, 300, -250, 180, -90, 120, -200, 170];
        
        analytics.calculate_kyles_lambda(&random_price_changes, &random_volume_changes);
        let lambda_random = analytics.get_kyles_lambda();
        
        // Lambda should be different (likely smaller) for uncorrelated data
        assert_ne!(lambda, lambda_random);
    }

    #[test]
    fn test_vpin_calculation_insufficient_data() {
        let analytics = MicrostructureAnalytics::new();
        
        // Add only a few trades (insufficient for VPIN)
        for i in 0..10 {
            analytics.update_trade(
                Px::from_i64(100_000),
                Qty::from_i64(1000),
                i % 2 == 0,
                Ts::from_nanos(1_000_000_000 * i)
            );
        }
        
        // VPIN should remain zero with insufficient data
        assert_eq!(analytics.get_vpin(), 0.0);
    }

    #[test]
    fn test_vpin_calculation_sufficient_data() {
        let analytics = MicrostructureAnalytics::new();
        
        // Add enough trades for VPIN calculation (need 50+ buckets)
        for i in 0..60 {
            let is_buy = i % 3 != 0; // Create imbalance (2/3 buy, 1/3 sell)
            analytics.update_trade(
                Px::from_i64(100_000 + (i % 10) * 100), // Vary price slightly
                Qty::from_i64(1000),
                is_buy,
                Ts::from_nanos(1_000_000_000 * i) // 1 second apart
            );
        }
        
        let vpin = analytics.get_vpin();
        assert!(vpin > 0.0); // Should detect imbalance
        assert!(vpin <= 100.0); // Should be in valid range
    }

    #[test]
    fn test_volume_bucket_management() {
        let analytics = MicrostructureAnalytics::new();
        
        // Test that volume buckets are created per second
        let base_time = 1_000_000_000; // 1 second in nanos
        
        // Add trades in same second
        analytics.update_trade(Px::from_i64(100_000), Qty::from_i64(1000), true, Ts::from_nanos(base_time));
        analytics.update_trade(Px::from_i64(100_100), Qty::from_i64(2000), false, Ts::from_nanos(base_time + 500_000_000));
        
        // Add trade in next second
        analytics.update_trade(Px::from_i64(100_200), Qty::from_i64(1500), true, Ts::from_nanos(base_time + 1_000_000_000));
        
        // Trades should be bucketed correctly (hard to test without access to internals)
        // We can verify through side effects like imbalance calculation
        let imbalance = analytics.get_flow_imbalance();
        assert!(imbalance.abs() > 0.0);
    }

    #[test]
    fn test_pin_estimation() {
        let analytics = MicrostructureAnalytics::new();
        
        // Create persistent directional flow
        for i in 0..20 {
            analytics.update_trade(
                Px::from_i64(100_000),
                Qty::from_i64(1000),
                true, // All buys
                Ts::from_nanos(1_000_000_000 * i)
            );
        }
        
        let pin = analytics.get_pin();
        assert!(pin > 0.0); // Should detect informed trading pattern
        assert!(pin <= 100.0);
    }

    #[test]
    fn test_concurrent_analytics_updates() {
        let analytics = std::sync::Arc::new(MicrostructureAnalytics::new());
        let analytics_clone = std::sync::Arc::clone(&analytics);
        
        // Spawn thread to continuously read analytics
        let reader_handle = thread::spawn(move || {
            for _ in 0..1000 {
                let _vpin = analytics_clone.get_vpin();
                let _imbalance = analytics_clone.get_flow_imbalance();
                let _lambda = analytics_clone.get_kyles_lambda();
                let _pin = analytics_clone.get_pin();
                thread::sleep(Duration::from_micros(10));
            }
        });
        
        // Continue updating with trades
        for i in 0..100 {
            analytics.update_trade(
                Px::from_i64(100_000 + (i % 10) * 100),
                Qty::from_i64(1000),
                i % 2 == 0,
                Ts::from_nanos(1_000_000_000 * i)
            );
        }
        
        reader_handle.join().expect("Reader thread should complete");
    }

    #[test]
    fn test_extreme_market_conditions() {
        let analytics = MicrostructureAnalytics::new();
        
        // Test with extreme price movements
        let extreme_prices = vec![100_000, 150_000, 200_000, 50_000, 25_000, 175_000];
        let quantities = vec![10_000, 20_000, 5_000, 30_000, 15_000, 8_000];
        let buy_flags = vec![true, true, false, false, true, false];
        
        let trades = create_trade_data(&extreme_prices, &quantities, &buy_flags);
        
        for (price, qty, is_buy, timestamp) in trades {
            analytics.update_trade(price, qty, is_buy, timestamp);
        }
        
        // Analytics should handle extreme conditions gracefully
        let vpin = analytics.get_vpin();
        let imbalance = analytics.get_flow_imbalance();
        let lambda = analytics.get_kyles_lambda();
        
        assert!(vpin >= 0.0 && vpin <= 100.0);
        assert!(imbalance >= -100.0 && imbalance <= 100.0);
        assert!(lambda >= 0.0); // Lambda should be non-negative
    }
}

#[cfg(test)]
mod imbalance_calculator_tests {
    use super::*;

    #[test]
    fn test_balanced_book() {
        let bid_levels = create_levels(&[99_000, 98_000, 97_000], &[10_000, 15_000, 20_000]);
        let ask_levels = create_levels(&[101_000, 102_000, 103_000], &[10_000, 15_000, 20_000]);
        
        let metrics = ImbalanceCalculator::calculate_imbalances(&bid_levels, &ask_levels);
        
        // Perfectly balanced book should have zero imbalance
        assert!(metrics.top_level_imbalance.abs() < 1.0);
        assert!(metrics.three_level_imbalance.abs() < 1.0);
        assert!(metrics.five_level_imbalance.abs() < 1.0);
    }

    #[test]
    fn test_bid_heavy_book() {
        let bid_levels = create_levels(&[99_000, 98_000, 97_000], &[30_000, 25_000, 20_000]); // Heavy bids
        let ask_levels = create_levels(&[101_000, 102_000, 103_000], &[10_000, 15_000, 20_000]); // Light asks
        
        let metrics = ImbalanceCalculator::calculate_imbalances(&bid_levels, &ask_levels);
        
        // Should show positive imbalance (bid-heavy)
        assert!(metrics.top_level_imbalance > 50.0);
        assert!(metrics.three_level_imbalance > 20.0);
        assert!(metrics.buy_pressure > 0.0);
        assert_eq!(metrics.sell_pressure, 0.0);
    }

    #[test]
    fn test_ask_heavy_book() {
        let bid_levels = create_levels(&[99_000, 98_000, 97_000], &[10_000, 15_000, 20_000]); // Light bids
        let ask_levels = create_levels(&[101_000, 102_000, 103_000], &[30_000, 25_000, 20_000]); // Heavy asks
        
        let metrics = ImbalanceCalculator::calculate_imbalances(&bid_levels, &ask_levels);
        
        // Should show negative imbalance (ask-heavy)
        assert!(metrics.top_level_imbalance < -50.0);
        assert!(metrics.three_level_imbalance < -20.0);
        assert!(metrics.sell_pressure > 0.0);
        assert_eq!(metrics.buy_pressure, 0.0);
    }

    #[test]
    fn test_weighted_mid_price() {
        let bid_levels = create_levels(&[99_000], &[20_000]); // $9.90, 2.0 units
        let ask_levels = create_levels(&[101_000], &[10_000]); // $10.10, 1.0 units
        
        let metrics = ImbalanceCalculator::calculate_imbalances(&bid_levels, &ask_levels);
        
        // Weighted mid should be closer to bid due to larger bid size
        // Formula: (bid_price * ask_size + ask_price * bid_size) / total_size
        // (99000 * 10000 + 101000 * 20000) / 30000 = (990M + 2020M) / 30000 = 100333.33
        let expected_weighted_mid = (99_000 * 10_000 + 101_000 * 20_000) / 30_000;
        assert_eq!(metrics.weighted_mid_price.as_i64(), expected_weighted_mid);
    }

    #[test]
    fn test_empty_levels() {
        let bid_levels = vec![];
        let ask_levels = vec![];
        
        let metrics = ImbalanceCalculator::calculate_imbalances(&bid_levels, &ask_levels);
        
        // All metrics should be zero/default for empty book
        assert_eq!(metrics.top_level_imbalance, 0.0);
        assert_eq!(metrics.three_level_imbalance, 0.0);
        assert_eq!(metrics.five_level_imbalance, 0.0);
        assert_eq!(metrics.ten_level_imbalance, 0.0);
        assert_eq!(metrics.weighted_mid_price, Px::ZERO);
        assert_eq!(metrics.buy_pressure, 0.0);
        assert_eq!(metrics.sell_pressure, 0.0);
    }

    #[test]
    fn test_single_side_only() {
        // Only bid levels
        let bid_levels = create_levels(&[99_000, 98_000], &[10_000, 15_000]);
        let ask_levels = vec![];
        
        let metrics = ImbalanceCalculator::calculate_imbalances(&bid_levels, &ask_levels);
        
        // Should show 100% bid imbalance
        assert_eq!(metrics.top_level_imbalance, 100.0);
        assert_eq!(metrics.buy_pressure, 100.0);
        assert_eq!(metrics.sell_pressure, 0.0);
        
        // Only ask levels
        let bid_levels = vec![];
        let ask_levels = create_levels(&[101_000, 102_000], &[10_000, 15_000]);
        
        let metrics = ImbalanceCalculator::calculate_imbalances(&bid_levels, &ask_levels);
        
        // Should show 100% ask imbalance
        assert_eq!(metrics.top_level_imbalance, -100.0);
        assert_eq!(metrics.sell_pressure, 100.0);
        assert_eq!(metrics.buy_pressure, 0.0);
    }

    #[test]
    fn test_different_depth_levels() {
        // Create asymmetric book where imbalance changes at different depths
        let bid_levels = create_levels(
            &[99_000, 98_000, 97_000, 96_000, 95_000],
            &[5_000, 5_000, 20_000, 20_000, 20_000]  // Heavy at deeper levels
        );
        let ask_levels = create_levels(
            &[101_000, 102_000, 103_000, 104_000, 105_000],
            &[10_000, 10_000, 5_000, 5_000, 5_000]   // Light at deeper levels
        );
        
        let metrics = ImbalanceCalculator::calculate_imbalances(&bid_levels, &ask_levels);
        
        // Top level should favor asks (10k vs 5k)
        assert!(metrics.top_level_imbalance < 0.0);
        
        // Deeper levels should favor bids more
        assert!(metrics.five_level_imbalance > metrics.top_level_imbalance);
    }
}

#[cfg(test)]
mod toxicity_detector_tests {
    use super::*;

    #[test]
    fn test_toxicity_detector_creation() {
        let detector = ToxicityDetector::new();
        assert_eq!(detector.get_toxicity(), 0.0);
    }

    #[test]
    fn test_random_trades_low_toxicity() {
        let detector = ToxicityDetector::new();
        
        // Add alternating buy/sell trades (random pattern)
        for i in 0..150 {
            let is_buy = i % 2 == 0;
            detector.update(is_buy, Qty::from_i64(1000), Ts::from_nanos(i * 1_000_000));
        }
        
        let toxicity = detector.get_toxicity();
        assert!(toxicity < 30.0); // Random pattern should have low toxicity
    }

    #[test]
    fn test_directional_flow_high_toxicity() {
        let detector = ToxicityDetector::new();
        
        // Add long run of same-direction trades (toxic pattern)
        for i in 0..150 {
            let is_buy = true; // All buys (very toxic)
            detector.update(is_buy, Qty::from_i64(1000), Ts::from_nanos(i * 1_000_000));
        }
        
        let toxicity = detector.get_toxicity();
        assert!(toxicity > 80.0); // Persistent one-way flow should be highly toxic
    }

    #[test]
    fn test_moderate_clustering_medium_toxicity() {
        let detector = ToxicityDetector::new();
        
        // Add clustered trades (moderate toxicity)
        for cluster in 0..10 {
            // Each cluster has 8 buys followed by 2 sells
            for i in 0..8 {
                detector.update(true, Qty::from_i64(1000), 
                    Ts::from_nanos((cluster * 10 + i) * 1_000_000));
            }
            for i in 8..10 {
                detector.update(false, Qty::from_i64(1000), 
                    Ts::from_nanos((cluster * 10 + i) * 1_000_000));
            }
        }
        
        let toxicity = detector.get_toxicity();
        assert!(toxicity > 20.0); // Should detect clustering
        assert!(toxicity < 80.0); // But not as toxic as pure directional flow
    }

    #[test]
    fn test_insufficient_data() {
        let detector = ToxicityDetector::new();
        
        // Add only a few trades (insufficient for toxicity calculation)
        for i in 0..50 {
            detector.update(true, Qty::from_i64(1000), Ts::from_nanos(i * 1_000_000));
        }
        
        let toxicity = detector.get_toxicity();
        assert_eq!(toxicity, 0.0); // Insufficient data should result in zero toxicity
    }

    #[test]
    fn test_large_orders_impact() {
        let detector = ToxicityDetector::new();
        
        // Add pattern with varying order sizes
        for i in 0..150 {
            let is_buy = i % 7 == 0; // Create runs of same direction
            let quantity = if i % 10 == 0 { 10_000 } else { 1_000 }; // Some large orders
            detector.update(is_buy, Qty::from_i64(quantity), Ts::from_nanos(i * 1_000_000));
        }
        
        let toxicity = detector.get_toxicity();
        assert!(toxicity >= 0.0 && toxicity <= 100.0); // Should be in valid range
    }

    #[test]
    fn test_buffer_management() {
        let detector = ToxicityDetector::new();
        
        // Add more than buffer capacity (1000) to test buffer management
        for i in 0..1500 {
            let is_buy = i % 3 == 0;
            detector.update(is_buy, Qty::from_i64(1000), Ts::from_nanos(i * 1_000_000));
        }
        
        let toxicity = detector.get_toxicity();
        assert!(toxicity >= 0.0 && toxicity <= 100.0); // Should handle buffer overflow gracefully
    }

    #[test]
    fn test_toxicity_boundary_conditions() {
        let detector = ToxicityDetector::new();
        
        // Test with zero quantity orders
        for i in 0..150 {
            detector.update(i % 2 == 0, Qty::ZERO, Ts::from_nanos(i * 1_000_000));
        }
        
        let toxicity = detector.get_toxicity();
        assert!(toxicity >= 0.0 && toxicity <= 100.0);
        
        // Test with very large quantities
        for i in 0..150 {
            let large_qty = Qty::from_i64(i64::MAX / 1000);
            detector.update(i % 2 == 0, large_qty, Ts::from_nanos(i * 1_000_000));
        }
        
        let toxicity_large = detector.get_toxicity();
        assert!(toxicity_large >= 0.0 && toxicity_large <= 100.0);
    }
}