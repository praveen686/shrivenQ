use rstest::*;
use approx::{assert_abs_diff_eq, assert_relative_eq};
use tokio;
use options_engine::grpc_service::OptionsEngineService;
use options_engine::pb::{
    PricingRequest, StrategyRequest, OptionChainRequest, StreamGreeksRequest,
    OptionsEngineServer
};
use tonic::{Request, Response, Status};
use std::sync::Arc;

/// Test fixture for gRPC service
#[fixture]
fn grpc_service() -> OptionsEngineService {
    OptionsEngineService::new()
}

/// Test fixture for standard pricing request
#[fixture]
fn standard_pricing_request() -> PricingRequest {
    PricingRequest {
        option_type: 0, // Call
        spot: 21500.0,
        strike: 21500.0,
        rate: 0.065,
        volatility: 0.15,
        time_to_expiry: 30.0 / 365.0,
    }
}

/// Test fixture for Nifty pricing request
#[fixture]
fn nifty_pricing_request() -> PricingRequest {
    PricingRequest {
        option_type: 0, // Call
        spot: 21500.0,
        strike: 21600.0,
        rate: 0.065,
        volatility: 0.18,
        time_to_expiry: 7.0 / 365.0,
    }
}

/// Test fixture for Bank Nifty pricing request
#[fixture]
fn bank_nifty_pricing_request() -> PricingRequest {
    PricingRequest {
        option_type: 1, // Put
        spot: 48000.0,
        strike: 47500.0,
        rate: 0.065,
        volatility: 0.20,
        time_to_expiry: 14.0 / 365.0,
    }
}

/// Test fixture for strategy request
#[fixture]
fn iron_condor_strategy_request() -> StrategyRequest {
    StrategyRequest {
        strategy_type: "iron_condor".to_string(),
        index: 0, // NIFTY50
        spot: 21500.0,
        legs: vec![], // Will be populated by the service
    }
}

/// Test fixture for option chain request
#[fixture]
fn nifty_option_chain_request() -> OptionChainRequest {
    OptionChainRequest {
        index: 0, // NIFTY50
        expiry: "2024-12-26".to_string(),
    }
}

#[cfg(test)]
mod pricing_service_tests {
    use super::*;
    use options_engine::pb::options_engine_server::OptionsEngine;

    #[rstest]
    #[tokio::test]
    async fn test_calculate_price_basic_functionality(
        grpc_service: OptionsEngineService,
        standard_pricing_request: PricingRequest
    ) {
        let service = grpc_service;
        let request = Request::new(standard_pricing_request);
        
        let response = service.calculate_price(request).await;
        
        assert!(response.is_ok());
        let response = response.unwrap().into_inner();
        
        // Price should be positive and reasonable
        assert!(response.price > 0.0);
        assert!(response.price < 1000.0); // Reasonable upper bound for Nifty options
        assert!(response.price.is_finite());
        
        // Greeks should be present and reasonable
        assert!(response.greeks.is_some());
        let greeks = response.greeks.unwrap();
        
        // Call delta should be between 0 and 1
        assert!(greeks.delta >= 0.0 && greeks.delta <= 1.0);
        
        // Gamma should be positive
        assert!(greeks.gamma > 0.0);
        
        // Theta should be negative (time decay)
        assert!(greeks.theta < 0.0);
        
        // Vega should be positive
        assert!(greeks.vega > 0.0);
        
        // All Greeks should be finite
        assert!(greeks.delta.is_finite());
        assert!(greeks.gamma.is_finite());
        assert!(greeks.theta.is_finite());
        assert!(greeks.vega.is_finite());
        assert!(greeks.rho.is_finite());
        assert!(greeks.lambda.is_finite());
        assert!(greeks.vanna.is_finite());
        assert!(greeks.charm.is_finite());
        
        // Implied volatility should match input
        assert_abs_diff_eq!(response.implied_volatility, 0.15, epsilon = 1e-6);
    }

