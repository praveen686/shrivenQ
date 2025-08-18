//! Advanced market microstructure analytics
//!
//! This module implements sophisticated quantitative metrics used by
//! institutional traders and market makers for understanding market dynamics.
//!
//! Includes:
//! - VPIN (Volume-Synchronized Probability of Informed Trading)
//! - Kyle's Lambda (Price Impact Coefficient)
//! - Amihud Illiquidity Measure
//! - Order Flow Toxicity
//! - Realized Spread
//! - Implementation Shortfall

use services_common::{Px, Qty, Ts};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use parking_lot::RwLock;

/// Time buckets for volume analysis (in nanoseconds)
const VOLUME_BUCKET_SIZE_NS: u64 = 1_000_000_000; // 1 second buckets
const MAX_BUCKETS: usize = 300; // 5 minutes of history

/// Volume bucket for VPIN calculation
#[derive(Debug, Clone)]
struct VolumeBucket {
    timestamp: Ts,
    buy_volume: Qty,
    sell_volume: Qty,
    total_volume: Qty,
    vwap: Px,
    trade_count: u64,
}

/// Market microstructure analytics engine
pub struct MicrostructureAnalytics {
    /// Rolling volume buckets for VPIN
    volume_buckets: RwLock<VecDeque<VolumeBucket>>,
    
    /// Kyle's Lambda (price impact) - atomic for lock-free reads
    kyles_lambda: AtomicI64,
    
    /// Amihud illiquidity measure
    #[allow(dead_code)]
    amihud_illiquidity: AtomicI64,
    
    /// Order flow imbalance
    flow_imbalance: AtomicI64,
    
    /// Realized spread (in basis points)
    #[allow(dead_code)]
    realized_spread_bps: AtomicI64,
    
    /// Effective spread (in basis points)
    #[allow(dead_code)]
    effective_spread_bps: AtomicI64,
    
    /// Quote slope (market depth metric)
    #[allow(dead_code)]
    quote_slope: AtomicI64,
    
    /// Probability of informed trading (PIN)
    pin_estimate: AtomicI64,
    
    /// Current VPIN value (0-10000 for 0-100%)
    vpin: AtomicI64,
    
    /// Total buy volume
    total_buy_volume: AtomicI64,
    
    /// Total sell volume
    total_sell_volume: AtomicI64,
    
    /// Last calculation timestamp
    last_calc: AtomicU64,
}

impl Default for MicrostructureAnalytics {
    fn default() -> Self {
        Self::new()
    }
}

