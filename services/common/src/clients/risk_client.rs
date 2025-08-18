//! Risk service gRPC client wrapper with production-grade streaming support

use anyhow::{Context, Result};
use futures::StreamExt;
use rustc_hash::FxHashMap;
use crate::proto::risk::v1::{
    CheckOrderRequest, CheckOrderResponse, GetMetricsRequest, GetMetricsResponse,
    GetPositionsRequest, GetPositionsResponse, KillSwitchRequest, KillSwitchResponse, RiskAlert,
    StreamAlertsRequest, UpdatePositionRequest, UpdatePositionResponse,
    risk_service_client::RiskServiceClient as GrpcClient,
};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tokio::time::{Duration, sleep, timeout};
use tonic::transport::{Channel, Endpoint};
use tonic::{Request, Streaming};
use tracing::{debug, error, info, warn};

/// Risk client configuration
#[derive(Clone, Debug)]
pub struct RiskClientConfig {
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
    /// Buffer size for alert channel
    pub alert_buffer_size: usize,
    /// Heartbeat interval in seconds
    pub heartbeat_interval: u64,
}

impl Default for RiskClientConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:50052".to_string(),
            connect_timeout: 10,
            request_timeout: 30,
            max_reconnect_attempts: 5,
            reconnect_backoff_ms: 1000,
            alert_buffer_size: 5000,
            heartbeat_interval: 30,
        }
    }
}

/// Alert subscription state
#[derive(Clone, Debug)]
struct AlertSubscriptionState {
    levels: Vec<i32>,
    active: bool,
    retry_count: u32,
}

