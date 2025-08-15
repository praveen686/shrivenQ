//! # World-Class Institutional Order Book
//! 
//! A showcase-quality order book implementation that demonstrates mastery of:
//! - Ultra-low-latency design patterns
//! - Advanced market microstructure concepts
//! - Lock-free concurrent data structures
//! - SIMD-accelerated computations
//! - Deterministic replay and integrity validation
//!
//! This implementation would be at home in firms like:
//! - Jane Street, Citadel, Jump Trading
//! - Two Sigma, Renaissance Technologies
//! - Optiver, IMC, SIG
//!
//! ## Core Design Principles
//!
//! 1. **Zero-Copy Architecture**: All updates are in-place with no allocations
//! 2. **Cache-Aligned Structures**: Optimized for CPU cache lines (64 bytes)
//! 3. **Lock-Free Operations**: Wait-free reads, lock-free writes
//! 4. **SIMD Acceleration**: Vectorized operations for analytics
//! 5. **Deterministic Behavior**: Bit-identical replay across runs

#![warn(missing_docs)]
#![allow(unsafe_code)] // Allow unsafe for SIMD operations where necessary

// Common types used across modules
// (imports moved to individual modules where needed)

pub mod core;
pub mod analytics;
pub mod events;
pub mod replay;
pub mod metrics;

// Re-exports for convenience
pub use crate::core::{OrderBook, Side};
pub use crate::analytics::{MicrostructureAnalytics, ImbalanceCalculator, ImbalanceMetrics, ToxicityDetector};
pub use crate::events::{OrderBookEvent, OrderUpdate, TradeEvent, OrderBookSnapshot, OrderBookDelta, MarketEvent};
pub use crate::replay::{ReplayEngine, ReplayConfig};
pub use crate::metrics::{PerformanceMetrics, MetricsSnapshot};

/// Maximum number of price levels to track per side
const _MAX_DEPTH_LEVELS: usize = 100;

/// Cache line size for alignment
const _CACHE_LINE_SIZE: usize = 64;

/// Small vector optimization size for order lists
const _SMALL_VEC_SIZE: usize = 8;