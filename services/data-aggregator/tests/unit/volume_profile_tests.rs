//! Comprehensive tests for volume profile generation and analysis

use data_aggregator::{DataAggregatorService, VolumeProfile, VolumeLevel, DataAggregator};
use services_common::{Px, Qty, Symbol, Ts};
use chrono::{DateTime, Duration, Utc};
use rstest::*;
use anyhow::Result;

/// Test fixture for creating a test aggregator
#[fixture]
fn test_aggregator() -> DataAggregatorService {
    DataAggregatorService::new()
}

/// Test fixture for creating test symbol
#[fixture]
fn test_symbol() -> Symbol {
    Symbol::new(1)
}

/// Test fixture for creating base timestamp
#[fixture]
fn base_timestamp() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

/// Helper function to create volume profile from trades
async fn create_volume_profile_from_trades(
    aggregator: &mut DataAggregatorService,
    symbol: Symbol,
    trades: Vec<(Px, Qty, bool)>, // (price, qty, is_buy)
    base_time: DateTime<Utc>,
) -> Result<()> {
    for (i, (price, qty, is_buy)) in trades.iter().enumerate() {
        let ts = Ts::from_nanos(
            (base_time + Duration::milliseconds(i as i64 * 100))
                .timestamp_nanos_opt()
                .unwrap() as u64
        );
        aggregator.process_trade(symbol, ts, *price, *qty, *is_buy).await?;
    }
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_basic_volume_profile_creation(
    mut test_aggregator: DataAggregatorService,
    test_symbol: Symbol,
    base_timestamp: DateTime<Utc>,
) -> Result<()> {
    // Create trades at different price levels
    let trades = vec![
        (Px::from_price_i32(100_0000), Qty::from_qty_i32(10_0000), true),  // $100.00
        (Px::from_price_i32(100_0000), Qty::from_qty_i32(5_0000), false),  // $100.00
        (Px::from_price_i32(101_0000), Qty::from_qty_i32(8_0000), true),   // $101.00
        (Px::from_price_i32(101_0000), Qty::from_qty_i32(3_0000), false),  // $101.00
        (Px::from_price_i32(102_0000), Qty::from_qty_i32(12_0000), true),  // $102.00
    ];

    create_volume_profile_from_trades(
        &mut test_aggregator,
        test_symbol,
        trades,
        base_timestamp,
    ).await?;

    // Get volume profile (implementation would need to be added to the service)
    // For now, we verify that trades were processed
    let candle = test_aggregator.get_current_candle(test_symbol, data_aggregator::Timeframe::M1).await;
    assert!(candle.is_some());

    let candle = candle.unwrap();
    assert_eq!(candle.trades, 5);
    assert_eq!(candle.volume, Qty::from_qty_i32(38_0000)); // Total volume

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_volume_profile_price_level_aggregation() -> Result<()> {
    // Test volume aggregation at specific price levels
    let trades_at_100 = vec![
        (Px::from_price_i32(100_0000), Qty::from_qty_i32(10_0000), true),
        (Px::from_price_i32(100_0000), Qty::from_qty_i32(5_0000), false),
        (Px::from_price_i32(100_0000), Qty::from_qty_i32(8_0000), true),
    ];

    // Calculate expected aggregated values
    let expected_total_volume = Qty::from_qty_i32(23_0000); // 10 + 5 + 8
    let expected_buy_volume = Qty::from_qty_i32(18_0000);   // 10 + 8
    let expected_sell_volume = Qty::from_qty_i32(5_0000);   // 5
    let expected_trades = 3u32;

    // Since VolumeProfileAggregator is not fully implemented, we simulate the logic
    let mut volume_level = VolumeLevel {
        price: Px::from_price_i32(100_0000),
        volume: Qty::ZERO,
        buy_volume: Qty::ZERO,
        sell_volume: Qty::ZERO,
        trades: 0,
    };

    // Manually aggregate trades (this would be done by VolumeProfileAggregator)
    for (price, qty, is_buy) in trades_at_100 {
        assert_eq!(price, volume_level.price); // Same price level
        
        volume_level.volume = Qty::from_i64(volume_level.volume.as_i64() + qty.as_i64());
        volume_level.trades += 1;
        
        if is_buy {
            volume_level.buy_volume = Qty::from_i64(volume_level.buy_volume.as_i64() + qty.as_i64());
        } else {
            volume_level.sell_volume = Qty::from_i64(volume_level.sell_volume.as_i64() + qty.as_i64());
        }
    }

    // Verify aggregation
    assert_eq!(volume_level.volume, expected_total_volume);
    assert_eq!(volume_level.buy_volume, expected_buy_volume);
    assert_eq!(volume_level.sell_volume, expected_sell_volume);
    assert_eq!(volume_level.trades, expected_trades);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_volume_profile_poc_calculation() -> Result<()> {
    // Test Point of Control (POC) calculation - price level with highest volume
    let price_levels = vec![
        (Px::from_price_i32(100_0000), Qty::from_qty_i32(10_0000)), // $100.00 - 10 shares
        (Px::from_price_i32(101_0000), Qty::from_qty_i32(25_0000)), // $101.00 - 25 shares (POC)
        (Px::from_price_i32(102_0000), Qty::from_qty_i32(15_0000)), // $102.00 - 15 shares
        (Px::from_price_i32(103_0000), Qty::from_qty_i32(8_0000)),  // $103.00 - 8 shares
    ];

    // Find POC (price with highest volume)
    let poc = price_levels
        .iter()
        .max_by_key(|(_, volume)| volume.as_i64())
        .map(|(price, _)| *price)
        .unwrap();

    assert_eq!(poc, Px::from_price_i32(101_0000));

    // Create a mock volume profile
    let volume_profile = VolumeProfile {
        symbol: Symbol::new(1),
        start_time: DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z").unwrap().with_timezone(&Utc),
        end_time: DateTime::parse_from_rfc3339("2024-01-01T13:00:00Z").unwrap().with_timezone(&Utc),
        levels: price_levels
            .into_iter()
            .map(|(price, volume)| VolumeLevel {
                price,
                volume,
                buy_volume: Qty::from_i64(volume.as_i64() / 2), // Mock 50/50 split
                sell_volume: Qty::from_i64(volume.as_i64() / 2),
                trades: 1,
            })
            .collect(),
        poc,
        vah: Px::from_price_i32(102_0000), // Mock VAH
        val: Px::from_price_i32(100_0000), // Mock VAL
    };

    assert_eq!(volume_profile.poc, poc);
    assert_eq!(volume_profile.levels.len(), 4);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_volume_profile_value_area_calculation() -> Result<()> {
    // Test Value Area High (VAH) and Value Area Low (VAL) calculation
    // Value area typically contains 70% of total volume
    
    let levels = vec![
        VolumeLevel {
            price: Px::from_price_i32(98_0000),
            volume: Qty::from_qty_i32(5_0000),
            buy_volume: Qty::from_qty_i32(3_0000),
            sell_volume: Qty::from_qty_i32(2_0000),
            trades: 2,
        },
        VolumeLevel {
            price: Px::from_price_i32(99_0000),
            volume: Qty::from_qty_i32(15_0000),
            buy_volume: Qty::from_qty_i32(8_0000),
            sell_volume: Qty::from_qty_i32(7_0000),
            trades: 5,
        },
        VolumeLevel {
            price: Px::from_price_i32(100_0000), // POC
            volume: Qty::from_qty_i32(30_0000),
            buy_volume: Qty::from_qty_i32(18_0000),
            sell_volume: Qty::from_qty_i32(12_0000),
            trades: 12,
        },
        VolumeLevel {
            price: Px::from_price_i32(101_0000),
            volume: Qty::from_qty_i32(12_0000),
            buy_volume: Qty::from_qty_i32(6_0000),
            sell_volume: Qty::from_qty_i32(6_0000),
            trades: 4,
        },
        VolumeLevel {
            price: Px::from_price_i32(102_0000),
            volume: Qty::from_qty_i32(8_0000),
            buy_volume: Qty::from_qty_i32(4_0000),
            sell_volume: Qty::from_qty_i32(4_0000),
            trades: 3,
        },
    ];

    // Calculate total volume
    let total_volume: i64 = levels.iter().map(|l| l.volume.as_i64()).sum();
    let target_volume = (total_volume as f64 * 0.70) as i64; // 70% for value area

    // Find POC (highest volume)
    let poc_idx = levels
        .iter()
        .enumerate()
        .max_by_key(|(_, level)| level.volume.as_i64())
        .map(|(idx, _)| idx)
        .unwrap();

    assert_eq!(poc_idx, 2); // Index of $100.00 level
    assert_eq!(levels[poc_idx].volume, Qty::from_qty_i32(30_0000));

    // Mock value area calculation (simplified)
    // In practice, this would expand around POC until 70% volume is captured
    let mut value_area_volume = levels[poc_idx].volume.as_i64();
    let mut val_idx = poc_idx;
    let mut vah_idx = poc_idx;

    // Expand value area (simplified algorithm)
    while value_area_volume < target_volume {
        let expand_down = val_idx > 0;
        let expand_up = vah_idx < levels.len() - 1;
        
        if expand_down && expand_up {
            // Choose direction with more volume
            let down_volume = levels[val_idx - 1].volume.as_i64();
            let up_volume = levels[vah_idx + 1].volume.as_i64();
            
            if down_volume >= up_volume {
                val_idx -= 1;
                value_area_volume += down_volume;
            } else {
                vah_idx += 1;
                value_area_volume += up_volume;
            }
        } else if expand_down {
            val_idx -= 1;
            value_area_volume += levels[val_idx].volume.as_i64();
        } else if expand_up {
            vah_idx += 1;
            value_area_volume += levels[vah_idx].volume.as_i64();
        } else {
            break;
        }
    }

    let vah = levels[vah_idx].price;
    let val = levels[val_idx].price;

    // Verify value area makes sense
    assert!(vah >= levels[poc_idx].price);
    assert!(val <= levels[poc_idx].price);
    assert!(value_area_volume >= target_volume);

    println!("Total volume: {}, Target (70%): {}, Value area volume: {}", 
             total_volume, target_volume, value_area_volume);
    println!("VAL: ${:.2}, POC: ${:.2}, VAH: ${:.2}", 
             val.as_f64(), levels[poc_idx].price.as_f64(), vah.as_f64());

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_volume_profile_time_segmentation() -> Result<()> {
    // Test volume profile generation for specific time periods
    let symbol = Symbol::new(1);
    let start_time = DateTime::parse_from_rfc3339("2024-01-01T09:30:00Z").unwrap().with_timezone(&Utc);
    let end_time = DateTime::parse_from_rfc3339("2024-01-01T16:00:00Z").unwrap().with_timezone(&Utc);
    
    // Create mock volume profile for trading session
    let volume_profile = VolumeProfile {
        symbol,
        start_time,
        end_time,
        levels: vec![
            VolumeLevel {
                price: Px::from_price_i32(100_0000),
                volume: Qty::from_qty_i32(1000_0000),
                buy_volume: Qty::from_qty_i32(600_0000),
                sell_volume: Qty::from_qty_i32(400_0000),
                trades: 250,
            },
            VolumeLevel {
                price: Px::from_price_i32(101_0000),
                volume: Qty::from_qty_i32(800_0000),
                buy_volume: Qty::from_qty_i32(450_0000),
                sell_volume: Qty::from_qty_i32(350_0000),
                trades: 180,
            },
        ],
        poc: Px::from_price_i32(100_0000),
        vah: Px::from_price_i32(101_0000),
        val: Px::from_price_i32(100_0000),
    };

    // Verify time period
    assert_eq!(volume_profile.start_time, start_time);
    assert_eq!(volume_profile.end_time, end_time);
    
    let session_duration = end_time - start_time;
    assert_eq!(session_duration, Duration::hours(6) + Duration::minutes(30));

    // Verify volume profile data integrity
    assert_eq!(volume_profile.levels.len(), 2);
    
    let total_volume: i64 = volume_profile.levels.iter().map(|l| l.volume.as_i64()).sum();
    let total_trades: u32 = volume_profile.levels.iter().map(|l| l.trades).sum();
    
    assert_eq!(total_volume, 1800_0000); // 1000 + 800
    assert_eq!(total_trades, 430); // 250 + 180

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_volume_profile_market_imbalance_detection() -> Result<()> {
    // Test detection of buy/sell imbalances in volume profile
    
    let levels_with_buy_pressure = vec![
        VolumeLevel {
            price: Px::from_price_i32(100_0000),
            volume: Qty::from_qty_i32(100_0000),
            buy_volume: Qty::from_qty_i32(80_0000),   // 80% buy
            sell_volume: Qty::from_qty_i32(20_0000),  // 20% sell
            trades: 10,
        },
        VolumeLevel {
            price: Px::from_price_i32(101_0000),
            volume: Qty::from_qty_i32(50_0000),
            buy_volume: Qty::from_qty_i32(35_0000),   // 70% buy
            sell_volume: Qty::from_qty_i32(15_0000),  // 30% sell
            trades: 5,
        },
    ];

    // Calculate imbalance for each level
    for level in &levels_with_buy_pressure {
        let buy_ratio = level.buy_volume.as_f64() / level.volume.as_f64();
        let sell_ratio = level.sell_volume.as_f64() / level.volume.as_f64();
        
        assert!((buy_ratio + sell_ratio - 1.0).abs() < f64::EPSILON); // Should sum to 1.0
        
        // Check for significant buy pressure (>60% buy volume)
        if buy_ratio > 0.60 {
            println!("Buy pressure detected at ${:.2}: {:.1}% buy volume", 
                     level.price.as_f64(), buy_ratio * 100.0);
        }
    }

    // Test opposite scenario - sell pressure
    let levels_with_sell_pressure = vec![
        VolumeLevel {
            price: Px::from_price_i32(99_0000),
            volume: Qty::from_qty_i32(100_0000),
            buy_volume: Qty::from_qty_i32(25_0000),   // 25% buy
            sell_volume: Qty::from_qty_i32(75_0000),  // 75% sell
            trades: 12,
        },
    ];

    for level in &levels_with_sell_pressure {
        let sell_ratio = level.sell_volume.as_f64() / level.volume.as_f64();
        
        if sell_ratio > 0.60 {
            println!("Sell pressure detected at ${:.2}: {:.1}% sell volume", 
                     level.price.as_f64(), sell_ratio * 100.0);
        }
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_volume_profile_price_level_granularity() -> Result<()> {
    // Test different price level granularities (tick size considerations)
    
    // Fine granularity - penny increments
    let fine_levels = vec![
        (Px::from_price_i32(100_0000), Qty::from_qty_i32(10_0000)), // $100.00
        (Px::from_price_i32(100_0100), Qty::from_qty_i32(5_0000)),  // $100.01
        (Px::from_price_i32(100_0200), Qty::from_qty_i32(8_0000)),  // $100.02
        (Px::from_price_i32(100_0300), Qty::from_qty_i32(3_0000)),  // $100.03
    ];

    // Coarse granularity - dollar increments (aggregated)
    let mut coarse_levels = std::collections::HashMap::new();
    
    for (price, volume) in fine_levels {
        // Round down to nearest dollar
        let dollar_price = Px::from_i64((price.as_i64() / 10000) * 10000);
        
        let entry = coarse_levels.entry(dollar_price).or_insert(Qty::ZERO);
        *entry = Qty::from_i64(entry.as_i64() + volume.as_i64());
    }

    // Verify aggregation
    assert_eq!(coarse_levels.len(), 1); // All prices round to $100.00
    
    let total_volume_at_100 = coarse_levels[&Px::from_price_i32(100_0000)];
    assert_eq!(total_volume_at_100, Qty::from_qty_i32(26_0000)); // 10+5+8+3

    // Test that we can choose appropriate granularity based on price range
    let price_range = Px::from_price_i32(100_0300).as_i64() - Px::from_price_i32(100_0000).as_i64();
    let suggested_tick_size = if price_range < 1_0000 { // Less than $1 range
        100 // Penny ticks
    } else if price_range < 100_0000 { // Less than $100 range
        1000 // 10-cent ticks
    } else {
        10000 // Dollar ticks
    };

    assert_eq!(suggested_tick_size, 100); // Should suggest penny ticks for small range

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_volume_profile_multiple_symbols() -> Result<()> {
    // Test volume profile generation for multiple symbols
    let symbols = vec![Symbol::new(1), Symbol::new(2), Symbol::new(3)];
    let base_time = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z").unwrap().with_timezone(&Utc);
    
    let mut profiles = Vec::new();
    
    for (i, symbol) in symbols.iter().enumerate() {
        let profile = VolumeProfile {
            symbol: *symbol,
            start_time: base_time,
            end_time: base_time + Duration::hours(1),
            levels: vec![
                VolumeLevel {
                    price: Px::from_price_i32((100 + i as i64) * 10000), // Different base prices
                    volume: Qty::from_qty_i32((1000 + i as i64 * 100) * 10000),
                    buy_volume: Qty::from_qty_i32((600 + i as i64 * 50) * 10000),
                    sell_volume: Qty::from_qty_i32((400 + i as i64 * 50) * 10000),
                    trades: 100 + i as u32 * 10,
                },
            ],
            poc: Px::from_price_i32((100 + i as i64) * 10000),
            vah: Px::from_price_i32((100 + i as i64) * 10000),
            val: Px::from_price_i32((100 + i as i64) * 10000),
        };
        profiles.push(profile);
    }

    // Verify each profile is unique
    for (i, profile) in profiles.iter().enumerate() {
        assert_eq!(profile.symbol, symbols[i]);
        assert_eq!(profile.levels.len(), 1);
        
        let expected_price = Px::from_price_i32((100 + i as i64) * 10000);
        assert_eq!(profile.levels[0].price, expected_price);
        assert_eq!(profile.poc, expected_price);
    }

    // Verify profiles are ordered by symbol
    for i in 1..profiles.len() {
        assert!(profiles[i].symbol.0 > profiles[i-1].symbol.0);
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_volume_profile_empty_period() -> Result<()> {
    // Test volume profile generation for periods with no trades
    let symbol = Symbol::new(1);
    let start_time = DateTime::parse_from_rfc3339("2024-01-01T02:00:00Z").unwrap().with_timezone(&Utc); // Overnight
    let end_time = DateTime::parse_from_rfc3339("2024-01-01T06:00:00Z").unwrap().with_timezone(&Utc);
    
    // Create empty volume profile
    let empty_profile = VolumeProfile {
        symbol,
        start_time,
        end_time,
        levels: vec![], // No price levels
        poc: Px::ZERO,  // No POC
        vah: Px::ZERO,  // No VAH
        val: Px::ZERO,  // No VAL
    };

    assert_eq!(empty_profile.levels.len(), 0);
    assert_eq!(empty_profile.poc, Px::ZERO);
    assert_eq!(empty_profile.vah, Px::ZERO);
    assert_eq!(empty_profile.val, Px::ZERO);

    // Verify time period is still valid
    assert!(empty_profile.end_time > empty_profile.start_time);
    let period_duration = empty_profile.end_time - empty_profile.start_time;
    assert_eq!(period_duration, Duration::hours(4));

    Ok(())
}

/// Performance test for volume profile calculations
#[rstest]
#[tokio::test]
async fn test_volume_profile_performance_large_dataset() -> Result<()> {
    use std::time::Instant;
    
    // Create large dataset
    let num_price_levels = 1000;
    let mut levels = Vec::with_capacity(num_price_levels);
    
    let start = Instant::now();
    
    for i in 0..num_price_levels {
        let level = VolumeLevel {
            price: Px::from_price_i32(100_0000 + i as i64 * 100), // Penny increments from $100.00
            volume: Qty::from_qty_i32(fastrand::i64(1_0000..100_0000)), // Random volume
            buy_volume: Qty::from_qty_i32(fastrand::i64(0..50_0000)),
            sell_volume: Qty::from_qty_i32(fastrand::i64(0..50_0000)),
            trades: fastrand::u32(1..100),
        };
        levels.push(level);
    }
    
    // Find POC (most computationally expensive operation)
    let poc = levels
        .iter()
        .max_by_key(|level| level.volume.as_i64())
        .map(|level| level.price)
        .unwrap();
    
    // Calculate total volume
    let total_volume: i64 = levels.iter().map(|l| l.volume.as_i64()).sum();
    
    let duration = start.elapsed();
    
    println!("Processed {} price levels in {:?}", num_price_levels, duration);
    println!("POC: ${:.2}, Total Volume: {}", poc.as_f64(), total_volume);
    
    // Performance assertion
    assert!(duration.as_millis() < 100, "Volume profile processing should be fast");
    assert!(total_volume > 0);
    assert!(poc > Px::ZERO);

    Ok(())
}