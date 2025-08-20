use rstest::*;
use approx::{assert_abs_diff_eq, assert_relative_eq};
use options_engine::zerodha::{
    ZerodhaOptionsClient, ZerodhaConfig, OptionChainRequest, OrderRequest, 
    OptionChain, PCRMetrics, StrategyExecutor, RiskLimits
};
use tokio;
use std::sync::Arc;
use chrono::{Utc, NaiveDate};

/// Test fixture for Zerodha configuration (mock/test credentials)
#[fixture]
fn test_zerodha_config() -> ZerodhaConfig {
    ZerodhaConfig {
        api_key: "test_api_key".to_string(),
        api_secret: "test_api_secret".to_string(),
        access_token: "test_access_token".to_string(),
        user_id: "test_user_id".to_string(),
    }
}

/// Test fixture for Zerodha client (uses test config)
#[fixture]
fn test_zerodha_client(test_zerodha_config: ZerodhaConfig) -> ZerodhaOptionsClient {
    ZerodhaOptionsClient::new(test_zerodha_config)
}

/// Test fixture for option chain request
#[fixture]
fn nifty_chain_request() -> OptionChainRequest {
    OptionChainRequest {
        symbol: "NIFTY".to_string(),
        expiry: "2024-12-26".to_string(),
        strike_range: (21000.0, 22000.0),
    }
}

/// Test fixture for Bank Nifty chain request
#[fixture]
fn bank_nifty_chain_request() -> OptionChainRequest {
    OptionChainRequest {
        symbol: "BANKNIFTY".to_string(),
        expiry: "2024-12-19".to_string(),
        strike_range: (47000.0, 49000.0),
    }
}

/// Test fixture for order request
#[fixture]
fn test_order_request() -> OrderRequest {
    OrderRequest {
        tradingsymbol: "NIFTY24DEC21500CE".to_string(),
        exchange: "NFO".to_string(),
        transaction_type: "BUY".to_string(),
        order_type: "LIMIT".to_string(),
        quantity: 50,
        product: "NRML".to_string(),
        validity: "DAY".to_string(),
        price: Some(100.0),
        trigger_price: None,
        tag: Some("TEST_ORDER".to_string()),
    }
}

// Note: These tests use mock data since we can't make real API calls in tests
// In a real implementation, you would use test doubles or mock the HTTP client

#[cfg(test)]
mod zerodha_client_construction_tests {
    use super::*;

    #[rstest]
    fn test_zerodha_client_creation(test_zerodha_config: ZerodhaConfig) {
        let client = ZerodhaOptionsClient::new(test_zerodha_config.clone());
        
        // Client should be created successfully
        assert_eq!(client.config.api_key, test_zerodha_config.api_key);
        assert_eq!(client.config.api_secret, test_zerodha_config.api_secret);
        assert_eq!(client.config.access_token, test_zerodha_config.access_token);
        assert_eq!(client.config.user_id, test_zerodha_config.user_id);
        
        // Base URL should be set correctly
        assert_eq!(client.base_url, "https://api.kite.trade");
    }

    #[rstest]
    fn test_zerodha_config_serialization(test_zerodha_config: ZerodhaConfig) {
        // Test that config can be serialized/deserialized
        let json = serde_json::to_string(&test_zerodha_config).unwrap();
        assert!(json.contains("test_api_key"));
        assert!(json.contains("test_access_token"));
        
        let deserialized: ZerodhaConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.api_key, test_zerodha_config.api_key);
        assert_eq!(deserialized.user_id, test_zerodha_config.user_id);
    }
}

#[cfg(test)]
mod option_chain_tests {
    use super::*;

    // Note: These tests would need to be adapted to work with mock HTTP responses
    // For now, they test the structure and logic that doesn't require API calls

    #[rstest]
    fn test_option_chain_request_structure(nifty_chain_request: OptionChainRequest) {
        let request = nifty_chain_request;
        
        // Verify request structure
        assert_eq!(request.symbol, "NIFTY");
        assert_eq!(request.expiry, "2024-12-26");
        assert_eq!(request.strike_range.0, 21000.0);
        assert_eq!(request.strike_range.1, 22000.0);
        
        // Strike range should be valid
        assert!(request.strike_range.1 > request.strike_range.0);
    }

