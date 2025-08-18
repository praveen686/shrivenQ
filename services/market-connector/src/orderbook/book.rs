//! Core order book implementation
//!
//! COMPLIANCE:
//! - Zero allocations in hot paths
//! - Fixed-point arithmetic only (no floats)
//! - < 200ns p50 for L2 updates
//! - Cache-aligned structures

use crate::orderbook::price_levels::{DEPTH, SideBook};
use services_common::{L2Update, LOBUpdate, Px, Qty, Side, Symbol, Ts};

/// Full order book for a single symbol
///
/// Performance characteristics:
/// - Apply L2 update: < 200ns p50
/// - Best bid/ask: O(1)
/// - Mid/microprice: O(1)
/// - State hash: O(n) where n = depth
#[derive(Clone, Debug)]
#[repr(C)] // Predictable memory layout
pub struct OrderBook {
    /// Symbol this book represents
    pub symbol: Symbol,
    /// Last update timestamp
    pub ts: Ts,
    /// Bid side (buyers)
    pub bids: SideBook,
    /// Ask side (sellers)
    pub asks: SideBook,
    /// Sequence number for updates
    pub sequence: u64,
}

impl OrderBook {
    /// Create a new empty order book
    /// No allocations - stack only
    #[inline]
    #[must_use]
    pub fn new(symbol: Symbol) -> Self {
        Self {
            symbol,
            ts: Ts::from_nanos(0),
            bids: SideBook::new(),
            asks: SideBook::new(),
            sequence: 0,
        }
    }

    /// Apply an L2 update to the book
    ///
    /// This is the hot path - must be < 200ns p50
    /// No allocations allowed
    ///
    /// # Errors
    ///
    /// Returns an error if the update would result in a crossed book.
    #[inline]
    pub fn apply(&mut self, update: &L2Update) -> Result<(), BookError> {
        // Update timestamp and sequence
        self.ts = update.ts;
        self.sequence += 1;

        // Select the side - no allocation
        let side = match update.side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };

        // Apply the update
        side.set(usize::from(update.level), update.price, update.qty);

        // Check for crossed book
        if self.is_crossed() {
            return Err(BookError::CrossedBook {
                bid: self.bids.best().map(|(p, _)| p),
                ask: self.asks.best().map(|(p, _)| p),
            });
        }

