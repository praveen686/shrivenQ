//! Risk management engine - Pre-allocated checks

use crate::core::EngineConfig;
use common::{Px, Qty, Side, Symbol};
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// Risk limits - cache-aligned
#[repr(C, align(64))]
pub struct RiskLimits {
    // Position limits
    max_position_size: u64,
    max_position_value: u64,
    max_total_exposure: u64,

    // Order limits
    max_order_size: u64,
    max_order_value: u64,
    max_orders_per_minute: u32,

    // Loss limits
    max_daily_loss: i64,
    max_drawdown: i64,

    // Rate limits
    order_rate_window_ms: u64,
    cancel_rate_window_ms: u64,

    _padding: [u8; 16],
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            max_position_size: 10000,
            max_position_value: 10_000_000, // 1 crore in paise
            max_total_exposure: 50_000_000, // 5 crore in paise
            max_order_size: 1000,
            max_order_value: 1_000_000, // 10 lakh in paise
            max_orders_per_minute: 100,
            max_daily_loss: -500_000, // 5 lakh loss limit
            max_drawdown: -1_000_000, // 10 lakh drawdown
            order_rate_window_ms: 1000,
            cancel_rate_window_ms: 1000,
            _padding: [0; 16],
        }
    }
}

/// Per-symbol risk tracking
#[repr(C, align(64))]
struct SymbolRisk {
    position_size: AtomicI64,
    position_value: AtomicU64,
    orders_sent: AtomicU64,
    last_order_time: AtomicU64,

    // Circuit breaker
    breaker_triggered: AtomicU64, // 0 = ok, >0 = triggered until timestamp
    consecutive_losses: AtomicU64,

    _padding: [u8; 16],
}

impl SymbolRisk {
    fn new() -> Self {
        Self {
            position_size: AtomicI64::new(0),
            position_value: AtomicU64::new(0),
            orders_sent: AtomicU64::new(0),
            last_order_time: AtomicU64::new(0),
            breaker_triggered: AtomicU64::new(0),
            consecutive_losses: AtomicU64::new(0),
            _padding: [0; 16],
        }
    }
}

/// Risk engine with pre-allocated checks
#[repr(C, align(64))]
pub struct RiskEngine {
    config: Arc<EngineConfig>,
    limits: RiskLimits,

    // Per-symbol tracking
    symbol_risks: DashMap<Symbol, SymbolRisk>,

    // Global tracking
    total_exposure: AtomicU64,
    daily_pnl: AtomicI64,
    peak_value: AtomicU64,
    current_drawdown: AtomicI64,

    // Rate limiting
    order_timestamps: Vec<u64>, // Ring buffer for rate limiting
    order_timestamp_idx: AtomicU64,

    // Kill switch
    emergency_stop: AtomicU64, // 0 = normal, 1 = stopped

    _padding: [u8; 24],
}

impl RiskEngine {
    pub fn new(config: Arc<EngineConfig>) -> Self {
        let mut order_timestamps = Vec::with_capacity(1000);
        order_timestamps.resize(1000, 0);

        Self {
            config,
            limits: RiskLimits::default(),
            symbol_risks: DashMap::with_capacity(100),
            total_exposure: AtomicU64::new(0),
            daily_pnl: AtomicI64::new(0),
            peak_value: AtomicU64::new(0),
            current_drawdown: AtomicI64::new(0),
            order_timestamps,
            order_timestamp_idx: AtomicU64::new(0),
            emergency_stop: AtomicU64::new(0),
            _padding: [0; 24],
        }
    }

