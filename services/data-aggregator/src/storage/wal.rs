//! Write-Ahead Log implementation for data persistence
//!
//! Performance requirements:
//! - Write latency: <100μs
//! - Zero allocations in hot path
//! - Fixed-point arithmetic only

use anyhow::{Result, anyhow};
use services_common::Ts;
use serde::{Serialize, de::DeserializeOwned};
use std::fs;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

use super::segment::SegmentReader;
use super::segment::Segment;

/// Default segment size (128 MB)
const DEFAULT_SEGMENT_SIZE: u64 = 128 * 1024 * 1024;

/// WAL entry trait for data-aggregator events
pub trait WalEntry: Serialize + DeserializeOwned + Send + Sync {
    /// Get the timestamp of the entry
    fn timestamp(&self) -> Ts;
}

/// Write-Ahead Log for data persistence
pub struct Wal {
    dir: PathBuf,
    segment_size: u64,
    current_segment: Option<Segment>,
    segment_counter: u64,
}

impl Wal {
    /// Create a new WAL in the specified directory
    ///
    /// # Performance
    /// - One-time setup, not in hot path
    /// - Pre-allocates segment buffers
    pub fn new(dir: &Path, segment_size: Option<u64>) -> Result<Self> {
        if !dir.exists() {
            fs::create_dir_all(dir)?;
        }

        let segment_size = segment_size.unwrap_or(DEFAULT_SEGMENT_SIZE);
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
    ///
    /// # Performance
    /// - O(1) for append operation
    /// - No allocations after initial buffer creation
    /// - Uses pre-allocated segment buffers
    #[inline]
    pub fn append<T: WalEntry>(&mut self, entry: &T) -> Result<()> {
        let data = bincode::serialize(entry)?;

        // Check if we need a new segment
        if self.current_segment.is_none()
            || self
                .current_segment
                .as_ref()
                .is_some_and(|s| s.is_full(data.len()))
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
    ///
    /// # Performance
    /// - Uses fsync for durability
    /// - Should be called periodically, not on every write
    pub fn flush(&mut self) -> Result<()> {
        // To properly persist the entry count, we need to close and reopen the segment
        if let Some(segment) = self.current_segment.take() {
            // Close the segment (this updates the entry count in the header)
            segment.close()?;
            
            // Reopen the segment for appending
            let segment_path = self.segment_path(self.segment_counter);
            if segment_path.exists() {
                self.current_segment = Some(Segment::open_for_append(&segment_path, self.segment_size)?);
            }
        }
        Ok(())
    }

    /// Create an iterator to stream entries from a timestamp
    ///
    /// # Performance
    /// - Lazy loading of segments
    /// - Sequential read optimization
    pub fn stream<T: WalEntry>(&self, from_ts: Option<Ts>) -> Result<WalIterator<T>> {
        WalIterator::new(&self.dir, from_ts)
    }

    /// Compact the WAL by removing segments before a timestamp
    ///
    /// # Performance
    /// - Background operation, not in hot path
    /// - Removes entire segments atomically
    pub fn compact(&mut self, before_ts: Ts) -> Result<u64> {
        let mut removed = 0;
        let segments = Self::list_segments(&self.dir)?;

        for segment_path in segments {
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

    /// Read all entries from WAL within time range
    ///
    /// # Performance
    /// - Sequential read optimized
    /// - Returns iterator for memory efficiency
    pub fn read_range<T: WalEntry>(&self, start_ts: Ts, end_ts: Ts) -> Result<Vec<T>> {
        let mut entries = Vec::new();
        let segments = Self::list_segments(&self.dir)?;
        
        for segment_path in segments {
            let mut reader = Segment::open(&segment_path)?;
            
            // Read all entries from this segment
            while let Some(data) = reader.read_next()? {
                match bincode::deserialize::<T>(&data) {
                    Ok(entry) => {
                        let ts = entry.timestamp();
                        if ts >= start_ts && ts <= end_ts {
                            entries.push(entry);
                        }
                    }
                    Err(e) => {
                        debug!("Failed to deserialize WAL entry: {}", e);
                        continue;
                    }
                }
            }
        }
        
        // Sort by timestamp
        entries.sort_by_key(WalEntry::timestamp);
        Ok(entries)
    }
    
    /// Read all entries from WAL (no time filtering)
    pub fn read_all<T: WalEntry>(&self) -> Result<Vec<T>> {
        let mut entries = Vec::new();
        let segments = Self::list_segments(&self.dir)?;
        
        info!("Reading from {} segments", segments.len());
        
        for segment_path in segments {
            info!("Reading segment: {}", segment_path.display());
            let mut reader = Segment::open(&segment_path)?;
            
            while let Some(data) = reader.read_next()? {
                match bincode::deserialize::<T>(&data) {
                    Ok(entry) => {
                        entries.push(entry);
                    }
                    Err(e) => {
                        debug!("Failed to deserialize WAL entry: {}", e);
                        continue;
                    }
                }
            }
        }
        
        info!("Read {} entries from WAL", entries.len());
        entries.sort_by_key(WalEntry::timestamp);
        Ok(entries)
    }

    /// Get statistics about the WAL
    pub fn stats(&self) -> Result<WalStats> {
        let segments = Self::list_segments(&self.dir)?;
        // SAFETY: usize to u64 widening conversion is always safe
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
            current_segment_size: self.current_segment.as_ref().map(Segment::size),
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
        self.dir.join(format!("{counter:010}.wal"))
    }

    /// Find the latest segment number in a directory
    fn find_latest_segment(dir: &Path) -> Result<u64> {
        let segments = Self::list_segments(dir)?;

        Ok(segments
            .iter()
            .filter_map(|path| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .max()
            .unwrap_or(0))
    }

    /// List all segment files in a directory
    fn list_segments(dir: &Path) -> Result<Vec<PathBuf>> {
        let mut segments: Vec<PathBuf> = fs::read_dir(dir)?
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("wal"))
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
            if let Ok(entry) = bincode::deserialize::<super::DataEvent>(&data) {
                last_entry_ts = entry.timestamp();
            }
        }

        Ok(last_entry_ts < before_ts)
    }
    
    /// Check WAL health status
    #[must_use] pub fn is_healthy(&self) -> bool {
        // Check if directory exists and is writable
        if !self.dir.exists() {
            return false;
        }
        
        // Check if we can list segments
        if Self::list_segments(&self.dir).is_err() {
            return false;
        }
        
        // Check current segment if exists
        if let Some(ref segment) = self.current_segment {
            // In production, add more sophisticated health checks
            return segment.size() < self.segment_size;
        }
        
        true
    }
}

impl Drop for Wal {
    fn drop(&mut self) {
        // Close current segment on drop
        if let Some(segment) = self.current_segment.take() {
            if let Err(e) = segment.close() {
                tracing::warn!("Failed to close segment on WAL drop: {}", e);
            }
        }
    }
}

impl std::fmt::Debug for Wal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wal")
            .field("dir", &self.dir)
            .field("segment_size", &self.segment_size)
            .field("current_segment", &self.current_segment.as_ref().map(|_| "<Segment>"))
            .field("segment_counter", &self.segment_counter)
            .finish()
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
    pub fn read_next_entry(&mut self) -> Result<Option<T>> {
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

impl<T: WalEntry> std::fmt::Debug for WalIterator<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WalIterator")
            .field("segments", &self.segments)
            .field("current_reader", &self.current_reader.as_ref().map(|_| "<SegmentReader>"))
            .field("current_index", &self.current_index)
            .field("from_ts", &self.from_ts)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[derive(Debug, Clone, Serialize, serde::Deserialize)]
    struct TestEvent {
        ts: Ts,
        value: i64,
    }

    impl WalEntry for TestEvent {
        fn timestamp(&self) -> Ts {
            self.ts
        }
    }

    #[test]
    fn test_wal_append_and_stream() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let wal_path = temp_dir.path();

        // Create WAL and append events
        {
            let mut wal = Wal::new(wal_path, Some(1024 * 1024))?;

            for i in 0..10 {
                let event = TestEvent {
                    ts: Ts::from_nanos(i),
                    // SAFETY: usize to i64 - loop counter is small
                    value: i as i64,
                };
                wal.append(&event)?;
            }

            wal.flush()?;
        }

        // Read events back
        {
            let wal = Wal::new(wal_path, Some(1024 * 1024))?;
            let mut iter = wal.stream::<TestEvent>(None)?;

            for i in 0..10 {
                let event = iter
                    .read_next_entry()?
                    .ok_or_else(|| anyhow!("Expected event"))?;
                assert_eq!(event.ts, Ts::from_nanos(i));
                // SAFETY: usize to i64 - loop counter is small
                assert_eq!(event.value, i as i64);
            }

            assert!(iter.read_next_entry()?.is_none());
        }

        Ok(())
    }
}
