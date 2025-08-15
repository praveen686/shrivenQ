//! # Institutional-Grade Order Book Implementation
//! 
//! A world-class order book implementation designed for ultra-low-latency trading
//! with sophisticated market microstructure analytics and deterministic replay capabilities.
//! 
//! ## Features
//! - **Lock-free operations** with atomic updates for multi-threaded access
//! - **SIMD-accelerated** computations for price levels and imbalance calculations  
//! - **L3 market data support** with individual order tracking
//! - **Deterministic replay** with nanosecond-precision event sequencing
//! - **Market microstructure analytics** including VPIN, Kyle's Lambda, and PIN
//! - **Advanced detection** for icebergs, spoofing, and toxic flow
//! - **Memory-efficient** with cache-aligned structures and zero allocations in hot path
//! 
//! ## Architecture
//! 
//! The orderbook is designed with three layers:
//! 1. **Core Layer**: Lock-free price level management with atomic operations
//! 2. **Analytics Layer**: Real-time microstructure computations with SIMD
//! 3. **Replay Layer**: Deterministic event replay with checksum validation

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod book;
pub mod depth;
pub mod events;
pub mod analytics;
pub mod replay;
pub mod metrics;

pub use book::{OrderBook, OrderBookConfig};
pub use depth::{DepthLevel, MarketDepth};
pub use events::{OrderBookEvent, OrderUpdate, TradeEvent};
pub use analytics::{MicrostructureAnalytics, ImbalanceCalculator};
pub use replay::{ReplayEngine, SnapshotManager};

/// The primary interface for the world-class orderbook
pub struct InstitutionalOrderBook {
    symbol: String,
    book: book::OrderBook,
    analytics: analytics::MicrostructureAnalytics,
    metrics: metrics::PerformanceMetrics,
}

impl InstitutionalOrderBook {
    /// Create a new institutional-grade orderbook
    pub fn new(symbol: impl Into<String>) -> Self {
        let symbol = symbol.into();
        Self {
            book: book::OrderBook::new(symbol.clone()),
            analytics: analytics::MicrostructureAnalytics::new(),
            metrics: metrics::PerformanceMetrics::new(&symbol),
            symbol,
        }
    }
}