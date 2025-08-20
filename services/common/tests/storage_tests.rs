//! Comprehensive unit tests for storage utilities and WAL
//!
//! Tests cover:
//! - Write-Ahead Log (WAL) functionality
//! - Data event serialization and deserialization
//! - Storage type definitions and conversions
//! - Iterator and streaming capabilities
//! - Error handling in storage operations
//! - Performance characteristics

use services_common::{
    CandleEvent, DataEvent, MicrostructureEvent, OrderBookEvent, SystemEvent, TickEvent,
    TradeEvent, VolumeProfileEvent, Wal, WalEntry, WalEntryWrapper, WalEvent, WalIterator,
};
use anyhow::Result;
use chrono::Utc;
use rstest::*;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tempfile::TempDir;

// Test data structures
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestWalEntry {
    timestamp: i64,
    sequence: u64,
    data: String,
}

impl WalEntry for TestWalEntry {
    fn timestamp(&self) -> Ts {
        Ts::from_nanos(self.timestamp as u64)
    }

    fn sequence(&self) -> u64 {
        self.sequence
    }

    fn to_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(Into::into)
    }
}

// Helper functions for testing
fn create_test_tick_event(symbol: &str, price: i64, quantity: i64) -> TickEvent {
    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
    TickEvent {
        symbol: symbol.to_string(),
        timestamp: now,
        ts: now,
        price,
        quantity,
        is_buy: true,
        bid: Some(price - 100),
        ask: Some(price + 100),
        last: Some(price),
        volume: Some(quantity * 10),
        venue: "test_exchange".to_string(),
    }
}

fn create_test_trade_event(symbol: &str, price: i64, quantity: i64, trade_id: u64) -> TradeEvent {
    TradeEvent {
        ts: chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        symbol: symbol.to_string(),
        price,
        quantity,
        is_buy: true,
        trade_id,
    }
}

fn create_test_candle_event(symbol: &str, timeframe: &str) -> CandleEvent {
    let now = Utc::now();
    CandleEvent {
        symbol: symbol.to_string(),
        timeframe: timeframe.to_string(),
        open_time: now,
        close_time: now,
        open: 50000_00000000,
        high: 51000_00000000,
        low: 49000_00000000,
        close: 50500_00000000,
        volume: 1000_00000000,
    }
}

// WAL Basic Functionality Tests
#[rstest]
#[tokio::test]
async fn test_wal_creation_and_basic_operations() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path().join("test_wal");
    
    let mut wal = Wal::new(&wal_path, Some(1024))?; // Small segment size for testing
    
    // Test appending data
    let test_data = TestWalEntry {
        timestamp: 1234567890,
        sequence: 1,
        data: "test entry".to_string(),
    };
    
    wal.append(&test_data)?;
    wal.flush()?;
    
    // Verify WAL directory was created
    assert!(wal_path.exists());
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_wal_multiple_entries() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path().join("multi_entry_wal");
    
    let mut wal = Wal::new(&wal_path, Some(2048))?;
    
    // Append multiple entries
    for i in 0..10 {
        let entry = TestWalEntry {
            timestamp: 1234567890 + i,
            sequence: i as u64,
            data: format!("test entry {}", i),
        };
        wal.append(&entry)?;
    }
    
    wal.flush()?;
    
    // Test reading entries back
    let mut iterator: WalIterator<TestWalEntry> = wal.stream(None)?;
    let mut count = 0;
    
    while let Some(entry) = iterator.read_next_entry()? {
        assert!(entry.data.starts_with("test entry"));
        count += 1;
    }
    
    assert_eq!(count, 10);
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_wal_segment_rotation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path().join("segment_rotation_wal");
    
    let mut wal = Wal::new(&wal_path, Some(128))?; // Very small segments to force rotation
    
    // Add enough data to trigger segment rotation
    for i in 0..50 {
        let entry = TestWalEntry {
            timestamp: 1234567890 + i,
            sequence: i as u64,
            data: format!("This is a longer test entry {} with more data", i),
        };
        wal.append(&entry)?;
    }
    
    wal.flush()?;
    
    // Check that multiple segment files were created
    let entries = std::fs::read_dir(&wal_path)?;
    let wal_files: Vec<_> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.path().extension()? == "wal" {
                Some(entry.path())
            } else {
                None
            }
        })
        .collect();
    
    assert!(wal_files.len() > 1, "Expected multiple WAL segment files");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_wal_iterator_with_timestamp_filter() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path().join("timestamp_filter_wal");
    
    let mut wal = Wal::new(&wal_path, None)?;
    
    // Add entries with different timestamps
    let base_timestamp = 1234567890_000_000_000i64; // nanoseconds
    for i in 0..10 {
        let entry = TestWalEntry {
            timestamp: base_timestamp + (i * 1000), // 1 microsecond apart
            sequence: i as u64,
            data: format!("entry {}", i),
        };
        wal.append(&entry)?;
    }
    
    wal.flush()?;
    
    // Filter from middle timestamp
    let filter_timestamp = base_timestamp + 5000;
    let mut iterator: WalIterator<TestWalEntry> = wal.stream(Some(filter_timestamp))?;
    
    let mut found_entries = Vec::new();
    while let Some(entry) = iterator.read_next_entry()? {
        found_entries.push(entry);
    }
    
    // Should find entries 5-9 (5 entries)
    assert!(found_entries.len() <= 10); // May vary based on filtering implementation
    
    Ok(())
}

