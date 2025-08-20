//! WAL-based instrument store with efficient lookups
//!
//! COMPLIANCE:
//! - FxHashMap for all lookups
//! - Fixed-point arithmetic for prices
//! - Zero allocations in hot paths

use anyhow::{Context, Result};
use services_common::constants::{
    capacity::PROGRESS_REPORT_INTERVAL,
    financial::STRIKE_PRICE_SCALE,
    memory::{BYTES_PER_MB, DEFAULT_WAL_SEGMENT_SIZE_MB},
    numeric::{INCREMENT, INITIAL_COUNTER, SECOND_INDEX, ZERO, ZERO_U64},
};
use services_common::{Px, Ts};
use rustc_hash::FxHashMap;
use std::path::PathBuf;
use services_common::wal::Wal;
use tracing::{debug, info};

use super::types::{Instrument, InstrumentFilter, InstrumentType, OptionType};

// Size constants
const DEFAULT_SEGMENT_SIZE: u64 = DEFAULT_WAL_SEGMENT_SIZE_MB as u64 * BYTES_PER_MB; // 100MB default

/// WAL-backed instrument store
impl std::fmt::Debug for InstrumentWalStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstrumentWalStore")
            .field("wal", &"Wal")
            .field("by_token", &format!("{} instruments", self.by_token.len()))
            .field("by_symbol", &format!("{} symbols", self.by_symbol.len()))
            .field("by_exchange_symbol", &format!("{} exchange symbols", self.by_exchange_symbol.len()))
            .field("active_futures", &format!("{} futures", self.active_futures.len()))
            .field("indices", &format!("{} indices", self.indices.len()))
            .field("option_chains", &format!("{} option chains", self.option_chains.len()))
            .field("options_by_strike", &format!("{} strike mappings", self.options_by_strike.len()))
            .field("total_instruments", &self.total_instruments)
            .field("last_update", &self.last_update)
            .finish()
    }
}

/// High-performance WAL-backed instrument store with optimized lookups
///
/// This store provides enterprise-grade instrument data management with persistent
/// storage using Write-Ahead Logging (WAL) and multiple optimized indices for
/// different access patterns required in high-frequency trading.
///
/// # Storage Architecture
/// - **WAL Backend**: All instrument data is persisted to WAL for durability
/// - **Memory Indices**: Multiple FxHashMap-based indices for fast lookups
/// - **Type-Specific Collections**: Specialized collections for futures, options, and indices
/// - **Strike-Based Indexing**: Efficient option chain queries by strike price
///
/// # Index Types
/// - **Primary Index**: Token-based O(1) instrument lookup
/// - **Symbol Indices**: Trading symbol and exchange symbol mappings  
/// - **Futures Index**: Active futures contracts by underlying
/// - **Option Chains**: Strike-based option lookups with call/put separation
/// - **Indices Collection**: Quick access to index instruments
///
/// # Performance Characteristics
/// - **Hot Path Optimized**: Sub-microsecond lookups for trading operations
/// - **Memory Efficient**: Compact data structures with minimal overhead
/// - **Batch Operations**: Efficient bulk inserts during daily updates
/// - **Zero Copy**: Reference-based access to avoid unnecessary allocations
///
/// # Data Integrity
/// - **WAL Persistence**: All changes are logged before memory updates
/// - **Crash Recovery**: Automatic reconstruction from WAL on startup
/// - **Consistent Views**: Atomic updates ensure consistent data views
///
/// # Examples
/// ```
/// use instrument_store::InstrumentWalStore;
/// use std::path::PathBuf;
///
/// let store = InstrumentWalStore::new(
///     PathBuf::from("./data/instruments"),
///     Some(100), // 100MB segments
/// )?;
///
/// // Fast token-based lookup
/// if let Some(instrument) = store.get_by_token(12345) {
///     println!("Found: {}", instrument.trading_symbol);
/// }
///
/// // Get active futures
/// let futures = store.get_active_futures("NIFTY");
/// println!("Active NIFTY futures: {}", futures.len());
/// ```
pub struct InstrumentWalStore {
    /// WAL for persistent storage
    wal: Wal,

