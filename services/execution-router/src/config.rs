//! Execution router configuration

use services_common::constants::{
    fixed_point::{SCALE_2, SCALE_3},
    network::{INITIAL_RETRY_DELAY_MS, MAX_RETRY_ATTEMPTS, MAX_RETRY_DELAY_MS},
    time::SECS_PER_MINUTE,
    trading::MIN_ORDER_QTY,
};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// Execution router configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// Default venue
    pub default_venue: String,

    /// Venue configurations
    pub venues: FxHashMap<String, VenueConfig>,

    /// Algorithm settings
    pub algorithm_settings: AlgorithmSettings,

    /// Risk checks
    pub risk_checks: RiskCheckConfig,

    /// Retry configuration
    pub retry_config: RetryConfig,
    
    /// Order cache size
    pub order_cache_size: usize,
    
    /// Venue timeout in milliseconds
    pub venue_timeout_ms: u64,
}

/// Venue configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VenueConfig {
    /// Venue name
    pub name: String,

    /// API endpoint
    pub api_url: String,

    /// WebSocket endpoint
    pub ws_url: Option<String>,

    /// API credentials
    pub api_key: String,
    pub api_secret: String,

    /// Rate limits
    pub max_orders_per_second: u32,
    pub max_cancels_per_second: u32,

    /// Supported symbols
    pub symbols: Vec<String>,

    /// Fee structure
    pub maker_fee_bps: i32,
    pub taker_fee_bps: i32,
}

/// Algorithm settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgorithmSettings {
    /// Default slice duration (seconds)
    pub default_slice_duration: u64,

    /// Maximum participation rate (fixed-point: `SCALE_3` = 100%)
    pub max_participation_rate: i32,

    /// Minimum order size (fixed-point)
    pub min_order_size: i64,

    /// Maximum order size (fixed-point)
    pub max_order_size: i64,

    /// VWAP lookback period (minutes)
    pub vwap_lookback_minutes: u32,

    /// Iceberg display percentage (fixed-point: `SCALE_3` = 100%)
    pub iceberg_display_pct: i32,
}

/// Risk check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskCheckConfig {
    /// Enable pre-trade risk checks
    pub enable_pretrade_checks: bool,

    /// Maximum order value (fixed-point)
    pub max_order_value: i64,

    /// Maximum position value (fixed-point)
    pub max_position_value: i64,

    /// Price tolerance percentage (fixed-point: `SCALE_2` = 100%)
    pub price_tolerance_pct: i32,

    /// Reject orders outside market hours
    pub check_market_hours: bool,
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum retry attempts
    pub max_retries: u32,

    /// Initial retry delay (milliseconds)
    pub initial_delay_ms: u64,

    /// Maximum retry delay (milliseconds)
    pub max_delay_ms: u64,

    /// Exponential backoff multiplier
    pub backoff_multiplier: u32,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        const DEFAULT_ORDER_CACHE_SIZE: usize = 10000;
        const DEFAULT_VENUE_TIMEOUT_MS: u64 = 5000;
        
        Self {
            default_venue: "binance".to_string(),
            venues: FxHashMap::default(),
            algorithm_settings: AlgorithmSettings {
                default_slice_duration: SECS_PER_MINUTE,
                // SAFETY: SCALE_3 / 10 fits in i32
                max_participation_rate: (SCALE_3 / 10) as i32, // 10%
                min_order_size: MIN_ORDER_QTY,                 // 1 unit
                max_order_size: MIN_ORDER_QTY * 1000,          // 1000 units
                vwap_lookback_minutes: 30,
                // SAFETY: SCALE_3 / 5 fits in i32
                iceberg_display_pct: (SCALE_3 / 5) as i32, // 20%
            },
            risk_checks: RiskCheckConfig {
                enable_pretrade_checks: true,
                max_order_value: 1000000_0000,     // 100K value
                max_position_value: 10000000_0000, // 1M value
                // SAFETY: SCALE_2 * 5 fits in i32
                price_tolerance_pct: (SCALE_2 * 5) as i32, // 5%
                check_market_hours: true,
            },
            retry_config: RetryConfig {
                max_retries: MAX_RETRY_ATTEMPTS,
                initial_delay_ms: INITIAL_RETRY_DELAY_MS,
                max_delay_ms: MAX_RETRY_DELAY_MS,
                backoff_multiplier: 2,
            },
            order_cache_size: DEFAULT_ORDER_CACHE_SIZE,
            venue_timeout_ms: DEFAULT_VENUE_TIMEOUT_MS,
        }
    }
}
