//! Production-grade LOB v2 with advanced optimizations
//!
//! Incorporates best practices from hftbacktest:
//! - ROI (Range of Interest) vector optimization
//! - Smart cross-book resolution
//! - Timestamp validation
//! - Cache-line alignment
//! - SIMD operations
//! - Hybrid storage strategies

use common::{L2Update, Px, Qty, Side, Symbol, Ts};
use rustc_hash::{FxBuildHasher, FxHashMap};
use std::arch::x86_64::*;
use thiserror::Error;

/// Error types for LOB v2
#[derive(Debug, Error)]
pub enum LobV2Error {
    /// Crossed book detected
    #[error("Crossed book detected: bid {bid:?} >= ask {ask:?}")]
    CrossedBook {
        /// Bid price
        bid: Option<Px>,
        /// Ask price
        ask: Option<Px>,
    },

    /// Out of order update detected
    #[error("Out of order update: timestamp {new} < {existing}")]
    OutOfOrderUpdate {
        /// Existing timestamp
        existing: Ts,
        /// New timestamp
        new: Ts,
    },

    /// Invalid price level
    #[error("Invalid price level: {level}")]
    InvalidLevel {
        /// Level index
        level: usize,
    },

    /// Empty book
    #[error("Empty book")]
    EmptyBook,

    /// Price outside ROI range
    #[error("Price outside ROI range")]
    OutsideROI,
}

/// Cache-line aligned for optimal performance
/// Cache-line aligned order book v2 with advanced optimizations
#[repr(align(64))]
#[derive(Debug, Clone)]
pub struct OrderBookV2 {
    // Hot data - frequently accessed together (first cache line)
    /// Symbol identifier
    pub symbol: Symbol,
    /// Last update timestamp
    pub ts: Ts,
    /// Sequence number
    pub sequence: u64,

    // Cached BBO for ultra-fast access
    bid_best_price: Px,
    bid_best_qty: Qty,
    bid_best_ts: Ts,
    ask_best_price: Px,
    ask_best_qty: Qty,
    ask_best_ts: Ts,

    // Book sides with advanced features
    /// Bid side book
    pub bids: SideBookV2,
    /// Ask side book
    pub asks: SideBookV2,

    // Configuration
    tick_size: Px, // Fixed-point tick size
    lot_size: Qty, // Fixed-point lot size,

    // Cross-book resolution strategy
    cross_resolution: CrossResolution,
}

/// Cross-book resolution strategy
#[derive(Debug, Clone, Copy)]
pub enum CrossResolution {
    /// Reject crossed updates
    Reject,
    /// Automatically resolve by removing conflicting levels
    AutoResolve,
    /// Trust the newest update without modification
    TrustNewest,
}

/// Advanced side book with multiple storage strategies
#[derive(Debug, Clone)]
pub struct SideBookV2 {
    // Fixed arrays for top N levels (cache-friendly)
    prices: [Px; 32],
    qtys: [Qty; 32],
    timestamps: [Ts; 32],
    depth: usize,

    // ROI optimization - dense array for hot price range
    roi_qtys: Vec<Qty>,
    roi_timestamps: Vec<Ts>,
    roi_lb_tick: i64,   // Lower bound in ticks
    roi_ub_tick: i64,   // Upper bound in ticks
    roi_tick_size: i64, // Store as fixed-point ticks

    // Sparse fallback for outliers
    sparse_levels: FxHashMap<i64, (Qty, Ts)>,

    // Tracking
    best_price_tick: i64,
    total_volume: i64,

    // Side indicator
    is_bid: bool,
}

// Constants for invalid prices
const INVALID_MIN: i64 = i64::MIN;
const INVALID_MAX: i64 = i64::MAX;

