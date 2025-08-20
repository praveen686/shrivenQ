//! Trade statistics aggregator

/// Aggregator for trade statistics processing
#[derive(Debug)]
pub struct TradeStatsAggregator {
    // Implementation details
}

impl Default for TradeStatsAggregator {
    fn default() -> Self {
        Self::new()
    }
}

impl TradeStatsAggregator {
    /// Create a new trade statistics aggregator
    #[must_use] pub const fn new() -> Self {
        Self {}
    }
}