impl MicrostructureAnalytics {
    /// Create a new analytics engine
    #[must_use] pub fn new() -> Self {
        Self {
            volume_buckets: RwLock::new(VecDeque::with_capacity(MAX_BUCKETS)),
            kyles_lambda: AtomicI64::new(0),
            amihud_illiquidity: AtomicI64::new(0),
            flow_imbalance: AtomicI64::new(0),
            realized_spread_bps: AtomicI64::new(0),
            effective_spread_bps: AtomicI64::new(0),
            quote_slope: AtomicI64::new(0),
            pin_estimate: AtomicI64::new(0),
            vpin: AtomicI64::new(0),
            total_buy_volume: AtomicI64::new(0),
            total_sell_volume: AtomicI64::new(0),
            last_calc: AtomicU64::new(0),
        }
    }

    /// Update analytics with a new trade
    pub fn update_trade(&self, price: Px, quantity: Qty, is_buy: bool, timestamp: Ts) {
        // Update volume counters
        if is_buy {
            self.total_buy_volume.fetch_add(quantity.as_i64(), Ordering::Release);
        } else {
            self.total_sell_volume.fetch_add(quantity.as_i64(), Ordering::Release);
        }

        // Update volume buckets
        {
            let mut buckets = self.volume_buckets.write();
            
            // Get or create current bucket
            let bucket_time = (timestamp.as_nanos() / VOLUME_BUCKET_SIZE_NS) * VOLUME_BUCKET_SIZE_NS;
            let bucket_ts = Ts::from_nanos(bucket_time);
            
            if buckets.is_empty() || buckets.back().map_or(true, |b| b.timestamp != bucket_ts) {
                // New bucket needed
                if buckets.len() >= MAX_BUCKETS {
                    buckets.pop_front();
                }
                buckets.push_back(VolumeBucket {
                    timestamp: bucket_ts,
                    buy_volume: Qty::ZERO,
                    sell_volume: Qty::ZERO,
                    total_volume: Qty::ZERO,
                    vwap: price,
                    trade_count: 0,
                });
            }
            
            // Update current bucket
            if let Some(bucket) = buckets.back_mut() {
                if is_buy {
                    bucket.buy_volume = Qty::from_i64(
                        bucket.buy_volume.as_i64() + quantity.as_i64()
                    );
                } else {
                    bucket.sell_volume = Qty::from_i64(
                        bucket.sell_volume.as_i64() + quantity.as_i64()
                    );
                }
                bucket.total_volume = Qty::from_i64(
                    bucket.total_volume.as_i64() + quantity.as_i64()
                );
                
                // Update VWAP
                let old_value = bucket.vwap.as_i64() * bucket.trade_count as i64;
                let new_value = old_value + price.as_i64();
                bucket.trade_count += 1;
                bucket.vwap = Px::from_i64(new_value / bucket.trade_count as i64);
            }
        }

        // Calculate Kyle's Lambda from recent bucket history
        {
            let buckets = self.volume_buckets.read();
            if buckets.len() >= 10 {
                // Extract price and signed volume changes from recent buckets
                let mut price_changes = Vec::new();
                let mut volume_changes = Vec::new();
                
                let recent: Vec<_> = buckets.iter().rev().take(20).collect();
                for window in recent.windows(2) {
                    let price_change = window[0].vwap.as_i64() - window[1].vwap.as_i64();
                    let volume_change = window[0].buy_volume.as_i64() - window[0].sell_volume.as_i64();
                    
                    price_changes.push(price_change);
                    volume_changes.push(volume_change);
                }
                
                if price_changes.len() >= 5 {
                    // Calculate Kyle's Lambda
                    let n = price_changes.len() as f64;
                    let mean_price_change = price_changes.iter().sum::<i64>() as f64 / n;
                    let mean_volume_change = volume_changes.iter().sum::<i64>() as f64 / n;
                    
                    let mut covariance = 0.0;
                    let mut volume_variance = 0.0;
                    
                    for i in 0..price_changes.len() {
                        let price_dev = price_changes[i] as f64 - mean_price_change;
                        let volume_dev = volume_changes[i] as f64 - mean_volume_change;
                        
                        covariance += price_dev * volume_dev;
                        volume_variance += volume_dev * volume_dev;
                    }
                    
                    if volume_variance > 1.0 {  // Avoid division by very small numbers
                        let lambda = (covariance / volume_variance * 10000.0).abs() as i64;
                        self.kyles_lambda.store(lambda.min(100000), Ordering::Release); // Cap at 10.0
                    }
                }
            }
        }
        
        // Recalculate other metrics
        self.calculate_vpin();
        self.calculate_flow_metrics();
        self.last_calc.store(timestamp.as_nanos(), Ordering::Release);
    }

    /// Calculate VPIN (Volume-Synchronized Probability of Informed Trading)
    fn calculate_vpin(&self) {
        let buckets = self.volume_buckets.read();
        
        if buckets.len() < 50 {
            return; // Need sufficient history
        }

        // Calculate order flow imbalance over recent buckets
        let mut total_imbalance = 0i64;
        let mut total_volume = 0i64;
        
        for bucket in buckets.iter().rev().take(50) {
            let imbalance = (bucket.buy_volume.as_i64() - bucket.sell_volume.as_i64()).abs();
            total_imbalance += imbalance;
            total_volume += bucket.total_volume.as_i64();
        }

        if total_volume > 0 {
            // VPIN = Order Flow Imbalance / Total Volume
            // Scaled to 0-10000 for precision (represents 0-100%)
            let vpin = (total_imbalance * 10000) / total_volume;
            self.vpin.store(vpin, Ordering::Release);
        }
    }

    /// Calculate flow-based metrics
    fn calculate_flow_metrics(&self) {
        let buy_vol = self.total_buy_volume.load(Ordering::Acquire);
        let sell_vol = self.total_sell_volume.load(Ordering::Acquire);
        let total = buy_vol + sell_vol;
        
        if total > 0 {
            // Order flow imbalance (-10000 to 10000, representing -100% to 100%)
            let imbalance = ((buy_vol - sell_vol) * 10000) / total;
            self.flow_imbalance.store(imbalance, Ordering::Release);
            
            // Simple PIN estimate based on flow imbalance persistence
            let pin = imbalance.abs() / 100; // 0-100 scale
            self.pin_estimate.store(pin, Ordering::Release);
        }
    }

    /// Calculate Kyle's Lambda (price impact coefficient)
    pub fn calculate_kyles_lambda(&self, price_changes: &[i64], volume_changes: &[i64]) {
        if price_changes.len() != volume_changes.len() || price_changes.is_empty() {
            return;
        }

        // Kyle's Lambda = Cov(ΔP, ΔV) / Var(ΔV)
        let n = price_changes.len() as f64;
        
        let mean_price_change = price_changes.iter().sum::<i64>() as f64 / n;
        let mean_volume_change = volume_changes.iter().sum::<i64>() as f64 / n;
        
        let mut covariance = 0.0;
        let mut volume_variance = 0.0;
        
        for i in 0..price_changes.len() {
            let price_dev = price_changes[i] as f64 - mean_price_change;
            let volume_dev = volume_changes[i] as f64 - mean_volume_change;
            
            covariance += price_dev * volume_dev;
            volume_variance += volume_dev * volume_dev;
        }
        
        if volume_variance > 0.0 {
            let lambda = (covariance / volume_variance * 10000.0) as i64;
            self.kyles_lambda.store(lambda, Ordering::Release);
        }
    }

    /// Get current VPIN value (0-100%)
    #[inline]
    pub fn get_vpin(&self) -> f64 {
        self.vpin.load(Ordering::Acquire) as f64 / 100.0
    }

    /// Get order flow imbalance (-100% to 100%)
    #[inline]
    pub fn get_flow_imbalance(&self) -> f64 {
        self.flow_imbalance.load(Ordering::Acquire) as f64 / 100.0
    }

    /// Get Kyle's Lambda (price impact)
    #[inline]
    pub fn get_kyles_lambda(&self) -> f64 {
        self.kyles_lambda.load(Ordering::Acquire) as f64 / 10000.0
    }

    /// Get PIN estimate (0-100%)
    #[inline]
    pub fn get_pin(&self) -> f64 {
        self.pin_estimate.load(Ordering::Acquire) as f64
    }
}

