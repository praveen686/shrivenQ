use rstest::*;
use approx::{assert_abs_diff_eq, assert_relative_eq};
use options_engine::{BlackScholes, OptionType, Greeks};

/// Test fixture for standard parameters for Greeks testing
#[fixture]
fn standard_greeks_params() -> (f64, f64, f64, f64, f64, f64) {
    // spot, strike, rate, volatility, time, dividend
    (100.0, 100.0, 0.05, 0.2, 0.25, 0.0)
}

/// Test fixture for Indian market parameters
#[fixture]
fn nifty_greeks_params() -> (f64, f64, f64, f64, f64, f64) {
    // Current Nifty level, strike, Indian risk-free rate, typical IV, 30 days, no dividend
    (21500.0, 21500.0, 0.065, 0.15, 30.0/365.0, 0.0)
}

/// Test fixture for ITM call parameters
#[fixture]
fn itm_call_params() -> (f64, f64, f64, f64, f64, f64) {
    (21500.0, 21000.0, 0.065, 0.15, 30.0/365.0, 0.0)
}

/// Test fixture for OTM call parameters
#[fixture]
fn otm_call_params() -> (f64, f64, f64, f64, f64, f64) {
    (21500.0, 22000.0, 0.065, 0.15, 30.0/365.0, 0.0)
}

/// Numerical derivative helper for testing Greeks accuracy
fn numerical_delta(
    option_type: OptionType,
    spot: f64,
    strike: f64,
    rate: f64,
    vol: f64,
    time: f64,
    dividend: f64,
    ds: f64,
) -> f64 {
    let price_up = BlackScholes::price(option_type, spot + ds, strike, rate, vol, time, dividend);
    let price_down = BlackScholes::price(option_type, spot - ds, strike, rate, vol, time, dividend);
    (price_up - price_down) / (2.0 * ds)
}

fn numerical_gamma(
    option_type: OptionType,
    spot: f64,
    strike: f64,
    rate: f64,
    vol: f64,
    time: f64,
    dividend: f64,
    ds: f64,
) -> f64 {
    let delta_up = numerical_delta(option_type, spot + ds, strike, rate, vol, time, dividend, ds);
    let delta_down = numerical_delta(option_type, spot - ds, strike, rate, vol, time, dividend, ds);
    (delta_up - delta_down) / (2.0 * ds)
}

fn numerical_theta(
    option_type: OptionType,
    spot: f64,
    strike: f64,
    rate: f64,
    vol: f64,
    time: f64,
    dividend: f64,
    dt: f64,
) -> f64 {
    let price_now = BlackScholes::price(option_type, spot, strike, rate, vol, time, dividend);
    let price_later = BlackScholes::price(option_type, spot, strike, rate, vol, time - dt, dividend);
    (price_later - price_now) / dt
}

fn numerical_vega(
    option_type: OptionType,
    spot: f64,
    strike: f64,
    rate: f64,
    vol: f64,
    time: f64,
    dividend: f64,
    dvol: f64,
) -> f64 {
    let price_up = BlackScholes::price(option_type, spot, strike, rate, vol + dvol, time, dividend);
    let price_down = BlackScholes::price(option_type, spot, strike, rate, vol - dvol, time, dividend);
    (price_up - price_down) / (2.0 * dvol)
}

fn numerical_rho(
    option_type: OptionType,
    spot: f64,
    strike: f64,
    rate: f64,
    vol: f64,
    time: f64,
    dividend: f64,
    dr: f64,
) -> f64 {
    let price_up = BlackScholes::price(option_type, spot, strike, rate + dr, vol, time, dividend);
    let price_down = BlackScholes::price(option_type, spot, strike, rate - dr, vol, time, dividend);
    (price_up - price_down) / (2.0 * dr)
}

#[cfg(test)]
mod delta_tests {
    use super::*;

    #[rstest]
    fn test_call_delta_bounds(standard_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = standard_greeks_params;
        
        let greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        
        // Call delta should be between 0 and 1
        assert!(greeks.delta >= 0.0);
        assert!(greeks.delta <= 1.0);
    }

