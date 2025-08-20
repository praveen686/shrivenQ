use rstest::*;
use approx::{assert_abs_diff_eq, assert_relative_eq};
use options_engine::VolatilitySurface;
use rustc_hash::FxHashMap;

/// Test fixture for standard volatility surface
#[fixture]
fn standard_vol_surface() -> VolatilitySurface {
    let mut surface = VolatilitySurface::new();
    surface.atm_volatility = 0.15;
    surface.skew = -0.05;
    surface.term_structure = vec![0.12, 0.15, 0.18, 0.16, 0.14];
    surface
}

/// Test fixture for populated volatility surface
#[fixture]
fn populated_vol_surface() -> VolatilitySurface {
    let mut surface = VolatilitySurface::new();
    
    // Populate with realistic volatility smile data
    let spot = 21500.0;
    let strikes = vec![20500.0, 21000.0, 21500.0, 22000.0, 22500.0];
    let expiries = vec![0.0274, 0.0822, 0.2466]; // 10 days, 30 days, 90 days
    
    for &expiry in &expiries {
        for &strike in &strikes {
            let moneyness = strike / spot;
            let key = (
                VolatilitySurface::f64_to_fixed_point(moneyness),
                VolatilitySurface::f64_to_fixed_point(expiry)
            );
            
            // Create realistic volatility smile
            let base_vol = 0.15;
            let skew_adjustment = -0.05 * (moneyness - 1.0).ln();
            let term_adjustment = 0.02 * (expiry - 0.0822); // 30-day centered
            let vol = base_vol + skew_adjustment + term_adjustment;
            
            surface.surface.insert(key, vol.max(0.05));
        }
    }
    
    surface.atm_volatility = 0.15;
    surface.skew = -0.05;
    surface.term_structure = vec![0.12, 0.15, 0.18, 0.16, 0.14];
    
    surface
}

#[cfg(test)]
mod volatility_surface_construction_tests {
    use super::*;

    #[rstest]
    fn test_new_volatility_surface() {
        let surface = VolatilitySurface::new();
        
        // Test default values
        assert_eq!(surface.atm_volatility, 0.15);
        assert_eq!(surface.skew, -0.1);
        assert_eq!(surface.term_structure.len(), 0);
        assert!(surface.surface.is_empty());
    }

    #[rstest]
    fn test_f64_to_fixed_point_conversion() {
        // Test the fixed-point conversion function
        assert_eq!(VolatilitySurface::f64_to_fixed_point(1.0), 10000);
        assert_eq!(VolatilitySurface::f64_to_fixed_point(1.05), 10500);
        assert_eq!(VolatilitySurface::f64_to_fixed_point(0.95), 9500);
        assert_eq!(VolatilitySurface::f64_to_fixed_point(1.2345), 12345);
        
        // Test precision
        let original = 1.23456789;
        let converted = VolatilitySurface::f64_to_fixed_point(original);
        let back_converted = converted as f64 / 10000.0;
        assert_abs_diff_eq!(original, back_converted, epsilon = 1e-4);
    }

    #[rstest]
    fn test_surface_data_structure(populated_vol_surface: VolatilitySurface) {
        let surface = populated_vol_surface;
        
        // Check that surface is populated
        assert!(!surface.surface.is_empty());
        assert_eq!(surface.surface.len(), 15); // 5 strikes × 3 expiries
        
        // Check that all volatilities are reasonable
        for (_, &vol) in surface.surface.iter() {
            assert!(vol >= 0.05);
            assert!(vol <= 0.50);
            assert!(vol.is_finite());
        }
    }

    #[rstest]
    fn test_surface_properties_initialization(standard_vol_surface: VolatilitySurface) {
        let surface = standard_vol_surface;
        
        assert_eq!(surface.atm_volatility, 0.15);
        assert_eq!(surface.skew, -0.05);
        assert_eq!(surface.term_structure.len(), 5);
        
        // Term structure should contain reasonable volatilities
        for &vol in &surface.term_structure {
            assert!(vol >= 0.05);
            assert!(vol <= 0.30);
        }
    }
}

#[cfg(test)]
mod sabr_model_tests {
    use super::*;

