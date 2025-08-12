//! Common components shared between exchanges

pub mod adapter;
pub mod event;
pub mod instrument_fetcher;
pub mod manager;

pub use adapter::{FeedAdapter, FeedConfig};
pub use event::MarketEvent;
pub use instrument_fetcher::{InstrumentFetcher, InstrumentFetcherConfig};
pub use manager::{FeedManager, FeedManagerConfig};