    #[rstest]
    #[tokio::test]
    async fn test_calculate_price_call_vs_put(
        grpc_service: OptionsEngineService,
        standard_pricing_request: PricingRequest
    ) {
        let service = grpc_service;
        
        // Test call option
        let call_request = Request::new(standard_pricing_request.clone());
        let call_response = service.calculate_price(call_request).await.unwrap().into_inner();
        
        // Test put option
        let mut put_request_data = standard_pricing_request;
        put_request_data.option_type = 1; // Put
        let put_request = Request::new(put_request_data);
        let put_response = service.calculate_price(put_request).await.unwrap().into_inner();
        
        // Both should have valid prices
        assert!(call_response.price > 0.0);
        assert!(put_response.price > 0.0);
        
        // Check put-call parity approximately holds
        // C - P = S - K * e^(-r*T)
        let s = 21500.0;
        let k = 21500.0;
        let r = 0.065;
        let t = 30.0 / 365.0;
        
        let theoretical_diff = s - k * (-r * t).exp();
        let actual_diff = call_response.price - put_response.price;
        
        assert_abs_diff_eq!(actual_diff, theoretical_diff, epsilon = 1.0);
        
        // Check Greeks relationships
        let call_greeks = call_response.greeks.unwrap();
        let put_greeks = put_response.greeks.unwrap();
        
        // Call delta should be positive, put delta negative
        assert!(call_greeks.delta > 0.0);
        assert!(put_greeks.delta < 0.0);
        
        // Delta difference should be approximately 1
        assert_abs_diff_eq!(call_greeks.delta - put_greeks.delta, 1.0, epsilon = 0.1);
        
        // Gamma and Vega should be equal for same strike/expiry
        assert_abs_diff_eq!(call_greeks.gamma, put_greeks.gamma, epsilon = 1e-6);
        assert_abs_diff_eq!(call_greeks.vega, put_greeks.vega, epsilon = 1e-6);
    }

    #[rstest]
    #[tokio::test]
    async fn test_calculate_price_different_strikes(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        let base_request = PricingRequest {
            option_type: 0, // Call
            spot: 21500.0,
            strike: 21500.0,
            rate: 0.065,
            volatility: 0.15,
            time_to_expiry: 30.0 / 365.0,
        };
        
        // Test different strikes
        let strikes = vec![21000.0, 21250.0, 21500.0, 21750.0, 22000.0];
        let mut prices = Vec::new();
        let mut deltas = Vec::new();
        
        for &strike in &strikes {
            let mut request_data = base_request.clone();
            request_data.strike = strike;
            let request = Request::new(request_data);
            
            let response = service.calculate_price(request).await.unwrap().into_inner();
            prices.push(response.price);
            deltas.push(response.greeks.unwrap().delta);
        }
        
        // Prices should decrease as strike increases (for calls)
        for i in 1..prices.len() {
            assert!(prices[i] <= prices[i-1], "Call prices should decrease with higher strikes");
        }
        
        // Deltas should decrease as strike increases (for calls)
        for i in 1..deltas.len() {
            assert!(deltas[i] <= deltas[i-1], "Call deltas should decrease with higher strikes");
        }
        
        // ITM call should have higher delta than OTM call
        assert!(deltas[0] > deltas[4]); // 21000 strike vs 22000 strike
    }

