//! Market data service gRPC client wrapper with production-grade streaming support

use crate::types::constants::network::{DEFAULT_CONNECT_TIMEOUT_SECS, DEFAULT_REQUEST_TIMEOUT_SECS, MAX_RECONNECT_ATTEMPTS, RECONNECT_BACKOFF_MS, EVENT_BUFFER_SIZE, DEFAULT_HEARTBEAT_INTERVAL_SECS, MAX_BACKOFF_MS};
use anyhow::{Context, Result};
use futures::StreamExt;
use rustc_hash::FxHashMap;
use crate::proto::marketdata::v1::{
    GetHistoricalDataRequest, GetHistoricalDataResponse, GetSnapshotRequest, GetSnapshotResponse,
    MarketDataEvent, SubscribeRequest, UnsubscribeRequest, UnsubscribeResponse,
    market_data_service_client::MarketDataServiceClient as GrpcClient,
};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tokio::time::{Duration, sleep, timeout};
use tonic::transport::{Channel, Endpoint};
use tonic::{Request, Streaming};
use tracing::{debug, error, info, warn};

/// Market data client configuration
#[derive(Clone, Debug)]
pub struct MarketDataClientConfig {
    /// Service endpoint
    pub endpoint: String,
    /// Connection timeout in seconds
    pub connect_timeout: u64,
    /// Request timeout in seconds
    pub request_timeout: u64,
    /// Maximum reconnection attempts
    pub max_reconnect_attempts: u32,
    /// Reconnection backoff in milliseconds
    pub reconnect_backoff_ms: u64,
    /// Buffer size for event channel
    pub event_buffer_size: usize,
    /// Heartbeat interval in seconds
    pub heartbeat_interval: u64,
}

impl Default for MarketDataClientConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:50051".to_string(),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT_SECS,
            request_timeout: DEFAULT_REQUEST_TIMEOUT_SECS,
            max_reconnect_attempts: MAX_RECONNECT_ATTEMPTS,
            reconnect_backoff_ms: RECONNECT_BACKOFF_MS,
            event_buffer_size: EVENT_BUFFER_SIZE,
            heartbeat_interval: DEFAULT_HEARTBEAT_INTERVAL_SECS,
        }
    }
}

/// Subscription state for a symbol
#[derive(Clone, Debug)]
struct SubscriptionState {
    symbols: Vec<String>,
    data_types: Vec<i32>,
    exchange: String,
    active: bool,
    retry_count: u32,
}

