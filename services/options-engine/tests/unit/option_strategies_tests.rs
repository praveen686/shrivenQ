use rstest::*;
use approx::{assert_abs_diff_eq, assert_relative_eq};
use options_engine::{OptionStrategy, OptionLeg, OptionContract, OptionType, IndexOption, Greeks};
use chrono::{DateTime, Utc, Duration};

/// Test fixture for standard strategy parameters
#[fixture]
fn standard_strategy_params() -> (IndexOption, f64, DateTime<Utc>, f64, f64) {
    // index, spot, expiry, wing_width, body_width
    (
        IndexOption::Nifty50,
        21500.0,
        Utc::now() + Duration::days(30),
        100.0,
        200.0
    )
}

/// Test fixture for Nifty options parameters
#[fixture]
fn nifty_params() -> (IndexOption, f64, DateTime<Utc>) {
    (
        IndexOption::Nifty50,
        21500.0,
        Utc::now() + Duration::days(7)
    )
}

/// Test fixture for Bank Nifty parameters
#[fixture]
fn bank_nifty_params() -> (IndexOption, f64, DateTime<Utc>) {
    (
        IndexOption::BankNifty,
        48000.0,
        Utc::now() + Duration::days(14)
    )
}

/// Create a sample option leg for testing
fn create_test_option_leg(
    index: IndexOption,
    option_type: OptionType,
    strike: f64,
    expiry: DateTime<Utc>,
    quantity: i32,
    entry_price: f64,
    greeks: Greeks,
) -> OptionLeg {
    OptionLeg {
        contract: OptionContract {
            index: index.clone(),
            option_type,
            strike,
            expiry,
            lot_size: index.lot_size(),
            premium: entry_price,
            open_interest: 10000,
            volume: 1000,
            implied_volatility: 0.15,
            greeks,
        },
        quantity,
        entry_price,
    }
}

/// Create sample Greeks for testing
fn create_test_greeks(delta: f64, gamma: f64, theta: f64, vega: f64, rho: f64) -> Greeks {
    Greeks {
        delta,
        gamma,
        theta,
        vega,
        rho,
        lambda: delta * 100.0 / 50.0, // Simplified lambda
        vanna: 0.1,
        charm: -0.01,
        vomma: 0.05,
        speed: -0.001,
        zomma: 0.02,
        color: -0.005,
    }
}

#[cfg(test)]
mod iron_condor_tests {
    use super::*;

    #[rstest]
    fn test_iron_condor_creation(standard_strategy_params: (IndexOption, f64, DateTime<Utc>, f64, f64)) {
        let (index, spot, expiry, wing_width, body_width) = standard_strategy_params;
        
        let iron_condor = OptionStrategy::iron_condor(index.clone(), spot, expiry, wing_width, body_width);
        
        // Check basic properties
        assert_eq!(iron_condor.name, "Iron Condor");
        assert_eq!(iron_condor.legs.len(), 4);
        
        // Check leg structure: Put spread + Call spread
        let leg_types: Vec<OptionType> = iron_condor.legs.iter().map(|leg| leg.contract.option_type).collect();
        assert_eq!(leg_types, vec![OptionType::Put, OptionType::Put, OptionType::Call, OptionType::Call]);
        
        // Check quantities: Long Put, Short Put, Short Call, Long Call
        let quantities: Vec<i32> = iron_condor.legs.iter().map(|leg| leg.quantity).collect();
        assert_eq!(quantities, vec![1, -1, -1, 1]);
        
        // Check strike relationships
        let strikes: Vec<f64> = iron_condor.legs.iter().map(|leg| leg.contract.strike).collect();
        assert!(strikes[0] < strikes[1]); // Long put < Short put
        assert!(strikes[1] < strikes[2]); // Short put < Short call
        assert!(strikes[2] < strikes[3]); // Short call < Long call
        
        // Check lot sizes match index
        for leg in &iron_condor.legs {
            assert_eq!(leg.contract.lot_size, index.lot_size());
            assert_eq!(leg.contract.index, index);
        }
    }