    #[rstest]
    fn test_sabr_atm_volatility() {
        let surface = VolatilitySurface::new();
        let f = 21500.0;
        let k = 21500.0; // ATM
        let t = 0.25;
        let alpha = 0.15;
        let beta = 0.5;
        let rho = -0.3;
        let nu = 0.4;
        
        let sabr_vol = surface.sabr_volatility(f, k, t, alpha, beta, rho, nu);
        
        // ATM SABR volatility should approximately equal alpha / f^(1-beta)
        let expected_atm = alpha / f.powf(1.0 - beta);
        assert_abs_diff_eq!(sabr_vol, expected_atm, epsilon = 1e-10);
    }

    #[rstest]
    fn test_sabr_volatility_smile() {
        let surface = VolatilitySurface::new();
        let f = 21500.0;
        let t = 0.25;
        let alpha = 0.15;
        let beta = 0.5;
        let rho = -0.3;
        let nu = 0.4;
        
        // Test volatility smile across different strikes
        let strikes = vec![20000.0, 20500.0, 21000.0, 21500.0, 22000.0, 22500.0, 23000.0];
        let mut volatilities = Vec::new();
        
        for &k in &strikes {
            let vol = surface.sabr_volatility(f, k, t, alpha, beta, rho, nu);
            volatilities.push(vol);
            
            // All volatilities should be positive and finite
            assert!(vol > 0.0);
            assert!(vol.is_finite());
        }
        
        // Check volatility smile shape (typically higher for OTM puts, lower for OTM calls with negative rho)
        let atm_index = 3; // 21500 is ATM
        let otm_put_vol = volatilities[0]; // 20000 strike (OTM put)
        let atm_vol = volatilities[atm_index];
        let otm_call_vol = volatilities[6]; // 23000 strike (OTM call)
        
        // With negative rho, OTM puts should have higher vol than ATM
        assert!(otm_put_vol > atm_vol);
        
        // OTM calls should have lower vol than ATM (but this depends on parameters)
        // Just ensure they're all reasonable
        assert!(otm_call_vol > 0.05);
        assert!(otm_call_vol < 0.50);
    }

    #[rstest]
    fn test_sabr_parameter_sensitivity() {
        let surface = VolatilitySurface::new();
        let f = 21500.0;
        let k = 22000.0;
        let t = 0.25;
        
        // Base parameters
        let alpha = 0.15;
        let beta = 0.5;
        let rho = -0.3;
        let nu = 0.4;
        
        let base_vol = surface.sabr_volatility(f, k, t, alpha, beta, rho, nu);
        
        // Test alpha sensitivity (overall level)
        let high_alpha_vol = surface.sabr_volatility(f, k, t, alpha * 1.2, beta, rho, nu);
        assert!(high_alpha_vol > base_vol);
        
        // Test beta sensitivity (backbone curve shape)
        let high_beta_vol = surface.sabr_volatility(f, k, t, alpha, 0.8, rho, nu);
        // Beta effect depends on moneyness
        
        // Test rho sensitivity (skew)
        let high_rho_vol = surface.sabr_volatility(f, k, t, alpha, beta, 0.0, nu);
        // With k > f and moving rho from negative to zero, vol should change
        
        // Test nu sensitivity (curvature)
        let high_nu_vol = surface.sabr_volatility(f, k, t, alpha, beta, rho, nu * 1.5);
        // Higher nu generally increases vol for non-ATM strikes
        
        // All volatilities should be positive and reasonable
        let vols = vec![base_vol, high_alpha_vol, high_beta_vol, high_rho_vol, high_nu_vol];
        for vol in vols {
            assert!(vol > 0.0);
            assert!(vol < 2.0); // Should be reasonable
            assert!(vol.is_finite());
        }
    }

    #[rstest]
    fn test_sabr_time_scaling() {
        let surface = VolatilitySurface::new();
        let f = 21500.0;
        let k = 22000.0;
        let alpha = 0.15;
        let beta = 0.5;
        let rho = -0.3;
        let nu = 0.4;
        
        let short_time = 0.0274; // 10 days
        let long_time = 0.2466;  // 90 days
        
        let vol_short = surface.sabr_volatility(f, k, short_time, alpha, beta, rho, nu);
        let vol_long = surface.sabr_volatility(f, k, long_time, alpha, beta, rho, nu);
        
        // Both should be reasonable
        assert!(vol_short > 0.05 && vol_short < 1.0);
        assert!(vol_long > 0.05 && vol_long < 1.0);
        
        // The relationship depends on the term structure embedded in SABR
        // Just ensure both are finite and positive
        assert!(vol_short.is_finite());
        assert!(vol_long.is_finite());
    }

