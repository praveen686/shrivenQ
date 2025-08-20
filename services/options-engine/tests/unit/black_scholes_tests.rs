use rstest::*;
use approx::{assert_abs_diff_eq, assert_relative_eq};
use options_engine::{BlackScholes, OptionType};
use std::f64::consts::PI;

/// Test fixture for standard Black-Scholes parameters
#[fixture]
fn standard_params() -> (f64, f64, f64, f64, f64, f64) {
    // spot, strike, rate, volatility, time, dividend
    (100.0, 100.0, 0.05, 0.2, 0.25, 0.0)
}

/// Test fixture for Indian market parameters (Nifty options)
#[fixture]
fn nifty_params() -> (f64, f64, f64, f64, f64, f64) {
    // Current Nifty level, strike, Indian risk-free rate, typical IV, 30 days, no dividend
    (21500.0, 21500.0, 0.065, 0.15, 30.0/365.0, 0.0)
}

/// Test fixture for deep ITM call parameters
#[fixture]
fn deep_itm_call_params() -> (f64, f64, f64, f64, f64, f64) {
    // Deep ITM call: spot much higher than strike
    (21500.0, 20000.0, 0.065, 0.15, 30.0/365.0, 0.0)
}

/// Test fixture for deep OTM call parameters  
#[fixture]
fn deep_otm_call_params() -> (f64, f64, f64, f64, f64, f64) {
    // Deep OTM call: spot much lower than strike
    (21500.0, 23000.0, 0.065, 0.15, 30.0/365.0, 0.0)
}

#[cfg(test)]
mod black_scholes_pricing_tests {
    use super::*;

    #[rstest]
    fn test_norm_cdf_standard_values() {
        // Test standard normal CDF at key points
        assert_abs_diff_eq!(BlackScholes::norm_cdf(0.0), 0.5, epsilon = 1e-10);
        assert_abs_diff_eq!(BlackScholes::norm_cdf(1.96), 0.975, epsilon = 1e-3);
        assert_abs_diff_eq!(BlackScholes::norm_cdf(-1.96), 0.025, epsilon = 1e-3);
        assert_abs_diff_eq!(BlackScholes::norm_cdf(3.0), 0.9987, epsilon = 1e-4);
        assert_abs_diff_eq!(BlackScholes::norm_cdf(-3.0), 0.0013, epsilon = 1e-4);
    }

    #[rstest]
    fn test_norm_pdf_standard_values() {
        // Test standard normal PDF at key points
        assert_abs_diff_eq!(BlackScholes::norm_pdf(0.0), 1.0 / (2.0 * PI).sqrt(), epsilon = 1e-10);
        assert_abs_diff_eq!(BlackScholes::norm_pdf(1.0), 0.24197, epsilon = 1e-5);
        assert_abs_diff_eq!(BlackScholes::norm_pdf(-1.0), 0.24197, epsilon = 1e-5);
    }

    #[rstest]
    fn test_d1_d2_calculation(standard_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, _) = standard_params;
        
        let d1 = BlackScholes::d1(spot, strike, rate, vol, time);
        let d2 = BlackScholes::d2(spot, strike, rate, vol, time);
        
        // d2 should equal d1 minus vol * sqrt(time)
        assert_abs_diff_eq!(d2, d1 - vol * time.sqrt(), epsilon = 1e-10);
        
