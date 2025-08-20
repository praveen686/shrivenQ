/// Test configuration and utilities for options-engine tests
/// 
/// This module provides shared test configuration, fixtures, and utilities
/// that can be used across all test modules.

use options_engine::*;
use chrono::{DateTime, Utc, Duration};
use std::sync::Once;

static INIT: Once = Once::new();

/// Initialize test environment
pub fn init_test_env() {
    INIT.call_once(|| {
        // Initialize logging for tests
        tracing_subscriber::fmt()
            .with_env_filter("options_engine=debug")
            .with_test_writer()
            .try_init()
            .ok(); // Ignore error if already initialized
    });
}

/// Test configuration constants
pub mod config {
    /// Default test parameters for Indian markets
    pub const NIFTY_SPOT: f64 = 21500.0;
    pub const BANK_NIFTY_SPOT: f64 = 48000.0;
    pub const FIN_NIFTY_SPOT: f64 = 20000.0;
    pub const MIDCAP_NIFTY_SPOT: f64 = 10000.0;
    
    /// Standard Indian risk-free rate
    pub const INDIAN_RISK_FREE_RATE: f64 = 0.065;
    
    /// Typical volatility ranges for Indian markets
    pub const LOW_VOLATILITY: f64 = 0.10;
    pub const MEDIUM_VOLATILITY: f64 = 0.15;
    pub const HIGH_VOLATILITY: f64 = 0.25;
    
    /// Standard time periods in years
    pub const ONE_DAY: f64 = 1.0 / 365.0;
    pub const ONE_WEEK: f64 = 7.0 / 365.0;
    pub const ONE_MONTH: f64 = 30.0 / 365.0;
    pub const THREE_MONTHS: f64 = 90.0 / 365.0;
    pub const ONE_YEAR: f64 = 1.0;
    
    /// Test tolerances for different precision levels
    pub const HIGH_PRECISION_EPSILON: f64 = 1e-12;
    pub const STANDARD_EPSILON: f64 = 1e-8;
    pub const MONTE_CARLO_EPSILON: f64 = 1e-2;
    pub const LOOSE_EPSILON: f64 = 1e-4;
    
    /// Monte Carlo simulation sizes
    pub const FAST_MC_SAMPLES: usize = 1000;
    pub const STANDARD_MC_SAMPLES: usize = 10000;
    pub const PRECISION_MC_SAMPLES: usize = 100000;
    pub const ULTRA_PRECISION_MC_SAMPLES: usize = 1000000;
    
    /// Time steps for path simulation
    pub const COARSE_TIME_STEPS: usize = 50;
    pub const STANDARD_TIME_STEPS: usize = 252; // Trading days
    pub const FINE_TIME_STEPS: usize = 1000;
}

/// Common test data generators
pub mod generators {
    use super::*;
    
    /// Generate standard test parameters for option pricing
    pub fn standard_option_params() -> Vec<(f64, f64, f64, f64, f64, f64)> {
        vec![
            // (spot, strike, rate, volatility, time, dividend)
            (100.0, 100.0, 0.05, 0.2, 0.25, 0.0),          // Classic ATM
            (config::NIFTY_SPOT, config::NIFTY_SPOT, config::INDIAN_RISK_FREE_RATE, config::MEDIUM_VOLATILITY, config::ONE_MONTH, 0.0), // Nifty ATM
            (config::BANK_NIFTY_SPOT, config::BANK_NIFTY_SPOT * 0.98, config::INDIAN_RISK_FREE_RATE, config::HIGH_VOLATILITY, config::ONE_WEEK, 0.0), // Bank Nifty ITM
            (config::NIFTY_SPOT, config::NIFTY_SPOT * 1.05, config::INDIAN_RISK_FREE_RATE, config::LOW_VOLATILITY, config::THREE_MONTHS, 0.0), // Nifty OTM
        ]
    }
    
