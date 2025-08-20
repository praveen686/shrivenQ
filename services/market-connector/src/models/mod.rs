//! Data models for market connector service

use serde::{Deserialize, Serialize};

/// Order book level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Level {
    /// Price level in quote currency (e.g., USDT for BTCUSDT)
    pub price: f64,
    
    /// Total quantity available at this price level in base currency
    pub quantity: f64,
    
    /// Number of individual orders at this price level (if available from exchange)
    pub order_count: Option<u32>,
}

/// Trade information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    /// Unique trade identifier from the exchange
    pub id: String,
    
    /// Execution price in quote currency
    pub price: f64,
    
    /// Traded quantity in base currency
    pub quantity: f64,
    
    /// Trade execution timestamp in milliseconds since Unix epoch
    pub timestamp: u64,
    
    /// Whether the buyer was the market maker (true) or taker (false)
    pub is_buyer_maker: bool,
}

/// OHLCV candle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    /// Opening price at the start of the time period
    pub open: f64,
    
    /// Highest price during the time period
    pub high: f64,
    
    /// Lowest price during the time period
    pub low: f64,
    
    /// Closing price at the end of the time period
    pub close: f64,
    
    /// Total trading volume in base currency during the time period
    pub volume: f64,
    
    /// Candle period start timestamp in milliseconds since Unix epoch
    pub timestamp: u64,
    
    /// Total number of trades executed during the time period
    pub trades: u32,
}