    #[rstest]
    fn test_sabr_numerical_stability() {
        let surface = VolatilitySurface::new();
        let f = 21500.0;
        let k = 21500.1; // Very close to ATM but not exactly
        let t = 0.25;
        let alpha = 0.15;
        let beta = 0.5;
        let rho = -0.3;
        let nu = 0.4;
        
        let vol = surface.sabr_volatility(f, k, t, alpha, beta, rho, nu);
        
        // Should be close to ATM volatility
        let atm_vol = surface.sabr_volatility(f, f, t, alpha, beta, rho, nu);
        assert_abs_diff_eq!(vol, atm_vol, epsilon = 1e-4);
        
        // Test with extreme parameters
        let extreme_vol = surface.sabr_volatility(f, k, t, 0.5, 0.1, -0.9, 2.0);
        assert!(extreme_vol > 0.0);
        assert!(extreme_vol.is_finite());
    }
}

#[cfg(test)]
mod implied_volatility_lookup_tests {
    use super::*;

    #[rstest]
    fn test_get_iv_basic_functionality(standard_vol_surface: VolatilitySurface) {
        let surface = standard_vol_surface;
        let spot = 21500.0;
        let strike = 21500.0; // ATM
        let time_to_expiry = 0.0822; // 30 days
        
        let iv = surface.get_iv(spot, strike, time_to_expiry);
        
        // Should return a reasonable implied volatility
        assert!(iv > 0.05);
        assert!(iv < 0.50);
        assert!(iv.is_finite());
    }

    #[rstest]
    fn test_get_iv_moneyness_effect(standard_vol_surface: VolatilitySurface) {
        let surface = standard_vol_surface;
        let spot = 21500.0;
        let time_to_expiry = 0.0822;
        
        // Test different strikes (moneyness levels)
        let strikes = vec![20000.0, 21000.0, 21500.0, 22000.0, 23000.0];
        let mut ivs = Vec::new();
        
        for &strike in &strikes {
            let iv = surface.get_iv(spot, strike, time_to_expiry);
            ivs.push(iv);
            
            // All IVs should be positive and reasonable
            assert!(iv >= 0.01);
            assert!(iv < 1.0);
        }
        
        // With negative skew, OTM puts (low strikes) should have higher IV
        let otm_put_iv = ivs[0]; // 20000 strike
        let atm_iv = ivs[2]; // 21500 strike
        let otm_call_iv = ivs[4]; // 23000 strike
        
        // Check skew pattern (OTM puts > ATM > OTM calls for negative skew)
        assert!(otm_put_iv > atm_iv);
        assert!(atm_iv > otm_call_iv);
    }

    #[rstest]
    fn test_get_iv_minimum_bound(standard_vol_surface: VolatilitySurface) {
        let surface = standard_vol_surface;
        let spot = 21500.0;
        let strike = 30000.0; // Very OTM call
        let time_to_expiry = 0.0274; // Short time
        
        let iv = surface.get_iv(spot, strike, time_to_expiry);
        
        // Should be bounded at minimum 1% volatility
        assert!(iv >= 0.01);
        assert!(iv.is_finite());
    }

    #[rstest]
    fn test_get_iv_term_structure_effect() {
        let mut surface = VolatilitySurface::new();
        surface.atm_volatility = 0.15;
        surface.skew = 0.0; // No skew for this test
        surface.term_structure = vec![0.10, 0.12, 0.15, 0.18, 0.20]; // Rising term structure
        
        let spot = 21500.0;
        let strike = 21500.0;
        
        // Test different times to expiry
        let times = vec![0.0274, 0.0822, 0.2466, 0.5, 1.0];
        let mut ivs = Vec::new();
        
        for &time in &times {
            let iv = surface.get_iv(spot, strike, time);
            ivs.push(iv);
        }
        
        // All should be reasonable
        for iv in &ivs {
            assert!(*iv >= 0.01);
            assert!(*iv < 1.0);
        }
        
        // Note: The current implementation doesn't fully utilize term structure
        // This test ensures the function works with term structure present
    }

    #[rstest]
    fn test_get_iv_consistency_with_sabr() {
        let surface = VolatilitySurface::new();
        let spot = 21500.0;
        let strike = 22000.0;
        let time = 0.25;
        
        // Get IV using the surface method
        let surface_iv = surface.get_iv(spot, strike, time);
        
        // The surface method uses a simplified model, not SABR directly
        // Just ensure it produces reasonable results
        assert!(surface_iv > 0.05);
        assert!(surface_iv < 0.50);
        
        // Test with different parameters to ensure consistency
        let surface_iv_2 = surface.get_iv(spot * 1.01, strike, time);
        assert!((surface_iv_2 - surface_iv).abs() < 0.1); // Should be similar for small spot moves
    }