    #[rstest]
    fn test_iron_condor_strike_calculation(standard_strategy_params: (IndexOption, f64, DateTime<Utc>, f64, f64)) {
        let (index, spot, expiry, wing_width, body_width) = standard_strategy_params;
        
        let iron_condor = OptionStrategy::iron_condor(index, spot, expiry, wing_width, body_width);
        
        let strikes: Vec<f64> = iron_condor.legs.iter().map(|leg| leg.contract.strike).collect();
        
        // Calculate expected strikes
        let put_short_expected = spot - body_width / 2.0;
        let put_long_expected = put_short_expected - wing_width;
        let call_short_expected = spot + body_width / 2.0;
        let call_long_expected = call_short_expected + wing_width;
        
        assert_abs_diff_eq!(strikes[0], put_long_expected, epsilon = 1e-6);
        assert_abs_diff_eq!(strikes[1], put_short_expected, epsilon = 1e-6);
        assert_abs_diff_eq!(strikes[2], call_short_expected, epsilon = 1e-6);
        assert_abs_diff_eq!(strikes[3], call_long_expected, epsilon = 1e-6);
    }

    #[rstest]
    fn test_iron_condor_different_indices() {
        let expiry = Utc::now() + Duration::days(15);
        
        // Test with different indices
        let indices = vec![
            (IndexOption::Nifty50, 21500.0),
            (IndexOption::BankNifty, 48000.0),
            (IndexOption::FinNifty, 20000.0),
            (IndexOption::MidCapNifty, 10000.0),
        ];
        
        for (index, spot) in indices {
            let iron_condor = OptionStrategy::iron_condor(index.clone(), spot, expiry, 100.0, 200.0);
            
            // Check that all legs have correct index and lot size
            for leg in &iron_condor.legs {
                assert_eq!(leg.contract.index, index);
                assert_eq!(leg.contract.lot_size, index.lot_size());
            }
            
            // Check max loss calculation
            if let Some(max_loss) = iron_condor.max_loss {
                assert_eq!(max_loss, 100.0 * index.lot_size() as f64); // wing_width * lot_size
            }
        }
    }

    #[rstest]
    fn test_iron_condor_breakeven_points(standard_strategy_params: (IndexOption, f64, DateTime<Utc>, f64, f64)) {
        let (index, spot, expiry, wing_width, body_width) = standard_strategy_params;
        
        let iron_condor = OptionStrategy::iron_condor(index, spot, expiry, wing_width, body_width);
        
        // Iron condor should have two breakeven points
        assert_eq!(iron_condor.breakeven_points.len(), 2);
        
        let put_short_strike = spot - body_width / 2.0;
        let call_short_strike = spot + body_width / 2.0;
        
        // Breakeven points should be at the short strikes (simplified)
        assert_abs_diff_eq!(iron_condor.breakeven_points[0], put_short_strike, epsilon = 1e-6);
        assert_abs_diff_eq!(iron_condor.breakeven_points[1], call_short_strike, epsilon = 1e-6);
    }

    #[rstest]
    fn test_iron_condor_max_loss(standard_strategy_params: (IndexOption, f64, DateTime<Utc>, f64, f64)) {
        let (index, spot, expiry, wing_width, body_width) = standard_strategy_params;
        
        let iron_condor = OptionStrategy::iron_condor(index.clone(), spot, expiry, wing_width, body_width);
        
        // Max loss should be wing width * lot size
        let expected_max_loss = wing_width * index.lot_size() as f64;
        
        if let Some(max_loss) = iron_condor.max_loss {
            assert_abs_diff_eq!(max_loss, expected_max_loss, epsilon = 1e-6);
        } else {
            panic!("Iron condor should have a defined max loss");
        }
    }
}

#[cfg(test)]
mod option_strategy_pnl_tests {
    use super::*;

    #[rstest]
    fn test_calculate_pnl_basic_functionality(nifty_params: (IndexOption, f64, DateTime<Utc>)) {
        let (index, spot, expiry) = nifty_params;
        
        // Create a simple bull call spread
        let mut strategy = OptionStrategy {
            name: "Bull Call Spread".to_string(),
            legs: vec![
                create_test_option_leg(
                    index.clone(),
                    OptionType::Call,
                    spot - 100.0, // 21400 strike - buy
                    expiry,
                    1,
                    100.0, // Entry price
                    create_test_greeks(0.6, 0.001, -5.0, 20.0, 15.0),
                ),
                create_test_option_leg(
                    index.clone(),
                    OptionType::Call,
                    spot + 100.0, // 21600 strike - sell
                    expiry,
                    -1,
                    50.0, // Entry price
                    create_test_greeks(0.4, 0.001, -3.0, 15.0, 10.0),
                ),
            ],
            max_profit: None,
            max_loss: None,
            breakeven_points: vec![],
            margin_required: 0.0,
        };
        
        // Test P&L at different spot prices
        let test_spots = vec![21300.0, 21400.0, 21500.0, 21600.0, 21700.0];
        
        for test_spot in test_spots {
            let pnl = strategy.calculate_pnl(test_spot);
            
            // P&L should be finite
            assert!(pnl.is_finite());
            
            // Calculate expected P&L manually
            let long_call_intrinsic = (test_spot - (spot - 100.0)).max(0.0);
            let short_call_intrinsic = (test_spot - (spot + 100.0)).max(0.0);
            
            let long_call_pnl = (long_call_intrinsic - 100.0) * index.lot_size() as f64;
            let short_call_pnl = -(short_call_intrinsic - 50.0) * index.lot_size() as f64;
            let expected_pnl = long_call_pnl + short_call_pnl;
            
            assert_abs_diff_eq!(pnl, expected_pnl, epsilon = 1e-6);
        }
    }

