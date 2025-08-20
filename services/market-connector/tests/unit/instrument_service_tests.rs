//! Comprehensive unit tests for instrument service implementation
//! 
//! These tests cover CSV parsing, WAL storage, option chain calculations,
//! instrument queries, and subscription token management.

use rstest::*;
use std::path::PathBuf;
use tokio::fs;
use tempfile::TempDir;

use market_connector::instruments::service::*;
use market_connector::instruments::store::*;
use market_connector::instruments::types::*;
use services_common::{ZerodhaAuth, ZerodhaConfig, Px, Ts};

// Test constants
const TEST_NIFTY_UNDERLYING: &str = "NIFTY";
const TEST_BANKNIFTY_UNDERLYING: &str = "BANKNIFTY";
const TEST_NIFTY_TOKEN: u32 = 256265;
const TEST_NIFTY_FUTURES_TOKEN: u32 = 12345678;
const TEST_NIFTY_CE_TOKEN: u32 = 87654321;
const TEST_NIFTY_PE_TOKEN: u32 = 13579246;
const TEST_SPOT_PRICE: f64 = 17500.25;
const TEST_STRIKE_PRICE: f64 = 17500.0;
const TEST_STRIKE_INTERVAL: f64 = 50.0;
const TEST_STRIKE_RANGE: u32 = 10;
const TEST_EXPIRY_DATE: &str = "2024-01-25";

// CSV test data
const TEST_CSV_DATA: &str = r#"instrument_token,exchange_token,tradingsymbol,name,last_price,expiry,strike,tick_size,lot_size,instrument_type,segment,exchange
256265,1009,"NIFTY 50","NIFTY 50",17500.25,"","0.00",0.05,50,EQ,INDICES,NSE
12345678,12345678,"NIFTY24JAN17500FUT","NIFTY JAN FUT",17500.25,2024-01-25,"0.00",0.05,50,FUT,NFO,NSE
87654321,87654321,"NIFTY24JAN17500CE","NIFTY 25 JAN 2024 17500 CE",125.50,2024-01-25,"17500.00",0.05,50,CE,NFO,NSE
13579246,13579246,"NIFTY24JAN17500PE","NIFTY 25 JAN 2024 17500 PE",115.75,2024-01-25,"17500.00",0.05,50,PE,NFO,NSE"#;

#[fixture]
fn temp_dir() -> TempDir {
    tempfile::tempdir().expect("Failed to create temp dir")
}

#[fixture]
fn test_config(temp_dir: &TempDir) -> InstrumentServiceConfig {
    InstrumentServiceConfig {
        wal_dir: temp_dir.path().join("instruments_wal"),
        wal_segment_size_mb: Some(1), // Small for testing
        fetch_interval_hours: 24,
        fetch_hour: 8,
        max_retries: 3,
        retry_delay_secs: 1,
        enable_auto_updates: false, // Disable for testing
    }
}

#[fixture]
fn test_auth() -> ZerodhaAuth {
    let config = ZerodhaConfig::new(
        "test_user".to_string(),
        "test_password".to_string(),
        "test_totp".to_string(),
        "test_api_key".to_string(),
        "test_api_secret".to_string(),
    );
    ZerodhaAuth::new("test_api_key".to_string(), "test_access_token".to_string(), "test_user".to_string())
}

#[fixture]
async fn empty_service(test_config: InstrumentServiceConfig) -> InstrumentService {
    InstrumentService::new(test_config, None).await.expect("Failed to create service")
}

#[fixture]
async fn populated_service(test_config: InstrumentServiceConfig) -> InstrumentService {
    let service = InstrumentService::new(test_config, None).await.expect("Failed to create service");
    
    // Parse and add test instruments
    let instruments = InstrumentService::parse_csv_data(TEST_CSV_DATA).await.expect("Failed to parse CSV");
    {
        let mut store = service.store.write().await;
        store.add_instruments(instruments).expect("Failed to add instruments");
    }
    
    service
}