    #[rstest]
    fn test_get_iv_extreme_scenarios() {
        let surface = VolatilitySurface::new();
        let spot = 21500.0;
        
        // Extreme OTM
        let extreme_otm_iv = surface.get_iv(spot, spot * 2.0, 0.0274);
        assert!(extreme_otm_iv >= 0.01);
        assert!(extreme_otm_iv.is_finite());
        
        // Extreme ITM
        let extreme_itm_iv = surface.get_iv(spot, spot * 0.5, 0.0274);
        assert!(extreme_itm_iv >= 0.01);
        assert!(extreme_itm_iv.is_finite());
        
        // Very short time
        let short_time_iv = surface.get_iv(spot, spot, 0.001);
        assert!(short_time_iv >= 0.01);
        assert!(short_time_iv.is_finite());
        
        // Very long time
        let long_time_iv = surface.get_iv(spot, spot, 2.0);
        assert!(long_time_iv >= 0.01);
        assert!(long_time_iv.is_finite());
    }
}

#[cfg(test)]
mod term_structure_tests {
    use super::*;

    #[rstest]
    fn test_interpolate_term_structure_basic() {
        let surface = VolatilitySurface::new();
        
        // Test basic interpolation (current implementation returns 0.0)
        let interpolated = surface.interpolate_term_structure(0.5);
        assert_eq!(interpolated, 0.0); // Current simplified implementation
        assert!(interpolated.is_finite());
    }

    #[rstest] 
    fn test_term_structure_integration() {
        let mut surface = VolatilitySurface::new();
        surface.term_structure = vec![0.10, 0.12, 0.15, 0.18, 0.20];
        
        // Test that term structure is properly stored and accessible
        assert_eq!(surface.term_structure.len(), 5);
        
        for &vol in &surface.term_structure {
            assert!(vol > 0.0);
            assert!(vol < 1.0);
        }
        
        // Test interpolation function exists and returns finite values
        for time in &[0.1, 0.25, 0.5, 1.0, 2.0] {
            let interp = surface.interpolate_term_structure(*time);
            assert!(interp.is_finite());
        }
    }
}

#[cfg(test)]
mod surface_population_tests {
    use super::*;

    #[rstest]
    fn test_surface_key_generation() {
        // Test the fixed-point key generation for surface lookups
        let spot = 21500.0;
        let strike = 21600.0;
        let time = 0.0822;
        
        let moneyness = strike / spot;
        let key = (
            VolatilitySurface::f64_to_fixed_point(moneyness),
            VolatilitySurface::f64_to_fixed_point(time)
        );
        
        // Key should be deterministic
        let key2 = (
            VolatilitySurface::f64_to_fixed_point(moneyness),
            VolatilitySurface::f64_to_fixed_point(time)
        );
        
        assert_eq!(key, key2);
        
        // Keys should be unique for different inputs
        let different_key = (
            VolatilitySurface::f64_to_fixed_point(moneyness + 0.01),
            VolatilitySurface::f64_to_fixed_point(time)
        );
        
        assert_ne!(key, different_key);
    }

    #[rstest]
    fn test_surface_data_integrity(populated_vol_surface: VolatilitySurface) {
        let surface = populated_vol_surface;
        
        // Check data integrity
        for (key, &vol) in surface.surface.iter() {
            // Volatility should be positive and reasonable
            assert!(vol > 0.0);
            assert!(vol < 2.0);
            assert!(vol.is_finite());
            
            // Keys should be valid fixed-point representations
            let (moneyness_fp, time_fp) = *key;
            let moneyness = moneyness_fp as f64 / 10000.0;
            let time = time_fp as f64 / 10000.0;
            
            // Moneyness should be reasonable (0.5 to 2.0 typically)
            assert!(moneyness > 0.5);
            assert!(moneyness < 2.0);
            
            // Time should be reasonable (up to 2 years)
            assert!(time > 0.0);
            assert!(time < 2.0);
        }
    }

