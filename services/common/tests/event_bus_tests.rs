//! Comprehensive unit tests for the event bus functionality
//!
//! Tests cover:
//! - Event routing and subscription
//! - Message handling and processing 
//! - Metrics collection and reporting
//! - Concurrent access patterns
//! - Error handling and recovery
//! - Performance characteristics

use services_common::{
    BusMessage, BusResult, EventBus, EventBusConfig, EventBusError, EventBusFactory,
    BusMetrics as EventBusMetrics, LoggingMiddleware, MessageEnvelope, MessageHandler, MessageMetadata,
    MessageRouter, MessageType, MetricsMiddleware, ShrivenQuantMessage,
    TopicRouter
};
use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use parking_lot::RwLock;
use rstest::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout};

/// Test message for event bus testing
#[derive(Debug, Clone)]
struct TestMessage {
    pub id: u64,
    pub content: String,
    pub priority: u8,
    pub topic_name: String,
}

impl BusMessage for TestMessage {
    fn topic(&self) -> &str {
        &self.topic_name
    }

    fn priority(&self) -> u8 {
        self.priority
    }
}

/// Mock message handler for testing
#[derive(Clone)]
struct MockHandler {
    name: String,
    received_messages: Arc<RwLock<Vec<MessageEnvelope<TestMessage>>>>,
    should_fail: Arc<RwLock<bool>>,
    processing_delay: Option<Duration>,
}

impl MockHandler {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            received_messages: Arc::new(RwLock::new(Vec::new())),
            should_fail: Arc::new(RwLock::new(false)),
            processing_delay: None,
        }
    }

    fn with_delay(name: &str, delay: Duration) -> Self {
        Self {
            name: name.to_string(),
            received_messages: Arc::new(RwLock::new(Vec::new())),
            should_fail: Arc::new(RwLock::new(false)),
            processing_delay: Some(delay),
        }
    }

    fn set_should_fail(&self, should_fail: bool) {
        *self.should_fail.write() = should_fail;
    }

    fn get_received_messages(&self) -> Vec<MessageEnvelope<TestMessage>> {
        self.received_messages.read().clone()
    }

    fn message_count(&self) -> usize {
        self.received_messages.read().len()
    }
}