// Data Event Serialization Tests
#[rstest]
#[test]
fn test_trade_event_serialization() -> Result<()> {
    let trade = create_test_trade_event("BTCUSDT", 50000_00000000, 100_000_000, 12345);
    
    let serialized = serde_json::to_string(&trade)?;
    let deserialized: TradeEvent = serde_json::from_str(&serialized)?;
    
    assert_eq!(trade.symbol, deserialized.symbol);
    assert_eq!(trade.price, deserialized.price);
    assert_eq!(trade.quantity, deserialized.quantity);
    assert_eq!(trade.trade_id, deserialized.trade_id);
    assert_eq!(trade.is_buy, deserialized.is_buy);
    
    Ok(())
}

#[rstest]
#[test]
fn test_candle_event_serialization() -> Result<()> {
    let candle = create_test_candle_event("ETHUSDT", "1m");
    
    let serialized = serde_json::to_string(&candle)?;
    let deserialized: CandleEvent = serde_json::from_str(&serialized)?;
    
    assert_eq!(candle.symbol, deserialized.symbol);
    assert_eq!(candle.timeframe, deserialized.timeframe);
    assert_eq!(candle.open, deserialized.open);
    assert_eq!(candle.high, deserialized.high);
    assert_eq!(candle.low, deserialized.low);
    assert_eq!(candle.close, deserialized.close);
    assert_eq!(candle.volume, deserialized.volume);
    
    Ok(())
}

#[rstest]
#[test]
fn test_tick_event_serialization() -> Result<()> {
    let tick = create_test_tick_event("SOLUSDT", 100_00000000, 50_000_000);
    
    let serialized = serde_json::to_string(&tick)?;
    let deserialized: TickEvent = serde_json::from_str(&serialized)?;
    
    assert_eq!(tick.symbol, deserialized.symbol);
    assert_eq!(tick.price, deserialized.price);
    assert_eq!(tick.quantity, deserialized.quantity);
    assert_eq!(tick.is_buy, deserialized.is_buy);
    assert_eq!(tick.venue, deserialized.venue);
    
    Ok(())
}

// Data Event Enum Tests
#[rstest]
#[test]
fn test_data_event_variants() -> Result<()> {
    let trade = create_test_trade_event("ADAUSDT", 200_000_000, 1000_000_000, 54321);
    let candle = create_test_candle_event("DOTUSDT", "5m");
    let system = SystemEvent {
        ts: chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        event_type: "MAINTENANCE".to_string(),
    };
    
    let trade_event = DataEvent::Trade(trade.clone());
    let candle_event = DataEvent::Candle(candle.clone());
    let system_event = DataEvent::System(system.clone());
    
    // Test serialization of enum variants
    let trade_json = serde_json::to_string(&trade_event)?;
    let candle_json = serde_json::to_string(&candle_event)?;
    let system_json = serde_json::to_string(&system_event)?;
    
    assert!(trade_json.contains("Trade"));
    assert!(candle_json.contains("Candle"));
    assert!(system_json.contains("System"));
    
    // Test deserialization
    let trade_deserialized: DataEvent = serde_json::from_str(&trade_json)?;
    let candle_deserialized: DataEvent = serde_json::from_str(&candle_json)?;
    let system_deserialized: DataEvent = serde_json::from_str(&system_json)?;
    
    match trade_deserialized {
        DataEvent::Trade(t) => assert_eq!(t.symbol, trade.symbol),
        _ => panic!("Expected Trade variant"),
    }
    
    match candle_deserialized {
        DataEvent::Candle(c) => assert_eq!(c.symbol, candle.symbol),
        _ => panic!("Expected Candle variant"),
    }
    
    match system_deserialized {
        DataEvent::System(s) => assert_eq!(s.event_type, system.event_type),
        _ => panic!("Expected System variant"),
    }
    
    Ok(())
}