    /// Generate test parameters for Greeks validation
    pub fn greeks_test_params() -> Vec<(f64, f64, f64, f64, f64, f64)> {
        vec![
            // ATM scenarios with different times
            (config::NIFTY_SPOT, config::NIFTY_SPOT, config::INDIAN_RISK_FREE_RATE, config::MEDIUM_VOLATILITY, config::ONE_DAY, 0.0),
            (config::NIFTY_SPOT, config::NIFTY_SPOT, config::INDIAN_RISK_FREE_RATE, config::MEDIUM_VOLATILITY, config::ONE_WEEK, 0.0),
            (config::NIFTY_SPOT, config::NIFTY_SPOT, config::INDIAN_RISK_FREE_RATE, config::MEDIUM_VOLATILITY, config::ONE_MONTH, 0.0),
            (config::NIFTY_SPOT, config::NIFTY_SPOT, config::INDIAN_RISK_FREE_RATE, config::MEDIUM_VOLATILITY, config::THREE_MONTHS, 0.0),
            
            // Different moneyness
            (config::NIFTY_SPOT, config::NIFTY_SPOT * 0.95, config::INDIAN_RISK_FREE_RATE, config::MEDIUM_VOLATILITY, config::ONE_MONTH, 0.0), // ITM
            (config::NIFTY_SPOT, config::NIFTY_SPOT * 1.05, config::INDIAN_RISK_FREE_RATE, config::MEDIUM_VOLATILITY, config::ONE_MONTH, 0.0), // OTM
            
            // Different volatilities
            (config::NIFTY_SPOT, config::NIFTY_SPOT, config::INDIAN_RISK_FREE_RATE, config::LOW_VOLATILITY, config::ONE_MONTH, 0.0),
            (config::NIFTY_SPOT, config::NIFTY_SPOT, config::INDIAN_RISK_FREE_RATE, config::HIGH_VOLATILITY, config::ONE_MONTH, 0.0),
        ]
    }
    
    /// Generate extreme test cases for stress testing
    pub fn extreme_test_params() -> Vec<(f64, f64, f64, f64, f64, f64, &'static str)> {
        vec![
            // (spot, strike, rate, volatility, time, dividend, description)
            (config::NIFTY_SPOT, config::NIFTY_SPOT, config::INDIAN_RISK_FREE_RATE, 0.0001, config::ONE_MONTH, 0.0, "Very low volatility"),
            (config::NIFTY_SPOT, config::NIFTY_SPOT, config::INDIAN_RISK_FREE_RATE, 3.0, config::ONE_MONTH, 0.0, "Very high volatility"),
            (config::NIFTY_SPOT, config::NIFTY_SPOT, config::INDIAN_RISK_FREE_RATE, config::MEDIUM_VOLATILITY, 0.0001, 0.0, "Very short time"),
            (config::NIFTY_SPOT, config::NIFTY_SPOT, config::INDIAN_RISK_FREE_RATE, config::MEDIUM_VOLATILITY, 10.0, 0.0, "Very long time"),
            (config::NIFTY_SPOT, config::NIFTY_SPOT / 2.0, config::INDIAN_RISK_FREE_RATE, config::MEDIUM_VOLATILITY, config::ONE_MONTH, 0.0, "Deep ITM"),
            (config::NIFTY_SPOT, config::NIFTY_SPOT * 2.0, config::INDIAN_RISK_FREE_RATE, config::MEDIUM_VOLATILITY, config::ONE_MONTH, 0.0, "Deep OTM"),
            (config::NIFTY_SPOT, config::NIFTY_SPOT, -0.02, config::MEDIUM_VOLATILITY, config::ONE_MONTH, 0.0, "Negative interest rate"),
            (config::NIFTY_SPOT, config::NIFTY_SPOT, 0.25, config::MEDIUM_VOLATILITY, config::ONE_MONTH, 0.0, "Very high interest rate"),
        ]
    }
    
    /// Generate test strikes around a given spot price
    pub fn generate_strike_chain(spot: f64, num_strikes: usize, strike_spacing: f64) -> Vec<f64> {
        let start_strike = spot - (num_strikes as f64 / 2.0) * strike_spacing;
        (0..num_strikes)
            .map(|i| start_strike + i as f64 * strike_spacing)
            .filter(|&strike| strike > 0.0) // Ensure positive strikes
            .collect()
    }
    
    /// Generate test expiry dates
    pub fn generate_expiry_dates(start_date: DateTime<Utc>, num_expiries: usize) -> Vec<DateTime<Utc>> {
        (1..=num_expiries)
            .map(|i| start_date + Duration::days(i as i64 * 7)) // Weekly expiries
            .collect()
    }
}

/// Test assertion helpers
pub mod assertions {
    use super::*;
    use approx::{assert_abs_diff_eq, assert_relative_eq};
    
    /// Assert that a price is reasonable for options
    pub fn assert_reasonable_option_price(price: f64, spot: f64, description: &str) {
        assert!(price >= 0.0, "Price should be non-negative for {}", description);
        assert!(price.is_finite(), "Price should be finite for {}", description);
        assert!(price <= spot * 2.0, "Price should not exceed 2x spot for {}", description);
    }
    