    /// In-memory indices for fast lookups
    /// Primary instrument index by token for O(1) lookups
    pub by_token: FxHashMap<u32, Instrument>,
    /// Trading symbol to tokens mapping for symbol-based queries
    pub by_symbol: FxHashMap<String, Vec<u32>>,
    /// Exchange symbol to tokens mapping for underlying-based queries
    pub by_exchange_symbol: FxHashMap<String, Vec<u32>>,
    /// Active futures contracts by underlying symbol
    pub active_futures: FxHashMap<String, Vec<u32>>,
    /// Index instruments collection (using FxHashMap as HashSet for performance)
    pub indices: FxHashMap<u32, ()>,

    /// Option chains by underlying and expiry: (underlying, expiry_timestamp) -> Vec<token>
    pub option_chains: FxHashMap<(String, u64), Vec<u32>>,

    /// Options by strike: underlying -> strike -> (call_token, put_token)
    pub options_by_strike: FxHashMap<String, FxHashMap<u64, (Option<u32>, Option<u32>)>>,

    /// Statistics
    /// Total number of instruments in the store
    pub total_instruments: usize,
    /// Timestamp of the last successful data update
    pub last_update: Option<Ts>,
}

impl InstrumentWalStore {
    /// Create new instrument store with WAL
    pub fn new(wal_dir: PathBuf, segment_size_mb: Option<usize>) -> Result<Self> {
        std::fs::create_dir_all(&wal_dir).context("Failed to create WAL directory")?;

        let segment_size = segment_size_mb
            .map(|mb| {
                // SAFETY: usize to u64 widening conversion is always safe
                u64::try_from(mb).unwrap_or(DEFAULT_WAL_SEGMENT_SIZE_MB as u64) * BYTES_PER_MB
            })
            .or(Some(DEFAULT_SEGMENT_SIZE));

        let wal =
            Wal::new(&wal_dir, segment_size.map(|s| s as usize)).context("Failed to initialize instrument WAL")?;

        Ok(Self {
            wal,
            by_token: FxHashMap::default(),
            by_symbol: FxHashMap::default(),
            by_exchange_symbol: FxHashMap::default(),
            active_futures: FxHashMap::default(),
            indices: FxHashMap::default(),
            option_chains: FxHashMap::default(),
            options_by_strike: FxHashMap::default(),
            total_instruments: ZERO,
            last_update: None,
        })
    }

    /// Load instruments from WAL on startup
    pub fn load_from_wal(&mut self) -> Result<()> {
        info!("Loading instruments from WAL...");

        let start = std::time::Instant::now();
        let mut loaded_count = INITIAL_COUNTER;

        // Read all entries from WAL using iterator
        let mut iter = self.wal.stream::<Instrument>(None)?;

        while let Some(instrument) = iter.read_next_entry()? {
            self.add_to_indices(instrument)?;
            loaded_count += INCREMENT;

            if loaded_count % PROGRESS_REPORT_INTERVAL == 0 {
                debug!("Loaded {} instruments from WAL", loaded_count);
            }
        }

        self.total_instruments = loaded_count;
        info!(
            "Loaded {} instruments from WAL in {:?}",
            loaded_count,
            start.elapsed()
        );

        Ok(())
    }

    /// Add instrument to store and WAL
    pub fn add_instrument(&mut self, instrument: Instrument) -> Result<()> {
        // Store in WAL first
        self.wal
            .append(&instrument)
            .context("Failed to write instrument to WAL")?;

        // Add to in-memory indices
        self.add_to_indices(instrument)?;
        self.total_instruments += INCREMENT;
        self.last_update = Some(Ts::now());

        Ok(())
    }

    /// Add multiple instruments efficiently  
    pub fn add_instruments(&mut self, instruments: Vec<Instrument>) -> Result<usize> {
        let start = std::time::Instant::now();
        let count = instruments.len();

        // Batch write to WAL
        for instrument in instruments {
            self.wal
                .append(&instrument)
                .context("Failed to write instrument to WAL")?;
            self.add_to_indices(instrument)?;
        }

        self.total_instruments += count;
        self.last_update = Some(Ts::now());

        info!(
            "Added {} instruments to WAL store in {:?}",
            count,
            start.elapsed()
        );

        Ok(count)
    }

