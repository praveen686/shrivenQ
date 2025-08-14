//! Order book management module
//!
//! COMPLIANCE: Zero allocations in hot paths, fixed-point arithmetic only

mod book;
mod price_levels;

pub use book::{BookError, OrderBook};
pub use price_levels::{DEPTH, SideBook};
