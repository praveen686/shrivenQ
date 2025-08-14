//! SIMD-optimized metrics calculation engine
//!
//! Enhanced version of the original metrics engine with:
//! - Advanced SIMD vectorization
//! - Cache-aligned data structures
//! - Zero allocations in hot paths
//! - Fixed-point arithmetic compliance

use common::{Px, Qty, Symbol, Ts};
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// SIMD-optimized metrics engine
#[repr(C, align(64))]
pub struct MetricsEngine {
    // Trading metrics (cache-aligned)
    total_trades: AtomicU64,
    winning_trades: AtomicU64,
    losing_trades: AtomicU64,

    // Volume metrics
    total_volume: AtomicU64,
    buy_volume: AtomicU64,
    sell_volume: AtomicU64,

    // PnL metrics (in smallest unit)
    gross_profit: AtomicI64,
    gross_loss: AtomicI64,
    max_drawdown: AtomicI64,
    peak_equity: AtomicI64,

    // Performance metrics (fixed-point)
    sharpe_ratio: AtomicU64,  // Fixed point * 10000
    win_rate: AtomicU64,      // Percentage * 10000
    profit_factor: AtomicU64, // Fixed point * 10000

    // Time metrics
    first_trade_time: AtomicU64,
    last_trade_time: AtomicU64,

    // Symbol-specific metrics
    symbol_metrics: RwLock<FxHashMap<Symbol, SymbolMetrics>>,

    // SIMD buffers for fast calculation (pre-allocated)
    returns_buffer: RwLock<Vec<f64>>,
    buffer_capacity: usize,

    _padding: [u8; 32],
}

/// Per-symbol metrics
#[repr(C, align(64))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMetrics {
    pub symbol: Symbol,
    pub trades: u64,
    pub volume: u64,
    pub pnl: i64,
    pub last_price: u64,
    pub best_bid: u64,
    pub best_ask: u64,
    pub spread_sum: u64,
    pub spread_count: u64,
    _padding: [u8; 16],
}

impl SymbolMetrics {
    fn new(symbol: Symbol) -> Self {
        Self {
            symbol,
            trades: 0,
            volume: 0,
            pnl: 0,
            last_price: 0,
            best_bid: 0,
            best_ask: 0,
            spread_sum: 0,
            spread_count: 0,
            _padding: [0; 16],
        }
    }

    /// Get average spread for this symbol
    #[inline(always)]
    pub fn avg_spread(&self) -> f64 {
        if self.spread_count > 0 {
            self.spread_sum as f64 / self.spread_count as f64 / 10000.0
        } else {
            0.0
        }
    }
}

impl MetricsEngine {
    /// Create new metrics engine with pre-allocated buffers
    pub fn new(buffer_capacity: usize) -> Self {
        let mut returns_buffer = Vec::with_capacity(buffer_capacity);
        // Pre-allocate to avoid allocations in hot paths
        returns_buffer.resize(buffer_capacity, 0.0);
        returns_buffer.clear(); // Clear but keep capacity

        Self {
            total_trades: AtomicU64::new(0),
            winning_trades: AtomicU64::new(0),
            losing_trades: AtomicU64::new(0),
            total_volume: AtomicU64::new(0),
            buy_volume: AtomicU64::new(0),
            sell_volume: AtomicU64::new(0),
            gross_profit: AtomicI64::new(0),
            gross_loss: AtomicI64::new(0),
            max_drawdown: AtomicI64::new(0),
            peak_equity: AtomicI64::new(0),
            sharpe_ratio: AtomicU64::new(0),
            win_rate: AtomicU64::new(0),
            profit_factor: AtomicU64::new(0),
            first_trade_time: AtomicU64::new(0),
            last_trade_time: AtomicU64::new(0),
            symbol_metrics: RwLock::new(FxHashMap::default()),
            returns_buffer: RwLock::new(returns_buffer),
            buffer_capacity,
            _padding: [0; 32],
        }
    }

