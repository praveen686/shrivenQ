//! Storage and WAL types

use crate::{Px, Qty, Symbol, Ts};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// WAL entry trait
pub trait WalEntry {
    /// Get timestamp
    fn timestamp(&self) -> Ts;
    
    /// Get sequence
    fn sequence(&self) -> u64;
    
    /// Serialize to bytes
    fn to_bytes(&self) -> Result<Vec<u8>>;
}

/// WAL entry wrapper struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalEntryWrapper<T> {
    /// Timestamp
    pub timestamp: Ts,
    /// Sequence number
    pub sequence: u64,
    /// Data payload
    pub data: T,
}

/// WAL event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalEvent {
    /// Tick event
    Tick(TickEvent),
}

impl WalEvent {
    /// Get timestamp from event
    #[must_use] pub const fn timestamp(&self) -> Ts {
        match self {
            Self::Tick(tick) => tick.timestamp,
        }
    }
}

/// Tick event for WAL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickEvent {
    /// Symbol
    pub symbol: Symbol,
    /// Timestamp
    pub timestamp: Ts,
    /// Timestamp (alternative name)
    pub ts: Ts,
    /// Price
    pub price: Px,
    /// Quantity
    pub quantity: Qty,
    /// Side (true = buy, false = sell)
    pub is_buy: bool,
    /// Bid price
    pub bid: Option<Px>,
    /// Ask price
    pub ask: Option<Px>,
    /// Last price
    pub last: Option<Px>,
    /// Volume
    pub volume: Option<Qty>,
    /// Venue
    pub venue: String,
}

/// Data event for aggregator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataEvent {
    /// Trade event
    Trade(TradeEvent),
    /// Candle event
    Candle(CandleEvent),
    /// System event
    System(SystemEvent),
    /// Volume profile event
    VolumeProfile(VolumeProfileEvent),
    /// Microstructure event
    Microstructure(MicrostructureEvent),
    /// Order book event
    OrderBook(OrderBookEvent),
}

/// Trade event for data aggregator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeEvent {
    /// Timestamp
    pub ts: Ts,
    /// Symbol
    pub symbol: Symbol,
    /// Price
    pub price: Px,
    /// Quantity
    pub quantity: Qty,
    /// Is buy side
    pub is_buy: bool,
    /// Trade ID
    pub trade_id: u64,
}

/// Candle event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandleEvent {
    /// Symbol
    pub symbol: Symbol,
    /// Timeframe
    pub timeframe: String,
    /// Open time
    pub open_time: DateTime<Utc>,
    /// Close time
    pub close_time: DateTime<Utc>,
    /// Open price
    pub open: Px,
    /// High price
    pub high: Px,
    /// Low price
    pub low: Px,
    /// Close price
    pub close: Px,
    /// Volume
    pub volume: Qty,
}

/// System event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    /// Timestamp
    pub ts: Ts,
    /// Event type
    pub event_type: String,
}

/// Volume profile event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeProfileEvent {
    /// Symbol
    pub symbol: Symbol,
    /// Timestamp
    pub ts: Ts,
}

/// Microstructure event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicrostructureEvent {
    /// Symbol
    pub symbol: Symbol,
    /// Timestamp
    pub ts: Ts,
}

/// Order book event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookEvent {
    /// Symbol
    pub symbol: Symbol,
    /// Sequence
    pub sequence: u64,
    /// Bid levels
    pub bid_levels: Vec<(Px, Qty)>,
    /// Ask levels
    pub ask_levels: Vec<(Px, Qty)>,
}

/// Production-grade Write-Ahead Log implementation
pub struct Wal {
    path: std::path::PathBuf,
    current_file: Option<std::fs::File>,
    sequence: u64,
    segment_size: usize,
    current_segment_size: usize,
    segment_index: u64,
}

impl std::fmt::Debug for Wal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wal")
            .field("path", &self.path)
            .field("has_current_file", &self.current_file.is_some())
            .field("sequence", &self.sequence)
            .field("segment_size", &self.segment_size)
            .field("current_segment_size", &self.current_segment_size)
            .field("segment_index", &self.segment_index)
            .finish()
    }
}

impl Wal {
    /// Create new WAL
    pub fn new(path: &Path, segment_size: Option<usize>) -> Result<Self> {
        std::fs::create_dir_all(path)?;
        
        Ok(Self {
            path: path.to_path_buf(),
            current_file: None,
            sequence: 0,
            segment_size: segment_size.unwrap_or(100 * 1024 * 1024), // 100MB default
            current_segment_size: 0,
            segment_index: 0,
        })
    }

