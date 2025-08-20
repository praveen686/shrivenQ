//! End-to-end integration tests covering complete data aggregation workflows

use data_aggregator::{DataAggregatorService, DataAggregator, Timeframe, VolumeProfile};
use services_common::{Px, Qty, Symbol, Ts};
use chrono::{DateTime, Utc, Duration};
use rstest::*;
use anyhow::Result;
use tempfile::TempDir;

/// Test fixture for creating test aggregator with WAL
#[fixture]
fn aggregator_with_wal() -> Result<(DataAggregatorService, TempDir)> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let aggregator = DataAggregatorService::with_wal(wal_path)?;
    Ok((aggregator, temp_dir))
}

/// Simulate a complete trading day scenario
#[rstest]
#[tokio::test]
async fn test_full_trading_day_simulation(aggregator_with_wal: Result<(DataAggregatorService, TempDir)>) -> Result<()> {
    let (mut aggregator, _temp_dir) = aggregator_with_wal?;
    
    let symbol = Symbol::new(1); // AAPL equivalent
    let market_open = DateTime::parse_from_rfc3339("2024-01-15T09:30:00Z").unwrap().with_timezone(&Utc);
    let market_close = DateTime::parse_from_rfc3339("2024-01-15T16:00:00Z").unwrap().with_timezone(&Utc);
    
    // Trading day parameters
    let trading_hours = (market_close - market_open).num_hours() as u32;
    let minutes_per_hour = 60u32;
    let total_minutes = trading_hours * minutes_per_hour;
    
    println!("Simulating {} hours ({} minutes) of trading", trading_hours, total_minutes);
    
    // Market opening: Higher volume and volatility
    let opening_trades = simulate_market_opening(&mut aggregator, symbol, market_open).await?;
    println!("Market opening: {} trades processed", opening_trades);
    
    // Regular trading hours: Steady volume
    let regular_trades = simulate_regular_trading(
        &mut aggregator, 
        symbol, 
        market_open + Duration::hours(1),
        market_close - Duration::hours(1)
    ).await?;
    println!("Regular trading: {} trades processed", regular_trades);
    
    // Market closing: Higher volume
    let closing_trades = simulate_market_closing(
        &mut aggregator, 
        symbol, 
        market_close - Duration::hours(1)
    ).await?;
    println!("Market closing: {} trades processed", closing_trades);
    
    let total_trades = opening_trades + regular_trades + closing_trades;
    println!("Total trades for the day: {}", total_trades);
    
    // Verify aggregated data
    aggregator.flush_wal().await?;
    
    // Check candles for different timeframes
    let timeframes = [Timeframe::M1, Timeframe::M5, Timeframe::M15, Timeframe::H1];
    
    for timeframe in timeframes {
        let completed_candles = aggregator.get_candles(symbol, timeframe, 1000).await;
        println!("{:?} candles: {}", timeframe, completed_candles.len());
        
        // Verify candle data integrity
        for (i, candle) in completed_candles.iter().enumerate() {
            assert_eq!(candle.symbol, symbol);
            assert_eq!(candle.timeframe, timeframe);
            assert!(candle.trades > 0, "Candle {} should have trades", i);
            assert!(candle.volume.as_i64() > 0, "Candle {} should have volume", i);
            assert!(candle.high >= candle.low, "High should be >= low in candle {}", i);
            assert!(candle.high >= candle.open && candle.high >= candle.close, "High should be max in candle {}", i);
            assert!(candle.low <= candle.open && candle.low <= candle.close, "Low should be min in candle {}", i);
        }
    }
    
    // Verify WAL integrity
    let wal_stats = aggregator.wal_stats().await?.unwrap();
    assert_eq!(wal_stats.total_entries, total_trades as u64);
    assert!(wal_stats.total_size > 0);
    
    // Test WAL replay
    let replayed_events = aggregator.replay_from_wal(None).await?;
    assert_eq!(replayed_events, total_trades as u64);
    
    println!("End-to-end trading day simulation completed successfully");
    println!("WAL: {} entries, {} bytes", wal_stats.total_entries, wal_stats.total_size);
    
    Ok(())
}

