//! Data aggregator configuration

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// Data aggregator configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatorConfig {
    /// Storage backend (redis, clickhouse, timescaledb)
    pub storage_backend: String,

    /// Storage connection URL
    pub storage_url: String,

    /// Enabled timeframes
    pub timeframes: Vec<String>,

    /// Maximum candles to keep in memory per symbol
    pub max_candles_memory: usize,

    /// Flush interval in seconds
    pub flush_interval_secs: u64,

    /// Volume profile configuration
    pub volume_profile: VolumeProfileConfig,

    /// Symbol-specific settings
    pub symbols: FxHashMap<String, SymbolConfig>,
}

/// Volume profile configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeProfileConfig {
    /// Number of price levels
    pub num_levels: usize,

    /// Value area percentage (fixed-point: 7000 = 70%)
    pub value_area_pct: i32,

    /// Update interval in seconds
    pub update_interval_secs: u64,
}

/// Per-symbol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolConfig {
    /// Tick size for price levels (fixed-point: 100 = 0.01)
    pub tick_size: i64,

    /// Minimum trade size to include (fixed-point: 10000 = 1 unit)
    pub min_trade_size: i64,

    /// Enable volume profile
    pub enable_volume_profile: bool,
}

impl Default for AggregatorConfig {
    fn default() -> Self {
        Self {
            storage_backend: "redis".to_string(),
            storage_url: "redis://127.0.0.1:6379".to_string(),
            timeframes: vec![
                "1m".to_string(),
                "5m".to_string(),
                "15m".to_string(),
                "1h".to_string(),
                "1d".to_string(),
            ],
            max_candles_memory: 1000,
            flush_interval_secs: 60,
            volume_profile: VolumeProfileConfig {
                num_levels: 50,
                value_area_pct: 7000, // 70% in fixed-point
                update_interval_secs: 300,
            },
            symbols: FxHashMap::default(),
        }
    }
}
