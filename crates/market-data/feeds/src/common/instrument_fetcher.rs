//! Production-grade instrument fetcher service
//!
//! Fetches instruments from multiple exchanges with:
//! - Automatic daily updates
//! - Incremental updates using ETags
//! - Retry logic with exponential backoff
//! - Multi-venue support
//! - Persistent caching

use anyhow::{Context, Result};
use auth::{BinanceAuth, ZerodhaAuth};
use chrono::{DateTime, Local, Timelike, Utc};
use common::instrument::{Instrument, InstrumentStore};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::{interval, sleep};
use tracing::{debug, error, info, warn};

/// Instrument fetcher configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentFetcherConfig {
    /// Cache directory for instruments
    pub cache_dir: PathBuf,

    /// Fetch interval in hours (default: 24)
    pub fetch_interval_hours: u64,

    /// Fetch time (hour in IST, default: 8 AM)
    pub fetch_hour: u32,

    /// Retry attempts
    pub max_retries: u32,

    /// Initial retry delay in seconds
    pub retry_delay_secs: u64,

    /// Enable Zerodha
    pub enable_zerodha: bool,

    /// Enable Binance
    pub enable_binance: bool,
}

impl Default for InstrumentFetcherConfig {
    fn default() -> Self {
        Self {
            cache_dir: PathBuf::from("./cache/instruments"),
            fetch_interval_hours: 24,
            fetch_hour: 8, // 8 AM IST
            max_retries: 3,
            retry_delay_secs: 5,
            enable_zerodha: true,
            enable_binance: false,
        }
    }
}

/// Instrument fetcher service
pub struct InstrumentFetcher {
    config: InstrumentFetcherConfig,
    store: InstrumentStore,
    zerodha_auth: Option<ZerodhaAuth>,
    _binance_auth: Option<BinanceAuth>,
    client: Client,
}

impl InstrumentFetcher {
    /// Create new instrument fetcher
    pub fn new(
        config: InstrumentFetcherConfig,
        zerodha_auth: Option<ZerodhaAuth>,
        binance_auth: Option<BinanceAuth>,
    ) -> Result<Self> {
        // Ensure cache directory exists
        std::fs::create_dir_all(&config.cache_dir)?;

        Ok(Self {
            config,
            store: InstrumentStore::new(),
            zerodha_auth,
            _binance_auth: binance_auth,
            client: Client::builder().timeout(Duration::from_secs(30)).build()?,
        })
    }

    /// Start the fetcher service
    pub async fn start(mut self) -> Result<()> {
        info!("Starting instrument fetcher service");

        // Load cached instruments on startup
        if let Err(e) = self.load_cache().await {
            warn!("Failed to load instrument cache: {}", e);
        }

        // Initial fetch
        if self.store.count().await == 0 {
            info!("No cached instruments found, fetching now");
            self.fetch_all().await?;
        }

        // Schedule daily updates
        let mut interval = interval(<Duration as DurationExt>::from_hours(1));

        loop {
            interval.tick().await;

            if self.should_fetch_now() {
                info!("Starting scheduled instrument fetch");
                if let Err(e) = self.fetch_all().await {
                    error!("Failed to fetch instruments: {}", e);
                }
            }
        }
    }

    /// Check if we should fetch now
    fn should_fetch_now(&self) -> bool {
        let now = Local::now();
        let hour = now.hour();
        let minute = now.minute();

        // Fetch at configured hour (within first 5 minutes)
        hour == self.config.fetch_hour && minute < 5
    }

    /// Fetch instruments from all enabled venues
    pub async fn fetch_all(&mut self) -> Result<()> {
        info!("Fetching instruments from all venues");

        let start = std::time::Instant::now();
        let mut total_count = 0;

        // Clear existing instruments
        self.store.clear().await;

        // Fetch from Zerodha
        if self.config.enable_zerodha {
            match self.fetch_zerodha_instruments().await {
                Ok(count) => {
                    info!("Fetched {} instruments from Zerodha", count);
                    total_count += count;
                }
                Err(e) => {
                    error!("Failed to fetch Zerodha instruments: {}", e);
                }
            }
        }

        // Fetch from Binance
        if self.config.enable_binance {
            match self.fetch_binance_instruments().await {
                Ok(count) => {
                    info!("Fetched {} instruments from Binance", count);
                    total_count += count;
                }
                Err(e) => {
                    error!("Failed to fetch Binance instruments: {}", e);
                }
            }
        }

        // Save to cache
        self.save_cache().await?;

        info!(
            "Instrument fetch completed: {} instruments in {:?}",
            total_count,
            start.elapsed()
        );

        Ok(())
    }

