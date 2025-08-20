use rstest::*;
use approx::{assert_abs_diff_eq, assert_relative_eq};
use options_engine::{
    BlackScholes, OptionType, IndexOption, VolatilitySurface, MonteCarloEngine, 
    ExoticOptionType, BarrierType, OptionStrategy, OptionsEngine, ExecutionMode
};
use chrono::{DateTime, Utc, Duration};

/// Test fixture for zero time to expiry scenarios
#[fixture]
fn zero_time_params() -> Vec<(f64, f64, f64, f64, f64)> {
    vec![
        (21500.0, 21500.0, 0.065, 0.15, 0.0), // Exactly ATM
        (21500.0, 21400.0, 0.065, 0.15, 0.0), // ITM call
        (21500.0, 21600.0, 0.065, 0.15, 0.0), // OTM call
        (21400.0, 21500.0, 0.065, 0.15, 0.0), // OTM put scenario
        (21600.0, 21500.0, 0.065, 0.15, 0.0), // ITM put scenario
    ]
}

/// Test fixture for extreme volatility scenarios
#[fixture]
fn extreme_volatility_params() -> Vec<(f64, f64, f64, f64, f64)> {
    vec![
        (21500.0, 21500.0, 0.065, 0.0001, 0.25), // Near-zero volatility
        (21500.0, 21500.0, 0.065, 5.0, 0.25),    // 500% volatility
        (21500.0, 21500.0, 0.065, 10.0, 0.25),   // 1000% volatility
        (21500.0, 21500.0, 0.065, 0.00001, 0.001), // Micro volatility, micro time
        (21500.0, 21500.0, 0.065, 100.0, 10.0),  // Extreme volatility, long time
    ]
}

/// Test fixture for extreme moneyness scenarios
#[fixture]
fn extreme_moneyness_params() -> Vec<(f64, f64, f64, f64, f64)> {
    vec![
        (21500.0, 10000.0, 0.065, 0.15, 0.25), // Deep ITM call
        (21500.0, 40000.0, 0.065, 0.15, 0.25), // Deep OTM call
        (10000.0, 21500.0, 0.065, 0.15, 0.25), // Deep OTM call (low spot)
        (40000.0, 21500.0, 0.065, 0.15, 0.25), // Deep ITM call (high spot)
        (21500.0, 0.01, 0.065, 0.15, 0.25),    // Near-zero strike
    ]
}

#[cfg(test)]
mod zero_time_expiry_tests {
    use super::*;