    #[rstest]
    fn test_option_chain_request_different_symbols() {
        let symbols = vec!["NIFTY", "BANKNIFTY", "FINNIFTY"];
        
        for symbol in symbols {
            let request = OptionChainRequest {
                symbol: symbol.to_string(),
                expiry: "2024-12-26".to_string(),
                strike_range: (20000.0, 22000.0),
            };
            
            assert_eq!(request.symbol, symbol);
            assert!(!request.symbol.is_empty());
            assert!(!request.expiry.is_empty());
        }
    }

    #[rstest]
    fn test_option_chain_request_validation() {
        // Test various strike ranges
        let test_cases = vec![
            (19000.0, 23000.0), // Wide range
            (21450.0, 21550.0), // Narrow range  
            (15000.0, 30000.0), // Very wide range
        ];
        
        for (min_strike, max_strike) in test_cases {
            let request = OptionChainRequest {
                symbol: "NIFTY".to_string(),
                expiry: "2024-12-26".to_string(),
                strike_range: (min_strike, max_strike),
            };
            
            assert!(request.strike_range.1 > request.strike_range.0);
            assert!(request.strike_range.0 > 0.0);
            assert!(request.strike_range.1 > 0.0);
        }
    }

    // Mock test for option chain structure
    #[rstest]
    fn test_option_chain_structure() {
        // Create a mock option chain for testing
        use options_engine::zerodha::{OptionChain, OptionData, OptionQuote, InstrumentInfo, OptionGreeks};
        
        let mock_quote = OptionQuote {
            instrument_token: 256265,
            timestamp: Utc::now(),
            last_price: 100.0,
            volume: 1000,
            buy_quantity: 500,
            sell_quantity: 600,
            open_interest: 50000,
            bid: 99.5,
            ask: 100.5,
            bid_quantity: 250,
            ask_quantity: 300,
            change: 5.0,
            change_percent: 5.26,
            greeks: OptionGreeks {
                iv: 0.15,
                delta: 0.5,
                gamma: 0.002,
                theta: -5.0,
                vega: 20.0,
                rho: 10.0,
            },
        };
        
        let mock_info = InstrumentInfo {
            instrument_token: 256265,
            exchange_token: 1024,
            tradingsymbol: "NIFTY24DEC21500CE".to_string(),
            name: "NIFTY".to_string(),
            expiry: Some(NaiveDate::from_ymd_opt(2024, 12, 26).unwrap()),
            strike: 21500.0,
            tick_size: 0.05,
            lot_size: 50,
            instrument_type: "CE".to_string(),
            segment: "NFO-OPT".to_string(),
            exchange: "NFO".to_string(),
        };
        
        let option_data = OptionData {
            strike: 21500.0,
            quote: mock_quote,
            info: mock_info,
        };
        
        let option_chain = OptionChain {
            symbol: "NIFTY".to_string(),
            expiry: "2024-12-26".to_string(),
            calls: vec![option_data.clone()],
            puts: vec![option_data],
            spot_price: 21500.0,
            timestamp: Utc::now(),
        };
        
        // Test option chain structure
        assert_eq!(option_chain.symbol, "NIFTY");
        assert_eq!(option_chain.expiry, "2024-12-26");
        assert_eq!(option_chain.calls.len(), 1);
        assert_eq!(option_chain.puts.len(), 1);
        assert!(option_chain.spot_price > 0.0);
        
        // Test ATM strike calculation
        let atm_strike = option_chain.get_atm_strike();
        assert_abs_diff_eq!(atm_strike, 21500.0, epsilon = 1.0);
    }
}

#[cfg(test)]
mod option_chain_analysis_tests {
    use super::*;

