//! Comprehensive tests for candle aggregation functionality

use data_aggregator::{Candle, DataAggregator, DataAggregatorService, Timeframe};
use services_common::{Px, Qty, Symbol, Ts};
use chrono::{DateTime, Duration, Utc};
use rstest::*;
use tempfile::TempDir;
use anyhow::Result;

/// Test fixture for creating a test aggregator
#[fixture]
fn test_aggregator() -> DataAggregatorService {
    DataAggregatorService::new()
}

/// Test fixture for creating a test aggregator with WAL
#[fixture]
fn test_aggregator_with_wal() -> Result<(DataAggregatorService, TempDir)> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let aggregator = DataAggregatorService::with_wal(wal_path)?;
    Ok((aggregator, temp_dir))
}

/// Test fixture for creating test symbols
#[fixture]
fn test_symbol() -> Symbol {
    Symbol::new(12345)
}

/// Test fixture for creating base timestamp
#[fixture]
fn base_timestamp() -> Ts {
    let dt = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z").unwrap();
    Ts::from_nanos(dt.timestamp_nanos_opt().unwrap() as u64)
}

#[rstest]
#[tokio::test]
async fn test_single_trade_creates_candle(
    mut test_aggregator: DataAggregatorService,
    test_symbol: Symbol,
    base_timestamp: Ts,
) -> Result<()> {
    let price = Px::from_price_i32(100_0000); // $100.00
    let qty = Qty::from_qty_i32(10_0000); // 10.00 shares

    // Process a single trade
    test_aggregator
        .process_trade(test_symbol, base_timestamp, price, qty, true)
        .await?;

    // Check M1 candle
    let candle = test_aggregator
        .get_current_candle(test_symbol, Timeframe::M1)
        .await
        .expect("Expected M1 candle");

    assert_eq!(candle.symbol, test_symbol);
    assert_eq!(candle.timeframe, Timeframe::M1);
    assert_eq!(candle.open, price);
    assert_eq!(candle.high, price);
    assert_eq!(candle.low, price);
    assert_eq!(candle.close, price);
    assert_eq!(candle.volume, qty);
    assert_eq!(candle.buy_volume, qty);
    assert_eq!(candle.sell_volume, Qty::ZERO);
    assert_eq!(candle.trades, 1);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_multiple_trades_update_candle_correctly(
    mut test_aggregator: DataAggregatorService,
    test_symbol: Symbol,
    base_timestamp: Ts,
) -> Result<()> {
    let trades = vec![
        (Px::from_price_i32(100_0000), Qty::from_qty_i32(10_0000), true),   // $100.00, 10 shares, buy
        (Px::from_price_i32(101_0000), Qty::from_qty_i32(5_0000), false),   // $101.00, 5 shares, sell (new high)
        (Px::from_price_i32(99_0000), Qty::from_qty_i32(15_0000), true),    // $99.00, 15 shares, buy (new low)
        (Px::from_price_i32(100_5000), Qty::from_qty_i32(8_0000), false),   // $100.50, 8 shares, sell (closing price)
    ];

    // Process all trades with small time increments
    for (i, (price, qty, is_buy)) in trades.iter().enumerate() {
        let ts = Ts::from_nanos(base_timestamp.as_nanos() + (i as u64 * 1_000_000)); // 1ms increments
        test_aggregator
            .process_trade(test_symbol, ts, *price, *qty, *is_buy)
            .await?;
    }

    // Verify the M1 candle has correct OHLCV values
    let candle = test_aggregator
        .get_current_candle(test_symbol, Timeframe::M1)
        .await
        .expect("Expected M1 candle");

    assert_eq!(candle.open, Px::from_price_i32(100_0000)); // First trade price
    assert_eq!(candle.high, Px::from_price_i32(101_0000)); // Highest price
    assert_eq!(candle.low, Px::from_price_i32(99_0000));   // Lowest price
    assert_eq!(candle.close, Px::from_price_i32(100_5000)); // Last trade price
    assert_eq!(candle.volume, Qty::from_qty_i32(38_0000)); // Total volume (10+5+15+8)
    assert_eq!(candle.buy_volume, Qty::from_qty_i32(25_0000)); // Buy volume (10+15)
    assert_eq!(candle.sell_volume, Qty::from_qty_i32(13_0000)); // Sell volume (5+8)
    assert_eq!(candle.trades, 4);

    Ok(())
}

#[rstest]
#[case(Timeframe::M1, 60)]
#[case(Timeframe::M5, 300)]
#[case(Timeframe::M15, 900)]
#[case(Timeframe::H1, 3600)]
#[case(Timeframe::D1, 86400)]
#[tokio::test]
async fn test_timeframe_durations(
    mut test_aggregator: DataAggregatorService,
    test_symbol: Symbol,
    base_timestamp: Ts,
    #[case] timeframe: Timeframe,
    #[case] expected_seconds: i64,
) -> Result<()> {
    let price = Px::from_price_i32(100_0000);
    let qty = Qty::from_qty_i32(10_0000);

    // Process trade to create candle
    test_aggregator
        .process_trade(test_symbol, base_timestamp, price, qty, true)
        .await?;

    let candle = test_aggregator
        .get_current_candle(test_symbol, timeframe)
        .await
        .expect("Expected candle");

    // Verify the candle time window
    let expected_duration = Duration::seconds(expected_seconds);
    let actual_duration = candle.close_time - candle.open_time;
    assert_eq!(actual_duration, expected_duration);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_candle_completion_and_new_candle_creation(
    mut test_aggregator: DataAggregatorService,
    test_symbol: Symbol,
) -> Result<()> {
    // Start at exact minute boundary
    let start_time = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z").unwrap();
    let start_ts = Ts::from_nanos(start_time.timestamp_nanos_opt().unwrap() as u64);
    
    let price1 = Px::from_price_i32(100_0000);
    let qty1 = Qty::from_qty_i32(10_0000);

    // First trade in first minute
    test_aggregator
        .process_trade(test_symbol, start_ts, price1, qty1, true)
        .await?;

    // Second trade in next minute (should create new candle)
    let next_minute = start_time + Duration::seconds(61);
    let next_ts = Ts::from_nanos(next_minute.timestamp_nanos_opt().unwrap() as u64);
    let price2 = Px::from_price_i32(101_0000);
    let qty2 = Qty::from_qty_i32(5_0000);

    test_aggregator
        .process_trade(test_symbol, next_ts, price2, qty2, false)
        .await?;

    // Verify current candle is the new one
    let current_candle = test_aggregator
        .get_current_candle(test_symbol, Timeframe::M1)
        .await
        .expect("Expected current candle");

    assert_eq!(current_candle.open, price2);
    assert_eq!(current_candle.close, price2);
    assert_eq!(current_candle.volume, qty2);
    assert_eq!(current_candle.trades, 1);

    // Verify completed candles
    let completed_candles = test_aggregator
        .get_candles(test_symbol, Timeframe::M1, 10)
        .await;

    assert_eq!(completed_candles.len(), 1);
    let completed = &completed_candles[0];
    assert_eq!(completed.open, price1);
    assert_eq!(completed.close, price1);
    assert_eq!(completed.volume, qty1);
    assert_eq!(completed.trades, 1);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_multiple_timeframes_simultaneously(
    mut test_aggregator: DataAggregatorService,
    test_symbol: Symbol,
    base_timestamp: Ts,
) -> Result<()> {
    let price = Px::from_price_i32(100_0000);
    let qty = Qty::from_qty_i32(10_0000);

    test_aggregator
        .process_trade(test_symbol, base_timestamp, price, qty, true)
        .await?;

    // Verify all timeframes have candles
    let timeframes = [
        Timeframe::M1, Timeframe::M5, Timeframe::M15, 
        Timeframe::M30, Timeframe::H1, Timeframe::H4, Timeframe::D1
    ];

    for timeframe in timeframes {
        let candle = test_aggregator
            .get_current_candle(test_symbol, timeframe)
            .await
            .expect(&format!("Expected {:?} candle", timeframe));

        assert_eq!(candle.symbol, test_symbol);
        assert_eq!(candle.timeframe, timeframe);
        assert_eq!(candle.open, price);
        assert_eq!(candle.high, price);
        assert_eq!(candle.low, price);
        assert_eq!(candle.close, price);
        assert_eq!(candle.volume, qty);
        assert_eq!(candle.trades, 1);
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_candle_alignment_to_time_boundaries() -> Result<()> {
    let mut aggregator = DataAggregatorService::new();
    let symbol = Symbol::new(1);
    
    // Test with a timestamp that's not on a minute boundary
    let off_boundary_time = DateTime::parse_from_rfc3339("2024-01-01T12:00:37Z").unwrap(); // 37 seconds past minute
    let ts = Ts::from_nanos(off_boundary_time.timestamp_nanos_opt().unwrap() as u64);
    
    let price = Px::from_price_i32(100_0000);
    let qty = Qty::from_qty_i32(10_0000);

    aggregator.process_trade(symbol, ts, price, qty, true).await?;

    let candle = aggregator
        .get_current_candle(symbol, Timeframe::M1)
        .await
        .expect("Expected M1 candle");

    // Candle should start at the beginning of the minute (12:00:00), not at 12:00:37
    let expected_start = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z").unwrap();
    let expected_end = expected_start + Duration::seconds(60);

    assert_eq!(
        candle.open_time.timestamp(),
        expected_start.timestamp()
    );
    assert_eq!(
        candle.close_time.timestamp(),
        expected_end.timestamp()
    );

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_high_low_tracking_accuracy(
    mut test_aggregator: DataAggregatorService,
    test_symbol: Symbol,
    base_timestamp: Ts,
) -> Result<()> {
    // Simulate price movements that test high/low tracking
    let price_sequence = [
        100_0000, 101_0000, 99_5000, 102_0000, 98_0000, 100_0000
    ];
    
    let expected_high = Px::from_price_i32(102_0000);
    let expected_low = Px::from_price_i32(98_0000);

    for (i, &price_cents) in price_sequence.iter().enumerate() {
        let price = Px::from_price_i32(price_cents);
        let qty = Qty::from_qty_i32(10_0000);
        let ts = Ts::from_nanos(base_timestamp.as_nanos() + (i as u64 * 1_000_000));
        
        test_aggregator
            .process_trade(test_symbol, ts, price, qty, true)
            .await?;
    }

    let candle = test_aggregator
        .get_current_candle(test_symbol, Timeframe::M1)
        .await
        .expect("Expected M1 candle");

    assert_eq!(candle.high, expected_high);
    assert_eq!(candle.low, expected_low);
    assert_eq!(candle.open, Px::from_price_i32(price_sequence[0]));
    assert_eq!(candle.close, Px::from_price_i32(*price_sequence.last().unwrap()));

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_buy_sell_volume_segregation(
    mut test_aggregator: DataAggregatorService,
    test_symbol: Symbol,
    base_timestamp: Ts,
) -> Result<()> {
    let trades = vec![
        (Px::from_price_i32(100_0000), Qty::from_qty_i32(10_0000), true),   // Buy 10
        (Px::from_price_i32(100_1000), Qty::from_qty_i32(5_0000), false),   // Sell 5
        (Px::from_price_i32(100_2000), Qty::from_qty_i32(8_0000), true),    // Buy 8
        (Px::from_price_i32(100_3000), Qty::from_qty_i32(3_0000), false),   // Sell 3
    ];

    let mut expected_buy_volume = Qty::ZERO;
    let mut expected_sell_volume = Qty::ZERO;

    for (i, (price, qty, is_buy)) in trades.iter().enumerate() {
        let ts = Ts::from_nanos(base_timestamp.as_nanos() + (i as u64 * 1_000_000));
        test_aggregator
            .process_trade(test_symbol, ts, *price, *qty, *is_buy)
            .await?;

        if *is_buy {
            expected_buy_volume = Qty::from_i64(expected_buy_volume.as_i64() + qty.as_i64());
        } else {
            expected_sell_volume = Qty::from_i64(expected_sell_volume.as_i64() + qty.as_i64());
        }
    }

    let candle = test_aggregator
        .get_current_candle(test_symbol, Timeframe::M1)
        .await
        .expect("Expected M1 candle");

    assert_eq!(candle.buy_volume, expected_buy_volume);   // 10 + 8 = 18
    assert_eq!(candle.sell_volume, expected_sell_volume); // 5 + 3 = 8
    assert_eq!(
        candle.volume,
        Qty::from_i64(expected_buy_volume.as_i64() + expected_sell_volume.as_i64())
    ); // Total: 26

    Ok(())
}

#[rstest]
#[tokio::test] 
async fn test_candle_persistence_with_wal() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let mut aggregator = DataAggregatorService::with_wal(wal_path)?;
    
    let symbol = Symbol::new(1);
    let base_time = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z").unwrap();
    let ts = Ts::from_nanos(base_time.timestamp_nanos_opt().unwrap() as u64);
    
    let price = Px::from_price_i32(100_0000);
    let qty = Qty::from_qty_i32(10_0000);

    // Process trade and flush WAL
    aggregator.process_trade(symbol, ts, price, qty, true).await?;
    aggregator.flush_wal().await?;

    // Verify WAL stats
    let stats = aggregator.wal_stats().await?.expect("Expected WAL stats");
    assert!(stats.total_entries > 0);
    assert!(stats.total_size > 0);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_no_trades_no_candle() -> Result<()> {
    let aggregator = DataAggregatorService::new();
    let symbol = Symbol::new(1);

    // Should return None when no trades have been processed
    let candle = aggregator.get_current_candle(symbol, Timeframe::M1).await;
    assert!(candle.is_none());

    let completed_candles = aggregator.get_candles(symbol, Timeframe::M1, 10).await;
    assert!(completed_candles.is_empty());

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_candle_history_limit() -> Result<()> {
    let mut aggregator = DataAggregatorService::new();
    let symbol = Symbol::new(1);
    
    // Create many completed candles by simulating time progression
    let base_time = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z").unwrap();
    
    // Create 1100 candles (more than the 1000 limit)
    for i in 0..1100 {
        let candle_time = base_time + Duration::seconds(i * 60); // Each minute
        let ts = Ts::from_nanos(candle_time.timestamp_nanos_opt().unwrap() as u64);
        let price = Px::from_price_i32(100_0000 + i as i64);
        let qty = Qty::from_qty_i32(10_0000);

        aggregator.process_trade(symbol, ts, price, qty, true).await?;
        
        // Trigger candle completion by processing trade in next minute
        let next_minute = candle_time + Duration::seconds(61);
        let next_ts = Ts::from_nanos(next_minute.timestamp_nanos_opt().unwrap() as u64);
        let next_price = Px::from_price_i32(100_0000 + (i + 1) as i64);
        
        aggregator.process_trade(symbol, next_ts, next_price, qty, true).await?;
    }

    // Should only keep last 1000 candles
    let completed_candles = aggregator.get_candles(symbol, Timeframe::M1, 2000).await;
    assert!(completed_candles.len() <= 1000);

    Ok(())
}

#[cfg(test)]
mod benchmarks {
    use super::*;
    use criterion::{Criterion, black_box};
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    #[bench]
    fn bench_single_trade_processing(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();
        let mut aggregator = DataAggregatorService::new();
        let symbol = Symbol::new(1);
        let price = Px::from_price_i32(100_0000);
        let qty = Qty::from_qty_i32(10_0000);
        let base_ts = Ts::from_nanos(1000000);

        c.bench_function("process_single_trade", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let ts = Ts::from_nanos(black_box(base_ts.as_nanos() + fastrand::u64(..1000000)));
                    black_box(
                        aggregator.process_trade(symbol, ts, price, qty, true).await
                    ).unwrap();
                });
            });
        });
    }
}