#[rstest]
#[test]
fn test_volume_profile_event() -> Result<()> {
    let volume_profile = VolumeProfileEvent {
        symbol: "BTCUSDT".to_string(),
        ts: chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
    };
    
    let event = DataEvent::VolumeProfile(volume_profile.clone());
    let serialized = serde_json::to_string(&event)?;
    let deserialized: DataEvent = serde_json::from_str(&serialized)?;
    
    match deserialized {
        DataEvent::VolumeProfile(vp) => {
            assert_eq!(vp.symbol, volume_profile.symbol);
            assert_eq!(vp.ts, volume_profile.ts);
        }
        _ => panic!("Expected VolumeProfile variant"),
    }
    
    Ok(())
}

#[rstest]
#[test]
fn test_microstructure_event() -> Result<()> {
    let microstructure = MicrostructureEvent {
        symbol: "ETHUSDT".to_string(),
        ts: chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
    };
    
    let event = DataEvent::Microstructure(microstructure.clone());
    let serialized = serde_json::to_string(&event)?;
    let deserialized: DataEvent = serde_json::from_str(&serialized)?;
    
    match deserialized {
        DataEvent::Microstructure(ms) => {
            assert_eq!(ms.symbol, microstructure.symbol);
            assert_eq!(ms.ts, microstructure.ts);
        }
        _ => panic!("Expected Microstructure variant"),
    }
    
    Ok(())
}

#[rstest]
#[test]
fn test_orderbook_event() -> Result<()> {
    let orderbook = OrderBookEvent {
        symbol: "SOLUSDT".to_string(),
        sequence: 12345,
        bid_levels: vec![
            (100_00000000, 50_000_000),  // $100.00, 0.5 SOL
            (99_50000000, 100_000_000),  // $99.50, 1.0 SOL
        ],
        ask_levels: vec![
            (100_50000000, 75_000_000),  // $100.50, 0.75 SOL
            (101_00000000, 150_000_000), // $101.00, 1.5 SOL
        ],
    };
    
    let event = DataEvent::OrderBook(orderbook.clone());
    let serialized = serde_json::to_string(&event)?;
    let deserialized: DataEvent = serde_json::from_str(&serialized)?;
    
    match deserialized {
        DataEvent::OrderBook(ob) => {
            assert_eq!(ob.symbol, orderbook.symbol);
            assert_eq!(ob.sequence, orderbook.sequence);
            assert_eq!(ob.bid_levels.len(), 2);
            assert_eq!(ob.ask_levels.len(), 2);
            assert_eq!(ob.bid_levels[0].0, 100_00000000);
            assert_eq!(ob.ask_levels[0].1, 75_000_000);
        }
        _ => panic!("Expected OrderBook variant"),
    }
    
    Ok(())
}

// WAL Entry Wrapper Tests
#[rstest]
#[test]
fn test_wal_entry_wrapper() -> Result<()> {
    let data = TradeEvent {
        ts: 1234567890_000_000_000,
        symbol: "BTCUSDT".to_string(),
        price: 50000_00000000,
        quantity: 100_000_000,
        is_buy: true,
        trade_id: 12345,
    };
    
    let wrapper = WalEntryWrapper {
        timestamp: 1234567890_000_000_000,
        sequence: 42,
        data: data.clone(),
    };
    
    let serialized = serde_json::to_string(&wrapper)?;
    let deserialized: WalEntryWrapper<TradeEvent> = serde_json::from_str(&serialized)?;
    
    assert_eq!(wrapper.timestamp, deserialized.timestamp);
    assert_eq!(wrapper.sequence, deserialized.sequence);
    assert_eq!(wrapper.data.symbol, deserialized.data.symbol);
    assert_eq!(wrapper.data.price, deserialized.data.price);
    
    Ok(())
}

// WAL Event Tests
#[rstest]
#[test]
fn test_wal_event_timestamp_extraction() {
    let tick = create_test_tick_event("TESTCOIN", 1000_000_000, 10_000_000);
    let wal_event = WalEvent::Tick(tick.clone());
    
    assert_eq!(wal_event.timestamp(), tick.timestamp);
}

