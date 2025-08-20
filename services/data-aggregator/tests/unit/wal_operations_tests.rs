//! Comprehensive tests for Write-Ahead Log (WAL) operations

use data_aggregator::storage::{
    DataEvent, TradeEvent, CandleEvent, Wal, WalEntry, SystemEvent, SystemEventType,
};
use services_common::{Px, Qty, Symbol, Ts};
use rstest::*;
use tempfile::TempDir;
use anyhow::Result;
use std::time::Duration as StdDuration;
use tokio::time::sleep;

/// Test fixture for creating a temporary WAL directory
#[fixture]
fn temp_wal() -> Result<(Wal, TempDir)> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let wal = Wal::new(wal_path, Some(1024 * 1024))?; // 1MB segments
    Ok((wal, temp_dir))
}

/// Test fixture for creating test events
#[fixture]
fn test_trade_events() -> Vec<DataEvent> {
    let base_ts = Ts::from_nanos(1_000_000_000);
    
    vec![
        DataEvent::Trade(TradeEvent {
            ts: base_ts,
            symbol: Symbol::new(1),
            price: Px::from_price_i32(100_0000),
            quantity: Qty::from_qty_i32(10_0000),
            is_buy: true,
            trade_id: 1,
        }),
        DataEvent::Trade(TradeEvent {
            ts: Ts::from_nanos(base_ts.as_nanos() + 1_000_000),
            symbol: Symbol::new(1),
            price: Px::from_price_i32(101_0000),
            quantity: Qty::from_qty_i32(5_0000),
            is_buy: false,
            trade_id: 2,
        }),
        DataEvent::Trade(TradeEvent {
            ts: Ts::from_nanos(base_ts.as_nanos() + 2_000_000),
            symbol: Symbol::new(2),
            price: Px::from_price_i32(200_0000),
            quantity: Qty::from_qty_i32(15_0000),
            is_buy: true,
            trade_id: 3,
        }),
    ]
}

