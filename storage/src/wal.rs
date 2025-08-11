//! Write-Ahead Log implementation with segmented storage

use crate::segment::{Segment, SegmentReader};
use anyhow::{Result, anyhow};
use common::Ts;
use serde::{Serialize, de::DeserializeOwned};
use std::fs;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Default segment size (128 MB)
const DEFAULT_SEGMENT_SIZE: u64 = 128 * 1024 * 1024;

/// WAL entry trait
pub trait WalEntry: Serialize + DeserializeOwned + Send + Sync {
    /// Get the timestamp of the entry
    fn timestamp(&self) -> Ts;
}

/// Write-Ahead Log
pub struct Wal {
    dir: PathBuf,
    segment_size: u64,
    current_segment: Option<Segment>,
    segment_counter: u64,
}

impl Wal {
    /// Create a new WAL in the specified directory
    pub fn new(dir: &Path, segment_size: Option<u64>) -> Result<Self> {
        if !dir.exists() {
            fs::create_dir_all(dir)?;
        }

        let segment_size = segment_size.unwrap_or(DEFAULT_SEGMENT_SIZE);

        // Find the highest segment number
        let segment_counter = Self::find_latest_segment(dir)?;

        info!(
            "Initialized WAL at {} with segment size {} MB",
            dir.display(),
            segment_size / (1024 * 1024)
        );

        Ok(Self {
            dir: dir.to_path_buf(),
            segment_size,
            current_segment: None,
            segment_counter,
        })
    }

    /// Append an entry to the WAL
    pub fn append<T: WalEntry>(&mut self, entry: &T) -> Result<()> {
        let data = bincode::serialize(entry)?;

        // Check if we need a new segment
        if self.current_segment.is_none()
            || self
                .current_segment
                .as_ref()
                .map_or(false, |s| s.is_full(data.len()))
        {
            self.rotate_segment()?;
        }

        // Append to current segment
        if let Some(segment) = &mut self.current_segment {
            segment.append(&data)?;
        } else {
            return Err(anyhow!("Failed to create segment"));
        }

        Ok(())
    }

    /// Flush the WAL to disk
    pub fn flush(&mut self) -> Result<()> {
        if let Some(segment) = &mut self.current_segment {
            segment.flush()?;
        }
        Ok(())
    }

    /// Create an iterator to stream entries from a timestamp
    pub fn stream<T: WalEntry>(&self, from_ts: Option<Ts>) -> Result<WalIterator<T>> {
        WalIterator::new(&self.dir, from_ts)
    }

    /// Compact the WAL by removing segments before a timestamp
    pub fn compact(&mut self, before_ts: Ts) -> Result<u64> {
        let mut removed = 0;
        let segments = Self::list_segments(&self.dir)?;

        for segment_path in segments {
            // Check if segment contains only old data
            if Self::should_compact(&segment_path, before_ts)? {
                fs::remove_file(&segment_path)?;
                removed += 1;
                debug!("Removed segment: {}", segment_path.display());
            }
        }

        info!(
            "Compacted {} segments before timestamp {}",
            removed, before_ts
        );
        Ok(removed)
    }

    /// Get statistics about the WAL
    pub fn stats(&self) -> Result<WalStats> {
        let segments = Self::list_segments(&self.dir)?;
        let segment_count = segments.len() as u64;

        let mut total_size = 0;
        let mut total_entries = 0;

        for segment_path in segments {
            let metadata = fs::metadata(&segment_path)?;
            total_size += metadata.len();

            if let Ok(reader) = Segment::open(&segment_path) {
                total_entries += reader.entry_count();
            }
        }

        Ok(WalStats {
            segment_count,
            total_size,
            total_entries,
            current_segment_size: self.current_segment.as_ref().map(|s| s.size()),
        })
    }

    /// Rotate to a new segment
    fn rotate_segment(&mut self) -> Result<()> {
        // Close current segment if exists
        if let Some(segment) = self.current_segment.take() {
            segment.close()?;
        }

        // Create new segment
        self.segment_counter += 1;
        let segment_path = self.segment_path(self.segment_counter);
        self.current_segment = Some(Segment::create(&segment_path, self.segment_size)?);

        debug!("Rotated to new segment: {}", segment_path.display());
        Ok(())
    }

    /// Get the path for a segment
    fn segment_path(&self, counter: u64) -> PathBuf {
        self.dir.join(format!("{:010}.wal", counter))
    }

    /// Find the latest segment number in a directory
    fn find_latest_segment(dir: &Path) -> Result<u64> {
        let segments = Self::list_segments(dir)?;

        segments
            .iter()
            .filter_map(|path| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .max()
            .unwrap_or(0)
            .try_into()
            .map_err(|e| anyhow!("Invalid segment counter: {}", e))
    }

    /// List all segment files in a directory
    fn list_segments(dir: &Path) -> Result<Vec<PathBuf>> {
        let mut segments: Vec<PathBuf> = fs::read_dir(dir)?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .map_or(false, |ext| ext == "wal")
            })
            .map(|entry| entry.path())
            .collect();

        segments.sort();
        Ok(segments)
    }

    /// Check if a segment should be compacted
    fn should_compact(segment_path: &Path, before_ts: Ts) -> Result<bool> {
        let mut reader = Segment::open(segment_path)?;

        // Check last entry in segment
        let mut last_entry_ts = Ts::from_nanos(0);
        while let Some(data) = reader.read_next()? {
            if let Ok(entry) = bincode::deserialize::<crate::WalEvent>(&data) {
                last_entry_ts = entry.timestamp();
            }
        }

        Ok(last_entry_ts < before_ts)
    }
}

