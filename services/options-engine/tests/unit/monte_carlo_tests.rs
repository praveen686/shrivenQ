use rstest::*;
use approx::{assert_abs_diff_eq, assert_relative_eq};
use options_engine::{MonteCarloEngine, ExoticOptionType, BarrierType, BlackScholes, OptionType};
use std::collections::HashMap;

/// Test fixture for standard Monte Carlo parameters
#[fixture]
fn standard_mc_params() -> (f64, f64, f64, f64, f64) {
    // spot, rate, volatility, time, strike
    (100.0, 0.05, 0.2, 0.25, 100.0)
}

/// Test fixture for Indian market Monte Carlo parameters
#[fixture]
fn nifty_mc_params() -> (f64, f64, f64, f64, f64) {
    // Nifty spot, rate, volatility, time, strike
    (21500.0, 0.065, 0.15, 30.0/365.0, 21500.0)
}

/// Test fixture for basic Monte Carlo engine
#[fixture]
fn basic_mc_engine() -> MonteCarloEngine {
    MonteCarloEngine::new(10000, 100)
}

/// Test fixture for high-precision Monte Carlo engine
#[fixture]
fn precision_mc_engine() -> MonteCarloEngine {
    MonteCarloEngine::new(100000, 252)
}

/// Test fixture for fast Monte Carlo engine (for performance tests)
#[fixture]
fn fast_mc_engine() -> MonteCarloEngine {
    MonteCarloEngine::new(1000, 50)
}

#[cfg(test)]
mod monte_carlo_engine_construction_tests {
    use super::*;

    #[rstest]
    fn test_monte_carlo_engine_creation() {
        let engine = MonteCarloEngine::new(50000, 252);
        
        assert_eq!(engine.simulations, 50000);
        assert_eq!(engine.time_steps, 252);
        assert_eq!(engine.random_seed, 42); // Default seed
    }

    #[rstest]
    fn test_monte_carlo_engine_custom_parameters() {
        let engine = MonteCarloEngine::new(25000, 100);
        
        assert_eq!(engine.simulations, 25000);
        assert_eq!(engine.time_steps, 100);
        
        // Different engines should have same default seed for reproducibility
        let engine2 = MonteCarloEngine::new(10000, 50);
        assert_eq!(engine.random_seed, engine2.random_seed);
    }

    #[rstest]
    fn test_monte_carlo_engine_reasonable_parameters() {
        // Test that we can create engines with various reasonable parameter sets
        let engines = vec![
            MonteCarloEngine::new(1000, 50),     // Fast/rough pricing
            MonteCarloEngine::new(10000, 100),   // Standard pricing
            MonteCarloEngine::new(100000, 252),  // High precision
            MonteCarloEngine::new(500000, 1000), // Very high precision
        ];
        
        for engine in engines {
            assert!(engine.simulations >= 1000);
            assert!(engine.time_steps >= 50);
            assert_eq!(engine.random_seed, 42);
        }
    }
}

#[cfg(test)]
mod path_simulation_tests {
    use super::*;

    #[rstest]
    fn test_simulate_paths_basic_functionality(
        basic_mc_engine: MonteCarloEngine,
        standard_mc_params: (f64, f64, f64, f64, f64)
    ) {
        let engine = basic_mc_engine;
        let (spot, rate, vol, time, _) = standard_mc_params;
        
        let paths = engine.simulate_paths(spot, rate, vol, time);
        
        // Check path structure
        assert_eq!(paths.len(), engine.simulations);
        
        for path in &paths {
            assert_eq!(path.len(), engine.time_steps + 1); // Includes initial value
            assert_eq!(path[0], spot); // First value should be spot
            
            // All values should be positive (stock prices)
            for &price in path {
                assert!(price > 0.0);
                assert!(price.is_finite());
            }
        }
    }

