//! Production-grade instrument management system
//!
//! Handles daily instrument updates from multiple exchanges with
//! caching, persistence, and efficient lookups.

#[allow(unused_imports)]
use crate::{Px, Symbol};
use anyhow::Result;
use chrono::{DateTime, Local, Timelike, Utc};
use rustc_hash::{FxBuildHasher, FxHashMap};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Type alias for option chain mapping: (underlying, expiry) -> token list
type OptionChainMap = Arc<RwLock<FxHashMap<(String, DateTime<Utc>), Vec<u32>>>>;

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
    /// Commodity instrument
    Commodity,
}

/// Option type for derivatives
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OptionType {
    /// Call option
    Call,
    /// Put option
    Put,
}

/// Complete instrument definition
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

    /// Expiry date for derivatives
    pub expiry: Option<DateTime<Utc>>,

    /// Strike price for options
    pub strike: Option<f64>,

    /// Option type (for options)
    pub option_type: Option<OptionType>,

    /// Tick size (minimum price movement)
    pub tick_size: f64,

    /// Lot size (minimum quantity)
    pub lot_size: u32,

    /// Last price (updated during market hours)
    pub last_price: Option<f64>,

    /// Timestamp of last update
    pub last_update: DateTime<Utc>,

    /// Is tradable
    pub tradable: bool,

    /// Additional metadata
    pub metadata: FxHashMap<String, String>,
}

/// Zerodha-specific instrument format
#[derive(Debug, Clone, Deserialize)]
pub struct ZerodhaInstrument {
    /// Instrument token
    pub instrument_token: u32,
    /// Exchange token
    pub exchange_token: u32,
    /// Trading symbol
    pub tradingsymbol: String,
    /// Name
    pub name: String,
    /// Last price
    pub last_price: Px,
    /// Expiry date
    pub expiry: String,
    /// Strike price
    pub strike: Px,
    /// Tick size
    pub tick_size: f64,
    /// Lot size
    pub lot_size: u32,
    /// Instrument type
    pub instrument_type: String,
    /// Segment
    pub segment: String,
    /// Exchange
    pub exchange: String,
}

/// Instrument store for efficient lookups
#[derive(Debug, Clone)]
pub struct InstrumentStore {
    /// All instruments by token
    by_token: Arc<RwLock<FxHashMap<u32, Instrument>>>,

    /// Instruments by trading symbol
    by_symbol: Arc<RwLock<FxHashMap<String, Vec<u32>>>>,

    /// Instruments by exchange symbol
    by_exchange_symbol: Arc<RwLock<FxHashMap<String, Vec<u32>>>>,

    /// Active futures by underlying
    active_futures: Arc<RwLock<FxHashMap<String, Vec<u32>>>>,

    /// Option chains by underlying and expiry
    option_chains: OptionChainMap,

    /// Indices
    indices: Arc<RwLock<HashSet<u32>>>,

    /// Last fetch timestamp
    _last_fetch: Arc<RwLock<Option<DateTime<Utc>>>>,

    /// `ETags` for caching
    _etags: Arc<RwLock<FxHashMap<String, String>>>,
}