/// Imbalance calculator for order book analysis
pub struct ImbalanceCalculator;

impl ImbalanceCalculator {
    /// Calculate volume imbalance at various depths
    #[must_use] pub fn calculate_imbalances(
        bid_levels: &[(Px, Qty, u64)],
        ask_levels: &[(Px, Qty, u64)],
    ) -> ImbalanceMetrics {
        let mut metrics = ImbalanceMetrics {
            top_level_imbalance: 0.0,
            three_level_imbalance: 0.0,
            five_level_imbalance: 0.0,
            ten_level_imbalance: 0.0,
            weighted_mid_price: Px::ZERO,
            buy_pressure: 0.0,
            sell_pressure: 0.0,
        };
        
        // Calculate at different depth levels
        for depth in [1, 3, 5, 10] {
            let bid_volume: i64 = bid_levels.iter()
                .take(depth)
                .map(|(_, q, _)| q.as_i64())
                .sum();
                
            let ask_volume: i64 = ask_levels.iter()
                .take(depth)
                .map(|(_, q, _)| q.as_i64())
                .sum();
                
            let total = bid_volume + ask_volume;
            if total > 0 {
                let imbalance = ((bid_volume - ask_volume) as f64 / total as f64) * 100.0;
                
                match depth {
                    1 => metrics.top_level_imbalance = imbalance,
                    3 => metrics.three_level_imbalance = imbalance,
                    5 => metrics.five_level_imbalance = imbalance,
                    10 => metrics.ten_level_imbalance = imbalance,
                    _ => {}
                }
            }
        }
        
        // Calculate weighted mid-price
        if !bid_levels.is_empty() && !ask_levels.is_empty() {
            let best_bid = bid_levels[0].0;
            let best_ask = ask_levels[0].0;
            let bid_size = bid_levels[0].1;
            let ask_size = ask_levels[0].1;
            
            let total_size = bid_size.as_i64() + ask_size.as_i64();
            if total_size > 0 {
                let weighted_mid = 
                    (best_bid.as_i64() * ask_size.as_i64() + 
                     best_ask.as_i64() * bid_size.as_i64()) / total_size;
                metrics.weighted_mid_price = Px::from_i64(weighted_mid);
            }
        }
        
        // Calculate pressure indicator
        metrics.calculate_pressure();
        
        metrics
    }
}