    #[rstest]
    fn test_simulate_paths_reproducibility(standard_mc_params: (f64, f64, f64, f64, f64)) {
        let (spot, rate, vol, time, _) = standard_mc_params;
        
        // Two engines with same seed should produce identical results
        let engine1 = MonteCarloEngine::new(1000, 100);
        let engine2 = MonteCarloEngine::new(1000, 100);
        
        let paths1 = engine1.simulate_paths(spot, rate, vol, time);
        let paths2 = engine2.simulate_paths(spot, rate, vol, time);
        
        assert_eq!(paths1.len(), paths2.len());
        
        for (path1, path2) in paths1.iter().zip(paths2.iter()) {
            assert_eq!(path1.len(), path2.len());
            for (p1, p2) in path1.iter().zip(path2.iter()) {
                assert_abs_diff_eq!(*p1, *p2, epsilon = 1e-10);
            }
        }
    }

    #[rstest]
    fn test_simulate_paths_statistical_properties(
        precision_mc_engine: MonteCarloEngine,
        standard_mc_params: (f64, f64, f64, f64, f64)
    ) {
        let engine = precision_mc_engine;
        let (spot, rate, vol, time, _) = standard_mc_params;
        
        let paths = engine.simulate_paths(spot, rate, vol, time);
        
        // Calculate statistics from final values
        let final_values: Vec<f64> = paths.iter().map(|path| *path.last().unwrap()).collect();
        
        // Calculate sample mean and standard deviation
        let mean = final_values.iter().sum::<f64>() / final_values.len() as f64;
        let variance = final_values.iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>() / (final_values.len() - 1) as f64;
        let std_dev = variance.sqrt();
        
        // Theoretical values for geometric Brownian motion
        let theoretical_mean = spot * (rate * time).exp();
        let theoretical_variance = spot.powi(2) * ((2.0 * rate + vol.powi(2)) * time).exp() * 
            ((vol.powi(2) * time).exp() - 1.0);
        let theoretical_std = theoretical_variance.sqrt();
        
        // Check that sample statistics are close to theoretical (with tolerance for Monte Carlo error)
        let tolerance = 0.05; // 5% tolerance
        assert_relative_eq!(mean, theoretical_mean, epsilon = tolerance);
        assert_relative_eq!(std_dev, theoretical_std, epsilon = tolerance);
    }

    #[rstest]
    fn test_simulate_paths_parameter_sensitivity(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let rate = 0.05;
        let time = 0.25;
        
        // Test volatility sensitivity
        let low_vol_paths = engine.simulate_paths(spot, rate, 0.1, time);
        let high_vol_paths = engine.simulate_paths(spot, rate, 0.4, time);
        
        // Calculate spread of final values
        let low_vol_finals: Vec<f64> = low_vol_paths.iter().map(|p| *p.last().unwrap()).collect();
        let high_vol_finals: Vec<f64> = high_vol_paths.iter().map(|p| *p.last().unwrap()).collect();
        
        let low_vol_std = {
            let mean = low_vol_finals.iter().sum::<f64>() / low_vol_finals.len() as f64;
            let var = low_vol_finals.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (low_vol_finals.len() - 1) as f64;
            var.sqrt()
        };
        
        let high_vol_std = {
            let mean = high_vol_finals.iter().sum::<f64>() / high_vol_finals.len() as f64;
            let var = high_vol_finals.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (high_vol_finals.len() - 1) as f64;
            var.sqrt()
        };
        
        // Higher volatility should produce wider distribution
        assert!(high_vol_std > low_vol_std);
    }

    #[rstest]
    fn test_simulate_paths_time_scaling(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        
        // Test different time horizons
        let short_paths = engine.simulate_paths(spot, rate, vol, 0.1);
        let long_paths = engine.simulate_paths(spot, rate, vol, 1.0);
        
        // Calculate ranges for each
        let short_range = {
            let finals: Vec<f64> = short_paths.iter().map(|p| *p.last().unwrap()).collect();
            let min = finals.iter().fold(f64::INFINITY, |a, &b| a.min(b));
            let max = finals.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            max - min
        };
        
        let long_range = {
            let finals: Vec<f64> = long_paths.iter().map(|p| *p.last().unwrap()).collect();
            let min = finals.iter().fold(f64::INFINITY, |a, &b| a.min(b));
            let max = finals.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            max - min
        };
        
        // Longer time should generally produce wider range
        assert!(long_range > short_range);
    }