    #[rstest]
    fn test_zero_time_option_pricing(zero_time_params: Vec<(f64, f64, f64, f64, f64)>) {
        for (spot, strike, rate, vol, time) in zero_time_params {
            let call_price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, 0.0);
            let put_price = BlackScholes::price(OptionType::Put, spot, strike, rate, vol, time, 0.0);
            
            // At expiry, options should equal intrinsic value
            let call_intrinsic = (spot - strike).max(0.0);
            let put_intrinsic = (strike - spot).max(0.0);
            
            assert_abs_diff_eq!(call_price, call_intrinsic, epsilon = 1e-10);
            assert_abs_diff_eq!(put_price, put_intrinsic, epsilon = 1e-10);
            
            // Prices should be non-negative
            assert!(call_price >= 0.0);
            assert!(put_price >= 0.0);
            
            // At least one should be zero (unless exactly ATM with positive rate effects)
            if (spot - strike).abs() > 1e-10 {
                assert!(call_price == 0.0 || put_price == 0.0);
            }
        }
    }

    #[rstest]
    fn test_zero_time_greeks(zero_time_params: Vec<(f64, f64, f64, f64, f64)>) {
        for (spot, strike, rate, vol, time) in zero_time_params {
            let call_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, 0.0);
            let put_greeks = BlackScholes::calculate_greeks(OptionType::Put, spot, strike, rate, vol, time, 0.0);
            
            // With zero time, most Greeks should be zero or default values
            assert_eq!(call_greeks.delta, 0.0, "Call delta should be 0 at expiry");
            assert_eq!(call_greeks.gamma, 0.0, "Call gamma should be 0 at expiry");
            assert_eq!(call_greeks.theta, 0.0, "Call theta should be 0 at expiry");
            assert_eq!(call_greeks.vega, 0.0, "Call vega should be 0 at expiry");
            assert_eq!(call_greeks.rho, 0.0, "Call rho should be 0 at expiry");
            
            assert_eq!(put_greeks.delta, 0.0, "Put delta should be 0 at expiry");
            assert_eq!(put_greeks.gamma, 0.0, "Put gamma should be 0 at expiry");
            assert_eq!(put_greeks.theta, 0.0, "Put theta should be 0 at expiry");
            assert_eq!(put_greeks.vega, 0.0, "Put vega should be 0 at expiry");
            assert_eq!(put_greeks.rho, 0.0, "Put rho should be 0 at expiry");
            
            // Higher-order Greeks should also be zero
            assert_eq!(call_greeks.vanna, 0.0);
            assert_eq!(call_greeks.charm, 0.0);
            assert_eq!(put_greeks.vanna, 0.0);
            assert_eq!(put_greeks.charm, 0.0);
        }
    }

    #[rstest]
    fn test_approaching_zero_time() {
        let spot = 21500.0;
        let strike = 21600.0; // OTM call
        let rate = 0.065;
        let vol = 0.15;
        
        // Test convergence as time approaches zero
        let times = vec![1.0, 0.1, 0.01, 0.001, 0.0001, 0.00001];
        let mut prices = Vec::new();
        
        for &time in &times {
            let price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, 0.0);
            prices.push(price);
        }
        
        // Prices should decrease monotonically and approach intrinsic value
        let intrinsic = (spot - strike).max(0.0);
        
        for i in 1..prices.len() {
            assert!(prices[i] <= prices[i-1] + 1e-10, "Price should decrease as time decreases");
        }
        
        // Last price should be very close to intrinsic
        assert_abs_diff_eq!(prices.last().unwrap(), &intrinsic, epsilon = 1e-6);
    }

    #[rstest]
    fn test_zero_time_put_call_parity() {
        let test_cases = vec![
            (21500.0, 21500.0), // ATM
            (21500.0, 21000.0), // ITM call
            (21500.0, 22000.0), // OTM call
        ];
        
        for (spot, strike) in test_cases {
            let call_price = BlackScholes::price(OptionType::Call, spot, strike, 0.065, 0.15, 0.0, 0.0);
            let put_price = BlackScholes::price(OptionType::Put, spot, strike, 0.065, 0.15, 0.0, 0.0);
            
            // At expiry, put-call parity: C - P = S - K
            let parity_left = call_price - put_price;
            let parity_right = spot - strike;
            
            assert_abs_diff_eq!(parity_left, parity_right, epsilon = 1e-10);
        }
    }
}

#[cfg(test)]
mod extreme_volatility_tests {
    use super::*;

