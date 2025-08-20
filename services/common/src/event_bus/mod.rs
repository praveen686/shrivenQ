//! Enhanced Event Bus for `ShrivenQuant` Microservices
//!
//! High-performance, lock-free event bus for inter-service communication
//! with support for:
//! - Multiple message types
//! - Topic-based routing
//! - Back-pressure handling
//! - Metrics collection
//! - Dead letter queue

pub mod bus;
pub mod message;
pub mod metrics;
pub mod router;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use tokio::sync::broadcast;
use tracing::{debug, error};

// Re-export main types
pub use bus::{EventBus, EventBusConfig};
pub use message::{Message, MessageEnvelope, MessageType};
pub use metrics::{BusMetrics, EventBusMetrics};
pub use router::{MessageRouter, TopicRouter};

/// Core message trait for all event bus messages
pub trait BusMessage: Send + Sync + Clone + Debug + 'static {
    /// Get the message topic for routing
    fn topic(&self) -> &str;

    /// Get message priority (0 = highest, 255 = lowest)
    fn priority(&self) -> u8 {
        128 // Default priority
    }

    /// Get message metadata
    fn metadata(&self) -> MessageMetadata {
        MessageMetadata::default()
    }
}

/// Message metadata for enhanced routing and processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// Message ID
    pub message_id: String,
    /// Source service
    pub source: String,
    /// Target service (optional)
    pub target: Option<String>,
    /// Correlation ID for request tracing
    pub correlation_id: Option<String>,
    /// Message timestamp (nanoseconds)
    pub timestamp: u64,
    /// Message TTL in milliseconds
    pub ttl_ms: Option<u64>,
    /// Retry count
    pub retry_count: u32,
    /// Custom headers
    pub headers: rustc_hash::FxHashMap<String, String>,
}

impl Default for MessageMetadata {
    fn default() -> Self {
        Self {
            message_id: uuid::Uuid::new_v4().to_string(),
            source: "unknown".to_string(),
            target: None,
            correlation_id: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                // SAFETY: u128 to u64 - nanoseconds since epoch fits in u64 for centuries
                .as_nanos() as u64,
            ttl_ms: None,
            retry_count: 0,
            headers: rustc_hash::FxHashMap::default(),
        }
    }
}

/// Publisher trait for sending messages
#[async_trait]
pub trait Publisher<T: BusMessage>: Send + Sync {
    /// Publish a message to the bus
    async fn publish(&self, message: T) -> Result<()>;

    /// Publish a message with custom metadata
    async fn publish_with_metadata(&self, message: T, metadata: MessageMetadata) -> Result<()>;
}

/// Subscriber trait for receiving messages
#[async_trait]
pub trait Subscriber<T: BusMessage>: Send + Sync {
    /// Subscribe to messages
    async fn subscribe(&self) -> Result<broadcast::Receiver<MessageEnvelope<T>>>;
}

/// Message handler trait for processing messages
#[async_trait]
pub trait MessageHandler<T: BusMessage>: Send + Sync {
    /// Handle a message
    async fn handle(&self, envelope: MessageEnvelope<T>) -> Result<()>;

    /// Get handler name for debugging
    fn name(&self) -> &str;
}

/// Error types for event bus operations
#[derive(Debug, thiserror::Error)]
pub enum EventBusError {
    /// Bus capacity exceeded error
    #[error("Bus capacity exceeded")]
    CapacityExceeded,

    /// Message TTL expired error
    #[error("Message TTL expired")]
    MessageExpired,

    /// No subscribers for topic error
    #[error("No subscribers for topic: {topic}")]
    NoSubscribers { 
        /// Topic name
        topic: String 
    },

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Channel error
    #[error("Channel error: {0}")]
    Channel(String),

    /// Handler error
    #[error("Handler error: {source}")]
    Handler { 
        /// Source error
        source: anyhow::Error 
    },
}

/// Result type for event bus operations
pub type BusResult<T> = std::result::Result<T, EventBusError>;