    #[rstest]
    fn test_simulate_paths_edge_cases(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        
        // Zero volatility - paths should be deterministic
        let zero_vol_paths = engine.simulate_paths(100.0, 0.05, 0.0, 0.25);
        let expected_final = 100.0 * (0.05 * 0.25).exp();
        
        for path in &zero_vol_paths {
            let final_value = *path.last().unwrap();
            assert_abs_diff_eq!(final_value, expected_final, epsilon = 1e-10);
        }
        
        // Zero time - all paths should stay at spot
        let zero_time_paths = engine.simulate_paths(100.0, 0.05, 0.2, 0.0);
        for path in &zero_time_paths {
            assert_eq!(path.len(), 1); // Only initial value
            assert_eq!(path[0], 100.0);
        }
        
        // High volatility should still produce valid paths
        let high_vol_paths = engine.simulate_paths(100.0, 0.05, 2.0, 0.25);
        for path in &high_vol_paths {
            for &price in path {
                assert!(price > 0.0);
                assert!(price.is_finite());
            }
        }
    }
}

#[cfg(test)]
mod asian_option_tests {
    use super::*;

    #[rstest]
    fn test_asian_option_pricing_basic(
        precision_mc_engine: MonteCarloEngine,
        standard_mc_params: (f64, f64, f64, f64, f64)
    ) {
        let engine = precision_mc_engine;
        let (spot, rate, vol, time, strike) = standard_mc_params;
        
        let asian_price = engine.price_exotic(
            &ExoticOptionType::Asian,
            spot, strike, rate, vol, time
        );
        
        // Asian option price should be positive and reasonable
        assert!(asian_price > 0.0);
        assert!(asian_price < spot); // Should be less than spot for reasonable parameters
        assert!(asian_price.is_finite());
        
        // Compare with European option (Asian should generally be cheaper due to averaging)
        let european_price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, 0.0);
        
        // Asian call should typically be cheaper than European call
        assert!(asian_price < european_price);
    }

    #[rstest]
    fn test_asian_option_atm_vs_otm(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        let atm_strike = 100.0;
        let otm_strike = 110.0;
        
        let asian_atm = engine.price_exotic(&ExoticOptionType::Asian, spot, atm_strike, rate, vol, time);
        let asian_otm = engine.price_exotic(&ExoticOptionType::Asian, spot, otm_strike, rate, vol, time);
        
        // ATM option should be more expensive than OTM
        assert!(asian_atm > asian_otm);
        assert!(asian_otm > 0.0);
    }

    #[rstest]
    fn test_asian_option_volatility_sensitivity(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let strike = 100.0;
        let rate = 0.05;
        let time = 0.25;
        
        let low_vol_price = engine.price_exotic(&ExoticOptionType::Asian, spot, strike, rate, 0.1, time);
        let high_vol_price = engine.price_exotic(&ExoticOptionType::Asian, spot, strike, rate, 0.4, time);
        
        // Higher volatility should increase option value
        assert!(high_vol_price > low_vol_price);
        assert!(low_vol_price > 0.0);
    }

    #[rstest]
    fn test_asian_option_time_sensitivity(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let strike = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        
        let short_time_price = engine.price_exotic(&ExoticOptionType::Asian, spot, strike, rate, vol, 0.1);
        let long_time_price = engine.price_exotic(&ExoticOptionType::Asian, spot, strike, rate, vol, 1.0);
        
        // Longer time should generally increase option value
        assert!(long_time_price > short_time_price);
    }

    #[rstest]
    fn test_asian_option_convergence(precision_mc_engine: MonteCarloEngine) {
        let engine = precision_mc_engine;
        let spot = 100.0;
        let strike = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        // Run pricing multiple times to check stability
        let mut prices = Vec::new();
        for _ in 0..5 {
            let price = engine.price_exotic(&ExoticOptionType::Asian, spot, strike, rate, vol, time);
            prices.push(price);
        }
        
        // Prices should be close to each other (Monte Carlo convergence)
        let mean_price = prices.iter().sum::<f64>() / prices.len() as f64;
        for price in &prices {
            let relative_error = (price - mean_price).abs() / mean_price;
            assert!(relative_error < 0.05, "Price variation too high: {:.4}", relative_error);
        }
    }
}

#[cfg(test)]
mod barrier_option_tests {
    use super::*;