    #[rstest]
    fn test_put_delta_bounds(standard_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = standard_greeks_params;
        
        let greeks = BlackScholes::calculate_greeks(OptionType::Put, spot, strike, rate, vol, time, dividend);
        
        // Put delta should be between -1 and 0
        assert!(greeks.delta >= -1.0);
        assert!(greeks.delta <= 0.0);
    }

    #[rstest]
    fn test_atm_delta_values(standard_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = standard_greeks_params;
        
        let call_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let put_greeks = BlackScholes::calculate_greeks(OptionType::Put, spot, strike, rate, vol, time, dividend);
        
        // ATM call delta should be around 0.5 (slightly above due to positive rates)
        assert!(call_greeks.delta > 0.45);
        assert!(call_greeks.delta < 0.55);
        
        // ATM put delta should be around -0.5 (slightly below due to positive rates)
        assert!(put_greeks.delta > -0.55);
        assert!(put_greeks.delta < -0.45);
        
        // Put-call delta relationship: call_delta - put_delta = 1
        assert_abs_diff_eq!(call_greeks.delta - put_greeks.delta, 1.0, epsilon = 1e-10);
    }

    #[rstest]
    fn test_deep_itm_delta(itm_call_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = itm_call_params;
        
        let call_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        
        // Deep ITM call should have delta close to 1
        assert!(call_greeks.delta > 0.8);
        assert!(call_greeks.delta <= 1.0);
        
        // Corresponding deep ITM put (high strike)
        let put_greeks = BlackScholes::calculate_greeks(OptionType::Put, spot, spot + 500.0, rate, vol, time, dividend);
        
        // Deep ITM put should have delta close to -1
        assert!(put_greeks.delta < -0.8);
        assert!(put_greeks.delta >= -1.0);
    }

    #[rstest]
    fn test_deep_otm_delta(otm_call_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = otm_call_params;
        
        let call_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        
        // Deep OTM call should have delta close to 0
        assert!(call_greeks.delta > 0.0);
        assert!(call_greeks.delta < 0.3);
        
        // Corresponding deep OTM put (low strike)
        let put_greeks = BlackScholes::calculate_greeks(OptionType::Put, spot, spot - 500.0, rate, vol, time, dividend);
        
        // Deep OTM put should have delta close to 0
        assert!(put_greeks.delta < 0.0);
        assert!(put_greeks.delta > -0.3);
    }

    #[rstest]
    fn test_delta_numerical_accuracy(nifty_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = nifty_greeks_params;
        
        let greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let numerical = numerical_delta(OptionType::Call, spot, strike, rate, vol, time, dividend, 0.01);
        
        // Analytical delta should match numerical delta within tolerance
        assert_abs_diff_eq!(greeks.delta, numerical, epsilon = 1e-4);
    }

    #[rstest]
    fn test_delta_time_decay_effect() {
        let spot = 21500.0;
        let strike = 21600.0; // Slightly OTM
        let rate = 0.065;
        let vol = 0.15;
        let dividend = 0.0;
        
        let long_time = 90.0 / 365.0;
        let short_time = 7.0 / 365.0;
        
        let delta_long = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, long_time, dividend).delta;
        let delta_short = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, short_time, dividend).delta;
        
        // For OTM calls, delta should decrease as expiry approaches
        assert!(delta_long > delta_short);
    }
}

#[cfg(test)]
mod gamma_tests {
    use super::*;

    #[rstest]
    fn test_gamma_positive(standard_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = standard_greeks_params;
        
        let call_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let put_greeks = BlackScholes::calculate_greeks(OptionType::Put, spot, strike, rate, vol, time, dividend);
        
        // Gamma should always be positive for both calls and puts
        assert!(call_greeks.gamma > 0.0);
        assert!(put_greeks.gamma > 0.0);
        
        // Call and put gamma should be equal for same strike/expiry
        assert_abs_diff_eq!(call_greeks.gamma, put_greeks.gamma, epsilon = 1e-10);
    }

    #[rstest]
    fn test_atm_gamma_maximum(nifty_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = nifty_greeks_params;
        
        let atm_gamma = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend).gamma;
        