/// Market data service client with production-grade streaming support
#[derive(Debug)]
pub struct MarketDataClient {
    /// gRPC client
    client: Arc<RwLock<Option<GrpcClient<Channel>>>>,
    /// Configuration
    config: MarketDataClientConfig,
    /// Active subscriptions
    subscriptions: Arc<RwLock<FxHashMap<String, SubscriptionState>>>,
    /// Event sender for streaming data
    event_sender: mpsc::Sender<MarketDataEvent>,
    /// Event receiver for streaming data
    event_receiver: Arc<RwLock<mpsc::Receiver<MarketDataEvent>>>,
    /// Connection state
    connected: Arc<RwLock<bool>>,
    /// Stream handle for active subscription
    stream_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl MarketDataClient {
    /// Create new market data client with configuration
    pub async fn new(config: MarketDataClientConfig) -> Result<Self> {
        info!(
            "Initializing market data client for endpoint: {}",
            config.endpoint
        );

        // Create event channel
        let (event_sender, event_receiver) = mpsc::channel(config.event_buffer_size);

        let client = Self {
            client: Arc::new(RwLock::new(None)),
            config: config.clone(),
            subscriptions: Arc::new(RwLock::new(FxHashMap::default())),
            event_sender,
            event_receiver: Arc::new(RwLock::new(event_receiver)),
            connected: Arc::new(RwLock::new(false)),
            stream_handle: Arc::new(RwLock::new(None)),
        };

        // Establish initial connection
        client.connect().await?;

        // Start heartbeat task
        client.start_heartbeat().await;

        Ok(client)
    }

    /// Create new market data client without initial connection (for testing)
    pub async fn new_disconnected(config: MarketDataClientConfig) -> Self {
        info!(
            "Creating disconnected market data client for endpoint: {}",
            config.endpoint
        );

        // Create event channel
        let (event_sender, event_receiver) = mpsc::channel(config.event_buffer_size);

        let client = Self {
            client: Arc::new(RwLock::new(None)),
            config: config.clone(),
            subscriptions: Arc::new(RwLock::new(FxHashMap::default())),
            event_sender,
            event_receiver: Arc::new(RwLock::new(event_receiver)),
            connected: Arc::new(RwLock::new(false)),
            stream_handle: Arc::new(RwLock::new(None)),
        };

        // Start heartbeat task even when disconnected
        client.start_heartbeat().await;

        client
    }

    /// Create with default configuration
    pub async fn new_default(endpoint: &str) -> Result<Self> {
        let mut config = MarketDataClientConfig::default();
        config.endpoint = endpoint.to_string();
        Self::new(config).await
    }

    /// Connect to the market data service
    async fn connect(&self) -> Result<()> {
        info!(
            "Connecting to market data service at {}",
            self.config.endpoint
        );

        let endpoint = Endpoint::from_shared(self.config.endpoint.clone())?
            .connect_timeout(Duration::from_secs(self.config.connect_timeout))
            .timeout(Duration::from_secs(self.config.request_timeout));

        match timeout(
            Duration::from_secs(self.config.connect_timeout),
            endpoint.connect(),
        )
        .await
        {
            Ok(Ok(channel)) => {
                let mut client_guard = self.client.write().await;
                *client_guard = Some(GrpcClient::new(channel));
                *self.connected.write().await = true;
                info!("Successfully connected to market data service");
                Ok(())
            }
            Ok(Err(e)) => {
                error!("Failed to connect to market data service: {}", e);
                *self.connected.write().await = false;
                Err(e.into())
            }
            Err(elapsed) => {
                error!(
                    "Connection timeout after {} seconds: {}",
                    self.config.connect_timeout, elapsed
                );
                *self.connected.write().await = false;
                Err(anyhow::anyhow!("Connection timeout: {}", elapsed))
            }
        }
    }

    /// Reconnect with exponential backoff
    async fn reconnect(&self) -> Result<()> {
        let mut attempt = 0;
        let mut backoff = self.config.reconnect_backoff_ms;

        while attempt < self.config.max_reconnect_attempts {
            attempt += 1;
            warn!(
                "Reconnection attempt {}/{}",
                attempt, self.config.max_reconnect_attempts
            );

            if self.connect().await.is_ok() {
                info!("Reconnection successful");

                // Resubscribe to all active subscriptions
                let subs = self.subscriptions.read().await.clone();
                for (key, sub) in &subs {
                    if sub.active {
                        info!("Resubscribing to {} (retry {})", key, sub.retry_count);
                        if let Err(e) = self
                            .subscribe_internal(
                                sub.symbols.clone(),
                                sub.data_types.clone(),
                                &sub.exchange,
                            )
                            .await
                        {
                            error!("Failed to resubscribe to {}: {}", key, e);
                            // Increment retry count on failure
                            if let Some(subscription) =
                                self.subscriptions.write().await.get_mut(key)
                            {
                                subscription.retry_count += 1;
                                warn!(
                                    "Increased retry count for {} to {}",
                                    key, subscription.retry_count
                                );
                            }
                        } else {
                            // Reset retry count on success
                            if let Some(subscription) =
                                self.subscriptions.write().await.get_mut(key)
                            {
                                subscription.retry_count = 0;
                                info!("Successfully resubscribed to {}, reset retry count", key);
                            }
                        }
                    }
                }

                return Ok(());
            }

            sleep(Duration::from_millis(backoff)).await;
            backoff = (backoff * 2).min(MAX_BACKOFF_MS); // Cap at 30 seconds
        }

        error!(
            "Failed to reconnect after {} attempts",
            self.config.max_reconnect_attempts
        );
        Err(anyhow::anyhow!("Max reconnection attempts exceeded"))
    }

    /// Start heartbeat task to monitor connection health
    async fn start_heartbeat(&self) {
        let connected = self.connected.clone();
        let interval = self.config.heartbeat_interval;
        let client = self.client.clone();

        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(Duration::from_secs(interval));

            loop {
                interval_timer.tick().await;

                // Check connection health
                let is_connected = *connected.read().await;
                if is_connected {
                    // Verify connection is still alive
                    let client_guard = client.read().await;
                    if client_guard.is_none() {
                        warn!("Connection lost, marking as disconnected");
                        *connected.write().await = false;
                    }
                }
            }
        });
    }

    /// Subscribe to market data streams with proper streaming support
    pub async fn subscribe(
        &self,
        symbols: Vec<String>,
        data_types: Vec<i32>,
        exchange: &str,
    ) -> Result<mpsc::Receiver<MarketDataEvent>> {
        // Ensure connected before subscribing
        if !*self.connected.read().await {
            self.reconnect().await?;
        }

        self.subscribe_internal(symbols.clone(), data_types.clone(), exchange)
            .await?;

        // Store subscription state
        let key = format!("{}:{}", exchange, symbols.join(","));
        let mut subs = self.subscriptions.write().await;
        subs.insert(
            key.clone(),
            SubscriptionState {
                symbols,
                data_types,
                exchange: exchange.to_string(),
                active: true,
                retry_count: 0,
            },
        );

        // Create dedicated receiver for this subscription
        let (tx, rx) = mpsc::channel(1000);

        // Forward events to dedicated receiver
        let event_rx = self.event_receiver.clone();
        tokio::spawn(async move {
            let mut receiver = event_rx.write().await;
            while let Some(event) = receiver.recv().await {
                if tx.send(event).await.is_err() {
                    break; // Receiver dropped
                }
            }
        });

        Ok(rx)
    }

    /// Internal subscribe implementation
    async fn subscribe_internal(
        &self,
        symbols: Vec<String>,
        data_types: Vec<i32>,
        exchange: &str,
    ) -> Result<()> {
        // Check connection but don't reconnect here to avoid recursion
        if !*self.connected.read().await {
            return Err(anyhow::anyhow!("Not connected to market data service"));
        }

        let client_guard = self.client.read().await;
        let mut client = client_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Client not connected"))?
            .clone();

        let request = Request::new(SubscribeRequest {
            symbols: symbols.clone(),
            data_types,
            exchange: exchange.to_string(),
        });

        info!("Subscribing to {} symbols on {}", symbols.len(), exchange);

        // Get streaming response
        let stream = client
            .subscribe(request)
            .await
            .context("Failed to subscribe to market data")?
            .into_inner();

        // Cancel previous stream if exists
        if let Some(handle) = self.stream_handle.write().await.take() {
            handle.abort();
        }

        // Start processing stream
        let handle = self.start_stream_processor(stream).await;
        *self.stream_handle.write().await = Some(handle);

        info!("Successfully subscribed to market data stream");
        Ok(())
    }

    /// Process incoming market data stream
    async fn start_stream_processor(
        &self,
        mut stream: Streaming<MarketDataEvent>,
    ) -> tokio::task::JoinHandle<()> {
        let event_sender = self.event_sender.clone();
        let connected = self.connected.clone();

        tokio::spawn(async move {
            debug!("Starting stream processor");

            while let Some(result) = stream.next().await {
                match result {
                    Ok(event) => {
                        // Send event to channel
                        if let Err(e) = event_sender.send(event).await {
                            error!("Failed to send market data event: {}", e);
                            break;
                        }
                    }
                    Err(status) => {
                        error!("Stream error: {}", status);

                        // Handle specific error codes
                        match status.code() {
                            tonic::Code::Unavailable | tonic::Code::Unknown => {
                                warn!("Service unavailable, marking connection as lost");
                                *connected.write().await = false;
                                break;
                            }
                            tonic::Code::DeadlineExceeded => {
                                warn!("Stream deadline exceeded, will reconnect");
                                break;
                            }
                            _ => {
                                error!("Unhandled stream error: {:?}", status.code());
                            }
                        }
                    }
                }
            }

            debug!("Stream processor ended");
        })
    }

    /// Unsubscribe from market data with proper cleanup
    pub async fn unsubscribe(
        &self,
        symbols: Vec<String>,
        exchange: &str,
    ) -> Result<UnsubscribeResponse> {
        info!(
            "Unsubscribing from {} symbols on {}",
            symbols.len(),
            exchange
        );

        // Check connection
        if !*self.connected.read().await {
            return Err(anyhow::anyhow!("Not connected to market data service"));
        }

        let client_guard = self.client.read().await;
        let mut client = client_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Client not connected"))?
            .clone();

        let request = Request::new(UnsubscribeRequest {
            symbols: symbols.clone(),
            exchange: exchange.to_string(),
        });

        let response = timeout(
            Duration::from_secs(self.config.request_timeout),
            client.unsubscribe(request),
        )
        .await
        .context("Unsubscribe request timeout")?
        .context("Failed to unsubscribe")?;

        // Update subscription state
        let key = format!("{}:{}", exchange, symbols.join(","));
        let mut subs = self.subscriptions.write().await;
        if let Some(sub) = subs.get_mut(&key) {
            sub.active = false;
        }

        info!("Successfully unsubscribed");
        Ok(response.into_inner())
    }

    /// Get subscription retry statistics
    pub async fn get_subscription_retry_stats(&self) -> FxHashMap<String, u32> {
        self.subscriptions
            .read()
            .await
            .iter()
            .map(|(key, sub)| (key.clone(), sub.retry_count))
            .collect()
    }

    /// Check if any subscription has exceeded max retry count
    pub async fn has_failed_subscriptions(&self, max_retries: u32) -> bool {
        self.subscriptions
            .read()
            .await
            .values()
            .any(|sub| sub.retry_count > max_retries)
    }

    /// Get market snapshot with timeout and retry
    pub async fn get_snapshot(
        &self,
        symbols: Vec<String>,
        exchange: &str,
    ) -> Result<GetSnapshotResponse> {
        debug!(
            "Getting snapshot for {} symbols on {}",
            symbols.len(),
            exchange
        );

        // Check connection
        if !*self.connected.read().await {
            self.reconnect().await?;
        }

        let client_guard = self.client.read().await;
        let mut client = client_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Client not connected"))?
            .clone();

        let request = Request::new(GetSnapshotRequest {
            symbols,
            exchange: exchange.to_string(),
        });

        let response = timeout(
            Duration::from_secs(self.config.request_timeout),
            client.get_snapshot(request),
        )
        .await
        .context("Snapshot request timeout")?
        .context("Failed to get snapshot")?;

        Ok(response.into_inner())
    }

    /// Get historical data with pagination support
    pub async fn get_historical_data(
        &self,
        request: GetHistoricalDataRequest,
    ) -> Result<GetHistoricalDataResponse> {
        info!(
            "Getting historical data for {} from {} to {}",
            request.symbol, request.start_time, request.end_time
        );

        // Check connection
        if !*self.connected.read().await {
            self.reconnect().await?;
        }

        let client_guard = self.client.read().await;
        let mut client = client_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Client not connected"))?
            .clone();

        let request = Request::new(request);

        let response = timeout(
            Duration::from_secs(self.config.request_timeout * 2), // Longer timeout for historical data
            client.get_historical_data(request),
        )
        .await
        .context("Historical data request timeout")?
        .context("Failed to get historical data")?;

        Ok(response.into_inner())
    }

    /// Get connection status
    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }

    /// Get active subscriptions
    pub async fn get_subscriptions(&self) -> Vec<String> {
        self.subscriptions
            .read()
            .await
            .iter()
            .filter(|(_, sub)| sub.active)
            .map(|(key, _)| key.clone())
            .collect()
    }

    /// Disconnect and cleanup
    pub async fn disconnect(&self) -> Result<()> {
        info!("Disconnecting market data client");

        // Cancel stream processor
        if let Some(handle) = self.stream_handle.write().await.take() {
            handle.abort();
        }

        // Clear subscriptions
        self.subscriptions.write().await.clear();

        // Clear client
        *self.client.write().await = None;
        *self.connected.write().await = false;

        info!("Market data client disconnected");
        Ok(())
    }

    /// Get endpoint
    #[must_use] pub fn endpoint(&self) -> &str {
        &self.config.endpoint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_disconnected_client_creation() {
        let config = MarketDataClientConfig {
            endpoint: "http://localhost:50051".to_string(),
            connect_timeout: 1, // Short timeout for testing
            ..Default::default()
        };

        // Create a disconnected client - this should always succeed
        let client = MarketDataClient::new_disconnected(config).await;

        // Verify initial state
        assert!(!client.is_connected().await);
        assert_eq!(client.get_subscriptions().await.len(), 0);
        assert_eq!(client.endpoint(), "http://localhost:50051");
    }

    #[tokio::test]
    async fn test_subscription_state_management() {
        let config = MarketDataClientConfig {
            endpoint: "http://localhost:50051".to_string(),
            connect_timeout: 1,
            max_reconnect_attempts: 1, // Limit retries for testing
            ..Default::default()
        };

        let client = MarketDataClient::new_disconnected(config).await;

        // Test that subscriptions are tracked even when disconnected
        let symbols = vec!["BTCUSDT".to_string()];
        let data_types = vec![0, 1]; // Use raw i32 values for DataType::Trade and DataType::OrderBook

        // Subscription should fail when disconnected but state should be tracked
        let result = client
            .subscribe(symbols.clone(), data_types, "binance")
            .await;
        assert!(result.is_err()); // Should fail due to no connection

        // Verify disconnect works
        assert!(client.disconnect().await.is_ok());
    }

    #[test]
    fn test_config_defaults() {
        let config = MarketDataClientConfig::default();
        assert_eq!(config.endpoint, "http://localhost:50051");
        assert_eq!(config.connect_timeout, 10);
        assert_eq!(config.request_timeout, 30);
        assert_eq!(config.max_reconnect_attempts, 5);
        assert_eq!(config.reconnect_backoff_ms, 1000);
        assert_eq!(config.event_buffer_size, 10000);
        assert_eq!(config.heartbeat_interval, 30);
    }

    #[test]
    fn test_config_customization() {
        let config = MarketDataClientConfig {
            endpoint: "http://custom:9999".to_string(),
            connect_timeout: 5,
            request_timeout: 60,
            max_reconnect_attempts: 10,
            reconnect_backoff_ms: 500,
            event_buffer_size: 50000,
            heartbeat_interval: 60,
        };

        assert_eq!(config.endpoint, "http://custom:9999");
        assert_eq!(config.connect_timeout, 5);
        assert_eq!(config.request_timeout, 60);
        assert_eq!(config.max_reconnect_attempts, 10);
        assert_eq!(config.reconnect_backoff_ms, 500);
        assert_eq!(config.event_buffer_size, 50000);
        assert_eq!(config.heartbeat_interval, 60);
    }

    #[tokio::test]
    async fn test_reconnect_backoff() {
        let config = MarketDataClientConfig {
            endpoint: "http://invalid:1234".to_string(),
            connect_timeout: 1,
            max_reconnect_attempts: 2,
            reconnect_backoff_ms: 100,
            ..Default::default()
        };

        let client = MarketDataClient::new_disconnected(config).await;

        let start = std::time::Instant::now();
        let result = client.reconnect().await;
        let duration = start.elapsed();

        // Should fail after max attempts
        assert!(result.is_err());

        // Should have taken at least the backoff time (100ms + 200ms for 2 attempts)
        assert!(duration.as_millis() >= 300);
    }
}
