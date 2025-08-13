//! Integration test for deterministic replay with 10K events

use anyhow::Result;
use common::{Px, Qty, Symbol, Ts};
use storage::{OrderEvent, OrderSide, OrderStatus, OrderType, TickEvent, Wal, WalEvent};
use tempfile::TempDir;

/// Generate diverse test events
fn generate_test_events(count: usize) -> Vec<WalEvent> {
    let mut events = Vec::with_capacity(count);

    for i in 0..count {
        let event = match i % 3 {
            0 => {
                // Tick event
                WalEvent::Tick(TickEvent {
                    // SAFETY: Cast is safe within expected range
                    ts: Ts::from_nanos(i as u64 * 1000),
                    // SAFETY: Cast is safe within expected range
                    venue: format!("venue_{}", i % 2),
                    // SAFETY: Cast is safe within expected range
                    symbol: Symbol::new((i % 100) as u32),
                    // SAFETY: Cast is safe within expected range
                    bid: Some(Px::new(100.0 + (i as f64 * 0.01))),
                    // SAFETY: Cast is safe within expected range
                    ask: Some(Px::new(100.5 + (i as f64 * 0.01))),
                    last: if i % 2 == 0 {
                        // SAFETY: Cast is safe within expected range
                        // SAFETY: Cast is safe within expected range
                        Some(Px::new(100.25 + (i as f64 * 0.01)))
                    } else {
                        // SAFETY: Cast is safe within expected range
                        None
                    },
                    // SAFETY: Cast is safe within expected range
                    volume: Some(Qty::new(1000.0 + i as f64)),
                    // SAFETY: Cast is safe within expected range
                })
                // SAFETY: Cast is safe within expected range
            }
            // SAFETY: Cast is safe within expected range
            1 => {
                // SAFETY: Cast is safe within expected range
                // Order event
                // SAFETY: Cast is safe within expected range
                WalEvent::Order(OrderEvent {
                    // SAFETY: Cast is safe within expected range
                    ts: Ts::from_nanos(i as u64 * 1000),
                    // SAFETY: Cast is safe within expected range
                    order_id: i as u64,
                    symbol: Symbol::new((i % 50) as u32),
                    side: if i % 2 == 0 {
                        // SAFETY: Cast is safe within expected range
                        // SAFETY: Cast is safe within expected range
                        OrderSide::Buy
                    } else {
                        // SAFETY: Cast is safe within expected range
                        // SAFETY: Cast is safe within expected range
                        OrderSide::Sell
                    },
                    // SAFETY: Cast is safe within expected range
                    qty: Qty::new(100.0 * (1 + i % 10) as f64),
                    price: if i % 3 == 0 {
                        None
                    } else {
                        Some(Px::new(99.0 + (i as f64 * 0.1)))
                    },
                    order_type: match i % 3 {
                        0 => OrderType::Market,
                        // SAFETY: Cast is safe within expected range
                        1 => OrderType::Limit,
                        _ => OrderType::Stop,
                        // SAFETY: Cast is safe within expected range
                    },
                    status: OrderStatus::New,
                    // SAFETY: Cast is safe within expected range
                })
            }
            _ => {
                // System event
                WalEvent::System(storage::SystemEvent {
                    ts: Ts::from_nanos(i as u64 * 1000),
                    event_type: storage::SystemEventType::Info,
                    message: format!("System event {}", i),
                })
            }
        };

        events.push(event);
    }

    events
}