        // Test gamma at different strikes
        let otm_gamma = BlackScholes::calculate_greeks(OptionType::Call, spot, strike + 500.0, rate, vol, time, dividend).gamma;
        let itm_gamma = BlackScholes::calculate_greeks(OptionType::Call, spot, strike - 500.0, rate, vol, time, dividend).gamma;
        
        // ATM gamma should be higher than both ITM and OTM gamma
        assert!(atm_gamma > otm_gamma);
        assert!(atm_gamma > itm_gamma);
    }

    #[rstest]
    fn test_gamma_numerical_accuracy(nifty_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = nifty_greeks_params;
        
        let greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let numerical = numerical_gamma(OptionType::Call, spot, strike, rate, vol, time, dividend, 0.01);
        
        // Analytical gamma should match numerical gamma within tolerance
        assert_abs_diff_eq!(greeks.gamma, numerical, epsilon = 1e-6);
    }

    #[rstest]
    fn test_gamma_time_decay() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let dividend = 0.0;
        
        let long_time = 90.0 / 365.0;
        let short_time = 7.0 / 365.0;
        
        let gamma_long = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, long_time, dividend).gamma;
        let gamma_short = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, short_time, dividend).gamma;
        
        // For ATM options, gamma increases as expiry approaches
        assert!(gamma_short > gamma_long);
    }

    #[rstest]
    fn test_gamma_volatility_relationship() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let time = 30.0 / 365.0;
        let dividend = 0.0;
        
        let low_vol = 0.10;
        let high_vol = 0.30;
        
        let gamma_low_vol = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, low_vol, time, dividend).gamma;
        let gamma_high_vol = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, high_vol, time, dividend).gamma;
        
        // Higher volatility should result in lower gamma for ATM options
        assert!(gamma_low_vol > gamma_high_vol);
    }
}

#[cfg(test)]
mod theta_tests {
    use super::*;

    #[rstest]
    fn test_theta_negative_for_long_options(standard_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = standard_greeks_params;
        
        let call_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let put_greeks = BlackScholes::calculate_greeks(OptionType::Put, spot, strike, rate, vol, time, dividend);
        
        // Theta should be negative for long options (time decay)
        assert!(call_greeks.theta < 0.0);
        assert!(put_greeks.theta < 0.0);
    }

    #[rstest]
    fn test_atm_theta_magnitude(nifty_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = nifty_greeks_params;
        
        let atm_theta = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend).theta;
        
        // Test theta at different strikes
        let otm_theta = BlackScholes::calculate_greeks(OptionType::Call, spot, strike + 500.0, rate, vol, time, dividend).theta;
        let itm_theta = BlackScholes::calculate_greeks(OptionType::Call, spot, strike - 500.0, rate, vol, time, dividend).theta;
        
        // ATM options typically have the highest time decay (most negative theta)
        assert!(atm_theta.abs() > otm_theta.abs());
        // ITM might have higher theta due to interest rate component
    }

    #[rstest]
    fn test_theta_numerical_accuracy(nifty_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = nifty_greeks_params;
        
        let greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let numerical = numerical_theta(OptionType::Call, spot, strike, rate, vol, time, dividend, 1.0/365.0);
        
        // Analytical theta should match numerical theta within tolerance
        // Note: Theta is converted to daily in the implementation
        assert_abs_diff_eq!(greeks.theta * 365.0, numerical, epsilon = 1e-3);
    }

    #[rstest]
    fn test_theta_time_acceleration() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let dividend = 0.0;
        
        let long_time = 90.0 / 365.0;
        let short_time = 7.0 / 365.0;
        
        let theta_long = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, long_time, dividend).theta;
        let theta_short = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, short_time, dividend).theta;
        
        // Theta should become more negative (accelerate) as expiry approaches
        assert!(theta_short < theta_long); // More negative
    }

    #[rstest]
    fn test_deep_itm_put_theta() {
        let spot = 21500.0;
        let strike = 22500.0; // Deep ITM put
        let rate = 0.065;
        let vol = 0.15;
        let time = 90.0 / 365.0;
        let dividend = 0.0;
        
        let put_greeks = BlackScholes::calculate_greeks(OptionType::Put, spot, strike, rate, vol, time, dividend);
        
        // Deep ITM puts can have positive theta due to interest rate effect
        // This is a unique characteristic when interest rates are positive
        assert!(put_greeks.theta > 0.0 || put_greeks.theta.abs() < 0.1); // Either positive or very small negative
    }
}