    #[rstest]
    fn test_calculate_pnl_iron_condor(standard_strategy_params: (IndexOption, f64, DateTime<Utc>, f64, f64)) {
        let (index, spot, expiry, wing_width, body_width) = standard_strategy_params;
        
        let mut iron_condor = OptionStrategy::iron_condor(index.clone(), spot, expiry, wing_width, body_width);
        
        // Set realistic entry prices
        iron_condor.legs[0].entry_price = 10.0;  // Long Put (21400)
        iron_condor.legs[1].entry_price = 40.0;  // Short Put (21500)
        iron_condor.legs[2].entry_price = 35.0;  // Short Call (21600)
        iron_condor.legs[3].entry_price = 8.0;   // Long Call (21700)
        
        // Test P&L across different scenarios
        let test_cases = vec![
            (21000.0, "Deep ITM Put scenario"),
            (21450.0, "Near put spread"),
            (21500.0, "ATM scenario"),
            (21550.0, "Between strikes"),
            (21650.0, "Near call spread"),
            (22000.0, "Deep ITM Call scenario"),
        ];
        
        for (test_spot, description) in test_cases {
            let pnl = iron_condor.calculate_pnl(test_spot);
            
            // P&L should be finite
            assert!(pnl.is_finite(), "P&L not finite for {}", description);
            
            // For an iron condor with the strikes we have:
            // - Maximum loss should occur at the wings
            // - Maximum profit should occur between the short strikes
            
            if test_spot < 21400.0 || test_spot > 21700.0 {
                // Should approach max loss at the wings
                // Max loss = wing_width * lot_size - net_credit
                let expected_max_loss = wing_width * index.lot_size() as f64;
                assert!(pnl > -expected_max_loss - 1000.0, "P&L too negative at wings for {}: {}", description, pnl);
            }
            
            if test_spot > 21500.0 && test_spot < 21600.0 {
                // Should be profitable in the middle
                // Net credit = 40 + 35 - 10 - 8 = 57 per unit
                let net_credit = (40.0 + 35.0 - 10.0 - 8.0) * index.lot_size() as f64;
                assert!(pnl >= 0.0 && pnl <= net_credit + 100.0, "P&L outside expected range in profit zone for {}: {}", description, pnl);
            }
        }
    }

    #[rstest]
    fn test_calculate_pnl_single_option(nifty_params: (IndexOption, f64, DateTime<Utc>)) {
        let (index, spot, expiry) = nifty_params;
        
        // Create a simple long call position
        let strategy = OptionStrategy {
            name: "Long Call".to_string(),
            legs: vec![
                create_test_option_leg(
                    index.clone(),
                    OptionType::Call,
                    spot, // ATM call
                    expiry,
                    1,
                    50.0, // Entry price
                    create_test_greeks(0.5, 0.002, -8.0, 25.0, 12.0),
                ),
            ],
            max_profit: None,
            max_loss: Some(50.0 * index.lot_size() as f64),
            breakeven_points: vec![spot + 50.0],
            margin_required: 0.0,
        };
        
        // Test P&L calculation
        let test_cases = vec![
            (spot - 100.0, -50.0 * index.lot_size() as f64), // OTM - lose premium
            (spot, -50.0 * index.lot_size() as f64), // ATM - lose premium
            (spot + 50.0, 0.0), // Breakeven
            (spot + 100.0, 50.0 * index.lot_size() as f64), // ITM - gain intrinsic
        ];
        
        for (test_spot, expected_pnl) in test_cases {
            let pnl = strategy.calculate_pnl(test_spot);
            assert_abs_diff_eq!(pnl, expected_pnl, epsilon = 1e-6);
        }
    }

