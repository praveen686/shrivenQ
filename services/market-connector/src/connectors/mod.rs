//! Connector implementations for different exchanges

pub mod adapter;

use crate::{MarketConnector, SubscriptionRequest};
use anyhow::Result;
use async_trait::async_trait;
use auth::ZerodhaAuth;
use std::sync::Arc;

/// Generic connector wrapper that implements MarketConnector trait
pub struct GenericConnector {
    exchange: String,
    zerodha_auth: Option<Arc<ZerodhaAuth>>,
    connected: bool,
    subscribed_symbols: Vec<String>,
}

impl GenericConnector {
    pub fn new(exchange: String) -> Self {
        Self {
            exchange,
            zerodha_auth: None,
            connected: false,
            subscribed_symbols: Vec::new(),
        }
    }

    pub fn with_zerodha_auth(mut self, auth: ZerodhaAuth) -> Self {
        self.zerodha_auth = Some(Arc::new(auth));
        self
    }
}

#[async_trait]
impl MarketConnector for GenericConnector {
    async fn connect(&mut self) -> Result<()> {
        match self.exchange.as_str() {
            "zerodha" => {
                if let Some(auth) = &self.zerodha_auth {
                    // Authenticate to verify connection
                    auth.authenticate().await?;
                    self.connected = true;
                    tracing::info!("Zerodha connector authenticated successfully");
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Zerodha auth not configured"))
                }
            }
            "binance" => {
                // Binance public endpoints don't require authentication for market data
                self.connected = true;
                tracing::info!("Binance connector ready (public data)");
                Ok(())
            }
            _ => Err(anyhow::anyhow!("Unsupported exchange: {}", self.exchange)),
        }
    }

    async fn disconnect(&mut self) -> Result<()> {
        tracing::info!("Disconnecting {} connector", self.exchange);
        self.connected = false;
        self.subscribed_symbols.clear();
        Ok(())
    }

    async fn subscribe(&mut self, request: SubscriptionRequest) -> Result<()> {
        if !self.connected {
            return Err(anyhow::anyhow!("Connector not connected"));
        }

        // Add symbol to subscribed list if not already present
        if !self.subscribed_symbols.contains(&request.symbol) {
            self.subscribed_symbols.push(request.symbol.clone());
        }

        tracing::info!(
            "Generic connector subscribed to {} on {}",
            request.symbol,
            request.exchange
        );
        Ok(())
    }

    async fn unsubscribe(&mut self, symbol: &str) -> Result<()> {
        // Remove symbol from subscribed list
        self.subscribed_symbols.retain(|s| s != symbol);
        tracing::info!(
            "Generic connector unsubscribed from {} on {}",
            symbol,
            self.exchange
        );
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn subscribed_symbols(&self) -> Vec<String> {
        self.subscribed_symbols.clone()
    }
}

/// Factory for creating exchange connectors
///
/// This factory creates generic connector wrappers that can be used with the
/// MarketConnectorService. For actual market data streaming, specialized feed
/// adapters (ZerodhaFeed, BinanceFeed) should be used directly as they provide
/// optimized implementations for each exchange's specific protocols.
pub struct ConnectorFactory;

impl ConnectorFactory {
    /// Create connector for exchange
    ///
    /// Returns a generic connector that implements the MarketConnector trait.
    /// This is useful for service orchestration and connection management.
    ///
    /// For high-performance market data streaming, use specialized feed adapters:
    /// - ZerodhaFeed for Zerodha WebSocket streaming with binary protocol parsing
    /// - BinanceFeed for Binance WebSocket streaming
    ///
    /// # Arguments
    /// * `exchange` - Exchange name ("zerodha", "binance")
    ///
    /// # Returns
    /// * `Some(Box<dyn MarketConnector>)` - Generic connector wrapper
    /// * `None` - If exchange is not supported
    pub fn create(exchange: &str) -> Option<Box<dyn MarketConnector>> {
        match exchange {
            "binance" => {
                // Create Binance generic connector
                // For actual streaming, use BinanceFeed directly
                Some(Box::new(GenericConnector::new("binance".to_string())))
            }
            "zerodha" => {
                // Create Zerodha generic connector
                // For actual streaming with authentication, use ZerodhaFeed directly
                Some(Box::new(GenericConnector::new("zerodha".to_string())))
            }
            _ => None,
        }
    }

    /// Create Zerodha connector with authentication
    ///
    /// Creates a Zerodha connector with proper authentication configured.
    /// This is useful for connection testing and service management.
    ///
    /// # Arguments
    /// * `auth` - ZerodhaAuth instance with credentials
    ///
    /// # Returns
    /// * `Box<dyn MarketConnector>` - Authenticated Zerodha connector
    pub fn create_zerodha_with_auth(auth: ZerodhaAuth) -> Box<dyn MarketConnector> {
        Box::new(GenericConnector::new("zerodha".to_string()).with_zerodha_auth(auth))
    }

    /// Get list of supported exchanges
    ///
    /// # Returns
    /// * `Vec<&'static str>` - List of supported exchange names
    pub fn supported_exchanges() -> Vec<&'static str> {
        vec!["zerodha", "binance"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory_create_supported_exchanges() {
        let zerodha = ConnectorFactory::create("zerodha");
        let binance = ConnectorFactory::create("binance");

        assert!(zerodha.is_some());
        assert!(binance.is_some());
    }

    #[test]
    fn test_factory_create_unsupported_exchange() {
        let unsupported = ConnectorFactory::create("unknown");
        assert!(unsupported.is_none());
    }

    #[test]
    fn test_supported_exchanges_list() {
        let exchanges = ConnectorFactory::supported_exchanges();
        assert_eq!(exchanges, vec!["zerodha", "binance"]);
    }

    #[tokio::test]
    async fn test_generic_connector_binance() {
        let mut connector = GenericConnector::new("binance".to_string());

        // Initially not connected
        assert!(!connector.is_connected());
        assert_eq!(connector.subscribed_symbols().len(), 0);

        // Binance should connect without auth for public data
        assert!(connector.connect().await.is_ok());
        assert!(connector.is_connected());

        // Should be able to subscribe
        let request = SubscriptionRequest {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            data_types: vec![crate::MarketDataType::OrderBook],
        };
        assert!(connector.subscribe(request).await.is_ok());
        assert_eq!(connector.subscribed_symbols().len(), 1);

        // Should be able to unsubscribe
        assert!(connector.unsubscribe("BTCUSDT").await.is_ok());
        assert_eq!(connector.subscribed_symbols().len(), 0);

        // Should disconnect properly
        assert!(connector.disconnect().await.is_ok());
        assert!(!connector.is_connected());
    }
}
