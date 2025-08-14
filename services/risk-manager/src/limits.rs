//! Risk limits management

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// Strategy-specific risk limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyLimits {
    /// Strategy identifier
    pub strategy_id: String,
    /// Maximum allocation
    pub max_allocation: u64,
    /// Maximum positions
    pub max_positions: u32,
    /// Allowed symbols
    pub allowed_symbols: Vec<String>,
    /// Custom limits
    pub custom_limits: FxHashMap<String, i64>, // Fixed-point values
}

/// Exchange-specific limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeLimits {
    /// Exchange name
    pub exchange: String,
    /// Maximum order rate
    pub max_order_rate: u32,
    /// Maximum cancel rate
    pub max_cancel_rate: u32,
    /// Maximum message rate
    pub max_message_rate: u32,
}

/// Time-based limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeLimits {
    /// Trading hours (UTC)
    pub trading_hours: Vec<(u32, u32)>,
    /// Blackout periods
    pub blackout_periods: Vec<String>,
    /// End of day flattening time
    pub eod_flatten_time: Option<u32>,
}