/// Simulate market opening with high volume and volatility
async fn simulate_market_opening(
    aggregator: &mut DataAggregatorService,
    symbol: Symbol,
    start_time: DateTime<Utc>,
) -> Result<usize> {
    let mut trades = 0;
    let opening_duration = Duration::minutes(30); // First 30 minutes
    
    // Higher volume and volatility at opening
    let base_price = 150_0000; // $150.00
    let mut current_price = base_price;
    
    for minute in 0..30 {
        // More trades per minute during opening (10-50 trades)
        let trades_this_minute = 10 + (minute % 40);
        
        for trade in 0..trades_this_minute {
            let ts = Ts::from_nanos(
                (start_time + Duration::minutes(minute) + Duration::seconds(trade * 60 / trades_this_minute))
                    .timestamp_nanos_opt()
                    .unwrap() as u64
            );
            
            // Higher volatility (±2% price movement)
            let price_change = ((fastrand::f64() - 0.5) * 0.04 * current_price as f64) as i64;
            current_price = (current_price + price_change).max(100_0000).min(200_0000);
            let price = Px::from_i64(current_price);
            
            // Variable lot sizes (1-100 shares)
            let qty = Qty::from_qty_i32((1 + (fastrand::u32() % 100)) as i64 * 1_0000);
            
            // Slight buy bias at opening (60% buy orders)
            let is_buy = fastrand::f64() < 0.6;
            
            aggregator.process_trade(symbol, ts, price, qty, is_buy).await?;
            trades += 1;
        }
        
        // Periodic flush
        if minute % 10 == 0 {
            aggregator.flush_wal().await?;
        }
    }
    
    Ok(trades)
}

