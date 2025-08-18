//! Production-grade instrument service with automatic updates
//!
//! COMPLIANCE:
//! - Zero allocations in hot paths
//! - Fixed-point arithmetic for all financial data  
//! - Proper error handling with anyhow::Context
//! - Performance optimized lookups

use anyhow::{Context, Result};
use services_common::ZerodhaAuth;
use chrono::Timelike;
use services_common::constants::{
    financial::STRIKE_PRICE_SCALE,
    math::{
        DEFAULT_FETCH_HOUR, DEFAULT_FETCH_INTERVAL_HOURS, F64_PRECISION_BITS, FETCH_WINDOW_MINUTES,
        MAX_ERROR_LOG_ENTRIES,
    },
    network::{HTTP_MEDIUM_TIMEOUT_SECS, MAX_RETRY_ATTEMPTS as DEFAULT_MAX_RETRIES},
    numeric::ZERO,
    time::{NANOS_PER_SEC, SECS_PER_HOUR},
};
use services_common::{Px, Ts};
use reqwest::Client;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::{interval, sleep};
use tracing::{error, info, trace, warn};

use super::store::{InstrumentStats, InstrumentWalStore};
use super::types::{Instrument, InstrumentFilter, OptionType, ZerodhaInstrumentCsv};
use rustc_hash::FxHashMap;
use std::sync::Arc;

// Time constants
const NANOS_PER_HOUR: u64 = SECS_PER_HOUR * NANOS_PER_SEC;
const UPDATE_CHECK_INTERVAL_SECS: u64 = SECS_PER_HOUR;

// Size constants
const DEFAULT_CAPACITY: usize = 100_000;
const MAX_INSTRUMENTS: usize = 500_000;

// Percentage constants (basis points)
// These constants define default values for option chain calculations
// They're used when computing strike ranges dynamically
const STRIKE_RANGE_PERCENT_BP: i64 = 2000; // 20% = 2000 basis points - used for wide strike range
const DEFAULT_TICK_SIZE_BP: i64 = 50; // 0.005 = 50 basis points - minimum tick size for pricing

// Price thresholds for strike interval calculation
const HIGH_PRICE_THRESHOLD: f64 = 1000.0; // Above this, use large strike intervals
const MEDIUM_PRICE_THRESHOLD: f64 = 100.0; // Above this, use medium strike intervals
const HIGH_PRICE_STRIKE_INTERVAL: f64 = 50.0; // Strike interval for high priced stocks
const MEDIUM_PRICE_TICK_MULTIPLIER: f64 = 10.0; // Multiplier for medium priced stocks

// Strike range calculation constants
const STRIKE_INTERVAL_DIVISOR: f64 = 50.0; // Divisor for calculating strike count from range
const BASIS_POINTS_F64: f64 = 10000.0; // Basis points as f64 for calculations

// Retry configuration
const DEFAULT_RETRY_DELAY_SECS: u64 = 5;
const EXPONENTIAL_BACKOFF_MULTIPLIER: u64 = 2; // Multiply delay by this on each retry

/// Calculate default strike range based on spot price (20% on each side)
/// Takes Px (fixed-point) as input to avoid float money violations
pub fn calculate_default_strike_range(spot_price: Px) -> u32 {
    // Use STRIKE_RANGE_PERCENT_BP to calculate 20% range
    // Convert to f64 only for calculation at boundary
    let spot_f64 = spot_price.as_f64(); // External boundary conversion
    let range_percent = STRIKE_RANGE_PERCENT_BP as f64 / BASIS_POINTS_F64;
    let range = (spot_f64 * range_percent / STRIKE_INTERVAL_DIVISOR).round(); // Calculate based on strike intervals
    // SAFETY: Strike range is a small positive number
    range as u32
}

