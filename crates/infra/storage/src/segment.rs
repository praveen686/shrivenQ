//! WAL segment management with CRC32 checksums

use anyhow::{Result, anyhow};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use crc32fast::Hasher;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tracing::{debug, trace};

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
    /// # Errors
    ///
    /// Returns an error if the file cannot be created or if header writing fails.
    pub fn create(path: &Path, max_size: u64) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .truncate(true)
            .open(path)?;

        let mut writer = BufWriter::new(file);

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

    /// Open an existing segment file for reading
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened or header reading fails.
    pub fn open(path: &Path) -> Result<SegmentReader> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

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
    /// # Errors
    ///
    /// Returns an error if the write operation fails or if the segment is full.
    pub fn append(&mut self, data: &[u8]) -> Result<()> {
        if self.is_full(data.len()) {
            return Err(anyhow!("Segment is full"));
        }

        // Calculate CRC32
        let mut hasher = Hasher::new();
        hasher.update(data);
        let crc = hasher.finalize();

        // Write entry: [length: u32][crc: u32][data: bytes]
        self.file
            .write_u32::<LittleEndian>(u32::try_from(data.len())?)?;
        self.file.write_u32::<LittleEndian>(crc)?;
        self.file.write_all(data)?;

        self.size += 8 + u64::try_from(data.len()).unwrap_or(0);
        self.entries += 1;

        trace!(
            "Appended entry {} ({} bytes) to segment",
            self.entries,
            data.len()
        );
        Ok(())
    }

    /// Check if segment has room for more data
    #[must_use]
    pub const fn is_full(&self, next_entry_size: usize) -> bool {
        // In const context, use saturating_add to prevent overflow
        // Safe cast: usize to u64 is always safe on 64-bit systems
        self.size
            .saturating_add(8)
            // SAFETY: Cast is safe within expected range
            .saturating_add(next_entry_size as u64)
            > self.max_size
    }

    /// Flush segment to disk
    ///
    /// # Errors
    ///
    /// Returns an error if the flush operation fails.
    pub fn flush(&mut self) -> Result<()> {
        self.file.flush()?;
        self.file.get_mut().sync_all()?;
        Ok(())
    }

    /// Close the segment, updating the header with final entry count
    ///
    /// # Errors
    ///
    /// Returns an error if the close operation fails.
    pub fn close(mut self) -> Result<()> {
        // Update entry count in header
        self.file.seek(SeekFrom::Start(8))?;
        self.file.write_u64::<LittleEndian>(self.entries)?;
        self.flush()?;

        debug!(
            "Closed segment {} with {} entries",
            self.path.display(),
            self.entries
        );
        Ok(())
    }

    /// Get the number of entries in the segment
    #[must_use]
    pub const fn entry_count(&self) -> u64 {
        self.entries
    }

    /// Get the current size of the segment
    #[must_use]
    pub const fn size(&self) -> u64 {
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
    /// # Errors
    ///
    /// Returns an error if the read operation fails.
    pub fn read_next(&mut self) -> Result<Option<Vec<u8>>> {
        if self.current >= self.entries {
            return Ok(None);
        }

        // Read entry header
        let length = usize::try_from(self.reader.read_u32::<LittleEndian>()?).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Length too large")
        })?;
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
    #[must_use]
    pub const fn entry_count(&self) -> u64 {
        self.entries
    }

    /// Reset reader to beginning of entries
    ///
    /// # Errors
    ///
    /// Returns an error if the seek operation fails.
    pub fn reset(&mut self) -> Result<()> {
        self.reader.seek(SeekFrom::Start(SEGMENT_HEADER_SIZE))?;
        self.current = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_segment_create_and_read() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let segment_path = temp_dir.path().join("test.seg");

        // Create and write to segment
        {
            let mut segment = Segment::create(&segment_path, 1024 * 1024)?;
            segment.append(b"entry1")?;
            segment.append(b"entry2")?;
            segment.append(b"entry3")?;
            assert_eq!(segment.entry_count(), 3);
            segment.close()?;
        }

        // Read from segment
        {
            let mut reader = Segment::open(&segment_path)?;
            assert_eq!(reader.entry_count(), 3);

            assert_eq!(reader.read_next()?, Some(b"entry1".to_vec()));
            assert_eq!(reader.read_next()?, Some(b"entry2".to_vec()));
            assert_eq!(reader.read_next()?, Some(b"entry3".to_vec()));
            assert_eq!(reader.read_next()?, None);
        }

        Ok(())
    }

    #[test]
    fn test_segment_crc_validation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let segment_path = temp_dir.path().join("test.seg");

        // Create segment with data
        {
            let mut segment = Segment::create(&segment_path, 1024)?;
            segment.append(b"test data")?;
            segment.close()?;
        }

        // Corrupt the data
        {
            let mut file = OpenOptions::new().write(true).open(&segment_path)?;
            file.seek(SeekFrom::Start(SEGMENT_HEADER_SIZE + 8))?;
            file.write_all(b"corrupted")?;
        }

        // Try to read corrupted data
        let mut reader = Segment::open(&segment_path)?;
        let result = reader.read_next();
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("CRC mismatch"));
        }

        Ok(())
    }

    #[test]
    fn test_segment_full() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let segment_path = temp_dir.path().join("test.seg");

        let mut segment = Segment::create(&segment_path, 100)?;

        // Should fit
        assert!(!segment.is_full(10));
        segment.append(b"small")?;

        // Should not fit (would exceed max_size)
        assert!(segment.is_full(100));

        Ok(())
    }
}
