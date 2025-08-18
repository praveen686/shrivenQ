//! Trade statistics aggregator

pub struct TradeStatsAggregator {
    // Implementation details
}

impl Default for TradeStatsAggregator {
    fn default() -> Self {
        Self::new()
    }
}

impl TradeStatsAggregator {
    #[must_use] pub const fn new() -> Self {
        Self {}
    }
}