    #[rstest]
    #[tokio::test]
    async fn test_calculate_price_different_volatilities(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        let base_request = PricingRequest {
            option_type: 0, // Call
            spot: 21500.0,
            strike: 21500.0,
            rate: 0.065,
            volatility: 0.15,
            time_to_expiry: 30.0 / 365.0,
        };
        
        // Test different volatilities
        let volatilities = vec![0.05, 0.10, 0.15, 0.25, 0.40];
        let mut prices = Vec::new();
        let mut vegas = Vec::new();
        
        for &vol in &volatilities {
            let mut request_data = base_request.clone();
            request_data.volatility = vol;
            let request = Request::new(request_data);
            
            let response = service.calculate_price(request).await.unwrap().into_inner();
            prices.push(response.price);
            vegas.push(response.greeks.unwrap().vega);
        }
        
        // Prices should increase with volatility
        for i in 1..prices.len() {
            assert!(prices[i] > prices[i-1], "Option prices should increase with volatility");
        }
        
        // Vegas should be positive for all volatilities
        for vega in &vegas {
            assert!(*vega > 0.0, "Vega should be positive");
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_calculate_price_different_times_to_expiry(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        let base_request = PricingRequest {
            option_type: 0, // Call
            spot: 21500.0,
            strike: 21500.0,
            rate: 0.065,
            volatility: 0.15,
            time_to_expiry: 30.0 / 365.0,
        };
        
        // Test different times to expiry
        let times = vec![1.0/365.0, 7.0/365.0, 30.0/365.0, 90.0/365.0, 365.0/365.0];
        let mut prices = Vec::new();
        let mut thetas = Vec::new();
        
        for &time in &times {
            let mut request_data = base_request.clone();
            request_data.time_to_expiry = time;
            let request = Request::new(request_data);
            
            let response = service.calculate_price(request).await.unwrap().into_inner();
            prices.push(response.price);
            thetas.push(response.greeks.unwrap().theta);
        }
        
        // Prices should generally increase with time (for ATM options)
        assert!(prices[4] > prices[0], "Longer-dated options should be more expensive");
        
        // Theta should be negative for all times (time decay)
        for theta in &thetas {
            assert!(*theta < 0.0, "Theta should be negative");
        }
        
        // Theta magnitude should generally increase as expiry approaches
        assert!(thetas[0].abs() > thetas[4].abs(), "Short-term options should have higher theta magnitude");
    }

    #[rstest]
    #[tokio::test]
    async fn test_calculate_price_edge_cases(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        
        // Test with very short time to expiry
        let short_time_request = Request::new(PricingRequest {
            option_type: 0,
            spot: 21500.0,
            strike: 21500.0,
            rate: 0.065,
            volatility: 0.15,
            time_to_expiry: 0.001, // Very short
        });
        
        let response = service.calculate_price(short_time_request).await;
        assert!(response.is_ok());
        let response = response.unwrap().into_inner();
        assert!(response.price >= 0.0);
        assert!(response.price.is_finite());
        
        // Test with very high volatility
        let high_vol_request = Request::new(PricingRequest {
            option_type: 0,
            spot: 21500.0,
            strike: 21500.0,
            rate: 0.065,
            volatility: 2.0, // 200% volatility
            time_to_expiry: 30.0 / 365.0,
        });
        
        let response = service.calculate_price(high_vol_request).await;
        assert!(response.is_ok());
        let response = response.unwrap().into_inner();
        assert!(response.price > 0.0);
        assert!(response.price.is_finite());
        
        // Test with deep ITM option
        let deep_itm_request = Request::new(PricingRequest {
            option_type: 0,
            spot: 22000.0,
            strike: 21000.0, // Deep ITM
            rate: 0.065,
            volatility: 0.15,
            time_to_expiry: 30.0 / 365.0,
        });
        
        let response = service.calculate_price(deep_itm_request).await;
        assert!(response.is_ok());
        let response = response.unwrap().into_inner();
        assert!(response.price > 900.0); // Should be close to intrinsic value
        assert!(response.greeks.unwrap().delta > 0.9); // High delta for deep ITM
    }

    #[rstest]
    #[tokio::test]
    async fn test_calculate_price_invalid_inputs(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        
        // Test with invalid option type
        let invalid_option_type_request = Request::new(PricingRequest {
            option_type: 2, // Invalid (should be 0 or 1)
            spot: 21500.0,
            strike: 21500.0,
            rate: 0.065,
            volatility: 0.15,
            time_to_expiry: 30.0 / 365.0,
        });
        
        let response = service.calculate_price(invalid_option_type_request).await;
        assert!(response.is_err());
        assert_eq!(response.unwrap_err().code(), tonic::Code::InvalidArgument);
        
        // Test with negative spot price
        let negative_spot_request = Request::new(PricingRequest {
            option_type: 0,
            spot: -100.0, // Negative spot
            strike: 21500.0,
            rate: 0.065,
            volatility: 0.15,
            time_to_expiry: 30.0 / 365.0,
        });
        
        // Service should handle this gracefully (may return error or handle internally)
        let response = service.calculate_price(negative_spot_request).await;
        // Implementation may choose to handle negative spots differently
        // Just ensure it doesn't crash
        if response.is_ok() {
            let response = response.unwrap().into_inner();
            assert!(response.price.is_finite());
        }
    }
}

#[cfg(test)]
mod implied_volatility_service_tests {
    use super::*;
    use options_engine::pb::options_engine_server::OptionsEngine;

    #[rstest]
    #[tokio::test]
    async fn test_get_implied_volatility_basic(
        grpc_service: OptionsEngineService,
        standard_pricing_request: PricingRequest
    ) {
        let service = grpc_service;
        let request = Request::new(standard_pricing_request);
        
        let response = service.get_implied_volatility(request).await;
        
        assert!(response.is_ok());
        let response = response.unwrap().into_inner();
        
        // Should return a reasonable implied volatility
        assert!(response.implied_volatility > 0.0);
        assert!(response.implied_volatility < 5.0); // Less than 500%
        assert!(response.implied_volatility.is_finite());
        
        // Price should be 0 (not calculated in IV method)
        assert_eq!(response.price, 0.0);
        
        // Greeks should not be present
        assert!(response.greeks.is_none());
    }

    #[rstest]
    #[tokio::test]
    async fn test_get_implied_volatility_different_scenarios(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        
        // Test scenarios with different option types and moneyness
        let scenarios = vec![
            (0, 21500.0, 21500.0, "ATM Call"),
            (1, 21500.0, 21500.0, "ATM Put"),
            (0, 21500.0, 22000.0, "OTM Call"),
            (1, 21500.0, 21000.0, "OTM Put"),
            (0, 21500.0, 21000.0, "ITM Call"),
            (1, 21500.0, 22000.0, "ITM Put"),
        ];
        
        for (option_type, spot, strike, description) in scenarios {
            let request = Request::new(PricingRequest {
                option_type,
                spot,
                strike,
                rate: 0.065,
                volatility: 0.15, // This is used as input for premium calculation
                time_to_expiry: 30.0 / 365.0,
            });
            
            let response = service.get_implied_volatility(request).await;
            assert!(response.is_ok(), "Failed for scenario: {}", description);
            
            let response = response.unwrap().into_inner();
            assert!(response.implied_volatility > 0.0, "IV should be positive for {}", description);
            assert!(response.implied_volatility < 3.0, "IV should be reasonable for {}", description);
            assert!(response.implied_volatility.is_finite(), "IV should be finite for {}", description);
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_get_implied_volatility_extreme_cases(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        
        // Test with very short expiry
        let short_expiry_request = Request::new(PricingRequest {
            option_type: 0,
            spot: 21500.0,
            strike: 21500.0,
            rate: 0.065,
            volatility: 0.15,
            time_to_expiry: 1.0 / 365.0, // 1 day
        });
        
        let response = service.get_implied_volatility(short_expiry_request).await;
        assert!(response.is_ok());
        let response = response.unwrap().into_inner();
        assert!(response.implied_volatility > 0.0);
        assert!(response.implied_volatility.is_finite());
        
        // Test with deep ITM option
        let deep_itm_request = Request::new(PricingRequest {
            option_type: 0,
            spot: 22000.0,
            strike: 21000.0, // Deep ITM
            rate: 0.065,
            volatility: 0.15,
            time_to_expiry: 30.0 / 365.0,
        });
        
        let response = service.get_implied_volatility(deep_itm_request).await;
        assert!(response.is_ok());
        let response = response.unwrap().into_inner();
        assert!(response.implied_volatility > 0.0);
        assert!(response.implied_volatility.is_finite());
    }
}

#[cfg(test)]
mod strategy_analysis_tests {
    use super::*;
    use options_engine::pb::options_engine_server::OptionsEngine;

    #[rstest]
    #[tokio::test]
    async fn test_analyze_strategy_iron_condor(
        grpc_service: OptionsEngineService,
        iron_condor_strategy_request: StrategyRequest
    ) {
        let service = grpc_service;
        let request = Request::new(iron_condor_strategy_request);
        
        let response = service.analyze_strategy(request).await;
        
        assert!(response.is_ok());
        let response = response.unwrap().into_inner();
        
        // Check strategy name
        assert_eq!(response.strategy_name, "iron_condor");
        
        // Check max profit and loss
        assert!(response.max_profit > 0.0);
        assert!(response.max_loss < 0.0); // Loss should be negative
        assert!(response.max_profit.is_finite());
        assert!(response.max_loss.is_finite());
        
        // Check breakeven points
        assert_eq!(response.breakeven_points.len(), 2); // Iron condor has 2 breakevens
        assert!(response.breakeven_points[0] < response.breakeven_points[1]);
        
        // Both breakevens should be around the spot
        let spot = 21500.0;
        for &breakeven in &response.breakeven_points {
            assert!(breakeven > spot - 500.0);
            assert!(breakeven < spot + 500.0);
        }
        
        // Check margin required
        assert!(response.margin_required > 0.0);
        assert!(response.margin_required.is_finite());
        
        // Check aggregate Greeks
        assert!(response.aggregate_greeks.is_some());
        let greeks = response.aggregate_greeks.unwrap();
        
        // Iron condor should be approximately delta neutral
        assert!(greeks.delta.abs() < 5.0, "Iron condor should be approximately delta neutral");
        
        // All Greeks should be finite
        assert!(greeks.delta.is_finite());
        assert!(greeks.gamma.is_finite());
        assert!(greeks.theta.is_finite());
        assert!(greeks.vega.is_finite());
        assert!(greeks.rho.is_finite());
    }

    #[rstest]
    #[tokio::test]
    async fn test_analyze_strategy_different_types(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        
        // Test different strategy types
        let strategy_types = vec![
            "iron_condor",
            "butterfly",
            "straddle",
            "strangle",
            "bull_call_spread",
        ];
        
        for strategy_type in strategy_types {
            let request = Request::new(StrategyRequest {
                strategy_type: strategy_type.to_string(),
                index: 0, // NIFTY50
                spot: 21500.0,
                legs: vec![], // Will be populated by the service
            });
            
            let response = service.analyze_strategy(request).await;
            assert!(response.is_ok(), "Failed for strategy type: {}", strategy_type);
            
            let response = response.unwrap().into_inner();
            assert_eq!(response.strategy_name, strategy_type);
            assert!(response.max_profit.is_finite());
            assert!(response.max_loss.is_finite());
            assert!(response.margin_required >= 0.0);
            
            // Breakeven points should be reasonable
            for &breakeven in &response.breakeven_points {
                assert!(breakeven > 0.0);
                assert!(breakeven < 50000.0); // Reasonable upper bound
            }
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_analyze_strategy_different_indices(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        
        // Test different indices
        let indices = vec![
            (0, 21500.0, "NIFTY50"),
            (1, 48000.0, "BANKNIFTY"),
            (2, 20000.0, "FINNIFTY"),
            (3, 10000.0, "MIDCAPNIFTY"),
        ];
        
        for (index, spot, name) in indices {
            let request = Request::new(StrategyRequest {
                strategy_type: "iron_condor".to_string(),
                index,
                spot,
                legs: vec![],
            });
            
            let response = service.analyze_strategy(request).await;
            assert!(response.is_ok(), "Failed for index: {}", name);
            
            let response = response.unwrap().into_inner();
            
            // Breakeven points should be reasonable relative to spot
            for &breakeven in &response.breakeven_points {
                assert!(breakeven > spot * 0.8, "Breakeven too low for {}", name);
                assert!(breakeven < spot * 1.2, "Breakeven too high for {}", name);
            }
            
            // Margin should be reasonable relative to spot
            assert!(response.margin_required > spot * 0.01, "Margin too low for {}", name);
            assert!(response.margin_required < spot * 10.0, "Margin too high for {}", name);
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_analyze_strategy_with_legs(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        
        // Create a strategy request with specific legs
        use options_engine::pb::{OptionContract, Greeks as PbGreeks};
        
        let legs = vec![
            OptionContract {
                index: 0, // NIFTY50
                option_type: 0, // Call
                strike: 21600.0,
                expiry: "2024-12-26".to_string(),
                lot_size: 50,
                premium: 100.0,
                open_interest: 10000,
                volume: 1000,
                implied_volatility: 0.15,
                greeks: Some(PbGreeks {
                    delta: 0.4,
                    gamma: 0.002,
                    theta: -5.0,
                    vega: 20.0,
                    rho: 8.0,
                    lambda: 4.0,
                    vanna: 0.1,
                    charm: -0.01,
                }),
            }
        ];
        
        let request = Request::new(StrategyRequest {
            strategy_type: "custom".to_string(),
            index: 0,
            spot: 21500.0,
            legs,
        });
        
        let response = service.analyze_strategy(request).await;
        assert!(response.is_ok());
        
        let response = response.unwrap().into_inner();
        assert_eq!(response.strategy_name, "custom");
        
        // Should have reasonable values even with custom legs
        assert!(response.margin_required >= 0.0);
        assert!(response.max_profit.is_finite());
        assert!(response.max_loss.is_finite());
    }
}

#[cfg(test)]
mod option_chain_tests {
    use super::*;
    use options_engine::pb::options_engine_server::OptionsEngine;

    #[rstest]
    #[tokio::test]
    async fn test_get_option_chain_basic(
        grpc_service: OptionsEngineService,
        nifty_option_chain_request: OptionChainRequest
    ) {
        let service = grpc_service;
        let request = Request::new(nifty_option_chain_request);
        
        let response = service.get_option_chain(request).await;
        
        assert!(response.is_ok());
        let response = response.unwrap().into_inner();
        
        // Check basic structure
        assert!(response.spot_price > 0.0);
        assert!(response.spot_price.is_finite());
        
        // Timestamp should be valid
        assert!(!response.timestamp.is_empty());
        
        // Options array (may be empty in test implementation)
        // Just ensure it's present
        assert!(response.options.len() >= 0);
        
        // Spot price should be reasonable for Nifty
        assert!(response.spot_price > 10000.0);
        assert!(response.spot_price < 50000.0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_get_option_chain_different_indices(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        
        // Test different indices
        let indices = vec![
            (0, "NIFTY50"),
            (1, "BANKNIFTY"),
            (2, "FINNIFTY"),
            (3, "MIDCAPNIFTY"),
        ];
        
        for (index, name) in indices {
            let request = Request::new(OptionChainRequest {
                index,
                expiry: "2024-12-26".to_string(),
            });
            
            let response = service.get_option_chain(request).await;
            assert!(response.is_ok(), "Failed for index: {}", name);
            
            let response = response.unwrap().into_inner();
            assert!(response.spot_price > 0.0, "Spot price should be positive for {}", name);
            assert!(!response.timestamp.is_empty(), "Timestamp should be present for {}", name);
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_get_option_chain_different_expiries(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        
        // Test different expiry dates
        let expiries = vec![
            "2024-12-19",
            "2024-12-26",
            "2025-01-02",
            "2025-01-30",
        ];
        
        for expiry in expiries {
            let request = Request::new(OptionChainRequest {
                index: 0, // NIFTY50
                expiry: expiry.to_string(),
            });
            
            let response = service.get_option_chain(request).await;
            assert!(response.is_ok(), "Failed for expiry: {}", expiry);
            
            let response = response.unwrap().into_inner();
            assert!(response.spot_price > 0.0);
            assert!(!response.timestamp.is_empty());
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_get_option_chain_response_structure(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        
        let request = Request::new(OptionChainRequest {
            index: 0, // NIFTY50
            expiry: "2024-12-26".to_string(),
        });
        
        let response = service.get_option_chain(request).await;
        assert!(response.is_ok());
        
        let response = response.unwrap().into_inner();
        
        // Check that all returned options (if any) have valid structure
        for option in &response.options {
            assert!(option.strike > 0.0);
            assert!(option.lot_size > 0);
            assert!(option.premium >= 0.0);
            assert!(!option.expiry.is_empty());
            
            // Option type should be valid (0 or 1)
            assert!(option.option_type <= 1);
            
            // Index should match request
            assert_eq!(option.index, 0);
            
            // Greeks should be present and finite if provided
            if let Some(ref greeks) = option.greeks {
                assert!(greeks.delta.is_finite());
                assert!(greeks.gamma.is_finite());
                assert!(greeks.theta.is_finite());
                assert!(greeks.vega.is_finite());
                assert!(greeks.rho.is_finite());
            }
        }
        
        // Timestamp should be a valid date string
        assert!(response.timestamp.contains("T") || response.timestamp.contains(":"));
    }
}

#[cfg(test)]
mod streaming_tests {
    use super::*;
    use options_engine::pb::options_engine_server::OptionsEngine;

    #[rstest]
    #[tokio::test]
    async fn test_stream_greeks_unimplemented(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        
        let request = Request::new(StreamGreeksRequest {
            contracts: vec![],
        });
        
        let response = service.stream_greeks(request).await;
        
        // Should return unimplemented error
        assert!(response.is_err());
        assert_eq!(response.unwrap_err().code(), tonic::Code::Unimplemented);
    }

    // Note: When streaming is implemented, add tests for:
    // - Stream establishment
    // - Real-time Greeks updates
    // - Stream disconnection handling
    // - Multiple concurrent streams
    // - Stream performance under load
}

#[cfg(test)]
mod service_integration_tests {
    use super::*;
    use options_engine::pb::options_engine_server::OptionsEngine;
    use std::time::Instant;

    #[rstest]
    #[tokio::test]
    async fn test_pricing_consistency_across_calls(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        
        let pricing_request = PricingRequest {
            option_type: 0,
            spot: 21500.0,
            strike: 21500.0,
            rate: 0.065,
            volatility: 0.15,
            time_to_expiry: 30.0 / 365.0,
        };
        
        // Make multiple calls with same parameters
        let mut prices = Vec::new();
        for _ in 0..5 {
            let request = Request::new(pricing_request.clone());
            let response = service.calculate_price(request).await.unwrap().into_inner();
            prices.push(response.price);
        }
        
        // All prices should be identical (deterministic pricing)
        for i in 1..prices.len() {
            assert_abs_diff_eq!(prices[i], prices[0], epsilon = 1e-10);
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_service_performance(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        
        let pricing_request = PricingRequest {
            option_type: 0,
            spot: 21500.0,
            strike: 21500.0,
            rate: 0.065,
            volatility: 0.15,
            time_to_expiry: 30.0 / 365.0,
        };
        
        let start = Instant::now();
        let iterations = 100;
        
        for _ in 0..iterations {
            let request = Request::new(pricing_request.clone());
            let _response = service.calculate_price(request).await.unwrap();
        }
        
        let duration = start.elapsed();
        let per_request = duration.as_millis() as f64 / iterations as f64;
        
        // Each request should complete quickly
        assert!(per_request < 10.0, "Service too slow: {:.2}ms per request", per_request);
    }

    #[rstest]
    #[tokio::test]
    async fn test_service_memory_stability(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        
        // Make many requests to test for memory leaks
        for i in 0..1000 {
            let pricing_request = PricingRequest {
                option_type: i % 2, // Alternate between calls and puts
                spot: 21500.0 + (i as f64),
                strike: 21500.0,
                rate: 0.065,
                volatility: 0.15,
                time_to_expiry: 30.0 / 365.0,
            };
            
            let request = Request::new(pricing_request);
            let response = service.calculate_price(request).await;
            assert!(response.is_ok());
            
            // Every 100 iterations, test other services too
            if i % 100 == 0 {
                let strategy_request = Request::new(StrategyRequest {
                    strategy_type: "iron_condor".to_string(),
                    index: 0,
                    spot: 21500.0,
                    legs: vec![],
                });
                
                let _response = service.analyze_strategy(strategy_request).await.unwrap();
            }
        }
        
        // If we reach here without crashing, memory stability is good
        assert!(true);
    }

    #[rstest]
    #[tokio::test]
    async fn test_concurrent_service_requests(grpc_service: OptionsEngineService) {
        use std::sync::Arc;
        
        let service = Arc::new(service);
        let mut handles = Vec::new();
        
        // Launch concurrent requests
        for i in 0..10 {
            let service_clone = service.clone();
            let handle = tokio::spawn(async move {
                let pricing_request = PricingRequest {
                    option_type: i % 2,
                    spot: 21500.0 + i as f64 * 10.0,
                    strike: 21500.0,
                    rate: 0.065,
                    volatility: 0.15,
                    time_to_expiry: 30.0 / 365.0,
                };
                
                let request = Request::new(pricing_request);
                service_clone.calculate_price(request).await
            });
            handles.push(handle);
        }
        
        // Wait for all requests to complete
        for handle in handles {
            let response = handle.await.unwrap();
            assert!(response.is_ok());
            
            let response = response.unwrap().into_inner();
            assert!(response.price > 0.0);
            assert!(response.price.is_finite());
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_service_error_handling(grpc_service: OptionsEngineService) {
        let service = grpc_service;
        
        // Test various error conditions
        let error_cases = vec![
            PricingRequest {
                option_type: 99, // Invalid option type
                spot: 21500.0,
                strike: 21500.0,
                rate: 0.065,
                volatility: 0.15,
                time_to_expiry: 30.0 / 365.0,
            },
            // Add other error cases as needed
        ];
        
        for (i, error_request) in error_cases.iter().enumerate() {
            let request = Request::new(error_request.clone());
            let response = service.calculate_price(request).await;
            
            // Should either handle gracefully or return proper error
            match response {
                Ok(resp) => {
                    // If handled gracefully, should still return valid data
                    let resp = resp.into_inner();
                    assert!(resp.price.is_finite());
                }
                Err(status) => {
                    // Should return proper gRPC error
                    assert!(matches!(
                        status.code(), 
                        tonic::Code::InvalidArgument | 
                        tonic::Code::Internal | 
                        tonic::Code::OutOfRange
                    ));
                    assert!(!status.message().is_empty());
                }
            }
        }
    }
}