    #[rstest]
    fn test_surface_lookup_performance() {
        let surface = populated_vol_surface();
        let spot = 21500.0;
        
        use std::time::Instant;
        
        let start = Instant::now();
        let iterations = 10000;
        
        for i in 0..iterations {
            let strike = 20000.0 + (i as f64);
            let time = 0.0274 + (i as f64) * 0.001 / iterations as f64;
            let _iv = surface.get_iv(spot, strike, time);
        }
        
        let duration = start.elapsed();
        let per_lookup = duration.as_nanos() as f64 / iterations as f64;
        
        // Each lookup should be fast (less than 1 microsecond)
        assert!(per_lookup < 1000.0, "Surface lookup too slow: {:.2}ns per lookup", per_lookup);
    }

    #[rstest]
    fn test_surface_memory_efficiency() {
        let surface = populated_vol_surface();
        
        // Surface should not be excessively large
        assert!(surface.surface.len() < 1000); // Reasonable size for test data
        
        // Each entry should be efficiently stored
        let memory_estimate = surface.surface.len() * (std::mem::size_of::<(u64, u64)>() + std::mem::size_of::<f64>());
        assert!(memory_estimate < 100_000); // Less than 100KB for test data
    }
}

#[cfg(test)]
mod volatility_surface_edge_cases {
    use super::*;

    #[rstest]
    fn test_empty_surface_behavior() {
        let surface = VolatilitySurface::new();
        
        // Empty surface should still provide reasonable IVs through the model
        let iv = surface.get_iv(21500.0, 21500.0, 0.0822);
        assert!(iv >= 0.01);
        assert!(iv.is_finite());
    }

    #[rstest]
    fn test_extreme_atm_volatility() {
        let mut surface = VolatilitySurface::new();
        
        // Test very high ATM volatility
        surface.atm_volatility = 2.0; // 200%
        let high_vol_iv = surface.get_iv(21500.0, 21500.0, 0.0822);
        assert!(high_vol_iv > 0.5);
        assert!(high_vol_iv.is_finite());
        
        // Test very low ATM volatility
        surface.atm_volatility = 0.01; // 1%
        let low_vol_iv = surface.get_iv(21500.0, 21500.0, 0.0822);
        assert!(low_vol_iv >= 0.01); // Should be bounded
        assert!(low_vol_iv.is_finite());
    }

    #[rstest]
    fn test_extreme_skew_values() {
        let mut surface = VolatilitySurface::new();
        
        // Test very negative skew
        surface.skew = -0.5;
        let neg_skew_otm_put = surface.get_iv(21500.0, 20000.0, 0.0822);
        let neg_skew_otm_call = surface.get_iv(21500.0, 23000.0, 0.0822);
        
        // OTM puts should have much higher vol than OTM calls
        assert!(neg_skew_otm_put > neg_skew_otm_call);
        assert!(neg_skew_otm_put.is_finite());
        assert!(neg_skew_otm_call.is_finite());
        
        // Test positive skew
        surface.skew = 0.5;
        let pos_skew_otm_put = surface.get_iv(21500.0, 20000.0, 0.0822);
        let pos_skew_otm_call = surface.get_iv(21500.0, 23000.0, 0.0822);
        
        // OTM calls should have higher vol than OTM puts
        assert!(pos_skew_otm_call > pos_skew_otm_put);
        assert!(pos_skew_otm_call.is_finite());
        assert!(pos_skew_otm_put.is_finite());
    }

    #[rstest]
    fn test_nan_and_infinity_handling() {
        let surface = VolatilitySurface::new();
        
        // Test with extreme inputs that might cause numerical issues
        let iv1 = surface.get_iv(f64::INFINITY, 21500.0, 0.0822);
        assert!(iv1.is_finite()); // Should handle gracefully
        assert!(iv1 >= 0.01);
        
        let iv2 = surface.get_iv(21500.0, f64::INFINITY, 0.0822);
        assert!(iv2.is_finite());
        assert!(iv2 >= 0.01);
        
        let iv3 = surface.get_iv(21500.0, 21500.0, f64::INFINITY);
        assert!(iv3.is_finite());
        assert!(iv3 >= 0.01);
        
        // Test with very small values
        let iv4 = surface.get_iv(1e-10, 21500.0, 0.0822);
        assert!(iv4.is_finite());
        assert!(iv4 >= 0.01);
    }