    #[rstest]
    fn test_calculate_pnl_put_spread(bank_nifty_params: (IndexOption, f64, DateTime<Utc>)) {
        let (index, spot, expiry) = bank_nifty_params;
        
        // Create a bear put spread
        let strategy = OptionStrategy {
            name: "Bear Put Spread".to_string(),
            legs: vec![
                create_test_option_leg(
                    index.clone(),
                    OptionType::Put,
                    spot + 200.0, // 48200 strike - buy (higher strike)
                    expiry,
                    1,
                    150.0, // Entry price
                    create_test_greeks(-0.3, 0.0008, -10.0, 30.0, -8.0),
                ),
                create_test_option_leg(
                    index.clone(),
                    OptionType::Put,
                    spot - 200.0, // 47800 strike - sell (lower strike)
                    expiry,
                    -1,
                    80.0, // Entry price
                    create_test_greeks(-0.15, 0.0005, -6.0, 20.0, -5.0),
                ),
            ],
            max_profit: Some((150.0 - 80.0 + 200.0) * index.lot_size() as f64),
            max_loss: Some((150.0 - 80.0) * index.lot_size() as f64),
            breakeven_points: vec![spot + 200.0 - (150.0 - 80.0)],
            margin_required: 5000.0,
        };
        
        let test_cases = vec![
            (47500.0, "Deep ITM - max profit"),
            (47800.0, "At short strike"),
            (48000.0, "ATM"),
            (48200.0, "At long strike"),
            (48500.0, "Deep OTM - max loss"),
        ];
        
        for (test_spot, description) in test_cases {
            let pnl = strategy.calculate_pnl(test_spot);
            assert!(pnl.is_finite(), "P&L not finite for {}", description);
            
            // Check that P&L is within reasonable bounds
            let net_debit = (150.0 - 80.0) * index.lot_size() as f64;
            let max_profit = 200.0 * index.lot_size() as f64 - net_debit;
            
            assert!(pnl >= -net_debit - 1.0, "P&L below max loss for {}: {}", description, pnl);
            assert!(pnl <= max_profit + 1.0, "P&L above max profit for {}: {}", description, pnl);
        }
    }

    #[rstest]
    fn test_calculate_pnl_straddle(nifty_params: (IndexOption, f64, DateTime<Utc>)) {
        let (index, spot, expiry) = nifty_params;
        
        // Create a long straddle
        let strategy = OptionStrategy {
            name: "Long Straddle".to_string(),
            legs: vec![
                create_test_option_leg(
                    index.clone(),
                    OptionType::Call,
                    spot, // ATM call
                    expiry,
                    1,
                    100.0,
                    create_test_greeks(0.5, 0.002, -8.0, 25.0, 12.0),
                ),
                create_test_option_leg(
                    index.clone(),
                    OptionType::Put,
                    spot, // ATM put
                    expiry,
                    1,
                    95.0,
                    create_test_greeks(-0.5, 0.002, -8.0, 25.0, -12.0),
                ),
            ],
            max_profit: None,
            max_loss: Some(195.0 * index.lot_size() as f64),
            breakeven_points: vec![spot - 195.0, spot + 195.0],
            margin_required: 0.0,
        };
        
        let test_cases = vec![
            (spot - 300.0, "Deep ITM put"),
            (spot - 195.0, "Put breakeven"),
            (spot, "ATM - max loss"),
            (spot + 195.0, "Call breakeven"),
            (spot + 300.0, "Deep ITM call"),
        ];
        
        for (test_spot, description) in test_cases {
            let pnl = strategy.calculate_pnl(test_spot);
            assert!(pnl.is_finite(), "P&L not finite for {}", description);
            
            if (test_spot - spot).abs() < 50.0 {
                // Near ATM - should be close to max loss
                let max_loss = 195.0 * index.lot_size() as f64;
                assert!(pnl >= -max_loss - 10.0, "P&L not near max loss at ATM for {}: {}", description, pnl);
            }
            
            if (test_spot - spot).abs() > 300.0 {
                // Far from ATM - should be profitable
                assert!(pnl > 0.0, "P&L should be profitable far from ATM for {}: {}", description, pnl);
            }
        }
    }
}

#[cfg(test)]
mod aggregate_greeks_tests {
    use super::*;