#[async_trait]
impl MessageHandler<TestMessage> for MockHandler {
    async fn handle(&self, envelope: MessageEnvelope<TestMessage>) -> Result<()> {
        if let Some(delay) = self.processing_delay {
            sleep(delay).await;
        }

        if *self.should_fail.read() {
            return Err(anyhow::anyhow!("Handler {} failed", self.name));
        }

        self.received_messages.write().push(envelope);
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// Event Bus Core Functionality Tests
#[rstest]
#[tokio::test]
async fn test_event_bus_creation() {
    let config = EventBusConfig {
        capacity: 1000,
        enable_metrics: true,
        enable_dead_letter_queue: true,
        max_retry_attempts: 3,
        default_ttl_ms: Some(30000),
    };

    let bus = EventBus::<TestMessage>::new(config);
    assert_eq!(bus.capacity(), 1000);
    assert!(bus.metrics_enabled());
}

#[rstest]
#[tokio::test]
async fn test_message_publishing_and_subscription() -> Result<()> {
    let bus = EventBus::<TestMessage>::new(EventBusConfig::default());
    let mut subscriber = bus.subscribe("test_topic").await?;

    let test_message = TestMessage {
        id: 1,
        content: "Hello, World!".to_string(),
        priority: 128,
        topic_name: "test_topic".to_string(),
    };

    // Publish message
    bus.publish(test_message.clone()).await?;

    // Receive message
    let received_envelope = timeout(Duration::from_millis(100), subscriber.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for message"))?
        .map_err(|e| anyhow::anyhow!("Failed to receive message: {}", e))?;

    assert_eq!(received_envelope.message.id, test_message.id);
    assert_eq!(received_envelope.message.content, test_message.content);
    assert_eq!(received_envelope.topic(), test_message.topic());

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_message_handler_registration() -> Result<()> {
    let bus = EventBus::<TestMessage>::new(EventBusConfig::default());
    let handler = MockHandler::new("test_handler");
    
    // Register handler
    bus.register_handler("test_topic", handler.clone()).await?;

    let test_message = TestMessage {
        id: 2,
        content: "Handler test".to_string(),
        priority: 64,
        topic_name: "test_topic".to_string(),
    };

    // Publish message
    bus.publish(test_message.clone()).await?;

    // Give handler time to process
    sleep(Duration::from_millis(10)).await;

    // Verify handler received the message
    let received_messages = handler.get_received_messages();
    assert_eq!(received_messages.len(), 1);
    assert_eq!(received_messages[0].message.id, test_message.id);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_multiple_handlers_same_topic() -> Result<()> {
    let bus = EventBus::<TestMessage>::new(EventBusConfig::default());
    let handler1 = MockHandler::new("handler_1");
    let handler2 = MockHandler::new("handler_2");
    
    // Register multiple handlers for same topic
    bus.register_handler("shared_topic", handler1.clone()).await?;
    bus.register_handler("shared_topic", handler2.clone()).await?;

    let test_message = TestMessage {
        id: 3,
        content: "Multi-handler test".to_string(),
        priority: 32,
        topic_name: "shared_topic".to_string(),
    };

    bus.publish(test_message.clone()).await?;

    // Give handlers time to process
    sleep(Duration::from_millis(10)).await;

    // Both handlers should have received the message
    assert_eq!(handler1.message_count(), 1);
    assert_eq!(handler2.message_count(), 1);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_message_priority_handling() -> Result<()> {
    let bus = EventBus::<TestMessage>::new(EventBusConfig::default());
    let handler = MockHandler::new("priority_handler");
    
    bus.register_handler("priority_topic", handler.clone()).await?;

    // Publish messages with different priorities
    let high_priority_msg = TestMessage {
        id: 4,
        content: "High priority".to_string(),
        priority: 0, // Highest priority
        topic_name: "priority_topic".to_string(),
    };

    let low_priority_msg = TestMessage {
        id: 5,
        content: "Low priority".to_string(),
        priority: 255, // Lowest priority
        topic_name: "priority_topic".to_string(),
    };

    bus.publish(high_priority_msg.clone()).await?;
    bus.publish(low_priority_msg.clone()).await?;

    sleep(Duration::from_millis(20)).await;

    let received_messages = handler.get_received_messages();
    assert_eq!(received_messages.len(), 2);

    // Verify priority values are preserved
    let high_pri_received = received_messages.iter()
        .find(|env| env.message.id == 4)
        .expect("High priority message not found");
    assert_eq!(high_pri_received.priority(), 0);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_handler_error_handling() -> Result<()> {
    let bus = EventBus::<TestMessage>::new(EventBusConfig::default());
    let handler = MockHandler::new("failing_handler");
    
    // Configure handler to fail
    handler.set_should_fail(true);
    bus.register_handler("error_topic", handler.clone()).await?;

    let test_message = TestMessage {
        id: 6,
        content: "This will fail".to_string(),
        priority: 128,
        topic_name: "error_topic".to_string(),
    };

    bus.publish(test_message).await?;

    // Give handler time to fail
    sleep(Duration::from_millis(10)).await;

    // Handler should not have successfully processed the message
    assert_eq!(handler.message_count(), 0);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_concurrent_message_publishing() -> Result<()> {
    let bus = Arc::new(EventBus::<TestMessage>::new(EventBusConfig::default()));
    let handler = MockHandler::new("concurrent_handler");
    
    bus.register_handler("concurrent_topic", handler.clone()).await?;

    let message_count = 100;
    let mut handles = Vec::new();

    // Spawn concurrent publishers
    for i in 0..message_count {
        let bus_clone = Arc::clone(&bus);
        let handle = tokio::spawn(async move {
            let msg = TestMessage {
                id: i,
                content: format!("Message {}", i),
                priority: (i % 256) as u8,
                topic_name: "concurrent_topic".to_string(),
            };
            bus_clone.publish(msg).await
        });
        handles.push(handle);
    }

    // Wait for all publishers to complete
    for handle in handles {
        handle.await??;
    }

    // Give handlers time to process all messages
    sleep(Duration::from_millis(100)).await;

    // All messages should be processed
    assert_eq!(handler.message_count(), message_count as usize);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_message_routing_with_topic_router() -> Result<()> {
    let router = TopicRouter::new();
    router.add_route("test_*", "target_topic");

    let test_message = TestMessage {
        id: 7,
        content: "Routing test".to_string(),
        priority: 128,
        topic_name: "test_routing".to_string(),
    };

    let envelope = MessageEnvelope::new(test_message, MessageMetadata::default());
    let targets = router.route(&envelope);

    assert_eq!(targets, vec!["target_topic"]);
    let router: &dyn MessageRouter<TestMessage> = &router;
    assert!(router.handles_topic("test_routing"));
    assert!(!router.handles_topic("unknown"));

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_event_bus_factory() {
    let bus = EventBusFactory::create_default();
    assert!(bus.capacity() > 0);
    assert!(bus.metrics_enabled());

    let hp_bus = EventBusFactory::create_high_performance();
    assert!(hp_bus.capacity() >= 100000);

    let reliable_bus = EventBusFactory::create_reliable();
    assert!(reliable_bus.capacity() >= 50000);
}

#[rstest]
#[tokio::test]
async fn test_message_metadata() {
    let metadata = MessageMetadata::default();
    assert!(!metadata.message_id.is_empty());
    assert_eq!(metadata.source, "unknown");
    assert!(metadata.timestamp > 0);
    assert_eq!(metadata.retry_count, 0);
}

#[rstest]
#[tokio::test]
async fn test_shrivenquant_message_topics() {
    let market_data = ShrivenQuantMessage::MarketData {
        symbol: "BTCUSDT".to_string(),
        exchange: "binance".to_string(),
        bid: 500000000,
        ask: 500010000,
        timestamp: 1234567890,
    };

    assert_eq!(market_data.topic(), "market_data");
    assert_eq!(market_data.priority(), 48);

    let risk_alert = ShrivenQuantMessage::RiskAlert {
        level: "EMERGENCY".to_string(),
        message: "Kill switch activated".to_string(),
        source: "risk_manager".to_string(),
        symbol: None,
        value: None,
        timestamp: 1234567890,
    };

    assert_eq!(risk_alert.topic(), "risk_alerts");
    assert_eq!(risk_alert.priority(), 0); // Highest priority
}

#[rstest]
#[tokio::test]
async fn test_message_expiration() -> Result<()> {
    let test_message = TestMessage {
        id: 8,
        content: "Expiring message".to_string(),
        priority: 128,
        topic_name: "expiry_test".to_string(),
    };

    let mut metadata = MessageMetadata::default();
    metadata.ttl_ms = Some(1); // 1ms TTL
    metadata.timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    let envelope = MessageEnvelope::new(test_message, metadata);

    // Should not be expired immediately
    assert!(!envelope.is_expired());

    // Wait and check again
    sleep(Duration::from_millis(5)).await;
    assert!(envelope.is_expired());

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_logging_middleware() -> Result<()> {
    let middleware = LoggingMiddleware::new("test_service");
    
    let test_message = TestMessage {
        id: 9,
        content: "Middleware test".to_string(),
        priority: 128,
        topic_name: "middleware_test".to_string(),
    };

    let mut envelope = MessageEnvelope::new(test_message, MessageMetadata::default());

    // Test middleware methods don't panic
    middleware.before_publish(&mut envelope).await?;
    middleware.after_publish(&envelope).await?;
    middleware.before_handle(&envelope).await?;
    
    let success_result = Ok(());
    middleware.after_handle(&envelope, &success_result).await?;
    
    let error_result: Result<()> = Err(anyhow::anyhow!("Test error"));
    middleware.after_handle(&envelope, &error_result).await?;

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_handler_with_processing_delay() -> Result<()> {
    let bus = EventBus::<TestMessage>::new(EventBusConfig::default());
    let handler = MockHandler::with_delay("slow_handler", Duration::from_millis(50));
    
    bus.register_handler("slow_topic", handler.clone()).await?;

    let test_message = TestMessage {
        id: 10,
        content: "Slow processing".to_string(),
        priority: 128,
        topic_name: "slow_topic".to_string(),
    };

    let start_time = std::time::Instant::now();
    bus.publish(test_message).await?;

    // Wait for processing to complete
    sleep(Duration::from_millis(100)).await;

    assert_eq!(handler.message_count(), 1);
    assert!(start_time.elapsed() >= Duration::from_millis(50));

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_bus_capacity_limits() -> Result<()> {
    let config = EventBusConfig {
        capacity: 10, // Small capacity for testing
        enable_metrics: false,
        enable_dead_letter_queue: false,
        max_retry_attempts: 1,
        default_ttl_ms: None,
    };

    let bus = EventBus::<TestMessage>::new(config);
    assert_eq!(bus.capacity(), 10);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_multiple_topic_routing() -> Result<()> {
    let bus = EventBus::<TestMessage>::new(EventBusConfig::default());
    let handler1 = MockHandler::new("topic1_handler");
    let handler2 = MockHandler::new("topic2_handler");
    
    bus.register_handler("topic1", handler1.clone()).await?;
    bus.register_handler("topic2", handler2.clone()).await?;

    let msg1 = TestMessage {
        id: 11,
        content: "Topic 1 message".to_string(),
        priority: 128,
        topic_name: "topic1".to_string(),
    };

    let msg2 = TestMessage {
        id: 12,
        content: "Topic 2 message".to_string(),
        priority: 128,
        topic_name: "topic2".to_string(),
    };

    bus.publish(msg1).await?;
    bus.publish(msg2).await?;

    sleep(Duration::from_millis(10)).await;

    // Each handler should only receive messages for their topic
    assert_eq!(handler1.message_count(), 1);
    assert_eq!(handler2.message_count(), 1);

    let handler1_msgs = handler1.get_received_messages();
    let handler2_msgs = handler2.get_received_messages();

    assert_eq!(handler1_msgs[0].message.id, 11);
    assert_eq!(handler2_msgs[0].message.id, 12);

    Ok(())
}