#[cfg(test)]
mod vega_tests {
    use super::*;

    #[rstest]
    fn test_vega_positive(standard_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = standard_greeks_params;
        
        let call_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let put_greeks = BlackScholes::calculate_greeks(OptionType::Put, spot, strike, rate, vol, time, dividend);
        
        // Vega should always be positive for both calls and puts
        assert!(call_greeks.vega > 0.0);
        assert!(put_greeks.vega > 0.0);
        
        // Call and put vega should be equal for same strike/expiry
        assert_abs_diff_eq!(call_greeks.vega, put_greeks.vega, epsilon = 1e-10);
    }

    #[rstest]
    fn test_atm_vega_maximum(nifty_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = nifty_greeks_params;
        
        let atm_vega = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend).vega;
        
        // Test vega at different strikes
        let otm_vega = BlackScholes::calculate_greeks(OptionType::Call, spot, strike + 500.0, rate, vol, time, dividend).vega;
        let itm_vega = BlackScholes::calculate_greeks(OptionType::Call, spot, strike - 500.0, rate, vol, time, dividend).vega;
        
        // ATM vega should be higher than both ITM and OTM vega
        assert!(atm_vega > otm_vega);
        assert!(atm_vega > itm_vega);
    }

    #[rstest]
    fn test_vega_numerical_accuracy(nifty_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = nifty_greeks_params;
        
        let greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let numerical = numerical_vega(OptionType::Call, spot, strike, rate, vol, time, dividend, 0.001);
        
        // Analytical vega should match numerical vega within tolerance
        // Note: Vega is scaled by /100 in the implementation (per 1% change)
        assert_abs_diff_eq!(greeks.vega * 100.0, numerical, epsilon = 1e-3);
    }

    #[rstest]
    fn test_vega_time_decay() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let dividend = 0.0;
        
        let long_time = 90.0 / 365.0;
        let short_time = 7.0 / 365.0;
        
        let vega_long = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, long_time, dividend).vega;
        let vega_short = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, short_time, dividend).vega;
        
        // Vega should decrease as time to expiry decreases
        assert!(vega_long > vega_short);
    }

    #[rstest]
    fn test_vega_scaling() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let time = 30.0 / 365.0;
        let dividend = 0.0;
        
        let greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        
        // Vega should be reasonable for Indian market context
        // For Nifty options, vega typically ranges from 10-100 for ATM options
        assert!(greeks.vega > 5.0);
        assert!(greeks.vega < 200.0);
    }
}

#[cfg(test)]
mod rho_tests {
    use super::*;

    #[rstest]
    fn test_call_rho_positive_put_rho_negative(standard_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = standard_greeks_params;
        
        let call_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let put_greeks = BlackScholes::calculate_greeks(OptionType::Put, spot, strike, rate, vol, time, dividend);
        
        // Call rho should be positive (calls benefit from higher rates)
        assert!(call_greeks.rho > 0.0);
        
        // Put rho should be negative (puts suffer from higher rates)
        assert!(put_greeks.rho < 0.0);
    }

    #[rstest]
    fn test_rho_numerical_accuracy(nifty_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = nifty_greeks_params;
        
        let greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let numerical = numerical_rho(OptionType::Call, spot, strike, rate, vol, time, dividend, 0.0001);
        
        // Analytical rho should match numerical rho within tolerance
        // Note: Rho is scaled by /100 in the implementation (per 1% change)
        assert_abs_diff_eq!(greeks.rho * 100.0, numerical, epsilon = 1e-3);
    }

