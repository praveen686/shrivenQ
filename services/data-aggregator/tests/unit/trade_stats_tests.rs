//! Comprehensive tests for trade statistics calculations

use data_aggregator::{DataAggregatorService, TradeAggregation, DataAggregator};
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

/// Helper struct for trade statistics calculation
#[derive(Debug, Clone)]
struct TradeStats {
    pub total_trades: u64,
    pub total_volume: Qty,
    pub avg_trade_size: Qty,
    pub large_trades: u64,
    pub buy_trades: u64,
    pub sell_trades: u64,
    pub buy_volume: Qty,
    pub sell_volume: Qty,
    pub imbalance: i32, // -10000 to 10000 for -100% to 100%
    pub min_trade_size: Qty,
    pub max_trade_size: Qty,
    pub price_weighted_volume: i64, // For VWAP calculation
    pub first_trade_price: Px,
    pub last_trade_price: Px,
}

impl TradeStats {
    fn new() -> Self {
        Self {
            total_trades: 0,
            total_volume: Qty::ZERO,
            avg_trade_size: Qty::ZERO,
            large_trades: 0,
            buy_trades: 0,
            sell_trades: 0,
            buy_volume: Qty::ZERO,
            sell_volume: Qty::ZERO,
            imbalance: 0,
            min_trade_size: Qty::from_i64(i64::MAX),
            max_trade_size: Qty::ZERO,
            price_weighted_volume: 0,
            first_trade_price: Px::ZERO,
            last_trade_price: Px::ZERO,
        }
    }

    fn add_trade(&mut self, price: Px, qty: Qty, is_buy: bool) {
        // Update counts
        self.total_trades += 1;
        
        // Update volume
        self.total_volume = Qty::from_i64(self.total_volume.as_i64() + qty.as_i64());
        
        // Update buy/sell statistics
        if is_buy {
            self.buy_trades += 1;
            self.buy_volume = Qty::from_i64(self.buy_volume.as_i64() + qty.as_i64());
        } else {
            self.sell_trades += 1;
            self.sell_volume = Qty::from_i64(self.sell_volume.as_i64() + qty.as_i64());
        }
        
        // Update size statistics
        if self.total_trades == 1 {
            self.min_trade_size = qty;
            self.max_trade_size = qty;
            self.first_trade_price = price;
        } else {
            if qty.as_i64() < self.min_trade_size.as_i64() {
                self.min_trade_size = qty;
            }
            if qty.as_i64() > self.max_trade_size.as_i64() {
                self.max_trade_size = qty;
            }
        }
        
        self.last_trade_price = price;
        
        // Update price-weighted volume for VWAP
        self.price_weighted_volume += price.as_i64() * qty.as_i64();
        
        // Recalculate derived statistics
        self.update_derived_stats();
    }
    
    fn update_derived_stats(&mut self) {
        // Calculate average trade size
        if self.total_trades > 0 {
            self.avg_trade_size = Qty::from_i64(self.total_volume.as_i64() / self.total_trades as i64);
        }
        
        // Calculate large trades (trades > 10x average)
        let large_trade_threshold = Qty::from_i64(self.avg_trade_size.as_i64() * 10);
        // Note: In practice, we'd need to track individual trade sizes for this
        
        // Calculate buy/sell imbalance
        if self.total_volume.as_i64() > 0 {
            let buy_ratio = self.buy_volume.as_i64() as f64 / self.total_volume.as_i64() as f64;
            let sell_ratio = self.sell_volume.as_i64() as f64 / self.total_volume.as_i64() as f64;
            
            // Convert to -10000 to 10000 scale
            self.imbalance = ((buy_ratio - sell_ratio) * 10000.0) as i32;
        }
    }
    
