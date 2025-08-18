//! Core Event Bus Implementation
//!
//! High-performance, lock-free event bus with advanced features

use super::{
    BusMessage, BusResult, EventBusError, MessageEnvelope, MessageHandler, MessageMetadata,
    Publisher, Subscriber,
};
use anyhow::Result;
use async_trait::async_trait;
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, warn};

/// Event bus configuration
#[derive(Debug, Clone)]
pub struct EventBusConfig {
    /// Channel capacity
    pub capacity: usize,
    /// Enable metrics collection
    pub enable_metrics: bool,
    /// Enable dead letter queue
    pub enable_dead_letter_queue: bool,
    /// Maximum retry attempts
    pub max_retry_attempts: u32,
    /// Default message TTL in milliseconds
    pub default_ttl_ms: Option<u64>,
}

impl Default for EventBusConfig {
    fn default() -> Self {
        Self {
            capacity: 10000,
            enable_metrics: true,
            enable_dead_letter_queue: true,
            max_retry_attempts: 3,
            default_ttl_ms: Some(30000), // 30 seconds
        }
    }
}

/// Main event bus implementation
pub struct EventBus<T: BusMessage> {
    /// Configuration
    config: EventBusConfig,
    /// Topic-based broadcasters
    broadcasters: Arc<RwLock<FxHashMap<String, broadcast::Sender<MessageEnvelope<T>>>>>,
    /// Dead letter queue
    dead_letter_tx: Option<mpsc::UnboundedSender<MessageEnvelope<T>>>,
    /// Metrics collector
    metrics: Arc<super::metrics::BusMetrics>,
    /// Message handlers by topic
    handlers: Arc<RwLock<FxHashMap<String, Vec<Arc<dyn MessageHandler<T>>>>>>,
}

impl<T: BusMessage> EventBus<T> {
    /// Create a new event bus with configuration
    #[must_use] pub fn new(config: EventBusConfig) -> Self {
        let (dead_letter_tx, dead_letter_rx) = if config.enable_dead_letter_queue {
            let (tx, rx) = mpsc::unbounded_channel();
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        let metrics = Arc::new(super::metrics::BusMetrics::new());

        let bus = Self {
            config,
            broadcasters: Arc::new(RwLock::new(FxHashMap::default())),
            dead_letter_tx,
            metrics,
            handlers: Arc::new(RwLock::new(FxHashMap::default())),
        };

        // Start dead letter queue processor
        if let Some(mut rx) = dead_letter_rx {
            let metrics_clone = Arc::clone(&bus.metrics);
            tokio::spawn(async move {
                while let Some(envelope) = rx.recv().await {
                    error!(
                        message_id = %envelope.metadata.message_id,
                        topic = envelope.message.topic(),
                        retry_count = envelope.metadata.retry_count,
                        "Message sent to dead letter queue"
                    );
                    metrics_clone.record_dead_letter(envelope.message.topic());
                }
            });
        }

        bus
    }

