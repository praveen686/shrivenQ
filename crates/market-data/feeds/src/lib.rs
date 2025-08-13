//! Feed adapters for market data sources
//!
//! Organized structure:
//! - zerodha/: NSE/BSE market data via Zerodha Kite
//! - binance/: Crypto market data via Binance
//! - common/: Shared components and traits

#![deny(warnings)]
#![deny(clippy::all)]

// Exchange-specific modules
pub mod binance;
pub mod zerodha;

// Common components
pub mod common;
pub mod display_utils;

// Integration modules
pub mod integration;

// Re-exports for backward compatibility
pub use common::adapter::{FeedAdapter, FeedConfig};
pub use common::event::MarketEvent;
pub use common::instrument_fetcher::{InstrumentFetcher, InstrumentFetcherConfig};
pub use common::manager::{FeedManager, FeedManagerConfig};

// Zerodha exports
pub use zerodha::ZerodhaFeed;
pub use zerodha::config::ZerodhaConfig;
pub use zerodha::market_data_pipeline::{MarketDataPipeline, PipelineConfig};
pub use zerodha::websocket::ZerodhaWebSocketFeed;

// Binance exports
pub use binance::BinanceFeed;
pub use binance::config::BinanceConfig;
pub use binance::websocket::BinanceWebSocketFeed;
