//! Core order book implementation

use common::{L2Update, LOBUpdate, Px, Qty, Side, Symbol, Ts};
use crate::price_levels::{SideBook, DEPTH};

/// Full order book for a single symbol
#[derive(Clone, Debug)]
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
    #[inline]
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
    #[inline]
    pub fn apply(&mut self, update: &L2Update) -> Result<(), BookError> {
        // Update timestamp and sequence
        self.ts = update.ts;
        self.sequence += 1;

        // Select the side
        let side = match update.side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };

        // Apply the update
        side.set(update.level as usize, update.price, update.qty);

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
    #[inline]
    pub fn best_bid(&self) -> Option<(Px, Qty)> {
        self.bids.best()
    }

    /// Get the best ask price and size
    #[inline]
    pub fn best_ask(&self) -> Option<(Px, Qty)> {
        self.asks.best()
    }

    /// Calculate mid price (average of best bid and ask)
    #[inline]
    pub fn mid(&self) -> Option<Px> {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid, _)), Some((ask, _))) => {
                let mid_ticks = (bid.as_i64() + ask.as_i64()) / 2;
                Some(Px::from_i64(mid_ticks))
            }
            _ => None,
        }
    }

    /// Calculate microprice (size-weighted mid)
    #[inline]
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
    #[inline]
    pub fn spread_ticks(&self) -> Option<i64> {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid, _)), Some((ask, _))) => {
                Some(ask.as_i64() - bid.as_i64())
            }
            _ => None,
        }
    }

    /// Calculate order book imbalance
    /// Returns value between -1.0 (all on ask) and 1.0 (all on bid)
    #[inline]
    pub fn imbalance(&self, depth: usize) -> Option<f64> {
        let bid_qty = self.bids.total_qty(depth).as_i64();
        let ask_qty = self.asks.total_qty(depth).as_i64();
        let total = bid_qty + ask_qty;
        
        if total > 0 {
            Some((bid_qty - ask_qty) as f64 / total as f64)
        } else {
            None
        }
    }

    /// Check if book is crossed (bid >= ask)
    #[inline]
    pub fn is_crossed(&self) -> bool {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid, _)), Some((ask, _))) => bid >= ask,
            _ => false,
        }
    }

    /// Check if book is locked (bid == ask)
    #[inline]
    pub fn is_locked(&self) -> bool {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid, _)), Some((ask, _))) => bid == ask,
            _ => false,
        }
    }

    /// Clear the entire book
    #[inline]
    pub fn clear(&mut self) {
        self.bids.clear();
        self.asks.clear();
        self.sequence = 0;
    }

    /// Create LOB update message
    #[inline]
    pub fn to_update(&self) -> LOBUpdate {
        let (bid, bid_size) = self.best_bid()
            .map(|(p, q)| (Some(p), Some(q)))
            .unwrap_or((None, None));
        
        let (ask, ask_size) = self.best_ask()
            .map(|(p, q)| (Some(p), Some(q)))
            .unwrap_or((None, None));

        LOBUpdate::new(
            self.ts,
            self.symbol,
            bid,
            bid_size,
            ask,
            ask_size,
        )
    }

    /// Get a hash of the book state for deterministic verification
    #[inline]
    pub fn state_hash(&self) -> u64 {
        let mut hash = 0u64;
        
        // Include bid levels
        for i in 0..self.bids.depth.min(DEPTH) {
            hash = hash.wrapping_mul(31).wrapping_add(self.bids.prices[i].as_i64() as u64);
            hash = hash.wrapping_mul(31).wrapping_add(self.bids.qtys[i].as_i64() as u64);
        }
        
        // Include ask levels
        for i in 0..self.asks.depth.min(DEPTH) {
            hash = hash.wrapping_mul(31).wrapping_add(self.asks.prices[i].as_i64() as u64);
            hash = hash.wrapping_mul(31).wrapping_add(self.asks.qtys[i].as_i64() as u64);
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

    #[test]
    fn test_order_book_basic() {
        let mut book = OrderBook::new(Symbol::new(1));
        
        // Add bid levels
        let update = L2Update::new(
            Ts::from_nanos(1000),
            Symbol::new(1),
            Side::Bid,
            Px::new(99.5),
            Qty::new(100.0),
            0,
        );
        book.apply(&update).unwrap();
        
        // Add ask levels
        let update = L2Update::new(
            Ts::from_nanos(2000),
            Symbol::new(1),
            Side::Ask,
            Px::new(100.5),
            Qty::new(150.0),
            0,
        );
        book.apply(&update).unwrap();
        
        assert_eq!(book.best_bid(), Some((Px::new(99.5), Qty::new(100.0))));
        assert_eq!(book.best_ask(), Some((Px::new(100.5), Qty::new(150.0))));
        assert_eq!(book.spread_ticks(), Some(10000)); // 1.0 * 10000
        assert!(!book.is_crossed());
    }

    #[test]
    fn test_crossed_book_prevention() {
        let mut book = OrderBook::new(Symbol::new(1));
        
        // Add ask at 100
        book.apply(&L2Update::new(
            Ts::from_nanos(1000),
            Symbol::new(1),
            Side::Ask,
            Px::new(100.0),
            Qty::new(100.0),
            0,
        )).unwrap();
        
        // Try to add bid at 101 (would cross)
        let result = book.apply(&L2Update::new(
            Ts::from_nanos(2000),
            Symbol::new(1),
            Side::Bid,
            Px::new(101.0),
            Qty::new(100.0),
            0,
        ));
        
        assert!(result.is_err());
        assert!(matches!(result, Err(BookError::CrossedBook { .. })));
    }

    #[test]
    fn test_microprice() {
        let mut book = OrderBook::new(Symbol::new(1));
        
        // Bid: 99.5 x 100
        book.apply(&L2Update::new(
            Ts::from_nanos(1000),
            Symbol::new(1),
            Side::Bid,
            Px::new(99.5),
            Qty::new(100.0),
            0,
        )).unwrap();
        
        // Ask: 100.5 x 200  
        book.apply(&L2Update::new(
            Ts::from_nanos(2000),
            Symbol::new(1),
            Side::Ask,
            Px::new(100.5),
            Qty::new(200.0),
            0,
        )).unwrap();
        
        let micro = book.microprice().unwrap();
        // Microprice = (99.5 * 200 + 100.5 * 100) / (100 + 200)
        // = (19900 + 10050) / 300 = 29950 / 300 = 99.833...
        // But we're in fixed point, so:
        // bid_val = 995000 * 2000000 = 1990000000000
        // ask_val = 1005000 * 1000000 = 1005000000000  
        // total = (1990000000000 + 1005000000000) / 3000000 = 998333
        // which is 99.8333 in real price
        assert!((micro.as_f64() - 99.8333).abs() < 0.01);
    }

    #[test]
    fn test_imbalance() {
        let mut book = OrderBook::new(Symbol::new(1));
        
        // Add more on bid side
        for i in 0..3 {
            book.apply(&L2Update::new(
                Ts::from_nanos(1000 + i),
                Symbol::new(1),
                Side::Bid,
                Px::new(99.5 - i as f64 * 0.1),
                Qty::new(100.0),
                i as u8,
            )).unwrap();
        }
        
        // Add less on ask side
        book.apply(&L2Update::new(
            Ts::from_nanos(5000),
            Symbol::new(1),
            Side::Ask,
            Px::new(100.0),
            Qty::new(50.0),
            0,
        )).unwrap();
        
        let imb = book.imbalance(5).unwrap();
        // Bid: 300, Ask: 50, Imbalance = (300 - 50) / 350 = 0.714...
        assert!(imb > 0.7 && imb < 0.75);
    }

    #[test]
    fn test_state_hash_deterministic() {
        let mut book1 = OrderBook::new(Symbol::new(1));
        let mut book2 = OrderBook::new(Symbol::new(1));
        
        let updates = vec![
            L2Update::new(Ts::from_nanos(1), Symbol::new(1), Side::Bid, Px::new(99.5), Qty::new(100.0), 0),
            L2Update::new(Ts::from_nanos(2), Symbol::new(1), Side::Ask, Px::new(100.5), Qty::new(150.0), 0),
            L2Update::new(Ts::from_nanos(3), Symbol::new(1), Side::Bid, Px::new(99.4), Qty::new(200.0), 1),
        ];
        
        for update in &updates {
            book1.apply(update).unwrap();
            book2.apply(update).unwrap();
        }
        
        assert_eq!(book1.state_hash(), book2.state_hash());
    }
}