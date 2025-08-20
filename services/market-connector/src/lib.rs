//! Market Connector Service
//!
//! Manages connections to multiple exchanges, handles market data subscriptions,
//! and provides normalized data feed to other services.

pub mod connectors;
pub mod exchanges;
pub mod grpc_service;
pub mod instruments;
pub mod models;
pub mod orderbook;

use anyhow::Result;
use async_trait::async_trait;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// Market data types that can be subscribed to from exchanges
///
/// This enum defines the different types of market data that can be requested
/// from various exchanges. Each type represents a specific kind of financial
/// data stream that trading systems typically need.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MarketDataType {
    /// Order book updates containing bid/ask levels
    ///
    /// Provides depth of market information showing all pending buy and sell orders
    /// at different price levels. This is essential for understanding market liquidity
    /// and price discovery.
    OrderBook,
    
    /// Individual trade execution events
    ///
    /// Real-time feed of actual trades that have been executed on the exchange,
    /// including price, quantity, and side information. Useful for trade analysis
    /// and market impact studies.
    Trades,
    
    /// Best bid and ask price quotes (Level 1 data)
    ///
    /// Provides the top-of-book information showing the best available bid and ask
    /// prices with their respective sizes. This is the most basic market data
    /// required for price display and simple trading decisions.
    Quotes,
    
    /// OHLCV candlestick/bar data
    ///
    /// Historical price bars containing Open, High, Low, Close, and Volume data
    /// for specified time intervals. Used for technical analysis and charting.
    Candles {
        /// Time interval for the candles (e.g., "1m", "5m", "1h", "1d")
        ///
        /// The interval format should follow exchange-specific conventions.
        /// Common formats include:
        /// - "1m", "5m", "15m", "30m" for minute intervals
        /// - "1h", "4h", "12h" for hourly intervals  
        /// - "1d", "1w", "1M" for daily, weekly, monthly intervals
        interval: String,
    },
}

/// Subscription request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionRequest {
    /// Exchange name
    pub exchange: String,
    /// Trading symbol
    pub symbol: String,
    /// Data types to subscribe
    pub data_types: Vec<MarketDataType>,
}

/// Market data event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDataEvent {
    /// Exchange source
    pub exchange: String,
    /// Symbol
    pub symbol: String,
    /// Event timestamp (nanoseconds)
    pub timestamp: u64,
    /// Event data
    pub data: MarketData,
}

/// Market data variants containing actual market information
///
/// This enum holds the actual market data received from exchanges.
/// Each variant corresponds to a MarketDataType and contains the
/// specific data fields relevant to that type of market information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketData {
    /// Order book snapshot or incremental update
    ///
    /// Contains the current state of the order book with bid and ask levels.
    /// Can represent either a full snapshot or an incremental update depending
    /// on the exchange's data feed implementation.
    OrderBook {
        /// List of bid levels as (price, quantity) tuples
        ///
        /// Sorted in descending order by price (highest bid first).
        /// Each tuple represents a price level and the total quantity
        /// available at that price on the buy side.
        bids: Vec<(f64, f64)>,
        
        /// List of ask levels as (price, quantity) tuples
        ///
        /// Sorted in ascending order by price (lowest ask first).
        /// Each tuple represents a price level and the total quantity
        /// available at that price on the sell side.
        asks: Vec<(f64, f64)>,
        
        /// Sequence number for ordering updates
        ///
        /// Used to ensure proper ordering of order book updates and detect
        /// any missed messages. Higher sequence numbers represent more recent data.
        sequence: u64,
    },
    
    /// Individual trade execution event
    ///
    /// Represents a single trade that was executed on the exchange,
    /// providing details about the transaction that occurred.
    Trade {
        /// Execution price of the trade
        ///
        /// The price at which the trade was executed, in the quote currency
        /// of the trading pair (e.g., USD for BTC/USD).
        price: f64,
        
        /// Quantity of the trade
        ///
        /// The amount that was traded, in the base currency of the trading pair
        /// (e.g., BTC for BTC/USD).
        quantity: f64,
        
        /// Side of the trade ("buy" or "sell")
        ///
        /// Indicates whether this trade was initiated by a buy order ("buy")
        /// or a sell order ("sell"). This represents the taker side of the trade.
        side: String,
        
        /// Unique identifier for the trade
        ///
        /// Exchange-specific trade ID that uniquely identifies this trade.
        /// Can be used for trade deduplication and reference purposes.
        trade_id: String,
    },
    
    /// Best bid and ask quote update (Level 1 market data)
    ///
    /// Provides the current best available prices on both sides of the market,
    /// representing the top of the order book.
    Quote {
        /// Best bid price
        ///
        /// The highest price at which someone is willing to buy,
        /// in the quote currency of the trading pair.
        bid_price: f64,
        
        /// Size available at the best bid price
        ///
        /// The total quantity available for purchase at the bid price,
        /// in the base currency of the trading pair.
        bid_size: f64,
        
        /// Best ask price
        ///
        /// The lowest price at which someone is willing to sell,
        /// in the quote currency of the trading pair.
        ask_price: f64,
        
        /// Size available at the best ask price
        ///
        /// The total quantity available for sale at the ask price,
        /// in the base currency of the trading pair.
        ask_size: f64,
    },
}

