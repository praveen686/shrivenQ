//! Event types for orderbook updates and market data
//!
//! This module defines all event types used in the orderbook system.
//! Events are designed to be:
//! - Zero-copy where possible
//! - Cache-aligned for optimal performance
//! - Deterministically ordered for replay
//! - Compact for network transmission

use common::{Px, Qty, Symbol, Ts};
use serde::{Deserialize, Serialize};

/// Side of an order or trade
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Side {
    /// Buy side (bid)
    Buy = 0,
    /// Sell side (ask/offer)
    Sell = 1,
}

impl Side {
    /// Check if this is the buy side
    #[inline]
    pub fn is_buy(&self) -> bool {
        matches!(self, Side::Buy)
    }

    /// Get the opposite side
    #[inline]
    pub fn opposite(&self) -> Self {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }
}

/// Type of order book update
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum UpdateType {
    /// New order added to book
    Add = 0,
    /// Existing order modified (quantity change)
    Modify = 1,
    /// Order removed from book
    Delete = 2,
    /// Trade executed
    Trade = 3,
    /// Full orderbook snapshot
    Snapshot = 4,
    /// Incremental update (diff)
    Delta = 5,
    /// Clear all orders (reset)
    Clear = 6,
}

/// Individual order update event (L3 data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderUpdate {
    /// Unique order identifier
    pub order_id: u64,
    /// Price of the order
    pub price: Px,
    /// Quantity (0 for deletions)
    pub quantity: Qty,
    /// Side of the order
    pub side: Side,
    /// Type of update
    pub update_type: UpdateType,
    /// Exchange timestamp
    pub exchange_time: Ts,
    /// Local receipt timestamp
    pub local_time: Ts,
    /// Sequence number for ordering
    pub sequence: u64,
}

/// Trade execution event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeEvent {
    /// Trade identifier
    pub trade_id: u64,
    /// Execution price
    pub price: Px,
    /// Executed quantity
    pub quantity: Qty,
    /// Aggressor side (who crossed the spread)
    pub aggressor_side: Side,
    /// Maker order ID (passive side)
    pub maker_order_id: Option<u64>,
    /// Taker order ID (aggressive side)
    pub taker_order_id: Option<u64>,
    /// Exchange timestamp
    pub exchange_time: Ts,
    /// Local receipt timestamp
    pub local_time: Ts,
    /// Sequence number
    pub sequence: u64,
}

/// Price level update (L2 data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelUpdate {
    /// Price of the level
    pub price: Px,
    /// Total quantity at this level
    pub quantity: Qty,
    /// Number of orders at this level
    pub order_count: u64,
    /// Side of the level
    pub side: Side,
}

/// Complete orderbook snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    /// Symbol
    pub symbol: Symbol,
    /// Bid levels
    pub bids: Vec<LevelUpdate>,
    /// Ask levels
    pub asks: Vec<LevelUpdate>,
    /// Snapshot sequence number
    pub sequence: u64,
    /// Exchange timestamp
    pub exchange_time: Ts,
    /// Local timestamp
    pub local_time: Ts,
    /// Checksum for validation
    pub checksum: u32,
}

/// Incremental orderbook update (diff)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookDelta {
    /// Symbol
    pub symbol: Symbol,
    /// Updated bid levels
    pub bid_updates: Vec<LevelUpdate>,
    /// Updated ask levels
    pub ask_updates: Vec<LevelUpdate>,
    /// Removed bid prices
    pub bid_deletions: Vec<Px>,
    /// Removed ask prices
    pub ask_deletions: Vec<Px>,
    /// Starting sequence number
    pub prev_sequence: u64,
    /// Ending sequence number
    pub sequence: u64,
    /// Exchange timestamp
    pub exchange_time: Ts,
    /// Local timestamp
    pub local_time: Ts,
}

/// Market-wide event affecting the orderbook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketEvent {
    /// Type of market event
    pub event_type: MarketEventType,
    /// Affected symbol (if specific)
    pub symbol: Option<Symbol>,
    /// Event timestamp
    pub timestamp: Ts,
    /// Additional message
    pub message: String,
}

/// Types of market events
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum MarketEventType {
    /// Trading halted
    TradingHalt = 0,
    /// Trading resumed
    TradingResume = 1,
    /// Market open
    MarketOpen = 2,
    /// Market close
    MarketClose = 3,
    /// Circuit breaker triggered
    CircuitBreaker = 4,
    /// Auction start
    AuctionStart = 5,
    /// Auction end
    AuctionEnd = 6,
}

/// Main orderbook event enum
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderBookEvent {
    /// Order update (add/modify/delete)
    Order(OrderUpdate),
    /// Trade execution
    Trade(TradeEvent),
    /// Full snapshot
    Snapshot(OrderBookSnapshot),
    /// Incremental update
    Delta(OrderBookDelta),
    /// Market event
    Market(MarketEvent),
}

impl OrderBookEvent {
    /// Get the sequence number of this event
    pub fn sequence(&self) -> u64 {
        match self {
            Self::Order(o) => o.sequence,
            Self::Trade(t) => t.sequence,
            Self::Snapshot(s) => s.sequence,
            Self::Delta(d) => d.sequence,
            Self::Market(_) => 0, // Market events don't have sequence
        }
    }

