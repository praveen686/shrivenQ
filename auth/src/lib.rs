//! ShrivenQ Authentication Module
//!
//! Provides authentication for Zerodha and Binance trading platforms.

#![deny(warnings)]
#![deny(clippy::all)]

// Platform-specific authentication modules
pub mod zerodha;
pub mod binance;

// Public exports with meaningful names
pub use zerodha::{
    ZerodhaAuth,
    ZerodhaConfig,
    SessionCache as ZerodhaSession,
};

pub use binance::{
    BinanceAuth,
    BinanceConfig,
    BinanceMarket,
};

// Note: AuthProvider trait removed as each platform has different auth mechanisms
// Use platform-specific implementations directly