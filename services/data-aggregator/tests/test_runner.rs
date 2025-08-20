//! Test runner for data-aggregator comprehensive tests

// Import all test modules
mod unit {
    mod candle_aggregation_tests;
    mod trade_stats_tests;
    mod volume_profile_tests;
    mod wal_operations_tests;
    mod storage_tests;
}

mod integration {
    mod concurrent_data_ingestion_tests;
    mod end_to_end_scenarios_tests;
}

use data_aggregator::{DataAggregatorService, DataAggregator, Timeframe};
use services_common::{Px, Qty, Symbol, Ts};
use chrono::Utc;
use anyhow::Result;

#[tokio::test]
async fn test_basic_functionality_integration() -> Result<()> {
    // Quick integration test to verify the system works end-to-end
    let mut aggregator = DataAggregatorService::new();
    let symbol = Symbol::new(1);
    let ts = Ts::from_nanos(Utc::now().timestamp_nanos_opt().unwrap() as u64);
    let price = Px::from_price_i32(100_0000);
    let qty = Qty::from_qty_i32(10_0000);

    // Process a trade
    aggregator.process_trade(symbol, ts, price, qty, true).await?;

    // Verify candle was created
    let candle = aggregator.get_current_candle(symbol, Timeframe::M1).await;
    assert!(candle.is_some());
    
    let candle = candle.unwrap();
    assert_eq!(candle.symbol, symbol);
    assert_eq!(candle.open, price);
    assert_eq!(candle.high, price);
    assert_eq!(candle.low, price);
    assert_eq!(candle.close, price);
    assert_eq!(candle.volume, qty);
    assert_eq!(candle.trades, 1);

    println!("✅ Basic functionality test passed");
    Ok(())
}