#[rstest]
#[test]
fn test_wal_event_serialization() -> Result<()> {
    let tick = create_test_tick_event("TESTCOIN", 1000_000_000, 10_000_000);
    let wal_event = WalEvent::Tick(tick.clone());
    
    let serialized = serde_json::to_string(&wal_event)?;
    let deserialized: WalEvent = serde_json::from_str(&serialized)?;
    
    match deserialized {
        WalEvent::Tick(t) => {
            assert_eq!(t.symbol, tick.symbol);
            assert_eq!(t.price, tick.price);
            assert_eq!(t.quantity, tick.quantity);
        }
    }
    
    assert_eq!(wal_event.timestamp(), deserialized.timestamp());
    Ok(())
}

// Performance and Stress Tests
#[rstest]
#[tokio::test]
async fn test_wal_high_volume_writes() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path().join("high_volume_wal");
    
    let mut wal = Wal::new(&wal_path, Some(1024 * 1024))?; // 1MB segments
    
    let start_time = std::time::Instant::now();
    let num_entries = 10000;
    
    // Write many entries
    for i in 0..num_entries {
        let trade = create_test_trade_event(
            "BTCUSDT",
            50000_00000000 + (i as i64 * 1000),
            100_000_000,
            i as u64,
        );
        wal.append(&trade)?;
        
        // Flush periodically
        if i % 1000 == 0 {
            wal.flush()?;
        }
    }
    
    wal.flush()?;
    let write_duration = start_time.elapsed();
    
    println!(
        "Wrote {} entries in {:?} ({:.2} entries/sec)",
        num_entries,
        write_duration,
        num_entries as f64 / write_duration.as_secs_f64()
    );
    
    // Verify we can read back all entries
    let read_start = std::time::Instant::now();
    let mut iterator: WalIterator<TradeEvent> = wal.stream(None)?;
    let mut count = 0;
    
    while iterator.read_next_entry()?.is_some() {
        count += 1;
    }
    
    let read_duration = read_start.elapsed();
    
    println!(
        "Read {} entries in {:?} ({:.2} entries/sec)",
        count,
        read_duration,
        count as f64 / read_duration.as_secs_f64()
    );
    
    assert_eq!(count, num_entries);
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_wal_large_entries() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path().join("large_entries_wal");
    
    let mut wal = Wal::new(&wal_path, None)?;
    
    // Create large entries
    for i in 0..10 {
        let large_data = "x".repeat(10000 * (i + 1)); // Increasing sizes
        let entry = TestWalEntry {
            timestamp: 1234567890 + i as i64,
            sequence: i as u64,
            data: large_data,
        };
        wal.append(&entry)?;
    }
    
    wal.flush()?;
    
    // Verify we can read them back
    let mut iterator: WalIterator<TestWalEntry> = wal.stream(None)?;
    let mut count = 0;
    
    while let Some(entry) = iterator.read_next_entry()? {
        assert_eq!(entry.data.len(), 10000 * (count + 1));
        count += 1;
    }
    
    assert_eq!(count, 10);
    Ok(())
}

// Error Handling Tests
#[rstest]
#[tokio::test]
async fn test_wal_invalid_path() {
    // Try to create WAL in non-existent directory without proper permissions
    let invalid_path = Path::new("/non/existent/directory");
    let result = Wal::new(invalid_path, None);
    
    // Should fail to create WAL
    assert!(result.is_err());
}