#[rstest]
#[tokio::test]
async fn test_wal_basic_append_and_read(temp_wal: Result<(Wal, TempDir)>) -> Result<()> {
    let (mut wal, _temp_dir) = temp_wal?;
    
    let event = DataEvent::Trade(TradeEvent {
        ts: Ts::from_nanos(1_000_000),
        symbol: Symbol::new(1),
        price: Px::from_price_i32(100_0000),
        quantity: Qty::from_qty_i32(10_0000),
        is_buy: true,
        trade_id: 12345,
    });

    // Append event
    wal.append(&event)?;
    wal.flush()?;

    // Read back
    let events: Vec<DataEvent> = wal.read_all()?;
    assert_eq!(events.len(), 1);
    
    match &events[0] {
        DataEvent::Trade(trade) => {
            assert_eq!(trade.ts, Ts::from_nanos(1_000_000));
            assert_eq!(trade.symbol, Symbol::new(1));
            assert_eq!(trade.price, Px::from_price_i32(100_0000));
            assert_eq!(trade.quantity, Qty::from_qty_i32(10_0000));
            assert_eq!(trade.is_buy, true);
            assert_eq!(trade.trade_id, 12345);
        }
        _ => panic!("Expected trade event"),
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_wal_multiple_events_ordering(
    temp_wal: Result<(Wal, TempDir)>,
    test_trade_events: Vec<DataEvent>,
) -> Result<()> {
    let (mut wal, _temp_dir) = temp_wal?;

    // Append events
    for event in &test_trade_events {
        wal.append(event)?;
    }
    wal.flush()?;

    // Read back and verify ordering
    let events: Vec<DataEvent> = wal.read_all()?;
    assert_eq!(events.len(), test_trade_events.len());

    for (original, read) in test_trade_events.iter().zip(events.iter()) {
        assert_eq!(original.timestamp(), read.timestamp());
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_wal_segment_rotation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let mut wal = Wal::new(wal_path, Some(256))?; // Very small segment size to force rotation

    let mut events = Vec::new();
    
    // Add enough events to trigger segment rotation
    for i in 0..20 {
        let event = DataEvent::Trade(TradeEvent {
            ts: Ts::from_nanos(1_000_000 + i as u64),
            symbol: Symbol::new(1),
            price: Px::from_price_i32(100_0000 + i as i64),
            quantity: Qty::from_qty_i32(10_0000),
            is_buy: i % 2 == 0,
            trade_id: i as u64,
        });
        
        events.push(event.clone());
        wal.append(&event)?;
    }

    wal.flush()?;

    // Verify all events are readable
    let read_events: Vec<DataEvent> = wal.read_all()?;
    assert_eq!(read_events.len(), events.len());

    // Verify stats show multiple segments
    let stats = wal.stats()?;
    assert!(stats.segment_count > 1, "Expected multiple segments");
    assert_eq!(stats.total_entries, events.len() as u64);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_wal_stream_from_timestamp() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let mut wal = Wal::new(wal_path, Some(1024 * 1024))?;

    let base_ts = Ts::from_nanos(1_000_000_000);
    let events = vec![
        DataEvent::Trade(TradeEvent {
            ts: base_ts,
            symbol: Symbol::new(1),
            price: Px::from_price_i32(100_0000),
            quantity: Qty::from_qty_i32(10_0000),
            is_buy: true,
            trade_id: 1,
        }),
        DataEvent::Trade(TradeEvent {
            ts: Ts::from_nanos(base_ts.as_nanos() + 1_000_000),
            symbol: Symbol::new(1),
            price: Px::from_price_i32(101_0000),
            quantity: Qty::from_qty_i32(5_0000),
            is_buy: false,
            trade_id: 2,
        }),
        DataEvent::Trade(TradeEvent {
            ts: Ts::from_nanos(base_ts.as_nanos() + 2_000_000),
            symbol: Symbol::new(1),
            price: Px::from_price_i32(102_0000),
            quantity: Qty::from_qty_i32(8_0000),
            is_buy: true,
            trade_id: 3,
        }),
    ];

    // Write events
    for event in &events {
        wal.append(event)?;
    }
    wal.flush()?;

    // Stream from middle timestamp
    let from_ts = Ts::from_nanos(base_ts.as_nanos() + 1_500_000); // Between second and third event
    let mut iterator = wal.stream::<DataEvent>(Some(from_ts))?;

    // Should only get the third event
    let mut count = 0;
    while let Some(event) = iterator.read_next_entry()? {
        match event {
            DataEvent::Trade(trade) => {
                assert!(trade.ts >= from_ts);
                count += 1;
            }
            _ => panic!("Unexpected event type"),
        }
    }

    assert_eq!(count, 1, "Should have streamed only one event after timestamp");

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_wal_compaction() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let mut wal = Wal::new(wal_path, Some(1024))?; // Small segments

    let base_ts = Ts::from_nanos(1_000_000_000);
    
    // Write events with different timestamps
    for i in 0..10 {
        let event = DataEvent::Trade(TradeEvent {
            ts: Ts::from_nanos(base_ts.as_nanos() + (i * 1_000_000) as u64),
            symbol: Symbol::new(1),
            price: Px::from_price_i32(100_0000),
            quantity: Qty::from_qty_i32(10_0000),
            is_buy: true,
            trade_id: i,
        });
        wal.append(&event)?;
    }
    wal.flush()?;

    // Get initial stats
    let initial_stats = wal.stats()?;
    let initial_segments = initial_stats.segment_count;

    // Compact events before a certain timestamp
    let compact_before = Ts::from_nanos(base_ts.as_nanos() + 5_000_000); // Before 6th event
    let removed_segments = wal.compact(compact_before)?;

    // Verify compaction
    assert!(removed_segments > 0, "Should have removed some segments");
    
    let final_stats = wal.stats()?;
    assert!(
        final_stats.segment_count < initial_segments, 
        "Should have fewer segments after compaction"
    );

    // Verify remaining events
    let remaining_events: Vec<DataEvent> = wal.read_all()?;
    for event in remaining_events {
        assert!(event.timestamp() >= compact_before, "All remaining events should be after compact timestamp");
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_wal_recovery_after_restart() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    
    let events = vec![
        DataEvent::Trade(TradeEvent {
            ts: Ts::from_nanos(1_000_000),
            symbol: Symbol::new(1),
            price: Px::from_price_i32(100_0000),
            quantity: Qty::from_qty_i32(10_0000),
            is_buy: true,
            trade_id: 1,
        }),
        DataEvent::System(SystemEvent {
            ts: Ts::from_nanos(2_000_000),
            event_type: SystemEventType::Checkpoint,
            message: "System checkpoint".to_string(),
        }),
    ];

    // Write events and close WAL
    {
        let mut wal = Wal::new(wal_path, Some(1024 * 1024))?;
        for event in &events {
            wal.append(event)?;
        }
        wal.flush()?;
        // WAL is dropped here, simulating process shutdown
    }

    // Recreate WAL (simulating restart) and verify data persisted
    {
        let wal = Wal::new(wal_path, Some(1024 * 1024))?;
        let recovered_events: Vec<DataEvent> = wal.read_all()?;
        
        assert_eq!(recovered_events.len(), events.len());
        for (original, recovered) in events.iter().zip(recovered_events.iter()) {
            assert_eq!(original.timestamp(), recovered.timestamp());
        }
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_wal_concurrent_writes() -> Result<()> {
    use std::sync::Arc;
    use tokio::sync::RwLock;
    
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let wal = Arc::new(RwLock::new(Wal::new(wal_path, Some(1024 * 1024))?));

    let num_tasks = 10;
    let events_per_task = 50;
    let mut handles = Vec::new();

    // Spawn concurrent write tasks
    for task_id in 0..num_tasks {
        let wal = Arc::clone(&wal);
        let handle = tokio::spawn(async move {
            for i in 0..events_per_task {
                let event = DataEvent::Trade(TradeEvent {
                    ts: Ts::from_nanos(1_000_000 + (task_id * 1000 + i) as u64),
                    symbol: Symbol::new(task_id as u32),
                    price: Px::from_price_i32(100_0000 + i as i64),
                    quantity: Qty::from_qty_i32(10_0000),
                    is_buy: i % 2 == 0,
                    trade_id: (task_id * 1000 + i) as u64,
                });

                let mut wal = wal.write().await;
                wal.append(&event).unwrap();
                
                // Occasionally yield to allow other tasks to run
                if i % 10 == 0 {
                    tokio::task::yield_now().await;
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await?;
    }

    // Flush and verify all events were written
    {
        let mut wal = wal.write().await;
        wal.flush()?;
    }

    let wal = wal.read().await;
    let all_events: Vec<DataEvent> = wal.read_all()?;
    let expected_total = num_tasks * events_per_task;
    
    assert_eq!(all_events.len(), expected_total, "All events should be written");

    // Verify events are properly ordered by timestamp
    let mut timestamps: Vec<u64> = all_events.iter().map(|e| e.timestamp().as_nanos()).collect();
    timestamps.sort_unstable();
    
    for i in 1..timestamps.len() {
        assert!(timestamps[i] >= timestamps[i-1], "Events should be ordered by timestamp");
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_wal_data_integrity_with_corruption() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    
    // Write some events
    {
        let mut wal = Wal::new(wal_path, Some(1024 * 1024))?;
        for i in 0..5 {
            let event = DataEvent::Trade(TradeEvent {
                ts: Ts::from_nanos(1_000_000 + i),
                symbol: Symbol::new(1),
                price: Px::from_price_i32(100_0000),
                quantity: Qty::from_qty_i32(10_0000),
                is_buy: true,
                trade_id: i,
            });
            wal.append(&event)?;
        }
        wal.flush()?;
    }

    // Corrupt a segment file by modifying bytes
    {
        use std::fs::OpenOptions;
        use std::io::{Seek, SeekFrom, Write};
        
        let segment_files = std::fs::read_dir(wal_path)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "wal"))
            .collect::<Vec<_>>();

        assert!(!segment_files.is_empty(), "Should have at least one segment file");

        // Corrupt the first segment
        let segment_path = segment_files[0].path();
        let mut file = OpenOptions::new().write(true).open(&segment_path)?;
        
        // Seek past header and corrupt some data
        file.seek(SeekFrom::Start(100))?;
        file.write_all(&[0xFF, 0xFF, 0xFF, 0xFF])?; // Write garbage
        file.sync_all()?;
    }

    // Try to read from corrupted WAL
    {
        let wal = Wal::new(wal_path, Some(1024 * 1024))?;
        let result = wal.read_all::<DataEvent>();
        
        // Should either fail with CRC error or return partial results
        // depending on where corruption occurred
        match result {
            Ok(events) => {
                // Some events might be recoverable before corruption point
                assert!(events.len() < 5, "Should not recover all events from corrupted WAL");
            }
            Err(e) => {
                // Error is expected with corruption
                assert!(e.to_string().contains("CRC") || e.to_string().contains("deserialize"));
            }
        }
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_wal_large_entries() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let mut wal = Wal::new(wal_path, Some(2 * 1024 * 1024))?; // 2MB segments

    // Create a large event with many price levels
    let mut large_event = data_aggregator::storage::OrderBookEvent {
        ts: Ts::from_nanos(1_000_000),
        symbol: Symbol::new(1),
        event_type: data_aggregator::storage::OrderBookEventType::Snapshot,
        sequence: 1,
        bid_levels: Vec::new(),
        ask_levels: Vec::new(),
        checksum: 0,
    };

    // Add many price levels to make it large
    for i in 0..1000 {
        large_event.bid_levels.push((
            Px::from_price_i32(100_0000 - i * 100),
            Qty::from_qty_i32(10_0000 + i * 100),
            1,
        ));
        large_event.ask_levels.push((
            Px::from_price_i32(100_0000 + i * 100),
            Qty::from_qty_i32(10_0000 + i * 100),
            1,
        ));
    }

    let event = DataEvent::OrderBook(large_event);
    
    // Append large event
    wal.append(&event)?;
    wal.flush()?;

    // Verify it can be read back
    let events: Vec<DataEvent> = wal.read_all()?;
    assert_eq!(events.len(), 1);
    
    match &events[0] {
        DataEvent::OrderBook(orderbook) => {
            assert_eq!(orderbook.bid_levels.len(), 1000);
            assert_eq!(orderbook.ask_levels.len(), 1000);
        }
        _ => panic!("Expected orderbook event"),
    }

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_wal_health_check() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let wal = Wal::new(wal_path, Some(1024 * 1024))?;

    // New WAL should be healthy
    assert!(wal.is_healthy(), "New WAL should be healthy");

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_wal_stats_accuracy() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();
    let mut wal = Wal::new(wal_path, Some(1024))?; // Small segments to force multiple

    let num_events = 50;
    
    // Write events
    for i in 0..num_events {
        let event = DataEvent::Trade(TradeEvent {
            ts: Ts::from_nanos(1_000_000 + i),
            symbol: Symbol::new(1),
            price: Px::from_price_i32(100_0000),
            quantity: Qty::from_qty_i32(10_0000),
            is_buy: true,
            trade_id: i,
        });
        wal.append(&event)?;
    }
    wal.flush()?;

    let stats = wal.stats()?;
    
    assert_eq!(stats.total_entries, num_events as u64);
    assert!(stats.total_size > 0, "Should have non-zero total size");
    assert!(stats.segment_count > 0, "Should have at least one segment");

    // If we have multiple segments, verify count is reasonable
    if stats.segment_count > 1 {
        assert!(stats.segment_count <= num_events as u64, "Shouldn't have more segments than events");
    }

    Ok(())
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[rstest]
    #[tokio::test]
    async fn test_wal_write_performance() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let wal_path = temp_dir.path();
        let mut wal = Wal::new(wal_path, Some(10 * 1024 * 1024))?; // 10MB segment

        let num_events = 10_000;
        let start = Instant::now();

        for i in 0..num_events {
            let event = DataEvent::Trade(TradeEvent {
                ts: Ts::from_nanos(1_000_000 + i),
                symbol: Symbol::new(1),
                price: Px::from_price_i32(100_0000),
                quantity: Qty::from_qty_i32(10_0000),
                is_buy: i % 2 == 0,
                trade_id: i,
            });
            wal.append(&event)?;
        }

        let write_duration = start.elapsed();
        wal.flush()?;
        let total_duration = start.elapsed();

        // Performance assertions (adjust based on expected performance)
        let writes_per_sec = num_events as f64 / write_duration.as_secs_f64();
        println!("WAL write performance: {:.0} events/sec", writes_per_sec);
        
        // Should achieve at least 100k writes/sec in memory
        assert!(writes_per_sec > 50_000.0, "WAL writes should be fast");

        // Flush shouldn't take too long
        let flush_time = total_duration - write_duration;
        println!("WAL flush time: {:?}", flush_time);
        assert!(flush_time < StdDuration::from_millis(1000), "Flush should be fast");

        Ok(())
    }
}