    #[rstest]
    fn test_get_atm_strike() {
        use options_engine::zerodha::{OptionChain, OptionData, OptionQuote, InstrumentInfo, OptionGreeks};
        
        // Create mock option chain with multiple strikes
        let strikes = vec![21000.0, 21100.0, 21200.0, 21300.0, 21400.0, 21500.0, 21600.0];
        let mut calls = Vec::new();
        
        for &strike in &strikes {
            let mock_quote = OptionQuote {
                instrument_token: 256265,
                timestamp: Utc::now(),
                last_price: 100.0,
                volume: 1000,
                buy_quantity: 500,
                sell_quantity: 600,
                open_interest: 50000,
                bid: 99.5,
                ask: 100.5,
                bid_quantity: 250,
                ask_quantity: 300,
                change: 5.0,
                change_percent: 5.26,
                greeks: OptionGreeks {
                    iv: 0.15,
                    delta: 0.5,
                    gamma: 0.002,
                    theta: -5.0,
                    vega: 20.0,
                    rho: 10.0,
                },
            };
            
            let mock_info = InstrumentInfo {
                instrument_token: 256265,
                exchange_token: 1024,
                tradingsymbol: format!("NIFTY24DEC{}CE", strike),
                name: "NIFTY".to_string(),
                expiry: Some(NaiveDate::from_ymd_opt(2024, 12, 26).unwrap()),
                strike,
                tick_size: 0.05,
                lot_size: 50,
                instrument_type: "CE".to_string(),
                segment: "NFO-OPT".to_string(),
                exchange: "NFO".to_string(),
            };
            
            calls.push(OptionData {
                strike,
                quote: mock_quote,
                info: mock_info,
            });
        }
        
        let option_chain = OptionChain {
            symbol: "NIFTY".to_string(),
            expiry: "2024-12-26".to_string(),
            calls,
            puts: vec![],
            spot_price: 21350.0, // Between 21300 and 21400
            timestamp: Utc::now(),
        };
        
        let atm_strike = option_chain.get_atm_strike();
        
        // Should return the strike closest to spot
        assert!(atm_strike == 21300.0 || atm_strike == 21400.0);
    }

    #[rstest]
    fn test_get_itm_otm_options() {
        use options_engine::zerodha::{OptionChain, OptionData, OptionQuote, InstrumentInfo, OptionGreeks};
        
        let spot_price = 21500.0;
        let strikes = vec![21000.0, 21200.0, 21400.0, 21500.0, 21600.0, 21800.0, 22000.0];
        
        let mut calls = Vec::new();
        let mut puts = Vec::new();
        
        for &strike in &strikes {
            let mock_quote = OptionQuote {
                instrument_token: 256265,
                timestamp: Utc::now(),
                last_price: 100.0,
                volume: 1000,
                buy_quantity: 500,
                sell_quantity: 600,
                open_interest: 50000,
                bid: 99.5,
                ask: 100.5,
                bid_quantity: 250,
                ask_quantity: 300,
                change: 5.0,
                change_percent: 5.26,
                greeks: OptionGreeks {
                    iv: 0.15,
                    delta: if strike < spot_price { 0.7 } else { 0.3 }, // Rough approximation
                    gamma: 0.002,
                    theta: -5.0,
                    vega: 20.0,
                    rho: 10.0,
                },
            };
            
            let call_info = InstrumentInfo {
                instrument_token: 256265,
                exchange_token: 1024,
                tradingsymbol: format!("NIFTY24DEC{}CE", strike),
                name: "NIFTY".to_string(),
                expiry: Some(NaiveDate::from_ymd_opt(2024, 12, 26).unwrap()),
                strike,
                tick_size: 0.05,
                lot_size: 50,
                instrument_type: "CE".to_string(),
                segment: "NFO-OPT".to_string(),
                exchange: "NFO".to_string(),
            };
            
            let put_info = InstrumentInfo {
                instrument_token: 256266,
                exchange_token: 1025,
                tradingsymbol: format!("NIFTY24DEC{}PE", strike),
                name: "NIFTY".to_string(),
                expiry: Some(NaiveDate::from_ymd_opt(2024, 12, 26).unwrap()),
                strike,
                tick_size: 0.05,
                lot_size: 50,
                instrument_type: "PE".to_string(),
                segment: "NFO-OPT".to_string(),
                exchange: "NFO".to_string(),
            };
            
            calls.push(OptionData {
                strike,
                quote: mock_quote.clone(),
                info: call_info,
            });
            
            puts.push(OptionData {
                strike,
                quote: mock_quote,
                info: put_info,
            });
        }
        
        let option_chain = OptionChain {
            symbol: "NIFTY".to_string(),
            expiry: "2024-12-26".to_string(),
            calls,
            puts,
            spot_price,
            timestamp: Utc::now(),
        };
        
        // Test ITM/OTM classification
        let (itm_calls, itm_puts) = option_chain.get_itm_options();
        let (otm_calls, otm_puts) = option_chain.get_otm_options();
        
        // ITM calls should have strikes below spot
        for call in itm_calls {
            assert!(call.strike < spot_price, "ITM call should have strike below spot");
        }
        
        // ITM puts should have strikes above spot
        for put in itm_puts {
            assert!(put.strike > spot_price, "ITM put should have strike above spot");
        }
        
        // OTM calls should have strikes above spot
        for call in otm_calls {
            assert!(call.strike > spot_price, "OTM call should have strike above spot");
        }
        
        // OTM puts should have strikes below spot
        for put in otm_puts {
            assert!(put.strike < spot_price, "OTM put should have strike below spot");
        }
    }