        // For ATM options with positive time, d1 should be close to (rate + 0.5*vol^2)*time / (vol*sqrt(time))
        let expected_d1 = (rate + 0.5 * vol * vol) * time / (vol * time.sqrt());
        assert_abs_diff_eq!(d1, expected_d1, epsilon = 1e-10);
    }

    #[rstest]
    fn test_atm_call_price(standard_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = standard_params;
        
        let price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, dividend);
        
        // ATM call should have positive value
        assert!(price > 0.0);
        
        // For ATM call with no dividend, price should be approximately spot * N(d1) - strike * exp(-r*T) * N(d2)
        let d1 = BlackScholes::d1(spot, strike, rate, vol, time);
        let d2 = BlackScholes::d2(spot, strike, rate, vol, time);
        let expected_price = spot * BlackScholes::norm_cdf(d1) - strike * (-rate * time).exp() * BlackScholes::norm_cdf(d2);
        
        assert_abs_diff_eq!(price, expected_price, epsilon = 1e-10);
    }

    #[rstest]
    fn test_atm_put_price(standard_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = standard_params;
        
        let put_price = BlackScholes::price(OptionType::Put, spot, strike, rate, vol, time, dividend);
        
        // ATM put should have positive value
        assert!(put_price > 0.0);
        
        // Verify put-call parity: C - P = S - K*e^(-r*T)
        let call_price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let parity_diff = call_price - put_price;
        let expected_diff = spot - strike * (-rate * time).exp();
        
        assert_abs_diff_eq!(parity_diff, expected_diff, epsilon = 1e-10);
    }

    #[rstest]
    fn test_deep_itm_call_pricing(deep_itm_call_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = deep_itm_call_params;
        
        let call_price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, dividend);
        
        // Deep ITM call should be worth approximately intrinsic value
        let intrinsic = spot - strike;
        assert!(call_price > intrinsic);
        assert!(call_price < intrinsic + 200.0); // Should not be too much above intrinsic
        
        // Time value should be positive but small for deep ITM
        let time_value = call_price - intrinsic;
        assert!(time_value > 0.0);
        assert!(time_value < intrinsic * 0.05); // Time value less than 5% of intrinsic
    }

    #[rstest]
    fn test_deep_otm_call_pricing(deep_otm_call_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = deep_otm_call_params;
        
        let call_price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, dividend);
        
        // Deep OTM call should have very small value
        assert!(call_price > 0.0);
        assert!(call_price < 50.0); // Should be small for deep OTM with short time
        
        // Should be much less than intrinsic value would be if ITM
        let would_be_intrinsic = strike - spot; // Would be negative, so option worthless
        assert!(call_price < would_be_intrinsic.abs() * 0.1);
    }

    #[rstest]
    fn test_zero_time_to_expiry() {
        let spot = 100.0;
        let strike = 95.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.0;
        let dividend = 0.0;
        
        // Call option with zero time should equal max(S-K, 0)
        let call_price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let expected_call = (spot - strike).max(0.0);
        assert_abs_diff_eq!(call_price, expected_call, epsilon = 1e-10);
        
        // Put option with zero time should equal max(K-S, 0)
        let put_price = BlackScholes::price(OptionType::Put, spot, strike, rate, vol, time, dividend);
        let expected_put = (strike - spot).max(0.0);
        assert_abs_diff_eq!(put_price, expected_put, epsilon = 1e-10);
    }

    #[rstest]
    fn test_indian_market_scenario(nifty_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = nifty_params;
        
        let call_price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let put_price = BlackScholes::price(OptionType::Put, spot, strike, rate, vol, time, dividend);
        
        // Both prices should be reasonable for Indian markets
        assert!(call_price > 20.0 && call_price < 500.0);
        assert!(put_price > 20.0 && put_price < 500.0);
        
        // Verify put-call parity holds
        let parity_diff = call_price - put_price;
        let expected_diff = spot - strike * (-rate * time).exp();
        assert_abs_diff_eq!(parity_diff, expected_diff, epsilon = 1e-8);
    }

    #[rstest]
    fn test_volatility_impact() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let time = 30.0 / 365.0;
        let dividend = 0.0;
        
        let low_vol = 0.10;
        let high_vol = 0.30;
        
        let call_low_vol = BlackScholes::price(OptionType::Call, spot, strike, rate, low_vol, time, dividend);
        let call_high_vol = BlackScholes::price(OptionType::Call, spot, strike, rate, high_vol, time, dividend);
        
        // Higher volatility should result in higher option price
        assert!(call_high_vol > call_low_vol);
        
        // The difference should be substantial
        assert!(call_high_vol / call_low_vol > 1.5);
    }

    #[rstest]
    fn test_time_decay_effect() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let dividend = 0.0;
        
        let long_time = 90.0 / 365.0;
        let short_time = 7.0 / 365.0;
        
        let call_long = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, long_time, dividend);
        let call_short = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, short_time, dividend);
        
        // Longer time to expiry should result in higher option price
        assert!(call_long > call_short);
        
        // The difference should reflect time decay
        assert!(call_long / call_short > 1.8); // Significantly higher with more time
    }

    #[rstest]
    fn test_moneyness_relationships() {
        let spot = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let time = 30.0 / 365.0;
        let dividend = 0.0;
        
        let itm_strike = 21000.0; // ITM call
        let atm_strike = 21500.0; // ATM call
        let otm_strike = 22000.0; // OTM call
        
        let call_itm = BlackScholes::price(OptionType::Call, spot, itm_strike, rate, vol, time, dividend);
        let call_atm = BlackScholes::price(OptionType::Call, spot, atm_strike, rate, vol, time, dividend);
        let call_otm = BlackScholes::price(OptionType::Call, spot, otm_strike, rate, vol, time, dividend);
        
        // ITM > ATM > OTM for calls
        assert!(call_itm > call_atm);
        assert!(call_atm > call_otm);
        
        // For puts, the relationship should be reversed
        let put_itm = BlackScholes::price(OptionType::Put, spot, otm_strike, rate, vol, time, dividend); // ITM put
        let put_atm = BlackScholes::price(OptionType::Put, spot, atm_strike, rate, vol, time, dividend);
        let put_otm = BlackScholes::price(OptionType::Put, spot, itm_strike, rate, vol, time, dividend); // OTM put
        
        assert!(put_itm > put_atm);
        assert!(put_atm > put_otm);
    }

    #[rstest]
    fn test_numerical_stability_extreme_values() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let time = 1.0 / 365.0; // 1 day
        let dividend = 0.0;
        
        // Test with very high volatility
        let high_vol = 2.0; // 200% volatility
        let price_high_vol = BlackScholes::price(OptionType::Call, spot, strike, rate, high_vol, time, dividend);
        assert!(price_high_vol > 0.0);
        assert!(price_high_vol.is_finite());
        
        // Test with very low volatility
        let low_vol = 0.001; // 0.1% volatility
        let price_low_vol = BlackScholes::price(OptionType::Call, spot, strike, rate, low_vol, time, dividend);
        assert!(price_low_vol > 0.0);
        assert!(price_low_vol.is_finite());
        
        // Test with very short time
        let very_short_time = 1.0 / (365.0 * 24.0); // 1 hour
        let price_short_time = BlackScholes::price(OptionType::Call, spot, strike, rate, 0.15, very_short_time, dividend);
        assert!(price_short_time >= 0.0);
        assert!(price_short_time.is_finite());
    }

    #[rstest]
    fn test_dividend_impact() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let time = 90.0 / 365.0;
        
        let no_dividend = 0.0;
        let with_dividend = 0.02; // 2% dividend yield
        
        let call_no_div = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, no_dividend);
        let call_with_div = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, with_dividend);
        
        // Dividends should reduce call option value
        assert!(call_with_div < call_no_div);
        
        let put_no_div = BlackScholes::price(OptionType::Put, spot, strike, rate, vol, time, no_dividend);
        let put_with_div = BlackScholes::price(OptionType::Put, spot, strike, rate, vol, time, with_dividend);
        
        // Dividends should increase put option value
        assert!(put_with_div > put_no_div);
    }

    #[rstest]
    fn test_interest_rate_sensitivity() {
        let spot = 21500.0;
        let strike = 21500.0;
        let vol = 0.15;
        let time = 90.0 / 365.0;
        let dividend = 0.0;
        
        let low_rate = 0.02; // 2% interest rate
        let high_rate = 0.08; // 8% interest rate
        
        let call_low_rate = BlackScholes::price(OptionType::Call, spot, strike, low_rate, vol, time, dividend);
        let call_high_rate = BlackScholes::price(OptionType::Call, spot, strike, high_rate, vol, time, dividend);
        
        // Higher interest rate should increase call option value
        assert!(call_high_rate > call_low_rate);
        
        let put_low_rate = BlackScholes::price(OptionType::Put, spot, strike, low_rate, vol, time, dividend);
        let put_high_rate = BlackScholes::price(OptionType::Put, spot, strike, high_rate, vol, time, dividend);
        
        // Higher interest rate should decrease put option value
        assert!(put_high_rate < put_low_rate);
    }

    #[rstest]
    fn test_boundary_conditions() {
        let spot = 100.0;
        let strike = 100.0;
        let rate = 0.05;
        let vol = 0.2;
        let dividend = 0.0;
        
        // Test very long time to expiry
        let long_time = 10.0; // 10 years
        let call_long = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, long_time, dividend);
        
        // Call option should approach the spot price for very long expiry
        assert!(call_long > spot * 0.8); // Should be substantial portion of spot
        assert!(call_long < spot * 1.2); // But not exceed reasonable bounds
        
        // Test when spot is much larger than strike
        let high_spot = 200.0;
        let call_high_spot = BlackScholes::price(OptionType::Call, high_spot, strike, rate, vol, 0.25, dividend);
        
        // Should be close to intrinsic value plus some time value
        let intrinsic = high_spot - strike;
        assert!(call_high_spot > intrinsic);
        assert!(call_high_spot < intrinsic + 20.0); // Reasonable time value
    }
}

