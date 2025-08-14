//! Price level management for order book sides
//!
//! COMPLIANCE:
//! - Zero allocations in hot paths
//! - Fixed-size arrays for predictable performance
//! - Cache-aligned structure-of-arrays design

use common::{Px, Qty};

/// Fixed depth for order book (32 levels per side)
/// Chosen for cache efficiency and typical market depth
pub const DEPTH: usize = 32;

/// One side of the order book (bid or ask)
///
/// Structure-of-arrays design for cache efficiency:
/// - All prices in one contiguous array
/// - All quantities in another contiguous array
/// - CPU can vectorize operations and prefetch efficiently
///
/// Performance: O(1) best price, O(n) level operations
#[derive(Clone, Debug)]
#[repr(C)] // Predictable memory layout
pub struct SideBook {
    /// Price levels (0 = best, DEPTH-1 = worst)
    pub prices: [Px; DEPTH],
    /// Quantities at each level
    pub qtys: [Qty; DEPTH],
    /// Number of valid levels
    pub depth: usize,
}

impl Default for SideBook {
    #[inline]
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
    /// No allocations - stack only
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear all levels
    /// Performance: O(n) where n = current depth
    #[inline]
    pub fn clear(&mut self) {
        // Only clear what we need to
        for i in 0..self.depth {
            self.qtys[i] = Qty::ZERO;
        }
        self.depth = 0;
    }

    /// Set a specific level (absolute replace)
    /// Performance: O(1) for add, O(n) for remove
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
    /// Performance: O(n) where n = depth - level
    #[inline]
    fn remove_level(&mut self, level: usize) {
        if level >= self.depth {
            return;
        }

        // Shift levels up - use memmove for efficiency
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
    /// Performance: O(1)
    #[inline(always)]
    pub fn best(&self) -> Option<(Px, Qty)> {
        if self.depth > 0 && !self.qtys[0].is_zero() {
            Some((self.prices[0], self.qtys[0]))
        } else {
            None
        }
    }

    /// Get price at a specific level
    /// Performance: O(1)
    #[inline]
    pub fn price_at(&self, level: usize) -> Option<Px> {
        if level < self.depth && !self.qtys[level].is_zero() {
            Some(self.prices[level])
        } else {
            None
        }
    }

    /// Get quantity at a specific level
    /// Performance: O(1)
    #[inline]
    pub fn qty_at(&self, level: usize) -> Option<Qty> {
        if level < self.depth && !self.qtys[level].is_zero() {
            Some(self.qtys[level])
        } else {
            None
        }
    }

    /// Get total quantity up to a certain depth
    /// Performance: O(n) where n = min(max_depth, depth)
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
    /// Performance: O(1)
    #[inline(always)]
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
        book.set(0, Px::from_i64(1000000), Qty::from_i64(100000));
        book.set(1, Px::from_i64(995000), Qty::from_i64(200000));
        book.set(2, Px::from_i64(990000), Qty::from_i64(300000));

        assert_eq!(book.depth, 3);
        assert_eq!(
            book.best(),
            Some((Px::from_i64(1000000), Qty::from_i64(100000)))
        );

        // Remove middle level
        book.set(1, Px::from_i64(995000), Qty::ZERO);
        assert_eq!(book.depth, 2);
        assert_eq!(book.price_at(1), Some(Px::from_i64(990000)));

        // Clear
        book.clear();
        assert!(book.is_empty());
    }

    #[test]
    fn test_total_qty() {
        let mut book = SideBook::new();
        book.set(0, Px::from_i64(1000000), Qty::from_i64(100000));
        book.set(1, Px::from_i64(995000), Qty::from_i64(200000));
        book.set(2, Px::from_i64(990000), Qty::from_i64(300000));

        assert_eq!(book.total_qty(2).as_i64(), 300000); // 10 + 20
        assert_eq!(book.total_qty(10).as_i64(), 600000); // All levels
    }
}