    /// Append to WAL
    pub fn append<T: Serialize>(&mut self, data: &T) -> Result<()> {
        use std::io::Write;
        
        // Serialize the data
        let serialized = serde_json::to_vec(data)?;
        let entry_size = serialized.len() + 8; // 8 bytes for length prefix
        
        // Check if we need a new segment
        if self.current_file.is_none() || 
           self.current_segment_size + entry_size > self.segment_size {
            self.create_new_segment()?;
        }
        
        // Write to current file
        if let Some(ref mut file) = self.current_file {
            // Write length prefix (4 bytes) + sequence (4 bytes) + data
            file.write_all(&(serialized.len() as u32).to_le_bytes())?;
            file.write_all(&(self.sequence as u32).to_le_bytes())?;
            file.write_all(&serialized)?;
            file.flush()?;
            
            self.current_segment_size += entry_size;
            self.sequence += 1;
        }
        
        Ok(())
    }

    /// Flush WAL
    pub fn flush(&mut self) -> Result<()> {
        use std::io::Write;
        if let Some(ref mut file) = self.current_file {
            file.flush()?;
        }
        Ok(())
    }

    /// Stream entries from WAL
    pub fn stream<T>(&self, from_ts: Option<crate::Ts>) -> Result<WalIterator<T>> {
        WalIterator::new(&self.path, from_ts)
    }
    
    /// Create a new segment file
    fn create_new_segment(&mut self) -> Result<()> {
        use std::fs::OpenOptions;
        
        let segment_path = self.path.join(format!("segment_{:06}.wal", self.segment_index));
        
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(segment_path)?;
            
        self.current_file = Some(file);
        self.current_segment_size = 0;
        self.segment_index += 1;
        
        Ok(())
    }
}

/// WAL iterator for reading entries
pub struct WalIterator<T> {
    segment_files: Vec<std::path::PathBuf>,
    current_file_index: usize,
    current_file: Option<std::fs::File>,
    from_ts: Option<crate::Ts>,
    _marker: std::marker::PhantomData<T>,
}

impl<T> std::fmt::Debug for WalIterator<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WalIterator")
            .field("segment_files", &self.segment_files)
            .field("current_file_index", &self.current_file_index)
            .field("has_current_file", &self.current_file.is_some())
            .field("from_ts", &self.from_ts)
            .finish()
    }
}

impl<T> WalIterator<T> {
    /// Create new iterator
    pub fn new(wal_path: &std::path::Path, from_ts: Option<crate::Ts>) -> Result<Self> {
        let mut segment_files = Vec::new();
        
        // Find all segment files
        if wal_path.exists() {
            for entry in std::fs::read_dir(wal_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "wal") {
                    segment_files.push(path);
                }
            }
        }
        
        // Sort by filename to ensure correct order
        segment_files.sort();
        
        Ok(Self {
            segment_files,
            current_file_index: 0,
            current_file: None,
            from_ts,
            _marker: std::marker::PhantomData,
        })
    }

    /// Read next entry
    pub fn read_next_entry(&mut self) -> Result<Option<T>>
    where
        T: serde::de::DeserializeOwned,
    {
        use std::io::Read;
        
        loop {
            // Open next file if needed
            if self.current_file.is_none() {
                if self.current_file_index >= self.segment_files.len() {
                    return Ok(None); // No more files
                }
                
                let file_path = &self.segment_files[self.current_file_index];
                self.current_file = Some(std::fs::File::open(file_path)?);
                self.current_file_index += 1;
            }
            
            // Try to read next entry from current file
            if let Some(ref mut file) = self.current_file {
                // Read length prefix (4 bytes)
                let mut len_buf = [0u8; 4];
                match file.read_exact(&mut len_buf) {
                    Ok(()) => {
                        let data_len = u32::from_le_bytes(len_buf) as usize;
                        
                        // Read sequence (4 bytes)
                        let mut seq_buf = [0u8; 4];
                        file.read_exact(&mut seq_buf)?;
                        
                        // Read data
                        let mut data_buf = vec![0u8; data_len];
                        file.read_exact(&mut data_buf)?;
                        
                        // Deserialize
                        match serde_json::from_slice::<T>(&data_buf) {
                            Ok(entry) => {
                                // Filter by timestamp if specified
                                if let Some(from_ts) = self.from_ts {
                                    // Try to extract timestamp from the entry
                                    if let Ok(json_value) = serde_json::from_slice::<serde_json::Value>(&data_buf) {
                                        if let Some(ts) = json_value.get("timestamp")
                                            .and_then(|v| v.as_u64())
                                            .map(|ts| crate::Ts::from_millis(ts as i64))
                                        {
                                            if ts < from_ts {
                                                continue; // Skip entries before from_ts
                                            }
                                        }
                                    }
                                }
                                return Ok(Some(entry))
                            },
                            Err(_) => continue, // Skip corrupted entries
                        }
                    }
                    Err(_) => {
                        // End of file, try next file
                        self.current_file = None;
                        continue;
                    }
                }
            }
        }
    }
}

impl<T> Default for WalIterator<T> {
    fn default() -> Self {
        Self {
            segment_files: Vec::new(),
            current_file_index: 0,
            current_file: None,
            from_ts: None,
            _marker: std::marker::PhantomData,
        }
    }
}