impl OrderBookV2 {
    /// Create a new order book with advanced features
    pub fn new(symbol: Symbol, tick_size: Px, lot_size: Qty) -> Self {
        Self {
            symbol,
            ts: Ts::from_nanos(0),
            sequence: 0,

            bid_best_price: Px::ZERO,
            bid_best_qty: Qty::ZERO,
            bid_best_ts: Ts::from_nanos(0),
            ask_best_price: Px::new(f64::MAX),
            ask_best_qty: Qty::ZERO,
            ask_best_ts: Ts::from_nanos(0),

            bids: SideBookV2::new_bid(tick_size.as_f64()),
            asks: SideBookV2::new_ask(tick_size.as_f64()),

            tick_size,
            lot_size,
            cross_resolution: CrossResolution::AutoResolve,
        }
    }

    /// Create with ROI optimization for specific price range
    pub fn new_with_roi(
        symbol: Symbol,
        tick_size: Px,
        lot_size: Qty,
        roi_center: Px,
        roi_width: Px,
    ) -> Self {
        let mut book = Self::new(symbol, tick_size, lot_size);

        // Initialize ROI for both sides using fixed-point arithmetic
        let roi_lower_bound = roi_center.sub(Px::from_i64(roi_width.as_i64() / 2));
        let roi_upper_bound = roi_center.add(Px::from_i64(roi_width.as_i64() / 2));

        book.bids
            .init_roi(roi_lower_bound, roi_upper_bound, tick_size);
        book.asks
            .init_roi(roi_lower_bound, roi_upper_bound, tick_size);

        book
    }

    /// Apply L2 update with full validation
    #[inline]
    pub fn apply_validated(&mut self, update: &L2Update) -> Result<(), LobV2Error> {
        // Timestamp validation for BBO
        if update.level == 0 {
            let side_ts = if update.side == Side::Bid {
                self.bid_best_ts
            } else if update.side == Side::Ask {
                self.ask_best_ts
            } else {
                return Ok(());
            };

            if update.ts < side_ts {
                return Err(LobV2Error::OutOfOrderUpdate {
                    existing: side_ts,
                    new: update.ts,
                });
            }
        }

        // Apply the update
        if update.side == Side::Bid {
            self.bids
                .update(update.price, update.qty, update.ts, update.level)?;
        } else if update.side == Side::Ask {
            self.asks
                .update(update.price, update.qty, update.ts, update.level)?;
        } else {
            return Ok(());
        }

        // Update cached BBO
        self.update_cached_bbo();

        // Handle crossed book
        if self.is_crossed() {
            match self.cross_resolution {
                CrossResolution::Reject => {
                    return Err(LobV2Error::CrossedBook {
                        bid: Some(self.bid_best_price),
                        ask: Some(self.ask_best_price),
                    });
                }
                CrossResolution::AutoResolve => {
                    self.resolve_cross(update.ts)?;
                    self.update_cached_bbo();
                }
                CrossResolution::TrustNewest => {
                    // Trust the update, market will resolve
                }
            }
        }

        self.ts = update.ts;
        self.sequence += 1;

        Ok(())
    }

    /// Ultra-fast apply without validation (for trusted feeds)
    #[inline]
    pub fn apply_fast(&mut self, update: &L2Update) {
        if update.side == Side::Bid {
            let _ = self
                .bids
                .update_fast(update.price, update.qty, update.level);
        } else if update.side == Side::Ask {
            let _ = self
                .asks
                .update_fast(update.price, update.qty, update.level);
        }

        self.update_cached_bbo();
        self.ts = update.ts;
        self.sequence += 1;
    }

    /// Update cached BBO for ultra-fast access
    #[inline]
    fn update_cached_bbo(&mut self) {
        if let Some((price, qty, ts)) = self.bids.best_with_ts() {
            self.bid_best_price = price;
            self.bid_best_qty = qty;
            self.bid_best_ts = ts;
        } else {
            self.bid_best_price = Px::ZERO;
            self.bid_best_qty = Qty::ZERO;
        }

        if let Some((price, qty, ts)) = self.asks.best_with_ts() {
            self.ask_best_price = price;
            self.ask_best_qty = qty;
            self.ask_best_ts = ts;
        } else {
            self.ask_best_price = Px::new(f64::MAX);
            self.ask_best_qty = Qty::ZERO;
        }
    }

