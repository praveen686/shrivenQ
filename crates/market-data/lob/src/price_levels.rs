//! Price level management for one side of the order book

use common::{Px, Qty};

/// Fixed depth for order book (32 levels per side)
pub const DEPTH: usize = 32;

/// One side of the order book (bid or ask)
///
/// Structure-of-arrays design for cache efficiency:
/// - All prices in one contiguous array
/// - All quantities in another contiguous array
/// - CPU can vectorize operations and prefetch efficiently
#[derive(Clone, Debug)]
pub struct SideBook {
    /// Price levels (0 = best, DEPTH-1 = worst)
    pub prices: [Px; DEPTH],
    /// Quantities at each level
    pub qtys: [Qty; DEPTH],
    /// Number of valid levels
    pub depth: usize,
}

impl Default for SideBook {
    fn default() -> Self {
        Self {
            prices: [Px::ZERO; DEPTH],
            qtys: [Qty::ZERO; DEPTH],
            depth: 0,
        }
    }
}

impl SideBook {
    /// Create a new empty side book
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear all levels
    #[inline]
    pub fn clear(&mut self) {
        // Only clear what we need to
        for i in 0..self.depth {
            self.qtys[i] = Qty::ZERO;
        }
        self.depth = 0;
    }

    /// Set a specific level (absolute replace)
    #[inline]
    pub fn set(&mut self, level: usize, price: Px, qty: Qty) {
        if level >= DEPTH {
            return;
        }

        if qty.is_zero() {
            // Remove level
            self.remove_level(level);
        } else {
            self.prices[level] = price;
            self.qtys[level] = qty;
            if level >= self.depth {
                self.depth = level + 1;
            }
        }
    }

    /// Remove a level and shift others up
    #[inline]
    fn remove_level(&mut self, level: usize) {
        if level >= self.depth {
            return;
        }

        // Shift levels up
        for i in level..self.depth.saturating_sub(1) {
            self.prices[i] = self.prices[i + 1];
            self.qtys[i] = self.qtys[i + 1];
        }

        if self.depth > 0 {
            self.depth -= 1;
            // Clear the last level
            if self.depth < DEPTH {
                self.qtys[self.depth] = Qty::ZERO;
            }
        }
    }

    /// Get the best price and quantity
    #[inline]
    pub fn best(&self) -> Option<(Px, Qty)> {
        if self.depth > 0 && !self.qtys[0].is_zero() {
            Some((self.prices[0], self.qtys[0]))
        } else {
            None
        }
    }

    /// Get price at a specific level
    #[inline]
    pub fn price_at(&self, level: usize) -> Option<Px> {
        if level < self.depth && !self.qtys[level].is_zero() {
            Some(self.prices[level])
        } else {
            None
        }
    }

    /// Get quantity at a specific level
    #[inline]
    pub fn qty_at(&self, level: usize) -> Option<Qty> {
        if level < self.depth && !self.qtys[level].is_zero() {
            Some(self.qtys[level])
        } else {
            None
        }
    }

    /// Get total quantity up to a certain depth
    #[inline]
    pub fn total_qty(&self, max_depth: usize) -> Qty {
        let limit = max_depth.min(self.depth);
        let mut total = 0i64;

        // Unroll for better performance
        for i in 0..limit {
            total += self.qtys[i].as_i64();
        }

        Qty::from_i64(total)
    }

    /// Check if side is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.depth == 0 || self.qtys[0].is_zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sidebook_operations() {
        let mut book = SideBook::new();
        assert!(book.is_empty());

        // Add some levels
        book.set(0, Px::new(100.0), Qty::new(10.0));
        book.set(1, Px::new(99.5), Qty::new(20.0));
        book.set(2, Px::new(99.0), Qty::new(30.0));

        assert_eq!(book.depth, 3);
        assert_eq!(book.best(), Some((Px::new(100.0), Qty::new(10.0))));

        // Remove middle level
        book.set(1, Px::new(99.5), Qty::ZERO);
        assert_eq!(book.depth, 2);
        assert_eq!(book.price_at(1), Some(Px::new(99.0)));

        // Clear
        book.clear();
        assert!(book.is_empty());
    }

    #[test]
    fn test_total_qty() {
        let mut book = SideBook::new();
        book.set(0, Px::new(100.0), Qty::new(10.0));
        book.set(1, Px::new(99.5), Qty::new(20.0));
        book.set(2, Px::new(99.0), Qty::new(30.0));

        assert!((book.total_qty(2).as_f64() - 30.0).abs() < f64::EPSILON); // 10 + 20
        assert!((book.total_qty(10).as_f64() - 60.0).abs() < f64::EPSILON); // All levels
    }
}
