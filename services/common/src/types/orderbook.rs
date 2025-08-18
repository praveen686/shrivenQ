//! Order book types

use crate::{Px, Qty, Symbol};
use serde::{Deserialize, Serialize};
use rustc_hash::FxHashMap;

/// Order book level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Level {
    /// Price
    pub price: Px,
    /// Quantity
    pub quantity: Qty,
}

/// Order book V2 implementation
#[derive(Debug, Clone)]
pub struct OrderBookV2 {
    /// Symbol
    pub symbol: Symbol,
    /// Bid levels
    pub bids: Vec<Level>,
    /// Ask levels
    pub asks: Vec<Level>,
    /// ROI width
    pub roi_width: f64,
    /// Center price
    pub center_price: f64,
}

impl OrderBookV2 {
    /// Create new order book with ROI
    #[must_use] pub const fn new_with_roi(symbol: Symbol, center_price: f64, roi_width: f64) -> Self {
        Self {
            symbol,
            bids: Vec::new(),
            asks: Vec::new(),
            roi_width,
            center_price,
        }
    }

    /// Update with tick
    pub const fn update_with_tick(&mut self, _price: Px, _quantity: Qty, _is_buy: bool) {
        // Placeholder implementation during migration
    }

    /// Apply fast update
    pub fn apply_fast(&mut self, _update: impl std::fmt::Debug) {
        // Placeholder implementation during migration
    }

    /// Apply validated update
    pub fn apply_validated(&mut self, _update: impl std::fmt::Debug) -> anyhow::Result<()> {
        // Placeholder implementation during migration
        Ok(())
    }

    /// Get best bid
    #[must_use] pub fn best_bid(&self) -> Option<(Px, Qty)> {
        self.bids.first().map(|level| (level.price, level.quantity))
    }

    /// Get best ask
    #[must_use] pub fn best_ask(&self) -> Option<(Px, Qty)> {
        self.asks.first().map(|level| (level.price, level.quantity))
    }
}

/// Feature calculator V2 with fixed-point arithmetic
#[derive(Debug)]
pub struct FeatureCalculatorV2Fixed {
    /// Symbol
    pub symbol: Symbol,
    /// Features cache
    pub features: FxHashMap<String, f64>,
}

impl FeatureCalculatorV2Fixed {
    /// Create new feature calculator
    #[must_use] pub fn new(symbol: Symbol) -> Self {
        Self {
            symbol,
            features: FxHashMap::default(),
        }
    }

    /// Update features with order book
    pub const fn update_features(&mut self, _orderbook: &OrderBookV2) {
        // Placeholder implementation during migration
    }

    /// Get feature value
    #[must_use] pub fn get_feature(&self, name: &str) -> Option<f64> {
        self.features.get(name).copied()
    }
}