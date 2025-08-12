//! Canonical market data types for L2 order book

use serde::{Deserialize, Serialize};

/// Trading side
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Side {
    /// Buy side (bid)
    Bid,
    /// Sell side (ask/offer)
    Ask,
}

/// Normalized L2 update (absolute replace at price level)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct L2Update {
    /// Event timestamp in nanoseconds
    pub ts: crate::Ts,
    /// Trading symbol
    pub symbol: crate::Symbol,
    /// Side of the book
    pub side: Side,
    /// Price level
    pub price: crate::Px,
    /// Quantity at this level (0 = remove level)
    pub qty: crate::Qty,
    /// Level index (0 = best)
    pub level: u8,
}

impl L2Update {
    /// Create a new L2 update
    #[must_use]
    pub fn new(
        ts: crate::Ts,
        symbol: crate::Symbol,
        side: Side,
        price: crate::Px,
        qty: crate::Qty,
        level: u8,
    ) -> Self {
        Self {
            ts,
            symbol,
            side,
            price,
            qty,
            level,
        }
    }

    /// Check if this update removes a level
    #[must_use]
    pub fn is_removal(&self) -> bool {
        self.qty.is_zero()
    }
}

/// Feature frame containing derived market metrics
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeatureFrame {
    /// Event timestamp
    pub ts: crate::Ts,
    /// Trading symbol
    pub symbol: crate::Symbol,
    /// Spread in price ticks
    pub spread_ticks: i64,
    /// Mid price (average of best bid/ask)
    pub mid: i64,
    /// Microprice (size-weighted mid)
    pub micro: i64,
    /// Order book imbalance (-1.0 to 1.0)
    pub imbalance: f64,
    /// VWAP deviation
    pub vwap_dev: f64,
}

impl FeatureFrame {
    /// Create a new feature frame
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ts: crate::Ts,
        symbol: crate::Symbol,
        spread_ticks: i64,
        mid: i64,
        micro: i64,
        imbalance: f64,
        vwap_dev: f64,
    ) -> Self {
        Self {
            ts,
            symbol,
            spread_ticks,
            mid,
            micro,
            imbalance,
            vwap_dev,
        }
    }
}

/// LOB update event for publishing
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LOBUpdate {
    /// Timestamp
    pub ts: crate::Ts,
    /// Symbol
    pub symbol: crate::Symbol,
    /// Best bid price
    pub bid: Option<crate::Px>,
    /// Best bid size
    pub bid_size: Option<crate::Qty>,
    /// Best ask price
    pub ask: Option<crate::Px>,
    /// Best ask size
    pub ask_size: Option<crate::Qty>,
}

impl LOBUpdate {
    /// Create a new LOB update
    #[must_use]
    pub fn new(
        ts: crate::Ts,
        symbol: crate::Symbol,
        bid: Option<crate::Px>,
        bid_size: Option<crate::Qty>,
        ask: Option<crate::Px>,
        ask_size: Option<crate::Qty>,
    ) -> Self {
        Self {
            ts,
            symbol,
            bid,
            bid_size,
            ask,
            ask_size,
        }
    }

    /// Check if book is crossed (bid >= ask)
    #[must_use]
    pub fn is_crossed(&self) -> bool {
        match (self.bid, self.ask) {
            (Some(b), Some(a)) => b.as_i64() >= a.as_i64(),
            _ => false,
        }
    }

    /// Check if book is locked (bid == ask)
    #[must_use]
    pub fn is_locked(&self) -> bool {
        match (self.bid, self.ask) {
            (Some(b), Some(a)) => b.as_i64() == a.as_i64(),
            _ => false,
        }
    }
}