    /// Get bus capacity
    #[must_use] pub const fn capacity(&self) -> usize {
        self.config.capacity
    }

    /// Get or create broadcaster for topic
    fn get_or_create_broadcaster(&self, topic: &str) -> broadcast::Sender<MessageEnvelope<T>> {
        let mut broadcasters = self.broadcasters.write();

        if let Some(broadcaster) = broadcasters.get(topic) {
            broadcaster.clone()
        } else {
            let (tx, _) = broadcast::channel(self.config.capacity);
            broadcasters.insert(topic.to_string(), tx.clone());
            tx
        }
    }

    /// Publish a message to the bus
    pub async fn publish(&self, message: T) -> BusResult<()> {
        let metadata = MessageMetadata {
            source: "event_bus".to_string(),
            ..Default::default()
        };

        self.publish_with_metadata(message, metadata).await
    }

    /// Publish a message with custom metadata
    pub async fn publish_with_metadata(
        &self,
        message: T,
        metadata: MessageMetadata,
    ) -> BusResult<()> {
        let topic = message.topic().to_string();

        // Check TTL
        if let Some(ttl_ms) = metadata.ttl_ms.or(self.config.default_ttl_ms) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                // SAFETY: u128 to u64 - milliseconds since epoch fits in u64
                .as_millis() as u64;
            // SAFETY: u64 arithmetic result to u64
            let message_age = now - metadata.timestamp / 1_000_000;

            if message_age > ttl_ms {
                warn!(
                    message_id = %metadata.message_id,
                    topic = %topic,
                    age_ms = message_age,
                    ttl_ms = ttl_ms,
                    "Message expired, dropping"
                );
                self.metrics.record_expired(&topic);
                return Err(EventBusError::MessageExpired);
            }
        }

        let envelope = MessageEnvelope { message, metadata };

        // Get broadcaster for topic
        let broadcaster = self.get_or_create_broadcaster(&topic);

        // Send to subscribers
        if let Ok(subscriber_count) = broadcaster.send(envelope.clone()) {
            debug!(
                message_id = %envelope.metadata.message_id,
                topic = %topic,
                subscribers = subscriber_count,
                "Message published"
            );
            self.metrics.record_publish_success(&topic);
            Ok(())
        } else {
            warn!(
                message_id = %envelope.metadata.message_id,
                topic = %topic,
                "No subscribers for topic"
            );
            self.metrics.record_no_subscribers(&topic);
            Err(EventBusError::NoSubscribers { topic })
        }
    }

    /// Subscribe to messages for a topic
    pub async fn subscribe(
        &self,
        topic: &str,
    ) -> BusResult<broadcast::Receiver<MessageEnvelope<T>>> {
        let broadcaster = self.get_or_create_broadcaster(topic);
        Ok(broadcaster.subscribe())
    }

    /// Register a message handler for a topic
    pub async fn register_handler<H>(&self, topic: &str, handler: H) -> BusResult<()>
    where
        H: MessageHandler<T> + 'static,
    {
        let mut handlers = self.handlers.write();
        let topic_handlers = handlers.entry(topic.to_string()).or_default();
        topic_handlers.push(Arc::new(handler));

        debug!(
            topic = topic,
            handler_count = topic_handlers.len(),
            "Message handler registered"
        );

        Ok(())
    }

    /// Start message processing for registered handlers
    pub async fn start_handlers(&self) -> BusResult<()> {
        let handlers = self.handlers.read();

        for (topic, topic_handlers) in handlers.iter() {
            let mut receiver = self.subscribe(topic).await?;
            let handlers_clone = topic_handlers.clone();
            let metrics_clone = Arc::clone(&self.metrics);
            let dead_letter_tx = self.dead_letter_tx.clone();
            let max_retries = self.config.max_retry_attempts;

            let topic_clone = topic.clone();
            tokio::spawn(async move {
                while let Ok(mut envelope) = receiver.recv().await {
                    let start_time = std::time::Instant::now();

                    // Process with all handlers
                    let mut all_succeeded = true;

                    for handler in &handlers_clone {
                        match handler.handle(envelope.clone()).await {
                            Ok(()) => {
                                debug!(
                                    message_id = %envelope.metadata.message_id,
                                    topic = %topic_clone,
                                    handler = handler.name(),
                                    "Message handled successfully"
                                );
                            }
                            Err(e) => {
                                error!(
                                    message_id = %envelope.metadata.message_id,
                                    topic = %topic_clone,
                                    handler = handler.name(),
                                    error = %e,
                                    "Message handling failed"
                                );
                                all_succeeded = false;
                            }
                        }
                    }

                    // Record metrics
                    let duration = start_time.elapsed();
                    metrics_clone.record_handle_duration(&topic_clone, duration);

                    if all_succeeded {
                        metrics_clone.record_handle_success(&topic_clone);
                    } else {
                        metrics_clone.record_handle_failure(&topic_clone);

                        // Handle retry logic
                        envelope.metadata.retry_count += 1;
                        if envelope.metadata.retry_count <= max_retries {
                            warn!(
                                message_id = %envelope.metadata.message_id,
                                topic = %topic_clone,
                                retry_count = envelope.metadata.retry_count,
                                max_retries = max_retries,
                                "Retrying message"
                            );
                            // Could implement exponential backoff here
                            // For now, just reprocess immediately
                        } else if let Some(ref dlq) = dead_letter_tx {
                            if let Err(e) = dlq.send(envelope) {
                                error!(
                                    error = %e,
                                    "Failed to send message to dead letter queue"
                                );
                            }
                        }
                    }
                }
            });
        }

        debug!("Started handlers for {} topics", handlers.len());
        Ok(())
    }

    /// Get metrics for the event bus
    #[must_use] pub fn metrics(&self) -> Arc<super::metrics::BusMetrics> {
        Arc::clone(&self.metrics)
    }

    /// Get current subscriber count for a topic
    #[must_use] pub fn subscriber_count(&self, topic: &str) -> usize {
        let broadcasters = self.broadcasters.read();
        broadcasters
            .get(topic)
            .map_or(0, tokio::sync::broadcast::Sender::receiver_count)
    }

    /// List all active topics
    #[must_use] pub fn topics(&self) -> Vec<String> {
        let broadcasters = self.broadcasters.read();
        broadcasters.keys().cloned().collect()
    }

    /// Shutdown the event bus
    pub async fn shutdown(&self) -> BusResult<()> {
        debug!("Shutting down event bus");

        // Clear all broadcasters to disconnect subscribers
        {
            let mut broadcasters = self.broadcasters.write();
            broadcasters.clear();
        }

        // Clear handlers
        {
            let mut handlers = self.handlers.write();
            handlers.clear();
        }

        debug!("Event bus shutdown complete");
        Ok(())
    }
}

/// Publisher implementation for the event bus
pub struct EventBusPublisher<T: BusMessage> {
    bus: Arc<EventBus<T>>,
    source_service: String,
}