    /// Update market data for spread analysis
    #[inline(always)]
    pub fn update_market(&self, symbol: Symbol, bid: Px, ask: Px, _ts: Ts) {
        let bid_raw = bid.as_i64() as u64;
        let ask_raw = ask.as_i64() as u64;
        let spread = ask_raw.saturating_sub(bid_raw);

        // Update symbol-specific metrics
        let mut metrics = self.symbol_metrics.write();
        let symbol_metric = metrics
            .entry(symbol)
            .or_insert_with(|| SymbolMetrics::new(symbol));

        symbol_metric.best_bid = bid_raw;
        symbol_metric.best_ask = ask_raw;
        symbol_metric.spread_sum = symbol_metric.spread_sum.saturating_add(spread);
        symbol_metric.spread_count = symbol_metric.spread_count.saturating_add(1);
    }

    /// Record fill and update metrics - HOT PATH
    #[inline(always)]
    pub fn record_fill(&self, order_id: u64, symbol: Symbol, qty: Qty, price: Px, ts: Ts) {
        // Update trade count
        self.total_trades.fetch_add(1, Ordering::Relaxed);

        // Calculate volume using fixed-point arithmetic
        let qty_raw = qty.raw();
        let price_raw = price.as_i64();

        // Volume = quantity * price (both in fixed-point)
        let volume = qty_raw.saturating_mul(price_raw) / 10000;
        let volume_unsigned = volume.unsigned_abs();

        self.total_volume
            .fetch_add(volume_unsigned, Ordering::Relaxed);

        // Track buy/sell volume
        if qty_raw > 0 {
            self.buy_volume
                .fetch_add(volume_unsigned, Ordering::Relaxed);
        } else {
            self.sell_volume
                .fetch_add(volume_unsigned, Ordering::Relaxed);
        }

        // Update time metrics
        self.last_trade_time.store(ts.nanos(), Ordering::Relaxed);

        // Set first trade time if this is the first trade
        let _ = self.first_trade_time.compare_exchange(
            0,
            ts.nanos(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        );

        // Update symbol-specific metrics
        {
            let mut metrics = self.symbol_metrics.write();
            let symbol_metric = metrics
                .entry(symbol)
                .or_insert_with(|| SymbolMetrics::new(symbol));
            symbol_metric.trades = symbol_metric.trades.saturating_add(1);
            symbol_metric.volume = symbol_metric.volume.saturating_add(volume_unsigned);
            symbol_metric.last_price = price_raw as u64;
        }

        // Add return to buffer for Sharpe calculation
        self.add_return_to_buffer(volume);

        tracing::debug!(
            order_id = order_id,
            symbol = symbol.0,
            qty = qty_raw,
            price = price_raw,
            volume = volume,
            "Fill recorded"
        );
    }

    /// Add return to SIMD buffer - optimized for hot path
    #[inline(always)]
    fn add_return_to_buffer(&self, return_value: i64) {
        let mut buffer = self.returns_buffer.write();

        // Convert fixed-point to f64 for analytics (ADR-0005 compliant)
        #[allow(clippy::cast_precision_loss)]
        let return_f64 = return_value as f64 / 10000.0;

        // Circular buffer logic to maintain fixed size
        if buffer.len() >= self.buffer_capacity {
            buffer.remove(0); // Remove oldest
        }
        buffer.push(return_f64);
    }

    /// Calculate Sharpe ratio using AVX2 SIMD instructions
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    #[allow(clippy::cast_precision_loss)] // Analytics boundary - ADR-0005
    unsafe fn calculate_sharpe_simd(&self) -> f64 {
        let buffer = self.returns_buffer.read();
        if buffer.len() < 2 {
            return 0.0;
        }

        let len = buffer.len();
        let data = buffer.as_slice();

        // Calculate mean using AVX2
        let mut sum = _mm256_setzero_pd();
        let mut chunks = data.chunks_exact(4);

        for chunk in chunks.by_ref() {
            let v = unsafe { _mm256_loadu_pd(chunk.as_ptr()) };
            sum = _mm256_add_pd(sum, v);
        }

        // Handle remainder
        let remainder = chunks.remainder();

        // Extract sum components
        let mut sum_array = [0.0; 4];
        unsafe {
            _mm256_storeu_pd(sum_array.as_mut_ptr(), sum);
        }
        let mut total_sum = sum_array.iter().sum::<f64>();

        // Add remainder elements
        for &val in remainder {
            total_sum += val;
        }

        let mean = total_sum / len as f64;

        // Calculate variance using AVX2
        let mean_vec = _mm256_set1_pd(mean);
        let mut variance_sum = _mm256_setzero_pd();
        let mut chunks = data.chunks_exact(4);

        for chunk in chunks.by_ref() {
            let v = unsafe { _mm256_loadu_pd(chunk.as_ptr()) };
            let diff = _mm256_sub_pd(v, mean_vec);
            let squared = _mm256_mul_pd(diff, diff);
            variance_sum = _mm256_add_pd(variance_sum, squared);
        }

        // Handle remainder for variance
        let remainder = chunks.remainder();

        // Extract variance sum components
        let mut var_array = [0.0; 4];
        unsafe {
            _mm256_storeu_pd(var_array.as_mut_ptr(), variance_sum);
        }
        let mut total_variance = var_array.iter().sum::<f64>();

        // Add remainder variance
        for &val in remainder {
            let diff = val - mean;
            total_variance += diff * diff;
        }

        let variance = total_variance / len as f64;
        let std_dev = variance.sqrt();

        if std_dev > 0.0 {
            // Annualized Sharpe ratio (assuming daily returns)
            (mean / std_dev) * (252.0_f64).sqrt()
        } else {
            0.0
        }
    }

    /// Calculate Sharpe ratio without SIMD (fallback)
    #[allow(clippy::cast_precision_loss)] // Analytics boundary - ADR-0005
    pub fn calculate_sharpe_standard(&self) -> f64 {
        let buffer = self.returns_buffer.read();
        if buffer.len() < 2 {
            return 0.0;
        }

        let mean = buffer.iter().sum::<f64>() / buffer.len() as f64;

        let variance = buffer
            .iter()
            .map(|&r| {
                let diff = r - mean;
                diff * diff
            })
            .sum::<f64>()
            / buffer.len() as f64;

        let std_dev = variance.sqrt();

        if std_dev > 0.0 {
            (mean / std_dev) * (252.0_f64).sqrt() // Annualized
        } else {
            0.0
        }
    }

    /// Detect if AVX2 is available at runtime
    #[cfg(target_arch = "x86_64")]
    fn has_avx2() -> bool {
        use std::arch::x86_64::__cpuid;
        unsafe {
            let cpuid = __cpuid(7);
            (cpuid.ebx & (1 << 5)) != 0 // AVX2 bit
        }
    }

    /// Calculate Sharpe ratio (automatically uses SIMD if available)
    #[allow(clippy::cast_precision_loss)]
    pub fn calculate_sharpe(&self) -> f64 {
        #[cfg(target_arch = "x86_64")]
        {
            if Self::has_avx2() && is_x86_feature_detected!("avx2") {
                unsafe { self.calculate_sharpe_simd() }
            } else {
                self.calculate_sharpe_standard()
            }
        }

        #[cfg(not(target_arch = "x86_64"))]
        {
            self.calculate_sharpe_standard()
        }
    }

    /// Update PnL and drawdown metrics
    #[inline(always)]
    pub fn update_pnl(&self, realized_pnl: i64, unrealized_pnl: i64) {
        let total_pnl = realized_pnl + unrealized_pnl;

        // Update profit/loss tracking
        if realized_pnl > 0 {
            self.gross_profit.fetch_add(realized_pnl, Ordering::Relaxed);
            self.winning_trades.fetch_add(1, Ordering::Relaxed);
        } else if realized_pnl < 0 {
            self.gross_loss.fetch_add(realized_pnl, Ordering::Relaxed);
            self.losing_trades.fetch_add(1, Ordering::Relaxed);
        }

        // Update drawdown calculation
        let current_equity = total_pnl;
        let peak = self.peak_equity.load(Ordering::Acquire);

        if current_equity > peak {
            self.peak_equity.store(current_equity, Ordering::Release);
        } else {
            let drawdown = peak - current_equity;
            let max_dd = self.max_drawdown.load(Ordering::Acquire);
            if drawdown > max_dd {
                self.max_drawdown.store(drawdown, Ordering::Release);
            }
        }
    }

    /// Get comprehensive trading metrics
    #[allow(clippy::cast_precision_loss)] // Analytics boundary - ADR-0005
    pub fn get_metrics(&self) -> TradingMetrics {
        let total = self.total_trades.load(Ordering::Acquire);
        let wins = self.winning_trades.load(Ordering::Acquire);
        let losses = self.losing_trades.load(Ordering::Acquire);

        let win_rate = if total > 0 {
            (wins as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let gross_profit = self.gross_profit.load(Ordering::Acquire);
        let gross_loss = self.gross_loss.load(Ordering::Acquire).abs();

        let profit_factor = if gross_loss > 0 {
            gross_profit as f64 / gross_loss as f64
        } else {
            0.0
        };

        // Calculate real-time Sharpe ratio
        let sharpe_ratio = self.calculate_sharpe();

        // Get symbol breakdown
        let symbol_breakdown = {
            let metrics = self.symbol_metrics.read();
            metrics.clone()
        };

        TradingMetrics {
            total_trades: total,
            winning_trades: wins,
            losing_trades: losses,
            win_rate,
            profit_factor,
            sharpe_ratio,
            max_drawdown: self.max_drawdown.load(Ordering::Acquire),
            total_volume: self.total_volume.load(Ordering::Acquire),
            buy_volume: self.buy_volume.load(Ordering::Acquire),
            sell_volume: self.sell_volume.load(Ordering::Acquire),
            first_trade_time: self.first_trade_time.load(Ordering::Acquire),
            last_trade_time: self.last_trade_time.load(Ordering::Acquire),
            symbol_breakdown,
        }
    }

    /// Reset all metrics (useful for testing)
    pub fn reset(&self) {
        self.total_trades.store(0, Ordering::Release);
        self.winning_trades.store(0, Ordering::Release);
        self.losing_trades.store(0, Ordering::Release);
        self.total_volume.store(0, Ordering::Release);
        self.buy_volume.store(0, Ordering::Release);
        self.sell_volume.store(0, Ordering::Release);
        self.gross_profit.store(0, Ordering::Release);
        self.gross_loss.store(0, Ordering::Release);
        self.max_drawdown.store(0, Ordering::Release);
        self.peak_equity.store(0, Ordering::Release);
        self.first_trade_time.store(0, Ordering::Release);
        self.last_trade_time.store(0, Ordering::Release);

        self.symbol_metrics.write().clear();
        self.returns_buffer.write().clear();
    }
}

/// Enhanced trading metrics structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TradingMetrics {
    // Basic trading metrics
    pub total_trades: u64,
    pub winning_trades: u64,
    pub losing_trades: u64,
    pub win_rate: f64,
    pub profit_factor: f64,

    // Performance metrics
    pub sharpe_ratio: f64,
    pub max_drawdown: i64,

    // Volume metrics
    pub total_volume: u64,
    pub buy_volume: u64,
    pub sell_volume: u64,

    // Time metrics
    pub first_trade_time: u64,
    pub last_trade_time: u64,

    // Symbol breakdown
    pub symbol_breakdown: FxHashMap<Symbol, SymbolMetrics>,
}

impl TradingMetrics {
    /// Get trading duration in seconds
    pub fn trading_duration_seconds(&self) -> u64 {
        if self.first_trade_time > 0 && self.last_trade_time > self.first_trade_time {
            (self.last_trade_time - self.first_trade_time) / 1_000_000_000
        } else {
            0
        }
    }

    /// Get average trade volume
    pub fn avg_trade_volume(&self) -> f64 {
        if self.total_trades > 0 {
            self.total_volume as f64 / self.total_trades as f64
        } else {
            0.0
        }
    }

    /// Get buy/sell ratio
    pub fn buy_sell_ratio(&self) -> f64 {
        if self.sell_volume > 0 {
            self.buy_volume as f64 / self.sell_volume as f64
        } else if self.buy_volume > 0 {
            f64::INFINITY
        } else {
            0.0
        }
    }
}

/// PnL structure (retained for compatibility)
#[repr(C, align(64))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PnL {
    pub realized: i64,
    pub unrealized: i64,
    pub total: i64,
    pub timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_engine_creation() {
        let engine = MetricsEngine::new(1000);
        let metrics = engine.get_metrics();

        assert_eq!(metrics.total_trades, 0);
        assert_eq!(metrics.total_volume, 0);
        assert_eq!(metrics.sharpe_ratio, 0.0);
    }

    #[test]
    fn test_fill_recording() {
        let engine = MetricsEngine::new(1000);
        let symbol = Symbol::new(1);

        // Record a profitable fill
        engine.record_fill(
            1,
            symbol,
            Qty::from_i64(1000000), // 100 units
            Px::from_i64(1000000),  // $100
            Ts::now(),
        );

        let metrics = engine.get_metrics();
        assert_eq!(metrics.total_trades, 1);
        assert!(metrics.total_volume > 0);
        assert!(metrics.symbol_breakdown.contains_key(&symbol));
    }

    #[test]
    fn test_market_update() {
        let engine = MetricsEngine::new(1000);
        let symbol = Symbol::new(1);

        engine.update_market(
            symbol,
            Px::from_i64(990000),  // $99 bid
            Px::from_i64(1000000), // $100 ask
            Ts::now(),
        );

        let metrics = engine.get_metrics();
        if let Some(symbol_metric) = metrics.symbol_breakdown.get(&symbol) {
            assert!(symbol_metric.avg_spread() > 0.0);
        }
    }

    #[test]
    fn test_sharpe_calculation() {
        let engine = MetricsEngine::new(1000);

        // Add some returns
        for i in 0..100 {
            engine.add_return_to_buffer(i * 100 - 5000); // Mix of positive/negative
        }

        let sharpe = engine.calculate_sharpe();
        assert!(sharpe.is_finite());
    }

    #[test]
    fn test_pnl_update() {
        let engine = MetricsEngine::new(1000);

        // Record some PnL
        engine.update_pnl(1000, 500); // Profitable
        engine.update_pnl(-500, -200); // Loss

        let metrics = engine.get_metrics();
        assert_eq!(metrics.winning_trades, 1);
        assert_eq!(metrics.losing_trades, 1);
        assert!(metrics.profit_factor > 0.0);
    }

    #[test]
    fn test_reset_functionality() {
        let engine = MetricsEngine::new(1000);
        let symbol = Symbol::new(1);

        // Add some data
        engine.record_fill(
            1,
            symbol,
            Qty::from_i64(1000000),
            Px::from_i64(1000000),
            Ts::now(),
        );

        // Verify data exists
        let metrics_before = engine.get_metrics();
        assert_eq!(metrics_before.total_trades, 1);

        // Reset and verify clean state
        engine.reset();
        let metrics_after = engine.get_metrics();
        assert_eq!(metrics_after.total_trades, 0);
        assert_eq!(metrics_after.total_volume, 0);
    }
}