#[cfg(test)]
mod implied_volatility_tests {
    use super::*;

    #[rstest]
    fn test_implied_volatility_accuracy(standard_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, true_vol, time, dividend) = standard_params;
        
        // Calculate theoretical price with known volatility
        let theoretical_price = BlackScholes::price(OptionType::Call, spot, strike, rate, true_vol, time, dividend);
        
        // Calculate implied volatility from the theoretical price
        let implied_vol = BlackScholes::implied_volatility(
            OptionType::Call, spot, strike, rate, time, theoretical_price, dividend
        ).unwrap();
        
        // Implied volatility should match the original volatility
        assert_abs_diff_eq!(implied_vol, true_vol, epsilon = 1e-6);
    }

    #[rstest]
    fn test_implied_volatility_convergence() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let time = 30.0 / 365.0;
        let dividend = 0.0;
        
        // Test convergence for various volatility levels
        let test_vols = vec![0.05, 0.10, 0.15, 0.20, 0.30, 0.50];
        
        for true_vol in test_vols {
            let price = BlackScholes::price(OptionType::Call, spot, strike, rate, true_vol, time, dividend);
            
            let implied_vol = BlackScholes::implied_volatility(
                OptionType::Call, spot, strike, rate, time, price, dividend
            ).unwrap();
            
            assert_abs_diff_eq!(implied_vol, true_vol, epsilon = 1e-4);
        }
    }

    #[rstest]
    fn test_implied_volatility_edge_cases() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let time = 7.0 / 365.0;
        let dividend = 0.0;
        
        // Test with very low price (should give low IV)
        let low_price = 1.0;
        let low_iv = BlackScholes::implied_volatility(
            OptionType::Call, spot, strike, rate, time, low_price, dividend
        );
        assert!(low_iv.is_ok());
        assert!(low_iv.unwrap() > 0.0);
        assert!(low_iv.unwrap() < 0.5);
        
        // Test with high price (should give high IV)
        let high_price = 500.0;
        let high_iv = BlackScholes::implied_volatility(
            OptionType::Call, spot, strike, rate, time, high_price, dividend
        );
        assert!(high_iv.is_ok());
        assert!(high_iv.unwrap() > 0.5);
    }

    #[rstest]
    fn test_implied_volatility_bounds() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let time = 30.0 / 365.0;
        let dividend = 0.0;
        
        // Generate various option prices and check IV bounds
        let prices = vec![10.0, 50.0, 100.0, 200.0, 300.0];
        
        for price in prices {
            if let Ok(iv) = BlackScholes::implied_volatility(
                OptionType::Call, spot, strike, rate, time, price, dividend
            ) {
                // Implied volatility should be within reasonable bounds
                assert!(iv >= 0.001); // At least 0.1%
                assert!(iv <= 5.0);   // At most 500%
            }
        }
    }

    #[rstest]
    fn test_implied_volatility_put_call_consistency() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let time = 30.0 / 365.0;
        let dividend = 0.0;
        
        // Calculate call and put prices
        let call_price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let put_price = BlackScholes::price(OptionType::Put, spot, strike, rate, vol, time, dividend);
        
        // Calculate implied volatilities
        let call_iv = BlackScholes::implied_volatility(
            OptionType::Call, spot, strike, rate, time, call_price, dividend
        ).unwrap();
        
        let put_iv = BlackScholes::implied_volatility(
            OptionType::Put, spot, strike, rate, time, put_price, dividend
        ).unwrap();
        
        // Both should give the same implied volatility
        assert_abs_diff_eq!(call_iv, put_iv, epsilon = 1e-6);
        assert_abs_diff_eq!(call_iv, vol, epsilon = 1e-6);
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[rstest]
    fn test_pricing_performance() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let time = 30.0 / 365.0;
        let dividend = 0.0;
        
        let iterations = 10000;
        let start = Instant::now();
        
        for _ in 0..iterations {
            let _price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, dividend);
        }
        
        let duration = start.elapsed();
        let per_calculation = duration.as_nanos() as f64 / iterations as f64;
        
        // Each pricing calculation should take less than 1 microsecond
        assert!(per_calculation < 1000.0, "Pricing too slow: {:.2}ns per calculation", per_calculation);
    }

    #[rstest]
    fn test_implied_volatility_performance() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let time = 30.0 / 365.0;
        let dividend = 0.0;
        
        // Pre-calculate a market price
        let market_price = BlackScholes::price(OptionType::Call, spot, strike, rate, 0.15, time, dividend);
        
        let iterations = 1000;
        let start = Instant::now();
        
        for _ in 0..iterations {
            let _iv = BlackScholes::implied_volatility(
                OptionType::Call, spot, strike, rate, time, market_price, dividend
            ).unwrap();
        }
        
        let duration = start.elapsed();
        let per_calculation = duration.as_micros() as f64 / iterations as f64;
        
        // Each IV calculation should take less than 100 microseconds
        assert!(per_calculation < 100.0, "IV calculation too slow: {:.2}μs per calculation", per_calculation);
    }
}