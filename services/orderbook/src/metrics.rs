//! Performance metrics and latency tracking for orderbook operations
//!
//! This module provides ultra-low-latency metrics collection specifically
//! designed for orderbook operations with nanosecond precision.

use services_common::{Qty, Ts};
use std::sync::atomic::{AtomicU64, AtomicI64, Ordering};
use parking_lot::RwLock;
use hdrhistogram::Histogram;

/// Performance metrics for orderbook operations
#[repr(align(64))] // Cache-line aligned
pub struct PerformanceMetrics {
    /// Symbol being tracked
    symbol: String,
    
    /// Operation counters
    orders_added: AtomicU64,
    orders_modified: AtomicU64,
    orders_canceled: AtomicU64,
    trades_executed: AtomicU64,
    #[allow(dead_code)]
    snapshots_processed: AtomicU64,
    
    /// Volume metrics
    total_volume_added: AtomicI64,
    total_volume_canceled: AtomicI64,
    total_volume_traded: AtomicI64,
    
    /// Depth metrics
    max_bid_levels: AtomicU64,
    max_ask_levels: AtomicU64,
    avg_bid_depth: AtomicI64,
    avg_ask_depth: AtomicI64,
    
    /// Spread metrics
    min_spread: AtomicI64,
    max_spread: AtomicI64,
    avg_spread: AtomicI64,
    spread_samples: AtomicU64,
    
    /// Latency tracking
    latency_tracker: LatencyTracker,
    
    /// Update frequency
    updates_per_second: AtomicU64,
    last_update_time: AtomicU64,
    update_counter: AtomicU64,
    
    /// Checksum validation
    checksum_matches: AtomicU64,
    checksum_mismatches: AtomicU64,
}