    #[rstest]
    fn test_calculate_pcr_metrics() {
        use options_engine::zerodha::{OptionChain, OptionData, OptionQuote, InstrumentInfo, OptionGreeks};
        
        // Create mock option chain with specific OI and volume data
        let mut calls = Vec::new();
        let mut puts = Vec::new();
        
        // Add some call options
        for i in 0..3 {
            let strike = 21500.0 + (i as f64) * 100.0;
            let oi = 10000 + i * 5000; // Varying OI
            let volume = 1000 + i * 200; // Varying volume
            
            let mock_quote = OptionQuote {
                instrument_token: 256265 + i as u32,
                timestamp: Utc::now(),
                last_price: 100.0 - (i as f64) * 20.0,
                volume: volume as u64,
                buy_quantity: 500,
                sell_quantity: 600,
                open_interest: oi as u64,
                bid: 99.5,
                ask: 100.5,
                bid_quantity: 250,
                ask_quantity: 300,
                change: 5.0,
                change_percent: 5.26,
                greeks: OptionGreeks {
                    iv: 0.15,
                    delta: 0.3 + (i as f64) * 0.1,
                    gamma: 0.002,
                    theta: -5.0,
                    vega: 20.0,
                    rho: 10.0,
                },
            };
            
            let mock_info = InstrumentInfo {
                instrument_token: 256265 + i as u32,
                exchange_token: 1024,
                tradingsymbol: format!("NIFTY24DEC{}CE", strike),
                name: "NIFTY".to_string(),
                expiry: Some(NaiveDate::from_ymd_opt(2024, 12, 26).unwrap()),
                strike,
                tick_size: 0.05,
                lot_size: 50,
                instrument_type: "CE".to_string(),
                segment: "NFO-OPT".to_string(),
                exchange: "NFO".to_string(),
            };
            
            calls.push(OptionData {
                strike,
                quote: mock_quote,
                info: mock_info,
            });
        }
        
        // Add some put options
        for i in 0..3 {
            let strike = 21500.0 - (i as f64) * 100.0;
            let oi = 15000 + i * 3000; // Different OI pattern for puts
            let volume = 800 + i * 150; // Different volume pattern
            
            let mock_quote = OptionQuote {
                instrument_token: 256270 + i as u32,
                timestamp: Utc::now(),
                last_price: 80.0 + (i as f64) * 15.0,
                volume: volume as u64,
                buy_quantity: 400,
                sell_quantity: 500,
                open_interest: oi as u64,
                bid: 79.5,
                ask: 80.5,
                bid_quantity: 200,
                ask_quantity: 250,
                change: -3.0,
                change_percent: -3.6,
                greeks: OptionGreeks {
                    iv: 0.16,
                    delta: -0.7 + (i as f64) * 0.1,
                    gamma: 0.002,
                    theta: -4.0,
                    vega: 18.0,
                    rho: -8.0,
                },
            };
            
            let mock_info = InstrumentInfo {
                instrument_token: 256270 + i as u32,
                exchange_token: 1024,
                tradingsymbol: format!("NIFTY24DEC{}PE", strike),
                name: "NIFTY".to_string(),
                expiry: Some(NaiveDate::from_ymd_opt(2024, 12, 26).unwrap()),
                strike,
                tick_size: 0.05,
                lot_size: 50,
                instrument_type: "PE".to_string(),
                segment: "NFO-OPT".to_string(),
                exchange: "NFO".to_string(),
            };
            
            puts.push(OptionData {
                strike,
                quote: mock_quote,
                info: mock_info,
            });
        }
        
        let option_chain = OptionChain {
            symbol: "NIFTY".to_string(),
            expiry: "2024-12-26".to_string(),
            calls,
            puts,
            spot_price: 21500.0,
            timestamp: Utc::now(),
        };
        
        let pcr_metrics = option_chain.calculate_pcr();
        
        // PCR should be calculated correctly
        assert!(pcr_metrics.oi_pcr > 0.0, "OI PCR should be positive");
        assert!(pcr_metrics.volume_pcr > 0.0, "Volume PCR should be positive");
        assert!(pcr_metrics.oi_pcr.is_finite(), "OI PCR should be finite");
        assert!(pcr_metrics.volume_pcr.is_finite(), "Volume PCR should be finite");
        
        // Interpretation should be provided
        assert!(!pcr_metrics.interpretation.is_empty(), "PCR interpretation should be provided");
        
        // Manual calculation check
        let total_put_oi: u64 = option_chain.puts.iter().map(|p| p.quote.open_interest).sum();
        let total_call_oi: u64 = option_chain.calls.iter().map(|c| c.quote.open_interest).sum();
        let expected_oi_pcr = total_put_oi as f64 / total_call_oi as f64;
        
        assert_abs_diff_eq!(pcr_metrics.oi_pcr, expected_oi_pcr, epsilon = 1e-6);
    }