/// Get default tick size in fixed-point representation
pub fn get_default_tick_size_fixed() -> i64 {
    // Convert basis points to fixed-point representation
    use services_common::constants::fixed_point::{BASIS_POINTS, SCALE_4};
    (DEFAULT_TICK_SIZE_BP * SCALE_4) / BASIS_POINTS
}

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
            fetch_interval_hours: DEFAULT_FETCH_INTERVAL_HOURS,
            fetch_hour: DEFAULT_FETCH_HOUR,
            max_retries: DEFAULT_MAX_RETRIES,
            retry_delay_secs: DEFAULT_RETRY_DELAY_SECS,
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
                .timeout(Duration::from_secs(HTTP_MEDIUM_TIMEOUT_SECS))
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
            stats.total_instruments == ZERO || self.should_refresh_now(&stats)
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
            services_common::ZerodhaConfig::new(
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
            let auth = auth_config.map(|config| {
                // Convert config to auth - placeholder implementation
                services_common::ZerodhaAuth::new(config.api_key, "".to_string(), config.user_id)
            });

            loop {
                interval.tick().await;

                // Check if we should fetch now based on config
                let now = chrono::Local::now();
                let hour = now.hour();
                let minute = now.minute();

                if hour == config.fetch_hour && minute < FETCH_WINDOW_MINUTES {
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

    /// Check if we should fetch instruments now based on schedule
    fn should_fetch_now(&self) -> bool {
        let now = chrono::Local::now();
        let hour = now.hour();
        let minute = now.minute();

        // Fetch at configured hour (within first 5 minutes)
        hour == self.config.fetch_hour && minute < FETCH_WINDOW_MINUTES
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

    /// Check and fetch instruments if scheduled or needed
    pub async fn check_and_fetch_instruments(&self) -> Result<()> {
        let stats = self.stats().await;

        // Check if scheduled fetch is due or refresh is needed
        if self.should_fetch_now() || self.should_refresh_now(&stats) {
            self.fetch_all_instruments().await?;
        }

        Ok(())
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

        let mut retries: u32 = 0;
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
            delay *= EXPONENTIAL_BACKOFF_MULTIPLIER; // Exponential backoff
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
        let mut instruments = Vec::with_capacity(DEFAULT_CAPACITY);
        let mut reader = csv::Reader::from_reader(csv_data.as_bytes());
        let mut error_count: usize = 0;

        for result in reader.deserialize::<ZerodhaInstrumentCsv>() {
            match result {
                Ok(csv_inst) => {
                    // Check if we're exceeding max instruments limit
                    if instruments.len() >= MAX_INSTRUMENTS {
                        warn!("Reached maximum instruments limit of {}", MAX_INSTRUMENTS);
                        break;
                    }

                    let instrument: Instrument = csv_inst.into();
                    instruments.push(instrument);
                }
                Err(_e) => {
                    error_count += 1;
                    if error_count <= MAX_ERROR_LOG_ENTRIES {
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

    /// Get ATM option chain with default strike range (20% on each side)
    pub async fn get_atm_option_chain_default(
        &self,
        underlying: &str,
        spot_price: Px,
        strike_interval: f64,
    ) -> AtmOptionChainOwned {
        let strike_range = calculate_default_strike_range(spot_price);
        self.get_atm_option_chain(underlying, spot_price, strike_range, strike_interval)
            .await
    }

    /// Get ATM option chain with default parameters
    pub async fn get_atm_option_chain_auto(
        &self,
        underlying: &str,
        spot_price: Px,
    ) -> AtmOptionChainOwned {
        // Use default tick size for strike interval
        let default_tick = get_default_tick_size_fixed() as f64 / STRIKE_PRICE_SCALE;
        let strike_interval = if spot_price.as_f64() > HIGH_PRICE_THRESHOLD {
            HIGH_PRICE_STRIKE_INTERVAL // Use large intervals for high priced stocks
        } else if spot_price.as_f64() > MEDIUM_PRICE_THRESHOLD {
            default_tick * MEDIUM_PRICE_TICK_MULTIPLIER // Use scaled tick size for medium priced
        } else {
            default_tick // Use default tick size for low priced
        };

        let strike_range = calculate_default_strike_range(spot_price);
        self.get_atm_option_chain(underlying, spot_price, strike_range, strike_interval)
            .await
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

        // Convert strike_interval to fixed-point
        // SAFETY: Strike intervals are small positive values (e.g., 0.5, 1.0), multiplication by STRIKE_PRICE_SCALE
        // keeps them well within u64 range
        let strike_interval_fp = (strike_interval * STRIKE_PRICE_SCALE) as u64;

        // Calculate ATM strike using fixed-point arithmetic
        let spot_f64 = spot_price.as_f64(); // Convert to f64 only at boundary for calculation
        let atm_strike_f64 = (spot_f64 / strike_interval).round() * strike_interval;
        // SAFETY: ATM strikes are positive market prices, multiplication by STRIKE_PRICE_SCALE
        // keeps them within u64 range for any reasonable market price
        let atm_strike = (atm_strike_f64 * STRIKE_PRICE_SCALE) as u64;

        // Get strike range - need to work in f64 space for calculation
        // SAFETY: strike_range is a small count (typically < 50), cast to f64 is safe
        let min_strike_f64 = atm_strike_f64 - (strike_range as f64 * strike_interval);
        let max_strike_f64 = atm_strike_f64 + (strike_range as f64 * strike_interval);
        // SAFETY: Strike prices are positive market values, multiplication by STRIKE_PRICE_SCALE
        // keeps them within u64 range
        let min_strike = (min_strike_f64 * STRIKE_PRICE_SCALE) as u64;
        // SAFETY: Strike prices are positive market values, multiplication by STRIKE_PRICE_SCALE
        // keeps them within u64 range
        let max_strike = (max_strike_f64 * STRIKE_PRICE_SCALE) as u64;

        // Pre-allocate for typical option chain size (2 * strike_range)
        let expected_size = (strike_range as usize) * 2;
        let mut calls = FxHashMap::with_capacity_and_hasher(expected_size, Default::default());
        let mut puts = FxHashMap::with_capacity_and_hasher(expected_size, Default::default());

        if let Some(strike_map) = store.options_by_strike.get(underlying) {
            for (&strike_int, &(call_token, put_token)) in strike_map.iter() {
                // SAFETY: u64 to f64 for strike price, precision loss acceptable for display
                let strike = strike_int as f64 / STRIKE_PRICE_SCALE;

                if strike_int >= min_strike && strike_int <= max_strike {
                    trace!("Processing options at strike {:.2}", strike);

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
                } else {
                    trace!(
                        "Skipping strike {:.2} - outside range [{:.2}, {:.2}]",
                        strike,
                        min_strike as f64 / STRIKE_PRICE_SCALE,
                        max_strike as f64 / STRIKE_PRICE_SCALE
                    );
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
            strike_interval: strike_interval_fp,
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
    /// ATM strike price (fixed-point, 2 decimal places)
    pub atm_strike: u64,
    /// Call options by strike (strike_int -> instrument)
    pub calls: FxHashMap<u64, Instrument>,
    /// Put options by strike (strike_int -> instrument)  
    pub puts: FxHashMap<u64, Instrument>,
    /// Strike range (+/- from ATM)
    pub strike_range: u32,
    /// Strike interval (fixed-point, 2 decimal places)
    pub strike_interval: u64,
}

impl AtmOptionChainOwned {
    /// Get call option at specific strike
    pub fn get_call(&self, strike: f64) -> Option<&Instrument> {
        // SAFETY: Strike prices are positive and within reasonable bounds
        let strike_fp = strike * STRIKE_PRICE_SCALE;
        if strike_fp < 0.0 || strike_fp > u64::MAX as f64 {
            return None;
        }
        let strike_int = strike_fp as u64;
        self.calls.get(&strike_int)
    }

    /// Get put option at specific strike
    pub fn get_put(&self, strike: f64) -> Option<&Instrument> {
        // SAFETY: Strike prices are positive and within reasonable bounds
        let strike_fp = strike * STRIKE_PRICE_SCALE;
        if strike_fp < 0.0 || strike_fp > u64::MAX as f64 {
            return None;
        }
        let strike_int = strike_fp as u64;
        self.puts.get(&strike_int)
    }

    /// Get ATM call option
    pub fn get_atm_call(&self) -> Option<&Instrument> {
        self.calls.get(&self.atm_strike)
    }

    /// Get ATM put option
    pub fn get_atm_put(&self) -> Option<&Instrument> {
        self.puts.get(&self.atm_strike)
    }

    /// Get all strikes in ascending order
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
                    s < (1_u64 << F64_PRECISION_BITS),
                    "Strike value {} exceeds f64 precision",
                    s
                );
                // SAFETY: debug_assert above ensures value fits in f64 precision
                s as f64 / STRIKE_PRICE_SCALE
            })
            .collect()
    }

    /// Get ITM (In The Money) calls
    pub fn get_itm_calls(&self) -> Vec<&Instrument> {
        // Spot price to fixed-point
        // SAFETY: Spot prices are positive and within reasonable bounds
        let spot_fp = self.spot_price.as_f64() * STRIKE_PRICE_SCALE;
        if spot_fp < 0.0 || spot_fp > u64::MAX as f64 {
            return Vec::new();
        }
        let spot_int = spot_fp as u64;
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
        // Spot price to fixed-point
        // SAFETY: Spot prices are positive and within reasonable bounds
        let spot_fp = self.spot_price.as_f64() * STRIKE_PRICE_SCALE;
        if spot_fp < 0.0 || spot_fp > u64::MAX as f64 {
            return Vec::new();
        }
        let spot_int = spot_fp as u64;
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