    /// Check if book is crossed
    #[inline(always)]
    #[must_use]
    pub fn is_crossed(&self) -> bool {
        self.bid_best_price >= self.ask_best_price
            && !self.bid_best_qty.is_zero()
            && !self.ask_best_qty.is_zero()
    }

    /// Smart cross-book resolution
    fn resolve_cross(&mut self, ts: Ts) -> Result<(), LobV2Error> {
        if !self.is_crossed() {
            return Ok(());
        }

        let bid_price = self.bid_best_price;
        let ask_price = self.ask_best_price;

        // Remove ask levels at or below best bid
        self.asks.clear_below_or_equal(bid_price);

        // Remove bid levels at or above best ask
        self.bids.clear_above_or_equal(ask_price);

        // Log the resolution
        tracing::debug!(
            "Resolved crossed book at {}: bid {} >= ask {}",
            ts.as_nanos(),
            bid_price.as_f64(),
            ask_price.as_f64()
        );

        Ok(())
    }

    /// Get best bid (cached - ultra fast)
    #[inline(always)]
    #[must_use]
    pub fn best_bid(&self) -> Option<(Px, Qty)> {
        if !self.bid_best_qty.is_zero() {
            Some((self.bid_best_price, self.bid_best_qty))
        } else {
            None
        }
    }

    /// Get best ask (cached - ultra fast)
    #[inline(always)]
    #[must_use]
    pub fn best_ask(&self) -> Option<(Px, Qty)> {
        if self.ask_best_qty.is_zero() {
            None
        } else {
            Some((self.ask_best_price, self.ask_best_qty))
        }
    }

    /// Calculate mid price
    #[inline]
    #[must_use]
    pub fn mid_price(&self) -> Option<f64> {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid, _)), Some((ask, _))) => Some(f64::midpoint(bid.as_f64(), ask.as_f64())),
            _ => None,
        }
    }

    /// Calculate weighted mid price (microprice)
    #[inline]
    #[must_use]
    pub fn microprice(&self) -> Option<f64> {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid_px, bid_qty)), Some((ask_px, ask_qty))) => {
                let bid_weight = bid_qty.as_f64();
                let ask_weight = ask_qty.as_f64();
                let total_weight = bid_weight + ask_weight;

                if total_weight > 0.0 {
                    Some(
                        bid_px
                            .as_f64()
                            .mul_add(ask_weight, ask_px.as_f64() * bid_weight)
                            / total_weight,
                    )
                } else {
                    self.mid_price()
                }
            }
            _ => None,
        }
    }

    /// Get spread in ticks
    #[inline]
    #[must_use]
    pub fn spread_ticks(&self) -> Option<i64> {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid, _)), Some((ask, _))) => {
                // Calculate spread in ticks using fixed-point arithmetic
                let spread = ask.sub(bid);
                let ticks = spread.as_i64() / self.tick_size.as_i64();
                Some(ticks)
            }
            _ => None,
        }
    }

    /// Get tick size
    #[inline]
    #[must_use]
    pub fn tick_size(&self) -> f64 {
        self.tick_size.as_f64()
    }

    /// Get lot size
    #[inline]
    #[must_use]
    pub fn lot_size(&self) -> f64 {
        self.lot_size.as_f64()
    }

    /// Set cross-book resolution strategy
    pub fn set_cross_resolution(&mut self, strategy: CrossResolution) {
        self.cross_resolution = strategy;
    }

    /// Calculate order book imbalance
    #[inline]
    #[must_use]
    pub fn imbalance(&self, levels: usize) -> f64 {
        let bid_volume = self.bids.total_qty_up_to(levels);
        let ask_volume = self.asks.total_qty_up_to(levels);
        let total = bid_volume + ask_volume;

        if total > 0.0 {
            (bid_volume - ask_volume) / total
        } else {
            0.0
        }
    }
}