/// Common `ShrivenQuant` message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShrivenQuantMessage {
    /// Market data update
    MarketData {
        /// Trading symbol
        symbol: String,
        /// Exchange name
        exchange: String,
        /// Bid price (fixed-point)
        bid: i64,
        /// Ask price (fixed-point)
        ask: i64,
        /// Event timestamp
        timestamp: u64,
    },

    /// Order event
    OrderEvent {
        /// Order ID
        order_id: u64,
        /// Trading symbol
        symbol: String,
        /// Order side (BUY/SELL)
        side: String,
        /// Order quantity (fixed-point)
        quantity: i64,
        /// Order price (fixed-point)
        price: i64,
        /// Order status
        status: String,
        /// Event timestamp
        timestamp: u64,
    },

    /// Fill event
    FillEvent {
        /// Order ID
        order_id: u64,
        /// Fill ID
        fill_id: String,
        /// Trading symbol
        symbol: String,
        /// Fill quantity (fixed-point)
        quantity: i64,
        /// Fill price (fixed-point)
        price: i64,
        /// Event timestamp
        timestamp: u64,
    },

    /// Position update
    PositionUpdate {
        /// Trading symbol
        symbol: String,
        /// Position quantity (fixed-point)
        quantity: i64,
        /// Average price (fixed-point)
        avg_price: i64,
        /// Unrealized P&L
        unrealized_pnl: i64,
        /// Realized P&L
        realized_pnl: i64,
        /// Event timestamp
        timestamp: u64,
    },

    /// Risk alert
    RiskAlert {
        /// Alert level (INFO, WARNING, CRITICAL, EMERGENCY)
        level: String,
        /// Alert message
        message: String,
        /// Alert source
        source: String,
        /// Optional symbol
        symbol: Option<String>,
        /// Optional value
        value: Option<i64>,
        /// Event timestamp
        timestamp: u64,
    },

    /// Performance metrics
    PerformanceMetrics {
        /// Service name
        service: String,
        /// Metric name
        metric_name: String,
        /// Metric value
        value: f64,
        /// Metric unit
        unit: String,
        /// Metric tags
        tags: rustc_hash::FxHashMap<String, String>,
        /// Event timestamp
        timestamp: u64,
    },

    /// System health check
    HealthCheck {
        /// Service name
        service: String,
        /// Health status (HEALTHY, DEGRADED, UNHEALTHY)
        status: String,
        /// Optional details
        details: Option<String>,
        /// Event timestamp
        timestamp: u64,
    },
}

impl BusMessage for ShrivenQuantMessage {
    fn topic(&self) -> &str {
        match self {
            Self::MarketData { .. } => "market_data",
            Self::OrderEvent { .. } => "orders",
            Self::FillEvent { .. } => "fills",
            Self::PositionUpdate { .. } => "positions",
            Self::RiskAlert { .. } => "risk_alerts",
            Self::PerformanceMetrics { .. } => "performance",
            Self::HealthCheck { .. } => "health",
        }
    }

    fn priority(&self) -> u8 {
        match self {
            Self::RiskAlert { level, .. } => {
                match level.as_str() {
                    "EMERGENCY" => 0, // Highest priority
                    "CRITICAL" => 32,
                    "WARNING" => 64,
                    _ => 96,
                }
            }
            Self::FillEvent { .. } => 16, // Very high
            Self::OrderEvent { .. } => 32, // High
            Self::MarketData { .. } => 48, // Medium-high
            Self::PositionUpdate { .. } => 64, // Medium
            Self::PerformanceMetrics { .. } => 128, // Normal
            Self::HealthCheck { .. } => 160, // Low
        }
    }
}

/// Event bus factory for creating configured instances
#[derive(Debug)]
pub struct EventBusFactory;

impl EventBusFactory {
    /// Create a new event bus with default configuration
    #[must_use] pub fn create_default() -> EventBus<ShrivenQuantMessage> {
        let config = EventBusConfig {
            capacity: 10000,
            enable_metrics: true,
            enable_dead_letter_queue: true,
            max_retry_attempts: 3,
            default_ttl_ms: Some(30000), // 30 seconds
        };

        EventBus::new(config)
    }

    /// Create a high-performance event bus for trading
    #[must_use] pub fn create_high_performance() -> EventBus<ShrivenQuantMessage> {
        let config = EventBusConfig {
            capacity: 100000, // Large capacity for high throughput
            enable_metrics: true,
            enable_dead_letter_queue: false, // Disabled for max performance
            max_retry_attempts: 1,           // Minimal retries
            default_ttl_ms: Some(5000),      // Short TTL (5 seconds)
        };

        EventBus::new(config)
    }

    /// Create a reliable event bus with full features
    #[must_use] pub fn create_reliable() -> EventBus<ShrivenQuantMessage> {
        let config = EventBusConfig {
            capacity: 50000,
            enable_metrics: true,
            enable_dead_letter_queue: true,
            max_retry_attempts: 5,
            default_ttl_ms: Some(60000), // 1 minute
        };

        EventBus::new(config)
    }
}

/// Event bus middleware trait for cross-cutting concerns
#[async_trait]
pub trait EventBusMiddleware<T: BusMessage>: Send + Sync {
    /// Process message before publishing
    async fn before_publish(&self, envelope: &mut MessageEnvelope<T>) -> Result<()>;

