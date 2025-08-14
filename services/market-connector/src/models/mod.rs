//! Data models for market connector service

use serde::{Deserialize, Serialize};

/// Order book level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Level {
    pub price: f64,
    pub quantity: f64,
    pub order_count: Option<u32>,
}

/// Trade information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: String,
    pub price: f64,
    pub quantity: f64,
    pub timestamp: u64,
    pub is_buyer_maker: bool,
}

/// OHLCV candle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub timestamp: u64,
    pub trades: u32,
}