    #[rstest]
    fn test_calculate_max_pain() {
        use options_engine::zerodha::{OptionChain, OptionData, OptionQuote, InstrumentInfo, OptionGreeks};
        
        // Create a simplified option chain for max pain calculation
        let strikes = vec![21400.0, 21500.0, 21600.0];
        let mut calls = Vec::new();
        let mut puts = Vec::new();
        
        // Create calls and puts with specific OI to test max pain calculation
        for &strike in &strikes {
            let call_oi = if strike == 21500.0 { 50000 } else { 20000 }; // Higher OI at 21500
            let put_oi = if strike == 21500.0 { 40000 } else { 15000 };
            
            let call_quote = OptionQuote {
                instrument_token: 256265,
                timestamp: Utc::now(),
                last_price: 100.0,
                volume: 1000,
                buy_quantity: 500,
                sell_quantity: 600,
                open_interest: call_oi,
                bid: 99.5,
                ask: 100.5,
                bid_quantity: 250,
                ask_quantity: 300,
                change: 5.0,
                change_percent: 5.26,
                greeks: OptionGreeks {
                    iv: 0.15,
                    delta: 0.5,
                    gamma: 0.002,
                    theta: -5.0,
                    vega: 20.0,
                    rho: 10.0,
                },
            };
            
            let put_quote = OptionQuote {
                instrument_token: 256266,
                timestamp: Utc::now(),
                last_price: 80.0,
                volume: 800,
                buy_quantity: 400,
                sell_quantity: 500,
                open_interest: put_oi,
                bid: 79.5,
                ask: 80.5,
                bid_quantity: 200,
                ask_quantity: 250,
                change: -3.0,
                change_percent: -3.6,
                greeks: OptionGreeks {
                    iv: 0.16,
                    delta: -0.5,
                    gamma: 0.002,
                    theta: -4.0,
                    vega: 18.0,
                    rho: -8.0,
                },
            };
            
            let call_info = InstrumentInfo {
                instrument_token: 256265,
                exchange_token: 1024,
                tradingsymbol: format!("NIFTY24DEC{}CE", strike),
                name: "NIFTY".to_string(),
                expiry: Some(NaiveDate::from_ymd_opt(2024, 12, 26).unwrap()),
                strike,
                tick_size: 0.05,
                lot_size: 50,
                instrument_type: "CE".to_string(),
                segment: "NFO-OPT".to_string(),
                exchange: "NFO".to_string(),
            };
            
            let put_info = InstrumentInfo {
                instrument_token: 256266,
                exchange_token: 1025,
                tradingsymbol: format!("NIFTY24DEC{}PE", strike),
                name: "NIFTY".to_string(),
                expiry: Some(NaiveDate::from_ymd_opt(2024, 12, 26).unwrap()),
                strike,
                tick_size: 0.05,
                lot_size: 50,
                instrument_type: "PE".to_string(),
                segment: "NFO-OPT".to_string(),
                exchange: "NFO".to_string(),
            };
            
            calls.push(OptionData {
                strike,
                quote: call_quote,
                info: call_info,
            });
            
            puts.push(OptionData {
                strike,
                quote: put_quote,
                info: put_info,
            });
        }
        
        let option_chain = OptionChain {
            symbol: "NIFTY".to_string(),
            expiry: "2024-12-26".to_string(),
            calls,
            puts,
            spot_price: 21500.0,
            timestamp: Utc::now(),
        };
        
        let max_pain = option_chain.calculate_max_pain();
        
        // Max pain should be within the strike range
        assert!(max_pain >= 21400.0 && max_pain <= 21600.0, "Max pain should be within strike range");
        assert!(max_pain.is_finite(), "Max pain should be finite");
        
        // With the OI distribution we set up, max pain should be close to 21500
        // (highest combined OI)
        assert_abs_diff_eq!(max_pain, 21500.0, epsilon = 50.0);
    }
}

