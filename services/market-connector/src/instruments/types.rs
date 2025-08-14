//! Instrument types with WAL serialization
//!
//! COMPLIANCE:
//! - Fixed-point arithmetic for all prices
//! - Efficient serialization with bincode
//! - FxHashMap for metadata storage

use chrono::Datelike;
use common::{Px, Ts};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use storage::wal::WalEntry;

/// Instrument type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InstrumentType {
    /// Equity/Stock instrument
    Equity,
    /// Index instrument  
    Index,
    /// Future contract
    Future,
    /// Option contract
    Option,
    /// Currency pair
    Currency,
}

/// Option type for derivatives
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OptionType {
    /// Call option
    Call,
    /// Put option  
    Put,
}

/// Complete instrument definition with WAL support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instrument {
    /// Unique instrument identifier within venue
    pub instrument_token: u32,

    /// Trading symbol (e.g., "NIFTY24DEC24000CE")
    pub trading_symbol: String,

    /// Exchange symbol (e.g., "NIFTY")
    pub exchange_symbol: String,

    /// Display name
    pub name: String,

    /// Instrument type
    pub instrument_type: InstrumentType,

    /// Exchange segment (e.g., "NSE", "NFO", "CDS")
    pub segment: String,

    /// Exchange (e.g., "NSE", "BSE")
    pub exchange: String,

    /// Expiry timestamp for derivatives (nanoseconds since epoch)
    pub expiry: Option<u64>,

    /// Strike price for options (fixed-point)
    pub strike: Option<Px>,

    /// Option type (for options)
    pub option_type: Option<OptionType>,

    /// Tick size (fixed-point)
    pub tick_size: Px,

    /// Lot size (minimum quantity)
    pub lot_size: u32,

    /// Last price (fixed-point, updated during market hours)
    pub last_price: Option<Px>,

    /// Timestamp of last update (nanoseconds)
    pub last_update: u64,

    /// Is tradable
    pub tradable: bool,

    /// Additional metadata (kept minimal for performance)
    pub metadata: FxHashMap<String, String>,
}

impl Instrument {
    /// Create new instrument with current timestamp
    pub fn new(
        instrument_token: u32,
        trading_symbol: String,
        exchange_symbol: String,
        name: String,
        instrument_type: InstrumentType,
        segment: String,
        exchange: String,
    ) -> Self {
        Self {
            instrument_token,
            trading_symbol,
            exchange_symbol,
            name,
            instrument_type,
            segment,
            exchange,
            expiry: None,
            strike: None,
            option_type: None,
            tick_size: Px::new(0.05), // Default tick size
            lot_size: 1,
            last_price: None,
            last_update: Ts::now().as_nanos(),
            tradable: true,
            metadata: FxHashMap::default(),
        }
    }

    /// Check if this instrument is active (not expired)
    pub fn is_active(&self) -> bool {
        match self.expiry {
            Some(expiry_ns) => expiry_ns > Ts::now().as_nanos(),
            None => true, // No expiry means always active
        }
    }

    /// Check if this is a spot instrument
    pub fn is_spot(&self) -> bool {
        matches!(
            self.instrument_type,
            InstrumentType::Equity | InstrumentType::Index
        ) && self.expiry.is_none()
    }

    /// Check if this is a futures instrument
    pub fn is_futures(&self) -> bool {
        matches!(self.instrument_type, InstrumentType::Future) && self.expiry.is_some()
    }

    /// Get current month futures symbol for this underlying
    pub fn get_current_month_futures_symbol(&self) -> Option<String> {
        if !self.is_spot() {
            return None;
        }

        // Generate current month futures symbol pattern
        let now = chrono::Utc::now();
        let year = now.format("%y").to_string();
        let month = match now.month() {
            1 => "JAN",
            2 => "FEB",
            3 => "MAR",
            4 => "APR",
            5 => "MAY",
            6 => "JUN",
            7 => "JUL",
            8 => "AUG",
            9 => "SEP",
            10 => "OCT",
            11 => "NOV",
            12 => "DEC",
            _ => return None,
        };

        Some(format!("{}{}{}FUT", self.exchange_symbol, year, month))
    }
}

impl WalEntry for Instrument {
    fn timestamp(&self) -> Ts {
        Ts::from_nanos(self.last_update)
    }
}