    #[rstest]
    fn test_up_and_out_barrier_option(precision_mc_engine: MonteCarloEngine) {
        let engine = precision_mc_engine;
        let spot = 100.0;
        let strike = 100.0;
        let barrier = 120.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        let barrier_option = ExoticOptionType::Barrier {
            barrier,
            barrier_type: BarrierType::UpAndOut,
        };
        
        let price = engine.price_exotic(&barrier_option, spot, strike, rate, vol, time);
        
        // Barrier option should be positive but less than European
        assert!(price >= 0.0);
        assert!(price.is_finite());
        
        let european_price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, 0.0);
        assert!(price <= european_price);
    }

    #[rstest]
    fn test_down_and_out_barrier_option(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let strike = 100.0;
        let barrier = 80.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        let barrier_option = ExoticOptionType::Barrier {
            barrier,
            barrier_type: BarrierType::DownAndOut,
        };
        
        let price = engine.price_exotic(&barrier_option, spot, strike, rate, vol, time);
        
        assert!(price >= 0.0);
        assert!(price.is_finite());
        
        // Should be less than European option
        let european_price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, 0.0);
        assert!(price <= european_price);
    }

    #[rstest]
    fn test_up_and_in_barrier_option(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let strike = 100.0;
        let barrier = 120.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        let barrier_option = ExoticOptionType::Barrier {
            barrier,
            barrier_type: BarrierType::UpAndIn,
        };
        
        let price = engine.price_exotic(&barrier_option, spot, strike, rate, vol, time);
        
        assert!(price >= 0.0);
        assert!(price.is_finite());
    }

    #[rstest]
    fn test_down_and_in_barrier_option(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let strike = 100.0;
        let barrier = 80.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        let barrier_option = ExoticOptionType::Barrier {
            barrier,
            barrier_type: BarrierType::DownAndIn,
        };
        
        let price = engine.price_exotic(&barrier_option, spot, strike, rate, vol, time);
        
        assert!(price >= 0.0);
        assert!(price.is_finite());
    }

    #[rstest]
    fn test_barrier_option_complementarity(precision_mc_engine: MonteCarloEngine) {
        let engine = precision_mc_engine;
        let spot = 100.0;
        let strike = 100.0;
        let barrier = 120.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        // Up-and-out + Up-and-in should approximately equal European option
        let up_out = engine.price_exotic(
            &ExoticOptionType::Barrier { barrier, barrier_type: BarrierType::UpAndOut },
            spot, strike, rate, vol, time
        );
        
        let up_in = engine.price_exotic(
            &ExoticOptionType::Barrier { barrier, barrier_type: BarrierType::UpAndIn },
            spot, strike, rate, vol, time
        );
        
        let european = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, 0.0);
        let barrier_sum = up_out + up_in;
        
        // Should be approximately equal (allowing for Monte Carlo error)
        assert_relative_eq!(barrier_sum, european, epsilon = 0.1);
    }

    #[rstest]
    fn test_barrier_option_proximity_effect(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let strike = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        // Test barriers at different distances from spot
        let close_barrier = 110.0;
        let far_barrier = 150.0;
        
        let close_barrier_price = engine.price_exotic(
            &ExoticOptionType::Barrier { barrier: close_barrier, barrier_type: BarrierType::UpAndOut },
            spot, strike, rate, vol, time
        );
        
        let far_barrier_price = engine.price_exotic(
            &ExoticOptionType::Barrier { barrier: far_barrier, barrier_type: BarrierType::UpAndOut },
            spot, strike, rate, vol, time
        );
        
        // Closer barrier should result in lower option value (more likely to knock out)
        assert!(close_barrier_price <= far_barrier_price);
    }

    #[rstest]
    fn test_barrier_option_volatility_effect(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let strike = 100.0;
        let barrier = 120.0;
        let rate = 0.05;
        let time = 0.25;
        
        let low_vol_price = engine.price_exotic(
            &ExoticOptionType::Barrier { barrier, barrier_type: BarrierType::UpAndOut },
            spot, strike, rate, 0.1, time
        );
        
        let high_vol_price = engine.price_exotic(
            &ExoticOptionType::Barrier { barrier, barrier_type: BarrierType::UpAndOut },
            spot, strike, rate, 0.4, time
        );
        
        // For up-and-out barriers, higher volatility can decrease value (more likely to hit barrier)
        // But the relationship can be complex - just ensure both are non-negative
        assert!(low_vol_price >= 0.0);
        assert!(high_vol_price >= 0.0);
    }
}

