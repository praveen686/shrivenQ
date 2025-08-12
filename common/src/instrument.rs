//! Production-grade instrument management system
//! 
//! Handles daily instrument updates from multiple exchanges with
//! caching, persistence, and efficient lookups.

#[allow(unused_imports)]
use crate::Symbol;
use chrono::{DateTime, Utc, Local, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;

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
    pub metadata: HashMap<String, String>,
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
    pub last_price: f64,
    /// Expiry date
    pub expiry: String,
    /// Strike price
    pub strike: f64,
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
    by_token: Arc<RwLock<HashMap<u32, Instrument>>>,
    
    /// Instruments by trading symbol
    by_symbol: Arc<RwLock<HashMap<String, Vec<u32>>>>,
    
    /// Instruments by exchange symbol
    by_exchange_symbol: Arc<RwLock<HashMap<String, Vec<u32>>>>,
    
    /// Active futures by underlying
    active_futures: Arc<RwLock<HashMap<String, Vec<u32>>>>,
    
    /// Option chains by underlying and expiry
    option_chains: Arc<RwLock<HashMap<(String, DateTime<Utc>), Vec<u32>>>>,
    
    /// Indices
    indices: Arc<RwLock<HashSet<u32>>>,
    
    /// Last fetch timestamp
    _last_fetch: Arc<RwLock<Option<DateTime<Utc>>>>,
    
    /// ETags for caching
    _etags: Arc<RwLock<HashMap<String, String>>>,
}

impl InstrumentStore {
    /// Create new instrument store
    pub fn new() -> Self {
        Self {
            by_token: Arc::new(RwLock::new(HashMap::new())),
            by_symbol: Arc::new(RwLock::new(HashMap::new())),
            by_exchange_symbol: Arc::new(RwLock::new(HashMap::new())),
            active_futures: Arc::new(RwLock::new(HashMap::new())),
            option_chains: Arc::new(RwLock::new(HashMap::new())),
            indices: Arc::new(RwLock::new(HashSet::new())),
            _last_fetch: Arc::new(RwLock::new(None)),
            _etags: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Load instruments from cache file
    pub async fn load_from_cache(&self, path: &str) -> Result<()> {
        let data = tokio::fs::read_to_string(path).await?;
        let instruments: Vec<Instrument> = serde_json::from_str(&data)?;
        
        for instrument in instruments {
            self.add_instrument(instrument).await?;
        }
        
        Ok(())
    }
    
    /// Save instruments to cache file
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
            by_symbol.entry(instrument.trading_symbol.clone())
                .or_insert_with(Vec::new)
                .push(token);
        }
        
        {
            let mut by_exchange_symbol = self.by_exchange_symbol.write().await;
            by_exchange_symbol.entry(instrument.exchange_symbol.clone())
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
                    futures.entry(instrument.exchange_symbol.clone())
                        .or_insert_with(Vec::new)
                        .push(token);
                }
            }
            InstrumentType::Option => {
                if let Some(expiry) = instrument.expiry {
                    let mut chains = self.option_chains.write().await;
                    chains.entry((instrument.exchange_symbol.clone(), expiry))
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
        
        by_symbol.get(symbol)
            .map(|tokens| {
                tokens.iter()
                    .filter_map(|t| by_token.get(t).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }
    
    /// Get active futures for underlying
    pub async fn get_active_futures(&self, underlying: &str) -> Vec<Instrument> {
        let futures = self.active_futures.read().await;
        let by_token = self.by_token.read().await;
        
        futures.get(underlying)
            .map(|tokens| {
                let now = Utc::now();
                tokens.iter()
                    .filter_map(|t| by_token.get(t).cloned())
                    .filter(|i| i.expiry.map(|e| e > now).unwrap_or(false))
                    .collect()
            })
            .unwrap_or_default()
    }
    
    /// Get option chain for underlying and expiry
    pub async fn get_option_chain(
        &self, 
        underlying: &str, 
        expiry: DateTime<Utc>
    ) -> Vec<Instrument> {
        let chains = self.option_chains.read().await;
        let by_token = self.by_token.read().await;
        
        chains.get(&(underlying.to_string(), expiry))
            .map(|tokens| {
                tokens.iter()
                    .filter_map(|t| by_token.get(t).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }
    
    /// Get all indices
    pub async fn get_indices(&self) -> Vec<Instrument> {
        let indices = self.indices.read().await;
        let by_token = self.by_token.read().await;
        
        indices.iter()
            .filter_map(|t| by_token.get(t).cloned())
            .collect()
    }
    
    /// Clear all instruments
    pub async fn clear(&self) {
        let mut by_token = self.by_token.write().await;
        let mut by_symbol = self.by_symbol.write().await;
        let mut by_exchange_symbol = self.by_exchange_symbol.write().await;
        let mut futures = self.active_futures.write().await;
        let mut chains = self.option_chains.write().await;
        let mut indices = self.indices.write().await;
        
        by_token.clear();
        by_symbol.clear();
        by_exchange_symbol.clear();
        futures.clear();
        chains.clear();
        indices.clear();
    }
    
    /// Get total instrument count
    pub async fn count(&self) -> usize {
        let by_token = self.by_token.read().await;
        by_token.len()
    }
    
    /// Check if refresh is needed (daily at 8:00 AM IST)
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
            "EQ" => InstrumentType::Equity,
            "INDEX" => InstrumentType::Index,
            "FUT" => InstrumentType::Future,
            "CE" => InstrumentType::Option,
            "PE" => InstrumentType::Option,
            "CUR" => InstrumentType::Currency,
            "COM" => InstrumentType::Commodity,
            _ => InstrumentType::Equity,
        };
        
        let option_type = match z.instrument_type.as_str() {
            "CE" => Some(OptionType::Call),
            "PE" => Some(OptionType::Put),
            _ => None,
        };
        
        let expiry = if !z.expiry.is_empty() && z.expiry != "" {
            chrono::NaiveDate::parse_from_str(&z.expiry, "%Y-%m-%d")
                .ok()
                .map(|d| d.and_hms_opt(15, 30, 0).unwrap())
                .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
        } else {
            None
        };
        
        let strike = if z.strike > 0.0 {
            Some(z.strike)
        } else {
            None
        };
        
        Instrument {
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
            last_price: if z.last_price > 0.0 { Some(z.last_price) } else { None },
            last_update: Utc::now(),
            tradable: true,
            metadata: HashMap::new(),
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
            instrument_token: 256265,
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
            metadata: HashMap::new(),
        };
        
        store.add_instrument(instrument.clone()).await.unwrap();
        
        // Test retrieval
        let retrieved = store.get_by_token(256265).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().trading_symbol, "NIFTY");
        
        // Test symbol lookup
        let by_symbol = store.get_by_symbol("NIFTY").await;
        assert_eq!(by_symbol.len(), 1);
        
        // Test indices
        let indices = store.get_indices().await;
        assert_eq!(indices.len(), 1);
    }
}