    /// Check if order passes risk checks - BRANCH-FREE HOT PATH
    #[inline(always)]
    pub fn check_order(&self, symbol: Symbol, side: Side, qty: Qty, price: Option<Px>) -> bool {
        // Emergency stop check (branch-free)
        let stopped = self.emergency_stop.load(Ordering::Acquire);
        if stopped != 0 {
            return false;
        }

        // Convert qty to actual units (qty is stored as fixed point with 4 decimals)
        let qty_units = (qty.as_f64()) as u64;
        let price_raw = price.map(|p| (p.as_f64() * 100.0) as u64).unwrap_or(10000);
        let order_value = qty_units * price_raw;

        // Size checks (branch-free using comparison masks)
        let size_ok = (qty_units <= self.limits.max_order_size) as u64;
        let value_ok = (order_value <= self.limits.max_order_value) as u64;

        // Get or create symbol risk
        let symbol_risk = self
            .symbol_risks
            .entry(symbol)
            .or_insert_with(|| SymbolRisk::new());

        // Check circuit breaker
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let breaker_time = symbol_risk.breaker_triggered.load(Ordering::Acquire);
        let breaker_ok = (breaker_time == 0 || now > breaker_time) as u64;

        // Position limit check
        let current_pos = symbol_risk.position_size.load(Ordering::Acquire);
        let new_pos = match side {
            Side::Bid => current_pos + qty_units as i64, // Buy
            Side::Ask => current_pos - qty_units as i64, // Sell
        };
        let position_ok = (new_pos.abs() as u64 <= self.limits.max_position_size) as u64;

        // Exposure check
        let current_exposure = self.total_exposure.load(Ordering::Acquire);
        let exposure_ok = (current_exposure + order_value <= self.limits.max_total_exposure) as u64;

        // Daily loss check
        let daily_pnl = self.daily_pnl.load(Ordering::Acquire);
        let loss_ok = (daily_pnl >= self.limits.max_daily_loss) as u64;

        // Rate limit check (simplified for performance)
        let rate_ok = self.check_rate_limit(now);

        // All checks must pass (branch-free AND)
        let all_checks = size_ok
            & value_ok
            & breaker_ok
            & position_ok
            & exposure_ok
            & loss_ok
            & (rate_ok as u64);

        all_checks != 0
    }

    /// Check rate limiting - uses ring buffer
    #[inline(always)]
    fn check_rate_limit(&self, now: u64) -> bool {
        let idx = self.order_timestamp_idx.fetch_add(1, Ordering::Relaxed) as usize;
        let ring_idx = idx % self.order_timestamps.len();

        // Check oldest timestamp in window
        let oldest = self.order_timestamps[ring_idx];
        let window_start = now.saturating_sub(60_000); // 60 second window

        if oldest > window_start {
            // Too many orders in window
            return false;
        }

        // Store new timestamp (unsafe for performance, but safe in practice)
        unsafe {
            let ptr = self.order_timestamps.as_ptr() as *mut u64;
            ptr.add(ring_idx).write(now);
        }

        true
    }

    /// Update position risk after fill
    #[inline(always)]
    pub fn update_position(&self, symbol: Symbol, side: Side, qty: Qty, _price: Px) {
        let qty_units = qty.as_f64() as i64;

        if let Some(risk) = self.symbol_risks.get(&symbol) {
            let delta = match side {
                Side::Bid => qty_units,  // Buy
                Side::Ask => -qty_units, // Sell
            };

            risk.position_size.fetch_add(delta, Ordering::AcqRel);
        }
    }

    /// Update PnL for risk tracking
    #[inline(always)]
    pub fn update_pnl(&self, pnl: i64) {
        self.daily_pnl.store(pnl, Ordering::Release);

        // Update drawdown
        let peak = self.peak_value.load(Ordering::Acquire) as i64;
        if pnl > peak {
            self.peak_value.store(pnl as u64, Ordering::Release);
            self.current_drawdown.store(0, Ordering::Release);
        } else {
            let drawdown = peak - pnl;
            self.current_drawdown.store(drawdown, Ordering::Release);

            // Trigger emergency stop if drawdown exceeded
            if drawdown > self.limits.max_drawdown.abs() {
                self.emergency_stop.store(1, Ordering::Release);
            }
        }
    }

    /// Reset daily counters
    pub fn reset_daily(&self) {
        self.daily_pnl.store(0, Ordering::Release);

        // Reset symbol counters
        for entry in self.symbol_risks.iter() {
            entry.orders_sent.store(0, Ordering::Release);
            entry.consecutive_losses.store(0, Ordering::Release);
        }
    }

    /// Emergency stop
    #[inline(always)]
    pub fn emergency_stop(&self) {
        self.emergency_stop.store(1, Ordering::Release);
    }

    /// Resume from emergency stop
    pub fn resume(&self) {
        self.emergency_stop.store(0, Ordering::Release);
    }

    /// Get risk metrics
    pub fn get_metrics(&self) -> RiskMetrics {
        RiskMetrics {
            total_exposure: self.total_exposure.load(Ordering::Acquire),
            daily_pnl: self.daily_pnl.load(Ordering::Acquire),
            current_drawdown: self.current_drawdown.load(Ordering::Acquire),
            emergency_stopped: self.emergency_stop.load(Ordering::Acquire) != 0,
            symbols_at_risk: self.symbol_risks.len(),
        }
    }
}

/// Risk metrics
#[repr(C)]
pub struct RiskMetrics {
    pub total_exposure: u64,
    pub daily_pnl: i64,
    pub current_drawdown: i64,
    pub emergency_stopped: bool,
    pub symbols_at_risk: usize,
}