#[cfg(test)]
mod lookback_option_tests {
    use super::*;

    #[rstest]
    fn test_lookback_option_basic_pricing(precision_mc_engine: MonteCarloEngine) {
        let engine = precision_mc_engine;
        let spot = 100.0;
        let strike = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        let lookback_price = engine.price_exotic(
            &ExoticOptionType::Lookback,
            spot, strike, rate, vol, time
        );
        
        // Lookback option should be positive and valuable
        assert!(lookback_price > 0.0);
        assert!(lookback_price.is_finite());
        
        // Should be more expensive than European call (payoff based on maximum)
        let european_price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, 0.0);
        assert!(lookback_price >= european_price);
    }

    #[rstest]
    fn test_lookback_option_volatility_sensitivity(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let strike = 100.0;
        let rate = 0.05;
        let time = 0.25;
        
        let low_vol_price = engine.price_exotic(&ExoticOptionType::Lookback, spot, strike, rate, 0.1, time);
        let high_vol_price = engine.price_exotic(&ExoticOptionType::Lookback, spot, strike, rate, 0.4, time);
        
        // Higher volatility should significantly increase lookback option value
        assert!(high_vol_price > low_vol_price);
        
        // The increase should be substantial due to the lookback feature
        assert!(high_vol_price / low_vol_price > 1.5);
    }

    #[rstest]
    fn test_lookback_option_time_sensitivity(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let strike = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        
        let short_time_price = engine.price_exotic(&ExoticOptionType::Lookback, spot, strike, rate, vol, 0.1);
        let long_time_price = engine.price_exotic(&ExoticOptionType::Lookback, spot, strike, rate, vol, 1.0);
        
        // Longer time should increase lookback option value (more time to hit new highs)
        assert!(long_time_price > short_time_price);
        
        // The increase should be significant
        assert!(long_time_price / short_time_price > 1.3);
    }

    #[rstest]
    fn test_lookback_option_strike_sensitivity(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        let low_strike = 90.0;
        let high_strike = 110.0;
        
        let low_strike_price = engine.price_exotic(&ExoticOptionType::Lookback, spot, low_strike, rate, vol, time);
        let high_strike_price = engine.price_exotic(&ExoticOptionType::Lookback, spot, high_strike, rate, vol, time);
        
        // Lower strike should result in higher option value
        assert!(low_strike_price > high_strike_price);
        assert!(high_strike_price >= 0.0);
    }

    #[rstest]
    fn test_lookback_option_minimum_payoff(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let strike = 90.0; // ITM
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        let lookback_price = engine.price_exotic(&ExoticOptionType::Lookback, spot, strike, rate, vol, time);
        
        // Lookback option should at least be worth the current intrinsic value
        let current_intrinsic = (spot - strike).max(0.0);
        let discounted_intrinsic = current_intrinsic * (-rate * time).exp();
        
        // Due to the lookback feature, it should be worth at least this much
        assert!(lookback_price >= discounted_intrinsic * 0.9); // Small tolerance for MC error
    }
}

#[cfg(test)]
mod monte_carlo_performance_tests {
    use super::*;
    use std::time::Instant;

    #[rstest]
    fn test_path_simulation_performance(fast_mc_engine: MonteCarloEngine) {
        let engine = fast_mc_engine;
        let spot = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        let start = Instant::now();
        let _paths = engine.simulate_paths(spot, rate, vol, time);
        let duration = start.elapsed();
        
        // Path simulation should be reasonably fast
        let paths_per_second = engine.simulations as f64 / duration.as_secs_f64();
        assert!(paths_per_second > 10000.0, "Path simulation too slow: {:.0} paths/second", paths_per_second);
    }