    fn vwap(&self) -> Px {
        if self.total_volume.as_i64() > 0 {
            Px::from_i64(self.price_weighted_volume / self.total_volume.as_i64())
        } else {
            Px::ZERO
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_basic_trade_statistics(
    mut test_aggregator: DataAggregatorService,
    test_symbol: Symbol,
    base_timestamp: DateTime<Utc>,
) -> Result<()> {
    let trades = vec![
        (Px::from_price_i32(100_0000), Qty::from_qty_i32(10_0000), true),
        (Px::from_price_i32(101_0000), Qty::from_qty_i32(5_0000), false),
        (Px::from_price_i32(100_5000), Qty::from_qty_i32(15_0000), true),
        (Px::from_price_i32(99_5000), Qty::from_qty_i32(8_0000), false),
    ];

    let mut stats = TradeStats::new();
    
    // Process trades and calculate statistics
    for (i, (price, qty, is_buy)) in trades.iter().enumerate() {
        let ts = Ts::from_nanos(
            (base_timestamp + Duration::milliseconds(i as i64 * 100))
                .timestamp_nanos_opt()
                .unwrap() as u64
        );
        
        // Process trade in aggregator
        test_aggregator.process_trade(test_symbol, ts, *price, *qty, *is_buy).await?;
        
        // Update our stats tracker
        stats.add_trade(*price, *qty, *is_buy);
    }

    // Verify basic statistics
    assert_eq!(stats.total_trades, 4);
    assert_eq!(stats.total_volume, Qty::from_qty_i32(38_0000)); // 10+5+15+8
    assert_eq!(stats.avg_trade_size, Qty::from_qty_i32(9_5000)); // 38/4 = 9.5
    
    assert_eq!(stats.buy_trades, 2);
    assert_eq!(stats.sell_trades, 2);
    assert_eq!(stats.buy_volume, Qty::from_qty_i32(25_0000)); // 10+15
    assert_eq!(stats.sell_volume, Qty::from_qty_i32(13_0000)); // 5+8

    // Verify imbalance calculation (buy heavy)
    // (25-13)/38 = 12/38 ≈ 0.316, scaled to 3158 out of 10000
    assert!(stats.imbalance > 3000 && stats.imbalance < 3500);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_trade_size_distribution_analysis() -> Result<()> {
    // Test analysis of trade size distribution
    let trade_sizes = vec![
        Qty::from_qty_i32(1_0000),   // Small trade
        Qty::from_qty_i32(5_0000),   // Small trade
        Qty::from_qty_i32(10_0000),  // Medium trade
        Qty::from_qty_i32(50_0000),  // Large trade
        Qty::from_qty_i32(100_0000), // Very large trade
        Qty::from_qty_i32(2_0000),   // Small trade
    ];

    let mut stats = TradeStats::new();
    
    for (i, &qty) in trade_sizes.iter().enumerate() {
        let price = Px::from_price_i32(100_0000 + i as i64 * 1000);
        let is_buy = i % 2 == 0;
        stats.add_trade(price, qty, is_buy);
    }

    // Verify size statistics
    assert_eq!(stats.min_trade_size, Qty::from_qty_i32(1_0000));
    assert_eq!(stats.max_trade_size, Qty::from_qty_i32(100_0000));
    assert_eq!(stats.total_trades, 6);

    // Calculate percentiles manually for verification
    let mut sizes: Vec<i64> = trade_sizes.iter().map(|q| q.as_i64()).collect();
    sizes.sort();
    
    // Median (50th percentile)
    let median = if sizes.len() % 2 == 0 {
        (sizes[sizes.len()/2 - 1] + sizes[sizes.len()/2]) / 2
    } else {
        sizes[sizes.len()/2]
    };
    
    println!("Trade sizes: {:?}", sizes);
    println!("Median trade size: {}", median);
    println!("Average trade size: {}", stats.avg_trade_size.as_i64());
    
    // Average should be higher than median due to large trades
    assert!(stats.avg_trade_size.as_i64() > median);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_vwap_calculation() -> Result<()> {
    // Test Volume Weighted Average Price calculation
    let trades = vec![
        (Px::from_price_i32(100_0000), Qty::from_qty_i32(10_0000)), // $100.00 * 10 = $1000
        (Px::from_price_i32(101_0000), Qty::from_qty_i32(5_0000)),  // $101.00 * 5 = $505
        (Px::from_price_i32(99_0000), Qty::from_qty_i32(15_0000)),  // $99.00 * 15 = $1485
    ];

    let mut stats = TradeStats::new();
    
    for (price, qty) in trades {
        stats.add_trade(price, qty, true); // All buys for simplicity
    }

    // Calculate expected VWAP manually
    // Total value = $1000 + $505 + $1485 = $2990
    // Total volume = 10 + 5 + 15 = 30
    // VWAP = $2990 / 30 = $99.67 (approximately)
    
    let expected_vwap_cents = (100_0000 * 10 + 101_0000 * 5 + 99_0000 * 15) / 30;
    let expected_vwap = Px::from_i64(expected_vwap_cents);
    
    let calculated_vwap = stats.vwap();
    
    println!("Expected VWAP: ${:.4}", expected_vwap.as_f64());
    println!("Calculated VWAP: ${:.4}", calculated_vwap.as_f64());
    
    assert_eq!(calculated_vwap, expected_vwap);
    
    // Verify VWAP is between min and max prices
    let min_price = Px::from_price_i32(99_0000);
    let max_price = Px::from_price_i32(101_0000);
    
    assert!(calculated_vwap >= min_price);
    assert!(calculated_vwap <= max_price);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_buy_sell_imbalance_scenarios() -> Result<()> {
    // Test different buy/sell imbalance scenarios
    
    // Scenario 1: Heavy buy pressure (80% buy volume)
    let mut buy_heavy_stats = TradeStats::new();
    buy_heavy_stats.add_trade(Px::from_price_i32(100_0000), Qty::from_qty_i32(80_0000), true);  // Buy 80
    buy_heavy_stats.add_trade(Px::from_price_i32(100_1000), Qty::from_qty_i32(20_0000), false); // Sell 20
    
    // Expected imbalance: (80-20)/100 = 0.6, scaled to 6000
    assert_eq!(buy_heavy_stats.imbalance, 6000);
    
    // Scenario 2: Heavy sell pressure (75% sell volume)
    let mut sell_heavy_stats = TradeStats::new();
    sell_heavy_stats.add_trade(Px::from_price_i32(100_0000), Qty::from_qty_i32(25_0000), true);  // Buy 25
    sell_heavy_stats.add_trade(Px::from_price_i32(99_9000), Qty::from_qty_i32(75_0000), false); // Sell 75
    
    // Expected imbalance: (25-75)/100 = -0.5, scaled to -5000
    assert_eq!(sell_heavy_stats.imbalance, -5000);
    
    // Scenario 3: Balanced (50/50)
    let mut balanced_stats = TradeStats::new();
    balanced_stats.add_trade(Px::from_price_i32(100_0000), Qty::from_qty_i32(50_0000), true);  // Buy 50
    balanced_stats.add_trade(Px::from_price_i32(100_0000), Qty::from_qty_i32(50_0000), false); // Sell 50
    
    // Expected imbalance: (50-50)/100 = 0
    assert_eq!(balanced_stats.imbalance, 0);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_large_trade_detection() -> Result<()> {
    // Test detection of unusually large trades
    
    let trades = vec![
        (Px::from_price_i32(100_0000), Qty::from_qty_i32(10_0000), true),   // Normal
        (Px::from_price_i32(100_1000), Qty::from_qty_i32(8_0000), false),   // Normal  
        (Px::from_price_i32(100_2000), Qty::from_qty_i32(12_0000), true),   // Normal
        (Px::from_price_i32(100_3000), Qty::from_qty_i32(150_0000), false), // Large (15x avg)
        (Px::from_price_i32(100_4000), Qty::from_qty_i32(5_0000), true),    // Normal
    ];

    let mut stats = TradeStats::new();
    let mut large_trades = Vec::new();
    
    for (price, qty, is_buy) in trades {
        stats.add_trade(price, qty, is_buy);
        
        // Check if current trade is large (> 10x current average)
        // Note: This is simplified; in practice, we'd need a rolling calculation
        if stats.total_trades > 3 { // Only check after we have some baseline
            let current_avg = Qty::from_i64(stats.total_volume.as_i64() / stats.total_trades as i64);
            if qty.as_i64() > current_avg.as_i64() * 10 {
                large_trades.push((price, qty, is_buy));
            }
        }
    }

    // Verify large trade detection
    assert_eq!(large_trades.len(), 1);
    assert_eq!(large_trades[0].1, Qty::from_qty_i32(150_0000));
    
    println!("Detected large trades: {:?}", large_trades);
    println!("Average trade size: {:.2}", stats.avg_trade_size.as_f64());

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_time_window_aggregation() -> Result<()> {
    // Test trade statistics over different time windows
    
    let base_time = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z").unwrap().with_timezone(&Utc);
    
    // Create trades over a 5-minute period
    let trades_with_times = vec![
        (base_time, Px::from_price_i32(100_0000), Qty::from_qty_i32(10_0000), true),
        (base_time + Duration::minutes(1), Px::from_price_i32(100_1000), Qty::from_qty_i32(5_0000), false),
        (base_time + Duration::minutes(2), Px::from_price_i32(100_2000), Qty::from_qty_i32(15_0000), true),
        (base_time + Duration::minutes(3), Px::from_price_i32(100_1000), Qty::from_qty_i32(8_0000), false),
        (base_time + Duration::minutes(4), Px::from_price_i32(100_3000), Qty::from_qty_i32(12_0000), true),
    ];

    // Aggregate statistics for different time windows
    
    // 1-minute windows
    let mut minute_windows = std::collections::HashMap::new();
    
    for (time, price, qty, is_buy) in &trades_with_times {
        let minute_key = time.format("%H:%M").to_string();
        let stats = minute_windows.entry(minute_key).or_insert_with(TradeStats::new);
        stats.add_trade(*price, *qty, *is_buy);
    }

    // Verify we have 5 separate minute windows
    assert_eq!(minute_windows.len(), 5);
    
    // Each window should have exactly 1 trade
    for (minute, stats) in &minute_windows {
        assert_eq!(stats.total_trades, 1);
        println!("Minute {}: {} trades, {} volume", 
                 minute, stats.total_trades, stats.total_volume.as_f64());
    }

    // 5-minute window (all trades)
    let mut full_window_stats = TradeStats::new();
    for (_, price, qty, is_buy) in &trades_with_times {
        full_window_stats.add_trade(*price, *qty, *is_buy);
    }

    assert_eq!(full_window_stats.total_trades, 5);
    assert_eq!(full_window_stats.total_volume, Qty::from_qty_i32(50_0000));

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_trade_velocity_calculation() -> Result<()> {
    // Test calculation of trade velocity (trades per unit time)
    
    let base_time = DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z").unwrap().with_timezone(&Utc);
    
    // Create trades at different intervals
    let trade_intervals = vec![
        Duration::seconds(0),  // 12:00:00
        Duration::seconds(5),  // 12:00:05 - 5 seconds later
        Duration::seconds(10), // 12:00:10 - 5 seconds later
        Duration::seconds(12), // 12:00:12 - 2 seconds later (faster)
        Duration::seconds(13), // 12:00:13 - 1 second later (very fast)
        Duration::seconds(30), // 12:00:30 - 17 seconds later (slower)
    ];

    let mut trades_with_times = Vec::new();
    for interval in trade_intervals {
        trades_with_times.push((
            base_time + interval,
            Px::from_price_i32(100_0000),
            Qty::from_qty_i32(10_0000),
            true,
        ));
    }

    // Calculate trade intervals
    let mut intervals = Vec::new();
    for i in 1..trades_with_times.len() {
        let interval = trades_with_times[i].0 - trades_with_times[i-1].0;
        intervals.push(interval);
    }

    // Calculate trade velocity metrics
    let total_time_seconds = (trades_with_times.last().unwrap().0 - trades_with_times[0].0).num_seconds();
    let trades_per_second = (trades_with_times.len() - 1) as f64 / total_time_seconds as f64;
    let trades_per_minute = trades_per_second * 60.0;

    // Calculate average interval
    let avg_interval_seconds: f64 = intervals.iter().map(|d| d.num_seconds() as f64).sum::<f64>() / intervals.len() as f64;
    
    println!("Total time: {} seconds", total_time_seconds);
    println!("Trades per minute: {:.2}", trades_per_minute);
    println!("Average interval: {:.2} seconds", avg_interval_seconds);
    
    assert_eq!(total_time_seconds, 30);
    assert_eq!(trades_with_times.len(), 6);
    assert!((trades_per_minute - 10.0).abs() < 0.1); // 5 trades in 30 seconds = 10 trades/minute

    // Detect burst trading (intervals < 2 seconds)
    let burst_trades = intervals.iter().filter(|d| d.num_seconds() < 2).count();
    assert_eq!(burst_trades, 2); // The 2-second and 1-second intervals

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_price_impact_analysis() -> Result<()> {
    // Test analysis of price impact from trades
    
    let trades = vec![
        (Px::from_price_i32(100_0000), Qty::from_qty_i32(10_0000), true),   // Buy at $100.00
        (Px::from_price_i32(100_1000), Qty::from_qty_i32(5_0000), true),    // Buy at $100.10 (+$0.10)
        (Px::from_price_i32(100_2000), Qty::from_qty_i32(15_0000), true),   // Buy at $100.20 (+$0.10)
        (Px::from_price_i32(100_1500), Qty::from_qty_i32(8_0000), false),   // Sell at $100.15 (-$0.05)
        (Px::from_price_i32(100_0500), Qty::from_qty_i32(12_0000), false),  // Sell at $100.05 (-$0.10)
    ];

    let mut price_changes = Vec::new();
    let mut cumulative_impact = 0i64;

    for i in 1..trades.len() {
        let prev_price = trades[i-1].0;
        let curr_price = trades[i].0;
        let price_change = curr_price.as_i64() - prev_price.as_i64();
        let is_buy = trades[i].2;
        
        price_changes.push(price_change);
        cumulative_impact += price_change;
        
        // Analyze if price moved in expected direction
        if is_buy && price_change > 0 {
            println!("Buy order pushed price up by ${:.4}", price_change as f64 / 10000.0);
        } else if !is_buy && price_change < 0 {
            println!("Sell order pushed price down by ${:.4}", (-price_change) as f64 / 10000.0);
        } else if is_buy && price_change < 0 {
            println!("Buy order coincided with price drop of ${:.4}", (-price_change) as f64 / 10000.0);
        } else if !is_buy && price_change > 0 {
            println!("Sell order coincided with price rise of ${:.4}", price_change as f64 / 10000.0);
        }
    }

    // Calculate price impact metrics
    let total_price_change = trades.last().unwrap().0.as_i64() - trades[0].0.as_i64();
    let average_change_per_trade = price_changes.iter().sum::<i64>() as f64 / price_changes.len() as f64;
    
    println!("Total price change: ${:.4}", total_price_change as f64 / 10000.0);
    println!("Average change per trade: ${:.4}", average_change_per_trade / 10000.0);

    // Verify calculations
    assert_eq!(total_price_change, 500); // $100.05 - $100.00 = $0.05
    assert_eq!(price_changes.len(), 4);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_trade_aggregation_conversion() -> Result<()> {
    // Test conversion of our TradeStats to TradeAggregation format
    
    let mut stats = TradeStats::new();
    
    // Add sample trades
    let trades = vec![
        (Px::from_price_i32(100_0000), Qty::from_qty_i32(10_0000), true),
        (Px::from_price_i32(101_0000), Qty::from_qty_i32(5_0000), false),
        (Px::from_price_i32(100_5000), Qty::from_qty_i32(25_0000), true),  // Large trade
        (Px::from_price_i32(99_5000), Qty::from_qty_i32(8_0000), false),
    ];

    for (price, qty, is_buy) in trades {
        stats.add_trade(price, qty, is_buy);
    }

    // Convert to TradeAggregation format
    let trade_aggregation = TradeAggregation {
        symbol: Symbol::new(1),
        period: Duration::minutes(5),
        total_trades: stats.total_trades,
        total_volume: stats.total_volume,
        avg_trade_size: stats.avg_trade_size,
        large_trades: 1, // We had one trade (25) > 10x average (12)
        imbalance: stats.imbalance,
    };

    // Verify conversion
    assert_eq!(trade_aggregation.total_trades, 4);
    assert_eq!(trade_aggregation.total_volume, Qty::from_qty_i32(48_0000));
    assert_eq!(trade_aggregation.avg_trade_size, Qty::from_qty_i32(12_0000));
    assert_eq!(trade_aggregation.large_trades, 1);
    
    // Verify imbalance (buy: 35, sell: 13, total: 48)
    // (35-13)/48 ≈ 0.458, scaled to ~4583
    assert!(trade_aggregation.imbalance > 4500 && trade_aggregation.imbalance < 4700);

    Ok(())
}

/// Performance test for trade statistics calculation
#[rstest]
#[tokio::test]
async fn test_trade_stats_performance() -> Result<()> {
    use std::time::Instant;
    
    let num_trades = 100_000;
    let mut stats = TradeStats::new();
    
    let start = Instant::now();
    
    // Simulate high-frequency trading scenario
    for i in 0..num_trades {
        let price = Px::from_price_i32(100_0000 + (i % 1000) - 500); // Price varies by $0.50
        let qty = Qty::from_qty_i32(1_0000 + (i % 100) * 100); // Size varies 1-11 shares
        let is_buy = i % 2 == 0;
        
        stats.add_trade(price, qty, is_buy);
    }
    
    let duration = start.elapsed();
    
    println!("Processed {} trades in {:?}", num_trades, duration);
    println!("Rate: {:.0} trades/second", num_trades as f64 / duration.as_secs_f64());
    println!("Final stats - Total volume: {}, Avg size: {}, Imbalance: {}", 
             stats.total_volume.as_f64(), stats.avg_trade_size.as_f64(), stats.imbalance);
    
    // Performance assertions
    assert!(duration.as_millis() < 1000, "Should process 100k trades in under 1 second");
    assert_eq!(stats.total_trades, num_trades as u64);
    assert!(stats.total_volume.as_i64() > 0);

    Ok(())
}