    #[rstest]
    fn test_itm_options_higher_rho() {
        let spot = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let time = 90.0 / 365.0;
        let dividend = 0.0;
        
        let atm_strike = 21500.0;
        let itm_call_strike = 21000.0;
        let otm_call_strike = 22000.0;
        
        let atm_rho = BlackScholes::calculate_greeks(OptionType::Call, spot, atm_strike, rate, vol, time, dividend).rho;
        let itm_rho = BlackScholes::calculate_greeks(OptionType::Call, spot, itm_call_strike, rate, vol, time, dividend).rho;
        let otm_rho = BlackScholes::calculate_greeks(OptionType::Call, spot, otm_call_strike, rate, vol, time, dividend).rho;
        
        // ITM options should have higher rho magnitude than OTM options
        assert!(itm_rho > atm_rho);
        assert!(atm_rho > otm_rho);
        assert!(otm_rho > 0.0); // But still positive for calls
    }

    #[rstest]
    fn test_rho_time_relationship() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let dividend = 0.0;
        
        let long_time = 365.0 / 365.0; // 1 year
        let short_time = 30.0 / 365.0;  // 1 month
        
        let rho_long = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, long_time, dividend).rho;
        let rho_short = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, short_time, dividend).rho;
        
        // Longer-dated options should have higher rho
        assert!(rho_long > rho_short);
    }
}

#[cfg(test)]
mod higher_order_greeks_tests {
    use super::*;

    #[rstest]
    fn test_lambda_calculation(nifty_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = nifty_greeks_params;
        
        let call_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let call_price = BlackScholes::price(OptionType::Call, spot, strike, rate, vol, time, dividend);
        
        // Lambda should equal Delta * Spot / Option_Price
        let expected_lambda = call_greeks.delta * spot / call_price;
        assert_abs_diff_eq!(call_greeks.lambda, expected_lambda, epsilon = 1e-6);
        
        // Lambda should be greater than 1 (leverage effect)
        assert!(call_greeks.lambda > 1.0);
    }

    #[rstest]
    fn test_vanna_properties(nifty_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = nifty_greeks_params;
        
        let call_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let put_greeks = BlackScholes::calculate_greeks(OptionType::Put, spot, strike, rate, vol, time, dividend);
        
        // Vanna should be equal for calls and puts with same parameters
        assert_abs_diff_eq!(call_greeks.vanna, put_greeks.vanna, epsilon = 1e-10);
        
        // For ATM options, vanna is typically negative
        // (delta decreases as volatility increases for ATM options near expiry)
    }

    #[rstest]
    fn test_charm_time_sensitivity() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let dividend = 0.0;
        
        let long_time = 90.0 / 365.0;
        let short_time = 7.0 / 365.0;
        
        let charm_long = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, long_time, dividend).charm;
        let charm_short = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, short_time, dividend).charm;
        
        // Charm magnitude typically increases as expiry approaches
        assert!(charm_short.abs() > charm_long.abs());
    }

    #[rstest]
    fn test_vomma_volatility_convexity(nifty_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = nifty_greeks_params;
        
        let greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        
        // Vomma represents the convexity of vega with respect to volatility
        // For ATM options, vomma is typically positive
        // (vega increases at an increasing rate as volatility increases)
        
        // Just ensure it's calculated and finite
        assert!(greeks.vomma.is_finite());
    }

    #[rstest]
    fn test_speed_gamma_sensitivity(nifty_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = nifty_greeks_params;
        
        let greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        
        // Speed is the rate of change of gamma with respect to underlying price
        // For ATM options, speed is typically negative
        // (gamma decreases as we move away from ATM in either direction)
        
        assert!(greeks.speed.is_finite());
    }

    #[rstest]
    fn test_zomma_gamma_volatility_sensitivity(nifty_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = nifty_greeks_params;
        
        let greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        
        // Zomma is the sensitivity of gamma to changes in volatility
        // Should be finite and reasonable
        assert!(greeks.zomma.is_finite());
    }

    #[rstest]
    fn test_color_gamma_time_decay(nifty_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = nifty_greeks_params;
        
        let greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        
        // Color is the rate of change of gamma over time
        // For ATM options approaching expiry, color is typically negative
        // (gamma increases as time passes, so color represents this acceleration)
        
        assert!(greeks.color.is_finite());
    }
}

#[cfg(test)]
mod greeks_edge_cases_tests {
    use super::*;

