//! WAL segment management with CRC32 checksums
//!
//! Performance requirements:
//! - Sequential write optimization
//! - Zero-copy reads where possible
//! - Pre-allocated buffers

use anyhow::{Result, anyhow};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use crc32fast::Hasher;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tracing::trace;

/// Size of segment header in bytes
const SEGMENT_HEADER_SIZE: u64 = 16;

/// Magic number for segment files
const SEGMENT_MAGIC: u32 = 0x5351_574C; // "SQWL" in hex

/// Version of segment format
const SEGMENT_VERSION: u32 = 1;

/// A single WAL segment file
pub struct Segment {
    path: PathBuf,
    file: BufWriter<File>,
    size: u64,
    max_size: u64,
    entries: u64,
}

impl Segment {
    /// Create a new segment file
    ///
    /// # Performance
    /// - Pre-allocates file buffer
    /// - Uses OS page-aligned writes
    pub fn create(path: &Path, max_size: u64) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .truncate(true)
            .open(path)?;

        // Use larger buffer for better throughput
        let mut writer = BufWriter::with_capacity(64 * 1024, file);

        // Write header
        writer.write_u32::<LittleEndian>(SEGMENT_MAGIC)?;
        writer.write_u32::<LittleEndian>(SEGMENT_VERSION)?;
        writer.write_u64::<LittleEndian>(0)?; // Entry count, updated on close

        writer.flush()?;

        Ok(Self {
            path: path.to_path_buf(),
            file: writer,
            size: SEGMENT_HEADER_SIZE,
            max_size,
            entries: 0,
        })
    }

    /// Open an existing segment file for appending
    ///
    /// # Performance
    /// - Reads header to get current state
    /// - Seeks to end for appending
    pub fn open_for_append(path: &Path, max_size: u64) -> Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .read(true)
            .open(path)?;
            
        // Read header to get current state
        let mut file_reader = BufReader::new(&file);
        let magic = file_reader.read_u32::<LittleEndian>()?;
        if magic != SEGMENT_MAGIC {
            return Err(anyhow!("Invalid segment magic: {:#x}", magic));
        }
        
        let version = file_reader.read_u32::<LittleEndian>()?;
        if version != SEGMENT_VERSION {
            return Err(anyhow!("Unsupported segment version: {}", version));
        }
        
        let entries = file_reader.read_u64::<LittleEndian>()?;
        drop(file_reader);
        
        // Seek to end for appending
        let mut file = file;
        let size = file.seek(SeekFrom::End(0))?;
        
        // Create writer for appending
        let writer = BufWriter::with_capacity(64 * 1024, file);
        
        Ok(Self {
            path: path.to_path_buf(),
            file: writer,
            size,
            max_size,
            entries,
        })
    }

    /// Open an existing segment file for reading
    pub fn open(path: &Path) -> Result<SegmentReader> {
        let file = File::open(path)?;
        let mut reader = BufReader::with_capacity(64 * 1024, file);

        // Read and verify header
        let magic = reader.read_u32::<LittleEndian>()?;
        if magic != SEGMENT_MAGIC {
            return Err(anyhow!("Invalid segment magic: {:#x}", magic));
        }

        let version = reader.read_u32::<LittleEndian>()?;
        if version != SEGMENT_VERSION {
            return Err(anyhow!("Unsupported segment version: {}", version));
        }

        let entries = reader.read_u64::<LittleEndian>()?;

        Ok(SegmentReader {
            reader,
            entries,
            current: 0,
        })
    }

    /// Append an entry to the segment
    ///
    /// # Performance
    /// - O(1) append operation
    /// - No allocations, writes directly to buffer
    #[inline]
    pub fn append(&mut self, data: &[u8]) -> Result<()> {
        if self.is_full(data.len()) {
            return Err(anyhow!("Segment is full"));
        }

        // Calculate CRC32
        let mut hasher = Hasher::new();
        hasher.update(data);
        let crc = hasher.finalize();

        // Write entry: [length: u32][crc: u32][data: bytes]
        // SAFETY: usize to u32 - data length must fit in u32 for segment format
        let data_len = data.len() as u32;
        self.file.write_u32::<LittleEndian>(data_len)?;
        self.file.write_u32::<LittleEndian>(crc)?;
        self.file.write_all(data)?;

        // SAFETY: usize to u64 widening conversion is always safe
        self.size += 8 + data.len() as u64;
        self.entries += 1;

        trace!(
            "Appended entry {} ({} bytes) to segment",
            self.entries,
            data.len()
        );
        Ok(())
    }

    /// Check if segment has room for more data
    #[must_use] pub const fn is_full(&self, next_entry_size: usize) -> bool {
        self.size
            .saturating_add(8)
            // SAFETY: usize to u64 widening conversion is always safe
            .saturating_add(next_entry_size as u64)
            > self.max_size
    }

    /// Flush segment to disk
    pub fn flush(&mut self) -> Result<()> {
        self.file.flush()?;
        self.file.get_mut().sync_all()?;
        Ok(())
    }

    /// Close the segment, updating the header with final entry count
    pub fn close(mut self) -> Result<()> {
        // Update entry count in header
        self.file.seek(SeekFrom::Start(8))?;
        self.file.write_u64::<LittleEndian>(self.entries)?;
        self.flush()?;

        tracing::debug!(
            "Closed segment {} with {} entries",
            self.path.display(),
            self.entries
        );
        Ok(())
    }

    /// Get the number of entries in the segment
    #[must_use] pub const fn entry_count(&self) -> u64 {
        self.entries
    }

    /// Get the current size of the segment
    #[must_use] pub const fn size(&self) -> u64 {
        self.size
    }
}