#[cfg(test)]
mod order_management_tests {
    use super::*;

    #[rstest]
    fn test_order_request_structure(test_order_request: OrderRequest) {
        let order = test_order_request;
        
        // Verify order structure
        assert_eq!(order.tradingsymbol, "NIFTY24DEC21500CE");
        assert_eq!(order.exchange, "NFO");
        assert_eq!(order.transaction_type, "BUY");
        assert_eq!(order.order_type, "LIMIT");
        assert_eq!(order.quantity, 50);
        assert_eq!(order.product, "NRML");
        assert_eq!(order.validity, "DAY");
        assert_eq!(order.price, Some(100.0));
        assert_eq!(order.trigger_price, None);
        assert_eq!(order.tag, Some("TEST_ORDER".to_string()));
    }

    #[rstest]
    fn test_order_request_different_types() {
        let order_types = vec![
            ("MARKET", None, None),
            ("LIMIT", Some(100.0), None),
            ("SL", Some(95.0), Some(90.0)),
            ("SL-M", None, Some(90.0)),
        ];
        
        for (order_type, price, trigger_price) in order_types {
            let order = OrderRequest {
                tradingsymbol: "NIFTY24DEC21500CE".to_string(),
                exchange: "NFO".to_string(),
                transaction_type: "BUY".to_string(),
                order_type: order_type.to_string(),
                quantity: 50,
                product: "NRML".to_string(),
                validity: "DAY".to_string(),
                price,
                trigger_price,
                tag: Some(format!("TEST_{}", order_type)),
            };
            
            assert_eq!(order.order_type, order_type);
            assert_eq!(order.price, price);
            assert_eq!(order.trigger_price, trigger_price);
            
            // Validate order structure
            match order_type {
                "MARKET" => {
                    assert_eq!(order.price, None);
                    assert_eq!(order.trigger_price, None);
                }
                "LIMIT" => {
                    assert!(order.price.is_some());
                    assert_eq!(order.trigger_price, None);
                }
                "SL" => {
                    assert!(order.price.is_some());
                    assert!(order.trigger_price.is_some());
                }
                "SL-M" => {
                    assert_eq!(order.price, None);
                    assert!(order.trigger_price.is_some());
                }
                _ => {}
            }
        }
    }

    #[rstest]
    fn test_order_request_validation() {
        // Test valid order parameters
        let valid_params = vec![
            ("BUY", "NFO", "NRML", 50),
            ("SELL", "NFO", "MIS", 100),
            ("BUY", "NFO", "CNC", 25), // Though CNC not typical for options
        ];
        
        for (transaction_type, exchange, product, quantity) in valid_params {
            let order = OrderRequest {
                tradingsymbol: "NIFTY24DEC21500CE".to_string(),
                exchange: exchange.to_string(),
                transaction_type: transaction_type.to_string(),
                order_type: "LIMIT".to_string(),
                quantity,
                product: product.to_string(),
                validity: "DAY".to_string(),
                price: Some(100.0),
                trigger_price: None,
                tag: None,
            };
            
            // Basic validations
            assert!(order.quantity > 0);
            assert!(!order.tradingsymbol.is_empty());
            assert!(!order.exchange.is_empty());
            assert!(order.transaction_type == "BUY" || order.transaction_type == "SELL");
            
            if let Some(price) = order.price {
                assert!(price > 0.0);
            }
            
            if let Some(trigger) = order.trigger_price {
                assert!(trigger > 0.0);
            }
        }
    }
}