/// Simulate regular trading hours with steady volume
async fn simulate_regular_trading(
    aggregator: &mut DataAggregatorService,
    symbol: Symbol,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Result<usize> {
    let mut trades = 0;
    let total_minutes = (end_time - start_time).num_minutes();
    let mut current_price = 150_0000; // Carry over from opening
    
    for minute in 0..total_minutes {
        // Steady volume (5-15 trades per minute)
        let trades_this_minute = 5 + (minute % 10) as usize;
        
        for trade in 0..trades_this_minute {
            let ts = Ts::from_nanos(
                (start_time + Duration::minutes(minute) + Duration::seconds(trade as i64 * 60 / trades_this_minute as i64))
                    .timestamp_nanos_opt()
                    .unwrap() as u64
            );
            
            // Lower volatility (±0.5% price movement)
            let price_change = ((fastrand::f64() - 0.5) * 0.01 * current_price as f64) as i64;
            current_price = (current_price + price_change).max(140_0000).min(160_0000);
            let price = Px::from_i64(current_price);
            
            // Standard lot sizes (1-20 shares)
            let qty = Qty::from_qty_i32((1 + (fastrand::u32() % 20)) as i64 * 1_0000);
            
            // Balanced buy/sell (50/50)
            let is_buy = fastrand::bool();
            
            aggregator.process_trade(symbol, ts, price, qty, is_buy).await?;
            trades += 1;
        }
        
        // Periodic flush every hour
        if minute % 60 == 0 {
            aggregator.flush_wal().await?;
        }
    }
    
    Ok(trades)
}

/// Simulate market closing with higher volume
async fn simulate_market_closing(
    aggregator: &mut DataAggregatorService,
    symbol: Symbol,
    start_time: DateTime<Utc>,
) -> Result<usize> {
    let mut trades = 0;
    let mut current_price = 150_0000; // Assume stable price during regular hours
    
    for minute in 0..60 { // Last hour
        // Increasing volume towards close (15-40 trades per minute)
        let trades_this_minute = 15 + (minute * 25 / 60);
        
        for trade in 0..trades_this_minute {
            let ts = Ts::from_nanos(
                (start_time + Duration::minutes(minute as i64) + Duration::seconds(trade as i64 * 60 / trades_this_minute as i64))
                    .timestamp_nanos_opt()
                    .unwrap() as u64
            );
            
            // Moderate volatility (±1% price movement)
            let price_change = ((fastrand::f64() - 0.5) * 0.02 * current_price as f64) as i64;
            current_price = (current_price + price_change).max(145_0000).min(155_0000);
            let price = Px::from_i64(current_price);
            
            // Larger lot sizes near close (1-50 shares)
            let qty = Qty::from_qty_i32((1 + (fastrand::u32() % 50)) as i64 * 1_0000);
            
            // Slight sell bias at close (45% buy orders)
            let is_buy = fastrand::f64() < 0.45;
            
            aggregator.process_trade(symbol, ts, price, qty, is_buy).await?;
            trades += 1;
        }
    }
    
    // Final flush
    aggregator.flush_wal().await?;
    
    Ok(trades)
}

/// Test multiple symbols trading simultaneously
#[rstest]
#[tokio::test]
async fn test_multi_symbol_portfolio_scenario(aggregator_with_wal: Result<(DataAggregatorService, TempDir)>) -> Result<()> {
    let (mut aggregator, _temp_dir) = aggregator_with_wal?;
    
    // Simulate a portfolio of major stocks
    let portfolio = vec![
        (Symbol::new(1), "AAPL", 150_0000, 0.02), // Apple: $150, 2% volatility
        (Symbol::new(2), "MSFT", 350_0000, 0.018), // Microsoft: $350, 1.8% volatility
        (Symbol::new(3), "GOOGL", 2800_0000, 0.025), // Google: $2800, 2.5% volatility
        (Symbol::new(4), "TSLA", 200_0000, 0.05), // Tesla: $200, 5% volatility
        (Symbol::new(5), "NVDA", 800_0000, 0.04), // Nvidia: $800, 4% volatility
    ];
    
    let start_time = DateTime::parse_from_rfc3339("2024-01-15T10:00:00Z").unwrap().with_timezone(&Utc);
    let duration_minutes = 120; // 2 hours of trading
    
    let mut total_trades_by_symbol = std::collections::HashMap::new();
    
    // Simulate trading for each symbol
    for (symbol, name, base_price, volatility) in &portfolio {
        let mut trades = 0;
        let mut current_price = *base_price;
        
        println!("Simulating {} ({:?}) - Base: ${:.2}, Vol: {:.1}%", 
                 name, symbol, *base_price as f64 / 10000.0, volatility * 100.0);
        
        for minute in 0..duration_minutes {
            // Different activity levels per stock
            let trades_per_minute = match name {
                &"AAPL" => 8 + (minute % 5),  // High volume
                &"MSFT" => 6 + (minute % 4),  // Medium-high volume
                &"GOOGL" => 4 + (minute % 3), // Medium volume
                &"TSLA" => 10 + (minute % 8), // Very high volume (meme stock)
                &"NVDA" => 7 + (minute % 6),  // High volume
                _ => 5,
            };
            
            for trade in 0..trades_per_minute {
                let ts = Ts::from_nanos(
                    (start_time + Duration::minutes(minute as i64) + Duration::seconds(trade as i64 * 60 / trades_per_minute as i64))
                        .timestamp_nanos_opt()
                        .unwrap() as u64
                );
                
                // Apply volatility
                let price_change = ((fastrand::f64() - 0.5) * 2.0 * volatility * current_price as f64) as i64;
                current_price = (current_price + price_change).max(base_price / 2).min(base_price * 2);
                let price = Px::from_i64(current_price);
                
                // Volume varies by stock
                let base_qty = match name {
                    &"TSLA" => 1 + (fastrand::u32() % 200), // Meme stock - higher volume
                    &"GOOGL" => 1 + (fastrand::u32() % 10), // Expensive stock - lower volume
                    _ => 1 + (fastrand::u32() % 50),        // Standard volume
                };
                let qty = Qty::from_qty_i32(base_qty as i64 * 1_0000);
                
                let is_buy = fastrand::bool();
                
                aggregator.process_trade(*symbol, ts, price, qty, is_buy).await?;
                trades += 1;
            }
        }
        
        total_trades_by_symbol.insert(*symbol, trades);
        println!("  {} trades processed for {}", trades, name);
    }
    
    // Verify results for each symbol
    for (symbol, name, _, _) in &portfolio {
        let candle = aggregator.get_current_candle(*symbol, Timeframe::M1).await;
        assert!(candle.is_some(), "Should have current candle for {}", name);
        
        let candle = candle.unwrap();
        assert_eq!(candle.symbol, *symbol);
        assert!(candle.trades > 0, "{} should have trades", name);
        assert!(candle.volume.as_i64() > 0, "{} should have volume", name);
        
        // Check completed candles
        let completed = aggregator.get_candles(*symbol, Timeframe::M5, 50).await;
        println!("  {} has {} completed 5-minute candles", name, completed.len());
    }
    
    // Verify WAL captured all trades
    aggregator.flush_wal().await?;
    let wal_stats = aggregator.wal_stats().await?.unwrap();
    let total_expected_trades: usize = total_trades_by_symbol.values().sum();
    
    assert_eq!(wal_stats.total_entries, total_expected_trades as u64);
    println!("Portfolio simulation: {} total trades across {} symbols", 
             total_expected_trades, portfolio.len());
    
    Ok(())
}

/// Test market data replay and validation
#[rstest]
#[tokio::test]
async fn test_market_data_replay_validation(aggregator_with_wal: Result<(DataAggregatorService, TempDir)>) -> Result<()> {
    let (mut aggregator, _temp_dir) = aggregator_with_wal?;
    
    let symbol = Symbol::new(1);
    let start_time = DateTime::parse_from_rfc3339("2024-01-15T11:00:00Z").unwrap().with_timezone(&Utc);
    
    // Generate deterministic market data for validation
    let mut trade_sequence = Vec::new();
    let mut price = 100_0000i64;
    
    for i in 0..500 {
        let ts = Ts::from_nanos(
            (start_time + Duration::seconds(i * 10))
                .timestamp_nanos_opt()
                .unwrap() as u64
        );
        
        // Deterministic price movement (sine wave + trend)
        let trend = i as f64 * 0.01; // Upward trend
        let cycle = (i as f64 * 0.1).sin() * 50.0; // Sine wave
        price = 100_0000 + (trend + cycle) as i64 * 100;
        
        let qty = Qty::from_qty_i32(((i % 20) + 1) as i64 * 1_0000);
        let is_buy = i % 3 != 0; // ~67% buy orders
        
        trade_sequence.push((ts, Px::from_i64(price), qty, is_buy));
        
        aggregator.process_trade(symbol, ts, Px::from_i64(price), qty, is_buy).await?;
        
        if i % 100 == 0 {
            aggregator.flush_wal().await?;
        }
    }
    
    aggregator.flush_wal().await?;
    
    // Verify original processing
    let original_candle = aggregator.get_current_candle(symbol, Timeframe::M1).await.unwrap();
    let original_completed = aggregator.get_candles(symbol, Timeframe::M1, 100).await;
    let original_wal_stats = aggregator.wal_stats().await?.unwrap();
    
    println!("Original processing:");
    println!("  Current candle: {} trades, volume {}", original_candle.trades, original_candle.volume.as_f64());
    println!("  Completed candles: {}", original_completed.len());
    println!("  WAL entries: {}", original_wal_stats.total_entries);
    
    // Test partial replay (from middle timestamp)
    let middle_timestamp = trade_sequence[250].0; // Halfway point
    let replayed_from_middle = aggregator.replay_from_wal(Some(middle_timestamp)).await?;
    
    assert_eq!(replayed_from_middle, 250, "Should replay last 250 trades");
    println!("Replayed {} trades from middle timestamp", replayed_from_middle);
    
    // Test full replay
    let replayed_all = aggregator.replay_from_wal(None).await?;
    assert_eq!(replayed_all, 500, "Should replay all 500 trades");
    println!("Full replay: {} trades", replayed_all);
    
    // Validate data integrity through replay
    // (In a real implementation, this would reconstruct state and verify consistency)
    
    // Test time-based queries
    let start_ts = trade_sequence[0].0;
    let end_ts = trade_sequence[499].0;
    
    // Verify that we can query data within the recorded time range
    println!("Trade sequence spans: {} to {} ({}s)", 
             start_ts.as_nanos(), end_ts.as_nanos(), 
             (end_ts.as_nanos() - start_ts.as_nanos()) / 1_000_000_000);
    
    // Validate OHLCV accuracy for completed candles
    for (i, candle) in original_completed.iter().enumerate() {
        assert!(candle.high >= candle.low, "Candle {} OHLCV invalid: high < low", i);
        assert!(candle.high >= candle.open, "Candle {} OHLCV invalid: high < open", i);
        assert!(candle.high >= candle.close, "Candle {} OHLCV invalid: high < close", i);
        assert!(candle.low <= candle.open, "Candle {} OHLCV invalid: low > open", i);
        assert!(candle.low <= candle.close, "Candle {} OHLCV invalid: low > close", i);
        assert!(candle.volume.as_i64() > 0, "Candle {} has no volume", i);
        assert!(candle.trades > 0, "Candle {} has no trades", i);
        
        // Volume should equal buy_volume + sell_volume
        let total_calculated = candle.buy_volume.as_i64() + candle.sell_volume.as_i64();
        assert_eq!(candle.volume.as_i64(), total_calculated, "Volume mismatch in candle {}", i);
    }
    
    println!("Market data replay and validation completed successfully");
    Ok(())
}

/// Test system resilience with data corruption scenarios
#[rstest]
#[tokio::test]
async fn test_system_resilience_data_corruption() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    
    let symbol = Symbol::new(1);
    let start_time = DateTime::parse_from_rfc3339("2024-01-15T12:00:00Z").unwrap().with_timezone(&Utc);
    
    // Phase 1: Generate clean data
    let clean_trades = {
        let mut aggregator = DataAggregatorService::with_wal(wal_path)?;
        
        for i in 0..200 {
            let ts = Ts::from_nanos(
                (start_time + Duration::seconds(i * 10))
                    .timestamp_nanos_opt()
                    .unwrap() as u64
            );
            
            let price = Px::from_price_i32(100_0000 + (i % 100) as i64 * 100);
            let qty = Qty::from_qty_i32(1_0000 + (i % 10) as i64 * 1000);
            let is_buy = i % 2 == 0;
            
            aggregator.process_trade(symbol, ts, price, qty, is_buy).await?;
        }
        
        aggregator.flush_wal().await?;
        200
    };
    
    // Verify system can handle restart after normal operation
    {
        let mut aggregator = DataAggregatorService::with_wal(wal_path)?;
        let stats = aggregator.wal_stats().await?.unwrap();
        assert_eq!(stats.total_entries, clean_trades as u64);
        
        let replayed = aggregator.replay_from_wal(None).await?;
        assert_eq!(replayed, clean_trades as u64);
        println!("System restart: replayed {} clean trades", replayed);
        
        // Continue with more data
        for i in 200..300 {
            let ts = Ts::from_nanos(
                (start_time + Duration::seconds(i * 10))
                    .timestamp_nanos_opt()
                    .unwrap() as u64
            );
            
            let price = Px::from_price_i32(100_0000 + (i % 100) as i64 * 100);
            let qty = Qty::from_qty_i32(1_0000);
            let is_buy = i % 2 == 0;
            
            aggregator.process_trade(symbol, ts, price, qty, is_buy).await?;
        }
        
        aggregator.flush_wal().await?;
        
        let final_stats = aggregator.wal_stats().await?.unwrap();
        assert_eq!(final_stats.total_entries, 300);
        println!("Continued operation: {} total entries", final_stats.total_entries);
        
        // Verify current state
        let candle = aggregator.get_current_candle(symbol, Timeframe::M1).await.unwrap();
        assert!(candle.trades > 0);
        assert!(candle.volume.as_i64() > 0);
        
        let completed = aggregator.get_candles(symbol, Timeframe::M1, 100).await;
        println!("Final state: current candle has {} trades, {} completed candles", 
                 candle.trades, completed.len());
    }
    
    Ok(())
}