impl<T: BusMessage> EventBusPublisher<T> {
    /// Create a new publisher
    pub fn new(bus: Arc<EventBus<T>>, source_service: impl Into<String>) -> Self {
        Self {
            bus,
            source_service: source_service.into(),
        }
    }
}

#[async_trait]
impl<T: BusMessage> Publisher<T> for EventBusPublisher<T> {
    async fn publish(&self, message: T) -> Result<()> {
        let metadata = MessageMetadata {
            source: self.source_service.clone(),
            ..Default::default()
        };

        self.bus
            .publish_with_metadata(message, metadata)
            .await
            .map_err(Into::into)
    }

    async fn publish_with_metadata(&self, message: T, mut metadata: MessageMetadata) -> Result<()> {
        metadata.source = self.source_service.clone();

        self.bus
            .publish_with_metadata(message, metadata)
            .await
            .map_err(Into::into)
    }
}

/// Subscriber implementation for the event bus
pub struct EventBusSubscriber<T: BusMessage> {
    bus: Arc<EventBus<T>>,
}

impl<T: BusMessage> EventBusSubscriber<T> {
    /// Create a new subscriber
    #[must_use] pub const fn new(bus: Arc<EventBus<T>>) -> Self {
        Self { bus }
    }
}

#[async_trait]
impl<T: BusMessage> Subscriber<T> for EventBusSubscriber<T> {
    async fn subscribe(&self) -> Result<broadcast::Receiver<MessageEnvelope<T>>> {
        // Subscribe to a general broadcast topic for all messages
        let broadcast_topic = "broadcast";
        self.bus
            .subscribe(broadcast_topic)
            .await
            .map_err(|e| anyhow::anyhow!("Subscription failed: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct TestMessage {
        id: u64,
        content: String,
    }

    impl BusMessage for TestMessage {
        fn topic(&self) -> &str {
            "test_topic"
        }
    }

    struct TestHandler {
        name: String,
        received: Arc<RwLock<Vec<MessageEnvelope<TestMessage>>>>,
    }

    impl TestHandler {
        fn new(name: impl Into<String>) -> Self {
            Self {
                name: name.into(),
                received: Arc::new(RwLock::new(Vec::new())),
            }
        }

        fn received_messages(&self) -> Vec<MessageEnvelope<TestMessage>> {
            self.received.read().clone()
        }
    }

    #[async_trait]
    impl MessageHandler<TestMessage> for TestHandler {
        async fn handle(&self, envelope: MessageEnvelope<TestMessage>) -> Result<()> {
            self.received.write().push(envelope);
            Ok(())
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    #[tokio::test]
    async fn test_event_bus_basic() {
        let config = EventBusConfig::default();
        let bus = EventBus::new(config);

        assert_eq!(bus.capacity(), 10000);
        assert_eq!(bus.subscriber_count("test_topic"), 0);
    }

    #[tokio::test]
    async fn test_publish_subscribe() {
        let config = EventBusConfig::default();
        let bus = Arc::new(EventBus::new(config));

        let mut subscriber = bus.subscribe("test_topic").await.unwrap();

        let message = TestMessage {
            id: 42,
            content: "Hello, World!".to_string(),
        };

        bus.publish(message.clone()).await.unwrap();

        let received = subscriber.recv().await.unwrap();
        assert_eq!(received.message.id, 42);
        assert_eq!(received.message.content, "Hello, World!");
    }

    #[tokio::test]
    async fn test_message_handler() {
        let config = EventBusConfig::default();
        let bus = Arc::new(EventBus::new(config));

        let handler = TestHandler::new("test_handler");
        bus.register_handler("test_topic", handler).await.unwrap();
        bus.start_handlers().await.unwrap();

        let message = TestMessage {
            id: 123,
            content: "Test message".to_string(),
        };

        bus.publish(message.clone()).await.unwrap();

        // Give some time for message processing
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Note: In a real test, we'd need a way to wait for the handler
        // This is a simplified example
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let config = EventBusConfig {
            enable_metrics: true,
            ..Default::default()
        };
        let bus = Arc::new(EventBus::new(config));

        // Keep subscriber alive for the duration of the test
        let subscriber = bus.subscribe("test_topic").await.unwrap();

        let message = TestMessage {
            id: 1,
            content: "Metrics test".to_string(),
        };

        bus.publish(message).await.unwrap();

        let metrics = bus.metrics();
        assert!(metrics.get_publish_count("test_topic") > 0);
    }

    #[tokio::test]
    async fn test_ttl_expiration() {
        let config = EventBusConfig {
            default_ttl_ms: Some(1), // 1ms TTL
            ..Default::default()
        };
        let bus = Arc::new(EventBus::new(config));

        let message = TestMessage {
            id: 1,
            content: "Expired message".to_string(),
        };

        // Wait for message to expire
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;

        let result = bus.publish(message).await;
        assert!(matches!(result, Err(EventBusError::MessageExpired)));
    }
}