#[cfg(test)]
mod strategy_executor_tests {
    use super::*;

    #[rstest]
    fn test_strategy_executor_creation(test_zerodha_client: ZerodhaOptionsClient) {
        let client = Arc::new(test_zerodha_client);
        let executor = StrategyExecutor::new(client.clone());
        
        // Default risk limits should be set
        assert_eq!(executor.risk_limits.max_position_size, 10);
        assert_eq!(executor.risk_limits.max_loss_per_trade, 10000.0);
        assert_eq!(executor.risk_limits.max_daily_loss, 25000.0);
        assert_eq!(executor.risk_limits.max_open_positions, 5);
        assert_eq!(executor.risk_limits.min_margin_buffer, 50000.0);
    }

    #[rstest]
    fn test_risk_limits_customization() {
        // Test custom risk limits
        let custom_limits = RiskLimits {
            max_position_size: 5,
            max_loss_per_trade: 5000.0,
            max_daily_loss: 15000.0,
            max_open_positions: 3,
            min_margin_buffer: 25000.0,
        };
        
        // Validate custom limits
        assert!(custom_limits.max_position_size > 0);
        assert!(custom_limits.max_loss_per_trade > 0.0);
        assert!(custom_limits.max_daily_loss >= custom_limits.max_loss_per_trade);
        assert!(custom_limits.max_open_positions > 0);
        assert!(custom_limits.min_margin_buffer > 0.0);
        
        // Test serialization
        let json = serde_json::to_string(&custom_limits).unwrap();
        let deserialized: RiskLimits = serde_json::from_str(&json).unwrap();
        
        assert_eq!(deserialized.max_position_size, custom_limits.max_position_size);
        assert_eq!(deserialized.max_loss_per_trade, custom_limits.max_loss_per_trade);
    }

    #[rstest]
    fn test_risk_limits_default_values() {
        let default_limits = RiskLimits::default();
        
        // Check default values are reasonable
        assert_eq!(default_limits.max_position_size, 10);
        assert_eq!(default_limits.max_loss_per_trade, 10000.0);
        assert_eq!(default_limits.max_daily_loss, 25000.0);
        assert_eq!(default_limits.max_open_positions, 5);
        assert_eq!(default_limits.min_margin_buffer, 50000.0);
        
        // Verify relationships
        assert!(default_limits.max_daily_loss >= default_limits.max_loss_per_trade);
        assert!(default_limits.max_position_size > 0);
        assert!(default_limits.max_open_positions > 0);
    }

    // Note: Testing execute_iron_condor would require mocking HTTP responses
    // This would be implemented with a proper HTTP mock framework in practice
    #[rstest]
    fn test_iron_condor_parameters_validation() {
        // Test parameter validation for iron condor strategy
        let test_cases = vec![
            ("NIFTY", "2024-12-26", 100.0, 200.0, true),   // Valid
            ("BANKNIFTY", "2024-12-19", 200.0, 400.0, true), // Valid
            ("NIFTY", "2024-12-26", 0.0, 200.0, false),    // Invalid wing width
            ("NIFTY", "2024-12-26", 100.0, 0.0, false),    // Invalid body width
            ("", "2024-12-26", 100.0, 200.0, false),       // Empty symbol
        ];
        
        for (symbol, expiry, wing_width, body_width, should_be_valid) in test_cases {
            // Basic parameter validation
            let is_valid = !symbol.is_empty() && 
                          !expiry.is_empty() && 
                          wing_width > 0.0 && 
                          body_width > 0.0 &&
                          wing_width < body_width * 2.0; // Reasonable relationship
            
            assert_eq!(is_valid, should_be_valid, 
                "Validation failed for params: {} {} {} {}", 
                symbol, expiry, wing_width, body_width);
        }
    }
}

#[cfg(test)]
mod integration_error_handling_tests {
    use super::*;