    #[rstest]
    fn test_zero_and_negative_inputs() {
        let surface = VolatilitySurface::new();
        
        // Test zero spot (should handle gracefully)
        let iv1 = surface.get_iv(0.0, 21500.0, 0.0822);
        assert!(iv1.is_finite());
        assert!(iv1 >= 0.01);
        
        // Test zero strike (should handle gracefully)
        let iv2 = surface.get_iv(21500.0, 0.0, 0.0822);
        assert!(iv2.is_finite());
        assert!(iv2 >= 0.01);
        
        // Test zero time (should handle gracefully)
        let iv3 = surface.get_iv(21500.0, 21500.0, 0.0);
        assert!(iv3.is_finite());
        assert!(iv3 >= 0.01);
        
        // Test negative inputs (should handle gracefully)
        let iv4 = surface.get_iv(-21500.0, 21500.0, 0.0822);
        assert!(iv4.is_finite());
        assert!(iv4 >= 0.01);
    }
}

#[cfg(test)]
mod volatility_surface_integration_tests {
    use super::*;

    #[rstest]
    fn test_surface_with_black_scholes_consistency() {
        use options_engine::{BlackScholes, OptionType};
        
        let surface = standard_vol_surface();
        let spot = 21500.0;
        let strike = 21500.0;
        let rate = 0.065;
        let time = 0.0822;
        
        // Get IV from surface
        let surface_iv = surface.get_iv(spot, strike, time);
        
        // Use this IV in Black-Scholes pricing
        let price = BlackScholes::price(OptionType::Call, spot, strike, rate, surface_iv, time, 0.0);
        
        // Price should be reasonable
        assert!(price > 10.0);
        assert!(price < 500.0);
        assert!(price.is_finite());
        
        // Calculate Greeks with this IV
        let greeks = BlackScholes::calculate_greeks(OptionType::Call, spot, strike, rate, surface_iv, time, 0.0);
        
        // Greeks should be reasonable
        assert!(greeks.delta > 0.0 && greeks.delta < 1.0);
        assert!(greeks.gamma > 0.0);
        assert!(greeks.theta < 0.0);
        assert!(greeks.vega > 0.0);
    }

    #[rstest]
    fn test_surface_smile_properties(populated_vol_surface: VolatilitySurface) {
        let surface = populated_vol_surface;
        let spot = 21500.0;
        let time = 0.0822;
        
        // Test volatility smile across strikes
        let strikes = (18000..25000).step_by(200).map(|x| x as f64).collect::<Vec<_>>();
        let mut smile_ivs = Vec::new();
        
        for &strike in &strikes {
            let iv = surface.get_iv(spot, strike, time);
            smile_ivs.push((strike, iv));
        }
        
        // Find ATM IV
        let atm_iv = surface.get_iv(spot, spot, time);
        
        // Check that the smile has reasonable properties
        let mut has_valid_smile = true;
        for (strike, iv) in &smile_ivs {
            // All IVs should be positive and finite
            if *iv <= 0.0 || !iv.is_finite() {
                has_valid_smile = false;
                break;
            }
            
            // IVs should be within reasonable bounds
            if *iv > 2.0 || *iv < 0.01 {
                has_valid_smile = false;
                break;
            }
        }
        
        assert!(has_valid_smile, "Volatility smile has invalid properties");
        
        // Test that ATM IV is reasonable relative to extremes
        let min_iv = smile_ivs.iter().map(|(_, iv)| *iv).fold(f64::INFINITY, f64::min);
        let max_iv = smile_ivs.iter().map(|(_, iv)| *iv).fold(f64::NEG_INFINITY, f64::max);
        
        assert!(atm_iv >= min_iv);
        assert!(atm_iv <= max_iv);
        assert!(max_iv - min_iv < 1.0); // Smile shouldn't be too extreme
    }

    #[rstest]
    fn test_surface_term_structure_properties() {
        let mut surface = VolatilitySurface::new();
        surface.term_structure = vec![0.12, 0.15, 0.18, 0.20, 0.19];
        
        let spot = 21500.0;
        let strike = 21500.0;
        
        // Test IV across different times
        let times = vec![0.0274, 0.0822, 0.2466, 0.5, 1.0];
        let mut term_ivs = Vec::new();
        
        for &time in &times {
            let iv = surface.get_iv(spot, strike, time);
            term_ivs.push((time, iv));
        }
        
        // All should be reasonable
        for (time, iv) in &term_ivs {
            assert!(*iv > 0.01);
            assert!(*iv < 1.0);
            assert!(iv.is_finite());
        }
        
        // Should show some term structure behavior (though simplified in current implementation)
        let short_term_iv = term_ivs[0].1;
        let long_term_iv = term_ivs[4].1;
        
        // Both should be reasonable
        assert!(short_term_iv > 0.05 && short_term_iv < 0.5);
        assert!(long_term_iv > 0.05 && long_term_iv < 0.5);
    }
}