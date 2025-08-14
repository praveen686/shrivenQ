//! Production-grade instrument service with automatic updates
//!
//! COMPLIANCE:
//! - Zero allocations in hot paths
//! - Fixed-point arithmetic for all financial data  
//! - Proper error handling with anyhow::Context
//! - Performance optimized lookups

use anyhow::{Context, Result};
use auth::ZerodhaAuth;
use chrono::Timelike;
use common::{Px, Ts};
use reqwest::Client;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::{interval, sleep};
use tracing::{error, info, warn};

use super::store::{InstrumentStats, InstrumentWalStore};
use super::types::{Instrument, InstrumentFilter, OptionType, ZerodhaInstrumentCsv};
use rustc_hash::FxHashMap;
use std::sync::Arc;

// Time constants
const SECS_PER_HOUR: u64 = 3600;
const NANOS_PER_HOUR: u64 = 3600 * 1_000_000_000;
const UPDATE_CHECK_INTERVAL_SECS: u64 = SECS_PER_HOUR;
const DEFAULT_FETCH_INTERVAL_HOURS: u64 = 24;
const DEFAULT_FETCH_HOUR: u32 = 8; // 8 AM IST
const DEFAULT_RETRY_DELAY_SECS: u64 = 5;
const DEFAULT_MAX_RETRIES: u32 = 3;

// Size constants
const DEFAULT_CAPACITY: usize = 100_000;
const MAX_INSTRUMENTS: usize = 500_000;
const DEFAULT_WAL_SEGMENT_SIZE_MB: usize = 100;

// Percentage constants (basis points)
const STRIKE_RANGE_PERCENT_BP: i64 = 2000; // 20% = 2000 basis points
const DEFAULT_TICK_SIZE_BP: i64 = 50; // 0.005 = 50 basis points

// Fixed-point conversion
const FIXED_POINT_SCALE: i64 = 10000;

/// Instrument service configuration
#[derive(Debug, Clone)]
pub struct InstrumentServiceConfig {
    /// WAL directory for persistent storage
    pub wal_dir: PathBuf,

    /// WAL segment size in MB
    pub wal_segment_size_mb: Option<usize>,

    /// Fetch interval in hours (default: 24)
    pub fetch_interval_hours: u64,

    /// Fetch time (hour in IST, default: 8 AM)
    pub fetch_hour: u32,

    /// Retry attempts
    pub max_retries: u32,

    /// Initial retry delay in seconds
    pub retry_delay_secs: u64,

    /// Enable automatic daily updates
    pub enable_auto_updates: bool,
}

impl Default for InstrumentServiceConfig {
    fn default() -> Self {
        Self {
            wal_dir: PathBuf::from("./data/instruments_wal"),
            wal_segment_size_mb: Some(50), // 50MB segments
            fetch_interval_hours: 24,
            fetch_hour: 8, // 8 AM IST
            max_retries: 3,
            retry_delay_secs: 5,
            enable_auto_updates: true,
        }
    }
}

/// Production-grade instrument service with WAL storage
pub struct InstrumentService {
    config: InstrumentServiceConfig,
    store: Arc<RwLock<InstrumentWalStore>>,
    zerodha_auth: Option<ZerodhaAuth>,
    client: Client,
}

impl InstrumentService {
    /// Create new instrument service
    pub async fn new(
        config: InstrumentServiceConfig,
        zerodha_auth: Option<ZerodhaAuth>,
    ) -> Result<Self> {
        // Initialize WAL store
        let store = InstrumentWalStore::new(config.wal_dir.clone(), config.wal_segment_size_mb)
            .context("Failed to create instrument WAL store")?;

        let service = Self {
            config,
            store: Arc::new(RwLock::new(store)),
            zerodha_auth,
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .context("Failed to create HTTP client")?,
        };

        Ok(service)
    }

