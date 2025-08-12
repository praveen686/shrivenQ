//! Ultra-fast Limit Order Book implementation
//!
//! Cache-friendly design with fixed-depth arrays for sub-200ns updates

#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::cargo)]
#![allow(clippy::multiple_crate_versions)] // Handled by cargo-deny configuration
#![deny(dead_code)]
#![deny(unused)]
#![deny(missing_docs)]
#![allow(unsafe_code)] // Allowed for SIMD optimizations in v2

pub mod adapters;
pub mod book;
pub mod features;
pub mod features_v2;
pub mod loaders;
pub mod price_levels;
pub mod v2;

pub use book::OrderBook;
pub use features::FeatureCalculator;
pub use features_v2::{FeatureCalculatorV2, FeatureFrameV2, MarketRegime};
pub use price_levels::{DEPTH, SideBook};
pub use v2::{CrossResolution, LobV2Error, OrderBookV2, SideBookV2};