    #[rstest]
    fn test_calculate_aggregate_greeks_simple() {
        let index = IndexOption::Nifty50;
        let expiry = Utc::now() + Duration::days(30);
        
        // Create a simple two-leg strategy
        let strategy = OptionStrategy {
            name: "Test Strategy".to_string(),
            legs: vec![
                create_test_option_leg(
                    index.clone(),
                    OptionType::Call,
                    21500.0,
                    expiry,
                    2, // Long 2 contracts
                    100.0,
                    create_test_greeks(0.5, 0.002, -5.0, 20.0, 10.0),
                ),
                create_test_option_leg(
                    index.clone(),
                    OptionType::Put,
                    21500.0,
                    expiry,
                    -1, // Short 1 contract
                    80.0,
                    create_test_greeks(-0.5, 0.002, -3.0, 20.0, -8.0),
                ),
            ],
            max_profit: None,
            max_loss: None,
            breakeven_points: vec![],
            margin_required: 0.0,
        };
        
        let aggregate_greeks = strategy.calculate_aggregate_greeks();
        
        let lot_size = index.lot_size() as f64;
        
        // Expected calculations:
        // Delta: 2 * 50 * 0.5 + (-1) * 50 * (-0.5) = 50 + 25 = 75
        let expected_delta = 2.0 * lot_size * 0.5 + (-1.0) * lot_size * (-0.5);
        assert_abs_diff_eq!(aggregate_greeks.delta, expected_delta, epsilon = 1e-6);
        
        // Gamma: 2 * 50 * 0.002 + (-1) * 50 * 0.002 = 0.2 - 0.1 = 0.1
        let expected_gamma = 2.0 * lot_size * 0.002 + (-1.0) * lot_size * 0.002;
        assert_abs_diff_eq!(aggregate_greeks.gamma, expected_gamma, epsilon = 1e-6);
        
        // Theta: 2 * 50 * (-5.0) + (-1) * 50 * (-3.0) = -500 + 150 = -350
        let expected_theta = 2.0 * lot_size * (-5.0) + (-1.0) * lot_size * (-3.0);
        assert_abs_diff_eq!(aggregate_greeks.theta, expected_theta, epsilon = 1e-6);
        
        // Vega: 2 * 50 * 20.0 + (-1) * 50 * 20.0 = 2000 - 1000 = 1000
        let expected_vega = 2.0 * lot_size * 20.0 + (-1.0) * lot_size * 20.0;
        assert_abs_diff_eq!(aggregate_greeks.vega, expected_vega, epsilon = 1e-6);
        
        // Rho: 2 * 50 * 10.0 + (-1) * 50 * (-8.0) = 1000 + 400 = 1400
        let expected_rho = 2.0 * lot_size * 10.0 + (-1.0) * lot_size * (-8.0);
        assert_abs_diff_eq!(aggregate_greeks.rho, expected_rho, epsilon = 1e-6);
    }

    #[rstest]
    fn test_calculate_aggregate_greeks_iron_condor(standard_strategy_params: (IndexOption, f64, DateTime<Utc>, f64, f64)) {
        let (index, spot, expiry, wing_width, body_width) = standard_strategy_params;
        
        let mut iron_condor = OptionStrategy::iron_condor(index.clone(), spot, expiry, wing_width, body_width);
        
        // Set Greeks for each leg (realistic values for iron condor)
        iron_condor.legs[0].contract.greeks = create_test_greeks(-0.2, 0.001, -2.0, 15.0, -5.0); // Long Put
        iron_condor.legs[1].contract.greeks = create_test_greeks(-0.4, 0.0015, -4.0, 18.0, -8.0); // Short Put
        iron_condor.legs[2].contract.greeks = create_test_greeks(0.4, 0.0015, -4.0, 18.0, 8.0); // Short Call
        iron_condor.legs[3].contract.greeks = create_test_greeks(0.2, 0.001, -2.0, 15.0, 5.0); // Long Call
        
        let aggregate_greeks = iron_condor.calculate_aggregate_greeks();
        let lot_size = index.lot_size() as f64;
        
        // Expected calculations for iron condor:
        // Delta: 1*50*(-0.2) + (-1)*50*(-0.4) + (-1)*50*(0.4) + 1*50*(0.2) = -10 + 20 - 20 + 10 = 0
        let expected_delta = 1.0 * lot_size * (-0.2) + (-1.0) * lot_size * (-0.4) + 
                           (-1.0) * lot_size * 0.4 + 1.0 * lot_size * 0.2;
        assert_abs_diff_eq!(aggregate_greeks.delta, expected_delta, epsilon = 1e-6);
        
        // Iron condor should be approximately delta neutral
        assert!(aggregate_greeks.delta.abs() < lot_size * 0.1, "Iron condor should be approximately delta neutral");
        
        // Gamma should be negative for iron condor (short gamma)
        let expected_gamma = 1.0 * lot_size * 0.001 + (-1.0) * lot_size * 0.0015 + 
                           (-1.0) * lot_size * 0.0015 + 1.0 * lot_size * 0.001;
        assert_abs_diff_eq!(aggregate_greeks.gamma, expected_gamma, epsilon = 1e-6);
        assert!(aggregate_greeks.gamma < 0.0, "Iron condor should have negative gamma");
        
        // Theta should be positive for iron condor (time decay benefits)
        let expected_theta = 1.0 * lot_size * (-2.0) + (-1.0) * lot_size * (-4.0) + 
                           (-1.0) * lot_size * (-4.0) + 1.0 * lot_size * (-2.0);
        assert_abs_diff_eq!(aggregate_greeks.theta, expected_theta, epsilon = 1e-6);
        assert!(aggregate_greeks.theta > 0.0, "Iron condor should have positive theta");
        
        // Vega should be negative for iron condor (short volatility)
        let expected_vega = 1.0 * lot_size * 15.0 + (-1.0) * lot_size * 18.0 + 
                          (-1.0) * lot_size * 18.0 + 1.0 * lot_size * 15.0;
        assert_abs_diff_eq!(aggregate_greeks.vega, expected_vega, epsilon = 1e-6);
        assert!(aggregate_greeks.vega < 0.0, "Iron condor should have negative vega");
    }