impl Drop for Wal {
    fn drop(&mut self) {
        // Close current segment on drop
        if let Some(segment) = self.current_segment.take() {
            if let Err(e) = segment.close() {
                warn!("Failed to close segment on WAL drop: {}", e);
            }
        }
    }
}

/// Statistics about the WAL
#[derive(Debug)]
pub struct WalStats {
    /// Number of segments
    pub segment_count: u64,
    /// Total size in bytes
    pub total_size: u64,
    /// Total number of entries
    pub total_entries: u64,
    /// Current segment size
    pub current_segment_size: Option<u64>,
}

/// Iterator for reading WAL entries
pub struct WalIterator<T: WalEntry> {
    segments: Vec<PathBuf>,
    current_reader: Option<SegmentReader>,
    current_index: usize,
    from_ts: Option<Ts>,
    _phantom: PhantomData<T>,
}

impl<T: WalEntry> WalIterator<T> {
    /// Create a new WAL iterator
    fn new(dir: &Path, from_ts: Option<Ts>) -> Result<Self> {
        let segments = Wal::list_segments(dir)?;

        Ok(Self {
            segments,
            current_reader: None,
            current_index: 0,
            from_ts,
            _phantom: PhantomData,
        })
    }

    /// Read the next entry
    pub fn next(&mut self) -> Result<Option<T>> {
        loop {
            // Open next segment if needed
            if self.current_reader.is_none() {
                if self.current_index >= self.segments.len() {
                    return Ok(None);
                }

                self.current_reader = Some(Segment::open(&self.segments[self.current_index])?);
                self.current_index += 1;
            }

            // Read from current segment
            if let Some(reader) = &mut self.current_reader {
                match reader.read_next()? {
                    Some(data) => {
                        let entry: T = bincode::deserialize(&data)?;

                        // Skip entries before from_ts
                        if let Some(from) = self.from_ts {
                            if entry.timestamp() < from {
                                continue;
                            }
                        }

                        return Ok(Some(entry));
                    }
                    None => {
                        // Move to next segment
                        self.current_reader = None;
                    }
                }
            }
        }
    }
}

// Implement WalEntry for our event types
impl WalEntry for crate::WalEvent {
    fn timestamp(&self) -> Ts {
        self.timestamp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::*;
    use common::{Px, Symbol};
    use tempfile::TempDir;

    #[test]
    fn test_wal_append_and_stream() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let wal_path = temp_dir.path();

        // Create WAL and append events
        {
            let mut wal = Wal::new(wal_path, Some(1024 * 1024))?;

            for i in 0..10 {
                let event = WalEvent::System(SystemEvent {
                    ts: Ts::from_nanos(i),
                    event_type: SystemEventType::Info,
                    message: format!("Event {}", i),
                });
                wal.append(&event)?;
            }

            wal.flush()?;
        }

        // Read events back
        {
            let wal = Wal::new(wal_path, Some(1024 * 1024))?;
            let mut iter = wal.stream::<WalEvent>(None)?;

            for i in 0..10 {
                let event = iter.next()?.ok_or_else(|| anyhow!("Expected event"))?;
                assert_eq!(event.timestamp(), Ts::from_nanos(i));
            }

            assert!(iter.next()?.is_none());
        }

        Ok(())
    }

    #[test]
    fn test_wal_segment_rotation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let wal_path = temp_dir.path();

        // Small segment size to trigger rotation
        let mut wal = Wal::new(wal_path, Some(1024))?;

        // Append enough data to trigger rotation
        for i in 0..100 {
            let event = WalEvent::Tick(TickEvent {
                ts: Ts::from_nanos(i),
                venue: "test".to_string(),
                symbol: Symbol::new(1),
                bid: Some(Px::new(100.0)),
                ask: Some(Px::new(101.0)),
                last: None,
                volume: None,
            });
            wal.append(&event)?;
        }

        // Drop the WAL to close the current segment
        drop(wal);

        // Re-open to get stats
        let wal = Wal::new(wal_path, Some(1024))?;
        let stats = wal.stats()?;
        assert!(stats.segment_count > 1);

        // Count events by reading them back
        let mut iter = wal.stream::<WalEvent>(None)?;
        let mut count = 0;
        while iter.next()?.is_some() {
            count += 1;
        }
        assert_eq!(count, 100);

        Ok(())
    }

    #[test]
    fn test_wal_stream_from_timestamp() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let wal_path = temp_dir.path();

        // Append events
        {
            let mut wal = Wal::new(wal_path, Some(1024 * 1024))?;

            for i in 0..20 {
                let event = WalEvent::System(SystemEvent {
                    ts: Ts::from_nanos(i * 100),
                    event_type: SystemEventType::Info,
                    message: format!("Event {}", i),
                });
                wal.append(&event)?;
            }
        }

        // Stream from timestamp 1000
        {
            let wal = Wal::new(wal_path, Some(1024 * 1024))?;
            let mut iter = wal.stream::<WalEvent>(Some(Ts::from_nanos(1000)))?;

            let first = iter.next()?.ok_or_else(|| anyhow!("Expected event"))?;
            assert_eq!(first.timestamp(), Ts::from_nanos(1000));
        }

        Ok(())
    }
}