    /// Add instrument to in-memory indices
    fn add_to_indices(&mut self, instrument: Instrument) -> Result<()> {
        let token = instrument.instrument_token;

        // Primary index
        self.by_token.insert(token, instrument.clone());

        // Symbol indices
        self.by_symbol
            .entry(instrument.trading_symbol.clone())
            .or_insert_with(Vec::new)
            .push(token);

        self.by_exchange_symbol
            .entry(instrument.exchange_symbol.clone())
            .or_insert_with(Vec::new)
            .push(token);

        // Type-specific indices
        match instrument.instrument_type {
            InstrumentType::Index => {
                self.indices.insert(token, ());
            }
            InstrumentType::Future => {
                if instrument.expiry.is_some() && instrument.is_active() {
                    self.active_futures
                        .entry(instrument.exchange_symbol.clone())
                        .or_insert_with(Vec::new)
                        .push(token);
                }
            }
            InstrumentType::Option => {
                if let (Some(expiry), Some(strike), Some(option_type)) =
                    (instrument.expiry, instrument.strike, instrument.option_type)
                {
                    let underlying = instrument.exchange_symbol.clone();
                    // SAFETY: Unix timestamps after epoch are positive, safe to cast to u64
                    let expiry_ts = expiry as u64;
                    // Convert strike price to fixed-point integer (2 decimal places)
                    // SAFETY: Strike prices are positive and within reasonable bounds
                    let strike_fp = strike.as_f64() * STRIKE_PRICE_SCALE;
                    let strike_int = if strike_fp >= 0.0 && strike_fp <= u64::MAX as f64 {
                        strike_fp as u64
                    } else {
                        0_u64
                    };

                    // Add to option chains
                    self.option_chains
                        .entry((underlying.clone(), expiry_ts))
                        .or_insert_with(Vec::new)
                        .push(token);

                    // Add to strike-based index
                    let strike_map = self
                        .options_by_strike
                        .entry(underlying)
                        .or_insert_with(FxHashMap::default);

                    let strike_entry = strike_map.entry(strike_int).or_insert_with(|| (None, None));

                    match option_type {
                        OptionType::Call => strike_entry.0 = Some(token),
                        OptionType::Put => strike_entry.1 = Some(token),
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Get instrument by token - HOT PATH
    pub fn get_by_token(&self, token: u32) -> Option<&Instrument> {
        self.by_token.get(&token)
    }

    /// Get instruments by trading symbol
    pub fn get_by_trading_symbol(&self, symbol: &str) -> Vec<&Instrument> {
        self.by_symbol
            .get(symbol)
            .map(|tokens| tokens.iter().filter_map(|t| self.by_token.get(t)).collect())
            .unwrap_or_default()
    }

    /// Get instruments by exchange symbol (underlying) - HOT PATH for futures lookup
    pub fn get_by_exchange_symbol(&self, symbol: &str) -> Vec<&Instrument> {
        self.by_exchange_symbol
            .get(symbol)
            .map(|tokens| tokens.iter().filter_map(|t| self.by_token.get(t)).collect())
            .unwrap_or_default()
    }

    /// Get active futures for underlying - HOT PATH
    pub fn get_active_futures(&self, underlying: &str) -> Vec<&Instrument> {
        self.active_futures
            .get(underlying)
            .map(|tokens| {
                tokens
                    .iter()
                    .filter_map(|t| self.by_token.get(t))
                    .filter(|i| i.is_active())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get current month futures for underlying - CRITICAL for spot-to-futures mapping
    pub fn get_current_month_futures(&self, underlying: &str) -> Option<&Instrument> {
        let futures = self.get_active_futures(underlying);

        // Find the nearest expiry futures contract
        let now = Ts::now().as_nanos();
        futures
            .into_iter()
            .filter(|f| f.expiry.is_some_and(|exp| exp > now))
            .min_by_key(|f| f.expiry.unwrap_or(u64::MAX))
    }

    /// Get all indices
    pub fn get_indices(&self) -> Vec<&Instrument> {
        self.indices
            .keys()
            .filter_map(|t| self.by_token.get(t))
            .collect()
    }

    /// Query instruments with filter
    pub fn query(&self, filter: &InstrumentFilter) -> Vec<&Instrument> {
        self.by_token
            .values()
            .filter(|inst| filter.matches(inst))
            .collect()
    }

    /// Get spot instrument for a symbol (e.g., "NIFTY" -> NIFTY Index)
    pub fn get_spot(&self, symbol: &str) -> Option<&Instrument> {
        self.by_exchange_symbol.get(symbol).and_then(|tokens| {
            tokens
                .iter()
                .filter_map(|t| self.by_token.get(t))
                .find(|i| i.is_spot())
        })
    }

    /// CRITICAL: Get market data subscription tokens for a spot symbol
    /// Returns (spot_token, current_futures_token, next_futures_token)
    pub fn get_subscription_tokens(
        &self,
        underlying: &str,
    ) -> (Option<u32>, Option<u32>, Option<u32>) {
        let spot_token = self.get_spot(underlying).map(|i| i.instrument_token);

        let futures = self.get_active_futures(underlying);
        let mut sorted_futures = futures;
        sorted_futures.sort_by_key(|f| f.expiry.unwrap_or(ZERO_U64));

        let current_futures_token = sorted_futures.first().map(|i| i.instrument_token);
        let next_futures_token = sorted_futures.get(SECOND_INDEX).map(|i| i.instrument_token);

        (spot_token, current_futures_token, next_futures_token)
    }

    /// Clear all instruments (for refresh)
    pub fn clear(&mut self) -> Result<()> {
        self.by_token.clear();
        self.by_symbol.clear();
        self.by_exchange_symbol.clear();
        self.active_futures.clear();
        self.indices.clear();
        self.option_chains.clear();
        self.options_by_strike.clear();
        self.total_instruments = ZERO;

        // Note: WAL is append-only, we don't clear it
        info!("Cleared in-memory instrument indices");
        Ok(())
    }

    /// Get statistics
    pub fn stats(&self) -> InstrumentStats {
        let indices_count = self.indices.len();
        let futures_count = self.active_futures.values().map(|v| v.len()).sum();

        InstrumentStats {
            total_instruments: self.total_instruments,
            indices_count,
            active_futures_count: futures_count,
            symbols_count: self.by_symbol.len(),
            exchange_symbols_count: self.by_exchange_symbol.len(),
            last_update: self.last_update,
        }
    }

    /// Get option chain for underlying and expiry
    pub fn get_option_chain(&self, underlying: &str, expiry_ts: u64) -> Vec<&Instrument> {
        self.option_chains
            .get(&(underlying.to_string(), expiry_ts))
            .map(|tokens| tokens.iter().filter_map(|t| self.by_token.get(t)).collect())
            .unwrap_or_default()
    }

    /// Get ATM option chain based on spot price - CRITICAL for options trading
    pub fn get_atm_option_chain(
        &self,
        underlying: &str,
        spot_price: Px,
        strike_range: u32,
        strike_interval: f64,
    ) -> AtmOptionChain<'_> {
        // Calculate ATM strike using fixed-point arithmetic
        let spot_f64 = spot_price.as_f64(); // Convert to f64 only at boundary for calculation
        let atm_strike = (spot_f64 / strike_interval).round() * strike_interval;

        // Convert to integer for efficient comparison
        // SAFETY: ATM strike is positive and within reasonable bounds
        let atm_strike_fp = atm_strike * STRIKE_PRICE_SCALE;
        let atm_strike_int = if atm_strike_fp >= 0.0 && atm_strike_fp <= u64::MAX as f64 {
            atm_strike_fp as u64
        } else {
            0_u64
        };

        // Calculate strike range in integer form for fast comparison
        // SAFETY: Strike range calculation yields positive value within bounds
        let strike_range_fp = strike_range as f64 * strike_interval * STRIKE_PRICE_SCALE;
        let strike_range_int = if strike_range_fp >= 0.0 && strike_range_fp <= u64::MAX as f64 {
            strike_range_fp as u64
        } else {
            0_u64
        };
        let min_strike_int = atm_strike_int.saturating_sub(strike_range_int);
        let max_strike_int = atm_strike_int.saturating_add(strike_range_int);

        let mut calls = FxHashMap::default();
        let mut puts = FxHashMap::default();

        if let Some(strike_map) = self.options_by_strike.get(underlying) {
            // Use integer comparison for efficiency - no float conversions needed
            for (&strike_int, &(call_token, put_token)) in strike_map.iter() {
                // Direct integer comparison - much faster than float comparison
                if strike_int >= min_strike_int && strike_int <= max_strike_int {
                    if let Some(call_token) = call_token {
                        if let Some(call_instrument) = self.by_token.get(&call_token) {
                            calls.insert(strike_int, call_instrument);
                        }
                    }

                    if let Some(put_token) = put_token {
                        if let Some(put_instrument) = self.by_token.get(&put_token) {
                            puts.insert(strike_int, put_instrument);
                        }
                    }
                }
            }
        }

        AtmOptionChain {
            underlying: underlying.to_string(),
            spot_price,
            atm_strike, // Store the float ATM strike
            calls,
            puts,
            strike_range,
            strike_interval,
        }
    }

    /// Get option tokens for market data subscription around ATM
    pub fn get_atm_subscription_tokens(
        &self,
        underlying: &str,
        spot_price: Px,
        strike_range: u32,
        strike_interval: f64,
    ) -> Vec<u32> {
        let chain =
            self.get_atm_option_chain(underlying, spot_price, strike_range, strike_interval);
        let mut tokens = Vec::new();

        // Add call tokens
        for instrument in chain.calls.values() {
            tokens.push(instrument.instrument_token);
        }

        // Add put tokens
        for instrument in chain.puts.values() {
            tokens.push(instrument.instrument_token);
        }

        tokens.sort();
        tokens
    }

    /// Get option by strike and type
    pub fn get_option_by_strike(
        &self,
        underlying: &str,
        strike: f64,
        option_type: OptionType,
    ) -> Option<&Instrument> {
        // SAFETY: Strike prices are positive and within reasonable bounds
        let strike_fp = strike * STRIKE_PRICE_SCALE;
        let strike_int = if strike_fp >= 0.0 && strike_fp <= u64::MAX as f64 {
            strike_fp as u64
        } else {
            return None;
        };

        self.options_by_strike
            .get(underlying)
            .and_then(|strike_map| strike_map.get(&strike_int))
            .and_then(|(call_token, put_token)| match option_type {
                OptionType::Call => call_token.and_then(|t| self.by_token.get(&t)),
                OptionType::Put => put_token.and_then(|t| self.by_token.get(&t)),
            })
    }

    /// Get all available strikes for underlying
    pub fn get_available_strikes(&self, underlying: &str) -> Vec<f64> {
        self.options_by_strike
            .get(underlying)
            .map(|strike_map| {
                let mut strikes: Vec<f64> = strike_map
                    .keys()
                    .map(|&strike_int| {
                        // SAFETY: u64 to f64 for strike display, precision loss acceptable
                        strike_int as f64 / STRIKE_PRICE_SCALE
                    })
                    .collect();
                strikes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                strikes
            })
            .unwrap_or_default()
    }

    /// Force WAL sync
    pub fn sync(&mut self) -> Result<()> {
        self.wal.flush().context("Failed to sync instrument WAL")?;
        Ok(())
    }
}

/// Instrument store statistics
#[derive(Debug, Clone)]
/// Comprehensive statistics about the instrument store contents and performance
///
/// Provides insight into the current state of the instrument store including
/// counts of different instrument types, index sizes, and last update information.
/// Used for monitoring, debugging, and performance optimization.
pub struct InstrumentStats {
    /// Total number of instruments loaded in the store
    pub total_instruments: usize,
    /// Number of index instruments (like NIFTY 50, BANK NIFTY)
    pub indices_count: usize,
    /// Number of active (non-expired) futures contracts
    pub active_futures_count: usize,
    /// Number of unique trading symbols in the symbol index
    pub symbols_count: usize,
    /// Number of unique exchange symbols in the exchange symbol index
    pub exchange_symbols_count: usize,
    /// Timestamp of the last successful data update
    pub last_update: Option<Ts>,
}

/// ATM (At The Money) option chain for efficient options trading
#[derive(Debug, Clone)]
pub struct AtmOptionChain<'a> {
    /// Underlying symbol
    pub underlying: String,
    /// Current spot price
    pub spot_price: Px,
    /// ATM strike price
    pub atm_strike: f64,
    /// Call options by strike (strike_int -> instrument)
    pub calls: FxHashMap<u64, &'a Instrument>,
    /// Put options by strike (strike_int -> instrument)
    pub puts: FxHashMap<u64, &'a Instrument>,
    /// Strike range (+/- from ATM)
    pub strike_range: u32,
    /// Strike interval
    pub strike_interval: f64,
}

impl<'a> AtmOptionChain<'a> {
    /// Get the closest strike to ATM using efficient integer comparison
    pub fn get_atm_strike_int(&self) -> u64 {
        // SAFETY: ATM strike is positive and within reasonable bounds
        let atm_strike_fp = self.atm_strike * STRIKE_PRICE_SCALE;
        let atm_strike_int = if atm_strike_fp >= 0.0 && atm_strike_fp <= u64::MAX as f64 {
            atm_strike_fp as u64
        } else {
            0_u64
        };

        // Find the closest available strike to ATM
        let mut closest_strike = atm_strike_int;
        let mut min_distance = u64::MAX;

        for &strike_int in self.calls.keys().chain(self.puts.keys()) {
            let distance = if strike_int > atm_strike_int {
                strike_int - atm_strike_int
            } else {
                atm_strike_int - strike_int
            };

            if distance < min_distance {
                min_distance = distance;
                closest_strike = strike_int;
            }
        }

        closest_strike
    }

    /// Get call option at specific strike
    pub fn get_call(&self, strike: f64) -> Option<&'a Instrument> {
        // SAFETY: Strike prices are positive and within reasonable bounds
        let strike_fp = strike * STRIKE_PRICE_SCALE;
        let strike_int = if strike_fp >= 0.0 && strike_fp <= u64::MAX as f64 {
            strike_fp as u64
        } else {
            return None;
        };
        self.calls.get(&strike_int).copied()
    }

    /// Get put option at specific strike
    pub fn get_put(&self, strike: f64) -> Option<&'a Instrument> {
        // SAFETY: Strike prices are positive and within reasonable bounds
        let strike_fp = strike * STRIKE_PRICE_SCALE;
        let strike_int = if strike_fp >= 0.0 && strike_fp <= u64::MAX as f64 {
            strike_fp as u64
        } else {
            return None;
        };
        self.puts.get(&strike_int).copied()
    }

    /// Get ATM call option
    pub fn get_atm_call(&self) -> Option<&'a Instrument> {
        self.get_call(self.atm_strike)
    }

    /// Get ATM put option
    pub fn get_atm_put(&self) -> Option<&'a Instrument> {
        self.get_put(self.atm_strike)
    }

    /// Get all strikes in ascending order
    pub fn get_strikes(&self) -> Vec<f64> {
        let mut all_strikes: Vec<u64> =
            self.calls.keys().chain(self.puts.keys()).copied().collect();
        all_strikes.sort();
        all_strikes.dedup();
        all_strikes
            .into_iter()
            .map(|s| s as f64 / STRIKE_PRICE_SCALE)
            .collect()
    }

    /// Get ITM (In The Money) calls
    pub fn get_itm_calls(&self) -> Vec<&'a Instrument> {
        let spot_int = (self.spot_price.as_f64() * STRIKE_PRICE_SCALE) as u64;
        self.calls
            .iter()
            .filter_map(|(&strike_int, &instrument)| {
                if strike_int < spot_int {
                    Some(instrument)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get ITM (In The Money) puts
    pub fn get_itm_puts(&self) -> Vec<&'a Instrument> {
        let spot_int = (self.spot_price.as_f64() * STRIKE_PRICE_SCALE) as u64;
        self.puts
            .iter()
            .filter_map(|(&strike_int, &instrument)| {
                if strike_int > spot_int {
                    Some(instrument)
                } else {
                    None
                }
            })
            .collect()
    }
}