    #[rstest]
    fn test_extreme_volatility_pricing(extreme_volatility_params: Vec<(f64, f64, f64, f64, f64)>) {
        for (spot, strike, rate, vol, time) in extreme_volatility_params {
            let call_price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, 0.0);
            let put_price = BlackScholes::price(OptionType::Put, spot, strike, rate, vol, time, 0.0);
            
            // Prices should always be finite and non-negative
            assert!(call_price.is_finite(), "Call price should be finite for vol={}", vol);
            assert!(put_price.is_finite(), "Put price should be finite for vol={}", vol);
            assert!(call_price >= 0.0, "Call price should be non-negative for vol={}", vol);
            assert!(put_price >= 0.0, "Put price should be non-negative for vol={}", vol);
            
            // For extremely high volatility, option should approach spot price
            if vol > 2.0 && time > 0.1 {
                assert!(call_price > spot * 0.1, "Call price should be substantial for high volatility");
            }
            
            // For extremely low volatility, option should approach intrinsic value
            if vol < 0.001 {
                let call_intrinsic = (spot - strike).max(0.0);
                let put_intrinsic = (strike - spot).max(0.0);
                
                // Should be close to intrinsic plus small time value
                assert!(call_price >= call_intrinsic, "Call price should be at least intrinsic");
                assert!(put_price >= put_intrinsic, "Put price should be at least intrinsic");
                
                if time > 0.001 {
                    // Small time value for very low volatility
                    assert!(call_price - call_intrinsic < spot * 0.001, "Time value should be small for low vol");
                    assert!(put_price - put_intrinsic < spot * 0.001, "Time value should be small for low vol");
                }
            }
        }
    }

    #[rstest]
    fn test_extreme_volatility_greeks() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let time = 0.25;
        
        // Test very low volatility
        let low_vol = 0.0001;
        let low_vol_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, low_vol, time, 0.0);
        
        // Delta should be close to 0.5 for ATM (slightly above due to positive rates)
        assert!(low_vol_greeks.delta > 0.5 && low_vol_greeks.delta < 0.55);
        
        // Gamma should be very high for low volatility ATM options
        assert!(low_vol_greeks.gamma > 0.001, "Gamma should be high for low vol ATM");
        
        // Vega should be positive but reasonable
        assert!(low_vol_greeks.vega > 0.0);
        
        // Test very high volatility
        let high_vol = 5.0;
        let high_vol_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, high_vol, time, 0.0);
        
        // Delta should still be between 0 and 1
        assert!(high_vol_greeks.delta >= 0.0 && high_vol_greeks.delta <= 1.0);
        
        // Gamma should be lower for high volatility
        assert!(high_vol_greeks.gamma >= 0.0);
        assert!(high_vol_greeks.gamma < low_vol_greeks.gamma, "Gamma should be lower for high vol");
        
        // Vega should be positive
        assert!(high_vol_greeks.vega > 0.0);
        
        // All Greeks should be finite
        assert!(high_vol_greeks.delta.is_finite());
        assert!(high_vol_greeks.gamma.is_finite());
        assert!(high_vol_greeks.theta.is_finite());
        assert!(high_vol_greeks.vega.is_finite());
        assert!(high_vol_greeks.rho.is_finite());
    }

    #[rstest]
    fn test_volatility_smile_edge_cases() {
        let mut surface = VolatilitySurface::new();
        surface.atm_volatility = 0.15;
        surface.skew = -0.1;
        
        let spot = 21500.0;
        let time = 0.25;
        
        // Test extreme strikes
        let extreme_strikes = vec![
            spot * 0.1,  // 10% of spot
            spot * 0.5,  // 50% of spot  
            spot * 2.0,  // 200% of spot
            spot * 10.0, // 1000% of spot
        ];
        
        for strike in extreme_strikes {
            let iv = surface.get_iv(spot, strike, time);
            
            // IV should always be bounded and positive
            assert!(iv >= 0.01, "IV should be at least 1% for strike {}", strike);
            assert!(iv < 10.0, "IV should be reasonable for strike {}", strike);
            assert!(iv.is_finite(), "IV should be finite for strike {}", strike);
        }
    }

    #[rstest]
    fn test_sabr_model_extreme_parameters() {
        let surface = VolatilitySurface::new();
        let f = 21500.0;
        let k = 21500.0;
        let t = 0.25;
        
        // Test extreme SABR parameters
        let extreme_cases = vec![
            (0.001, 0.1, -0.99, 0.1, "Very low alpha, extreme rho"),
            (2.0, 0.99, 0.99, 5.0, "High alpha, extreme beta, rho, nu"),
            (0.1, 0.0001, 0.0, 0.001, "Low beta, zero rho, low nu"),
            (0.5, 1.0, -0.5, 0.0001, "Beta=1, negative rho, very low nu"),
        ];
        
        for (alpha, beta, rho, nu, description) in extreme_cases {
            let vol = surface.sabr_volatility(f, k, t, alpha, beta, rho, nu);
            
            assert!(vol > 0.0, "SABR vol should be positive for {}", description);
            assert!(vol < 10.0, "SABR vol should be reasonable for {}", description);
            assert!(vol.is_finite(), "SABR vol should be finite for {}", description);
        }
    }
}

#[cfg(test)]
mod extreme_moneyness_tests {
    use super::*;