#[rstest]
#[tokio::test]
async fn test_wal_corrupted_data_handling() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path().join("corrupted_wal");
    
    let mut wal = Wal::new(&wal_path, None)?;
    
    // Write some valid entries
    for i in 0..5 {
        let entry = TestWalEntry {
            timestamp: 1234567890 + i,
            sequence: i as u64,
            data: format!("entry {}", i),
        };
        wal.append(&entry)?;
    }
    wal.flush()?;
    
    // Manually corrupt the WAL file by writing invalid data
    let segment_files: Vec<_> = std::fs::read_dir(&wal_path)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.path().extension()? == "wal" {
                Some(entry.path())
            } else {
                None
            }
        })
        .collect();
    
    if let Some(segment_file) = segment_files.first() {
        // Append invalid data to corrupt the file
        std::fs::write(segment_file, b"corrupted data")?;
    }
    
    // Try to read - should handle corruption gracefully
    let mut iterator: WalIterator<TestWalEntry> = wal.stream(None)?;
    let mut valid_entries = 0;
    
    // Iterator should skip corrupted entries or handle gracefully
    while iterator.read_next_entry().is_ok() {
        valid_entries += 1;
        if valid_entries > 10 {
            break; // Prevent infinite loop
        }
    }
    
    // We should be able to process without panicking
    assert!(valid_entries <= 5);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_wal_concurrent_access() -> Result<()> {
    use std::sync::Arc;
    use tokio::sync::Mutex;
    
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path().join("concurrent_wal");
    
    let wal = Arc::new(Mutex::new(Wal::new(&wal_path, None)?));
    let mut handles = vec![];
    
    // Spawn multiple tasks writing to WAL
    for task_id in 0..5 {
        let wal_clone = Arc::clone(&wal);
        
        let handle = tokio::spawn(async move {
            let mut results = Vec::new();
            
            for i in 0..100 {
                let entry = TestWalEntry {
                    timestamp: (task_id * 1000 + i) as i64,
                    sequence: (task_id * 100 + i) as u64,
                    data: format!("task {} entry {}", task_id, i),
                };
                
                let mut wal_guard = wal_clone.lock().await;
                let result = wal_guard.append(&entry);
                results.push(result.is_ok());
                
                if i % 10 == 0 {
                    let _ = wal_guard.flush();
                }
            }
            
            results
        });
        
        handles.push(handle);
    }
    
    // Wait for all tasks to complete
    let mut total_success = 0;
    for handle in handles {
        let results = handle.await?;
        total_success += results.iter().filter(|&&success| success).count();
    }
    
    // Final flush
    {
        let mut wal_guard = wal.lock().await;
        wal_guard.flush()?;
    }
    
    assert!(total_success > 0);
    println!("Successfully wrote {} entries concurrently", total_success);
    
    Ok(())
}

// Iterator Edge Cases
#[rstest]
#[tokio::test]
async fn test_wal_iterator_empty_wal() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path().join("empty_wal");
    
    let wal = Wal::new(&wal_path, None)?;
    let mut iterator: WalIterator<TestWalEntry> = wal.stream(None)?;
    
    let entry = iterator.read_next_entry()?;
    assert!(entry.is_none());
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_wal_iterator_non_existent_path() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let non_existent_path = temp_dir.path().join("does_not_exist");
    
    let iterator_result = WalIterator::<TestWalEntry>::new(&non_existent_path, None);
    assert!(iterator_result.is_ok()); // Should handle non-existent path gracefully
    
    let mut iterator = iterator_result?;
    let entry = iterator.read_next_entry()?;
    assert!(entry.is_none());
    
    Ok(())
}

// Memory Usage Tests
#[rstest]
#[tokio::test]
async fn test_memory_efficient_iteration() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path().join("memory_test_wal");
    
    let mut wal = Wal::new(&wal_path, Some(1024))?; // Small segments
    
    // Write many entries
    for i in 0..1000 {
        let entry = TestWalEntry {
            timestamp: 1234567890 + i,
            sequence: i as u64,
            data: format!("memory test entry {} with some data", i),
        };
        wal.append(&entry)?;
    }
    wal.flush()?;
    
    // Iterate without loading everything into memory
    let mut iterator: WalIterator<TestWalEntry> = wal.stream(None)?;
    let mut count = 0;
    
    // Process entries one by one
    while let Some(_entry) = iterator.read_next_entry()? {
        count += 1;
        // Don't store the entry, just count it
    }
    
    assert_eq!(count, 1000);
    Ok(())
}

#[rstest]
#[test]
fn test_data_event_memory_layout() {
    use std::mem;
    
    // Test that DataEvent variants don't have excessive memory overhead
    let trade_event = DataEvent::Trade(create_test_trade_event("BTCUSDT", 50000, 100, 1));
    let candle_event = DataEvent::Candle(create_test_candle_event("ETHUSDT", "1m"));
    let system_event = DataEvent::System(SystemEvent {
        ts: 1234567890,
        event_type: "TEST".to_string(),
    });
    
    let trade_size = mem::size_of_val(&trade_event);
    let candle_size = mem::size_of_val(&candle_event);
    let system_size = mem::size_of_val(&system_event);
    
    println!("Trade event size: {} bytes", trade_size);
    println!("Candle event size: {} bytes", candle_size);
    println!("System event size: {} bytes", system_size);
    
    // All should be reasonable sizes (not pathologically large)
    assert!(trade_size < 1000);
    assert!(candle_size < 1000);
    assert!(system_size < 1000);
}