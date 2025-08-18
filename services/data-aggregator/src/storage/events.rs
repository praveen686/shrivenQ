//! Event types for data-aggregator WAL persistence
//!
//! All events use fixed-point arithmetic and pre-allocated structures

use services_common::{Px, Qty, Symbol, Ts};
use serde::{Deserialize, Serialize};

use super::wal::WalEntry;

/// Base event types for data aggregator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataEvent {
    /// OHLCV candle data
    Candle(CandleEvent),
    /// Trade aggregation data
    Trade(TradeEvent),
    /// Volume profile update
    VolumeProfile(VolumeProfileEvent),
    /// Market microstructure event
    Microstructure(MicrostructureEvent),
    /// `OrderBook` snapshot or update
    OrderBook(OrderBookEvent),
    /// System event
    System(SystemEvent),
}

impl WalEntry for DataEvent {
    fn timestamp(&self) -> Ts {
        match self {
            Self::Candle(e) => e.ts,
            Self::Trade(e) => e.ts,
            Self::VolumeProfile(e) => e.ts,
            Self::Microstructure(e) => e.ts,
            Self::OrderBook(e) => e.ts,
            Self::System(e) => e.ts,
        }
    }
}

/// OHLCV candle event
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CandleEvent {
    pub ts: Ts,
    pub symbol: Symbol,
    pub timeframe: u32, // Seconds
    pub open: Px,
    pub high: Px,
    pub low: Px,
    pub close: Px,
    pub volume: Qty,
    pub trades: u32,
}

impl WalEntry for CandleEvent {
    fn timestamp(&self) -> Ts {
        self.ts
    }
}

/// Trade aggregation event
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TradeEvent {
    pub ts: Ts,
    pub symbol: Symbol,
    pub price: Px,
    pub quantity: Qty,
    pub is_buy: bool,
    pub trade_id: u64,
}

impl WalEntry for TradeEvent {
    fn timestamp(&self) -> Ts {
        self.ts
    }
}

/// Volume profile event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeProfileEvent {
    pub ts: Ts,
    pub symbol: Symbol,
    pub price_level: Px,
    pub buy_volume: Qty,
    pub sell_volume: Qty,
    pub trades: u32,
}

impl WalEntry for VolumeProfileEvent {
    fn timestamp(&self) -> Ts {
        self.ts
    }
}

/// Market microstructure event
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MicrostructureEvent {
    pub ts: Ts,
    pub symbol: Symbol,
    pub bid: Px,
    pub ask: Px,
    pub bid_size: Qty,
    pub ask_size: Qty,
    pub spread: i64,    // Fixed-point basis points
    pub imbalance: i64, // Fixed-point ratio
}

impl WalEntry for MicrostructureEvent {
    fn timestamp(&self) -> Ts {
        self.ts
    }
}

/// System event for metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    pub ts: Ts,
    pub event_type: SystemEventType,
    pub message: String,
}

impl WalEntry for SystemEvent {
    fn timestamp(&self) -> Ts {
        self.ts
    }
}

/// `OrderBook` event for snapshots and updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookEvent {
    pub ts: Ts,
    pub symbol: Symbol,
    pub event_type: OrderBookEventType,
    pub sequence: u64,
    pub bid_levels: Vec<(Px, Qty, u32)>, // (price, quantity, order_count)
    pub ask_levels: Vec<(Px, Qty, u32)>,
    pub checksum: u32,
}

impl WalEntry for OrderBookEvent {
    fn timestamp(&self) -> Ts {
        self.ts
    }
}

/// `OrderBook` event types
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum OrderBookEventType {
    Snapshot,
    Update,
    Clear,
}

/// System event types
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SystemEventType {
    Start,
    Stop,
    Checkpoint,
    Error,
    Info,
}

impl std::fmt::Display for SystemEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start => write!(f, "START"),
            Self::Stop => write!(f, "STOP"),
            Self::Checkpoint => write!(f, "CHECKPOINT"),
            Self::Error => write!(f, "ERROR"),
            Self::Info => write!(f, "INFO"),
        }
    }
}

/// Statistics for a time period
#[derive(Debug, Clone, Copy)]
pub struct PeriodStats {
    pub total_volume: Qty,
    pub total_trades: u64,
    pub avg_spread: i64, // Fixed-point
    pub max_spread: i64, // Fixed-point
    pub min_spread: i64, // Fixed-point
}

impl Default for PeriodStats {
    fn default() -> Self {
        Self {
            total_volume: Qty::ZERO,
            total_trades: 0,
            avg_spread: 0,
            max_spread: 0,
            min_spread: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_timestamp() {
        let ts = Ts::from_nanos(1000);

        let candle = DataEvent::Candle(CandleEvent {
            ts,
            symbol: Symbol::new(1),
            timeframe: 60,
            open: Px::from_i64(100_0000),
            high: Px::from_i64(101_0000),
            low: Px::from_i64(99_0000),
            close: Px::from_i64(100_5000),
            volume: Qty::from_i64(1000_0000),
            trades: 50,
        });

        assert_eq!(candle.timestamp(), ts);
    }

    #[test]
    fn test_event_serialization() {
        let event = TradeEvent {
            ts: Ts::from_nanos(2000),
            symbol: Symbol::new(1),
            price: Px::from_i64(100_0000),
            quantity: Qty::from_i64(10_0000),
            is_buy: true,
            trade_id: 12345,
        };

        let serialized = bincode::serialize(&event).expect("Serialization failed");
        let deserialized: TradeEvent =
            bincode::deserialize(&serialized).expect("Deserialization failed");

        assert_eq!(event.ts, deserialized.ts);
        assert_eq!(event.price, deserialized.price);
        assert_eq!(event.quantity, deserialized.quantity);
    }
}