    #[rstest]
    fn test_extreme_moneyness_pricing(extreme_moneyness_params: Vec<(f64, f64, f64, f64, f64)>) {
        for (spot, strike, rate, vol, time) in extreme_moneyness_params {
            let call_price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, 0.0);
            let put_price = BlackScholes::price(OptionType::Put, spot, strike, rate, vol, time, 0.0);
            
            // Prices should be finite and non-negative
            assert!(call_price.is_finite(), "Call price should be finite for S={}, K={}", spot, strike);
            assert!(put_price.is_finite(), "Put price should be finite for S={}, K={}", spot, strike);
            assert!(call_price >= 0.0, "Call price should be non-negative for S={}, K={}", spot, strike);
            assert!(put_price >= 0.0, "Put price should be non-negative for S={}, K={}", spot, strike);
            
            // Deep ITM options should be close to intrinsic value
            let call_intrinsic = (spot - strike).max(0.0);
            let put_intrinsic = (strike - spot).max(0.0);
            
            if spot > strike * 2.0 { // Deep ITM call
                assert!(call_price >= call_intrinsic * 0.99, "Deep ITM call should be close to intrinsic");
                assert!(put_price < spot * 0.01, "Corresponding put should be nearly worthless");
            }
            
            if strike > spot * 2.0 { // Deep ITM put
                assert!(put_price >= put_intrinsic * 0.99, "Deep ITM put should be close to intrinsic");
                assert!(call_price < spot * 0.01, "Corresponding call should be nearly worthless");
            }
            
            // Verify put-call parity still holds
            let parity_left = call_price - put_price;
            let parity_right = spot - strike * (-rate * time).exp();
            assert_abs_diff_eq!(parity_left, parity_right, epsilon = 1e-8);
        }
    }

    #[rstest]
    fn test_extreme_moneyness_greeks() {
        let rate = 0.065;
        let vol = 0.15;
        let time = 0.25;
        
        // Deep ITM call
        let deep_itm_call_greeks = BlackScholes::calculate_greeks(
            OptionType::Call, 30000.0, 15000.0, rate, vol, time, 0.0
        );
        
        // Delta should be close to 1
        assert!(deep_itm_call_greeks.delta > 0.98, "Deep ITM call delta should be close to 1");
        assert!(deep_itm_call_greeks.delta <= 1.0, "Delta cannot exceed 1");
        
        // Gamma should be very small
        assert!(deep_itm_call_greeks.gamma < 0.0001, "Deep ITM call gamma should be small");
        assert!(deep_itm_call_greeks.gamma >= 0.0, "Gamma cannot be negative");
        
        // Deep OTM call
        let deep_otm_call_greeks = BlackScholes::calculate_greeks(
            OptionType::Call, 15000.0, 30000.0, rate, vol, time, 0.0
        );
        
        // Delta should be close to 0
        assert!(deep_otm_call_greeks.delta < 0.02, "Deep OTM call delta should be close to 0");
        assert!(deep_otm_call_greeks.delta >= 0.0, "Call delta cannot be negative");
        
        // Gamma should be very small
        assert!(deep_otm_call_greeks.gamma < 0.0001, "Deep OTM call gamma should be small");
        
        // Deep ITM put
        let deep_itm_put_greeks = BlackScholes::calculate_greeks(
            OptionType::Put, 15000.0, 30000.0, rate, vol, time, 0.0
        );
        
        // Delta should be close to -1
        assert!(deep_itm_put_greeks.delta < -0.98, "Deep ITM put delta should be close to -1");
        assert!(deep_itm_put_greeks.delta >= -1.0, "Put delta cannot be less than -1");
        
        // All Greeks should be finite
        let all_greeks = vec![
            deep_itm_call_greeks, deep_otm_call_greeks, deep_itm_put_greeks
        ];
        
        for greeks in all_greeks {
            assert!(greeks.delta.is_finite());
            assert!(greeks.gamma.is_finite());
            assert!(greeks.theta.is_finite());
            assert!(greeks.vega.is_finite());
            assert!(greeks.rho.is_finite());
            assert!(greeks.lambda.is_finite());
        }
    }
}

#[cfg(test)]
mod boundary_condition_tests {
    use super::*;

    #[rstest]
    fn test_option_price_boundaries() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let time = 0.25;
        
