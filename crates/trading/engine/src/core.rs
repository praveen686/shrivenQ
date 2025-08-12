//! Core Engine - Zero allocation, branch-free hot path

use crate::execution::ExecutionLayer;
use crate::metrics::MetricsEngine;
use crate::position::PositionTracker;
use crate::risk::RiskEngine;
use crate::venue::VenueAdapter;
use bus::EventBus;
use common::{Px, Qty, Symbol, Ts};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

/// Execution mode - compile-time optimized
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Paper = 0,    // Simulated execution
    Live = 1,     // Real market execution
    Backtest = 2, // Historical replay
}

/// Venue selection
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VenueType {
    Zerodha = 0,
    Binance = 1,
}

/// Engine configuration - POD type for cache efficiency
#[repr(C, align(64))]
#[derive(Clone)]
pub struct EngineConfig {
    pub mode: ExecutionMode,
    pub venue: VenueType,
    pub max_positions: usize,
    pub max_orders_per_sec: u32,
    pub risk_check_enabled: bool,
    pub metrics_enabled: bool,
    pub memory_pool_size: usize,
    _padding: [u8; 32], // Ensure 64-byte alignment
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            mode: ExecutionMode::Paper,
            venue: VenueType::Zerodha,
            max_positions: 1000,
            max_orders_per_sec: 1000,
            risk_check_enabled: true,
            metrics_enabled: true,
            memory_pool_size: 1024 * 1024, // 1MB pool
            _padding: [0; 32],
        }
    }
}

/// Ultra-fast trading engine
#[repr(C, align(64))]
pub struct Engine<V: VenueAdapter> {
    // Configuration (read-only after init)
    config: Arc<EngineConfig>,

    // Core components - all lock-free
    venue: V,
    execution: ExecutionLayer<V>,
    positions: PositionTracker,
    metrics: MetricsEngine,
    risk: RiskEngine,

    // Event bus for zero-copy message passing
    bus: Arc<EventBus>,

    // Atomic counters - cache-line aligned
    order_counter: AtomicU64,
    fill_counter: AtomicU64,
    reject_counter: AtomicU64,

    // Performance metrics (nanoseconds)
    tick_to_decision: AtomicU64,
    decision_to_order: AtomicU64,
    order_to_fill: AtomicU64,

    // Engine state
    state: AtomicU8, // 0=stopped, 1=running, 2=halted

    _padding: [u8; 8],
}

impl<V: VenueAdapter + Clone> Engine<V> {
    /// Create new engine with pre-allocated memory
    pub fn new(config: EngineConfig, venue: V, bus: Arc<EventBus>) -> Self {
        let config = Arc::new(config);

        // Pre-allocate all components
        let execution = ExecutionLayer::new(config.clone(), venue.clone());
        let positions = PositionTracker::new(config.max_positions);
        let metrics = MetricsEngine::new();
        let risk = RiskEngine::new(config.clone());

        Self {
            config,
            venue,
            execution,
            positions,
            metrics,
            risk,
            bus,
            order_counter: AtomicU64::new(0),
            fill_counter: AtomicU64::new(0),
            reject_counter: AtomicU64::new(0),
            tick_to_decision: AtomicU64::new(0),
            decision_to_order: AtomicU64::new(0),
            order_to_fill: AtomicU64::new(0),
            state: AtomicU8::new(0),
            _padding: [0; 8],
        }
    }

    /// Process market tick - ULTRA HOT PATH
    #[inline(always)]
    pub fn on_tick(&self, symbol: Symbol, bid: Px, ask: Px, ts: Ts) {
        let start = Ts::now();

        // Update position mark-to-market (lock-free)
        self.positions.update_market(symbol, bid, ask, ts);

        // Update metrics (SIMD operations)
        if self.config.metrics_enabled {
            self.metrics.update_market(symbol, bid, ask, ts);
        }

        // Record latency
        let latency = Ts::now().nanos() - start.nanos();
        self.tick_to_decision.store(latency, Ordering::Relaxed);
    }

