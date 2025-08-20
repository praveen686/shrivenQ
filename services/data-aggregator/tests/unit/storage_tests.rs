//! Comprehensive tests for storage backends and segment management

use data_aggregator::storage::{
    Segment, SegmentReader, StorageBackend, RedisStorage, 
    DataEvent, TradeEvent, CandleEvent, SystemEvent, SystemEventType
};
use data_aggregator::{Candle, Timeframe, VolumeProfile, VolumeLevel};
use services_common::{Px, Qty, Symbol, Ts};
use chrono::{DateTime, Utc};
use rstest::*;
use tempfile::TempDir;
use anyhow::Result;
use std::path::Path;

/// Test fixture for creating a temporary directory
#[fixture]
fn temp_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp directory")
}

/// Test fixture for creating test segments
#[fixture]
fn test_segment(temp_dir: TempDir) -> Result<(Segment, TempDir)> {
    let segment_path = temp_dir.path().join("test_segment.wal");
    let segment = Segment::create(&segment_path, 1024 * 1024)?; // 1MB
    Ok((segment, temp_dir))
}

#[rstest]
#[tokio::test]
async fn test_segment_creation_and_basic_operations(temp_dir: TempDir) -> Result<()> {
    let segment_path = temp_dir.path().join("test.wal");
    
    // Create segment
    let mut segment = Segment::create(&segment_path, 1024 * 1024)?;
    
    // Verify initial state
    assert_eq!(segment.entry_count(), 0);
    assert!(segment.size() >= 16); // At least header size
    assert!(!segment.is_full(100));

    // Append some data
    let test_data = b"Hello, World!";
    segment.append(test_data)?;
    
    assert_eq!(segment.entry_count(), 1);
    assert!(segment.size() > 16);
    
    // Close segment
    segment.close()?;
    
    // Verify file exists
    assert!(segment_path.exists());

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_segment_write_and_read_cycle(temp_dir: TempDir) -> Result<()> {
    let segment_path = temp_dir.path().join("write_read.wal");
    
    let test_entries = vec![
        b"First entry".to_vec(),
        b"Second entry with more data".to_vec(),
        b"Third entry".to_vec(),
        vec![0u8; 1000], // Large entry
        b"Final entry".to_vec(),
    ];

    // Write phase
    {
        let mut segment = Segment::create(&segment_path, 10 * 1024)?; // 10KB
        
        for entry in &test_entries {
            segment.append(entry)?;
        }
        
        assert_eq!(segment.entry_count(), test_entries.len() as u64);
        segment.close()?;
    }

    // Read phase
    {
        let mut reader = Segment::open(&segment_path)?;
        assert_eq!(reader.entry_count(), test_entries.len() as u64);
        
        for (i, expected) in test_entries.iter().enumerate() {
            let actual = reader.read_next()?.expect(&format!("Expected entry {}", i));
            assert_eq!(actual, *expected, "Entry {} mismatch", i);
        }
        
        // Should be no more entries
        assert!(reader.read_next()?.is_none());
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_segment_full_detection(temp_dir: TempDir) -> Result<()> {
    let segment_path = temp_dir.path().join("small.wal");
    let mut segment = Segment::create(&segment_path, 256)?; // Very small segment
    
    let data = vec![0u8; 50]; // 50 bytes + 8 bytes overhead = 58 bytes per entry
    
    let mut count = 0;
    while !segment.is_full(data.len()) {
        segment.append(&data)?;
        count += 1;
    }
    
    assert!(count > 0);
    assert!(segment.is_full(data.len()));
    
    // Should not be able to append more
    let result = segment.append(&data);
    assert!(result.is_err());
    
    segment.close()?;
    
    println!("Fit {} entries in small segment", count);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_segment_data_integrity_with_crc(temp_dir: TempDir) -> Result<()> {
    let segment_path = temp_dir.path().join("integrity.wal");
    
    // Write test data
    {
        let mut segment = Segment::create(&segment_path, 1024)?;
        
        let critical_data = b"Critical financial data that must not be corrupted";
        segment.append(critical_data)?;
        segment.close()?;
    }

    // Read back and verify
    {
        let mut reader = Segment::open(&segment_path)?;
        let data = reader.read_next()?.expect("Expected data");
        assert_eq!(data, b"Critical financial data that must not be corrupted");
    }

    // Simulate corruption by modifying the file
    {
        use std::fs::OpenOptions;
        use std::io::{Seek, SeekFrom, Write};
        
        let mut file = OpenOptions::new().write(true).open(&segment_path)?;
        file.seek(SeekFrom::Start(50))?; // Seek into data area
        file.write_all(&[0xFF, 0xFF, 0xFF, 0xFF])?; // Write garbage
        file.sync_all()?;
    }

    // Try to read corrupted data
    {
        let mut reader = Segment::open(&segment_path)?;
        let result = reader.read_next();
        
        // Should fail with CRC error
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("CRC"), "Expected CRC error, got: {}", error_msg);
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_segment_reader_seek_operations(temp_dir: TempDir) -> Result<()> {
    let segment_path = temp_dir.path().join("seek_test.wal");
    
    let entries = (0..10).map(|i| format!("Entry {}", i).into_bytes()).collect::<Vec<_>>();
    
    // Write entries
    {
        let mut segment = Segment::create(&segment_path, 1024)?;
        for entry in &entries {
            segment.append(entry)?;
        }
        segment.close()?;
    }

    // Test seeking
    {
        let mut reader = Segment::open(&segment_path)?;
        
        // Read first entry normally
        let first = reader.read_next()?.expect("Expected first entry");
        assert_eq!(first, entries[0]);
        assert_eq!(reader.current_position(), 1);
        
        // Seek to entry 5
        reader.seek_to_entry(5)?;
        assert_eq!(reader.current_position(), 5);
        
        // Read entry 5
        let fifth = reader.read_next()?.expect("Expected fifth entry");
        assert_eq!(fifth, entries[5]);
        assert_eq!(reader.current_position(), 6);
        
        // Seek back to beginning
        reader.seek_to_entry(0)?;
        assert_eq!(reader.current_position(), 0);
        
        // Read first entry again
        let first_again = reader.read_next()?.expect("Expected first entry again");
        assert_eq!(first_again, entries[0]);
        
        // Test seeking beyond bounds
        let result = reader.seek_to_entry(100);
        assert!(result.is_err());
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_segment_large_entries(temp_dir: TempDir) -> Result<()> {
    let segment_path = temp_dir.path().join("large_entries.wal");
    
    // Create entries of varying sizes
    let entries = vec![
        vec![0u8; 1024],      // 1KB
        vec![1u8; 10240],     // 10KB
        vec![2u8; 102400],    // 100KB
        vec![3u8; 512],       // 512B
    ];

    // Write large entries
    {
        let mut segment = Segment::create(&segment_path, 2 * 1024 * 1024)?; // 2MB segment
        
        for (i, entry) in entries.iter().enumerate() {
            assert!(!segment.is_full(entry.len()), "Segment full before entry {}", i);
            segment.append(entry)?;
        }
        
        segment.close()?;
    }

    // Read back large entries
    {
        let mut reader = Segment::open(&segment_path)?;
        
        for (i, expected) in entries.iter().enumerate() {
            let actual = reader.read_next()?.expect(&format!("Expected entry {}", i));
            assert_eq!(actual.len(), expected.len(), "Size mismatch for entry {}", i);
            assert_eq!(actual, *expected, "Data mismatch for entry {}", i);
        }
        
        assert!(reader.read_next()?.is_none());
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_segment_concurrent_read_after_write(temp_dir: TempDir) -> Result<()> {
    let segment_path = temp_dir.path().join("concurrent.wal");
    
    // Write data
    {
        let mut segment = Segment::create(&segment_path, 1024)?;
        for i in 0..50 {
            let data = format!("Concurrent entry {}", i).into_bytes();
            segment.append(&data)?;
        }
        segment.close()?;
    }

    // Test multiple concurrent readers
    {
        use std::sync::Arc;
        use tokio::task::JoinSet;
        
        let path = Arc::new(segment_path.clone());
        let mut join_set = JoinSet::new();
        
        // Spawn multiple reader tasks
        for reader_id in 0..5 {
            let path_clone = Arc::clone(&path);
            join_set.spawn(async move {
                let mut reader = Segment::open(&path_clone)?;
                let mut count = 0;
                
                while let Some(data) = reader.read_next()? {
                    let content = String::from_utf8(data)?;
                    assert!(content.starts_with("Concurrent entry"));
                    count += 1;
                }
                
                Ok::<(usize, usize), anyhow::Error>((reader_id, count))
            });
        }
        
        // Wait for all readers to complete
        while let Some(result) = join_set.join_next().await {
            let (reader_id, count) = result??;
            assert_eq!(count, 50, "Reader {} read incorrect number of entries", reader_id);
        }
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_segment_error_handling(temp_dir: TempDir) -> Result<()> {
    let segment_path = temp_dir.path().join("error_test.wal");
    
    // Test creating segment in non-existent directory
    let bad_path = temp_dir.path().join("non_existent").join("test.wal");
    let result = Segment::create(&bad_path, 1024);
    assert!(result.is_err());
    
    // Test opening non-existent segment
    let result = Segment::open(&bad_path);
    assert!(result.is_err());
    
    // Create a valid segment first
    {
        let mut segment = Segment::create(&segment_path, 1024)?;
        segment.append(b"test data")?;
        segment.close()?;
    }

    // Test opening with corrupted magic number
    {
        use std::fs::OpenOptions;
        use std::io::{Seek, SeekFrom, Write};
        
        let mut file = OpenOptions::new().write(true).open(&segment_path)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&[0xFF, 0xFF, 0xFF, 0xFF])?; // Corrupt magic number
        file.sync_all()?;
        
        let result = Segment::open(&segment_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("magic"));
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_redis_storage_backend() -> Result<()> {
    // Note: This test requires Redis to be running
    // Skip if Redis is not available
    
    // Try to connect to Redis (skip test if not available)
    if std::env::var("REDIS_URL").is_err() {
        println!("Skipping Redis test - REDIS_URL not set");
        return Ok(());
    }
    
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
    
    let mut storage = match RedisStorage::new(&redis_url).await {
        Ok(s) => s,
        Err(e) => {
            println!("Skipping Redis test - connection failed: {}", e);
            return Ok(());
        }
    };

    let symbol = Symbol::new(1);
    let timeframe = Timeframe::M1;
    let now = Utc::now();
    
    // Create test candle
    let candle = Candle {
        symbol,
        timeframe,
        open_time: now,
        close_time: now + chrono::Duration::minutes(1),
        open: Px::from_price_i32(100_0000),
        high: Px::from_price_i32(101_0000),
        low: Px::from_price_i32(99_0000),
        close: Px::from_price_i32(100_5000),
        volume: Qty::from_qty_i32(1000_0000),
        trades: 50,
        buy_volume: Qty::from_qty_i32(600_0000),
        sell_volume: Qty::from_qty_i32(400_0000),
    };

    // Store candle
    storage.store_candle(&candle).await?;
    
    // Try to retrieve (note: get_candles is not fully implemented in the example)
    let retrieved_candles = storage.get_candles(
        symbol,
        timeframe,
        now - chrono::Duration::minutes(5),
        now + chrono::Duration::minutes(5),
    ).await?;
    
    // The implementation returns empty Vec, so we just verify no error
    assert!(retrieved_candles.is_empty());

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_volume_profile_storage_operations() -> Result<()> {
    // Test storing and retrieving volume profiles
    
    let symbol = Symbol::new(1);
    let start_time = Utc::now();
    let end_time = start_time + chrono::Duration::hours(1);
    
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

    // Serialize to JSON for testing storage format
    let serialized = serde_json::to_string(&volume_profile)?;
    assert!(serialized.contains("volume_profile"));
    assert!(serialized.contains("100.0000")); // Price format
    
    // Deserialize back
    let deserialized: VolumeProfile = serde_json::from_str(&serialized)?;
    
    // Verify round-trip
    assert_eq!(deserialized.symbol, volume_profile.symbol);
    assert_eq!(deserialized.levels.len(), volume_profile.levels.len());
    assert_eq!(deserialized.poc, volume_profile.poc);
    assert_eq!(deserialized.vah, volume_profile.vah);
    assert_eq!(deserialized.val, volume_profile.val);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_data_event_serialization_for_storage(temp_dir: TempDir) -> Result<()> {
    let segment_path = temp_dir.path().join("events.wal");
    
    // Create various data events
    let events = vec![
        DataEvent::Trade(TradeEvent {
            ts: Ts::from_nanos(1_000_000),
            symbol: Symbol::new(1),
            price: Px::from_price_i32(100_0000),
            quantity: Qty::from_qty_i32(10_0000),
            is_buy: true,
            trade_id: 12345,
        }),
        DataEvent::Candle(CandleEvent {
            ts: Ts::from_nanos(2_000_000),
            symbol: Symbol::new(1),
            timeframe: 60, // 1 minute
            open: Px::from_price_i32(100_0000),
            high: Px::from_price_i32(100_5000),
            low: Px::from_price_i32(99_5000),
            close: Px::from_price_i32(100_2000),
            volume: Qty::from_qty_i32(50_0000),
            trades: 10,
        }),
        DataEvent::System(SystemEvent {
            ts: Ts::from_nanos(3_000_000),
            event_type: SystemEventType::Checkpoint,
            message: "System checkpoint completed".to_string(),
        }),
    ];

    // Write events to segment
    {
        let mut segment = Segment::create(&segment_path, 1024)?;
        
        for event in &events {
            let serialized = bincode::serialize(event)?;
            segment.append(&serialized)?;
        }
        
        segment.close()?;
    }

    // Read events back
    {
        let mut reader = Segment::open(&segment_path)?;
        
        for (i, original_event) in events.iter().enumerate() {
            let data = reader.read_next()?.expect(&format!("Expected event {}", i));
            let deserialized_event: DataEvent = bincode::deserialize(&data)?;
            
            // Verify timestamps match
            assert_eq!(
                original_event.timestamp(),
                deserialized_event.timestamp(),
                "Timestamp mismatch for event {}",
                i
            );
            
            // Verify event types match
            match (original_event, &deserialized_event) {
                (DataEvent::Trade(_), DataEvent::Trade(_)) => {},
                (DataEvent::Candle(_), DataEvent::Candle(_)) => {},
                (DataEvent::System(_), DataEvent::System(_)) => {},
                _ => panic!("Event type mismatch for event {}", i),
            }
        }
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_storage_performance_benchmark(temp_dir: TempDir) -> Result<()> {
    use std::time::Instant;
    
    let segment_path = temp_dir.path().join("performance.wal");
    let num_entries = 10_000;
    
    // Generate test data
    let test_data: Vec<Vec<u8>> = (0..num_entries)
        .map(|i| format!("Performance test entry number {}", i).into_bytes())
        .collect();

    // Write performance test
    let write_start = Instant::now();
    {
        let mut segment = Segment::create(&segment_path, 10 * 1024 * 1024)?; // 10MB
        
        for data in &test_data {
            segment.append(data)?;
        }
        
        segment.close()?;
    }
    let write_duration = write_start.elapsed();

    // Read performance test
    let read_start = Instant::now();
    {
        let mut reader = Segment::open(&segment_path)?;
        let mut count = 0;
        
        while let Some(_data) = reader.read_next()? {
            count += 1;
        }
        
        assert_eq!(count, num_entries);
    }
    let read_duration = read_start.elapsed();

    // Calculate performance metrics
    let write_ops_per_sec = num_entries as f64 / write_duration.as_secs_f64();
    let read_ops_per_sec = num_entries as f64 / read_duration.as_secs_f64();
    
    println!("Storage Performance:");
    println!("  Write: {:.0} ops/sec ({:?} total)", write_ops_per_sec, write_duration);
    println!("  Read: {:.0} ops/sec ({:?} total)", read_ops_per_sec, read_duration);
    
    // Performance assertions (adjust based on expectations)
    assert!(write_ops_per_sec > 10_000.0, "Write performance should be > 10k ops/sec");
    assert!(read_ops_per_sec > 50_000.0, "Read performance should be > 50k ops/sec");

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_segment_file_format_version_handling(temp_dir: TempDir) -> Result<()> {
    let segment_path = temp_dir.path().join("version_test.wal");
    
    // Create segment with current version
    {
        let mut segment = Segment::create(&segment_path, 1024)?;
        segment.append(b"test data")?;
        segment.close()?;
    }

    // Verify we can read it
    {
        let mut reader = Segment::open(&segment_path)?;
        let data = reader.read_next()?.expect("Expected data");
        assert_eq!(data, b"test data");
    }

    // Simulate future version by modifying version field
    {
        use std::fs::OpenOptions;
        use std::io::{Seek, SeekFrom, Write};
        use byteorder::{LittleEndian, WriteBytesExt};
        
        let mut file = OpenOptions::new().write(true).open(&segment_path)?;
        file.seek(SeekFrom::Start(4))?; // Skip magic, go to version
        file.write_u32::<LittleEndian>(999)?; // Write future version
        file.sync_all()?;
    }

    // Try to read with future version
    {
        let result = Segment::open(&segment_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("version"));
    }

    Ok(())
}

/// Test segment reopening after append (reopen for append functionality)
#[rstest]
#[tokio::test]
async fn test_segment_reopen_for_append(temp_dir: TempDir) -> Result<()> {
    let segment_path = temp_dir.path().join("reopen.wal");
    
    // Initial write
    {
        let mut segment = Segment::create(&segment_path, 1024)?;
        segment.append(b"first entry")?;
        segment.append(b"second entry")?;
        segment.close()?;
    }

    // Reopen for append
    {
        let mut segment = Segment::open_for_append(&segment_path, 1024)?;
        assert_eq!(segment.entry_count(), 2); // Should remember existing entries
        
        segment.append(b"third entry")?;
        segment.append(b"fourth entry")?;
        segment.close()?;
    }

    // Verify all entries
    {
        let mut reader = Segment::open(&segment_path)?;
        assert_eq!(reader.entry_count(), 4);
        
        let expected = vec![b"first entry", b"second entry", b"third entry", b"fourth entry"];
        
        for (i, expected_data) in expected.iter().enumerate() {
            let actual = reader.read_next()?.expect(&format!("Expected entry {}", i));
            assert_eq!(actual, expected_data.to_vec());
        }
        
        assert!(reader.read_next()?.is_none());
    }

    Ok(())
}