/// Risk service client with production-grade streaming support
pub struct RiskClient {
    /// gRPC client
    client: Arc<RwLock<Option<GrpcClient<Channel>>>>,
    /// Configuration
    config: RiskClientConfig,
    /// Alert subscriptions
    subscriptions: Arc<RwLock<FxHashMap<String, AlertSubscriptionState>>>,
    /// Alert sender for streaming data
    alert_sender: mpsc::Sender<RiskAlert>,
    /// Alert receiver for streaming data
    alert_receiver: Arc<RwLock<mpsc::Receiver<RiskAlert>>>,
    /// Connection state
    connected: Arc<RwLock<bool>>,
    /// Stream handle for active subscription
    stream_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl RiskClient {
    /// Create new risk client with configuration
    pub async fn new(config: RiskClientConfig) -> Result<Self> {
        info!("Initializing risk client for endpoint: {}", config.endpoint);

        // Create alert channel
        let (alert_sender, alert_receiver) = mpsc::channel(config.alert_buffer_size);

        let client = Self {
            client: Arc::new(RwLock::new(None)),
            config: config.clone(),
            subscriptions: Arc::new(RwLock::new(FxHashMap::default())),
            alert_sender,
            alert_receiver: Arc::new(RwLock::new(alert_receiver)),
            connected: Arc::new(RwLock::new(false)),
            stream_handle: Arc::new(RwLock::new(None)),
        };

        // Establish initial connection
        client.connect().await?;

        // Start heartbeat task
        client.start_heartbeat().await;

        Ok(client)
    }

    /// Create new risk client with default configuration
    pub async fn new_default(endpoint: &str) -> Result<Self> {
        let mut config = RiskClientConfig::default();
        config.endpoint = endpoint.to_string();
        Self::new(config).await
    }

    /// Create new risk client without initial connection (for testing)
    pub async fn new_disconnected(config: RiskClientConfig) -> Self {
        info!(
            "Creating disconnected risk client for endpoint: {}",
            config.endpoint
        );

        // Create alert channel
        let (alert_sender, alert_receiver) = mpsc::channel(config.alert_buffer_size);

        let client = Self {
            client: Arc::new(RwLock::new(None)),
            config: config.clone(),
            subscriptions: Arc::new(RwLock::new(FxHashMap::default())),
            alert_sender,
            alert_receiver: Arc::new(RwLock::new(alert_receiver)),
            connected: Arc::new(RwLock::new(false)),
            stream_handle: Arc::new(RwLock::new(None)),
        };

        // Start heartbeat task even when disconnected
        client.start_heartbeat().await;

        client
    }

    /// Connect to the risk service
    async fn connect(&self) -> Result<()> {
        info!("Connecting to risk service at {}", self.config.endpoint);

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
                info!("Successfully connected to risk service");
                Ok(())
            }
            Ok(Err(e)) => {
                error!("Failed to connect to risk service: {}", e);
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

                // Resubscribe to all active alert subscriptions
                let subs = self.subscriptions.read().await.clone();
                for (key, sub) in &subs {
                    if sub.active {
                        info!(
                            "Resubscribing to alerts: {} (retry {})",
                            key, sub.retry_count
                        );
                        if let Err(e) = self.stream_alerts_internal(sub.levels.clone()).await {
                            error!("Failed to resubscribe to alerts {}: {}", key, e);
                            // Increment retry count on failure
                            if let Some(subscription) =
                                self.subscriptions.write().await.get_mut(key)
                            {
                                subscription.retry_count += 1;
                                warn!(
                                    "Increased retry count for alerts {} to {}",
                                    key, subscription.retry_count
                                );
                            }
                        } else {
                            // Reset retry count on success
                            if let Some(subscription) =
                                self.subscriptions.write().await.get_mut(key)
                            {
                                subscription.retry_count = 0;
                                info!(
                                    "Successfully resubscribed to alerts {}, reset retry count",
                                    key
                                );
                            }
                        }
                    }
                }

                return Ok(());
            }

            sleep(Duration::from_millis(backoff)).await;
            backoff = (backoff * 2).min(30000); // Cap at 30 seconds
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

    /// Check if order can be placed with timeout protection
    pub async fn check_order(
        &self,
        order_request: CheckOrderRequest,
    ) -> Result<CheckOrderResponse> {
        // Check connection
        if !*self.connected.read().await {
            return Err(anyhow::anyhow!("Not connected to risk service"));
        }

        let client_guard = self.client.read().await;
        let mut client = client_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Client not connected"))?
            .clone();

        let request = Request::new(order_request);

        debug!("Checking order risk");
        let response = timeout(
            Duration::from_secs(self.config.request_timeout),
            client.check_order(request),
        )
        .await
        .context("Order check request timeout")?
        .context("Order risk check failed")?;

        Ok(response.into_inner())
    }

    /// Update position after fill with timeout protection
    pub async fn update_position(
        &self,
        position_request: UpdatePositionRequest,
    ) -> Result<UpdatePositionResponse> {
        // Check connection
        if !*self.connected.read().await {
            return Err(anyhow::anyhow!("Not connected to risk service"));
        }

        let client_guard = self.client.read().await;
        let mut client = client_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Client not connected"))?
            .clone();

        let request = Request::new(position_request);

        debug!("Updating position");
        let response = timeout(
            Duration::from_secs(self.config.request_timeout),
            client.update_position(request),
        )
        .await
        .context("Position update request timeout")?
        .context("Position update failed")?;

        Ok(response.into_inner())
    }

    /// Get current positions with timeout protection
    pub async fn get_positions(&self, symbol: Option<String>) -> Result<GetPositionsResponse> {
        // Check connection
        if !*self.connected.read().await {
            return Err(anyhow::anyhow!("Not connected to risk service"));
        }

        let client_guard = self.client.read().await;
        let mut client = client_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Client not connected"))?
            .clone();

        let request = Request::new(GetPositionsRequest {
            symbol: symbol.unwrap_or_default(),
        });

        debug!("Getting positions");
        let response = timeout(
            Duration::from_secs(self.config.request_timeout),
            client.get_positions(request),
        )
        .await
        .context("Get positions request timeout")?
        .context("Get positions failed")?;

        Ok(response.into_inner())
    }

    /// Get risk metrics with timeout protection
    pub async fn get_metrics(&self) -> Result<GetMetricsResponse> {
        // Check connection
        if !*self.connected.read().await {
            return Err(anyhow::anyhow!("Not connected to risk service"));
        }

        let client_guard = self.client.read().await;
        let mut client = client_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Client not connected"))?
            .clone();

        let request = Request::new(GetMetricsRequest {});

        debug!("Getting risk metrics");
        let response = timeout(
            Duration::from_secs(self.config.request_timeout),
            client.get_metrics(request),
        )
        .await
        .context("Get metrics request timeout")?
        .context("Get metrics failed")?;

        Ok(response.into_inner())
    }

    /// Activate or deactivate kill switch with timeout protection
    pub async fn kill_switch(&self, activate: bool, reason: &str) -> Result<KillSwitchResponse> {
        // Check connection
        if !*self.connected.read().await {
            return Err(anyhow::anyhow!("Not connected to risk service"));
        }

        let client_guard = self.client.read().await;
        let mut client = client_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Client not connected"))?
            .clone();

        let request = Request::new(KillSwitchRequest {
            activate,
            reason: reason.to_string(),
        });

        info!("Setting kill switch: {} (reason: {})", activate, reason);
        let response = timeout(
            Duration::from_secs(self.config.request_timeout),
            client.activate_kill_switch(request),
        )
        .await
        .context("Kill switch request timeout")?
        .context("Kill switch operation failed")?;

        Ok(response.into_inner())
    }

    /// Stream risk alerts with production-grade streaming support
    pub async fn stream_alerts(&self, levels: Vec<i32>) -> Result<mpsc::Receiver<RiskAlert>> {
        // Ensure connected before subscribing
        if !*self.connected.read().await {
            self.reconnect().await?;
        }

        self.stream_alerts_internal(levels.clone()).await?;

        // Store subscription state
        let key = format!(
            "alerts:{}",
            levels
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        let mut subs = self.subscriptions.write().await;
        subs.insert(
            key.clone(),
            AlertSubscriptionState {
                levels,
                active: true,
                retry_count: 0,
            },
        );

        // Create dedicated receiver for alerts
        let (tx, rx) = mpsc::channel(1000);

        // Forward alerts to dedicated receiver
        let alert_rx = self.alert_receiver.clone();
        tokio::spawn(async move {
            let mut receiver = alert_rx.write().await;
            while let Some(alert) = receiver.recv().await {
                if tx.send(alert).await.is_err() {
                    break; // Receiver dropped
                }
            }
        });

        Ok(rx)
    }

    /// Internal stream alerts implementation
    async fn stream_alerts_internal(&self, levels: Vec<i32>) -> Result<()> {
        // Check connection but don't reconnect here to avoid recursion
        if !*self.connected.read().await {
            return Err(anyhow::anyhow!("Not connected to risk service"));
        }

        let client_guard = self.client.read().await;
        let mut client = client_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Client not connected"))?
            .clone();

        let request = Request::new(StreamAlertsRequest {
            levels: levels.clone(),
        });

        info!("Streaming risk alerts for levels: {:?}", levels);

        // Get streaming response
        let stream = client
            .stream_alerts(request)
            .await
            .context("Failed to start alert stream")?
            .into_inner();

        // Cancel previous stream if exists
        if let Some(handle) = self.stream_handle.write().await.take() {
            handle.abort();
        }

        // Start processing stream
        let handle = self.start_alert_processor(stream).await;
        *self.stream_handle.write().await = Some(handle);

        info!("Successfully started risk alert stream");
        Ok(())
    }

    /// Process incoming risk alert stream
    async fn start_alert_processor(
        &self,
        mut stream: Streaming<RiskAlert>,
    ) -> tokio::task::JoinHandle<()> {
        let alert_sender = self.alert_sender.clone();
        let connected = self.connected.clone();

        tokio::spawn(async move {
            debug!("Starting alert stream processor");

            while let Some(result) = stream.next().await {
                match result {
                    Ok(alert) => {
                        // Send alert to channel
                        if let Err(e) = alert_sender.send(alert).await {
                            error!("Failed to send risk alert: {}", e);
                            break;
                        }
                    }
                    Err(status) => {
                        error!("Alert stream error: {}", status);

                        // Handle specific error codes
                        match status.code() {
                            tonic::Code::Unavailable | tonic::Code::Unknown => {
                                warn!("Risk service unavailable, marking connection as lost");
                                *connected.write().await = false;
                                break;
                            }
                            tonic::Code::DeadlineExceeded => {
                                warn!("Alert stream deadline exceeded, will reconnect");
                                break;
                            }
                            _ => {
                                error!("Unhandled alert stream error: {:?}", status.code());
                            }
                        }
                    }
                }
            }

            debug!("Alert stream processor ended");
        })
    }

    /// Get connection status
    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }

    /// Get active alert subscriptions
    pub async fn get_alert_subscriptions(&self) -> Vec<String> {
        self.subscriptions
            .read()
            .await
            .iter()
            .filter(|(_, sub)| sub.active)
            .map(|(key, _)| key.clone())
            .collect()
    }

    /// Get alert subscription retry statistics
    pub async fn get_alert_retry_stats(&self) -> FxHashMap<String, u32> {
        self.subscriptions
            .read()
            .await
            .iter()
            .map(|(key, sub)| (key.clone(), sub.retry_count))
            .collect()
    }

    /// Check if any alert subscription has exceeded max retry count
    pub async fn has_failed_alert_subscriptions(&self, max_retries: u32) -> bool {
        self.subscriptions
            .read()
            .await
            .values()
            .any(|sub| sub.retry_count > max_retries)
    }

    /// Stop alert streaming
    pub async fn stop_alerts(&self) -> Result<()> {
        info!("Stopping risk alert streams");

        // Cancel stream processor
        if let Some(handle) = self.stream_handle.write().await.take() {
            handle.abort();
        }

        // Deactivate all subscriptions
        let mut subs = self.subscriptions.write().await;
        for sub in subs.values_mut() {
            sub.active = false;
        }

        info!("Risk alert streams stopped");
        Ok(())
    }

    /// Disconnect and cleanup
    pub async fn disconnect(&self) -> Result<()> {
        info!("Disconnecting risk client");

        // Stop alert streams
        self.stop_alerts().await?;

        // Clear subscriptions
        self.subscriptions.write().await.clear();

        // Clear client
        *self.client.write().await = None;
        *self.connected.write().await = false;

        info!("Risk client disconnected");
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
        let config = RiskClientConfig {
            endpoint: "http://localhost:50052".to_string(),
            connect_timeout: 1,
            ..Default::default()
        };

        let client = RiskClient::new_disconnected(config).await;

        // Verify initial state
        assert!(!client.is_connected().await);
        assert_eq!(client.get_alert_subscriptions().await.len(), 0);
        assert_eq!(client.endpoint(), "http://localhost:50052");
    }

    #[tokio::test]
    async fn test_alert_subscription_management() {
        let config = RiskClientConfig {
            endpoint: "http://localhost:50052".to_string(),
            connect_timeout: 1,
            max_reconnect_attempts: 1,
            ..Default::default()
        };

        let client = RiskClient::new_disconnected(config).await;

        // Test alert subscription fails when disconnected
        let levels = vec![1, 2, 3]; // Critical, High, Medium alerts
        let result = client.stream_alerts(levels).await;
        assert!(result.is_err());

        // Verify disconnect works
        assert!(client.disconnect().await.is_ok());
    }

    #[test]
    fn test_config_defaults() {
        let config = RiskClientConfig::default();
        assert_eq!(config.endpoint, "http://localhost:50052");
        assert_eq!(config.connect_timeout, 10);
        assert_eq!(config.request_timeout, 30);
        assert_eq!(config.max_reconnect_attempts, 5);
        assert_eq!(config.reconnect_backoff_ms, 1000);
        assert_eq!(config.alert_buffer_size, 5000);
        assert_eq!(config.heartbeat_interval, 30);
    }

    #[tokio::test]
    async fn test_stop_alerts() {
        let config = RiskClientConfig {
            endpoint: "http://localhost:50052".to_string(),
            connect_timeout: 1,
            ..Default::default()
        };

        let client = RiskClient::new_disconnected(config).await;

        // Stop alerts should always succeed
        assert!(client.stop_alerts().await.is_ok());

        // Should have no active subscriptions
        assert_eq!(client.get_alert_subscriptions().await.len(), 0);
    }
}