impl SideBookV2 {
    /// Create new bid side book
    #[must_use]
    pub fn new_bid(tick_size: f64) -> Self {
        Self {
            prices: [Px::ZERO; 32],
            qtys: [Qty::ZERO; 32],
            timestamps: [Ts::from_nanos(0); 32],
            depth: 0,

            roi_qtys: Vec::with_capacity(1000), // Pre-allocate for ROI range
            roi_timestamps: Vec::with_capacity(1000),
            roi_lb_tick: 0,
            roi_ub_tick: 0,
            roi_tick_size: i64::try_from((tick_size * 10000.0).round() as i128).unwrap_or(i64::MAX), // Safe conversion to fixed-point

            sparse_levels: FxHashMap::with_capacity_and_hasher(100, FxBuildHasher),

            best_price_tick: INVALID_MIN,
            total_volume: 0,
            is_bid: true,
        }
    }

    /// Create new ask side book
    #[must_use]
    pub fn new_ask(tick_size: f64) -> Self {
        Self {
            prices: [Px::new(f64::MAX); 32],
            qtys: [Qty::ZERO; 32],
            timestamps: [Ts::from_nanos(0); 32],
            depth: 0,

            roi_qtys: Vec::with_capacity(1000), // Pre-allocate for ROI range
            roi_timestamps: Vec::with_capacity(1000),
            roi_lb_tick: 0,
            roi_ub_tick: 0,
            roi_tick_size: i64::try_from((tick_size * 10000.0).round() as i128).unwrap_or(i64::MAX), // Safe conversion to fixed-point

            sparse_levels: FxHashMap::with_capacity_and_hasher(100, FxBuildHasher),

            best_price_tick: INVALID_MAX,
            total_volume: 0,
            is_bid: false,
        }
    }

    /// Initialize ROI range
    pub fn init_roi(&mut self, roi_lower_bound: Px, roi_upper_bound: Px, tick_size: Px) {
        // Use fixed-point arithmetic to calculate tick boundaries
        self.roi_lb_tick = roi_lower_bound.as_i64() / tick_size.as_i64();
        self.roi_ub_tick = roi_upper_bound.as_i64() / tick_size.as_i64();
        let roi_size = usize::try_from(self.roi_ub_tick - self.roi_lb_tick + 1).unwrap_or(0);

        self.roi_qtys = vec![Qty::ZERO; roi_size];
        self.roi_timestamps = vec![Ts::from_nanos(0); roi_size];
        self.roi_tick_size = tick_size.as_i64();
    }

    /// Update with validation
    /// Update with validation
    ///
    /// # Errors
    ///
    /// Returns `LobV2Error::InvalidLevel` if level >= 32
    pub fn update(&mut self, price: Px, qty: Qty, ts: Ts, level: u8) -> Result<(), LobV2Error> {
        #[allow(clippy::cast_possible_truncation)]
        // Use fixed-point arithmetic for tick calculation
        let price_tick = if self.roi_tick_size > 0 {
            price.as_i64() / self.roi_tick_size
        } else {
            0
        };

        // Try ROI update first (fastest path)
        if self.update_roi(price_tick, qty, ts) {
            return Ok(());
        }

        // Fallback to array update for top levels
        if level < 32 {
            self.update_array(usize::from(level), price, qty, ts)?;
            return Ok(());
        }

        // Fallback to sparse storage
        if qty.is_zero() {
            self.sparse_levels.remove(&price_tick);
        } else {
            self.sparse_levels.insert(price_tick, (qty, ts));
        }

        // Update total volume after any change
        self.update_total_volume();

        Ok(())
    }

    /// Ultra-fast update without validation
    #[inline]
    pub fn update_fast(&mut self, price: Px, qty: Qty, level: u8) -> bool {
        if level < 32 {
            let idx = usize::from(level);
            self.prices[idx] = price;
            self.qtys[idx] = qty;

            if qty.is_zero() && idx < self.depth {
                // Shift remaining levels up
                for i in idx..self.depth - 1 {
                    self.prices[i] = self.prices[i + 1];
                    self.qtys[i] = self.qtys[i + 1];
                }
                self.depth -= 1;
            } else if !qty.is_zero() && idx >= self.depth {
                self.depth = idx + 1;
            }

            true
        } else {
            false
        }
    }