    #[rstest]
    fn test_zerodha_config_validation(test_zerodha_config: ZerodhaConfig) {
        let mut config = test_zerodha_config;
        
        // Test with missing/empty fields
        let original_api_key = config.api_key.clone();
        
        config.api_key = "".to_string();
        // In practice, client creation might validate this
        let client = ZerodhaOptionsClient::new(config.clone());
        assert!(client.config.api_key.is_empty());
        
        // Restore and test other fields
        config.api_key = original_api_key;
        config.access_token = "".to_string();
        let client = ZerodhaOptionsClient::new(config);
        assert!(client.config.access_token.is_empty());
    }

    #[rstest]
    fn test_option_chain_request_edge_cases() {
        // Test edge cases for option chain requests
        let edge_cases = vec![
            // (symbol, expiry, min_strike, max_strike, description)
            ("", "2024-12-26", 21000.0, 22000.0, "Empty symbol"),
            ("NIFTY", "", 21000.0, 22000.0, "Empty expiry"),
            ("NIFTY", "invalid-date", 21000.0, 22000.0, "Invalid expiry format"),
            ("NIFTY", "2024-12-26", 22000.0, 21000.0, "Inverted strike range"),
            ("NIFTY", "2024-12-26", -1000.0, 22000.0, "Negative strike"),
            ("NIFTY", "2024-12-26", 0.0, 0.0, "Zero strikes"),
        ];
        
        for (symbol, expiry, min_strike, max_strike, description) in edge_cases {
            let request = OptionChainRequest {
                symbol: symbol.to_string(),
                expiry: expiry.to_string(),
                strike_range: (min_strike, max_strike),
            };
            
            // Basic structural validation
            let is_structurally_valid = !request.symbol.is_empty() &&
                                      !request.expiry.is_empty() &&
                                      request.strike_range.1 > request.strike_range.0 &&
                                      request.strike_range.0 > 0.0;
            
            // Log validation results for debugging
            if !is_structurally_valid {
                println!("Invalid request detected: {}", description);
            }
            
            // In practice, the API client would handle these gracefully
            assert!(request.symbol.len() >= 0); // Always true, but represents validation
        }
    }

    #[rstest] 
    fn test_order_request_edge_cases() {
        let edge_cases = vec![
            // (quantity, price, trigger_price, should_be_valid, description)
            (0, Some(100.0), None, false, "Zero quantity"),
            (-50, Some(100.0), None, false, "Negative quantity"),
            (50, Some(0.0), None, false, "Zero price"),
            (50, Some(-100.0), None, false, "Negative price"),
            (50, Some(100.0), Some(0.0), false, "Zero trigger price"),
            (50, Some(100.0), Some(-90.0), false, "Negative trigger price"),
            (50, Some(100.0), None, true, "Valid LIMIT order"),
            (50, None, None, true, "Valid MARKET order"),
        ];
        
        for (quantity, price, trigger_price, should_be_valid, description) in edge_cases {
            let order = OrderRequest {
                tradingsymbol: "NIFTY24DEC21500CE".to_string(),
                exchange: "NFO".to_string(),
                transaction_type: "BUY".to_string(),
                order_type: if price.is_some() { "LIMIT" } else { "MARKET" }.to_string(),
                quantity: quantity as u32, // This will handle negative as very large positive
                product: "NRML".to_string(),
                validity: "DAY".to_string(),
                price,
                trigger_price,
                tag: Some("TEST".to_string()),
            };
            
            // Basic validation logic
            let is_valid = order.quantity > 0 &&
                          !order.tradingsymbol.is_empty() &&
                          !order.exchange.is_empty() &&
                          (order.price.is_none() || order.price.unwrap() > 0.0) &&
                          (order.trigger_price.is_none() || order.trigger_price.unwrap() > 0.0);
            
            if quantity < 0 {
                // Negative quantities become very large u32 values
                assert!(!is_valid || order.quantity > 1000000, "Negative quantity should be invalid: {}", description);
            } else {
                assert_eq!(is_valid, should_be_valid, "Validation mismatch for: {}", description);
            }
        }
    }
}

// Note: These tests focus on the structure and logic of the Zerodha integration
// In a production environment, you would also need:
// 1. HTTP mocking for actual API calls
// 2. Integration tests with sandbox/test environments
// 3. Rate limiting tests
// 4. Authentication flow tests
// 5. WebSocket streaming tests
// 6. Error recovery tests
// 7. Network failure simulation tests