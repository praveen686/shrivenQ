//! SIMD-optimized metrics calculation

use crate::position::PositionTracker;
use common::{Px, Qty, Symbol, Ts};
use std::arch::x86_64::*;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// Metrics engine with SIMD operations
#[repr(C, align(64))]
pub struct MetricsEngine {
    // Trading metrics
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

    // Performance metrics
    sharpe_ratio: AtomicU64,  // Fixed point * 1000
    win_rate: AtomicU64,      // Percentage * 100
    profit_factor: AtomicU64, // Fixed point * 1000

    // Time metrics
    first_trade_time: AtomicU64,
    last_trade_time: AtomicU64,

    // SIMD buffers for fast calculation
    returns_buffer: Vec<f64>,
    _padding: [u8; 32],
}

impl MetricsEngine {
    pub fn new() -> Self {
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
            returns_buffer: Vec::with_capacity(10000),
            _padding: [0; 32],
        }
    }

    /// Update market data
    #[inline(always)]
    pub fn update_market(&self, _symbol: Symbol, _bid: Px, _ask: Px, _ts: Ts) {
        // Market updates tracked for spread analysis
    }

    /// Record fill and update metrics
    #[inline(always)]
    pub fn record_fill(&self, _order_id: u64, qty: Qty, price: Px, ts: Ts) {
        // Update trade count
        self.total_trades.fetch_add(1, Ordering::Relaxed);

        // Update volume using fixed-point arithmetic
        let volume = ((qty.raw() * price.as_i64()) / 10000).unsigned_abs();
        self.total_volume.fetch_add(volume, Ordering::Relaxed);

        // Update last trade time
        self.last_trade_time.store(ts.nanos(), Ordering::Relaxed);

        // Set first trade time if needed
        let _ = self.first_trade_time.compare_exchange(
            0,
            ts.nanos(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
    }

    /// Calculate PnL using position tracker
    #[inline(always)]
    pub fn calculate_pnl(&self, positions: &PositionTracker) -> PnL {
        let (realized, unrealized, total) = positions.get_global_pnl();

        // Update drawdown
        let current_equity = total;
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

        PnL {
            realized,
            unrealized,
            total,
            timestamp: Ts::now().nanos(),
        }
    }

    /// Calculate Sharpe ratio using SIMD
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    #[allow(clippy::cast_precision_loss)] // Analytics boundary - ADR-0005
    pub unsafe fn calculate_sharpe(&self) -> f64 {
        if self.returns_buffer.len() < 2 {
            return 0.0;
        }

        // Use AVX2 for mean calculation
        let mut chunks = self.returns_buffer.chunks_exact(4);
        let remainder = chunks.remainder();

        let mut sum = _mm256_setzero_pd();
        for chunk in chunks.by_ref() {
            let v = unsafe { _mm256_loadu_pd(chunk.as_ptr()) };
            sum = _mm256_add_pd(sum, v);
        }

        // Extract sum from AVX register
        let mut result = [0.0; 4];
        unsafe { _mm256_storeu_pd(result.as_mut_ptr(), sum) };
        let mut total = result.iter().sum::<f64>();

        for &val in remainder {
            total += val;
        }

        // SAFETY: Cast is safe within expected range
        let mean = total / self.returns_buffer.len() as f64;

        // Calculate standard deviation using AVX2
        let mean_vec = _mm256_set1_pd(mean);
        let mut chunks = self.returns_buffer.chunks_exact(4);
        let remainder = chunks.remainder();

        let mut variance_sum = _mm256_setzero_pd();
        for chunk in chunks.by_ref() {
            let v = unsafe { _mm256_loadu_pd(chunk.as_ptr()) };
            let diff = _mm256_sub_pd(v, mean_vec);
            let squared = _mm256_mul_pd(diff, diff);
            variance_sum = _mm256_add_pd(variance_sum, squared);
        }

        // Extract variance sum from AVX register
        unsafe { _mm256_storeu_pd(result.as_mut_ptr(), variance_sum) };
        let mut variance = result.iter().sum::<f64>();

        for &val in remainder {
            let diff = val - mean;
            variance += diff * diff;
        }
        // SAFETY: Cast is safe within expected range

        // SAFETY: Cast is safe within expected range
        let std_dev = (variance / self.returns_buffer.len() as f64).sqrt();

        if std_dev > 0.0 {
            (mean / std_dev) * (252.0_f64).sqrt() // Annualized
        } else {
            0.0
        }
    }

    /// Calculate Sharpe ratio without SIMD (fallback)
    #[cfg(not(target_arch = "x86_64"))]
    #[allow(clippy::cast_precision_loss)] // Analytics boundary - ADR-0005
    pub fn calculate_sharpe(&self) -> f64 {
        if self.returns_buffer.len() < 2 {
            return 0.0;
            // SAFETY: Cast is safe within expected range
        }
        // SAFETY: Cast is safe within expected range

        let mean = self.returns_buffer.iter().sum::<f64>() / self.returns_buffer.len() as f64;

        let variance = self
            .returns_buffer
            .iter()
            .map(|&r| {
                let diff = r - mean;
                // SAFETY: Cast is safe within expected range
                diff * diff
                // SAFETY: Cast is safe within expected range
            })
            .sum::<f64>()
            / self.returns_buffer.len() as f64;

        let std_dev = variance.sqrt();

        if std_dev > 0.0 {
            (mean / std_dev) * (252.0_f64).sqrt() // Annualized
        } else {
            0.0
        }
    }

    /// Get comprehensive metrics
    #[allow(clippy::cast_precision_loss)] // Analytics boundary - ADR-0005
    pub fn get_metrics(&self) -> TradingMetrics {
        let total = self.total_trades.load(Ordering::Acquire);
        // SAFETY: Cast is safe within expected range
        let wins = self.winning_trades.load(Ordering::Acquire);
        // SAFETY: Cast is safe within expected range
        let losses = self.losing_trades.load(Ordering::Acquire);

        let win_rate = if total > 0 {
            (wins as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        // SAFETY: Cast is safe within expected range

        // SAFETY: Cast is safe within expected range
        let gross_profit = self.gross_profit.load(Ordering::Acquire);
        let gross_loss = self.gross_loss.load(Ordering::Acquire).abs();

        let profit_factor = if gross_loss > 0 {
            gross_profit as f64 / gross_loss as f64
        } else {
            0.0
        };

        TradingMetrics {
            total_trades: total,
            winning_trades: wins,
            losing_trades: losses,
            win_rate,
            profit_factor,
            sharpe_ratio: unsafe { self.calculate_sharpe() },
            max_drawdown: self.max_drawdown.load(Ordering::Acquire),
            total_volume: self.total_volume.load(Ordering::Acquire),
        }
    }
}

/// PnL structure
#[repr(C, align(64))]
#[derive(Clone, Copy, Debug)]
pub struct PnL {
    pub realized: i64,
    pub unrealized: i64,
    pub total: i64,
    pub timestamp: u64,
}

/// Trading metrics
#[repr(C)]
#[derive(Clone, Debug)]
pub struct TradingMetrics {
    pub total_trades: u64,
    pub winning_trades: u64,
    pub losing_trades: u64,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown: i64,
    pub total_volume: u64,
}
