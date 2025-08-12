//! Ultra-Low Latency Trading Engine
//!
//! PERFORMANCE FIRST: Zero-copy, lock-free, cache-optimized design
//!
//! Key Design Principles:
//! - NO ALLOCATIONS in hot path
//! - NO LOCKS - only atomics and lock-free structures
//! - Cache-line aligned data structures (64 bytes)
//! - Memory pools for all objects
//! - SIMD operations for calculations
//! - Branch-free code in critical paths
//!
//! Execution Modes (zero-cost abstraction):
//! - Paper: In-memory execution simulation
//! - Live: Direct market execution
//! - Backtest: Historical replay with same code
//!
//! Venues (compile-time polymorphism):
//! - Zerodha (NSE/BSE)
//! - Binance (Crypto)

#![deny(warnings)]
#![deny(clippy::all)]

pub mod core;
pub mod execution;
pub mod memory;
pub mod metrics;
pub mod position;
pub mod risk;
pub mod venue;

// Re-exports
pub use core::{Engine, EngineConfig, ExecutionMode};
pub use execution::{ExecutionLayer, Order, OrderPool};
pub use memory::ObjectPool;
pub use metrics::{MetricsEngine, PnL};
pub use position::{PositionCache, PositionTracker};
pub use risk::{RiskEngine, RiskLimits};
pub use venue::VenueAdapter;