    /// Assert that Greeks are within reasonable bounds
    pub fn assert_reasonable_greeks(greeks: &Greeks, option_type: OptionType, description: &str) {
        // Delta bounds
        match option_type {
            OptionType::Call => {
                assert!(greeks.delta >= 0.0 && greeks.delta <= 1.0, "Call delta out of bounds for {}", description);
            }
            OptionType::Put => {
                assert!(greeks.delta >= -1.0 && greeks.delta <= 0.0, "Put delta out of bounds for {}", description);
            }
        }
        
        // Gamma should be non-negative
        assert!(greeks.gamma >= 0.0, "Gamma should be non-negative for {}", description);
        
        // Vega should be non-negative
        assert!(greeks.vega >= 0.0, "Vega should be non-negative for {}", description);
        
        // All Greeks should be finite
        assert!(greeks.delta.is_finite(), "Delta should be finite for {}", description);
        assert!(greeks.gamma.is_finite(), "Gamma should be finite for {}", description);
        assert!(greeks.theta.is_finite(), "Theta should be finite for {}", description);
        assert!(greeks.vega.is_finite(), "Vega should be finite for {}", description);
        assert!(greeks.rho.is_finite(), "Rho should be finite for {}", description);
    }
    
    /// Assert put-call parity within tolerance
    pub fn assert_put_call_parity(
        call_price: f64,
        put_price: f64,
        spot: f64,
        strike: f64,
        rate: f64,
        time: f64,
        epsilon: f64,
        description: &str
    ) {
        let parity_left = call_price - put_price;
        let parity_right = spot - strike * (-rate * time).exp();
        assert_abs_diff_eq!(parity_left, parity_right, epsilon = epsilon);
    }
    
    /// Assert that Monte Carlo convergence is reasonable
    pub fn assert_monte_carlo_convergence(
        mc_price: f64,
        analytical_price: f64,
        samples: usize,
        confidence_level: f64,
        description: &str
    ) {
        let relative_error = (mc_price - analytical_price).abs() / analytical_price;
        let expected_error = confidence_level / (samples as f64).sqrt();
        
        assert!(relative_error <= expected_error,
            "Monte Carlo convergence too slow for {}: {:.6} > {:.6} with {} samples",
            description, relative_error, expected_error, samples);
    }
    
    /// Assert numerical stability (no NaN or infinite values)
    pub fn assert_numerical_stability(values: &[f64], description: &str) {
        for (i, &value) in values.iter().enumerate() {
            assert!(value.is_finite(), "Value {} is not finite at index {} for {}", value, i, description);
        }
    }
}

/// Mock data generators for testing
pub mod mocks {
    use super::*;
    
    /// Create a mock Greeks structure with realistic values
    pub fn create_mock_greeks(delta: f64, gamma: f64, theta: f64, vega: f64, rho: f64) -> Greeks {
        Greeks {
            delta,
            gamma,
            theta,
            vega,
            rho,
            lambda: delta * 100.0 / 50.0, // Simplified lambda calculation
            vanna: gamma * vega * 0.01,
            charm: theta * delta * 0.01,
            vomma: vega * vega * 0.001,
            speed: gamma * gamma * 0.01,
            zomma: gamma * vega * 0.01,
            color: gamma * theta * 0.01,
        }
    }
    
    /// Create a mock option contract
    pub fn create_mock_option_contract(
        index: IndexOption,
        option_type: OptionType,
        strike: f64,
        expiry: DateTime<Utc>,
        premium: f64,
        greeks: Greeks
    ) -> OptionContract {
        let lot_size = index.lot_size();
        OptionContract {
            index,
            option_type,
            strike,
            expiry,
            lot_size,
            premium,
            open_interest: 10000,
            volume: 1000,
            implied_volatility: config::MEDIUM_VOLATILITY,
            greeks,
        }
    }
}

/// Performance testing utilities
pub mod performance {
    use std::time::{Duration, Instant};
    
    /// Measure the execution time of a function
    pub fn measure_time<F, R>(f: F) -> (R, Duration)
    where
        F: FnOnce() -> R,
    {
        let start = Instant::now();
        let result = f();
        let duration = start.elapsed();
        (result, duration)
    }
    
    /// Assert that a function executes within a time limit
    pub fn assert_performance<F, R>(f: F, max_duration: Duration, description: &str) -> R
    where
        F: FnOnce() -> R,
    {
        let (result, duration) = measure_time(f);
        assert!(duration <= max_duration,
            "Performance test failed for {}: took {:?}, expected <= {:?}",
            description, duration, max_duration);
        result
    }
    
