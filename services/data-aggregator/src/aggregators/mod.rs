//! Aggregator implementations

pub mod candle;
pub mod trade_stats;
pub mod volume_profile;

pub use candle::CandleAggregator;
pub use trade_stats::TradeStatsAggregator;
pub use volume_profile::VolumeProfileAggregator;
