//! Feed adapters for market data sources
//!
//! Organized structure:
//! - zerodha/: NSE/BSE market data via Zerodha Kite
//! - binance/: Crypto market data via Binance
//! - common/: Shared components and traits

#![deny(warnings)]
#![deny(clippy::all)]

// Exchange-specific modules
pub mod zerodha;
pub mod binance;

// Common components
pub mod common;


// Re-exports for backward compatibility
pub use common::adapter::{FeedAdapter, FeedConfig};
pub use common::event::MarketEvent;
pub use common::manager::{FeedManager, FeedManagerConfig};
pub use common::instrument_fetcher::{InstrumentFetcher, InstrumentFetcherConfig};

// Zerodha exports
pub use zerodha::ZerodhaFeed;
pub use zerodha::websocket::ZerodhaWebSocketFeed;
pub use zerodha::config::ZerodhaConfig;
pub use zerodha::market_data_pipeline::{MarketDataPipeline, PipelineConfig};

// Binance exports  
pub use binance::BinanceFeed;
pub use binance::websocket::BinanceWebSocketFeed;
pub use binance::config::BinanceConfig;