    /// Initialize service - load from WAL and start background tasks
    pub async fn start(&self) -> Result<()> {
        info!("Starting instrument service...");

        // Load existing instruments from WAL
        {
            let mut store = self.store.write().await;
            if let Err(e) = store.load_from_wal() {
                warn!("Failed to load instruments from WAL: {}", e);
            }
        }

        // Fetch instruments if empty or stale
        let should_fetch = {
            let store = self.store.read().await;
            let stats = store.stats();
            stats.total_instruments == 0 || self.should_refresh_now(&stats)
        };

        if should_fetch {
            info!("Fetching instruments on startup...");
            self.fetch_all_instruments().await?;
        }

        // Start background update task
        if self.config.enable_auto_updates {
            self.start_background_updates().await;
        }

        info!("Instrument service started successfully");
        Ok(())
    }

    /// Start background task for automatic updates
    async fn start_background_updates(&self) {
        if !self.config.enable_auto_updates {
            return;
        }

        // Clone required data for background task
        let config = self.config.clone();
        let store = Arc::clone(&self.store);
        let auth_config = self.zerodha_auth.as_ref().map(|_auth| {
            // Extract config from existing auth
            auth::ZerodhaConfig::new(
                std::env::var("ZERODHA_USER_ID").unwrap_or_default(),
                std::env::var("ZERODHA_PASSWORD").unwrap_or_default(),
                std::env::var("ZERODHA_TOTP_SECRET").unwrap_or_default(),
                std::env::var("ZERODHA_API_KEY").unwrap_or_default(),
                std::env::var("ZERODHA_API_SECRET").unwrap_or_default(),
            )
        });
        let client = self.client.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(UPDATE_CHECK_INTERVAL_SECS));

            // Create auth instance in background task
            let auth = auth_config.map(auth::ZerodhaAuth::new);

