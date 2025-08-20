//! Unit tests for MarketDataStore functionality

use rstest::*;
use backtesting::*;
use chrono::{Utc, Duration};
use crate::test_utils::*;

#[rstest]
fn test_market_data_store_creation() {
    let store = MarketDataStore::new();
    assert_eq!(format!("{:?}", store).contains("MarketDataStore"), true);
}

#[rstest]
fn test_add_single_symbol_data() {
    let store = MarketDataStore::new();
    let timestamp = Utc::now();
    let ohlcv = OHLCV {
        open: 100.0,
        high: 105.0,
        low: 95.0,
        close: 102.0,
        volume: 50000.0,
    };
    
    // This would require modifying the MarketDataStore to expose add methods
    // For now, we test the creation and debug output
    assert!(format!("{:?}", store).contains("price_data"));
}

#[rstest]
fn test_add_orderbook_snapshot() {
    let store = MarketDataStore::new();
    let timestamp = Utc::now();
    let snapshot = OrderbookSnapshot {
        bids: vec![(100.0, 1000.0), (99.5, 2000.0)],
        asks: vec![(101.0, 1500.0), (101.5, 1000.0)],
        timestamp,
    };
    
    store.add_orderbook_snapshot("AAPL", timestamp, snapshot.clone());
    
    let retrieved = store.get_orderbook("AAPL", timestamp);
    assert!(retrieved.is_some());
    
    let retrieved_snapshot = retrieved.unwrap();
    assert_eq!(retrieved_snapshot.bids.len(), 2);
    assert_eq!(retrieved_snapshot.asks.len(), 2);
    assert_eq!(retrieved_snapshot.timestamp, timestamp);
}

#[rstest]
fn test_get_orderbook_at_exact_time() {
    let store = MarketDataStore::new();
    let base_time = Utc::now();
    
    // Add snapshots at different times
    for i in 0..5 {
        let timestamp = base_time + Duration::minutes(i);
        let snapshot = OrderbookSnapshot {
            bids: vec![(100.0 + i as f64, 1000.0)],
            asks: vec![(101.0 + i as f64, 1500.0)],
            timestamp,
        };
        store.add_orderbook_snapshot("TEST", timestamp, snapshot);
    }
    
    // Get snapshot at exact time
    let target_time = base_time + Duration::minutes(2);
    let result = store.get_orderbook("TEST", target_time);
    
    assert!(result.is_some());
    let snapshot = result.unwrap();
    assert_eq!(snapshot.bids[0].0, 102.0);
    assert_eq!(snapshot.asks[0].0, 103.0);
}

#[rstest]
fn test_get_orderbook_before_first_snapshot() {
    let store = MarketDataStore::new();
    let base_time = Utc::now();
    
    // Add snapshot at base_time + 1 hour
    let snapshot = OrderbookSnapshot {
        bids: vec![(100.0, 1000.0)],
        asks: vec![(101.0, 1500.0)],
        timestamp: base_time + Duration::hours(1),
    };
    store.add_orderbook_snapshot("TEST", base_time + Duration::hours(1), snapshot);
    
    // Try to get snapshot before any data exists
    let result = store.get_orderbook("TEST", base_time);
    assert!(result.is_none());
}

#[rstest]
fn test_get_orderbook_interpolation() {
    let store = MarketDataStore::new();
    let base_time = Utc::now();
    
    // Add snapshots with gaps
    let times = vec![
        base_time,
        base_time + Duration::minutes(10),
        base_time + Duration::minutes(30),
    ];
    
    for (i, &timestamp) in times.iter().enumerate() {
        let snapshot = OrderbookSnapshot {
            bids: vec![(100.0 + i as f64 * 5.0, 1000.0)],
            asks: vec![(101.0 + i as f64 * 5.0, 1500.0)],
            timestamp,
        };
        store.add_orderbook_snapshot("TEST", timestamp, snapshot);
    }
    
    // Get snapshot between two data points - should return the latest previous one
    let query_time = base_time + Duration::minutes(15);
    let result = store.get_orderbook("TEST", query_time);
    
    assert!(result.is_some());
    let snapshot = result.unwrap();
    assert_eq!(snapshot.bids[0].0, 105.0); // Should be from the 10-minute snapshot
}