impl Default for InstrumentStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InstrumentStore {
    /// Create new instrument store
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_token: Arc::new(RwLock::new(FxHashMap::with_capacity_and_hasher(
                10_000,
                FxBuildHasher,
            ))),
            by_symbol: Arc::new(RwLock::new(FxHashMap::with_capacity_and_hasher(
                5_000,
                FxBuildHasher,
            ))),
            by_exchange_symbol: Arc::new(RwLock::new(FxHashMap::with_capacity_and_hasher(
                1_000,
                FxBuildHasher,
            ))),
            active_futures: Arc::new(RwLock::new(FxHashMap::with_capacity_and_hasher(
                500,
                FxBuildHasher,
            ))),
            option_chains: Arc::new(RwLock::new(FxHashMap::with_capacity_and_hasher(
                200,
                FxBuildHasher,
            ))),
            indices: Arc::new(RwLock::new(HashSet::with_capacity(50))),
            _last_fetch: Arc::new(RwLock::new(None)),
            _etags: Arc::new(RwLock::new(FxHashMap::with_capacity_and_hasher(
                10,
                FxBuildHasher,
            ))),
        }
    }

    /// Load instruments from cache file
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed
    pub async fn load_from_cache(&self, path: &str) -> Result<()> {
        let data = tokio::fs::read_to_string(path).await?;
        let instruments: Vec<Instrument> = serde_json::from_str(&data)?;

        for instrument in instruments {
            self.add_instrument(instrument).await?;
        }

        Ok(())
    }

    /// Save instruments to cache file
    ///
    /// # Errors
    /// Returns an error if the file cannot be written or serialized
    pub async fn save_to_cache(&self, path: &str) -> Result<()> {
        let instruments: Vec<Instrument> = {
            let by_token = self.by_token.read().await;
            by_token.values().cloned().collect()
        };

        let data = serde_json::to_string_pretty(&instruments)?;
        tokio::fs::write(path, data).await?;

        Ok(())
    }

    /// Add instrument to store
    ///
    /// # Errors
    /// Returns an error if the instrument cannot be stored due to lock contention
    pub async fn add_instrument(&self, instrument: Instrument) -> Result<()> {
        let token = instrument.instrument_token;

        // Update main index
        {
            let mut by_token = self.by_token.write().await;
            by_token.insert(token, instrument.clone());
        }

        // Update symbol indices
        {
            let mut by_symbol = self.by_symbol.write().await;
            by_symbol
                .entry(instrument.trading_symbol.clone())
                .or_insert_with(Vec::new)
                .push(token);
        }

        {
            let mut by_exchange_symbol = self.by_exchange_symbol.write().await;
            by_exchange_symbol
                .entry(instrument.exchange_symbol.clone())
                .or_insert_with(Vec::new)
                .push(token);
        }

        // Update type-specific indices
        match instrument.instrument_type {
            InstrumentType::Index => {
                let mut indices = self.indices.write().await;
                indices.insert(token);
            }
            InstrumentType::Future => {
                if let Some(_expiry) = instrument.expiry {
                    let mut futures = self.active_futures.write().await;
                    futures
                        .entry(instrument.exchange_symbol.clone())
                        .or_insert_with(Vec::new)
                        .push(token);
                }
            }
            InstrumentType::Option => {
                if let Some(expiry) = instrument.expiry {
                    let mut chains = self.option_chains.write().await;
                    chains
                        .entry((instrument.exchange_symbol.clone(), expiry))
                        .or_insert_with(Vec::new)
                        .push(token);
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Get instrument by token
    pub async fn get_by_token(&self, token: u32) -> Option<Instrument> {
        let by_token = self.by_token.read().await;
        by_token.get(&token).cloned()
    }

    /// Get instruments by trading symbol
    pub async fn get_by_symbol(&self, symbol: &str) -> Vec<Instrument> {
        let by_symbol = self.by_symbol.read().await;
        let by_token = self.by_token.read().await;

        by_symbol
            .get(symbol)
            .map(|tokens| {
                tokens
                    .iter()
                    .filter_map(|t| by_token.get(t).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get active futures for underlying
    pub async fn get_active_futures(&self, underlying: &str) -> Vec<Instrument> {
        let futures = self.active_futures.read().await;
        let by_token = self.by_token.read().await;

        futures
            .get(underlying)
            .map(|tokens| {
                let now = Utc::now();
                tokens
                    .iter()
                    .filter_map(|t| by_token.get(t).cloned())
                    .filter(|i| i.expiry.is_some_and(|e| e > now))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get option chain for underlying and expiry
    pub async fn get_option_chain(
        &self,
        underlying: &str,
        expiry: DateTime<Utc>,
    ) -> Vec<Instrument> {
        let chains = self.option_chains.read().await;
        let by_token = self.by_token.read().await;

        chains
            .get(&(underlying.to_string(), expiry))
            .map(|tokens| {
                tokens
                    .iter()
                    .filter_map(|t| by_token.get(t).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all indices
    pub async fn get_indices(&self) -> Vec<Instrument> {
        let indices = self.indices.read().await;
        let by_token = self.by_token.read().await;

        indices
            .iter()
            .filter_map(|t| by_token.get(t).cloned())
            .collect()
    }

    /// Clear all instruments
    pub async fn clear(&self) {
        self.by_token.write().await.clear();
        self.by_symbol.write().await.clear();
        self.by_exchange_symbol.write().await.clear();
        self.active_futures.write().await.clear();
        self.option_chains.write().await.clear();
        self.indices.write().await.clear();
    }

    /// Get total instrument count
    pub async fn count(&self) -> usize {
        let by_token = self.by_token.read().await;
        by_token.len()
    }

    /// Check if refresh is needed (daily at 8:00 AM IST)
    #[must_use]
    pub fn should_refresh(&self) -> bool {
        let now = Local::now();
        let hour = now.hour();
        let minute = now.minute();

        // Refresh at 8:00 AM IST
        hour == 8 && minute < 5
    }
}

/// Convert Zerodha instrument to generic instrument
impl From<ZerodhaInstrument> for Instrument {
    fn from(z: ZerodhaInstrument) -> Self {
        let instrument_type = match z.instrument_type.as_str() {
            "INDEX" => InstrumentType::Index,
            "FUT" => InstrumentType::Future,
            "CE" | "PE" => InstrumentType::Option,
            "CUR" => InstrumentType::Currency,
            "COM" => InstrumentType::Commodity,
            _ => InstrumentType::Equity,
        };

        let option_type = match z.instrument_type.as_str() {
            "CE" => Some(OptionType::Call),
            "PE" => Some(OptionType::Put),
            _ => None,
        };

        let expiry = if z.expiry.is_empty() {
            None
        } else {
            chrono::NaiveDate::parse_from_str(&z.expiry, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(15, 30, 0))
                .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
        };

        let strike = if z.strike.as_f64() > 0.0 {
            Some(z.strike.as_f64())
        } else {
            None
        };

        Self {
            instrument_token: z.instrument_token,
            trading_symbol: z.tradingsymbol.clone(),
            exchange_symbol: z.name.clone(),
            name: z.name,
            instrument_type,
            segment: z.segment,
            exchange: z.exchange,
            expiry,
            strike,
            option_type,
            tick_size: z.tick_size,
            lot_size: z.lot_size,
            last_price: if z.last_price.as_f64() > 0.0 {
                Some(z.last_price.as_f64())
            } else {
                None
            },
            last_update: Utc::now(),
            tradable: true,
            metadata: FxHashMap::with_capacity_and_hasher(4, FxBuildHasher),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_instrument_store() {
        let store = InstrumentStore::new();

        let instrument = Instrument {
            instrument_token: 256_265,
            trading_symbol: "NIFTY".to_string(),
            exchange_symbol: "NIFTY".to_string(),
            name: "Nifty 50".to_string(),
            instrument_type: InstrumentType::Index,
            segment: "INDICES".to_string(),
            exchange: "NSE".to_string(),
            expiry: None,
            strike: None,
            option_type: None,
            tick_size: 0.05,
            lot_size: 1,
            last_price: Some(25000.0),
            last_update: Utc::now(),
            tradable: false,
            metadata: FxHashMap::with_capacity_and_hasher(4, FxBuildHasher),
        };

        assert!(store.add_instrument(instrument.clone()).await.is_ok());

        // Test retrieval
        let retrieved = store.get_by_token(256_265).await;
        assert!(retrieved.is_some());
        let Some(inst) = retrieved else {
            unreachable!("Expected to retrieve instrument");
        };
        assert_eq!(inst.trading_symbol, "NIFTY");

        // Test symbol lookup
        let by_symbol = store.get_by_symbol("NIFTY").await;
        assert_eq!(by_symbol.len(), 1);

        // Test indices
        let indices = store.get_indices().await;
        assert_eq!(indices.len(), 1);
    }
}
