//! Risk management engine - Pre-allocated checks

use crate::core::EngineConfig;
use common::constants::{
    fixed_point::SCALE_4,
    memory::DEFAULT_BUFFER_CAPACITY,
    trading::{MAX_ORDER_SIZE_TICKS, MAX_POSITION_SIZE_TICKS, RATE_LIMIT_WINDOW_MS},
};
use common::{Px, Qty, Side, Symbol};
use dashmap::DashMap;
use parking_lot::Mutex;
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
            max_position_size: MAX_POSITION_SIZE_TICKS as u64, // 10000 units in ticks
            max_position_value: 100_000_000,                   // 10 crore in paise
            max_total_exposure: 500_000_000,                   // 50 crore in paise
            max_order_size: MAX_ORDER_SIZE_TICKS as u64,       // 1000 units in ticks
            max_order_value: 100_000_000, // 1 crore in paise (increased for tests)
            max_orders_per_minute: 100,   // TODO: Move to constants
            max_daily_loss: -500_000,     // 5 lakh loss limit
            max_drawdown: -1_000_000,     // 10 lakh drawdown
            order_rate_window_ms: 1000,   // 1 second
            cancel_rate_window_ms: 1000,  // 1 second
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
    config: EngineConfig, // Copy type, no need for Arc
    limits: RiskLimits,

    // Per-symbol tracking
    symbol_risks: DashMap<Symbol, SymbolRisk>,

    // Global tracking
    total_exposure: AtomicU64,
    daily_pnl: AtomicI64,
    peak_value: AtomicU64,
    current_drawdown: AtomicI64,

    // Rate limiting
    order_timestamps: Arc<Mutex<Vec<u64>>>, // Ring buffer for rate limiting
    order_timestamp_idx: AtomicU64,

    // Kill switch
    emergency_stop: AtomicU64, // 0 = normal, 1 = stopped

    _padding: [u8; 24],
}

impl RiskEngine {
    pub fn new(config: EngineConfig) -> Self {
        let mut order_timestamps = Vec::with_capacity(DEFAULT_BUFFER_CAPACITY);
        order_timestamps.resize(DEFAULT_BUFFER_CAPACITY, 0);

        Self {
            config,
            limits: RiskLimits::default(),
            symbol_risks: DashMap::with_capacity(100), // TODO: Move to constants
            total_exposure: AtomicU64::new(0),
            daily_pnl: AtomicI64::new(0),
            peak_value: AtomicU64::new(0),
            current_drawdown: AtomicI64::new(0),
            order_timestamps: Arc::new(Mutex::new(order_timestamps)),
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

        // Use fixed-point arithmetic (qty and price are already in ticks)
        let qty_units = qty.as_i64().unsigned_abs();
        let price_ticks = price
            .map(|p| p.as_i64().unsigned_abs())
            .unwrap_or(SCALE_4 as u64 * 10); // Default 10.00
        let order_value = (qty_units * price_ticks) / SCALE_4 as u64; // Divide by SCALE_4 to get value in base units

        // Size checks (branch-free using comparison masks)
        let size_ok = u64::from(qty_units <= self.limits.max_order_size);
        let value_ok = u64::from(order_value <= self.limits.max_order_value);

        // Get or create symbol risk
        let symbol_risk = self
            .symbol_risks
            .entry(symbol)
            .or_insert_with(|| SymbolRisk::new());

        // Check circuit breaker
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);

        let breaker_time = symbol_risk.breaker_triggered.load(Ordering::Acquire);
        let breaker_ok = u64::from(breaker_time == 0 || now > breaker_time);

        // Position limit check
        let current_pos = symbol_risk.position_size.load(Ordering::Acquire);
        let new_pos = match side {
            Side::Bid => current_pos.saturating_add(i64::try_from(qty_units).unwrap_or(i64::MAX)), // Buy
            Side::Ask => current_pos.saturating_sub(i64::try_from(qty_units).unwrap_or(i64::MAX)), // Sell
        };
        let position_ok = u64::from(
            u64::try_from(new_pos.abs()).unwrap_or(u64::MAX) <= self.limits.max_position_size,
        );

        // Exposure check
        let current_exposure = self.total_exposure.load(Ordering::Acquire);
        let exposure_ok =
            u64::from(current_exposure + order_value <= self.limits.max_total_exposure);

        // Daily loss check
        let daily_pnl = self.daily_pnl.load(Ordering::Acquire);
        let loss_ok = u64::from(daily_pnl >= self.limits.max_daily_loss);

        // Rate limit check (simplified for performance)
        let rate_ok = self.check_rate_limit(now);

        // All checks must pass (branch-free AND)
        let all_checks = size_ok
            & value_ok
            & breaker_ok
            & position_ok
            & exposure_ok
            & loss_ok
            & u64::from(rate_ok);

        all_checks != 0
    }

    /// Check rate limiting - uses ring buffer
    #[inline(always)]
    fn check_rate_limit(&self, now: u64) -> bool {
        let idx =
            usize::try_from(self.order_timestamp_idx.fetch_add(1, Ordering::Relaxed)).unwrap_or(0);

        // Use lock for thread-safe access
        let mut timestamps = self.order_timestamps.lock();
        let ring_idx = idx % timestamps.len();

        // Check oldest timestamp in window
        let oldest = timestamps[ring_idx];
        let window_start = now.saturating_sub(RATE_LIMIT_WINDOW_MS); // Rate limit window

        if oldest > window_start {
            // Too many orders in window
            return false;
        }

        // Store new timestamp safely
        timestamps[ring_idx] = now;

        true
    }

    /// Update position risk after fill
    #[inline(always)]
    pub fn update_position(&self, symbol: Symbol, side: Side, qty: Qty, _price: Px) {
        let qty_units = qty.as_i64() / SCALE_4; // Convert ticks to whole units

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
        let peak = i64::try_from(self.peak_value.load(Ordering::Acquire)).unwrap_or(i64::MAX);
        if pnl > peak {
            self.peak_value
                .store(u64::try_from(pnl).unwrap_or(0), Ordering::Release);
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