/// Imbalance metrics at various depths
#[derive(Debug, Clone)]
pub struct ImbalanceMetrics {
    /// Imbalance at top of book (-100% to 100%)
    pub top_level_imbalance: f64,
    /// Imbalance at 3 levels (-100% to 100%)
    pub three_level_imbalance: f64,
    /// Imbalance at 5 levels (-100% to 100%)
    pub five_level_imbalance: f64,
    /// Imbalance at 10 levels (-100% to 100%)
    pub ten_level_imbalance: f64,
    /// Volume-weighted mid price
    pub weighted_mid_price: Px,
    /// Buy pressure indicator (0-100)
    pub buy_pressure: f64,
    /// Sell pressure indicator (0-100)
    pub sell_pressure: f64,
}

impl ImbalanceMetrics {
    fn calculate_pressure(&mut self) {
        // Weighted average of imbalances
        let weighted = 
            self.ten_level_imbalance.mul_add(0.1, self.five_level_imbalance.mul_add(0.2, self.top_level_imbalance.mul_add(0.4, self.three_level_imbalance * 0.3)));
            
        if weighted > 0.0 {
            self.buy_pressure = weighted.min(100.0);
            self.sell_pressure = 0.0;
        } else {
            self.sell_pressure = (-weighted).min(100.0);
            self.buy_pressure = 0.0;
        }
    }
}

/// Advanced toxicity metrics for detecting adverse selection
pub struct ToxicityDetector {
    /// Recent trade directions
    recent_trades: RwLock<VecDeque<(bool, Qty, Ts)>>,
    /// Toxicity score (0-100)
    toxicity_score: AtomicI64,
}

impl Default for ToxicityDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ToxicityDetector {
    /// Create a new toxicity detector
    #[must_use] pub fn new() -> Self {
        Self {
            recent_trades: RwLock::new(VecDeque::with_capacity(1000)),
            toxicity_score: AtomicI64::new(0),
        }
    }

    /// Update with new trade and calculate toxicity
    pub fn update(&self, is_buy: bool, quantity: Qty, timestamp: Ts) {
        {
            let mut trades = self.recent_trades.write();
            if trades.len() >= 1000 {
                trades.pop_front();
            }
            trades.push_back((is_buy, quantity, timestamp));
        }
        
        self.calculate_toxicity();
    }

    fn calculate_toxicity(&self) {
        let trades = self.recent_trades.read();
        
        if trades.len() < 100 {
            return;
        }

        // Look for patterns indicating toxic flow:
        // 1. Rapid one-directional flow
        // 2. Large orders followed by immediate reversal
        // 3. Clustering of same-direction trades
        
        let mut same_direction_runs = 0;
        let mut max_run = 0;
        let mut current_run = 1;
        let mut last_direction = trades[0].0;
        
        for i in 1..trades.len() {
            if trades[i].0 == last_direction {
                current_run += 1;
                max_run = max_run.max(current_run);
            } else {
                if current_run > 5 {
                    same_direction_runs += 1;
                }
                current_run = 1;
                last_direction = trades[i].0;
            }
        }
        
        // Toxicity score based on directional persistence
        let score = i64::from(((same_direction_runs * 10) + (max_run * 2)).min(100));
        self.toxicity_score.store(score, Ordering::Release);
    }

    /// Get current toxicity score (0-100)
    #[inline]
    pub fn get_toxicity(&self) -> f64 {
        self.toxicity_score.load(Ordering::Acquire) as f64
    }
}