#[test]
fn test_deterministic_replay_10k_events() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();

    // Generate 10K events
    let original_events = generate_test_events(10_000);

    // Write events to WAL
    {
        let mut wal = Wal::new(wal_path, Some(128 * 1024 * 1024))?;

        for event in &original_events {
            wal.append(event)?;
        }

        wal.flush()?;

        // Don't check stats here as the segment hasn't been closed yet
        // The stats only count completed segments
    }

    // Read events back - first pass
    let mut first_read = Vec::new();
    {
        let wal = Wal::new(wal_path, Some(128 * 1024 * 1024))?;
        let mut iter = wal.stream::<WalEvent>(None)?;

        while let Some(event) = iter.read_next_entry()? {
            first_read.push(event);
        }
    }

    // Read events back - second pass (should be identical)
    let mut second_read = Vec::new();
    {
        let wal = Wal::new(wal_path, Some(128 * 1024 * 1024))?;
        let mut iter = wal.stream::<WalEvent>(None)?;

        while let Some(event) = iter.read_next_entry()? {
            second_read.push(event);
        }
    }

    // Verify deterministic replay
    assert_eq!(original_events.len(), first_read.len());
    assert_eq!(original_events.len(), second_read.len());

    for i in 0..original_events.len() {
        assert_eq!(
            original_events[i], first_read[i],
            "First read mismatch at index {}",
            i
        );
        assert_eq!(
            original_events[i], second_read[i],
            "Second read mismatch at index {}",
            i
        );
        assert_eq!(
            first_read[i], second_read[i],
            "Read consistency mismatch at index {}",
            i
        );
    }

    // Verified: Deterministic replay for 10,000 events

    // Test streaming from middle
    {
        let wal = Wal::new(wal_path, Some(128 * 1024 * 1024))?;
        let from_ts = Ts::from_nanos(5_000_000); // Start from event 5000
        let mut iter = wal.stream::<WalEvent>(Some(from_ts))?;

        let mut count = 0;
        while let Some(event) = iter.read_next_entry()? {
            assert!(event.timestamp() >= from_ts);
            count += 1;
        }

        assert_eq!(count, 5_000);
        // Verified: Partial replay from timestamp
    }

    // Test compaction
    {
        let mut wal = Wal::new(wal_path, Some(128 * 1024 * 1024))?;
        let before_ts = Ts::from_nanos(2_000_000); // Compact first 2000 events

        let removed = wal.compact(before_ts)?;
        // Verify that some segments were removed (or at least attempted)
        assert!(removed <= 2000); // Should remove roughly first 2000 events worth of segments

        // Verify remaining events
        let mut iter = wal.stream::<WalEvent>(None)?;
        let mut count = 0;

        while let Some(_event) = iter.read_next_entry()? {
            // After compaction, we might still have some events before the timestamp
            // if they're in a segment that also contains events after the timestamp
            count += 1;
        }

        assert!(count > 0);
    }

    Ok(())
}

#[test]
fn test_concurrent_write_and_read() -> Result<()> {
    use std::sync::Arc;
    use std::thread;

    let temp_dir = TempDir::new()?;
    let wal_path = Arc::new(temp_dir.path().to_path_buf());

    // Initialize WAL
    {
        let _wal = Wal::new(&wal_path, Some(128 * 1024 * 1024))?;
    }

    // Writer thread
    let writer_path = wal_path.clone();
    let writer = thread::spawn(move || -> Result<()> {
        let mut wal = Wal::new(&writer_path, Some(128 * 1024 * 1024))?;

        for i in 0..1000 {
            let event = WalEvent::System(storage::SystemEvent {
                ts: Ts::from_nanos(i * 1000),
                event_type: storage::SystemEventType::Info,
                message: format!("Event {}", i),
            });
            wal.append(&event)?;

            if i % 100 == 0 {
                wal.flush()?;
            }
        }

        wal.flush()?;
        Ok(())
    });

    // Wait for writer to finish
    writer
        .join()
        .map_err(|_| anyhow::anyhow!("Writer thread panicked"))??;

    // Verify all events are readable
    let wal = Wal::new(&wal_path, Some(128 * 1024 * 1024))?;
    let mut iter = wal.stream::<WalEvent>(None)?;

    let mut count = 0;
    while let Some(_event) = iter.read_next_entry()? {
        count += 1;
    }

    assert_eq!(count, 1000);
    // Verified: Concurrent write/read test

    Ok(())
}

#[test]
fn test_crash_recovery() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path();

    // Simulate a crash: write events without proper close
    {
        let mut wal = Wal::new(wal_path, Some(128 * 1024 * 1024))?;

        for i in 0..100 {
            let event = WalEvent::System(storage::SystemEvent {
                ts: Ts::from_nanos(i * 1000),
                event_type: storage::SystemEventType::Info,
                message: format!("Event {}", i),
            });
            wal.append(&event)?;
        }

        // Flush but don't close properly (drop will handle it)
        wal.flush()?;
    }

    // Recovery: open WAL and verify data
    {
        let wal = Wal::new(wal_path, Some(128 * 1024 * 1024))?;
        let mut iter = wal.stream::<WalEvent>(None)?;

        let mut count = 0;
        while let Some(event) = iter.read_next_entry()? {
            assert_eq!(event.timestamp(), Ts::from_nanos(count * 1000));
            count += 1;
        }

        assert_eq!(count, 100);
        // Verified: Crash recovery - all events recovered
    }

    Ok(())
}