    #[rstest]
    fn test_zero_time_greeks() {
        let spot = 100.0;
        let strike = 95.0;
        let rate = 0.05;
        let vol = 0.2;
        let time = 0.0;
        let dividend = 0.0;
        
        let call_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let put_greeks = BlackScholes::calculate_greeks(OptionType::Put, spot, strike, rate, vol, time, dividend);
        
        // With zero time, most Greeks should be zero or default
        assert_eq!(call_greeks.delta, 0.0);
        assert_eq!(call_greeks.gamma, 0.0);
        assert_eq!(call_greeks.theta, 0.0);
        assert_eq!(call_greeks.vega, 0.0);
        assert_eq!(call_greeks.rho, 0.0);
        
        // Same for puts
        assert_eq!(put_greeks.delta, 0.0);
        assert_eq!(put_greeks.gamma, 0.0);
        assert_eq!(put_greeks.theta, 0.0);
        assert_eq!(put_greeks.vega, 0.0);
        assert_eq!(put_greeks.rho, 0.0);
    }

    #[rstest]
    fn test_extreme_volatility_greeks() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let time = 30.0 / 365.0;
        let dividend = 0.0;
        
        // Test very high volatility
        let high_vol = 3.0; // 300% volatility
        let greeks_high_vol = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, high_vol, time, dividend);
        
        // Greeks should be finite and reasonable
        assert!(greeks_high_vol.delta > 0.0 && greeks_high_vol.delta < 1.0);
        assert!(greeks_high_vol.gamma > 0.0 && greeks_high_vol.gamma.is_finite());
        assert!(greeks_high_vol.vega > 0.0 && greeks_high_vol.vega.is_finite());
        
        // Test very low volatility
        let low_vol = 0.001; // 0.1% volatility
        let greeks_low_vol = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, low_vol, time, dividend);
        
        // Greeks should still be reasonable
        assert!(greeks_low_vol.delta > 0.0 && greeks_low_vol.delta < 1.0);
        assert!(greeks_low_vol.gamma > 0.0 && greeks_low_vol.gamma.is_finite());
        assert!(greeks_low_vol.vega > 0.0 && greeks_low_vol.vega.is_finite());
    }

    #[rstest]
    fn test_extreme_moneyness_greeks() {
        let spot = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let time = 30.0 / 365.0;
        let dividend = 0.0;
        
        // Deep ITM call (very low strike)
        let very_low_strike = 15000.0;
        let deep_itm_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, very_low_strike, rate, vol, time, dividend);
        
        // Delta should be close to 1, gamma close to 0
        assert!(deep_itm_greeks.delta > 0.95);
        assert!(deep_itm_greeks.gamma < 0.001); // Very small gamma for deep ITM
        
        // Deep OTM call (very high strike)
        let very_high_strike = 28000.0;
        let deep_otm_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, very_high_strike, rate, vol, time, dividend);
        
        // Delta should be close to 0, gamma close to 0
        assert!(deep_otm_greeks.delta < 0.05);
        assert!(deep_otm_greeks.gamma < 0.001); // Very small gamma for deep OTM
    }

    #[rstest]
    fn test_negative_rates_greeks() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = -0.01; // Negative interest rate
        let vol = 0.15;
        let time = 30.0 / 365.0;
        let dividend = 0.0;
        
        let call_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let put_greeks = BlackScholes::calculate_greeks(OptionType::Put, spot, strike, rate, vol, time, dividend);
        
        // Greeks should still be calculated properly with negative rates
        assert!(call_greeks.delta > 0.0 && call_greeks.delta < 1.0);
        assert!(put_greeks.delta < 0.0 && put_greeks.delta > -1.0);
        
        // Rho signs should reverse with negative rates
        assert!(call_greeks.rho < 0.0); // Negative rate hurts calls
        assert!(put_greeks.rho > 0.0);  // Negative rate helps puts
        
        // Other Greeks should remain reasonable
        assert!(call_greeks.gamma > 0.0);
        assert!(call_greeks.vega > 0.0);
        assert!(call_greeks.theta < 0.0);
    }

    #[rstest]
    fn test_high_dividend_yield_greeks() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let time = 90.0 / 365.0;
        let dividend = 0.08; // 8% dividend yield
        
        let call_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let put_greeks = BlackScholes::calculate_greeks(OptionType::Put, spot, strike, rate, vol, time, dividend);
        
        // High dividend should reduce call delta and increase put delta magnitude
        let no_div_call_delta = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, 0.0).delta;
        let no_div_put_delta = BlackScholes::calculate_greeks(OptionType::Put, spot, strike, rate, vol, time, 0.0).delta;
        
        assert!(call_greeks.delta < no_div_call_delta);
        assert!(put_greeks.delta.abs() > no_div_put_delta.abs());
        
        // Other Greeks should remain reasonable
        assert!(call_greeks.gamma > 0.0);
        assert!(call_greeks.vega > 0.0);
    }
}

