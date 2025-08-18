//! Message types and envelope for the event bus

use super::{BusMessage, MessageMetadata};
use serde::{Deserialize, Serialize};

/// Message envelope wrapping the actual message with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEnvelope<T: BusMessage> {
    /// The actual message
    pub message: T,
    /// Message metadata
    pub metadata: MessageMetadata,
}

impl<T: BusMessage> MessageEnvelope<T> {
    /// Create a new message envelope
    pub const fn new(message: T, metadata: MessageMetadata) -> Self {
        Self { message, metadata }
    }

    /// Create a message envelope with default metadata
    pub fn with_defaults(message: T, source: impl Into<String>) -> Self {
        let metadata = MessageMetadata {
            source: source.into(),
            ..Default::default()
        };
        Self { message, metadata }
    }

    /// Get the message topic
    pub fn topic(&self) -> &str {
        self.message.topic()
    }

    /// Get the message priority
    pub fn priority(&self) -> u8 {
        self.message.priority()
    }

    /// Check if message has expired
    pub fn is_expired(&self) -> bool {
        if let Some(ttl_ms) = self.metadata.ttl_ms {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                // SAFETY: u128 to u64 - milliseconds since epoch fits in u64
                .as_millis() as u64;
            // SAFETY: u64 arithmetic result to u64
            let message_age = now - self.metadata.timestamp / 1_000_000;
            message_age > ttl_ms
        } else {
            false
        }
    }

    /// Get message age in milliseconds
    pub fn age_ms(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            // SAFETY: u128 to u64 - milliseconds since epoch fits in u64
            .as_millis() as u64;
        // SAFETY: u64 arithmetic result to u64
        now - self.metadata.timestamp / 1_000_000
    }
}

/// Message type enumeration for routing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageType {
    /// Market data updates
    MarketData,
    /// Trading orders
    Order,
    /// Order fills
    Fill,
    /// Position updates
    Position,
    /// Risk alerts
    Risk,
    /// Performance metrics
    Performance,
    /// System health
    Health,
    /// Custom message type
    Custom,
}

impl std::fmt::Display for MessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MarketData => write!(f, "market_data"),
            Self::Order => write!(f, "order"),
            Self::Fill => write!(f, "fill"),
            Self::Position => write!(f, "position"),
            Self::Risk => write!(f, "risk"),
            Self::Performance => write!(f, "performance"),
            Self::Health => write!(f, "health"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

/// Message trait for basic messages
pub trait Message: Send + Sync + Clone + std::fmt::Debug {
    /// Get message type
    fn message_type(&self) -> MessageType;

    /// Serialize message to bytes
    fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error>;

    /// Deserialize message from bytes
    fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error>
    where
        Self: Sized;
}

/// Default implementation for serializable messages
impl<T> Message for T
where
    T: Send + Sync + Clone + std::fmt::Debug + Serialize + for<'de> Deserialize<'de>,
{
    fn message_type(&self) -> MessageType {
        MessageType::Custom
    }

    fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestMessage {
        id: u64,
        data: String,
    }

    impl BusMessage for TestMessage {
        fn topic(&self) -> &str {
            "test"
        }
    }

    #[test]
    fn test_message_envelope_creation() {
        let message = TestMessage {
            id: 1,
            data: "test".to_string(),
        };

        let envelope = MessageEnvelope::with_defaults(message.clone(), "test_service");
        assert_eq!(envelope.topic(), "test");
        assert_eq!(envelope.metadata.source, "test_service");
        assert!(!envelope.metadata.message_id.is_empty());
    }

    #[test]
    fn test_message_expiration() {
        let message = TestMessage {
            id: 1,
            data: "test".to_string(),
        };

        let mut metadata = MessageMetadata::default();
        metadata.ttl_ms = Some(1); // 1ms TTL
        metadata.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            // SAFETY: u128 to u64 - nanoseconds since epoch fits in u64
            .as_nanos() as u64;

        let envelope = MessageEnvelope::new(message, metadata);

        // Should not be expired immediately
        assert!(!envelope.is_expired());

        // Wait and check again
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(envelope.is_expired());
    }

    #[test]
    fn test_message_serialization() {
        let message = TestMessage {
            id: 42,
            data: "hello world".to_string(),
        };

        let bytes = message.to_bytes().unwrap();
        let deserialized = TestMessage::from_bytes(&bytes).unwrap();

        assert_eq!(message.id, deserialized.id);
        assert_eq!(message.data, deserialized.data);
    }

    #[test]
    fn test_message_type_display() {
        assert_eq!(MessageType::MarketData.to_string(), "market_data");
        assert_eq!(MessageType::Order.to_string(), "order");
        assert_eq!(MessageType::Risk.to_string(), "risk");
    }
}