impl PerformanceMetrics {
    /// Create new performance metrics tracker
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            orders_added: AtomicU64::new(0),
            orders_modified: AtomicU64::new(0),
            orders_canceled: AtomicU64::new(0),
            trades_executed: AtomicU64::new(0),
            snapshots_processed: AtomicU64::new(0),
            total_volume_added: AtomicI64::new(0),
            total_volume_canceled: AtomicI64::new(0),
            total_volume_traded: AtomicI64::new(0),
            max_bid_levels: AtomicU64::new(0),
            max_ask_levels: AtomicU64::new(0),
            avg_bid_depth: AtomicI64::new(0),
            avg_ask_depth: AtomicI64::new(0),
            min_spread: AtomicI64::new(i64::MAX),
            max_spread: AtomicI64::new(0),
            avg_spread: AtomicI64::new(0),
            spread_samples: AtomicU64::new(0),
            latency_tracker: LatencyTracker::new(),
            updates_per_second: AtomicU64::new(0),
            last_update_time: AtomicU64::new(0),
            update_counter: AtomicU64::new(0),
            checksum_matches: AtomicU64::new(0),
            checksum_mismatches: AtomicU64::new(0),
        }
    }

    /// Record an order add operation
    #[inline]
    pub fn record_order_add(&self, quantity: Qty, latency_ns: u64) {
        self.orders_added.fetch_add(1, Ordering::Relaxed);
        self.total_volume_added.fetch_add(quantity.as_i64(), Ordering::Relaxed);
        self.latency_tracker.record_operation(OperationType::OrderAdd, latency_ns);
        self.update_frequency();
    }

    /// Record an order modify operation
    #[inline]
    pub fn record_order_modify(&self, latency_ns: u64) {
        self.orders_modified.fetch_add(1, Ordering::Relaxed);
        self.latency_tracker.record_operation(OperationType::OrderModify, latency_ns);
        self.update_frequency();
    }

    /// Record an order cancel operation
    #[inline]
    pub fn record_order_cancel(&self, quantity: Qty, latency_ns: u64) {
        self.orders_canceled.fetch_add(1, Ordering::Relaxed);
        self.total_volume_canceled.fetch_add(quantity.as_i64(), Ordering::Relaxed);
        self.latency_tracker.record_operation(OperationType::OrderCancel, latency_ns);
        self.update_frequency();
    }

    /// Record a trade execution
    #[inline]
    pub fn record_trade(&self, quantity: Qty, latency_ns: u64) {
        self.trades_executed.fetch_add(1, Ordering::Relaxed);
        self.total_volume_traded.fetch_add(quantity.as_i64(), Ordering::Relaxed);
        self.latency_tracker.record_operation(OperationType::Trade, latency_ns);
        self.update_frequency();
    }

    /// Record spread metrics
    #[inline]
    pub fn record_spread(&self, spread: i64) {
        // Update min spread
        let mut current_min = self.min_spread.load(Ordering::Acquire);
        while spread < current_min {
            match self.min_spread.compare_exchange_weak(
                current_min,
                spread,
                Ordering::Release,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(x) => current_min = x,
            }
        }

        // Update max spread
        let mut current_max = self.max_spread.load(Ordering::Acquire);
        while spread > current_max {
            match self.max_spread.compare_exchange_weak(
                current_max,
                spread,
                Ordering::Release,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(x) => current_max = x,
            }
        }

        // Update average spread
        let samples = self.spread_samples.fetch_add(1, Ordering::AcqRel) + 1;
        let current_avg = self.avg_spread.load(Ordering::Acquire);
        let new_avg = current_avg + (spread - current_avg) / samples as i64;
        self.avg_spread.store(new_avg, Ordering::Release);
    }

    /// Record orderbook depth
    #[inline]
    pub fn record_depth(&self, bid_levels: u64, ask_levels: u64, bid_volume: i64, ask_volume: i64) {
        // Update max levels
        let mut current_max = self.max_bid_levels.load(Ordering::Acquire);
        while bid_levels > current_max {
            match self.max_bid_levels.compare_exchange_weak(
                current_max,
                bid_levels,
                Ordering::Release,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(x) => current_max = x,
            }
        }

        current_max = self.max_ask_levels.load(Ordering::Acquire);
        while ask_levels > current_max {
            match self.max_ask_levels.compare_exchange_weak(
                current_max,
                ask_levels,
                Ordering::Release,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(x) => current_max = x,
            }
        }

        // Update average depth (exponential moving average)
        let alpha = 100; // Weight for new value (out of 1000)
        let current_bid = self.avg_bid_depth.load(Ordering::Acquire);
        let new_bid = (current_bid * (1000 - alpha) + bid_volume * alpha) / 1000;
        self.avg_bid_depth.store(new_bid, Ordering::Release);

        let current_ask = self.avg_ask_depth.load(Ordering::Acquire);
        let new_ask = (current_ask * (1000 - alpha) + ask_volume * alpha) / 1000;
        self.avg_ask_depth.store(new_ask, Ordering::Release);
    }

    /// Record checksum validation result
    #[inline]
    pub fn record_checksum(&self, matched: bool) {
        if matched {
            self.checksum_matches.fetch_add(1, Ordering::Relaxed);
        } else {
            self.checksum_mismatches.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Update frequency counter
    #[inline]
    fn update_frequency(&self) {
        let now = Ts::now().as_nanos();
        let last = self.last_update_time.load(Ordering::Acquire);
        
        // Reset counter every second
        if now - last > 1_000_000_000 {
            let count = self.update_counter.swap(1, Ordering::AcqRel);
            self.updates_per_second.store(count, Ordering::Release);
            self.last_update_time.store(now, Ordering::Release);
        } else {
            self.update_counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get current metrics snapshot
    pub fn get_snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            symbol: self.symbol.clone(),
            orders_added: self.orders_added.load(Ordering::Acquire),
            orders_modified: self.orders_modified.load(Ordering::Acquire),
            orders_canceled: self.orders_canceled.load(Ordering::Acquire),
            trades_executed: self.trades_executed.load(Ordering::Acquire),
            total_volume_added: self.total_volume_added.load(Ordering::Acquire),
            total_volume_canceled: self.total_volume_canceled.load(Ordering::Acquire),
            total_volume_traded: self.total_volume_traded.load(Ordering::Acquire),
            max_bid_levels: self.max_bid_levels.load(Ordering::Acquire),
            max_ask_levels: self.max_ask_levels.load(Ordering::Acquire),
            avg_bid_depth: self.avg_bid_depth.load(Ordering::Acquire),
            avg_ask_depth: self.avg_ask_depth.load(Ordering::Acquire),
            min_spread: self.min_spread.load(Ordering::Acquire),
            max_spread: self.max_spread.load(Ordering::Acquire),
            avg_spread: self.avg_spread.load(Ordering::Acquire),
            updates_per_second: self.updates_per_second.load(Ordering::Acquire),
            checksum_matches: self.checksum_matches.load(Ordering::Acquire),
            checksum_mismatches: self.checksum_mismatches.load(Ordering::Acquire),
            latency_stats: self.latency_tracker.get_stats(),
        }
    }
}

/// Types of orderbook operations for latency tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum OperationType {
    /// Order add operation
    OrderAdd = 0,
    /// Order modify operation
    OrderModify = 1,
    /// Order cancel operation
    OrderCancel = 2,
    /// Trade execution
    Trade = 3,
    /// Snapshot processing
    Snapshot = 4,
    /// Checksum validation
    Checksum = 5,
    /// Replay operation
    Replay = 6,
}