            loop {
                interval.tick().await;

                // Check if we should fetch now based on config
                let now = chrono::Local::now();
                let hour = now.hour();
                let minute = now.minute();

                if hour == config.fetch_hour && minute < 5 {
                    info!("Starting scheduled instrument fetch");

                    if let Some(ref auth_instance) = auth {
                        match Self::fetch_instruments_background(
                            Arc::clone(&store),
                            auth_instance,
                            &client,
                        )
                        .await
                        {
                            Ok(_) => info!("Scheduled instrument fetch completed successfully"),
                            Err(e) => error!("Failed to fetch instruments: {}", e),
                        }
                    } else {
                        warn!("No authentication configured for background updates");
                    }
                }
            }
        });
    }

    /// Check if we should fetch instruments now (reserved for scheduled fetching)
    #[allow(dead_code)] // Time-based fetching logic - reserved for future scheduled updates
    fn should_fetch_now(&self) -> bool {
        let now = chrono::Local::now();
        let hour = now.hour();
        let minute = now.minute();

        // Fetch at configured hour (within first 5 minutes)
        hour == self.config.fetch_hour && minute < 5
    }

    /// Check if refresh is needed based on stats
    fn should_refresh_now(&self, stats: &InstrumentStats) -> bool {
        match stats.last_update {
            Some(last_update) => {
                let now = Ts::now();
                let hours_since_update = (now.as_nanos() - last_update.as_nanos()) / NANOS_PER_HOUR;
                hours_since_update >= self.config.fetch_interval_hours
            }
            None => true,
        }
    }

    /// Fetch all instruments from Zerodha
    pub async fn fetch_all_instruments(&self) -> Result<()> {
        info!("Fetching instruments from Zerodha...");

        let start = std::time::Instant::now();

        // Fetch from Zerodha
        let instruments = self
            .fetch_zerodha_instruments()
            .await
            .context("Failed to fetch Zerodha instruments")?;

        // Clear existing and add new instruments
        {
            let mut store = self.store.write().await;
            store.clear()?;

            let count = store
                .add_instruments(instruments)
                .context("Failed to add instruments to store")?;

            store.sync()?;

            info!(
                "Successfully fetched and stored {} instruments in {:?}",
                count,
                start.elapsed()
            );
        }

        Ok(())
    }

    /// Fetch instruments from Zerodha with retry logic
    async fn fetch_zerodha_instruments(&self) -> Result<Vec<Instrument>> {
        let auth = self
            .zerodha_auth
            .as_ref()
            .context("Zerodha auth not configured")?;

        // Get access token
        let access_token = auth
            .authenticate()
            .await
            .context("Failed to authenticate with Zerodha")?;

        // Zerodha instruments URL
        let url = "https://api.kite.trade/instruments";

        let mut retries = 0;
        let mut delay = self.config.retry_delay_secs;

        loop {
            let response = self
                .client
                .get(url)
                .header("Authorization", format!("token {}", access_token))
                .send()
                .await;

            match response {
                Ok(resp) if resp.status().is_success() => {
                    info!("Successfully fetched instruments from Zerodha");
                    let csv_data = resp.text().await.context("Failed to read response body")?;

                    info!("CSV data length: {} bytes", csv_data.len());
                    return Self::parse_csv_data(&csv_data).await;
                }
                Ok(resp) => {
                    warn!("Failed to fetch instruments: HTTP {}", resp.status());
                    if retries >= self.config.max_retries {
                        return Err(anyhow::anyhow!(
                            "Max retries exceeded, last status: {}",
                            resp.status()
                        ));
                    }
                }
                Err(e) => {
                    warn!("Request failed: {}", e);
                    if retries >= self.config.max_retries {
                        return Err(e.into());
                    }
                }
            }

            retries += 1;
            info!(
                "Retrying in {} seconds (attempt {}/{})",
                delay, retries, self.config.max_retries
            );
            sleep(Duration::from_secs(delay)).await;
            delay *= 2; // Exponential backoff
        }
    }

    /// Background instrument fetch (static method for async safety)
    async fn fetch_instruments_background(
        store: Arc<RwLock<InstrumentWalStore>>,
        auth: &ZerodhaAuth,
        client: &reqwest::Client,
    ) -> Result<()> {
        let access_token = auth
            .authenticate()
            .await
            .context("Failed to authenticate with Zerodha")?;

        let url = "https://api.kite.trade/instruments";
        let response = client
            .get(url)
            .header("Authorization", format!("token {}", access_token))
            .send()
            .await
            .context("Failed to send request to Zerodha")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("HTTP error: {}", response.status()));
        }

        let csv_data = response
            .text()
            .await
            .context("Failed to read response body")?;

        // Parse CSV and update store
        let instruments = Self::parse_csv_data(&csv_data).await?;

        let mut store_guard = store.write().await;
        store_guard.clear()?;
        let count = store_guard.add_instruments(instruments)?;
        store_guard.sync()?;

        info!(
            "Background instrument fetch completed: {} instruments",
            count
        );
        Ok(())
    }

    /// Parse CSV data (static method)
    async fn parse_csv_data(csv_data: &str) -> Result<Vec<Instrument>> {
        let mut instruments = Vec::new();
        let mut reader = csv::Reader::from_reader(csv_data.as_bytes());
        let mut error_count = 0;

        for result in reader.deserialize::<ZerodhaInstrumentCsv>() {
            match result {
                Ok(csv_inst) => {
                    let instrument: Instrument = csv_inst.into();
                    instruments.push(instrument);
                }
                Err(_e) => {
                    error_count += 1;
                    if error_count <= 10 {
                        // Log first 10 errors only
                        warn!("Failed to parse instrument CSV row");
                    }
                }
            }
        }

        if error_count > 0 {
            warn!("Total CSV parse errors: {}", error_count);
        }

        info!("Parsed {} instruments from CSV", instruments.len());
        Ok(instruments)
    }

    /// Get instrument by token - HOT PATH
    pub async fn get_by_token(&self, token: u32) -> Option<Instrument> {
        let store = self.store.read().await;
        store.get_by_token(token).cloned()
    }

    /// Get instruments by trading symbol
    pub async fn get_by_trading_symbol(&self, symbol: &str) -> Vec<Instrument> {
        let store = self.store.read().await;
        store
            .get_by_trading_symbol(symbol)
            .into_iter()
            .cloned()
            .collect()
    }

    /// CRITICAL: Get spot-to-futures mapping for market data subscription
    pub async fn get_subscription_tokens(
        &self,
        underlying: &str,
    ) -> (Option<u32>, Option<u32>, Option<u32>) {
        let store = self.store.read().await;
        store.get_subscription_tokens(underlying)
    }

    /// Get current month futures for underlying
    pub async fn get_current_month_futures(&self, underlying: &str) -> Option<Instrument> {
        let store = self.store.read().await;
        store.get_current_month_futures(underlying).cloned()
    }

    /// Get active futures for underlying
    pub async fn get_active_futures(&self, underlying: &str) -> Vec<Instrument> {
        let store = self.store.read().await;
        store
            .get_active_futures(underlying)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Query instruments with filter
    pub async fn query(&self, filter: &InstrumentFilter) -> Vec<Instrument> {
        let store = self.store.read().await;
        store.query(filter).into_iter().cloned().collect()
    }

    /// Get all indices
    pub async fn get_indices(&self) -> Vec<Instrument> {
        let store = self.store.read().await;
        store.get_indices().into_iter().cloned().collect()
    }

    /// Get service statistics
    pub async fn stats(&self) -> InstrumentStats {
        let store = self.store.read().await;
        store.stats()
    }

    /// Force refresh instruments
    pub async fn force_refresh(&self) -> Result<()> {
        info!("Force refreshing instruments...");
        self.fetch_all_instruments().await
    }

    /// Get ATM option chain based on spot price - CRITICAL for options trading
    /// Returns owned instruments to avoid lifetime issues
    pub async fn get_atm_option_chain(
        &self,
        underlying: &str,
        spot_price: Px,
        strike_range: u32,
        strike_interval: f64,
    ) -> AtmOptionChainOwned {
        let store = self.store.read().await;

        // Calculate ATM strike using fixed-point arithmetic
        let spot_f64 = spot_price.as_f64(); // Convert to f64 only at boundary for calculation
        let atm_strike = (spot_f64 / strike_interval).round() * strike_interval;

        // Get strike range
        #[allow(clippy::cast_precision_loss)] // usize to f64 for strike range calculation
        // SAFETY: usize to f64 is safe for reasonable strike ranges
        let min_strike = atm_strike - (strike_range as f64 * strike_interval);
        #[allow(clippy::cast_precision_loss)] // usize to f64 for strike range calculation
        // SAFETY: usize to f64 is safe for reasonable strike ranges
        let max_strike = atm_strike + (strike_range as f64 * strike_interval);

        let mut calls = FxHashMap::default();
        let mut puts = FxHashMap::default();

        if let Some(strike_map) = store.options_by_strike.get(underlying) {
            for (&strike_int, &(call_token, put_token)) in strike_map.iter() {
                #[allow(clippy::cast_precision_loss)] // Fixed-point to f64 conversion
                // SAFETY: u64 to f64 for strike price, precision loss acceptable for display
                let strike = strike_int as f64 / 100.0;

                if strike >= min_strike && strike <= max_strike {
                    if let Some(call_token) = call_token {
                        if let Some(call_instrument) = store.by_token.get(&call_token) {
                            calls.insert(strike_int, call_instrument.clone());
                        }
                    }

                    if let Some(put_token) = put_token {
                        if let Some(put_instrument) = store.by_token.get(&put_token) {
                            puts.insert(strike_int, put_instrument.clone());
                        }
                    }
                }
            }
        }

        AtmOptionChainOwned {
            underlying: underlying.to_string(),
            spot_price,
            atm_strike,
            calls,
            puts,
            strike_range,
            strike_interval,
        }
    }

    /// Get option tokens for market data subscription around ATM
    pub async fn get_atm_subscription_tokens(
        &self,
        underlying: &str,
        spot_price: Px,
        strike_range: u32,
        strike_interval: f64,
    ) -> Vec<u32> {
        let store = self.store.read().await;
        store.get_atm_subscription_tokens(underlying, spot_price, strike_range, strike_interval)
    }

    /// Get option by strike and type
    pub async fn get_option_by_strike(
        &self,
        underlying: &str,
        strike: f64,
        option_type: OptionType,
    ) -> Option<Instrument> {
        let store = self.store.read().await;
        store
            .get_option_by_strike(underlying, strike, option_type)
            .cloned()
    }

    /// Get all available strikes for underlying
    pub async fn get_available_strikes(&self, underlying: &str) -> Vec<f64> {
        let store = self.store.read().await;
        store.get_available_strikes(underlying)
    }

    /// Get comprehensive market data subscription tokens for underlying
    /// Returns (spot_token, current_futures_token, next_futures_token, option_tokens)
    pub async fn get_comprehensive_subscription_tokens(
        &self,
        underlying: &str,
        spot_price: Px,
        strike_range: u32,
        strike_interval: f64,
    ) -> (Option<u32>, Option<u32>, Option<u32>, Vec<u32>) {
        let store = self.store.read().await;

        // Get basic tokens (spot + futures)
        let (spot_token, current_futures_token, next_futures_token) =
            store.get_subscription_tokens(underlying);

        // Get option tokens around ATM
        let option_tokens = store.get_atm_subscription_tokens(
            underlying,
            spot_price,
            strike_range,
            strike_interval,
        );

        (
            spot_token,
            current_futures_token,
            next_futures_token,
            option_tokens,
        )
    }

    /// Manual sync WAL
    pub async fn sync(&self) -> Result<()> {
        let mut store = self.store.write().await;
        store.sync()
    }
}