    /// Run a performance test with multiple iterations
    pub fn benchmark_function<F>(mut f: F, iterations: usize, description: &str) -> Duration
    where
        F: FnMut(),
    {
        let start = Instant::now();
        for _ in 0..iterations {
            f();
        }
        let total_duration = start.elapsed();
        let avg_duration = total_duration / iterations as u32;
        
        println!("Benchmark for {}: {} iterations in {:?} (avg: {:?} per iteration)",
                description, iterations, total_duration, avg_duration);
        
        avg_duration
    }
}

/// Integration test helpers
pub mod integration {
    use super::*;
    
    /// Setup a test options engine
    pub fn setup_test_options_engine() -> OptionsEngine {
        OptionsEngine::new(ExecutionMode::Paper)
    }
    
    /// Create a test volatility surface with realistic data
    pub fn create_test_volatility_surface() -> VolatilitySurface {
        let mut surface = VolatilitySurface::new();
        surface.atm_volatility = config::MEDIUM_VOLATILITY;
        surface.skew = -0.05; // Typical negative skew
        surface.term_structure = vec![0.12, 0.15, 0.18, 0.16, 0.14];
        surface
    }
    
    /// Run a complete integration test scenario
    pub async fn run_integration_scenario(
        engine: &OptionsEngine,
        scenario_name: &str,
    ) -> Result<(), anyhow::Error> {
        tracing::info!("Running integration scenario: {}", scenario_name);
        
        // Create a test strategy
        let expiry = Utc::now() + Duration::days(30);
        let strategy = OptionStrategy::iron_condor(
            IndexOption::Nifty50,
            config::NIFTY_SPOT,
            expiry,
            100.0, // wing width
            200.0, // body width
        );
        
        // Execute the strategy
        engine.execute_strategy(strategy).await?;
        
        // Update risk metrics
        engine.update_risk_metrics().await;
        
        tracing::info!("Integration scenario completed: {}", scenario_name);
        Ok(())
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;
    
    #[test]
    fn test_config_constants() {
        // Test that config constants are reasonable
        assert!(config::NIFTY_SPOT > 10000.0);
        assert!(config::NIFTY_SPOT < 50000.0);
        
        assert!(config::INDIAN_RISK_FREE_RATE > 0.0);
        assert!(config::INDIAN_RISK_FREE_RATE < 0.15);
        
        assert!(config::LOW_VOLATILITY < config::MEDIUM_VOLATILITY);
        assert!(config::MEDIUM_VOLATILITY < config::HIGH_VOLATILITY);
        
        assert!(config::ONE_DAY < config::ONE_WEEK);
        assert!(config::ONE_WEEK < config::ONE_MONTH);
        assert!(config::ONE_MONTH < config::THREE_MONTHS);
        assert!(config::THREE_MONTHS < config::ONE_YEAR);
    }
    
    #[test]
    fn test_generators() {
        init_test_env();
        
        let params = generators::standard_option_params();
        assert!(!params.is_empty());
        
        for (spot, strike, rate, vol, time, dividend) in params {
            assert!(spot > 0.0);
            assert!(strike > 0.0);
            assert!(rate >= -0.1 && rate <= 1.0);
            assert!(vol > 0.0 && vol <= 10.0);
            assert!(time > 0.0);
            assert!(dividend >= 0.0);
        }
        
        let strikes = generators::generate_strike_chain(config::NIFTY_SPOT, 10, 100.0);
        assert_eq!(strikes.len(), 10);
        
        let expiries = generators::generate_expiry_dates(Utc::now(), 5);
        assert_eq!(expiries.len(), 5);
    }
    
    #[test]
    fn test_mock_data() {
        let greeks = mocks::create_mock_greeks(0.5, 0.002, -5.0, 20.0, 10.0);
        assertions::assert_reasonable_greeks(&greeks, OptionType::Call, "mock Greeks");
        
        let contract = mocks::create_mock_option_contract(
            IndexOption::Nifty50,
            OptionType::Call,
            config::NIFTY_SPOT,
            Utc::now() + Duration::days(30),
            100.0,
            greeks
        );
        
        assert_eq!(contract.index, IndexOption::Nifty50);
        assert_eq!(contract.option_type, OptionType::Call);
        assert_eq!(contract.strike, config::NIFTY_SPOT);
        assert_eq!(contract.premium, 100.0);
    }
}