    #[rstest]
    fn test_calculate_aggregate_greeks_different_lot_sizes() {
        let expiry = Utc::now() + Duration::days(30);
        
        // Test with different indices having different lot sizes
        let test_cases = vec![
            (IndexOption::Nifty50, 50),
            (IndexOption::BankNifty, 25),
            (IndexOption::FinNifty, 25),
            (IndexOption::MidCapNifty, 50),
        ];
        
        for (index, expected_lot_size) in test_cases {
            let strategy = OptionStrategy {
                name: "Test Strategy".to_string(),
                legs: vec![
                    create_test_option_leg(
                        index.clone(),
                        OptionType::Call,
                        21500.0,
                        expiry,
                        1,
                        100.0,
                        create_test_greeks(0.5, 0.002, -5.0, 20.0, 10.0),
                    ),
                ],
                max_profit: None,
                max_loss: None,
                breakeven_points: vec![],
                margin_required: 0.0,
            };
            
            let aggregate_greeks = strategy.calculate_aggregate_greeks();
            
            // Delta should be scaled by lot size
            let expected_delta = 1.0 * expected_lot_size as f64 * 0.5;
            assert_abs_diff_eq!(aggregate_greeks.delta, expected_delta, epsilon = 1e-6);
        }
    }

    #[rstest]
    fn test_calculate_aggregate_greeks_zero_quantities() {
        let index = IndexOption::Nifty50;
        let expiry = Utc::now() + Duration::days(30);
        
        // Create a strategy with zero quantity (closed position)
        let strategy = OptionStrategy {
            name: "Closed Strategy".to_string(),
            legs: vec![
                create_test_option_leg(
                    index.clone(),
                    OptionType::Call,
                    21500.0,
                    expiry,
                    0, // Zero quantity
                    100.0,
                    create_test_greeks(0.5, 0.002, -5.0, 20.0, 10.0),
                ),
            ],
            max_profit: None,
            max_loss: None,
            breakeven_points: vec![],
            margin_required: 0.0,
        };
        
        let aggregate_greeks = strategy.calculate_aggregate_greeks();
        
        // All Greeks should be zero
        assert_eq!(aggregate_greeks.delta, 0.0);
        assert_eq!(aggregate_greeks.gamma, 0.0);
        assert_eq!(aggregate_greeks.theta, 0.0);
        assert_eq!(aggregate_greeks.vega, 0.0);
        assert_eq!(aggregate_greeks.rho, 0.0);
    }