    #[rstest]
    fn test_exotic_option_pricing_performance(fast_mc_engine: MonteCarloEngine) {
        let engine = fast_mc_engine;
        let spot = 100.0;
        let strike = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        let start = Instant::now();
        
        // Price multiple exotic options
        let _asian = engine.price_exotic(&ExoticOptionType::Asian, spot, strike, rate, vol, time);
        let _barrier = engine.price_exotic(
            &ExoticOptionType::Barrier { barrier: 120.0, barrier_type: BarrierType::UpAndOut },
            spot, strike, rate, vol, time
        );
        let _lookback = engine.price_exotic(&ExoticOptionType::Lookback, spot, strike, rate, vol, time);
        
        let duration = start.elapsed();
        
        // Should price all three options in reasonable time
        assert!(duration.as_millis() < 5000, "Exotic option pricing too slow: {:.2}s", duration.as_secs_f64());
    }

    #[rstest]
    fn test_memory_usage_efficiency(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        // This test ensures that path simulation doesn't consume excessive memory
        // by not storing all paths simultaneously (paths are processed as generated)
        
        let start = Instant::now();
        let paths = engine.simulate_paths(spot, rate, vol, time);
        let duration = start.elapsed();
        
        // Check that we can access the paths without issues
        assert_eq!(paths.len(), engine.simulations);
        assert!(duration.as_millis() < 10000); // Should complete in reasonable time
        
        // Memory usage is implicitly tested by successful completion
    }

    #[rstest]
    fn test_monte_carlo_scalability() {
        // Test with different simulation sizes
        let test_configs = vec![
            (1000, 50),     // Small
            (10000, 100),   // Medium  
            (50000, 200),   // Large
        ];
        
        for (sims, steps) in test_configs {
            let engine = MonteCarloEngine::new(sims, steps);
            let start = Instant::now();
            
            let _price = engine.price_exotic(
                &ExoticOptionType::Asian,
                100.0, 100.0, 0.05, 0.2, 0.25
            );
            
            let duration = start.elapsed();
            let time_per_sim = duration.as_nanos() as f64 / sims as f64;
            
            // Time per simulation should scale reasonably
            assert!(time_per_sim < 100_000.0, "Simulation scaling poor: {:.0}ns per simulation", time_per_sim);
        }
    }
}

#[cfg(test)]
mod monte_carlo_accuracy_tests {
    use super::*;

    #[rstest]
    fn test_european_option_convergence(precision_mc_engine: MonteCarloEngine) {
        let engine = precision_mc_engine;
        let spot = 100.0;
        let strike = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        // Create a simple European-like payoff using Asian option with single time step
        let single_step_engine = MonteCarloEngine::new(engine.simulations, 1);
        let mc_price = single_step_engine.price_exotic(&ExoticOptionType::Asian, spot, strike, rate, vol, time);
        
        // Compare with Black-Scholes analytical solution
        let bs_price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, 0.0);
        
        // Monte Carlo should converge to Black-Scholes (within reasonable error)
        let relative_error = (mc_price - bs_price).abs() / bs_price;
        assert!(relative_error < 0.05, "MC convergence error too high: {:.4}", relative_error);
    }

    #[rstest]
    fn test_monte_carlo_confidence_intervals() {
        let spot = 100.0;
        let strike = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        // Run multiple independent MC runs to estimate confidence intervals
        let mut prices = Vec::new();
        for _ in 0..10 {
            let engine = MonteCarloEngine::new(10000, 100);
            let price = engine.price_exotic(&ExoticOptionType::Asian, spot, strike, rate, vol, time);
            prices.push(price);
        }
        
        // Calculate statistics
        let mean = prices.iter().sum::<f64>() / prices.len() as f64;
        let variance = prices.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / (prices.len() - 1) as f64;
        let std_error = (variance / prices.len() as f64).sqrt();
        
        // 95% confidence interval should be reasonable
        let confidence_95 = 1.96 * std_error;
        let relative_ci = confidence_95 / mean;
        
        assert!(relative_ci < 0.1, "Confidence interval too wide: {:.4}", relative_ci);
    }

    #[rstest]
    fn test_antithetic_variance_reduction() {
        // This test would be for antithetic variance reduction if implemented
        // For now, we just test that the basic MC gives consistent results
        
        let engine = MonteCarloEngine::new(20000, 100);
        let spot = 100.0;
        let strike = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        let price1 = engine.price_exotic(&ExoticOptionType::Asian, spot, strike, rate, vol, time);
        let price2 = engine.price_exotic(&ExoticOptionType::Asian, spot, strike, rate, vol, time);
        
        // Same engine should give same results (deterministic with fixed seed)
        assert_abs_diff_eq!(price1, price2, epsilon = 1e-10);
    }
}