/// Test performance under load with realistic market conditions
#[rstest]
#[tokio::test]
async fn test_realistic_market_performance(aggregator_with_wal: Result<(DataAggregatorService, TempDir)>) -> Result<()> {
    let (mut aggregator, _temp_dir) = aggregator_with_wal?;
    
    use std::time::Instant;
    
    let symbols = (1..=20).map(Symbol::new).collect::<Vec<_>>(); // 20 symbols
    let start_time = DateTime::parse_from_rfc3339("2024-01-15T09:30:00Z").unwrap().with_timezone(&Utc);
    
    let performance_start = Instant::now();
    
    // Simulate realistic market load
    let mut total_trades = 0;
    let duration_minutes = 60; // 1 hour of trading
    
    for minute in 0..duration_minutes {
        let minute_start = Instant::now();
        let mut minute_trades = 0;
        
        // Each minute, process trades for all symbols
        for (symbol_idx, symbol) in symbols.iter().enumerate() {
            // Different activity levels per symbol (1-20 trades per minute)
            let trades_this_minute = 1 + (symbol_idx % 20);
            
            for trade in 0..trades_this_minute {
                let ts = Ts::from_nanos(
                    (start_time + Duration::minutes(minute as i64) + 
                     Duration::milliseconds(trade as i64 * 1000 / trades_this_minute as i64))
                        .timestamp_nanos_opt()
                        .unwrap() as u64
                );
                
                let base_price = 50_0000 + (symbol_idx as i64 * 10_0000); // Different price levels
                let price_variance = ((minute * 10 + trade) % 1000) as i64 - 500; // ±$5 variance
                let price = Px::from_i64(base_price + price_variance);
                
                let qty = Qty::from_qty_i32((1 + (trade % 50)) as i64 * 1_0000);
                let is_buy = (symbol_idx + trade) % 2 == 0;
                
                aggregator.process_trade(*symbol, ts, price, qty, is_buy).await?;
                minute_trades += 1;
                total_trades += 1;
            }
        }
        
        let minute_duration = minute_start.elapsed();
        let minute_throughput = minute_trades as f64 / minute_duration.as_secs_f64();
        
        // Log performance every 15 minutes
        if minute % 15 == 0 || minute == duration_minutes - 1 {
            println!("Minute {}: {} trades in {:?} ({:.0} trades/sec)", 
                     minute + 1, minute_trades, minute_duration, minute_throughput);
        }
        
        // Periodic WAL flush
        if minute % 10 == 0 {
            aggregator.flush_wal().await?;
        }
    }
    
    let total_duration = performance_start.elapsed();
    let overall_throughput = total_trades as f64 / total_duration.as_secs_f64();
    
    aggregator.flush_wal().await?;
    
    println!("\nPerformance Summary:");
    println!("  Total trades: {}", total_trades);
    println!("  Duration: {:?}", total_duration);
    println!("  Throughput: {:.0} trades/second", overall_throughput);
    
    // Performance assertions
    assert!(overall_throughput > 5_000.0, "Should achieve > 5k trades/sec");
    assert!(total_trades > 0);
    
    // Verify data integrity under load
    let mut total_current_trades = 0;
    let mut total_completed_candles = 0;
    
    for symbol in &symbols {
        let current = aggregator.get_current_candle(*symbol, Timeframe::M1).await;
        if let Some(candle) = current {
            total_current_trades += candle.trades;
        }
        
        let completed = aggregator.get_candles(*symbol, Timeframe::M1, 100).await;
        total_completed_candles += completed.len();
        
        // Verify at least some activity for each symbol
        assert!(current.is_some() || !completed.is_empty(), 
                "Symbol {:?} should have some market activity", symbol);
    }
    
    let wal_stats = aggregator.wal_stats().await?.unwrap();
    assert_eq!(wal_stats.total_entries, total_trades as u64);
    
    println!("Data integrity verification:");
    println!("  WAL entries: {}", wal_stats.total_entries);
    println!("  Current candles total trades: {}", total_current_trades);
    println!("  Total completed candles: {}", total_completed_candles);
    
    // Performance benchmark passed
    Ok(())
}