    #[rstest]
    fn test_calculate_aggregate_greeks_large_positions() {
        let index = IndexOption::Nifty50;
        let expiry = Utc::now() + Duration::days(30);
        
        // Create a strategy with large positions
        let strategy = OptionStrategy {
            name: "Large Position Strategy".to_string(),
            legs: vec![
                create_test_option_leg(
                    index.clone(),
                    OptionType::Call,
                    21500.0,
                    expiry,
                    100, // Large long position
                    100.0,
                    create_test_greeks(0.5, 0.002, -5.0, 20.0, 10.0),
                ),
                create_test_option_leg(
                    index.clone(),
                    OptionType::Put,
                    21500.0,
                    expiry,
                    -50, // Large short position
                    80.0,
                    create_test_greeks(-0.5, 0.002, -3.0, 20.0, -8.0),
                ),
            ],
            max_profit: None,
            max_loss: None,
            breakeven_points: vec![],
            margin_required: 50000.0,
        };
        
        let aggregate_greeks = strategy.calculate_aggregate_greeks();
        let lot_size = index.lot_size() as f64;
        
        // Verify scaling with large quantities
        let expected_delta = 100.0 * lot_size * 0.5 + (-50.0) * lot_size * (-0.5);
        assert_abs_diff_eq!(aggregate_greeks.delta, expected_delta, epsilon = 1e-6);
        
        // All Greeks should scale proportionally
        assert!(aggregate_greeks.delta.abs() > 1000.0); // Should be large
        assert!(aggregate_greeks.vega.abs() > 10000.0); // Should be large
        
        // Results should be finite
        assert!(aggregate_greeks.delta.is_finite());
        assert!(aggregate_greeks.gamma.is_finite());
        assert!(aggregate_greeks.theta.is_finite());
        assert!(aggregate_greeks.vega.is_finite());
        assert!(aggregate_greeks.rho.is_finite());
    }
}

#[cfg(test)]
mod option_strategy_validation_tests {
    use super::*;

    #[rstest]
    fn test_strategy_properties_consistency(standard_strategy_params: (IndexOption, f64, DateTime<Utc>, f64, f64)) {
        let (index, spot, expiry, wing_width, body_width) = standard_strategy_params;
        
        let iron_condor = OptionStrategy::iron_condor(index.clone(), spot, expiry, wing_width, body_width);
        
        // Check that all legs have consistent properties
        for leg in &iron_condor.legs {
            assert_eq!(leg.contract.index, index);
            assert_eq!(leg.contract.expiry, expiry);
            assert_eq!(leg.contract.lot_size, index.lot_size());
            
            // Quantities should be reasonable (-10 to 10 for most strategies)
            assert!(leg.quantity.abs() <= 10);
            
            // Entry prices should be non-negative
            assert!(leg.entry_price >= 0.0);
            
            // Strikes should be positive
            assert!(leg.contract.strike > 0.0);
        }
        
        // Strategy should have a name
        assert!(!iron_condor.name.is_empty());
        
        // Breakeven points should be reasonable
        for &breakeven in &iron_condor.breakeven_points {
            assert!(breakeven > 0.0);
            assert!(breakeven.is_finite());
        }
        
        // Max loss should be reasonable if defined
        if let Some(max_loss) = iron_condor.max_loss {
            assert!(max_loss > 0.0);
            assert!(max_loss.is_finite());
            assert!(max_loss < 1_000_000.0); // Sanity check
        }
    }

    #[rstest]
    fn test_strategy_edge_cases() {
        let index = IndexOption::Nifty50;
        let expiry = Utc::now() + Duration::days(1); // Very short expiry
        
        // Test with very narrow spreads
        let narrow_spread = OptionStrategy::iron_condor(index.clone(), 21500.0, expiry, 25.0, 50.0);
        assert_eq!(narrow_spread.legs.len(), 4);
        
        // Check that strikes are still ordered correctly
        let strikes: Vec<f64> = narrow_spread.legs.iter().map(|leg| leg.contract.strike).collect();
        assert!(strikes[0] < strikes[1]);
        assert!(strikes[1] < strikes[2]);
        assert!(strikes[2] < strikes[3]);
        
        // Test with wide spreads
        let wide_spread = OptionStrategy::iron_condor(index.clone(), 21500.0, expiry, 500.0, 1000.0);
        assert_eq!(wide_spread.legs.len(), 4);
        
        let wide_strikes: Vec<f64> = wide_spread.legs.iter().map(|leg| leg.contract.strike).collect();
        assert!(wide_strikes[3] - wide_strikes[0] == 1000.0); // Total spread width
    }