    /// Process message after successful publish
    async fn after_publish(&self, envelope: &MessageEnvelope<T>) -> Result<()>;

    /// Process message before handling
    async fn before_handle(&self, envelope: &MessageEnvelope<T>) -> Result<()>;

    /// Process message after handling
    async fn after_handle(&self, envelope: &MessageEnvelope<T>, result: &Result<()>) -> Result<()>;
}

/// Logging middleware for debugging
#[derive(Debug)]
pub struct LoggingMiddleware {
    service_name: String,
}

impl LoggingMiddleware {
    /// Create new logging middleware
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
        }
    }
}

#[async_trait]
impl<T: BusMessage> EventBusMiddleware<T> for LoggingMiddleware {
    async fn before_publish(&self, envelope: &mut MessageEnvelope<T>) -> Result<()> {
        debug!(
            service = %self.service_name,
            message_id = %envelope.metadata.message_id,
            topic = envelope.message.topic(),
            "Publishing message"
        );
        Ok(())
    }

    async fn after_publish(&self, envelope: &MessageEnvelope<T>) -> Result<()> {
        debug!(
            service = %self.service_name,
            message_id = %envelope.metadata.message_id,
            "Message published successfully"
        );
        Ok(())
    }

    async fn before_handle(&self, envelope: &MessageEnvelope<T>) -> Result<()> {
        debug!(
            service = %self.service_name,
            message_id = %envelope.metadata.message_id,
            topic = envelope.message.topic(),
            "Handling message"
        );
        Ok(())
    }

    async fn after_handle(&self, envelope: &MessageEnvelope<T>, result: &Result<()>) -> Result<()> {
        match result {
            Ok(()) => debug!(
                service = %self.service_name,
                message_id = %envelope.metadata.message_id,
                "Message handled successfully"
            ),
            Err(e) => error!(
                service = %self.service_name,
                message_id = %envelope.metadata.message_id,
                error = %e,
                "Message handling failed"
            ),
        }
        Ok(())
    }
}

/// Metrics middleware for performance monitoring
#[derive(Debug)]
pub struct MetricsMiddleware {
    metrics: std::sync::Arc<BusMetrics>,
}

impl MetricsMiddleware {
    /// Create new metrics middleware
    pub const fn new(metrics: std::sync::Arc<BusMetrics>) -> Self {
        Self { metrics }
    }
}

#[async_trait]
impl<T: BusMessage> EventBusMiddleware<T> for MetricsMiddleware {
    async fn before_publish(&self, envelope: &mut MessageEnvelope<T>) -> Result<()> {
        self.metrics
            .record_publish_attempt(envelope.message.topic());
        Ok(())
    }

    async fn after_publish(&self, envelope: &MessageEnvelope<T>) -> Result<()> {
        self.metrics
            .record_publish_success(envelope.message.topic());
        Ok(())
    }

    async fn before_handle(&self, envelope: &MessageEnvelope<T>) -> Result<()> {
        self.metrics.record_handle_attempt(envelope.message.topic());
        Ok(())
    }

    async fn after_handle(&self, envelope: &MessageEnvelope<T>, result: &Result<()>) -> Result<()> {
        match result {
            Ok(()) => self.metrics.record_handle_success(envelope.message.topic()),
            Err(error) => {
                tracing::warn!("Message handling failed: {}", error);
                self.metrics.record_handle_failure(envelope.message.topic());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct TestMessage {
        id: u64,
        data: String,
    }

    impl BusMessage for TestMessage {
        fn topic(&self) -> &str {
            "test"
        }
    }

    #[tokio::test]
    async fn test_message_metadata() {
        // Use TestMessage to verify it's properly constructed
        let test_msg = TestMessage {
            id: 42,
            data: "test payload".to_string(),
        };
        assert_eq!(test_msg.topic(), "test");
        assert_eq!(test_msg.id, 42);
        assert_eq!(test_msg.data, "test payload");
        
        let metadata = MessageMetadata::default();
        assert!(!metadata.message_id.is_empty());
        assert_eq!(metadata.source, "unknown");
        assert!(metadata.timestamp > 0);
    }

    #[test]
    fn test_shrivenquant_message_topics() {
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

    #[tokio::test]
    async fn test_event_bus_factory() {
        let bus = EventBusFactory::create_default();
        assert!(bus.capacity() > 0);

        let hp_bus = EventBusFactory::create_high_performance();
        assert!(hp_bus.capacity() >= 100000);
    }
}
