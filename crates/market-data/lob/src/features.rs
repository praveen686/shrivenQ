//! Feature extraction from order book

use crate::book::OrderBook;
use common::{FeatureFrame, Px, Qty, Ts};

/// Feature calculator for order book metrics
pub struct FeatureCalculator {
    /// VWAP window in nanoseconds
    window_ns: u64,
    /// VWAP price history (circular buffer)
    prices: Vec<(Ts, Px, Qty)>,
    /// Current position in circular buffer
    pos: usize,
    /// Max entries in VWAP buffer
    capacity: usize,
}

impl FeatureCalculator {
    /// Create a new feature calculator
    #[must_use]
    pub fn new(vwap_window_ns: u64, vwap_capacity: usize) -> Self {
        Self {
            window_ns: vwap_window_ns,
            prices: Vec::with_capacity(vwap_capacity),
            pos: 0,
            capacity: vwap_capacity,
        }
    }

    /// Calculate features from current order book state
    #[inline]
    pub fn calculate(&mut self, book: &OrderBook) -> Option<FeatureFrame> {
        // Need both sides for most features
        let (bid_px, bid_qty) = book.best_bid()?;
        let (ask_px, ask_qty) = book.best_ask()?;

        // Basic features
        let spread_ticks = ask_px.as_i64() - bid_px.as_i64();
        let mid = i64::midpoint(bid_px.as_i64(), ask_px.as_i64());

        // Microprice
        let micro = {
            let bid_val = bid_px.as_i64() * ask_qty.as_i64();
            let ask_val = ask_px.as_i64() * bid_qty.as_i64();
            let total_qty = bid_qty.as_i64() + ask_qty.as_i64();
            if total_qty > 0 {
                (bid_val + ask_val) / total_qty
            } else {
                mid
            }
        };

        // Imbalance at depth 5
        let imbalance = book.imbalance(5).unwrap_or(0.0);

        // Update VWAP tracking with mid price
        let mid_px = Px::from_i64(mid);
        let total_qty = Qty::from_i64(bid_qty.as_i64() + ask_qty.as_i64());
        self.update_prices(book.ts, mid_px, total_qty);

        // Calculate VWAP deviation
        let vwap_dev = self.calculate_deviation(mid_px);

        Some(
            FeatureFrame::new(book.ts, book.symbol)
                .with_prices(spread_ticks, mid, micro)
                .with_metrics(imbalance, vwap_dev),
        )
    }

    /// Update price tracking
    #[inline]
    fn update_prices(&mut self, ts: Ts, price: Px, qty: Qty) {
        // Add to circular buffer
        if self.prices.len() < self.capacity {
            self.prices.push((ts, price, qty));
        } else {
            self.prices[self.pos] = (ts, price, qty);
            self.pos = (self.pos + 1) % self.capacity;
        }

        // Remove old entries
        let cutoff = ts.as_nanos().saturating_sub(self.window_ns);
        self.prices.retain(|(t, _, _)| t.as_nanos() >= cutoff);
    }

    /// Calculate VWAP deviation as percentage
    #[inline]
    fn calculate_deviation(&self, current_price: Px) -> f64 {
        if self.prices.is_empty() {
            return 0.0;
        }

        let mut value_sum = 0i64;
        let mut qty_sum = 0i64;

        for (_, px, qty) in &self.prices {
            value_sum += px.as_i64() * qty.as_i64();
            qty_sum += qty.as_i64();
        }

        if qty_sum > 0 {
            let vwap = value_sum / qty_sum;
            let deviation = current_price.as_i64() - vwap;
            #[allow(clippy::cast_precision_loss)]
            // SAFETY: Cast is safe within expected range
            let result = (deviation as f64 / vwap as f64) * 100.0;
            result
        } else {
            0.0
        }
    }

    /// Reset the calculator
    pub fn reset(&mut self) {
        self.prices.clear();
        self.pos = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{L2Update, Side, Symbol};

    #[test]
    fn test_feature_calculation() {
        let mut book = OrderBook::new(Symbol::new(1));
        let mut calc = FeatureCalculator::new(60_000_000_000, 1000); // 60s window

        // Setup book
        assert!(
            book.apply(
                &L2Update::new(Ts::from_nanos(1000), Symbol::new(1)).with_level_data(
                    Side::Bid,
                    Px::new(99.5),
                    Qty::new(100.0),
                    0,
                ),
            )
            .is_ok(),
            "Failed to apply bid update in test"
        );

        assert!(
            book.apply(
                &L2Update::new(Ts::from_nanos(2000), Symbol::new(1)).with_level_data(
                    Side::Ask,
                    Px::new(100.5),
                    Qty::new(150.0),
                    0,
                ),
            )
            .is_ok(),
            "Failed to apply ask update in test"
        );

        let features = match calc.calculate(&book) {
            Some(f) => f,
            None => {
                assert!(
                    false,
                    "Failed to calculate features in test - book should have valid BBO"
                );
                return;
            }
        };

        assert_eq!(features.symbol, Symbol::new(1));
        assert_eq!(features.spread_ticks, 10000); // 1.0 * 10000
        assert_eq!(features.mid, 1_000_000); // 100.0 * 10000
        // With bid=100 and ask=150, imbalance = (100-150)/(100+150) = -50/250 = -0.2
        assert!((features.imbalance - (-0.2)).abs() < 0.01);
    }

    #[test]
    fn test_vwap_tracking() {
        let mut calc = FeatureCalculator::new(5_000_000_000, 100); // 5s window

        // Add some price points
        calc.update_prices(Ts::from_nanos(1000), Px::new(100.0), Qty::new(100.0));
        calc.update_prices(Ts::from_nanos(2000), Px::new(101.0), Qty::new(200.0));
        calc.update_prices(Ts::from_nanos(3000), Px::new(99.0), Qty::new(150.0));

        // VWAP = (100*100 + 101*200 + 99*150) / (100+200+150)
        //      = (10000 + 20200 + 14850) / 450
        //      = 45050 / 450 = 100.111...

        let deviation = calc.calculate_deviation(Px::new(102.0));
        // Deviation = (102 - 100.111) / 100.111 * 100 ≈ 1.88%
        assert!(deviation > 1.8 && deviation < 2.0);
    }
}