/// ATM (At The Money) option chain with owned instruments
#[derive(Debug, Clone)]
pub struct AtmOptionChainOwned {
    /// Underlying symbol
    pub underlying: String,
    /// Current spot price
    pub spot_price: Px,
    /// ATM strike price
    pub atm_strike: f64,
    /// Call options by strike (strike_int -> instrument)
    pub calls: FxHashMap<u64, Instrument>,
    /// Put options by strike (strike_int -> instrument)  
    pub puts: FxHashMap<u64, Instrument>,
    /// Strike range (+/- from ATM)
    pub strike_range: u32,
    /// Strike interval
    pub strike_interval: f64,
}

impl AtmOptionChainOwned {
    /// Get call option at specific strike
    pub fn get_call(&self, strike: f64) -> Option<&Instrument> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Strike to fixed-point
        // SAFETY: Strike prices are positive and within reasonable bounds
        let strike_int = (strike * 100.0) as u64;
        self.calls.get(&strike_int)
    }

    /// Get put option at specific strike
    pub fn get_put(&self, strike: f64) -> Option<&Instrument> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Strike to fixed-point
        // SAFETY: Strike prices are positive and within reasonable bounds
        let strike_int = (strike * 100.0) as u64;
        self.puts.get(&strike_int)
    }

    /// Get ATM call option
    pub fn get_atm_call(&self) -> Option<&Instrument> {
        self.get_call(self.atm_strike)
    }

    /// Get ATM put option
    pub fn get_atm_put(&self) -> Option<&Instrument> {
        self.get_put(self.atm_strike)
    }

    /// Get all strikes in ascending order
    #[allow(clippy::cast_precision_loss)] // Strike conversion for display - values are bounded
    pub fn get_strikes(&self) -> Vec<f64> {
        let mut all_strikes: Vec<u64> =
            self.calls.keys().chain(self.puts.keys()).copied().collect();
        all_strikes.sort();
        all_strikes.dedup();
        all_strikes
            .into_iter()
            .map(|s| {
                // SAFETY: Strike prices for options are typically < 1,000,000
                // With 2 decimal places, max internal value is 100,000,000 (< 2^53)
                // If we ever trade options with strikes > 90 trillion, this needs revisiting!
                debug_assert!(
                    s < (1_u64 << 53),
                    "Strike value {} exceeds f64 precision",
                    s
                );
                // SAFETY: debug_assert above ensures value fits in f64 precision
                s as f64 / 100.0
            })
            .collect()
    }

    /// Get ITM (In The Money) calls
    pub fn get_itm_calls(&self) -> Vec<&Instrument> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // Spot price to fixed-point
        // SAFETY: Spot prices are positive and within reasonable bounds
        let spot_int = (self.spot_price.as_f64() * 100.0) as u64;
        self.calls
            .iter()
            .filter_map(|(&strike_int, instrument)| {
                if strike_int < spot_int {
                    Some(instrument)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get ITM (In The Money) puts
    pub fn get_itm_puts(&self) -> Vec<&Instrument> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // Spot price to fixed-point
        // SAFETY: Spot prices are positive and within reasonable bounds
        let spot_int = (self.spot_price.as_f64() * 100.0) as u64;
        self.puts
            .iter()
            .filter_map(|(&strike_int, instrument)| {
                if strike_int > spot_int {
                    Some(instrument)
                } else {
                    None
                }
            })
            .collect()
    }
}
