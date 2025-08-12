//! Canonical event types for WAL persistence

use common::{Px, Qty, Symbol, Ts};
use serde::{Deserialize, Serialize};

/// Core event that can be persisted to WAL
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WalEvent {
    /// Market tick data
    Tick(TickEvent),
    /// Order submission event
    Order(OrderEvent),
    /// Order fill event
    Fill(FillEvent),
    /// Trading signal event
    Signal(SignalEvent),
    /// Risk decision event
    Risk(RiskEvent),
    /// System event (startup, shutdown, etc)
    System(SystemEvent),
    /// LOB snapshot event
    Lob(LobSnapshot),
}

impl WalEvent {
    /// Get the timestamp of the event
    #[must_use]
    pub const fn timestamp(&self) -> Ts {
        match self {
            Self::Tick(e) => e.ts,
            Self::Order(e) => e.ts,
            Self::Fill(e) => e.ts,
            Self::Signal(e) => e.ts,
            Self::Risk(e) => e.ts,
            Self::System(e) => e.ts,
            Self::Lob(e) => e.ts,
        }
    }
}

/// Market tick event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TickEvent {
    /// Event timestamp
    pub ts: Ts,
    /// Trading venue (e.g., "zerodha", "binance")
    pub venue: String,
    /// Trading symbol
    pub symbol: Symbol,
    /// Best bid price
    pub bid: Option<Px>,
    /// Best ask price
    pub ask: Option<Px>,
    /// Last traded price
    pub last: Option<Px>,
    /// Volume
    pub volume: Option<Qty>,
}

/// Order submission event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrderEvent {
    /// Event timestamp
    pub ts: Ts,
    /// Order ID
    pub order_id: u64,
    /// Trading symbol
    pub symbol: Symbol,
    /// Order side
    pub side: OrderSide,
    /// Order quantity
    pub qty: Qty,
    /// Order price (None for market orders)
    pub price: Option<Px>,
    /// Order type
    pub order_type: OrderType,
    /// Order status
    pub status: OrderStatus,
}

/// Order side
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderSide {
    /// Buy order
    Buy,
    /// Sell order
    Sell,
}

/// Order type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderType {
    /// Market order
    Market,
    /// Limit order
    Limit,
    /// Stop order
    Stop,
}

/// Order status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderStatus {
    /// Order created locally
    New,
    /// Order acknowledged by broker
    Acknowledged,
    /// Order partially filled
    PartiallyFilled,
    /// Order fully filled
    Filled,
    /// Order rejected
    Rejected,
    /// Order cancelled
    Cancelled,
}

/// Order fill event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FillEvent {
    /// Event timestamp
    pub ts: Ts,
    /// Order ID
    pub order_id: u64,
    /// Fill ID
    pub fill_id: u64,
    /// Trading symbol
    pub symbol: Symbol,
    /// Fill side
    pub side: OrderSide,
    /// Fill quantity
    pub qty: Qty,
    /// Fill price
    pub price: Px,
    /// Commission paid
    pub commission: f64,
}

/// Trading signal event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignalEvent {
    /// Event timestamp
    pub ts: Ts,
    /// Trading symbol
    pub symbol: Symbol,
    /// Signal strength (-1.0 to 1.0, negative = sell, positive = buy)
    pub strength: f64,
    /// Strategy that generated the signal
    pub strategy: String,
    /// Rationale for the signal
    pub rationale: String,
    /// Stop loss price
    pub stop: Option<Px>,
    /// Take profit price
    pub take: Option<Px>,
    /// Signal horizon in milliseconds
    pub horizon_ms: u64,
}

/// Risk decision event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiskEvent {
    /// Event timestamp
    pub ts: Ts,
    /// Order or signal ID being evaluated
    pub id: u64,
    /// Risk verdict
    pub verdict: RiskVerdict,
    /// Reason for the verdict
    pub reason: String,
}

/// Risk verdict
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RiskVerdict {
    /// Risk check passed
    Approved,
    /// Risk check failed
    Rejected,
    /// Risk check resulted in modification
    Modified,
}

/// System event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SystemEvent {
    /// Event timestamp
    pub ts: Ts,
    /// Event type
    pub event_type: SystemEventType,
    /// Event message
    pub message: String,
}

/// System event type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SystemEventType {
    /// System startup
    Startup,
    /// System shutdown
    Shutdown,
    /// Configuration change
    ConfigChange,
    /// Error occurred
    Error,
    /// Warning
    Warning,
    /// Informational
    Info,
}

/// LOB snapshot event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LobSnapshot {
    /// Event timestamp
    pub ts: Ts,
    /// Trading symbol
    pub symbol: Symbol,
    /// Venue/exchange
    pub venue: String,
    /// Bid levels (price, quantity)
    pub bids: Vec<(Px, Qty)>,
    /// Ask levels (price, quantity)
    pub asks: Vec<(Px, Qty)>,
    /// Sequence number for ordering
    pub sequence: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let tick = TickEvent {
            ts: Ts::from_nanos(1_234_567_890),
            venue: "zerodha".to_string(),
            symbol: Symbol::new(42),
            bid: Some(Px::new(100.0)),
            ask: Some(Px::new(100.5)),
            last: Some(Px::new(100.25)),
            volume: Some(Qty::new(1000.0)),
        };

        let event = WalEvent::Tick(tick);
        let encoded = bincode::serialize(&event)?;
        let decoded: WalEvent = bincode::deserialize(&encoded)?;

        assert_eq!(event, decoded);
        assert_eq!(event.timestamp(), Ts::from_nanos(1_234_567_890));
        Ok(())
    }

    #[test]
    fn test_all_event_types() -> Result<(), Box<dyn std::error::Error>> {
        let events = vec![
            WalEvent::Tick(TickEvent {
                ts: Ts::from_nanos(1),
                venue: "test".to_string(),
                symbol: Symbol::new(1),
                bid: None,
                ask: None,
                last: None,
                volume: None,
            }),
            WalEvent::Order(OrderEvent {
                ts: Ts::from_nanos(2),
                order_id: 1,
                symbol: Symbol::new(1),
                side: OrderSide::Buy,
                qty: Qty::new(100.0),
                price: Some(Px::new(50.0)),
                order_type: OrderType::Limit,
                status: OrderStatus::New,
            }),
            WalEvent::System(SystemEvent {
                ts: Ts::from_nanos(3),
                event_type: SystemEventType::Startup,
                message: "System started".to_string(),
            }),
            WalEvent::Lob(LobSnapshot {
                ts: Ts::from_nanos(4),
                symbol: Symbol::new(1),
                venue: "test".to_string(),
                bids: vec![(Px::new(100.0), Qty::new(10.0))],
                asks: vec![(Px::new(101.0), Qty::new(10.0))],
                sequence: 1,
            }),
        ];

        for event in events {
            let encoded = bincode::serialize(&event)?;
            let decoded: WalEvent = bincode::deserialize(&encoded)?;
            assert_eq!(event, decoded);
        }
        Ok(())
    }
}