/// Zerodha CSV instrument format for parsing
#[derive(Debug, Deserialize)]
pub struct ZerodhaInstrumentCsv {
    pub instrument_token: u32,
    pub exchange_token: u32,
    pub tradingsymbol: String,
    pub name: Option<String>,
    pub last_price: f64,
    pub expiry: Option<String>,
    pub strike: f64,
    pub tick_size: f64,
    pub lot_size: u32,
    pub instrument_type: String,
    pub segment: String,
    pub exchange: String,
}

impl From<ZerodhaInstrumentCsv> for Instrument {
    fn from(z: ZerodhaInstrumentCsv) -> Self {
        let instrument_type = match z.instrument_type.as_str() {
            "EQ" => InstrumentType::Equity,
            "INDEX" => InstrumentType::Index,
            "FUT" => InstrumentType::Future,
            "CE" => InstrumentType::Option,
            "PE" => InstrumentType::Option,
            "CUR" => InstrumentType::Currency,
            _ => InstrumentType::Equity,
        };

        let option_type = match z.instrument_type.as_str() {
            "CE" => Some(OptionType::Call),
            "PE" => Some(OptionType::Put),
            _ => None,
        };

        // Parse expiry date and convert to nanoseconds
        let expiry = z
            .expiry
            .as_ref()
            .and_then(|e| {
                if e.is_empty() {
                    None
                } else {
                    chrono::NaiveDate::parse_from_str(e, "%Y-%m-%d").ok()
                }
            })
            .and_then(|d| d.and_hms_opt(15, 30, 0))
            .map(|dt| {
                let utc_dt =
                    chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc);
                // SAFETY: timestamp_nanos_opt returns i64, but for dates after 1970 it's positive
                // and fits safely in u64. For dates before 1970 we default to 0.
                // SAFETY: max(0) ensures non-negative value safe to cast to u64
                utc_dt.timestamp_nanos_opt().unwrap_or(0).max(0) as u64
            });

        Self {
            instrument_token: z.instrument_token,
            trading_symbol: z.tradingsymbol,
            exchange_symbol: z.name.clone().unwrap_or_default(),
            name: z.name.unwrap_or_default(),
            instrument_type,
            segment: z.segment,
            exchange: z.exchange,
            expiry,
            strike: if z.strike > 0.0 {
                Some(Px::new(z.strike))
            } else {
                None
            },
            option_type,
            tick_size: Px::new(z.tick_size),
            lot_size: z.lot_size,
            last_price: if z.last_price > 0.0 {
                Some(Px::new(z.last_price))
            } else {
                None
            },
            last_update: Ts::now().as_nanos(),
            tradable: true,
            metadata: FxHashMap::default(),
        }
    }
}

/// Instrument query filters
#[derive(Debug, Clone, Default)]
pub struct InstrumentFilter {
    pub instrument_type: Option<InstrumentType>,
    pub segment: Option<String>,
    pub exchange: Option<String>,
    pub underlying: Option<String>,
    pub active_only: bool,
}

impl InstrumentFilter {
    /// Create filter for spot instruments
    pub fn spot() -> Self {
        Self {
            instrument_type: Some(InstrumentType::Equity),
            active_only: true,
            ..Default::default()
        }
    }

    /// Create filter for futures of specific underlying
    pub fn futures(underlying: &str) -> Self {
        Self {
            instrument_type: Some(InstrumentType::Future),
            underlying: Some(underlying.to_string()),
            active_only: true,
            ..Default::default()
        }
    }

    /// Create filter for indices
    pub fn indices() -> Self {
        Self {
            instrument_type: Some(InstrumentType::Index),
            active_only: true,
            ..Default::default()
        }
    }

    /// Check if instrument matches filter
    pub fn matches(&self, instrument: &Instrument) -> bool {
        if let Some(ref inst_type) = self.instrument_type {
            if instrument.instrument_type != *inst_type {
                return false;
            }
        }

        if let Some(ref segment) = self.segment {
            if instrument.segment != *segment {
                return false;
            }
        }

        if let Some(ref exchange) = self.exchange {
            if instrument.exchange != *exchange {
                return false;
            }
        }

        if let Some(ref underlying) = self.underlying {
            if instrument.exchange_symbol != *underlying {
                return false;
            }
        }

        if self.active_only && !instrument.is_active() {
            return false;
        }

        true
    }
}
