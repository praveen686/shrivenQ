//! Wire protocol adapter for our actual WAL implementation

use anyhow::Result;
use common::{Px, Qty, Symbol, Ts};
use std::path::Path;
use storage::{TickEvent, Wal, WalEvent};

/// Synthetic event for write tests
#[derive(Clone)]
pub struct SyntheticEvent {
    pub ts_ns: u64,
    pub payload: Vec<u8>,
}

pub struct Writer {
    inner: Wal,
}

pub struct Reader {
    inner: Wal,
}

pub fn open_writer(path: &Path, segment_bytes: usize, _fsync_ms: Option<u64>) -> Result<Writer> {
    // Note: Our WAL implementation handles fsync internally
    let inner = Wal::new(path, Some(segment_bytes as u64))?;
    Ok(Writer { inner })
}

pub fn open_reader(path: &Path) -> Result<Reader> {
    let inner = Wal::new(path, None)?;
    Ok(Reader { inner })
}

impl Writer {
    pub fn append_synth(&mut self, ev: &SyntheticEvent) -> Result<()> {
        // Map SyntheticEvent to a TickEvent for our WAL
        // Use payload length to vary the volume field for more realistic testing
        #[allow(clippy::cast_precision_loss)] // Acceptable for testing/benchmarking
        let volume = ev.payload.len() as f64;
        let event = WalEvent::Tick(TickEvent {
            ts: Ts::from_nanos(ev.ts_ns),
            venue: "benchmark".to_string(),
            symbol: Symbol::new(1),
            bid: Some(Px::new(100.0)),
            ask: Some(Px::new(100.5)),
            last: Some(Px::new(100.25)),
            volume: Some(Qty::new(volume)),
        });
        self.inner.append(&event)?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.inner.flush()
    }
}

impl Reader {
    pub fn replay<F>(&self, mut on_event: F) -> Result<u64>
    where
        F: FnMut(u64 /*ts_ns*/, usize /*len*/),
    {
        let mut count = 0;
        let mut iter = self.inner.stream::<WalEvent>(None)?;

        while let Some(event) = iter.read_next_entry()? {
            // Get timestamp and approximate size
            let ts_ns = event.timestamp().as_nanos();
            let size = std::mem::size_of_val(&event); // Approximate
            on_event(ts_ns, size);
            count += 1;
        }
        Ok(count)
    }

    pub fn seek_to(&self, ts_ns: u64) -> Result<()> {
        // Our WAL supports streaming from a timestamp
        let _iter = self.inner.stream::<WalEvent>(Some(Ts::from_nanos(ts_ns)))?;
        Ok(())
    }
}