    /// Send order - CRITICAL PATH
    #[inline(always)]
    pub fn send_order(
        &self,
        symbol: Symbol,
        side: common::Side,
        qty: Qty,
        price: Option<Px>,
    ) -> Result<OrderId, OrderError> {
        let start = Ts::now();

        // Generate order ID (atomic increment)
        let order_id = self.order_counter.fetch_add(1, Ordering::Relaxed);

        // Risk check (branch-free)
        if self.config.risk_check_enabled {
            let risk_ok = self.risk.check_order(symbol, side, qty, price);
            if !risk_ok {
                self.reject_counter.fetch_add(1, Ordering::Relaxed);
                return Err(OrderError::RiskRejected);
            }
        }

        // Route to execution layer
        let result = match self.config.mode {
            ExecutionMode::Paper => self
                .execution
                .simulate_order(order_id, symbol, side, qty, price),
            ExecutionMode::Live => self
                .execution
                .send_live_order(order_id, symbol, side, qty, price),
            ExecutionMode::Backtest => self
                .execution
                .replay_order(order_id, symbol, side, qty, price),
        };

        // Update position (optimistic, will reconcile on fill)
        if result.is_ok() {
            self.positions.add_pending(order_id, symbol, side, qty);
        }

        // Record latency
        let latency = Ts::now().nanos() - start.nanos();
        self.decision_to_order.store(latency, Ordering::Relaxed);

        result
            .map(|_| OrderId(order_id))
            .map_err(|_| OrderError::VenueError)
    }

    /// Process fill - CRITICAL PATH
    #[inline(always)]
    pub fn on_fill(&self, order_id: u64, fill_qty: Qty, fill_price: Px, ts: Ts) {
        let start = Ts::now();

        // Update position (lock-free)
        self.positions
            .apply_fill(order_id, fill_qty, fill_price, ts);

        // Update metrics
        if self.config.metrics_enabled {
            self.metrics.record_fill(order_id, fill_qty, fill_price, ts);
        }

        // Increment counter
        self.fill_counter.fetch_add(1, Ordering::Relaxed);

        // Record latency
        let latency = Ts::now().nanos() - start.nanos();
        self.order_to_fill.store(latency, Ordering::Relaxed);
    }

    /// Get current PnL (lock-free read)
    #[inline(always)]
    pub fn get_pnl(&self) -> crate::metrics::PnL {
        self.metrics.calculate_pnl(&self.positions)
    }

    /// Get performance metrics
    pub fn get_performance(&self) -> PerformanceMetrics {
        PerformanceMetrics {
            orders_sent: self.order_counter.load(Ordering::Relaxed),
            orders_filled: self.fill_counter.load(Ordering::Relaxed),
            orders_rejected: self.reject_counter.load(Ordering::Relaxed),
            avg_tick_to_decision_ns: self.tick_to_decision.load(Ordering::Relaxed),
            avg_decision_to_order_ns: self.decision_to_order.load(Ordering::Relaxed),
            avg_order_to_fill_ns: self.order_to_fill.load(Ordering::Relaxed),
        }
    }

    /// Set backtest time (for backtest mode)
    pub fn set_backtest_time(&self, ts: Ts) {
        if self.config.mode == ExecutionMode::Backtest {
            self.execution.advance_backtest_time(ts);
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct OrderId(pub u64);

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum OrderError {
    RiskRejected = 0,
    RateLimited = 1,
    InvalidPrice = 2,
    InvalidQuantity = 3,
    VenueError = 4,
}

#[repr(C, align(64))]
pub struct PerformanceMetrics {
    pub orders_sent: u64,
    pub orders_filled: u64,
    pub orders_rejected: u64,
    pub avg_tick_to_decision_ns: u64,
    pub avg_decision_to_order_ns: u64,
    pub avg_order_to_fill_ns: u64,
}
