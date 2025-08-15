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

/// Market data types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MarketDataType {
    /// Order book updates
    OrderBook,
    /// Trade ticks
    Trades,
    /// Best bid/ask quotes
    Quotes,
    /// OHLCV bars
    Candles { interval: String },
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

/// Market data variants
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketData {
    /// Order book snapshot or update
    OrderBook {
        bids: Vec<(f64, f64)>,
        asks: Vec<(f64, f64)>,
        sequence: u64,
    },
    /// Trade event
    Trade {
        price: f64,
        quantity: f64,
        side: String,
        trade_id: String,
    },
    /// Quote update
    Quote {
        bid_price: f64,
        bid_size: f64,
        ask_price: f64,
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