/// Market connector trait
#[async_trait]
pub trait MarketConnector: Send + Sync {
    /// Connect to exchange
    async fn connect(&mut self) -> Result<()>;

    /// Disconnect from exchange
    async fn disconnect(&mut self) -> Result<()>;

    /// Subscribe to market data
    async fn subscribe(&mut self, request: SubscriptionRequest) -> Result<()>;

    /// Unsubscribe from market data
    async fn unsubscribe(&mut self, symbol: &str) -> Result<()>;

    /// Get connection status
    fn is_connected(&self) -> bool;

    /// Get subscribed symbols
    fn subscribed_symbols(&self) -> Vec<String>;
}

/// Market connector service
impl std::fmt::Debug for MarketConnectorService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarketConnectorService")
            .field("connectors", &self.connectors.keys().collect::<Vec<_>>())
            .field("event_sender", &"tokio::sync::mpsc::Sender<MarketDataEvent>")
            .finish()
    }
}

/// Market connector service that manages multiple exchange connections
///
/// This service acts as a centralized hub for managing connections to multiple cryptocurrency
/// and financial exchanges. It provides a unified interface for subscribing to market data
/// across different exchanges and handles the complexity of managing multiple connector
/// instances.
///
/// # Architecture
/// - Maintains a collection of exchange-specific connectors
/// - Provides unified market data streaming through a single event channel
/// - Handles connection lifecycle management (start/stop operations)
/// - Routes market data events from individual connectors to subscribers
///
/// # Usage
/// The service is designed to be used in a multi-exchange trading system where
/// you need to aggregate market data from various sources. Each exchange has
/// its own connector implementation that handles the specific protocols and
/// authentication requirements.
///
/// # Thread Safety
/// The service is designed to be thread-safe and can handle concurrent operations
/// across multiple exchange connections.
///
/// # Examples
/// ```
/// use market_connector::{MarketConnectorService, MarketDataEvent};
/// use tokio::sync::mpsc;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let (tx, mut rx) = mpsc::channel::<MarketDataEvent>(1000);
///     let mut service = MarketConnectorService::new(tx);
///     
///     // Add connectors for different exchanges
///     // service.add_connector("binance".to_string(), binance_connector);
///     // service.add_connector("zerodha".to_string(), zerodha_connector);
///     
///     // Start all connections
///     service.start().await?;
///     
///     // Process incoming market data
///     while let Some(event) = rx.recv().await {
///         println!("Received market data: {:?}", event);
///     }
///     
///     Ok(())
/// }
/// ```
pub struct MarketConnectorService {
    /// Active connectors
    connectors: FxHashMap<String, Box<dyn MarketConnector>>,
    /// Event channel sender for streaming market data events
    event_sender: tokio::sync::mpsc::Sender<MarketDataEvent>,
}

impl MarketConnectorService {
    /// Create new service
    pub fn new(event_sender: tokio::sync::mpsc::Sender<MarketDataEvent>) -> Self {
        Self {
            connectors: FxHashMap::default(),
            event_sender,
        }
    }

    /// Add connector for exchange
    pub fn add_connector(&mut self, exchange: String, connector: Box<dyn MarketConnector>) {
        self.connectors.insert(exchange, connector);
    }

    /// Start all connectors
    pub async fn start(&mut self) -> Result<()> {
        for (exchange, connector) in &mut self.connectors {
            tracing::info!("Connecting to {}", exchange);
            connector.connect().await?;
        }
        Ok(())
    }

    /// Stop all connectors
    pub async fn stop(&mut self) -> Result<()> {
        for (exchange, connector) in &mut self.connectors {
            tracing::info!("Disconnecting from {}", exchange);
            connector.disconnect().await?;
        }
        Ok(())
    }

    /// Send market data event to subscribers
    pub async fn send_event(&self, event: MarketDataEvent) -> Result<()> {
        self.event_sender
            .send(event)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send market data event: {}", e))?;
        Ok(())
    }

    /// Process and forward market data from connectors
    pub async fn process_market_data(
        &self,
        exchange: &str,
        symbol: &str,
        data: MarketDataEvent,
    ) -> Result<()> {
        tracing::debug!("Processing market data from {} for {}", exchange, symbol);
        self.send_event(data).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_request() {
        let request = SubscriptionRequest {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            data_types: vec![MarketDataType::OrderBook, MarketDataType::Trades],
        };

        assert_eq!(request.exchange, "binance");
        assert_eq!(request.data_types.len(), 2);
    }
}