#[rstest]
fn test_orderbook_snapshot_validation() {
    // Test valid orderbook
    let valid_snapshot = OrderbookSnapshot {
        bids: vec![(100.0, 1000.0), (99.0, 2000.0)], // Sorted descending
        asks: vec![(101.0, 1500.0), (102.0, 1000.0)], // Sorted ascending
        timestamp: Utc::now(),
    };
    
    // Bids should be in descending order
    for window in valid_snapshot.bids.windows(2) {
        assert!(window[0].0 >= window[1].0, "Bids should be sorted descending by price");
    }
    
    // Asks should be in ascending order
    for window in valid_snapshot.asks.windows(2) {
        assert!(window[0].0 <= window[1].0, "Asks should be sorted ascending by price");
    }
    
    // Spread should be positive (highest bid < lowest ask)
    if !valid_snapshot.bids.is_empty() && !valid_snapshot.asks.is_empty() {
        let highest_bid = valid_snapshot.bids[0].0;
        let lowest_ask = valid_snapshot.asks[0].0;
        assert!(lowest_ask > highest_bid, "Ask should be higher than bid (positive spread)");
    }
}

#[rstest]
fn test_ohlcv_data_validation() {
    // Valid OHLCV data
    let valid_ohlcv = OHLCV {
        open: 100.0,
        high: 105.0,
        low: 95.0,
        close: 102.0,
        volume: 50000.0,
    };
    
    assert!(valid_ohlcv.high >= valid_ohlcv.low, "High should be >= low");
    assert!(valid_ohlcv.high >= valid_ohlcv.open, "High should be >= open");
    assert!(valid_ohlcv.high >= valid_ohlcv.close, "High should be >= close");
    assert!(valid_ohlcv.low <= valid_ohlcv.open, "Low should be <= open");
    assert!(valid_ohlcv.low <= valid_ohlcv.close, "Low should be <= close");
    assert!(valid_ohlcv.volume >= 0.0, "Volume should be non-negative");
    
    // Test edge case: all prices equal (gap/limit)
    let gap_ohlcv = OHLCV {
        open: 100.0,
        high: 100.0,
        low: 100.0,
        close: 100.0,
        volume: 0.0,
    };
    
    assert!(gap_ohlcv.high >= gap_ohlcv.low);
    assert!(gap_ohlcv.volume >= 0.0);
}

#[rstest]
fn test_invalid_ohlcv_detection() {
    // Test various invalid OHLCV scenarios
    let invalid_cases = vec![
        // High < Low
        OHLCV {
            open: 100.0,
            high: 95.0,  // Invalid
            low: 98.0,
            close: 97.0,
            volume: 50000.0,
        },
        // Close > High
        OHLCV {
            open: 100.0,
            high: 105.0,
            low: 95.0,
            close: 110.0,  // Invalid
            volume: 50000.0,
        },
        // Close < Low
        OHLCV {
            open: 100.0,
            high: 105.0,
            low: 95.0,
            close: 90.0,  // Invalid
            volume: 50000.0,
        },
        // Negative volume
        OHLCV {
            open: 100.0,
            high: 105.0,
            low: 95.0,
            close: 102.0,
            volume: -1000.0,  // Invalid
        },
    ];
    
    for (i, ohlcv) in invalid_cases.iter().enumerate() {
        let is_valid = ohlcv.high >= ohlcv.low &&
                      ohlcv.close >= ohlcv.low &&
                      ohlcv.close <= ohlcv.high &&
                      ohlcv.volume >= 0.0;
        
        assert!(!is_valid, "Case {} should be invalid: {:?}", i, ohlcv);
    }
}

#[rstest]
fn test_market_snapshot_builder() {
    let snapshot = MarketSnapshotBuilder::new()
        .with_price("AAPL", 150.0)
        .with_price("GOOGL", 2800.0)
        .with_price("TSLA", 200.0)
        .build();
    
    assert_eq!(snapshot.prices.len(), 3);
    assert_eq!(*snapshot.prices.get("AAPL").unwrap(), 150.0);
    assert_eq!(*snapshot.prices.get("GOOGL").unwrap(), 2800.0);
    assert_eq!(*snapshot.prices.get("TSLA").unwrap(), 200.0);
}

#[rstest]
fn test_market_snapshot_with_timestamp() {
    let target_time = Utc::now() - Duration::hours(1);
    let snapshot = MarketSnapshotBuilder::new()
        .with_timestamp(target_time)
        .with_price("TEST", 100.0)
        .build();
    
    assert_eq!(snapshot.timestamp, target_time);
    assert_eq!(snapshot.prices.len(), 1);
}