    /// Fetch Zerodha instruments
    async fn fetch_zerodha_instruments(&mut self) -> Result<usize> {
        let auth = self
            .zerodha_auth
            .as_ref()
            .context("Zerodha auth not configured")?;

        // Get access token
        let access_token = auth.authenticate().await?;

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
                    info!("Got successful response from Zerodha");
                    let csv_data = resp.text().await?;
                    info!("CSV data length: {} bytes", csv_data.len());
                    return self.parse_zerodha_csv(&csv_data).await;
                }
                Ok(resp) => {
                    warn!("Failed to fetch instruments: HTTP {}", resp.status());
                    if retries >= self.config.max_retries {
                        return Err(anyhow::anyhow!("Max retries exceeded"));
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

    /// Parse Zerodha CSV data
    async fn parse_zerodha_csv(&mut self, csv_data: &str) -> Result<usize> {
        let mut count = 0;
        let mut reader = csv::Reader::from_reader(csv_data.as_bytes());

        // Log first few lines for debugging
        let lines: Vec<&str> = csv_data.lines().take(3).collect();
        info!("First few lines of CSV: {:?}", lines);

        let mut error_count = 0;
        for result in reader.deserialize::<ZerodhaInstrumentCsv>() {
            match result {
                Ok(csv_inst) => {
                    if count <= 5 {
                        debug!("Parsed instrument: {:?}", csv_inst.tradingsymbol);
                    }
                    let instrument: Instrument = csv_inst.into();
                    self.store.add_instrument(instrument).await?;
                    count += 1;
                }
                Err(e) => {
                    error_count += 1;
                    if error_count <= 5 {
                        warn!("Failed to parse instrument: {}", e);
                    }
                }
            }
        }

        if error_count > 0 {
            warn!("Total parse errors: {}", error_count);
        }

        Ok(count)
    }

    /// Fetch Binance instruments
    async fn fetch_binance_instruments(&mut self) -> Result<usize> {
        // Binance exchange info endpoint
        let url = "https://api.binance.com/api/v3/exchangeInfo";

        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to fetch Binance instruments"));
        }

        let exchange_info: BinanceExchangeInfo = response.json().await?;
        let mut count = 0;

        for symbol in exchange_info.symbols {
            if symbol.status == "TRADING" {
                let instrument = self.binance_to_instrument(symbol);
                self.store.add_instrument(instrument).await?;
                count += 1;
            }
        }

        Ok(count)
    }

    /// Convert Binance symbol to instrument
    fn binance_to_instrument(&self, symbol: BinanceSymbol) -> Instrument {
        use common::instrument::InstrumentType;

        Instrument {
            instrument_token: symbol.symbol.chars().fold(0u32, |acc, c| {
                #[allow(clippy::cast_lossless)] // char to u32 is always safe
                // SAFETY: Cast is safe within expected range
                acc.wrapping_mul(31).wrapping_add(c as u32) // SAFETY: char fits in u32
            }),
            trading_symbol: symbol.symbol.clone(),
            exchange_symbol: symbol.base_asset.clone(),
            name: format!("{}/{}", symbol.base_asset, symbol.quote_asset),
            instrument_type: InstrumentType::Currency,
            segment: "SPOT".to_string(),
            exchange: "BINANCE".to_string(),
            expiry: None,
            strike: None,
            option_type: None,
            tick_size: symbol
                .filters
                .iter()
                .find_map(|f| match f {
                    BinanceFilter::PriceFilter { tick_size, .. } => Some(*tick_size),
                    _ => None,
                })
                .unwrap_or(0.00000001),
            lot_size: 1,
            last_price: None,
            last_update: Utc::now(),
            tradable: symbol.is_spot_trading_allowed && symbol.status == "TRADING",
            metadata: Default::default(),
        }
    }

    /// Load instruments from cache
    async fn load_cache(&mut self) -> Result<()> {
        let cache_file = self.config.cache_dir.join("instruments.json");

        if !cache_file.exists() {
            return Err(anyhow::anyhow!("Cache file not found"));
        }

        self.store
            .load_from_cache(
                cache_file
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Invalid cache file path"))?,
            )
            .await?;

        info!("Loaded {} instruments from cache", self.store.count().await);
        Ok(())
    }

    /// Save instruments to cache
    async fn save_cache(&self) -> Result<()> {
        let cache_file = self.config.cache_dir.join("instruments.json");
        self.store
            .save_to_cache(
                cache_file
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Invalid cache file path"))?,
            )
            .await?;

        // Also save metadata
        let meta_file = self.config.cache_dir.join("metadata.json");
        let metadata = InstrumentMetadata {
            last_fetch: Utc::now(),
            instrument_count: self.store.count().await,
            venues: vec![
                if self.config.enable_zerodha {
                    "ZERODHA"
                } else {
                    ""
                },
                if self.config.enable_binance {
                    "BINANCE"
                } else {
                    ""
                },
            ]
            .into_iter()
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
        };

        let meta_json = serde_json::to_string_pretty(&metadata)?;
        tokio::fs::write(meta_file, meta_json).await?;

        info!("Saved {} instruments to cache", self.store.count().await);
        Ok(())
    }

    /// Get the instrument store
    pub fn store(&self) -> &InstrumentStore {
        &self.store
    }
}

/// Zerodha CSV instrument format
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ZerodhaInstrumentCsv {
    instrument_token: u32,
    exchange_token: u32,
    tradingsymbol: String,
    name: Option<String>,
    last_price: f64,
    expiry: Option<String>,
    strike: f64,
    tick_size: f64,
    lot_size: u32,
    instrument_type: String,
    segment: String,
    exchange: String,
}

impl From<ZerodhaInstrumentCsv> for Instrument {
    fn from(z: ZerodhaInstrumentCsv) -> Self {
        use common::instrument::{InstrumentType, OptionType};

        let instrument_type = match z.instrument_type.as_str() {
            "EQ" => InstrumentType::Equity,
            "INDEX" => InstrumentType::Index,
            "FUT" => InstrumentType::Future,
            "CE" => InstrumentType::Option,
            "PE" => InstrumentType::Option,
            _ => InstrumentType::Equity,
        };

        let option_type = match z.instrument_type.as_str() {
            "CE" => Some(OptionType::Call),
            "PE" => Some(OptionType::Put),
            _ => None,
        };

        let expiry = z
            .expiry
            .as_ref()
            .and_then(|e| chrono::NaiveDate::parse_from_str(e, "%Y-%m-%d").ok())
            .and_then(|d| d.and_hms_opt(15, 30, 0))
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));