    /// Update ROI array (ultra-fast for hot range)
    #[inline]
    fn update_roi(&mut self, price_tick: i64, qty: Qty, ts: Ts) -> bool {
        if self.roi_qtys.is_empty() {
            return false;
        }

        if price_tick >= self.roi_lb_tick && price_tick <= self.roi_ub_tick {
            let idx = usize::try_from(price_tick - self.roi_lb_tick).unwrap_or(0);

            // Ultra-fast unchecked access
            unsafe {
                *self.roi_qtys.get_unchecked_mut(idx) = qty;
                *self.roi_timestamps.get_unchecked_mut(idx) = ts;
            }

            // Update best price tracking
            if qty.is_zero() && price_tick == self.best_price_tick {
                self.recalc_best_from_roi();
            } else if !qty.is_zero() {
                if self.is_bid {
                    if price_tick > self.best_price_tick || self.best_price_tick == INVALID_MIN {
                        self.best_price_tick = price_tick;
                    }
                } else if price_tick < self.best_price_tick || self.best_price_tick == INVALID_MAX {
                    self.best_price_tick = price_tick;
                }
            }

            true
        } else {
            false
        }
    }

    /// Update array storage
    fn update_array(
        &mut self,
        level: usize,
        price: Px,
        qty: Qty,
        ts: Ts,
    ) -> Result<(), LobV2Error> {
        if level >= 32 {
            return Err(LobV2Error::InvalidLevel { level });
        }

        self.prices[level] = price;
        self.qtys[level] = qty;
        self.timestamps[level] = ts;

        if qty.is_zero() && level < self.depth {
            // Remove and shift
            for i in level..self.depth - 1 {
                self.prices[i] = self.prices[i + 1];
                self.qtys[i] = self.qtys[i + 1];
                self.timestamps[i] = self.timestamps[i + 1];
            }
            self.depth -= 1;
        } else if !qty.is_zero() && level >= self.depth {
            self.depth = level + 1;
        }

        Ok(())
    }

    /// Recalculate best price from ROI
    fn recalc_best_from_roi(&mut self) {
        if self.is_bid {
            // Search from high to low for bids
            for i in (0..self.roi_qtys.len()).rev() {
                if !self.roi_qtys[i].is_zero() {
                    self.best_price_tick = self.roi_lb_tick + i64::try_from(i).unwrap_or(0);
                    return;
                }
            }
            self.best_price_tick = INVALID_MIN;
        } else {
            // Search from low to high for asks
            for i in 0..self.roi_qtys.len() {
                if !self.roi_qtys[i].is_zero() {
                    self.best_price_tick = self.roi_lb_tick + i64::try_from(i).unwrap_or(0);
                    return;
                }
            }
            self.best_price_tick = INVALID_MAX;
        }
    }

    /// Get best price and quantity
    #[inline]
    #[must_use]
    pub const fn best(&self) -> Option<(Px, Qty)> {
        if self.depth > 0 && !self.qtys[0].is_zero() {
            Some((self.prices[0], self.qtys[0]))
        } else {
            None
        }
    }

    /// Get best with timestamp
    #[inline]
    #[must_use]
    pub fn best_with_ts(&self) -> Option<(Px, Qty, Ts)> {
        // Check ROI first if it's initialized
        if !self.roi_qtys.is_empty()
            && self.best_price_tick != INVALID_MIN
            && self.best_price_tick != INVALID_MAX
        {
            let idx = usize::try_from(self.best_price_tick - self.roi_lb_tick).unwrap_or(0);
            if idx < self.roi_qtys.len() && !self.roi_qtys[idx].is_zero() {
                // Convert tick back to price using fixed-point arithmetic
                let price = Px::from_i64(self.best_price_tick * self.roi_tick_size);
                return Some((price, self.roi_qtys[idx], self.roi_timestamps[idx]));
            }
        }

        // Fallback to array storage
        if self.depth > 0 && !self.qtys[0].is_zero() {
            Some((self.prices[0], self.qtys[0], self.timestamps[0]))
        } else {
            None
        }
    }