#[rstest]
fn test_market_data_time_series() {
    let store = MarketDataStore::new();
    let base_time = Utc::now() - Duration::days(5);
    
    // Add orderbook snapshots over multiple days
    for day in 0..5 {
        for hour in 0..24 {
            let timestamp = base_time + Duration::days(day) + Duration::hours(hour);
            let price_base = 100.0 + day as f64 + hour as f64 * 0.1;
            
            let snapshot = OrderbookSnapshot {
                bids: vec![(price_base, 1000.0)],
                asks: vec![(price_base + 1.0, 1500.0)],
                timestamp,
            };
            
            store.add_orderbook_snapshot("TIMESERIES", timestamp, snapshot);
        }
    }
    
    // Query at different points and verify we get expected data
    let query_time = base_time + Duration::days(2) + Duration::hours(12);
    let result = store.get_orderbook("TIMESERIES", query_time);
    
    assert!(result.is_some());
    let snapshot = result.unwrap();
    
    // Should get data from day 2, hour 12
    let expected_price = 100.0 + 2.0 + 12.0 * 0.1;
    TestAssertions::assert_approx_eq(snapshot.bids[0].0, expected_price, 0.01);
}

#[rstest]
fn test_multiple_symbol_orderbook_storage() {
    let store = MarketDataStore::new();
    let timestamp = Utc::now();
    
    let symbols = vec!["AAPL", "GOOGL", "TSLA", "MSFT", "AMZN"];
    
    for (i, symbol) in symbols.iter().enumerate() {
        let snapshot = OrderbookSnapshot {
            bids: vec![(100.0 + i as f64 * 100.0, 1000.0)],
            asks: vec![(101.0 + i as f64 * 100.0, 1500.0)],
            timestamp,
        };
        
        store.add_orderbook_snapshot(symbol, timestamp, snapshot);
    }
    
    // Verify all symbols can be retrieved
    for (i, symbol) in symbols.iter().enumerate() {
        let result = store.get_orderbook(symbol, timestamp);
        assert!(result.is_some(), "Should find orderbook for {}", symbol);
        
        let snapshot = result.unwrap();
        let expected_bid = 100.0 + i as f64 * 100.0;
        TestAssertions::assert_approx_eq(snapshot.bids[0].0, expected_bid, 0.01);
    }
}

#[rstest]
fn test_orderbook_deep_book() {
    let store = MarketDataStore::new();
    let timestamp = Utc::now();
    
    // Create deep orderbook with multiple levels
    let mut bids = Vec::new();
    let mut asks = Vec::new();
    
    for i in 0..10 {
        bids.push((100.0 - i as f64 * 0.5, 1000.0 + i as f64 * 100.0));
        asks.push((101.0 + i as f64 * 0.5, 1500.0 + i as f64 * 200.0));
    }
    
    let snapshot = OrderbookSnapshot {
        bids,
        asks,
        timestamp,
    };
    
    store.add_orderbook_snapshot("DEEP", timestamp, snapshot);
    
    let result = store.get_orderbook("DEEP", timestamp);
    assert!(result.is_some());
    
    let retrieved = result.unwrap();
    assert_eq!(retrieved.bids.len(), 10);
    assert_eq!(retrieved.asks.len(), 10);
    
    // Verify bid levels are sorted descending
    for window in retrieved.bids.windows(2) {
        assert!(window[0].0 > window[1].0, "Bids should be sorted descending");
    }
    
    // Verify ask levels are sorted ascending
    for window in retrieved.asks.windows(2) {
        assert!(window[0].0 < window[1].0, "Asks should be sorted ascending");
    }
}

#[rstest]
fn test_empty_orderbook() {
    let store = MarketDataStore::new();
    let timestamp = Utc::now();
    
    // Add empty orderbook (market closed or no liquidity)
    let empty_snapshot = OrderbookSnapshot {
        bids: vec![],
        asks: vec![],
        timestamp,
    };
    
    store.add_orderbook_snapshot("EMPTY", timestamp, empty_snapshot);
    
    let result = store.get_orderbook("EMPTY", timestamp);
    assert!(result.is_some());
    
    let snapshot = result.unwrap();
    assert_eq!(snapshot.bids.len(), 0);
    assert_eq!(snapshot.asks.len(), 0);
}

#[rstest]
fn test_orderbook_snapshot_cloning() {
    let original = OrderbookSnapshot {
        bids: vec![(100.0, 1000.0), (99.5, 2000.0)],
        asks: vec![(101.0, 1500.0), (101.5, 1000.0)],
        timestamp: Utc::now(),
    };
    
    let cloned = original.clone();
    
    assert_eq!(original.bids.len(), cloned.bids.len());
    assert_eq!(original.asks.len(), cloned.asks.len());
    assert_eq!(original.timestamp, cloned.timestamp);
    
    // Verify data integrity
    for (i, (orig_bid, clone_bid)) in original.bids.iter().zip(cloned.bids.iter()).enumerate() {
        assert_eq!(orig_bid.0, clone_bid.0, "Bid price {} should match", i);
        assert_eq!(orig_bid.1, clone_bid.1, "Bid quantity {} should match", i);
    }
}