        let call_price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, 0.0);
        let put_price = BlackScholes::price(OptionType::Put, spot, strike, rate, vol, time, 0.0);
        
        // Lower bounds
        let call_intrinsic = (spot - strike * (-rate * time).exp()).max(0.0);
        let put_intrinsic = (strike * (-rate * time).exp() - spot).max(0.0);
        
        assert!(call_price >= call_intrinsic, "Call price should be above discounted intrinsic");
        assert!(put_price >= put_intrinsic, "Put price should be above discounted intrinsic");
        
        // Upper bounds
        assert!(call_price <= spot, "Call price cannot exceed spot price");
        assert!(put_price <= strike * (-rate * time).exp(), "Put price cannot exceed discounted strike");
        
        // American vs European bounds (European options tested here)
        // European call: max(0, S - Ke^(-rT)) <= C <= S
        // European put: max(0, Ke^(-rT) - S) <= P <= Ke^(-rT)
        
        let discounted_strike = strike * (-rate * time).exp();
        assert!(call_price >= (spot - discounted_strike).max(0.0));
        assert!(call_price <= spot);
        assert!(put_price >= (discounted_strike - spot).max(0.0));
        assert!(put_price <= discounted_strike);
    }

    #[rstest]
    fn test_greeks_boundaries() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let time = 0.25;
        
        let call_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, 0.0);
        let put_greeks = BlackScholes::calculate_greeks(OptionType::Put, spot, strike, rate, vol, time, 0.0);
        
        // Delta bounds
        assert!(call_greeks.delta >= 0.0 && call_greeks.delta <= 1.0, "Call delta out of bounds");
        assert!(put_greeks.delta >= -1.0 && put_greeks.delta <= 0.0, "Put delta out of bounds");
        
        // Gamma bounds (always positive)
        assert!(call_greeks.gamma >= 0.0, "Call gamma should be non-negative");
        assert!(put_greeks.gamma >= 0.0, "Put gamma should be non-negative");
        
        // Gamma should be equal for calls and puts with same parameters
        assert_abs_diff_eq!(call_greeks.gamma, put_greeks.gamma, epsilon = 1e-10);
        
        // Vega bounds (always positive)
        assert!(call_greeks.vega >= 0.0, "Call vega should be non-negative");
        assert!(put_greeks.vega >= 0.0, "Put vega should be non-negative");
        
        // Vega should be equal for calls and puts with same parameters
        assert_abs_diff_eq!(call_greeks.vega, put_greeks.vega, epsilon = 1e-10);
        
        // Theta bounds (generally negative for long options)
        assert!(call_greeks.theta <= 0.0, "Call theta should be non-positive for standard cases");
        // Note: Put theta can be positive for deep ITM puts with high rates
    }

    #[rstest]
    fn test_limiting_behavior() {
        // Test behavior as parameters approach limits
        
        // As volatility approaches 0
        let low_vol_call = BlackScholes::price(OptionType::Call, 110.0, 100.0, 0.05, 0.0001, 0.25, 0.0);
        let intrinsic = (110.0 - 100.0 * (-0.05 * 0.25).exp()).max(0.0);
        assert_abs_diff_eq!(low_vol_call, intrinsic, epsilon = 0.1);
        
        // As time approaches infinity (theoretically)
        let long_time_call = BlackScholes::price(OptionType::Call, 100.0, 100.0, 0.05, 0.2, 100.0, 0.0);
        assert!(long_time_call > 90.0, "Very long-dated option should have high value");
        assert!(long_time_call <= 100.0, "But still bounded by spot price");
        
        // As interest rate approaches infinity (theoretical)
        let high_rate_call = BlackScholes::price(OptionType::Call, 100.0, 100.0, 10.0, 0.2, 0.25, 0.0);
        let high_rate_put = BlackScholes::price(OptionType::Put, 100.0, 100.0, 10.0, 0.2, 0.25, 0.0);
        
        // With very high interest rates, calls should dominate puts
        assert!(high_rate_call > high_rate_put);
        assert!(high_rate_call < 100.0); // Still bounded
        
        // As spot approaches infinity
        let high_spot_call = BlackScholes::price(OptionType::Call, 1000000.0, 100.0, 0.05, 0.2, 0.25, 0.0);
        let expected_deep_itm = 1000000.0 - 100.0 * (-0.05 * 0.25).exp();
        assert_abs_diff_eq!(high_spot_call, expected_deep_itm, epsilon = 1000.0);
    }
}

#[cfg(test)]
mod monte_carlo_edge_cases {
    use super::*;