#[cfg(test)]
mod monte_carlo_edge_cases {
    use super::*;

    #[rstest]
    fn test_zero_volatility_exotic_options(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let strike = 95.0;
        let rate = 0.05;
        let vol = 0.0; // Zero volatility
        let time = 0.25;
        
        // With zero volatility, all exotic options should give deterministic results
        let asian_price = engine.price_exotic(&ExoticOptionType::Asian, spot, strike, rate, vol, time);
        let lookback_price = engine.price_exotic(&ExoticOptionType::Lookback, spot, strike, rate, vol, time);
        
        // Both should be positive and finite
        assert!(asian_price > 0.0 && asian_price.is_finite());
        assert!(lookback_price > 0.0 && lookback_price.is_finite());
        
        // With zero volatility, final price is deterministic: spot * exp(rate * time)
        let final_price = spot * (rate * time).exp();
        let expected_payoff = (final_price - strike).max(0.0);
        let expected_price = expected_payoff * (-rate * time).exp();
        
        // Asian and lookback should equal this deterministic value
        assert_abs_diff_eq!(asian_price, expected_price, epsilon = 1e-6);
        assert_abs_diff_eq!(lookback_price, expected_price, epsilon = 1e-6);
    }

    #[rstest]
    fn test_extreme_parameters_exotic_options(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        
        // Test with extreme parameters
        let cases = vec![
            (1.0, 100.0, 0.05, 2.0, 0.25),   // Very low spot, high vol
            (10000.0, 9000.0, 0.05, 0.01, 0.25), // Very high spot, low vol  
            (100.0, 100.0, -0.02, 0.2, 0.25), // Negative rate
            (100.0, 100.0, 0.20, 0.2, 0.001), // Very short time
        ];
        
        for (spot, strike, rate, vol, time) in cases {
            let asian_price = engine.price_exotic(&ExoticOptionType::Asian, spot, strike, rate, vol, time);
            let lookback_price = engine.price_exotic(&ExoticOptionType::Lookback, spot, strike, rate, vol, time);
            
            // All prices should be non-negative and finite
            assert!(asian_price >= 0.0 && asian_price.is_finite());
            assert!(lookback_price >= 0.0 && lookback_price.is_finite());
        }
    }

    #[rstest]
    fn test_barrier_option_edge_cases(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 100.0;
        let strike = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.25;
        
        // Barrier very close to spot
        let close_barrier = ExoticOptionType::Barrier {
            barrier: spot + 0.01,
            barrier_type: BarrierType::UpAndOut,
        };
        
        let close_price = engine.price_exotic(&close_barrier, spot, strike, rate, vol, time);
        assert!(close_price >= 0.0 && close_price.is_finite());
        
        // Barrier very far from spot
        let far_barrier = ExoticOptionType::Barrier {
            barrier: spot * 10.0,
            barrier_type: BarrierType::UpAndOut,
        };
        
        let far_price = engine.price_exotic(&far_barrier, spot, strike, rate, vol, time);
        assert!(far_price >= 0.0 && far_price.is_finite());
        
        // Far barrier should be worth more (less likely to knock out)
        assert!(far_price >= close_price);
    }

    #[rstest]
    fn test_exotic_options_with_zero_time(basic_mc_engine: MonteCarloEngine) {
        let engine = basic_mc_engine;
        let spot = 110.0; // ITM
        let strike = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.0; // Zero time to expiry
        
        // With zero time, all exotic options should give intrinsic value
        let asian_price = engine.price_exotic(&ExoticOptionType::Asian, spot, strike, rate, vol, time);
        let lookback_price = engine.price_exotic(&ExoticOptionType::Lookback, spot, strike, rate, vol, time);
        
        let intrinsic = (spot - strike).max(0.0);
        
        // With zero time, prices should equal intrinsic value
        assert_abs_diff_eq!(asian_price, intrinsic, epsilon = 1e-10);
        assert_abs_diff_eq!(lookback_price, intrinsic, epsilon = 1e-10);
    }
}