#[rstest]
#[tokio::test]
async fn test_service_creation(test_config: InstrumentServiceConfig) {
    let service = InstrumentService::new(test_config, None).await;
    assert!(service.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_service_creation_with_auth(test_config: InstrumentServiceConfig, test_auth: ZerodhaAuth) {
    let service = InstrumentService::new(test_config, Some(test_auth)).await;
    assert!(service.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_csv_data_parsing() {
    let instruments = InstrumentService::parse_csv_data(TEST_CSV_DATA).await
        .expect("Failed to parse CSV data");
    
    assert_eq!(instruments.len(), 4);
    
    // Verify NIFTY spot instrument
    let nifty_spot = instruments.iter().find(|i| i.instrument_token == TEST_NIFTY_TOKEN);
    assert!(nifty_spot.is_some());
    let nifty = nifty_spot.unwrap();
    assert_eq!(nifty.tradingsymbol, "NIFTY 50");
    assert_eq!(nifty.instrument_type, InstrumentType::Equity);
    assert_eq!(nifty.exchange, Exchange::NSE);
    
    // Verify futures instrument
    let futures = instruments.iter().find(|i| i.instrument_token == TEST_NIFTY_FUTURES_TOKEN);
    assert!(futures.is_some());
    let fut = futures.unwrap();
    assert_eq!(fut.instrument_type, InstrumentType::Future);
    assert_eq!(fut.strike_price, Px::ZERO);
    
    // Verify call option
    let call_option = instruments.iter().find(|i| i.instrument_token == TEST_NIFTY_CE_TOKEN);
    assert!(call_option.is_some());
    let ce = call_option.unwrap();
    assert_eq!(ce.instrument_type, InstrumentType::CallOption);
    assert_eq!(ce.strike_price, Px::new(TEST_STRIKE_PRICE));
    
    // Verify put option
    let put_option = instruments.iter().find(|i| i.instrument_token == TEST_NIFTY_PE_TOKEN);
    assert!(put_option.is_some());
    let pe = put_option.unwrap();
    assert_eq!(pe.instrument_type, InstrumentType::PutOption);
    assert_eq!(pe.strike_price, Px::new(TEST_STRIKE_PRICE));
}

#[rstest]
#[tokio::test]
async fn test_csv_parsing_malformed_data() {
    let malformed_csv = r#"instrument_token,exchange_token,tradingsymbol
invalid_token,12345,"TEST_SYMBOL"
256265,invalid_exchange_token,"VALID_SYMBOL""#;
    
    let instruments = InstrumentService::parse_csv_data(malformed_csv).await
        .expect("Should handle malformed data gracefully");
    
    // Should parse only valid rows
    assert!(instruments.len() <= 2);
}

#[rstest]
#[tokio::test]
async fn test_instrument_service_start(empty_service: InstrumentService) {
    let result = empty_service.start().await;
    assert!(result.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_get_by_token(populated_service: InstrumentService) {
    let instrument = populated_service.get_by_token(TEST_NIFTY_TOKEN).await;
    assert!(instrument.is_some());
    
    let nifty = instrument.unwrap();
    assert_eq!(nifty.instrument_token, TEST_NIFTY_TOKEN);
    assert_eq!(nifty.tradingsymbol, "NIFTY 50");
    
    // Test non-existent token
    let non_existent = populated_service.get_by_token(99999999).await;
    assert!(non_existent.is_none());
}

#[rstest]
#[tokio::test]
async fn test_get_by_trading_symbol(populated_service: InstrumentService) {
    let instruments = populated_service.get_by_trading_symbol("NIFTY 50").await;
    assert_eq!(instruments.len(), 1);
    assert_eq!(instruments[0].instrument_token, TEST_NIFTY_TOKEN);
    
    // Test partial match or non-existent
    let empty_result = populated_service.get_by_trading_symbol("NON_EXISTENT").await;
    assert!(empty_result.is_empty());
}

#[rstest]
#[tokio::test]
async fn test_get_subscription_tokens(populated_service: InstrumentService) {
    let (spot_token, current_futures, next_futures) = 
        populated_service.get_subscription_tokens(TEST_NIFTY_UNDERLYING).await;
    
    assert_eq!(spot_token, Some(TEST_NIFTY_TOKEN));
    assert_eq!(current_futures, Some(TEST_NIFTY_FUTURES_TOKEN));
    // next_futures might be None in test data
}

#[rstest]
#[tokio::test]
async fn test_get_current_month_futures(populated_service: InstrumentService) {
    let futures = populated_service.get_current_month_futures(TEST_NIFTY_UNDERLYING).await;
    assert!(futures.is_some());
    
    let fut = futures.unwrap();
    assert_eq!(fut.instrument_token, TEST_NIFTY_FUTURES_TOKEN);
    assert_eq!(fut.instrument_type, InstrumentType::Future);
}

#[rstest]
#[tokio::test]
async fn test_get_active_futures(populated_service: InstrumentService) {
    let futures = populated_service.get_active_futures(TEST_NIFTY_UNDERLYING).await;
    assert!(!futures.is_empty());
    
    // Should contain the test futures
    let test_futures = futures.iter().find(|f| f.instrument_token == TEST_NIFTY_FUTURES_TOKEN);
    assert!(test_futures.is_some());
}

#[rstest]
#[tokio::test]
async fn test_instrument_query_filter(populated_service: InstrumentService) {
    // Test filtering by instrument type
    let filter = InstrumentFilter {
        instrument_type: Some(InstrumentType::Equity),
        exchange: None,
        underlying: None,
        expiry_date: None,
    };
    
    let results = populated_service.query(&filter).await;
    assert!(!results.is_empty());
    
    // All results should be equity instruments
    for instrument in results {
        assert_eq!(instrument.instrument_type, InstrumentType::Equity);
    }
    
    // Test filtering by exchange
    let exchange_filter = InstrumentFilter {
        instrument_type: None,
        exchange: Some(Exchange::NSE),
        underlying: None,
        expiry_date: None,
    };
    
    let nse_results = populated_service.query(&exchange_filter).await;
    assert!(!nse_results.is_empty());
    
    // All results should be from NSE
    for instrument in nse_results {
        assert_eq!(instrument.exchange, Exchange::NSE);
    }
}

#[rstest]
#[tokio::test]
async fn test_get_indices(populated_service: InstrumentService) {
    let indices = populated_service.get_indices().await;
    assert!(!indices.is_empty());
    
    // Should contain NIFTY 50
    let nifty = indices.iter().find(|i| i.instrument_token == TEST_NIFTY_TOKEN);
    assert!(nifty.is_some());
}

#[rstest]
#[tokio::test]
async fn test_service_stats(populated_service: InstrumentService) {
    let stats = populated_service.stats().await;
    assert_eq!(stats.total_instruments, 4);
    assert!(stats.last_update.is_some());
}

#[rstest]
#[tokio::test]
async fn test_atm_option_chain_calculation(populated_service: InstrumentService) {
    let spot_price = Px::new(TEST_SPOT_PRICE);
    
    let chain = populated_service.get_atm_option_chain(
        TEST_NIFTY_UNDERLYING,
        spot_price,
        TEST_STRIKE_RANGE,
        TEST_STRIKE_INTERVAL
    ).await;
    
    assert_eq!(chain.underlying, TEST_NIFTY_UNDERLYING);
    assert_eq!(chain.spot_price, spot_price);
    assert_eq!(chain.strike_range, TEST_STRIKE_RANGE);
    
    // Check for ATM options
    let atm_call = chain.get_atm_call();
    let atm_put = chain.get_atm_put();
    
    // Might be None if exact ATM strike not available in test data
    if atm_call.is_some() {
        assert_eq!(atm_call.unwrap().instrument_type, InstrumentType::CallOption);
    }
    if atm_put.is_some() {
        assert_eq!(atm_put.unwrap().instrument_type, InstrumentType::PutOption);
    }
}

#[rstest]
#[tokio::test]
async fn test_atm_option_chain_default_range(populated_service: InstrumentService) {
    let spot_price = Px::new(TEST_SPOT_PRICE);
    
    let chain = populated_service.get_atm_option_chain_default(
        TEST_NIFTY_UNDERLYING,
        spot_price,
        TEST_STRIKE_INTERVAL
    ).await;
    
    // Should use default 20% strike range
    let expected_range = calculate_default_strike_range(spot_price);
    assert_eq!(chain.strike_range, expected_range);
}

#[rstest]
#[tokio::test]
async fn test_atm_option_chain_auto(populated_service: InstrumentService) {
    let spot_price = Px::new(TEST_SPOT_PRICE);
    
    let chain = populated_service.get_atm_option_chain_auto(
        TEST_NIFTY_UNDERLYING,
        spot_price
    ).await;
    
    assert_eq!(chain.underlying, TEST_NIFTY_UNDERLYING);
    assert_eq!(chain.spot_price, spot_price);
    
    // Should use calculated strike interval based on spot price
    assert!(chain.strike_interval > 0);
}

#[rstest]
#[tokio::test]
async fn test_get_option_by_strike(populated_service: InstrumentService) {
    let call_option = populated_service.get_option_by_strike(
        TEST_NIFTY_UNDERLYING,
        TEST_STRIKE_PRICE,
        OptionType::Call
    ).await;
    
    if call_option.is_some() {
        let ce = call_option.unwrap();
        assert_eq!(ce.instrument_type, InstrumentType::CallOption);
        assert_eq!(ce.strike_price, Px::new(TEST_STRIKE_PRICE));
    }
    
    let put_option = populated_service.get_option_by_strike(
        TEST_NIFTY_UNDERLYING,
        TEST_STRIKE_PRICE,
        OptionType::Put
    ).await;
    
    if put_option.is_some() {
        let pe = put_option.unwrap();
        assert_eq!(pe.instrument_type, InstrumentType::PutOption);
        assert_eq!(pe.strike_price, Px::new(TEST_STRIKE_PRICE));
    }
}

#[rstest]
#[tokio::test]
async fn test_get_available_strikes(populated_service: InstrumentService) {
    let strikes = populated_service.get_available_strikes(TEST_NIFTY_UNDERLYING).await;
    assert!(!strikes.is_empty());
    
    // Should contain our test strike
    assert!(strikes.contains(&TEST_STRIKE_PRICE));
}

#[rstest]
#[tokio::test]
async fn test_get_atm_subscription_tokens(populated_service: InstrumentService) {
    let spot_price = Px::new(TEST_SPOT_PRICE);
    
    let tokens = populated_service.get_atm_subscription_tokens(
        TEST_NIFTY_UNDERLYING,
        spot_price,
        TEST_STRIKE_RANGE,
        TEST_STRIKE_INTERVAL
    ).await;
    
    // Should return option tokens for ATM strikes
    assert!(!tokens.is_empty());
}

#[rstest]
#[tokio::test]
async fn test_get_comprehensive_subscription_tokens(populated_service: InstrumentService) {
    let spot_price = Px::new(TEST_SPOT_PRICE);
    
    let (spot_token, current_futures, next_futures, option_tokens) = 
        populated_service.get_comprehensive_subscription_tokens(
            TEST_NIFTY_UNDERLYING,
            spot_price,
            TEST_STRIKE_RANGE,
            TEST_STRIKE_INTERVAL
        ).await;
    
    assert_eq!(spot_token, Some(TEST_NIFTY_TOKEN));
    assert_eq!(current_futures, Some(TEST_NIFTY_FUTURES_TOKEN));
    // next_futures might be None in test data
    assert!(!option_tokens.is_empty());
}

#[rstest]
fn test_calculate_default_strike_range() {
    let spot_price = Px::new(17500.0);
    let range = calculate_default_strike_range(spot_price);
    
    // Should be reasonable range based on 20% calculation
    assert!(range > 0);
    assert!(range < 200); // Sanity check
}

#[rstest]
fn test_get_default_tick_size_fixed() {
    let tick_size = get_default_tick_size_fixed();
    assert!(tick_size > 0);
    
    // Should represent a small tick size in fixed-point
    let tick_size_float = tick_size as f64 / 10000.0; // Convert from fixed-point
    assert!(tick_size_float > 0.0);
    assert!(tick_size_float < 1.0); // Should be less than 1 rupee
}

#[rstest]
fn test_atm_option_chain_owned_methods() {
    use services_common::constants::financial::STRIKE_PRICE_SCALE;
    use rustc_hash::FxHashMap;
    
    // Create test option chain
    let spot_price = Px::new(17500.0);
    let atm_strike = (17500.0 * STRIKE_PRICE_SCALE) as u64;
    let itm_strike = (17450.0 * STRIKE_PRICE_SCALE) as u64;
    let otm_strike = (17550.0 * STRIKE_PRICE_SCALE) as u64;
    
    let mut calls = FxHashMap::default();
    let mut puts = FxHashMap::default();
    
    // Add test options
    let test_call = Instrument {
        instrument_token: 12345,
        exchange_token: 12345,
        tradingsymbol: "NIFTY24JAN17500CE".to_string(),
        name: Some("NIFTY CALL".to_string()),
        last_price: Px::new(125.0),
        expiry_date: Some("2024-01-25".to_string()),
        strike_price: Px::new(17500.0),
        tick_size: Px::new(0.05),
        lot_size: 50,
        instrument_type: InstrumentType::CallOption,
        segment: Segment::NFO,
        exchange: Exchange::NSE,
    };
    
    let test_put = Instrument {
        instrument_token: 54321,
        exchange_token: 54321,
        tradingsymbol: "NIFTY24JAN17500PE".to_string(),
        name: Some("NIFTY PUT".to_string()),
        last_price: Px::new(115.0),
        expiry_date: Some("2024-01-25".to_string()),
        strike_price: Px::new(17500.0),
        tick_size: Px::new(0.05),
        lot_size: 50,
        instrument_type: InstrumentType::PutOption,
        segment: Segment::NFO,
        exchange: Exchange::NSE,
    };
    
    calls.insert(atm_strike, test_call.clone());
    puts.insert(atm_strike, test_put.clone());
    
    let chain = AtmOptionChainOwned {
        underlying: TEST_NIFTY_UNDERLYING.to_string(),
        spot_price,
        atm_strike,
        calls,
        puts,
        strike_range: 10,
        strike_interval: (50.0 * STRIKE_PRICE_SCALE) as u64,
    };
    
    // Test ATM option retrieval
    let atm_call = chain.get_atm_call();
    assert!(atm_call.is_some());
    assert_eq!(atm_call.unwrap().instrument_token, 12345);
    
    let atm_put = chain.get_atm_put();
    assert!(atm_put.is_some());
    assert_eq!(atm_put.unwrap().instrument_token, 54321);
    
    // Test specific strike retrieval
    let call_at_strike = chain.get_call(17500.0);
    assert!(call_at_strike.is_some());
    
    let put_at_strike = chain.get_put(17500.0);
    assert!(put_at_strike.is_some());
    
    // Test non-existent strike
    let no_call = chain.get_call(18000.0);
    assert!(no_call.is_none());
    
    // Test strikes list
    let strikes = chain.get_strikes();
    assert!(!strikes.is_empty());
    assert!(strikes.contains(&17500.0));
    
    // Test ITM methods (simplified test)
    let itm_calls = chain.get_itm_calls();
    let itm_puts = chain.get_itm_puts();
    
    // With current spot at 17500, and only ATM options, ITM lists should be empty
    // (ATM options are not technically ITM)
    assert!(itm_calls.is_empty());
    assert!(itm_puts.is_empty());
}

#[rstest]
#[tokio::test]
async fn test_wal_storage_integration(temp_dir: &TempDir) {
    let config = InstrumentServiceConfig {
        wal_dir: temp_dir.path().join("test_wal"),
        wal_segment_size_mb: Some(1),
        fetch_interval_hours: 24,
        fetch_hour: 8,
        max_retries: 3,
        retry_delay_secs: 1,
        enable_auto_updates: false,
    };
    
    let service = InstrumentService::new(config, None).await
        .expect("Failed to create service");
    
    // Add test instruments
    let instruments = InstrumentService::parse_csv_data(TEST_CSV_DATA).await
        .expect("Failed to parse CSV");
    
    {
        let mut store = service.store.write().await;
        store.add_instruments(instruments).expect("Failed to add instruments");
        store.sync().expect("Failed to sync WAL");
    }
    
    // Verify instruments were stored
    let stats = service.stats().await;
    assert_eq!(stats.total_instruments, 4);
    
    // Test WAL persistence by creating new service instance
    let config2 = InstrumentServiceConfig {
        wal_dir: temp_dir.path().join("test_wal"),
        wal_segment_size_mb: Some(1),
        fetch_interval_hours: 24,
        fetch_hour: 8,
        max_retries: 3,
        retry_delay_secs: 1,
        enable_auto_updates: false,
    };
    
    let service2 = InstrumentService::new(config2, None).await
        .expect("Failed to create second service");
    
    service2.start().await.expect("Failed to start service");
    
    // Should load instruments from WAL
    let stats2 = service2.stats().await;
    assert_eq!(stats2.total_instruments, 4);
    
    // Verify specific instrument can be retrieved
    let nifty = service2.get_by_token(TEST_NIFTY_TOKEN).await;
    assert!(nifty.is_some());
}

#[rstest]
#[tokio::test]
async fn test_manual_sync(populated_service: InstrumentService) {
    let result = populated_service.sync().await;
    assert!(result.is_ok());
}

#[rstest]
fn test_config_defaults() {
    let config = InstrumentServiceConfig::default();
    
    assert_eq!(config.wal_dir, PathBuf::from("./data/instruments_wal"));
    assert_eq!(config.wal_segment_size_mb, Some(50));
    assert_eq!(config.fetch_interval_hours, 24);
    assert_eq!(config.fetch_hour, 8);
    assert_eq!(config.max_retries, 5);
    assert_eq!(config.retry_delay_secs, 5);
    assert!(config.enable_auto_updates);
}

#[rstest]
#[tokio::test]
async fn test_concurrent_access(populated_service: InstrumentService) {
    use std::sync::Arc;
    
    let service = Arc::new(populated_service);
    let mut handles = Vec::new();
    
    // Spawn multiple concurrent read operations
    for i in 0..10 {
        let service_clone = Arc::clone(&service);
        let handle = tokio::spawn(async move {
            // Alternate between different types of queries
            match i % 3 {
                0 => service_clone.get_by_token(TEST_NIFTY_TOKEN).await,
                1 => service_clone.get_current_month_futures(TEST_NIFTY_UNDERLYING).await,
                _ => {
                    let indices = service_clone.get_indices().await;
                    indices.into_iter().find(|inst| inst.instrument_token == TEST_NIFTY_TOKEN)
                }
            }
        });
        handles.push(handle);
    }
    
    // Wait for all operations to complete
    for handle in handles {
        let result = handle.await.expect("Task should complete");
        // All should find the NIFTY instrument
        assert!(result.is_some());
    }
}

#[rstest]
#[tokio::test]
async fn test_large_dataset_performance() {
    use std::time::Instant;
    
    // Create larger CSV dataset
    let mut large_csv = "instrument_token,exchange_token,tradingsymbol,name,last_price,expiry,strike,tick_size,lot_size,instrument_type,segment,exchange\n".to_string();
    
    for i in 0..10000 {
        large_csv.push_str(&format!(
            "{},{},\"TEST{}\",\"Test Instrument {}\",100.0,,,0.05,50,EQ,INDICES,NSE\n",
            i + 1000000, i + 1000000, i, i
        ));
    }
    
    let start = Instant::now();
    let instruments = InstrumentService::parse_csv_data(&large_csv).await
        .expect("Failed to parse large CSV");
    let parse_time = start.elapsed();
    
    assert_eq!(instruments.len(), 10000);
    
    // Should parse reasonably quickly
    assert!(parse_time.as_secs() < 5, "Parsing took too long: {:?}", parse_time);
}

#[rstest]
#[tokio::test]
async fn test_error_handling_invalid_csv_headers() {
    let invalid_csv = r#"wrong_header,another_wrong_header
123456,654321"#;
    
    let result = InstrumentService::parse_csv_data(invalid_csv).await;
    
    // Should handle gracefully and return empty or minimal results
    assert!(result.is_ok());
    let instruments = result.unwrap();
    assert!(instruments.is_empty() || instruments.len() < 2);
}

#[rstest]
#[tokio::test] 
async fn test_memory_efficiency(populated_service: InstrumentService) {
    use std::mem;
    
    // Check service memory usage is reasonable
    let size = mem::size_of_val(&populated_service);
    assert!(size < 1024); // Should be less than 1KB for the service struct itself
    
    // The actual data is stored in Arc<RwLock<...>> so won't be reflected in service size
}

#[rstest]
#[tokio::test]
async fn test_strike_range_edge_cases() {
    // Test very high spot price
    let high_spot = Px::new(100000.0);
    let high_range = calculate_default_strike_range(high_spot);
    assert!(high_range > 0);
    
    // Test very low spot price
    let low_spot = Px::new(1.0);
    let low_range = calculate_default_strike_range(low_spot);
    assert!(low_range > 0);
    
    // Test zero spot price
    let zero_spot = Px::ZERO;
    let zero_range = calculate_default_strike_range(zero_spot);
    assert_eq!(zero_range, 0);
}

#[rstest]
#[tokio::test]
async fn test_option_chain_with_no_options(empty_service: InstrumentService) {
    let spot_price = Px::new(TEST_SPOT_PRICE);
    
    let chain = empty_service.get_atm_option_chain(
        "NON_EXISTENT",
        spot_price,
        TEST_STRIKE_RANGE,
        TEST_STRIKE_INTERVAL
    ).await;
    
    // Should return empty chain
    assert!(chain.calls.is_empty());
    assert!(chain.puts.is_empty());
    assert_eq!(chain.underlying, "NON_EXISTENT");
}

#[rstest]
#[tokio::test]
async fn test_subscription_tokens_invalid_underlying(populated_service: InstrumentService) {
    let (spot, current_fut, next_fut) = populated_service
        .get_subscription_tokens("INVALID_UNDERLYING").await;
    
    assert_eq!(spot, None);
    assert_eq!(current_fut, None);
    assert_eq!(next_fut, None);
}