#[cfg(test)]
mod greeks_consistency_tests {
    use super::*;

    #[rstest]
    fn test_put_call_greeks_relationships(nifty_greeks_params: (f64, f64, f64, f64, f64, f64)) {
        let (spot, strike, rate, vol, time, dividend) = nifty_greeks_params;
        
        let call_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        let put_greeks = BlackScholes::calculate_greeks(OptionType::Put, spot, strike, rate, vol, time, dividend);
        
        // Put-Call relationships
        assert_abs_diff_eq!(call_greeks.delta - put_greeks.delta, 1.0, epsilon = 1e-10);
        assert_abs_diff_eq!(call_greeks.gamma, put_greeks.gamma, epsilon = 1e-10);
        assert_abs_diff_eq!(call_greeks.vega, put_greeks.vega, epsilon = 1e-10);
        assert_abs_diff_eq!(call_greeks.vanna, put_greeks.vanna, epsilon = 1e-10);
        assert_abs_diff_eq!(call_greeks.vomma, put_greeks.vomma, epsilon = 1e-10);
        assert_abs_diff_eq!(call_greeks.zomma, put_greeks.zomma, epsilon = 1e-10);
        assert_abs_diff_eq!(call_greeks.color, put_greeks.color, epsilon = 1e-10);
    }

    #[rstest]
    fn test_greeks_scaling_consistency() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let time = 30.0 / 365.0;
        let dividend = 0.0;
        
        let greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        
        // Test that vega is properly scaled (per 1% vol change)
        let vol1 = vol;
        let vol2 = vol + 0.01;
        let price1 = BlackScholes::price(OptionType::Call, spot, strike, rate, vol1, time, dividend);
        let price2 = BlackScholes::price(OptionType::Call, spot, strike, rate, vol2, time, dividend);
        let price_diff = price2 - price1;
        
        assert_abs_diff_eq!(greeks.vega, price_diff, epsilon = 1e-3);
        
        // Test that rho is properly scaled (per 1% rate change)
        let rate1 = rate;
        let rate2 = rate + 0.01;
        let price_r1 = BlackScholes::price(OptionType::Call, spot, strike, rate1, vol, time, dividend);
        let price_r2 = BlackScholes::price(OptionType::Call, spot, strike, rate2, vol, time, dividend);
        let price_r_diff = price_r2 - price_r1;
        
        assert_abs_diff_eq!(greeks.rho, price_r_diff, epsilon = 1e-3);
    }

    #[rstest]
    fn test_greeks_cross_derivatives() {
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let vol = 0.15;
        let time = 30.0 / 365.0;
        let dividend = 0.0;
        
        let base_greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time, dividend);
        
        // Test that vanna equals the cross-derivative of delta w.r.t. volatility
        let dvol = 0.001;
        let delta_up = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol + dvol, time, dividend).delta;
        let delta_down = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol - dvol, time, dividend).delta;
        let numerical_vanna = (delta_up - delta_down) / (2.0 * dvol);
        
        assert_abs_diff_eq!(base_greeks.vanna, numerical_vanna, epsilon = 1e-4);
        
        // Test that charm equals the cross-derivative of delta w.r.t. time
        let dt = 1.0 / 365.0;
        let delta_later = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, vol, time - dt, time, dividend).delta;
        let numerical_charm = (delta_later - base_greeks.delta) / (-dt);
        
        assert_abs_diff_eq!(base_greeks.charm, numerical_charm, epsilon = 1e-3);
    }
}