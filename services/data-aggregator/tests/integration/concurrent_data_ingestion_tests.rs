//! Integration tests for concurrent data ingestion scenarios

use data_aggregator::{DataAggregatorService, DataAggregator, Timeframe};
use services_common::{Px, Qty, Symbol, Ts};
use chrono::{DateTime, Utc, Duration};
use rstest::*;
use tokio::sync::Arc;
use tokio::task::JoinSet;
use anyhow::Result;
use std::collections::HashMap;
use tempfile::TempDir;

/// Test fixture for creating test aggregator with WAL
#[fixture]
async fn test_aggregator_with_wal() -> Result<(Arc<tokio::sync::RwLock<DataAggregatorService>>, TempDir)> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let aggregator = DataAggregatorService::with_wal(wal_path)?;
    Ok((Arc::new(tokio::sync::RwLock::new(aggregator)), temp_dir))
}

#[rstest]
#[tokio::test]
async fn test_concurrent_single_symbol_ingestion() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let aggregator = Arc::new(tokio::sync::RwLock::new(
        DataAggregatorService::with_wal(wal_path)?
    ));
    
    let symbol = Symbol::new(1);
    let base_time = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    
    let num_producers = 10;
    let trades_per_producer = 100;
    let mut join_set = JoinSet::new();
    
    // Spawn concurrent trade producers
    for producer_id in 0..num_producers {
        let aggregator = Arc::clone(&aggregator);
        
        join_set.spawn(async move {
            let mut trade_count = 0;
            
            for i in 0..trades_per_producer {
                let ts = Ts::from_nanos(
                    (base_time + Duration::milliseconds(producer_id * 1000 + i * 10))
                        .timestamp_nanos_opt()
                        .unwrap() as u64
                );
                
                let price = Px::from_price_i32(100_0000 + (producer_id * 100 + i) as i64);
                let qty = Qty::from_qty_i32(1_0000 + (i % 10) as i64 * 1000);
                let is_buy = (producer_id + i) % 2 == 0;
                
                {
                    let mut agg = aggregator.write().await;
                    agg.process_trade(symbol, ts, price, qty, is_buy).await.unwrap();
                }
                
                trade_count += 1;
                
                // Occasionally yield to allow other tasks to run
                if i % 10 == 0 {
                    tokio::task::yield_now().await;
                }
            }
            
            trade_count
        });
    }
    
    // Wait for all producers to complete
    let mut total_trades_processed = 0;
    while let Some(result) = join_set.join_next().await {
        total_trades_processed += result??;
    }
    
    // Verify results
    assert_eq!(total_trades_processed, num_producers * trades_per_producer);
    
    // Check final state
    {
        let agg = aggregator.read().await;
        let candle = agg.get_current_candle(symbol, Timeframe::M1).await;
        assert!(candle.is_some());
        
        let candle = candle.unwrap();
        assert!(candle.trades > 0);
        assert!(candle.volume.as_i64() > 0);
        
        // Flush WAL and check stats
        drop(agg);
        let mut agg = aggregator.write().await;
        agg.flush_wal().await?;
        
        let stats = agg.wal_stats().await?.expect("Expected WAL stats");
        assert!(stats.total_entries > 0);
        println!("Processed {} trades, WAL has {} entries", 
                 total_trades_processed, stats.total_entries);
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_concurrent_multi_symbol_ingestion() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let aggregator = Arc::new(tokio::sync::RwLock::new(
        DataAggregatorService::with_wal(wal_path)?
    ));
    
    let symbols = vec![Symbol::new(1), Symbol::new(2), Symbol::new(3), Symbol::new(4), Symbol::new(5)];
    let base_time = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    
    let trades_per_symbol = 200;
    let mut join_set = JoinSet::new();
    
    // Spawn one producer per symbol
    for (i, symbol) in symbols.iter().enumerate() {
        let aggregator = Arc::clone(&aggregator);
        let symbol = *symbol;
        
        join_set.spawn(async move {
            let mut trades_processed = 0;
            
            for j in 0..trades_per_symbol {
                let ts = Ts::from_nanos(
                    (base_time + Duration::microseconds((i * trades_per_symbol + j) as i64 * 100))
                        .timestamp_nanos_opt()
                        .unwrap() as u64
                );
                
                let base_price = 100_0000 + (i as i64 * 10_0000); // Different base price per symbol
                let price = Px::from_price_i32(base_price + (j % 100) as i64 * 100);
                let qty = Qty::from_qty_i32(1_0000 + (j % 20) as i64 * 500);
                let is_buy = (i + j) % 3 != 0; // Varied buy/sell ratio per symbol
                
                {
                    let mut agg = aggregator.write().await;
                    agg.process_trade(symbol, ts, price, qty, is_buy).await.unwrap();
                }
                
                trades_processed += 1;
                
                // Yield periodically
                if j % 50 == 0 {
                    tokio::task::yield_now().await;
                }
            }
            
            (symbol, trades_processed)
        });
    }
    
    // Collect results
    let mut symbol_results = HashMap::new();
    while let Some(result) = join_set.join_next().await {
        let (symbol, count) = result??;
        symbol_results.insert(symbol, count);
    }
    
    // Verify each symbol processed correct number of trades
    assert_eq!(symbol_results.len(), symbols.len());
    for symbol in &symbols {
        assert_eq!(symbol_results[symbol], trades_per_symbol);
    }
    
    // Check final state for each symbol
    {
        let agg = aggregator.read().await;
        
        for symbol in &symbols {
            let candle = agg.get_current_candle(*symbol, Timeframe::M1).await;
            assert!(candle.is_some(), "Expected candle for symbol {:?}", symbol);
            
            let candle = candle.unwrap();
            assert_eq!(candle.symbol, *symbol);
            assert!(candle.trades > 0);
            assert!(candle.volume.as_i64() > 0);
            
            println!("Symbol {:?}: {} trades, volume {}", 
                     symbol, candle.trades, candle.volume.as_f64());
        }
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_high_frequency_burst_ingestion() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let aggregator = Arc::new(tokio::sync::RwLock::new(
        DataAggregatorService::with_wal(wal_path)?
    ));
    
    let symbol = Symbol::new(1);
    let base_time = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    
    // Simulate high-frequency burst: 1000 trades in 1 second
    let burst_trades = 1000;
    let burst_duration_ms = 1000;
    let mut join_set = JoinSet::new();
    
    // Create burst producers
    let producers = 5;
    let trades_per_producer = burst_trades / producers;
    
    use std::time::Instant;
    let start_time = Instant::now();
    
    for producer_id in 0..producers {
        let aggregator = Arc::clone(&aggregator);
        
        join_set.spawn(async move {
            let mut successful_trades = 0;
            
            for i in 0..trades_per_producer {
                // Microsecond-level timestamp distribution
                let ts = Ts::from_nanos(
                    (base_time + Duration::microseconds(
                        (producer_id * trades_per_producer + i) as i64
                    )).timestamp_nanos_opt().unwrap() as u64
                );
                
                let price = Px::from_price_i32(100_0000 + (i % 50) as i64);
                let qty = Qty::from_qty_i32(1_0000);
                let is_buy = i % 2 == 0;
                
                {
                    let mut agg = aggregator.write().await;
                    agg.process_trade(symbol, ts, price, qty, is_buy).await.unwrap();
                }
                
                successful_trades += 1;
                
                // No yielding to simulate real burst scenario
            }
            
            successful_trades
        });
    }
    
    // Wait for burst completion
    let mut total_processed = 0;
    while let Some(result) = join_set.join_next().await {
        total_processed += result??;
    }
    
    let processing_time = start_time.elapsed();
    let throughput = total_processed as f64 / processing_time.as_secs_f64();
    
    println!("High-frequency burst results:");
    println!("  Processed {} trades in {:?}", total_processed, processing_time);
    println!("  Throughput: {:.0} trades/second", throughput);
    
    // Verify high throughput achieved
    assert_eq!(total_processed, burst_trades);
    assert!(throughput > 50_000.0, "Should achieve > 50k trades/sec throughput");
    
    // Verify data integrity
    {
        let agg = aggregator.read().await;
        let candle = agg.get_current_candle(symbol, Timeframe::M1).await.unwrap();
        assert_eq!(candle.trades, burst_trades as u32);
        
        // Check WAL integrity
        drop(agg);
        let mut agg = aggregator.write().await;
        agg.flush_wal().await?;
        
        let stats = agg.wal_stats().await?.unwrap();
        assert_eq!(stats.total_entries, burst_trades as u64);
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_mixed_timeframe_concurrent_access() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let aggregator = Arc::new(tokio::sync::RwLock::new(
        DataAggregatorService::with_wal(wal_path)?
    ));
    
    let symbol = Symbol::new(1);
    let base_time = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    
    let mut join_set = JoinSet::new();
    
    // Producer task: Generate trades
    {
        let aggregator = Arc::clone(&aggregator);
        join_set.spawn(async move {
            for i in 0..500 {
                let ts = Ts::from_nanos(
                    (base_time + Duration::milliseconds(i * 100))
                        .timestamp_nanos_opt()
                        .unwrap() as u64
                );
                
                let price = Px::from_price_i32(100_0000 + (i % 100) as i64 * 10);
                let qty = Qty::from_qty_i32(1_0000 + (i % 10) as i64 * 1000);
                let is_buy = i % 2 == 0;
                
                {
                    let mut agg = aggregator.write().await;
                    agg.process_trade(symbol, ts, price, qty, is_buy).await.unwrap();
                }
                
                // Small delay to allow readers
                if i % 50 == 0 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                }
            }
            500
        });
    }
    
    // Reader tasks: Access different timeframes concurrently
    let timeframes = vec![Timeframe::M1, Timeframe::M5, Timeframe::M15, Timeframe::H1];
    
    for timeframe in timeframes {
        let aggregator = Arc::clone(&aggregator);
        
        join_set.spawn(async move {
            let mut readings = 0;
            
            // Read candles periodically
            for _ in 0..20 {
                tokio::time::sleep(tokio::time::Duration::from_millis(25)).await;
                
                {
                    let agg = aggregator.read().await;
                    let candle = agg.get_current_candle(symbol, timeframe).await;
                    
                    if let Some(candle) = candle {
                        assert_eq!(candle.symbol, symbol);
                        assert_eq!(candle.timeframe, timeframe);
                        readings += 1;
                    }
                }
            }
            
            (timeframe, readings)
        });
    }
    
    // Wait for all tasks
    let mut producer_result = None;
    let mut reader_results = HashMap::new();
    
    while let Some(result) = join_set.join_next().await {
        match result? {
            Ok(count) if count == 500 => {
                producer_result = Some(count);
            }
            Ok((timeframe, readings)) => {
                reader_results.insert(timeframe, readings);
            }
            Err(e) => return Err(e),
        }
    }
    
    // Verify results
    assert_eq!(producer_result, Some(500));
    assert_eq!(reader_results.len(), 4); // All timeframes read
    
    for (timeframe, readings) in reader_results {
        assert!(readings > 0, "Timeframe {:?} should have some readings", timeframe);
        println!("Timeframe {:?}: {} readings", timeframe, readings);
    }
    
    // Final verification
    {
        let agg = aggregator.read().await;
        
        for timeframe in [Timeframe::M1, Timeframe::M5, Timeframe::M15, Timeframe::H1] {
            let candle = agg.get_current_candle(symbol, timeframe).await;
            assert!(candle.is_some(), "Should have candle for {:?}", timeframe);
            
            let candle = candle.unwrap();
            assert!(candle.trades > 0);
            assert!(candle.volume.as_i64() > 0);
        }
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_concurrent_candle_completion_and_history() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let aggregator = Arc::new(tokio::sync::RwLock::new(
        DataAggregatorService::with_wal(wal_path)?
    ));
    
    let symbol = Symbol::new(1);
    let base_time = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    
    let mut join_set = JoinSet::new();
    
    // Producer: Generate trades across multiple minute boundaries
    {
        let aggregator = Arc::clone(&aggregator);
        join_set.spawn(async move {
            let mut completed_candles = 0;
            
            // Generate trades over 5 minutes (5 candles)
            for minute in 0..5 {
                for second in 0..60 {
                    let ts = Ts::from_nanos(
                        (base_time + Duration::minutes(minute) + Duration::seconds(second))
                            .timestamp_nanos_opt()
                            .unwrap() as u64
                    );
                    
                    let price = Px::from_price_i32(100_0000 + minute * 1000 + second as i64 * 10);
                    let qty = Qty::from_qty_i32(1_0000);
                    let is_buy = second % 2 == 0;
                    
                    {
                        let mut agg = aggregator.write().await;
                        agg.process_trade(symbol, ts, price, qty, is_buy).await.unwrap();
                    }
                    
                    // Small delay to simulate real-time
                    if second % 10 == 0 {
                        tokio::task::yield_now().await;
                    }
                }
                
                completed_candles += 1;
            }
            
            completed_candles
        });
    }
    
    // Consumer: Monitor completed candles
    {
        let aggregator = Arc::clone(&aggregator);
        join_set.spawn(async move {
            let mut max_completed_seen = 0;
            
            // Check completed candles periodically
            for _ in 0..100 {
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                
                {
                    let agg = aggregator.read().await;
                    let completed = agg.get_candles(symbol, Timeframe::M1, 10).await;
                    
                    if completed.len() > max_completed_seen {
                        max_completed_seen = completed.len();
                        println!("Found {} completed candles", completed.len());
                        
                        // Verify candle ordering
                        for i in 1..completed.len() {
                            assert!(completed[i].open_time > completed[i-1].open_time,
                                   "Candles should be in chronological order");
                        }
                    }
                }
            }
            
            max_completed_seen
        });
    }
    
    // Wait for completion
    let mut results = Vec::new();
    while let Some(result) = join_set.join_next().await {
        results.push(result??);
    }
    
    // Should have generated 5 minutes of data and seen multiple completed candles
    assert!(results.contains(&5)); // Producer completed 5 minutes
    
    // Verify final state
    {
        let agg = aggregator.read().await;
        let completed_candles = agg.get_candles(symbol, Timeframe::M1, 10).await;
        
        // Should have completed candles (at least 4, since we're still in the 5th minute)
        assert!(completed_candles.len() >= 4, "Should have at least 4 completed candles");
        
        // Verify each completed candle has data
        for candle in &completed_candles {
            assert_eq!(candle.symbol, symbol);
            assert_eq!(candle.timeframe, Timeframe::M1);
            assert_eq!(candle.trades, 60); // 60 trades per minute
            assert!(candle.volume.as_i64() > 0);
            assert!(candle.open <= candle.high);
            assert!(candle.low <= candle.close);
        }
        
        println!("Final state: {} completed candles", completed_candles.len());
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_concurrent_wal_operations() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let aggregator = Arc::new(tokio::sync::RwLock::new(
        DataAggregatorService::with_wal(wal_path)?
    ));
    
    let symbols = vec![Symbol::new(1), Symbol::new(2)];
    let base_time = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    
    let mut join_set = JoinSet::new();
    
    // Multiple writers
    for (i, symbol) in symbols.iter().enumerate() {
        let aggregator = Arc::clone(&aggregator);
        let symbol = *symbol;
        
        join_set.spawn(async move {
            for j in 0..200 {
                let ts = Ts::from_nanos(
                    (base_time + Duration::milliseconds((i * 200 + j) as i64 * 10))
                        .timestamp_nanos_opt()
                        .unwrap() as u64
                );
                
                let price = Px::from_price_i32(100_0000 + j as i64 * 100);
                let qty = Qty::from_qty_i32(1_0000);
                let is_buy = j % 2 == 0;
                
                {
                    let mut agg = aggregator.write().await;
                    agg.process_trade(symbol, ts, price, qty, is_buy).await.unwrap();
                }
                
                // Periodic WAL flush
                if j % 50 == 0 {
                    let mut agg = aggregator.write().await;
                    agg.flush_wal().await.unwrap();
                }
                
                tokio::task::yield_now().await;
            }
            
            symbol
        });
    }
    
    // WAL monitor task
    {
        let aggregator = Arc::clone(&aggregator);
        join_set.spawn(async move {
            let mut max_entries = 0;
            
            for _ in 0..50 {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                
                {
                    let agg = aggregator.read().await;
                    if let Ok(Some(stats)) = agg.wal_stats().await {
                        if stats.total_entries > max_entries {
                            max_entries = stats.total_entries;
                            println!("WAL now has {} entries", stats.total_entries);
                        }
                    }
                }
            }
            
            max_entries
        });
    }
    
    // Wait for all tasks
    let mut writer_results = Vec::new();
    let mut monitor_result = None;
    
    while let Some(result) = join_set.join_next().await {
        match result? {
            Ok(Symbol(id)) => writer_results.push(Symbol(id)),
            Ok(entries) => monitor_result = Some(entries),
            Err(e) => return Err(e),
        }
    }
    
    // Verify all writers completed
    assert_eq!(writer_results.len(), symbols.len());
    for symbol in &symbols {
        assert!(writer_results.contains(symbol));
    }
    
    // Verify WAL monitoring worked
    assert!(monitor_result.is_some());
    assert!(monitor_result.unwrap() > 0);
    
    // Final WAL verification
    {
        let mut agg = aggregator.write().await;
        agg.flush_wal().await?;
        
        let stats = agg.wal_stats().await?.unwrap();
        assert_eq!(stats.total_entries, 400); // 200 trades per symbol × 2 symbols
        assert!(stats.total_size > 0);
        
        // Test WAL replay
        let replayed_count = agg.replay_from_wal(None).await?;
        assert_eq!(replayed_count, 400);
        
        println!("Final WAL stats: {} entries, {} bytes", 
                 stats.total_entries, stats.total_size);
    }
    
    Ok(())
}

/// Test recovery after simulated crash during concurrent operations
#[rstest]
#[tokio::test]
async fn test_crash_recovery_during_concurrent_ingestion() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path().to_path_buf();
    
    // Phase 1: Normal operation
    let trades_before_crash = {
        let aggregator = Arc::new(tokio::sync::RwLock::new(
            DataAggregatorService::with_wal(&wal_path)?
        ));
        
        let symbol = Symbol::new(1);
        let base_time = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        
        // Process some trades
        let mut trade_count = 0;
        for i in 0..150 {
            let ts = Ts::from_nanos(
                (base_time + Duration::milliseconds(i * 10))
                    .timestamp_nanos_opt()
                    .unwrap() as u64
            );
            
            let price = Px::from_price_i32(100_0000 + i as i64 * 10);
            let qty = Qty::from_qty_i32(1_0000);
            let is_buy = i % 2 == 0;
            
            {
                let mut agg = aggregator.write().await;
                agg.process_trade(symbol, ts, price, qty, is_buy).await?;
                trade_count += 1;
            }
            
            // Flush occasionally
            if i % 50 == 0 {
                let mut agg = aggregator.write().await;
                agg.flush_wal().await?;
            }
        }
        
        // Get stats before "crash"
        let mut agg = aggregator.write().await;
        agg.flush_wal().await?;
        
        let stats = agg.wal_stats().await?.unwrap();
        println!("Before crash: {} trades, {} WAL entries", 
                 trade_count, stats.total_entries);
        
        // Aggregator is dropped here (simulating crash)
        trade_count
    };
    
    // Phase 2: Recovery and continued operation
    {
        let aggregator = Arc::new(tokio::sync::RwLock::new(
            DataAggregatorService::with_wal(&wal_path)?
        ));
        
        // Verify WAL survived the "crash"
        {
            let mut agg = aggregator.write().await;
            let stats = agg.wal_stats().await?.unwrap();
            assert_eq!(stats.total_entries, trades_before_crash as u64);
            
            // Test replay
            let replayed = agg.replay_from_wal(None).await?;
            assert_eq!(replayed, trades_before_crash as u64);
            println!("After recovery: replayed {} events", replayed);
        }
        
        // Continue processing new trades
        let symbol = Symbol::new(1);
        let base_time = DateTime::parse_from_rfc3339("2024-01-01T12:05:00Z") // Later time
            .unwrap()
            .with_timezone(&Utc);
        
        for i in 0..100 {
            let ts = Ts::from_nanos(
                (base_time + Duration::milliseconds(i * 10))
                    .timestamp_nanos_opt()
                    .unwrap() as u64
            );
            
            let price = Px::from_price_i32(101_0000 + i as i64 * 10);
            let qty = Qty::from_qty_i32(1_0000);
            let is_buy = i % 2 == 1;
            
            {
                let mut agg = aggregator.write().await;
                agg.process_trade(symbol, ts, price, qty, is_buy).await?;
            }
        }
        
        // Verify final state
        {
            let mut agg = aggregator.write().await;
            agg.flush_wal().await?;
            
            let stats = agg.wal_stats().await?.unwrap();
            assert_eq!(stats.total_entries, (trades_before_crash + 100) as u64);
            
            let candle = agg.get_current_candle(symbol, Timeframe::M1).await.unwrap();
            assert!(candle.trades > 0);
            
            println!("Final recovery state: {} WAL entries, current candle has {} trades",
                     stats.total_entries, candle.trades);
        }
    }
    
    Ok(())
}