    /// Get the exchange timestamp
    pub fn exchange_time(&self) -> Ts {
        match self {
            Self::Order(o) => o.exchange_time,
            Self::Trade(t) => t.exchange_time,
            Self::Snapshot(s) => s.exchange_time,
            Self::Delta(d) => d.exchange_time,
            Self::Market(m) => m.timestamp,
        }
    }

    /// Get the local timestamp
    pub fn local_time(&self) -> Option<Ts> {
        match self {
            Self::Order(o) => Some(o.local_time),
            Self::Trade(t) => Some(t.local_time),
            Self::Snapshot(s) => Some(s.local_time),
            Self::Delta(d) => Some(d.local_time),
            Self::Market(_) => None,
        }
    }

    /// Check if this is a snapshot event
    #[inline]
    pub fn is_snapshot(&self) -> bool {
        matches!(self, Self::Snapshot(_))
    }

    /// Check if this is a trade event
    #[inline]
    pub fn is_trade(&self) -> bool {
        matches!(self, Self::Trade(_))
    }
}

/// Statistics for a batch of events
#[derive(Debug, Clone, Default)]
pub struct EventBatchStats {
    /// Number of order adds
    pub adds: u64,
    /// Number of order modifies
    pub modifies: u64,
    /// Number of order deletes
    pub deletes: u64,
    /// Number of trades
    pub trades: u64,
    /// Total volume traded
    pub volume: i64,
    /// Min latency in nanoseconds
    pub min_latency_ns: u64,
    /// Max latency in nanoseconds
    pub max_latency_ns: u64,
    /// Average latency in nanoseconds
    pub avg_latency_ns: u64,
}

/// Event builder for constructing events efficiently
pub struct EventBuilder {
    sequence_counter: u64,
}

impl EventBuilder {
    /// Create a new event builder
    pub fn new() -> Self {
        Self {
            sequence_counter: 0,
        }
    }

    /// Build an order add event
    pub fn order_add(&mut self, order_id: u64, price: Px, quantity: Qty, side: Side) -> OrderUpdate {
        self.sequence_counter += 1;
        OrderUpdate {
            order_id,
            price,
            quantity,
            side,
            update_type: UpdateType::Add,
            exchange_time: Ts::now(),
            local_time: Ts::now(),
            sequence: self.sequence_counter,
        }
    }

    /// Build an order modify event
    pub fn order_modify(&mut self, order_id: u64, price: Px, new_quantity: Qty, side: Side) -> OrderUpdate {
        self.sequence_counter += 1;
        OrderUpdate {
            order_id,
            price,
            quantity: new_quantity,
            side,
            update_type: UpdateType::Modify,
            exchange_time: Ts::now(),
            local_time: Ts::now(),
            sequence: self.sequence_counter,
        }
    }

    /// Build an order delete event
    pub fn order_delete(&mut self, order_id: u64, price: Px, side: Side) -> OrderUpdate {
        self.sequence_counter += 1;
        OrderUpdate {
            order_id,
            price,
            quantity: Qty::ZERO,
            side,
            update_type: UpdateType::Delete,
            exchange_time: Ts::now(),
            local_time: Ts::now(),
            sequence: self.sequence_counter,
        }
    }

    /// Build a trade event
    pub fn trade(&mut self, trade_id: u64, price: Px, quantity: Qty, aggressor_side: Side) -> TradeEvent {
        self.sequence_counter += 1;
        TradeEvent {
            trade_id,
            price,
            quantity,
            aggressor_side,
            maker_order_id: None,
            taker_order_id: None,
            exchange_time: Ts::now(),
            local_time: Ts::now(),
            sequence: self.sequence_counter,
        }
    }

    /// Get current sequence number
    pub fn current_sequence(&self) -> u64 {
        self.sequence_counter
    }

    /// Reset sequence counter
    pub fn reset_sequence(&mut self) {
        self.sequence_counter = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_side_operations() {
        assert!(Side::Buy.is_buy());
        assert!(!Side::Sell.is_buy());
        assert_eq!(Side::Buy.opposite(), Side::Sell);
        assert_eq!(Side::Sell.opposite(), Side::Buy);
    }

    #[test]
    fn test_event_builder() {
        let mut builder = EventBuilder::new();
        
        let add = builder.order_add(1, Px::from_i64(100_000), Qty::from_i64(10), Side::Buy);
        assert_eq!(add.sequence, 1);
        assert_eq!(add.update_type, UpdateType::Add);
        
        let modify = builder.order_modify(1, Px::from_i64(100_000), Qty::from_i64(5), Side::Buy);
        assert_eq!(modify.sequence, 2);
        assert_eq!(modify.update_type, UpdateType::Modify);
        
        let delete = builder.order_delete(1, Px::from_i64(100_000), Side::Buy);
        assert_eq!(delete.sequence, 3);
        assert_eq!(delete.update_type, UpdateType::Delete);
    }
}