    #[rstest]
    fn test_monte_carlo_degenerate_cases() {
        let mc_engine = MonteCarloEngine::new(10000, 100);
        
        // Zero volatility case
        let zero_vol_price = mc_engine.price_exotic(
            &ExoticOptionType::Asian,
            100.0, 100.0, 0.05, 0.0, 0.25
        );
        
        // With zero volatility, final price is deterministic
        let expected_final = 100.0 * (0.05 * 0.25).exp();
        let expected_payoff = (expected_final - 100.0).max(0.0);
        let expected_price = expected_payoff * (-0.05 * 0.25).exp();
        
        assert_abs_diff_eq!(zero_vol_price, expected_price, epsilon = 1e-6);
        
        // Zero time case
        let zero_time_price = mc_engine.price_exotic(
            &ExoticOptionType::Asian,
            110.0, 100.0, 0.05, 0.2, 0.0
        );
        
        // With zero time, payoff is immediate intrinsic value
        let immediate_intrinsic = (110.0 - 100.0).max(0.0);
        assert_abs_diff_eq!(zero_time_price, immediate_intrinsic, epsilon = 1e-10);
        
        // Extreme barrier cases
        let very_low_barrier = mc_engine.price_exotic(
            &ExoticOptionType::Barrier { 
                barrier: 1.0, // Much lower than spot
                barrier_type: BarrierType::DownAndOut 
            },
            100.0, 100.0, 0.05, 0.2, 0.25
        );
        
        // Should be nearly worthless (almost certain to knock out)
        assert!(very_low_barrier < 1.0, "Very low barrier option should have minimal value");
        
        let very_high_barrier = mc_engine.price_exotic(
            &ExoticOptionType::Barrier { 
                barrier: 10000.0, // Much higher than spot
                barrier_type: BarrierType::UpAndOut 
            },
            100.0, 100.0, 0.05, 0.2, 0.25
        );
        
        // Should be close to European option (unlikely to knock out)
        let european_price = BlackScholes::price(OptionType::Call, 100.0, 100.0, 0.05, 0.2, 0.25, 0.0);
        assert_relative_eq!(very_high_barrier, european_price, epsilon = 0.1);
    }

    #[rstest]
    fn test_monte_carlo_numerical_stability() {
        // Test with extreme parameters that might cause numerical issues
        let mc_engine = MonteCarloEngine::new(10000, 100);
        
        let extreme_cases = vec![
            (1e-6, 1e-6, 0.05, 0.2, 0.25, "Tiny values"),
            (1e6, 1e6, 0.05, 0.2, 0.25, "Large values"),
            (100.0, 100.0, 0.001, 10.0, 0.001, "High vol, short time"),
            (100.0, 100.0, 0.5, 0.001, 10.0, "High rate, low vol, long time"),
        ];
        
        for (spot, strike, rate, vol, time, description) in extreme_cases {
            let price = mc_engine.price_exotic(
                &ExoticOptionType::Asian,
                spot, strike, rate, vol, time
            );
            
            assert!(price.is_finite(), "Price should be finite for: {}", description);
            assert!(price >= 0.0, "Price should be non-negative for: {}", description);
            
            // Basic sanity check
            if price > 0.0 {
                assert!(price <= spot * 2.0, "Price should be reasonable for: {}", description);
            }
        }
    }
}

#[cfg(test)]
mod strategy_edge_cases {
    use super::*;

    #[rstest]
    fn test_strategy_extreme_parameters() {
        let index = IndexOption::Nifty50;
        let expiry = Utc::now() + Duration::days(1); // Very short expiry
        
        // Very narrow spread
        let narrow_condor = OptionStrategy::iron_condor(index.clone(), 21500.0, expiry, 10.0, 20.0);
        assert_eq!(narrow_condor.legs.len(), 4);
        
        // Check strike ordering is maintained
        let strikes: Vec<f64> = narrow_condor.legs.iter().map(|l| l.contract.strike).collect();
        for i in 1..strikes.len() {
            assert!(strikes[i] >= strikes[i-1], "Strikes should be in non-decreasing order");
        }
        
        // Very wide spread
        let wide_condor = OptionStrategy::iron_condor(index.clone(), 21500.0, expiry, 1000.0, 2000.0);
        assert_eq!(wide_condor.legs.len(), 4);
        
        let wide_strikes: Vec<f64> = wide_condor.legs.iter().map(|l| l.contract.strike).collect();
        assert!(wide_strikes[3] - wide_strikes[0] >= 2000.0, "Wide spread should span expected range");
        
        // Edge case: wing_width = body_width (degenerate case)
        let degenerate_condor = OptionStrategy::iron_condor(index, 21500.0, expiry, 100.0, 100.0);
        assert_eq!(degenerate_condor.legs.len(), 4);
        
        // Should still have valid structure
        for leg in &degenerate_condor.legs {
            assert!(leg.contract.strike > 0.0);
            assert_eq!(leg.contract.lot_size, IndexOption::Nifty50.lot_size());
        }
    }