        Ok(())
    }

    /// Get the best bid price and size
    /// Performance: O(1)
    #[inline(always)]
    #[must_use]
    pub fn best_bid(&self) -> Option<(Px, Qty)> {
        self.bids.best()
    }

    /// Get the best ask price and size
    /// Performance: O(1)
    #[inline(always)]
    #[must_use]
    pub fn best_ask(&self) -> Option<(Px, Qty)> {
        self.asks.best()
    }

    /// Calculate mid price (average of best bid and ask)
    /// Uses fixed-point arithmetic only
    /// Performance: O(1)
    #[inline]
    #[must_use]
    pub fn mid(&self) -> Option<Px> {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid, _)), Some((ask, _))) => {
                // Fixed-point arithmetic - no floats
                let mid_ticks = (bid.as_i64() + ask.as_i64()) >> 1; // Fast division by 2
                Some(Px::from_i64(mid_ticks))
            }
            _ => None,
        }
    }

    /// Calculate microprice (size-weighted mid)
    /// Uses fixed-point arithmetic only
    /// Performance: O(1)
    #[inline]
    #[must_use]
    pub fn microprice(&self) -> Option<Px> {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid_px, bid_qty)), Some((ask_px, ask_qty))) => {
                let bid_val = bid_px.as_i64() * ask_qty.as_i64();
                let ask_val = ask_px.as_i64() * bid_qty.as_i64();
                let total_qty = bid_qty.as_i64() + ask_qty.as_i64();

                if total_qty > 0 {
                    let micro_ticks = (bid_val + ask_val) / total_qty;
                    Some(Px::from_i64(micro_ticks))
                } else {
                    self.mid()
                }
            }
            _ => None,
        }
    }

    /// Calculate spread in ticks
    /// Performance: O(1)
    #[inline(always)]
    #[must_use]
    pub fn spread_ticks(&self) -> Option<i64> {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid, _)), Some((ask, _))) => Some(ask.as_i64() - bid.as_i64()),
            _ => None,
        }
    }

    /// Calculate order book imbalance
    /// Returns value between -1.0 (all on ask) and 1.0 (all on bid)
    /// NOTE: Returns f64 for ratio only - not for money calculations
    /// Performance: O(n) where n = depth
    #[inline]
    #[must_use]
    pub fn imbalance(&self, depth: usize) -> Option<f64> {
        let bid_qty = self.bids.total_qty(depth).as_i64();
        let ask_qty = self.asks.total_qty(depth).as_i64();
        let total = bid_qty + ask_qty;

        if total > 0 {
            // Convert to f64 for ratio calculation only
            // This is NOT money calculation - just a ratio
            // SAFETY: i64 to f64 for ratio calculation, not money
            let bid_f64 = bid_qty as f64;
            // SAFETY: i64 to f64 for ratio calculation, not money
            let ask_f64 = ask_qty as f64;
            // SAFETY: i64 to f64 for ratio calculation, not money
            let total_f64 = total as f64;
            Some((bid_f64 - ask_f64) / total_f64)
        } else {
            None
        }
    }

    /// Check if book is crossed (bid >= ask)
    /// Performance: O(1)
    #[inline(always)]
    #[must_use]
    pub fn is_crossed(&self) -> bool {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid, _)), Some((ask, _))) => bid >= ask,
            _ => false,
        }
    }

    /// Check if book is locked (bid == ask)
    /// Performance: O(1)
    #[inline(always)]
    #[must_use]
    pub fn is_locked(&self) -> bool {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid, _)), Some((ask, _))) => bid == ask,
            _ => false,
        }
    }

    /// Clear the entire book
    /// Performance: O(n) where n = depth
    #[inline]
    pub fn clear(&mut self) {
        self.bids.clear();
        self.asks.clear();
        self.sequence = 0;
    }

    /// Create LOB update message
    /// No allocations - stack only
    /// Performance: O(1)
    #[inline]
    #[must_use]
    pub fn to_update(&self) -> LOBUpdate {
        let (bid, bid_size) = self
            .best_bid()
            .map_or((None, None), |(p, q)| (Some(p), Some(q)));

        let (ask, ask_size) = self
            .best_ask()
            .map_or((None, None), |(p, q)| (Some(p), Some(q)));

        let mut update = LOBUpdate::new(self.ts, self.symbol);
        if let (Some(bid_px), Some(bid_sz)) = (bid, bid_size) {
            update = update.with_bid(bid_px, bid_sz);
        }
        if let (Some(ask_px), Some(ask_sz)) = (ask, ask_size) {
            update = update.with_ask(ask_px, ask_sz);
        }
        update
    }

    /// Get a hash of the book state for deterministic verification
    /// Performance: O(n) where n = depth
    #[inline]
    #[must_use]
    pub fn state_hash(&self) -> u64 {
        let mut hash = 0u64;

        // Include bid levels
        for i in 0..self.bids.depth.min(DEPTH) {
            hash = hash
                .wrapping_mul(31)
                // SAFETY: Wrapping cast for hash calculation
                .wrapping_add(self.bids.prices[i].as_i64() as u64);
            hash = hash
                .wrapping_mul(31)
                // SAFETY: Wrapping cast for hash calculation
                .wrapping_add(self.bids.qtys[i].as_i64() as u64);
        }

        // Include ask levels
        for i in 0..self.asks.depth.min(DEPTH) {
            hash = hash
                .wrapping_mul(31)
                // SAFETY: Wrapping cast for hash calculation
                .wrapping_add(self.asks.prices[i].as_i64() as u64);
            hash = hash
                .wrapping_mul(31)
                // SAFETY: Wrapping cast for hash calculation
                .wrapping_add(self.asks.qtys[i].as_i64() as u64);
        }

        hash
    }
}

/// Error types for order book operations
#[derive(Debug, thiserror::Error)]
pub enum BookError {
    /// Book would be crossed after update
    #[error("Crossed book: bid={bid:?} >= ask={ask:?}")]
    CrossedBook {
        /// Best bid that would cross
        bid: Option<Px>,
        /// Best ask that would cross
        ask: Option<Px>,
    },