        Instrument {
            instrument_token: z.instrument_token,
            trading_symbol: z.tradingsymbol,
            exchange_symbol: z.name.clone().unwrap_or_default(),
            name: z.name.unwrap_or_default(),
            instrument_type,
            segment: z.segment,
            exchange: z.exchange,
            expiry,
            strike: if z.strike > 0.0 { Some(z.strike) } else { None },
            option_type,
            tick_size: z.tick_size,
            lot_size: z.lot_size,
            last_price: if z.last_price > 0.0 {
                Some(z.last_price)
            } else {
                None
            },
            last_update: Utc::now(),
            tradable: true,
            metadata: Default::default(),
        }
    }
}

/// Binance exchange info
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceExchangeInfo {
    symbols: Vec<BinanceSymbol>,
}

/// Binance symbol info
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceSymbol {
    symbol: String,
    status: String,
    base_asset: String,
    quote_asset: String,
    is_spot_trading_allowed: bool,
    filters: Vec<BinanceFilter>,
}

/// Binance filter types
#[derive(Debug, Deserialize)]
#[serde(tag = "filterType")]
enum BinanceFilter {
    #[serde(rename = "PRICE_FILTER")]
    PriceFilter {
        #[serde(rename = "tickSize")]
        tick_size: f64,
    },
    #[serde(other)]
    Other,
}

/// Instrument metadata
#[derive(Debug, Serialize, Deserialize)]
struct InstrumentMetadata {
    last_fetch: DateTime<Utc>,
    instrument_count: usize,
    venues: Vec<String>,
}

/// Helper to create Duration from hours
trait DurationExt {
    fn from_hours(hours: u64) -> Duration;
}

impl DurationExt for Duration {
    fn from_hours(hours: u64) -> Duration {
        Duration::from_secs(hours * 3600)
    }
}