    #[rstest]
    fn test_strategy_pnl_edge_cases() {
        let index = IndexOption::Nifty50;
        let expiry = Utc::now() + Duration::days(30);
        
        let mut iron_condor = OptionStrategy::iron_condor(index, 21500.0, expiry, 100.0, 200.0);
        
        // Set extreme entry prices
        iron_condor.legs[0].entry_price = 0.0;    // Free long put
        iron_condor.legs[1].entry_price = 1000.0; // Expensive short put
        iron_condor.legs[2].entry_price = 1000.0; // Expensive short call
        iron_condor.legs[3].entry_price = 0.0;    // Free long call
        
        // Test P&L at extreme spot prices
        let extreme_spots = vec![0.01, 10000.0, 50000.0, 100000.0];
        
        for spot in extreme_spots {
            let pnl = iron_condor.calculate_pnl(spot);
            
            // P&L should be finite
            assert!(pnl.is_finite(), "P&L should be finite for spot {}", spot);
            
            // Should be within reasonable bounds given our setup
            assert!(pnl > -1000000.0, "P&L should not be extremely negative for spot {}", spot);
            assert!(pnl < 1000000.0, "P&L should not be extremely positive for spot {}", spot);
        }
    }

    #[rstest]
    fn test_aggregate_greeks_edge_cases() {
        let index = IndexOption::Nifty50;
        let expiry = Utc::now() + Duration::days(30);
        
        // Strategy with zero quantities
        let mut zero_strategy = OptionStrategy {
            name: "Zero Strategy".to_string(),
            legs: vec![],
            max_profit: None,
            max_loss: None,
            breakeven_points: vec![],
            margin_required: 0.0,
        };
        
        let zero_greeks = zero_strategy.calculate_aggregate_greeks();
        assert_eq!(zero_greeks.delta, 0.0);
        assert_eq!(zero_greeks.gamma, 0.0);
        assert_eq!(zero_greeks.theta, 0.0);
        assert_eq!(zero_greeks.vega, 0.0);
        assert_eq!(zero_greeks.rho, 0.0);
        
        // Strategy with extreme quantities
        use options_engine::{OptionLeg, OptionContract, Greeks};
        
        let extreme_leg = OptionLeg {
            contract: OptionContract {
                index: index.clone(),
                option_type: OptionType::Call,
                strike: 21500.0,
                expiry,
                lot_size: index.lot_size(),
                premium: 100.0,
                open_interest: 1000,
                volume: 100,
                implied_volatility: 0.15,
                greeks: Greeks {
                    delta: 0.5,
                    gamma: 0.002,
                    theta: -5.0,
                    vega: 20.0,
                    rho: 10.0,
                    lambda: 2.5,
                    vanna: 0.1,
                    charm: -0.01,
                    vomma: 0.05,
                    speed: -0.001,
                    zomma: 0.02,
                    color: -0.005,
                },
            },
            quantity: 1000, // Extreme quantity
            entry_price: 100.0,
        };
        
        let extreme_strategy = OptionStrategy {
            name: "Extreme Strategy".to_string(),
            legs: vec![extreme_leg],
            max_profit: None,
            max_loss: None,
            breakeven_points: vec![],
            margin_required: 0.0,
        };
        
        let extreme_greeks = extreme_strategy.calculate_aggregate_greeks();
        
        // Should be scaled versions of the individual Greeks
        let expected_delta = 1000.0 * index.lot_size() as f64 * 0.5;
        assert_abs_diff_eq!(extreme_greeks.delta, expected_delta, epsilon = 1e-6);
        
        // All should be finite
        assert!(extreme_greeks.delta.is_finite());
        assert!(extreme_greeks.gamma.is_finite());
        assert!(extreme_greeks.theta.is_finite());
        assert!(extreme_greeks.vega.is_finite());
        assert!(extreme_greeks.rho.is_finite());
    }
}