/// Latency tracker using HDR histogram for accurate percentiles
pub struct LatencyTracker {
    /// Histograms for each operation type
    histograms: RwLock<[Histogram<u64>; 7]>,
    /// Total samples per operation
    sample_counts: [AtomicU64; 7],
}

impl Default for LatencyTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl LatencyTracker {
    /// Create new latency tracker
    #[must_use] pub fn new() -> Self {
        // Create histograms with safe fallback
        // Precision 3 gives us microsecond accuracy up to ~1 hour
        let create_histogram = || {
            Histogram::new(3).unwrap_or_else(|_| {
                // Fallback to precision 2 if 3 fails (unlikely but safe)
                Histogram::new(2).unwrap_or_else(|_| {
                    // Last resort: precision 1 (this should never fail)
                    Histogram::new(1).unwrap_or_default()
                })
            })
        };
        
        let histograms = [
            create_histogram(),
            create_histogram(),
            create_histogram(),
            create_histogram(),
            create_histogram(),
            create_histogram(),
            create_histogram(),
        ];
        
        Self {
            histograms: RwLock::new(histograms),
            sample_counts: Default::default(),
        }
    }

    /// Record a latency measurement
    #[inline]
    pub fn record_operation(&self, op_type: OperationType, latency_ns: u64) {
        let index = op_type as usize;
        self.sample_counts[index].fetch_add(1, Ordering::Relaxed);
        
        let mut histograms = self.histograms.write();
        let _ = histograms[index].record(latency_ns);
    }

    /// Get latency statistics
    pub fn get_stats(&self) -> LatencyStats {
        let histograms = self.histograms.read();
        
        let mut stats = LatencyStats::default();
        
        for (i, hist) in histograms.iter().enumerate() {
            if !hist.is_empty() {
                let op_stats = OperationLatency {
                    count: self.sample_counts[i].load(Ordering::Acquire),
                    min: hist.min(),
                    max: hist.max(),
                    mean: hist.mean() as u64,
                    p50: hist.value_at_percentile(50.0),
                    p90: hist.value_at_percentile(90.0),
                    p95: hist.value_at_percentile(95.0),
                    p99: hist.value_at_percentile(99.0),
                    p999: hist.value_at_percentile(99.9),
                };
                
                match i {
                    0 => stats.order_add = Some(op_stats),
                    1 => stats.order_modify = Some(op_stats),
                    2 => stats.order_cancel = Some(op_stats),
                    3 => stats.trade = Some(op_stats),
                    4 => stats.snapshot = Some(op_stats),
                    5 => stats.checksum = Some(op_stats),
                    6 => stats.replay = Some(op_stats),
                    _ => {}
                }
            }
        }
        
        stats
    }

    /// Reset all histograms
    pub fn reset(&self) {
        let mut histograms = self.histograms.write();
        for hist in histograms.iter_mut() {
            hist.reset();
        }
        for counter in &self.sample_counts {
            counter.store(0, Ordering::Release);
        }
    }
}