/// Reader for a WAL segment
pub struct SegmentReader {
    reader: BufReader<File>,
    entries: u64,
    current: u64,
}

impl SegmentReader {
    /// Read the next entry from the segment
    ///
    /// # Performance
    /// - Sequential read optimization
    /// - Uses buffered I/O for efficiency
    pub fn read_next(&mut self) -> Result<Option<Vec<u8>>> {
        if self.current >= self.entries {
            return Ok(None);
        }

        // Read entry header
        // SAFETY: u32 to usize widening on 64-bit, identity on 32-bit
        let length = self.reader.read_u32::<LittleEndian>()? as usize;
        let expected_crc = self.reader.read_u32::<LittleEndian>()?;

        // Read data
        let mut data = vec![0u8; length];
        self.reader.read_exact(&mut data)?;

        // Verify CRC
        let mut hasher = Hasher::new();
        hasher.update(&data);
        let actual_crc = hasher.finalize();

        if actual_crc != expected_crc {
            return Err(anyhow!(
                "CRC mismatch: expected {:#x}, got {:#x}",
                expected_crc,
                actual_crc
            ));
        }

        self.current += 1;
        Ok(Some(data))
    }

    /// Get the total number of entries
    #[must_use] pub const fn entry_count(&self) -> u64 {
        self.entries
    }

    /// Get the current position
    #[must_use] pub const fn current_position(&self) -> u64 {
        self.current
    }

    /// Seek to a specific entry
    pub fn seek_to_entry(&mut self, entry_num: u64) -> Result<()> {
        if entry_num >= self.entries {
            return Err(anyhow!("Entry {} out of bounds", entry_num));
        }

        // Reset to start of data
        self.reader.seek(SeekFrom::Start(SEGMENT_HEADER_SIZE))?;
        self.current = 0;

        // Skip entries until we reach the target
        while self.current < entry_num {
            // SAFETY: u32 to u64 widening conversion is always safe
            let length = u64::from(self.reader.read_u32::<LittleEndian>()?);
            // SAFETY: u64 to i64 - length is from u32 so fits in i64
            self.reader.seek(SeekFrom::Current(4 + length as i64))?; // Skip CRC + data
            self.current += 1;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_segment_write_read() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let segment_path = temp_dir.path().join("test.wal");

        // Write data
        {
            let mut segment = Segment::create(&segment_path, 1024 * 1024)?;

            for i in 0..10 {
                let data = format!("Entry {}", i).into_bytes();
                segment.append(&data)?;
            }

            assert_eq!(segment.entry_count(), 10);
            segment.close()?;
        }

        // Read data back
        {
            let mut reader = Segment::open(&segment_path)?;
            assert_eq!(reader.entry_count(), 10);

            for i in 0..10 {
                let data = reader.read_next()?.expect("Expected entry");
                let text = String::from_utf8(data)?;
                assert_eq!(text, format!("Entry {}", i));
            }

            assert!(reader.read_next()?.is_none());
        }

        Ok(())
    }

    #[test]
    fn test_segment_full() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let segment_path = temp_dir.path().join("small.wal");

        // Small segment that fills up quickly
        let mut segment = Segment::create(&segment_path, 256)?;

        // Fill segment
        let mut count = 0;
        let data = vec![0u8; 20];
        while !segment.is_full(data.len()) {
            segment.append(&data)?;
            count += 1;
        }

        assert!(count > 0);
        assert!(segment.is_full(data.len()));

        Ok(())
    }

    #[test]
    fn test_crc_validation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let segment_path = temp_dir.path().join("crc.wal");

        // Write data
        {
            let mut segment = Segment::create(&segment_path, 1024)?;
            segment.append(b"test data")?;
            segment.close()?;
        }

        // Corrupt the file
        {
            use std::fs::OpenOptions;
            let mut file = OpenOptions::new().write(true).open(&segment_path)?;

            // Corrupt data after header
            file.seek(SeekFrom::Start(SEGMENT_HEADER_SIZE + 8))?;
            file.write_all(b"corrupted")?;
        }

        // Try to read corrupted data
        {
            let mut reader = Segment::open(&segment_path)?;
            let result = reader.read_next();
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("CRC mismatch"));
        }

        Ok(())
    }
}
