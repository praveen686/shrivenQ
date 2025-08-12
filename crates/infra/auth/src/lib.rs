//! `ShrivenQ` Authentication Module
//!
//! Provides authentication for Zerodha and Binance trading platforms.

#![deny(warnings)]
#![deny(clippy::all)]
#![allow(clippy::multiple_crate_versions)]

// Platform-specific authentication modules
pub mod binance;
pub mod zerodha;

// Public exports with meaningful names
pub use zerodha::{SessionCache as ZerodhaSession, ZerodhaAuth, ZerodhaConfig};

pub use binance::{BinanceAuth, BinanceConfig, BinanceMarket};

// Note: AuthProvider trait removed as each platform has different auth mechanisms
// Use platform-specific implementations directly