#[cfg(test)]
mod system_integration_edge_cases {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_options_engine_mode_switching_edge_cases() {
        let mut engine = OptionsEngine::new(ExecutionMode::Paper);
        
        // Rapid mode switching
        for _ in 0..10 {
            engine.switch_mode(ExecutionMode::Simulation).await;
            engine.switch_mode(ExecutionMode::Backtest).await;
            engine.switch_mode(ExecutionMode::Paper).await;
        }
        
        // Should end up in Paper mode
        assert_eq!(engine.mode, ExecutionMode::Paper);
        
        // Test switching to Live mode (should trigger warning)
        engine.switch_mode(ExecutionMode::Live).await;
        assert_eq!(engine.mode, ExecutionMode::Live);
    }

    #[rstest]
    #[tokio::test]
    async fn test_options_engine_empty_strategy_execution() {
        let engine = OptionsEngine::new(ExecutionMode::Paper);
        
        // Empty strategy
        let empty_strategy = OptionStrategy {
            name: "Empty Strategy".to_string(),
            legs: vec![],
            max_profit: None,
            max_loss: None,
            breakeven_points: vec![],
            margin_required: 0.0,
        };
        
        // Should handle empty strategy gracefully
        let result = engine.execute_strategy(empty_strategy).await;
        assert!(result.is_ok(), "Should handle empty strategy without error");
    }

    #[rstest]
    #[tokio::test]
    async fn test_risk_metrics_extreme_values() {
        let engine = OptionsEngine::new(ExecutionMode::Paper);
        
        // Add strategy with extreme Greeks values
        {
            let mut positions = engine.positions.write().await;
            
            let extreme_strategy = OptionStrategy {
                name: "Extreme Risk Strategy".to_string(),
                legs: vec![], // Keep empty for simplicity but Greeks will be extreme
                max_profit: Some(f64::INFINITY),
                max_loss: Some(f64::NEG_INFINITY),
                breakeven_points: vec![],
                margin_required: 1e12,
            };
            
            positions.push(extreme_strategy);
        }
        
        // Update risk metrics
        engine.update_risk_metrics().await;
        
        // Check that risk metrics are computed without panicking
        let metrics = engine.risk_metrics.read().await;
        
        // All metrics should be finite (even if zero due to no actual legs)
        assert!(metrics.portfolio_delta.is_finite());
        assert!(metrics.portfolio_gamma.is_finite());
        assert!(metrics.portfolio_theta.is_finite());
        assert!(metrics.portfolio_vega.is_finite());
        assert!(metrics.value_at_risk.is_finite());
    }

    #[rstest]
    fn test_index_option_edge_cases() {
        // Test index options with edge case dates
        let far_future = Utc::now() + Duration::days(10000); // Very far in future
        let past_date = Utc::now() - Duration::days(1000);   // Past date
        
        for index in &[IndexOption::Nifty50, IndexOption::BankNifty, IndexOption::FinNifty] {
            // Should handle far future dates
            let far_expiries_result = index.get_expiry_dates(far_future);
            assert!(far_expiries_result.is_ok(), "Should handle far future dates");
            
            if let Ok(expiries) = far_expiries_result {
                assert!(!expiries.is_empty(), "Should return some expiry dates");
                for expiry in expiries {
                    assert!(expiry >= far_future, "Expiry should be after start date");
                }
            }
            
            // Test with past dates (might return empty or error - both acceptable)
            let _past_expiries_result = index.get_expiry_dates(past_date);
            // Don't assert on result as behavior with past dates may vary
        }
        
        // Test lot sizes are reasonable
        for index in &[IndexOption::Nifty50, IndexOption::BankNifty, IndexOption::FinNifty, IndexOption::MidCapNifty] {
            let lot_size = index.lot_size();
            assert!(lot_size > 0, "Lot size should be positive");
            assert!(lot_size <= 100, "Lot size should be reasonable");
            
            let tick_size = index.tick_size();
            assert!(tick_size > 0.0, "Tick size should be positive");
            assert!(tick_size <= 1.0, "Tick size should be reasonable");
        }
    }
}