    /// Get price and quantity at specific level
    #[inline]
    #[must_use]
    pub const fn get_level(&self, level: usize) -> Option<(Px, Qty)> {
        if level < self.depth && !self.qtys[level].is_zero() {
            Some((self.prices[level], self.qtys[level]))
        } else {
            None
        }
    }

    /// Get current depth (number of levels)
    #[inline]
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.depth
    }

    /// Clear levels below or equal to price (for asks)
    pub fn clear_below_or_equal(&mut self, price: Px) {
        let mut new_depth = 0;
        for i in 0..self.depth {
            if self.prices[i] > price {
                if new_depth != i {
                    self.prices[new_depth] = self.prices[i];
                    self.qtys[new_depth] = self.qtys[i];
                    self.timestamps[new_depth] = self.timestamps[i];
                }
                new_depth += 1;
            }
        }
        self.depth = new_depth;

        // Clear ROI if needed
        if !self.roi_qtys.is_empty() {
            #[allow(clippy::cast_possible_truncation)]
            // Use fixed-point arithmetic for tick calculation
            let price_tick = if self.roi_tick_size > 0 {
                price.as_i64() / self.roi_tick_size
            } else {
                0
            };
            if price_tick >= self.roi_lb_tick {
                let clear_up_to = usize::try_from(price_tick - self.roi_lb_tick + 1)
                    .unwrap_or(0)
                    .min(self.roi_qtys.len());
                for i in 0..clear_up_to {
                    self.roi_qtys[i] = Qty::ZERO;
                    self.roi_timestamps[i] = Ts::from_nanos(0);
                }
            }
        }
    }

    /// Clear levels above or equal to price (for bids)
    pub fn clear_above_or_equal(&mut self, price: Px) {
        let mut new_depth = 0;
        for i in 0..self.depth {
            if self.prices[i] < price {
                if new_depth != i {
                    self.prices[new_depth] = self.prices[i];
                    self.qtys[new_depth] = self.qtys[i];
                    self.timestamps[new_depth] = self.timestamps[i];
                }
                new_depth += 1;
            }
        }
        self.depth = new_depth;

        // Clear ROI if needed
        if !self.roi_qtys.is_empty() {
            #[allow(clippy::cast_possible_truncation)]
            // Use fixed-point arithmetic for tick calculation
            let price_tick = if self.roi_tick_size > 0 {
                price.as_i64() / self.roi_tick_size
            } else {
                0
            };
            if price_tick <= self.roi_ub_tick {
                let clear_from = usize::try_from(price_tick - self.roi_lb_tick).unwrap_or(0);
                for i in clear_from..self.roi_qtys.len() {
                    self.roi_qtys[i] = Qty::ZERO;
                    self.roi_timestamps[i] = Ts::from_nanos(0);
                }
            }
        }
    }

    /// Get total volume tracked
    #[inline]
    #[must_use]
    pub const fn total_volume(&self) -> i64 {
        self.total_volume
    }

    /// Update total volume when book changes
    fn update_total_volume(&mut self) {
        let mut total = 0i64;
        for i in 0..self.depth {
            total += self.qtys[i].as_i64();
        }
        // Add ROI volumes if present
        if !self.roi_qtys.is_empty() {
            for qty in &self.roi_qtys {
                total += qty.as_i64();
            }
        }
        self.total_volume = total;
    }

    /// Calculate total quantity up to N levels
    #[must_use]
    pub fn total_qty_up_to(&self, levels: usize) -> f64 {
        let limit = levels.min(self.depth);
        let mut total = 0.0;

        for i in 0..limit {
            total += self.qtys[i].as_f64();
        }

        total
    }

    /// SIMD-optimized total quantity calculation
    ///
    /// # Safety
    ///
    /// This function uses AVX2 intrinsics and requires that the caller ensures:
    /// - The target CPU supports AVX2 instructions
    /// - The quantity arrays are properly aligned and initialized
    /// - Array bounds are respected (levels <= self.depth <= 32)
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    #[must_use]
    pub unsafe fn total_qty_simd(&self, levels: usize) -> f64 {
        let limit = levels.min(self.depth).min(32);

        if limit >= 4 {
            // Process 4 elements at a time with AVX2
            let mut sum = _mm256_setzero_pd();
            let chunks = limit / 4;

            for i in 0..chunks {
                let base = i * 4;
                let values = _mm256_set_pd(
                    self.qtys[base + 3].as_f64(),
                    self.qtys[base + 2].as_f64(),
                    self.qtys[base + 1].as_f64(),
                    self.qtys[base].as_f64(),
                );
                sum = _mm256_add_pd(sum, values);
            }

            // Extract and sum the 4 f64 values
            let mut result = [0.0; 4];
            unsafe {
                _mm256_storeu_pd(result.as_mut_ptr(), sum);
            }
            let mut total = result.iter().sum::<f64>();

            // Handle remaining elements
            for i in (chunks * 4)..limit {
                total += self.qtys[i].as_f64();
            }

            total
        } else {
            self.total_qty_up_to(levels)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roi_updates() {
        let mut book = OrderBookV2::new_with_roi(
            Symbol(1),
            Px::new(0.01),  // tick size
            Qty::new(1.0),  // lot size
            Px::new(100.0), // center
            Px::new(10.0),  // width (95-105)
        );

        // Update within ROI
        let update = L2Update::new(Ts::from_nanos(1), Symbol(1)).with_level_data(
            Side::Bid,
            Px::new(99.50),
            Qty::new(100.0),
            0,
        );

        assert!(book.apply_validated(&update).is_ok());
        assert_eq!(book.best_bid(), Some((Px::new(99.50), Qty::new(100.0))));
    }

    #[test]
    fn test_cross_resolution() {
        let mut book = OrderBookV2::new(Symbol(1), Px::new(0.01), Qty::new(1.0));
        book.cross_resolution = CrossResolution::AutoResolve;

        // Set up initial book
        assert!(
            book.apply_validated(
                &L2Update::new(Ts::from_nanos(1), Symbol(1)).with_level_data(
                    Side::Bid,
                    Px::new(100.0),
                    Qty::new(100.0),
                    0,
                ),
            )
            .is_ok(),
            "Failed to apply bid in cross resolution test"
        );

        assert!(
            book.apply_validated(
                &L2Update::new(Ts::from_nanos(2), Symbol(1)).with_level_data(
                    Side::Ask,
                    Px::new(100.5),
                    Qty::new(100.0),
                    0,
                ),
            )
            .is_ok(),
            "Failed to apply ask in cross resolution test"
        );

        // Create a crossed update
        let crossed = L2Update::new(Ts::from_nanos(3), Symbol(1)).with_level_data(
            Side::Bid,
            Px::new(101.0),
            Qty::new(50.0),
            0,
        );

        // Should auto-resolve
        assert!(book.apply_validated(&crossed).is_ok());
        assert!(!book.is_crossed());
    }

    #[test]
    fn test_timestamp_validation() {
        let mut book = OrderBookV2::new(Symbol(1), Px::new(0.01), Qty::new(1.0));

        // First update
        assert!(
            book.apply_validated(
                &L2Update::new(Ts::from_nanos(100), Symbol(1)).with_level_data(
                    Side::Bid,
                    Px::new(100.0),
                    Qty::new(100.0),
                    0,
                ),
            )
            .is_ok(),
            "Failed to apply bid in timestamp validation test"
        );

        // Out of order update should fail
        let old_update = L2Update::new(Ts::from_nanos(50), Symbol(1)).with_level_data(
            Side::Bid,
            Px::new(99.0),
            Qty::new(50.0),
            0,
        );

        assert!(matches!(
            book.apply_validated(&old_update),
            Err(LobV2Error::OutOfOrderUpdate { .. })
        ));
    }
}