    #[rstest]
    fn test_strategy_with_extreme_spots() {
        let index = IndexOption::BankNifty;
        let expiry = Utc::now() + Duration::days(15);
        
        // Test with very low spot
        let low_spot_strategy = OptionStrategy::iron_condor(index.clone(), 1000.0, expiry, 50.0, 100.0);
        assert_eq!(low_spot_strategy.legs.len(), 4);
        
        // All strikes should be positive
        for leg in &low_spot_strategy.legs {
            assert!(leg.contract.strike > 0.0);
        }
        
        // Test with very high spot
        let high_spot_strategy = OptionStrategy::iron_condor(index.clone(), 100000.0, expiry, 1000.0, 2000.0);
        assert_eq!(high_spot_strategy.legs.len(), 4);
        
        // Strikes should be reasonable relative to spot
        for leg in &high_spot_strategy.legs {
            assert!(leg.contract.strike > 90000.0);
            assert!(leg.contract.strike < 110000.0);
        }
    }

    #[rstest]
    fn test_strategy_different_expiries() {
        let index = IndexOption::FinNifty;
        let spot = 20000.0;
        
        // Test with different expiry dates
        let expiry_dates = vec![
            Utc::now() + Duration::days(1),   // Very short
            Utc::now() + Duration::days(7),   // Weekly
            Utc::now() + Duration::days(30),  // Monthly
            Utc::now() + Duration::days(90),  // Quarterly
        ];
        
        for expiry in expiry_dates {
            let strategy = OptionStrategy::iron_condor(index.clone(), spot, expiry, 100.0, 200.0);
            
            // All legs should have the same expiry
            for leg in &strategy.legs {
                assert_eq!(leg.contract.expiry, expiry);
            }
            
            // Strategy should be valid regardless of expiry
            assert_eq!(strategy.legs.len(), 4);
            assert_eq!(strategy.name, "Iron Condor");
        }
    }
}

#[cfg(test)]
mod option_strategy_performance_tests {
    use super::*;
    use std::time::Instant;

    #[rstest]
    fn test_pnl_calculation_performance() {
        let index = IndexOption::Nifty50;
        let expiry = Utc::now() + Duration::days(30);
        
        let iron_condor = OptionStrategy::iron_condor(index, 21500.0, expiry, 100.0, 200.0);
        
        let start = Instant::now();
        let iterations = 10000;
        
        for i in 0..iterations {
            let test_spot = 20000.0 + i as f64;
            let _pnl = iron_condor.calculate_pnl(test_spot);
        }
        
        let duration = start.elapsed();
        let per_calculation = duration.as_nanos() as f64 / iterations as f64;
        
        // Each P&L calculation should be very fast
        assert!(per_calculation < 10000.0, "P&L calculation too slow: {:.2}ns per calculation", per_calculation);
    }

    #[rstest]
    fn test_aggregate_greeks_performance() {
        let index = IndexOption::Nifty50;
        let expiry = Utc::now() + Duration::days(30);
        
        // Create a complex strategy with many legs
        let mut complex_strategy = OptionStrategy {
            name: "Complex Strategy".to_string(),
            legs: Vec::new(),
            max_profit: None,
            max_loss: None,
            breakeven_points: vec![],
            margin_required: 0.0,
        };
        
        // Add many legs
        for i in 0..20 {
            let strike = 21000.0 + i as f64 * 50.0;
            let option_type = if i % 2 == 0 { OptionType::Call } else { OptionType::Put };
            let quantity = if i % 3 == 0 { 1 } else { -1 };
            
            complex_strategy.legs.push(create_test_option_leg(
                index.clone(),
                option_type,
                strike,
                expiry,
                quantity,
                50.0 + i as f64,
                create_test_greeks(0.5, 0.001, -2.0, 15.0, 5.0),
            ));
        }
        
        let start = Instant::now();
        let iterations = 1000;
        
        for _ in 0..iterations {
            let _greeks = complex_strategy.calculate_aggregate_greeks();
        }
        
        let duration = start.elapsed();
        let per_calculation = duration.as_nanos() as f64 / iterations as f64;
        
        // Greeks calculation should be fast even for complex strategies
        assert!(per_calculation < 100000.0, "Greeks calculation too slow: {:.2}ns per calculation", per_calculation);
    }

    #[rstest]
    fn test_strategy_creation_performance() {
        let index = IndexOption::Nifty50;
        let expiry = Utc::now() + Duration::days(30);
        
        let start = Instant::now();
        let iterations = 1000;
        
        for i in 0..iterations {
            let spot = 21000.0 + i as f64;
            let _strategy = OptionStrategy::iron_condor(index.clone(), spot, expiry, 100.0, 200.0);
        }
        
        let duration = start.elapsed();
        let per_creation = duration.as_nanos() as f64 / iterations as f64;
        
        // Strategy creation should be fast
        assert!(per_creation < 50000.0, "Strategy creation too slow: {:.2}ns per creation", per_creation);
    }
}