/// Latency statistics for an operation type
#[derive(Debug, Clone, Default)]
pub struct OperationLatency {
    /// Total number of operations measured
    pub count: u64,
    /// Minimum latency observed in nanoseconds
    pub min: u64,
    /// Maximum latency observed in nanoseconds
    pub max: u64,
    /// Mean latency in nanoseconds
    pub mean: u64,
    /// 50th percentile latency in nanoseconds
    pub p50: u64,
    /// 90th percentile latency in nanoseconds
    pub p90: u64,
    /// 95th percentile latency in nanoseconds
    pub p95: u64,
    /// 99th percentile latency in nanoseconds
    pub p99: u64,
    /// 99.9th percentile latency in nanoseconds
    pub p999: u64,
}

/// Complete latency statistics
#[derive(Debug, Clone, Default)]
pub struct LatencyStats {
    /// Latency statistics for order add operations
    pub order_add: Option<OperationLatency>,
    /// Latency statistics for order modify operations
    pub order_modify: Option<OperationLatency>,
    /// Latency statistics for order cancel operations
    pub order_cancel: Option<OperationLatency>,
    /// Latency statistics for trade operations
    pub trade: Option<OperationLatency>,
    /// Latency statistics for snapshot operations
    pub snapshot: Option<OperationLatency>,
    /// Latency statistics for checksum operations
    pub checksum: Option<OperationLatency>,
    /// Latency statistics for replay operations
    pub replay: Option<OperationLatency>,
}

/// Snapshot of all metrics
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub symbol: String,
    pub orders_added: u64,
    pub orders_modified: u64,
    pub orders_canceled: u64,
    pub trades_executed: u64,
    pub total_volume_added: i64,
    pub total_volume_canceled: i64,
    pub total_volume_traded: i64,
    pub max_bid_levels: u64,
    pub max_ask_levels: u64,
    pub avg_bid_depth: i64,
    pub avg_ask_depth: i64,
    pub min_spread: i64,
    pub max_spread: i64,
    pub avg_spread: i64,
    pub updates_per_second: u64,
    pub checksum_matches: u64,
    pub checksum_mismatches: u64,
    pub latency_stats: LatencyStats,
}

impl MetricsSnapshot {
    /// Format metrics as a report
    #[must_use] pub fn format_report(&self) -> String {
        let mut report = String::new();
        report.push_str(&format!("=== Orderbook Metrics: {} ===\n", self.symbol));
        report.push_str(&format!("Orders: {} added, {} modified, {} canceled\n", 
            self.orders_added, self.orders_modified, self.orders_canceled));
        report.push_str(&format!("Trades: {} executed\n", self.trades_executed));
        report.push_str(&format!("Volume: {} added, {} canceled, {} traded\n",
            self.total_volume_added, self.total_volume_canceled, self.total_volume_traded));
        report.push_str(&format!("Depth: max {} bid / {} ask levels\n",
            self.max_bid_levels, self.max_ask_levels));
        report.push_str(&format!("Spread: min {}, max {}, avg {}\n",
            self.min_spread, self.max_spread, self.avg_spread));
        report.push_str(&format!("Updates: {} per second\n", self.updates_per_second));
        report.push_str(&format!("Checksums: {} matches, {} mismatches\n",
            self.checksum_matches, self.checksum_mismatches));
        
        // Add latency stats
        if let Some(ref add) = self.latency_stats.order_add {
            report.push_str("\nOrder Add Latency (ns):\n");
            report.push_str(&format!("  p50: {}, p90: {}, p99: {}, p99.9: {}\n",
                add.p50, add.p90, add.p99, add.p999));
        }
        
        if let Some(ref trade) = self.latency_stats.trade {
            report.push_str("\nTrade Latency (ns):\n");
            report.push_str(&format!("  p50: {}, p90: {}, p99: {}, p99.9: {}\n",
                trade.p50, trade.p90, trade.p99, trade.p999));
        }
        
        report
    }
}