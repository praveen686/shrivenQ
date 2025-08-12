//! Common components shared between exchanges

pub mod adapter;
pub mod event;
pub mod manager;
pub mod instrument_fetcher;

pub use adapter::{FeedAdapter, FeedConfig};
pub use event::MarketEvent;
pub use manager::{FeedManager, FeedManagerConfig};
pub use instrument_fetcher::{InstrumentFetcher, InstrumentFetcherConfig};