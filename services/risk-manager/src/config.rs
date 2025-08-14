//! Risk manager configuration

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// Risk manager configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    /// Risk limits
    pub limits: crate::RiskLimits,

    /// Alert thresholds
    pub alert_thresholds: AlertThresholds,

    /// Strategy-specific limits
    pub strategy_limits: FxHashMap<String, crate::limits::StrategyLimits>,

    /// Exchange-specific limits
    pub exchange_limits: FxHashMap<String, crate::limits::ExchangeLimits>,
}

/// Alert thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// Exposure warning level (fixed-point: 8000 = 80%)
    pub exposure_warning_pct: i32,

    /// Drawdown warning level (fixed-point: 500 = 5%)
    pub drawdown_warning_pct: i32,

    /// Loss rate warning (losses per minute)
    pub loss_rate_warning: u32,
}
