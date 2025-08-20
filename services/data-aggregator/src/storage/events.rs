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
    /// Timestamp of the candle event
    pub ts: Ts,
    /// Symbol identifier
    pub symbol: Symbol,
    /// Timeframe in seconds
    pub timeframe: u32,
    /// Opening price
    pub open: Px,
    /// Highest price
    pub high: Px,
    /// Lowest price
    pub low: Px,
    /// Closing price
    pub close: Px,
    /// Total volume traded
    pub volume: Qty,
    /// Number of trades
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
    /// Timestamp of the trade
    pub ts: Ts,
    /// Symbol identifier
    pub symbol: Symbol,
    /// Trade price
    pub price: Px,
    /// Trade quantity
    pub quantity: Qty,
    /// True if buy order, false if sell order
    pub is_buy: bool,
    /// Unique trade identifier
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
    /// Timestamp of the volume profile update
    pub ts: Ts,
    /// Symbol identifier
    pub symbol: Symbol,
    /// Price level for this volume data
    pub price_level: Px,
    /// Volume from buy orders at this price level
    pub buy_volume: Qty,
    /// Volume from sell orders at this price level
    pub sell_volume: Qty,
    /// Number of trades at this price level
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
    /// Timestamp of the microstructure event
    pub ts: Ts,
    /// Symbol identifier
    pub symbol: Symbol,
    /// Best bid price
    pub bid: Px,
    /// Best ask price
    pub ask: Px,
    /// Size at best bid
    pub bid_size: Qty,
    /// Size at best ask
    pub ask_size: Qty,
    /// Spread in fixed-point basis points
    pub spread: i64,
    /// Order book imbalance as fixed-point ratio
    pub imbalance: i64,
}

impl WalEntry for MicrostructureEvent {
    fn timestamp(&self) -> Ts {
        self.ts
    }
}

/// System event for metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    /// Timestamp of the system event
    pub ts: Ts,
    /// Type of system event
    pub event_type: SystemEventType,
    /// Human-readable message describing the event
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
    /// Timestamp of the order book event
    pub ts: Ts,
    /// Symbol identifier
    pub symbol: Symbol,
    /// Type of order book event (snapshot, update, or clear)
    pub event_type: OrderBookEventType,
    /// Sequence number for ordering events
    pub sequence: u64,
    /// Bid price levels (price, quantity, order_count)
    pub bid_levels: Vec<(Px, Qty, u32)>,
    /// Ask price levels (price, quantity, order_count)
    pub ask_levels: Vec<(Px, Qty, u32)>,
    /// Checksum for data integrity verification
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
    /// Full order book snapshot
    Snapshot,
    /// Incremental order book update
    Update,
    /// Clear all order book data
    Clear,
}

/// System event types
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SystemEventType {
    /// System or service startup event
    Start,
    /// System or service shutdown event
    Stop,
    /// Checkpoint or milestone event
    Checkpoint,
    /// Error condition event
    Error,
    /// Informational event
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
    /// Total volume traded in the period
    pub total_volume: Qty,
    /// Total number of trades in the period
    pub total_trades: u64,
    /// Average spread in fixed-point representation
    pub avg_spread: i64,
    /// Maximum spread observed in fixed-point representation
    pub max_spread: i64,
    /// Minimum spread observed in fixed-point representation
    pub min_spread: i64,
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