    /// Invalid price level
    #[error("Invalid level: {level} >= {}", DEPTH)]
    InvalidLevel {
        /// The invalid level
        level: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test constants for order book testing
    const TEST_SYMBOL_ID: u32 = 1;
    const TEST_TIMESTAMP_1: u64 = 1000;
    const TEST_TIMESTAMP_2: u64 = 2000;
    const TEST_BID_PRICE: i64 = 995000; // 99.50 in fixed-point
    const TEST_ASK_PRICE: i64 = 1005000; // 100.50 in fixed-point
    const TEST_QUANTITY: i64 = 1000000; // 100.0 in fixed-point
    const TEST_MICROPRICE_EXPECTED: i64 = 998333; // Expected microprice value
    const TEST_PRICE_INCREMENT: i64 = 1000; // Price increment for multi-level tests
    const TEST_LEVELS_COUNT: usize = 5; // Number of levels to test
    const TEST_SPREAD: i64 = 10000; // Expected spread in test (1.0)
    const TEST_LARGE_QUANTITY: i64 = 1500000; // Large quantity for tests (150.0)
    const TEST_SMALL_QUANTITY: i64 = 500000; // Small quantity for tests (50.0)
    const TEST_CROSSED_BID_PRICE: i64 = 1010000; // Price that would cross the book (101.0)
    const TEST_MID_ASK_PRICE: i64 = 1000000; // Middle ask price (100.0)
    const TEST_LARGER_QUANTITY: i64 = 2000000; // Larger quantity for microprice test (200.0)

    #[test]
    fn test_order_book_basic() {
        let mut book = OrderBook::new(Symbol::new(TEST_SYMBOL_ID));

        // Add bid levels
        let update = L2Update {
            ts: Ts::from_nanos(TEST_TIMESTAMP_1),
            symbol: Symbol::new(TEST_SYMBOL_ID),
            side: Side::Bid,
            price: Px::from_i64(TEST_BID_PRICE),
            qty: Qty::from_i64(TEST_QUANTITY),
            level: 0,
        };
        book.apply(&update).unwrap();

        // Add ask levels
        let update = L2Update {
            ts: Ts::from_nanos(TEST_TIMESTAMP_2),
            symbol: Symbol::new(TEST_SYMBOL_ID),
            side: Side::Ask,
            price: Px::from_i64(TEST_ASK_PRICE),
            qty: Qty::from_i64(TEST_QUANTITY + 500000), // 150.0 = 100.0 + 50.0
            level: 0,
        };
        book.apply(&update).unwrap();

        assert_eq!(
            book.best_bid(),
            Some((Px::from_i64(TEST_BID_PRICE), Qty::from_i64(TEST_QUANTITY)))
        );
        assert_eq!(
            book.best_ask(),
            Some((
                Px::from_i64(TEST_ASK_PRICE),
                Qty::from_i64(TEST_LARGE_QUANTITY)
            ))
        );
        assert_eq!(book.spread_ticks(), Some(TEST_SPREAD));
        assert!(!book.is_crossed());
    }

    #[test]
    fn test_crossed_book_prevention() {
        let mut book = OrderBook::new(Symbol::new(TEST_SYMBOL_ID));

        // Add ask at 100
        let update = L2Update {
            ts: Ts::from_nanos(TEST_TIMESTAMP_1),
            symbol: Symbol::new(TEST_SYMBOL_ID),
            side: Side::Ask,
            price: Px::from_i64(TEST_MID_ASK_PRICE), // 100.0
            qty: Qty::from_i64(TEST_QUANTITY),
            level: 0,
        };
        book.apply(&update).unwrap();

        // Try to add bid at 101 (would cross)
        let update = L2Update {
            ts: Ts::from_nanos(TEST_TIMESTAMP_2),
            symbol: Symbol::new(TEST_SYMBOL_ID),
            side: Side::Bid,
            price: Px::from_i64(TEST_CROSSED_BID_PRICE), // 101.0
            qty: Qty::from_i64(TEST_QUANTITY),
            level: 0,
        };
        let result = book.apply(&update);

        assert!(result.is_err());
        assert!(matches!(result, Err(BookError::CrossedBook { .. })));
    }

    #[test]
    fn test_microprice() {
        let mut book = OrderBook::new(Symbol::new(TEST_SYMBOL_ID));

        // Bid: 99.5 x 100
        let update = L2Update {
            ts: Ts::from_nanos(TEST_TIMESTAMP_1),
            symbol: Symbol::new(TEST_SYMBOL_ID),
            side: Side::Bid,
            price: Px::from_i64(TEST_BID_PRICE), // 99.5
            qty: Qty::from_i64(TEST_QUANTITY),   // 100.0
            level: 0,
        };
        book.apply(&update).unwrap();

        // Ask: 100.5 x 200
        let update = L2Update {
            ts: Ts::from_nanos(TEST_TIMESTAMP_2),
            symbol: Symbol::new(TEST_SYMBOL_ID),
            side: Side::Ask,
            price: Px::from_i64(TEST_ASK_PRICE),      // 100.5
            qty: Qty::from_i64(TEST_LARGER_QUANTITY), // 200.0
            level: 0,
        };
        book.apply(&update).unwrap();

        let micro = book.microprice().unwrap();
        // Microprice = (bid_price * ask_qty + ask_price * bid_qty) / (bid_qty + ask_qty)
        // Uses size-weighted calculation for accurate mid price
        assert_eq!(micro.as_i64(), TEST_MICROPRICE_EXPECTED);
    }

    #[test]
    fn test_imbalance() {
        let mut book = OrderBook::new(Symbol::new(TEST_SYMBOL_ID));

        // Add more on bid side
        const IMBALANCE_TEST_LEVELS: usize = 3;
        for i in 0..IMBALANCE_TEST_LEVELS {
            let update = L2Update {
                ts: Ts::from_nanos(TEST_TIMESTAMP_1 + i as u64),
                symbol: Symbol::new(TEST_SYMBOL_ID),
                side: Side::Bid,
                // SAFETY: i64 cast is safe for small loop values
                price: Px::from_i64(TEST_BID_PRICE - (i as i64) * TEST_PRICE_INCREMENT),
                qty: Qty::from_i64(TEST_QUANTITY),
                // SAFETY: Loop runs 0..3, always fits in u8
                level: i as u8,
            };
            book.apply(&update).unwrap();
        }

        // Add less on ask side
        const IMBALANCE_TEST_TIMESTAMP: u64 = 5000;
        let update = L2Update {
            ts: Ts::from_nanos(IMBALANCE_TEST_TIMESTAMP),
            symbol: Symbol::new(TEST_SYMBOL_ID),
            side: Side::Ask,
            price: Px::from_i64(TEST_MID_ASK_PRICE),
            qty: Qty::from_i64(TEST_SMALL_QUANTITY),
            level: 0,
        };
        book.apply(&update).unwrap();

        let imb = book.imbalance(5).unwrap();
        // Bid: 300, Ask: 50, Imbalance = (300 - 50) / 350 = 0.714...
        assert!(imb > 0.7 && imb < 0.75);
    }

    #[test]
    fn test_state_hash_deterministic() {
        let mut book1 = OrderBook::new(Symbol::new(TEST_SYMBOL_ID));
        let mut book2 = OrderBook::new(Symbol::new(TEST_SYMBOL_ID));

        const HASH_TEST_PRICE_2: i64 = 994000; // Lower bid price for level 1
        let updates = vec![
            L2Update {
                ts: Ts::from_nanos(1),
                symbol: Symbol::new(TEST_SYMBOL_ID),
                side: Side::Bid,
                price: Px::from_i64(TEST_BID_PRICE),
                qty: Qty::from_i64(TEST_QUANTITY),
                level: 0,
            },
            L2Update {
                ts: Ts::from_nanos(2),
                symbol: Symbol::new(TEST_SYMBOL_ID),
                side: Side::Ask,
                price: Px::from_i64(TEST_ASK_PRICE),
                qty: Qty::from_i64(TEST_LARGE_QUANTITY),
                level: 0,
            },
            L2Update {
                ts: Ts::from_nanos(3),
                symbol: Symbol::new(TEST_SYMBOL_ID),
                side: Side::Bid,
                price: Px::from_i64(HASH_TEST_PRICE_2),
                qty: Qty::from_i64(TEST_LARGER_QUANTITY),
                level: 1,
            },
        ];

        for update in &updates {
            book1.apply(update).unwrap();
            book2.apply(update).unwrap();
        }

        assert_eq!(book1.state_hash(), book2.state_hash());
    }
}
