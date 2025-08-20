//! Comprehensive unit tests for orderbook events
//! 
//! Tests cover:
//! - Event creation and serialization
//! - Event builder functionality
//! - Event type conversions and validations
//! - Batch statistics calculation
//! - Sequence number management
//! - Timestamp handling and latency calculations

use orderbook::events::{
    OrderBookEvent, OrderUpdate, TradeEvent, LevelUpdate, OrderBookSnapshot, 
    OrderBookDelta, MarketEvent, MarketEventType, UpdateType, Side, 
    EventBuilder, EventBatchStats
};
use services_common::{Px, Qty, Ts, Symbol};
use serde_json;
use bincode;

/// Helper function to create test order update
fn create_test_order_update(order_id: u64, price: i64, quantity: i64, side: Side, update_type: UpdateType) -> OrderUpdate {
    OrderUpdate {
        order_id,
        price: Px::from_i64(price),
        quantity: Qty::from_i64(quantity),
        side,
        update_type,
        exchange_time: Ts::now(),
        local_time: Ts::now(),
        sequence: 1,
    }
}

/// Helper function to create test trade event
fn create_test_trade(trade_id: u64, price: i64, quantity: i64, aggressor_side: Side) -> TradeEvent {
    TradeEvent {
        trade_id,
        price: Px::from_i64(price),
        quantity: Qty::from_i64(quantity),
        aggressor_side,
        maker_order_id: Some(123),
        taker_order_id: Some(456),
        exchange_time: Ts::now(),
        local_time: Ts::now(),
        sequence: 1,
    }
}

#[cfg(test)]
mod event_creation_tests {
    use super::*;

    #[test]
    fn test_side_operations() {
        assert!(Side::Buy.is_buy());
        assert!(!Side::Sell.is_buy());
        
        assert_eq!(Side::Buy.opposite(), Side::Sell);
        assert_eq!(Side::Sell.opposite(), Side::Buy);
    }

    #[test]
    fn test_order_update_creation() {
        let update = create_test_order_update(12345, 100_000, 10_000, Side::Buy, UpdateType::Add);
        
        assert_eq!(update.order_id, 12345);
        assert_eq!(update.price, Px::from_i64(100_000));
        assert_eq!(update.quantity, Qty::from_i64(10_000));
        assert_eq!(update.side, Side::Buy);
        assert_eq!(update.update_type, UpdateType::Add);
        assert!(update.exchange_time.as_nanos() > 0);
        assert!(update.local_time.as_nanos() > 0);
    }

    #[test]
    fn test_trade_event_creation() {
        let trade = create_test_trade(54321, 100_500, 5_000, Side::Sell);
        
        assert_eq!(trade.trade_id, 54321);
        assert_eq!(trade.price, Px::from_i64(100_500));
        assert_eq!(trade.quantity, Qty::from_i64(5_000));
        assert_eq!(trade.aggressor_side, Side::Sell);
        assert_eq!(trade.maker_order_id, Some(123));
        assert_eq!(trade.taker_order_id, Some(456));
    }

    #[test]
    fn test_level_update_creation() {
        let level = LevelUpdate {
            price: Px::from_i64(99_000),
            quantity: Qty::from_i64(25_000),
            order_count: 3,
            side: Side::Bid,
        };
        
        assert_eq!(level.price, Px::from_i64(99_000));
        assert_eq!(level.quantity, Qty::from_i64(25_000));
        assert_eq!(level.order_count, 3);
        assert_eq!(level.side, Side::Bid);
    }

    #[test]
    fn test_orderbook_snapshot_creation() {
        let bids = vec![
            LevelUpdate { price: Px::from_i64(100_000), quantity: Qty::from_i64(10_000), order_count: 2, side: Side::Bid },
            LevelUpdate { price: Px::from_i64(99_000), quantity: Qty::from_i64(15_000), order_count: 3, side: Side::Bid },
        ];
        
        let asks = vec![
            LevelUpdate { price: Px::from_i64(101_000), quantity: Qty::from_i64(8_000), order_count: 1, side: Side::Sell },
            LevelUpdate { price: Px::from_i64(102_000), quantity: Qty::from_i64(12_000), order_count: 2, side: Side::Sell },
        ];
        
        let snapshot = OrderBookSnapshot {
            symbol: Symbol::new(1),
            bids: bids.clone(),
            asks: asks.clone(),
            sequence: 12345,
            exchange_time: Ts::now(),
            local_time: Ts::now(),
            checksum: 0xDEADBEEF,
        };
        
        assert_eq!(snapshot.symbol, Symbol::new(1));
        assert_eq!(snapshot.bids.len(), 2);
        assert_eq!(snapshot.asks.len(), 2);
        assert_eq!(snapshot.sequence, 12345);
        assert_eq!(snapshot.checksum, 0xDEADBEEF);
    }

    #[test]
    fn test_orderbook_delta_creation() {
        let bid_updates = vec![
            LevelUpdate { price: Px::from_i64(100_000), quantity: Qty::from_i64(5_000), order_count: 1, side: Side::Bid },
        ];
        
        let ask_updates = vec![
            LevelUpdate { price: Px::from_i64(101_000), quantity: Qty::from_i64(3_000), order_count: 1, side: Side::Sell },
        ];
        
        let bid_deletions = vec![Px::from_i64(99_000)];
        let ask_deletions = vec![Px::from_i64(102_000)];
        
        let delta = OrderBookDelta {
            symbol: Symbol::new(2),
            bid_updates,
            ask_updates,
            bid_deletions,
            ask_deletions,
            prev_sequence: 100,
            sequence: 101,
            exchange_time: Ts::now(),
            local_time: Ts::now(),
        };
        
        assert_eq!(delta.symbol, Symbol::new(2));
        assert_eq!(delta.bid_updates.len(), 1);
        assert_eq!(delta.ask_updates.len(), 1);
        assert_eq!(delta.bid_deletions.len(), 1);
        assert_eq!(delta.ask_deletions.len(), 1);
        assert_eq!(delta.prev_sequence, 100);
        assert_eq!(delta.sequence, 101);
    }

    #[test]
    fn test_market_event_creation() {
        let market_event = MarketEvent {
            event_type: MarketEventType::TradingHalt,
            symbol: Some(Symbol::new(3)),
            timestamp: Ts::now(),
            message: "Trading halted due to volatility".to_string(),
        };
        
        assert_eq!(market_event.event_type, MarketEventType::TradingHalt);
        assert_eq!(market_event.symbol, Some(Symbol::new(3)));
        assert_eq!(market_event.message, "Trading halted due to volatility");
    }
}

#[cfg(test)]
mod orderbook_event_tests {
    use super::*;

    #[test]
    fn test_orderbook_event_order_variant() {
        let order_update = create_test_order_update(123, 100_000, 10_000, Side::Buy, UpdateType::Add);
        let event = OrderBookEvent::Order(order_update);
        
        assert_eq!(event.sequence(), 1);
        assert!(event.exchange_time().as_nanos() > 0);
        assert!(event.local_time().is_some());
        assert!(!event.is_snapshot());
        assert!(!event.is_trade());
    }

    #[test]
    fn test_orderbook_event_trade_variant() {
        let trade = create_test_trade(456, 100_500, 5_000, Side::Sell);
        let event = OrderBookEvent::Trade(trade);
        
        assert_eq!(event.sequence(), 1);
        assert!(event.exchange_time().as_nanos() > 0);
        assert!(event.local_time().is_some());
        assert!(!event.is_snapshot());
        assert!(event.is_trade());
    }

    #[test]
    fn test_orderbook_event_snapshot_variant() {
        let snapshot = OrderBookSnapshot {
            symbol: Symbol::new(1),
            bids: vec![],
            asks: vec![],
            sequence: 789,
            exchange_time: Ts::now(),
            local_time: Ts::now(),
            checksum: 0x12345678,
        };
        
        let event = OrderBookEvent::Snapshot(snapshot);
        
        assert_eq!(event.sequence(), 789);
        assert!(event.exchange_time().as_nanos() > 0);
        assert!(event.local_time().is_some());
        assert!(event.is_snapshot());
        assert!(!event.is_trade());
    }

    #[test]
    fn test_orderbook_event_delta_variant() {
        let delta = OrderBookDelta {
            symbol: Symbol::new(2),
            bid_updates: vec![],
            ask_updates: vec![],
            bid_deletions: vec![],
            ask_deletions: vec![],
            prev_sequence: 99,
            sequence: 100,
            exchange_time: Ts::now(),
            local_time: Ts::now(),
        };
        
        let event = OrderBookEvent::Delta(delta);
        
        assert_eq!(event.sequence(), 100);
        assert!(event.exchange_time().as_nanos() > 0);
        assert!(event.local_time().is_some());
        assert!(!event.is_snapshot());
        assert!(!event.is_trade());
    }

    #[test]
    fn test_orderbook_event_market_variant() {
        let market_event = MarketEvent {
            event_type: MarketEventType::CircuitBreaker,
            symbol: None,
            timestamp: Ts::now(),
            message: "Circuit breaker triggered".to_string(),
        };
        
        let event = OrderBookEvent::Market(market_event);
        
        assert_eq!(event.sequence(), 0); // Market events don't have sequence
        assert!(event.exchange_time().as_nanos() > 0);
        assert!(event.local_time().is_none()); // Market events don't have local time
        assert!(!event.is_snapshot());
        assert!(!event.is_trade());
    }
}

#[cfg(test)]
mod event_builder_tests {
    use super::*;

    #[test]
    fn test_event_builder_creation() {
        let builder = EventBuilder::new();
        assert_eq!(builder.current_sequence(), 0);
    }

    #[test]
    fn test_event_builder_order_add() {
        let mut builder = EventBuilder::new();
        
        let order_add = builder.order_add(123, Px::from_i64(100_000), Qty::from_i64(10_000), Side::Buy);
        
        assert_eq!(order_add.order_id, 123);
        assert_eq!(order_add.price, Px::from_i64(100_000));
        assert_eq!(order_add.quantity, Qty::from_i64(10_000));
        assert_eq!(order_add.side, Side::Buy);
        assert_eq!(order_add.update_type, UpdateType::Add);
        assert_eq!(order_add.sequence, 1);
        assert_eq!(builder.current_sequence(), 1);
    }

    #[test]
    fn test_event_builder_order_modify() {
        let mut builder = EventBuilder::new();
        
        let order_modify = builder.order_modify(456, Px::from_i64(99_500), Qty::from_i64(5_000), Side::Sell);
        
        assert_eq!(order_modify.order_id, 456);
        assert_eq!(order_modify.price, Px::from_i64(99_500));
        assert_eq!(order_modify.quantity, Qty::from_i64(5_000));
        assert_eq!(order_modify.side, Side::Sell);
        assert_eq!(order_modify.update_type, UpdateType::Modify);
        assert_eq!(order_modify.sequence, 1);
    }

    #[test]
    fn test_event_builder_order_delete() {
        let mut builder = EventBuilder::new();
        
        let order_delete = builder.order_delete(789, Px::from_i64(101_000), Side::Buy);
        
        assert_eq!(order_delete.order_id, 789);
        assert_eq!(order_delete.price, Px::from_i64(101_000));
        assert_eq!(order_delete.quantity, Qty::ZERO);
        assert_eq!(order_delete.side, Side::Buy);
        assert_eq!(order_delete.update_type, UpdateType::Delete);
        assert_eq!(order_delete.sequence, 1);
    }

    #[test]
    fn test_event_builder_trade() {
        let mut builder = EventBuilder::new();
        
        let trade = builder.trade(999, Px::from_i64(100_250), Qty::from_i64(2_500), Side::Sell);
        
        assert_eq!(trade.trade_id, 999);
        assert_eq!(trade.price, Px::from_i64(100_250));
        assert_eq!(trade.quantity, Qty::from_i64(2_500));
        assert_eq!(trade.aggressor_side, Side::Sell);
        assert_eq!(trade.sequence, 1);
        assert!(trade.maker_order_id.is_none());
        assert!(trade.taker_order_id.is_none());
    }

    #[test]
    fn test_event_builder_sequence_increment() {
        let mut builder = EventBuilder::new();
        
        builder.order_add(1, Px::from_i64(100_000), Qty::from_i64(1000), Side::Buy);
        assert_eq!(builder.current_sequence(), 1);
        
        builder.order_modify(2, Px::from_i64(100_000), Qty::from_i64(500), Side::Buy);
        assert_eq!(builder.current_sequence(), 2);
        
        builder.order_delete(3, Px::from_i64(100_000), Side::Buy);
        assert_eq!(builder.current_sequence(), 3);
        
        builder.trade(4, Px::from_i64(100_000), Qty::from_i64(250), Side::Sell);
        assert_eq!(builder.current_sequence(), 4);
    }

    #[test]
    fn test_event_builder_reset_sequence() {
        let mut builder = EventBuilder::new();
        
        builder.order_add(1, Px::from_i64(100_000), Qty::from_i64(1000), Side::Buy);
        assert_eq!(builder.current_sequence(), 1);
        
        builder.reset_sequence();
        assert_eq!(builder.current_sequence(), 0);
        
        builder.order_add(2, Px::from_i64(100_000), Qty::from_i64(1000), Side::Buy);
        assert_eq!(builder.current_sequence(), 1);
    }
}

#[cfg(test)]
mod serialization_tests {
    use super::*;

    #[test]
    fn test_order_update_json_serialization() {
        let update = create_test_order_update(123, 100_000, 10_000, Side::Buy, UpdateType::Add);
        
        let json = serde_json::to_string(&update).expect("Should serialize to JSON");
        let deserialized: OrderUpdate = serde_json::from_str(&json).expect("Should deserialize from JSON");
        
        assert_eq!(update.order_id, deserialized.order_id);
        assert_eq!(update.price, deserialized.price);
        assert_eq!(update.quantity, deserialized.quantity);
        assert_eq!(update.side, deserialized.side);
        assert_eq!(update.update_type, deserialized.update_type);
    }

    #[test]
    fn test_trade_event_json_serialization() {
        let trade = create_test_trade(456, 100_500, 5_000, Side::Sell);
        
        let json = serde_json::to_string(&trade).expect("Should serialize to JSON");
        let deserialized: TradeEvent = serde_json::from_str(&json).expect("Should deserialize from JSON");
        
        assert_eq!(trade.trade_id, deserialized.trade_id);
        assert_eq!(trade.price, deserialized.price);
        assert_eq!(trade.quantity, deserialized.quantity);
        assert_eq!(trade.aggressor_side, deserialized.aggressor_side);
    }

    #[test]
    fn test_orderbook_event_json_serialization() {
        let update = create_test_order_update(789, 99_000, 7_500, Side::Bid, UpdateType::Modify);
        let event = OrderBookEvent::Order(update);
        
        let json = serde_json::to_string(&event).expect("Should serialize to JSON");
        let deserialized: OrderBookEvent = serde_json::from_str(&json).expect("Should deserialize from JSON");
        
        if let OrderBookEvent::Order(deserialized_update) = deserialized {
            assert_eq!(deserialized_update.order_id, 789);
            assert_eq!(deserialized_update.price, Px::from_i64(99_000));
            assert_eq!(deserialized_update.quantity, Qty::from_i64(7_500));
            assert_eq!(deserialized_update.side, Side::Bid);
            assert_eq!(deserialized_update.update_type, UpdateType::Modify);
        } else {
            panic!("Deserialized event should be Order variant");
        }
    }

    #[test]
    fn test_order_update_bincode_serialization() {
        let update = create_test_order_update(321, 98_000, 12_500, Side::Ask, UpdateType::Delete);
        
        let encoded = bincode::serialize(&update).expect("Should serialize with bincode");
        let decoded: OrderUpdate = bincode::deserialize(&encoded).expect("Should deserialize with bincode");
        
        assert_eq!(update.order_id, decoded.order_id);
        assert_eq!(update.price, decoded.price);
        assert_eq!(update.quantity, decoded.quantity);
        assert_eq!(update.side, decoded.side);
        assert_eq!(update.update_type, decoded.update_type);
    }

    #[test]
    fn test_snapshot_serialization() {
        let snapshot = OrderBookSnapshot {
            symbol: Symbol::new(42),
            bids: vec![
                LevelUpdate { price: Px::from_i64(100_000), quantity: Qty::from_i64(10_000), order_count: 2, side: Side::Bid },
            ],
            asks: vec![
                LevelUpdate { price: Px::from_i64(101_000), quantity: Qty::from_i64(8_000), order_count: 1, side: Side::Sell },
            ],
            sequence: 12345,
            exchange_time: Ts::from_nanos(1_000_000_000),
            local_time: Ts::from_nanos(1_000_000_100),
            checksum: 0xABCDEF,
        };
        
        let json = serde_json::to_string(&snapshot).expect("Should serialize snapshot to JSON");
        let deserialized: OrderBookSnapshot = serde_json::from_str(&json).expect("Should deserialize snapshot from JSON");
        
        assert_eq!(snapshot.symbol, deserialized.symbol);
        assert_eq!(snapshot.bids.len(), deserialized.bids.len());
        assert_eq!(snapshot.asks.len(), deserialized.asks.len());
        assert_eq!(snapshot.sequence, deserialized.sequence);
        assert_eq!(snapshot.checksum, deserialized.checksum);
    }

    #[test]
    fn test_market_event_serialization() {
        let market_event = MarketEvent {
            event_type: MarketEventType::AuctionStart,
            symbol: Some(Symbol::new(99)),
            timestamp: Ts::from_nanos(2_000_000_000),
            message: "Opening auction started".to_string(),
        };
        
        let json = serde_json::to_string(&market_event).expect("Should serialize market event to JSON");
        let deserialized: MarketEvent = serde_json::from_str(&json).expect("Should deserialize market event from JSON");
        
        assert_eq!(market_event.event_type, deserialized.event_type);
        assert_eq!(market_event.symbol, deserialized.symbol);
        assert_eq!(market_event.timestamp, deserialized.timestamp);
        assert_eq!(market_event.message, deserialized.message);
    }
}

#[cfg(test)]
mod batch_stats_tests {
    use super::*;

    #[test]
    fn test_event_batch_stats_creation() {
        let stats = EventBatchStats::default();
        
        assert_eq!(stats.adds, 0);
        assert_eq!(stats.modifies, 0);
        assert_eq!(stats.deletes, 0);
        assert_eq!(stats.trades, 0);
        assert_eq!(stats.volume, 0);
        assert_eq!(stats.min_latency_ns, 0);
        assert_eq!(stats.max_latency_ns, 0);
        assert_eq!(stats.avg_latency_ns, 0);
    }

    #[test]
    fn test_event_batch_stats_manual_calculation() {
        // Simulate processing a batch of events and calculating stats
        let mut stats = EventBatchStats {
            adds: 10,
            modifies: 5,
            deletes: 3,
            trades: 7,
            volume: 50_000,
            min_latency_ns: 1_000,
            max_latency_ns: 10_000,
            avg_latency_ns: 0,
        };
        
        // Calculate average latency
        let total_events = stats.adds + stats.modifies + stats.deletes + stats.trades;
        if total_events > 0 {
            // Simulate total latency calculation
            let total_latency = 125_000; // Example total latency
            stats.avg_latency_ns = total_latency / total_events;
        }
        
        assert_eq!(stats.avg_latency_ns, 5_000); // 125,000 / 25 = 5,000
        assert_eq!(total_events, 25);
    }
}

#[cfg(test)]
mod update_type_tests {
    use super::*;

    #[test]
    fn test_update_type_values() {
        // Test that enum values are as expected (important for serialization)
        assert_eq!(UpdateType::Add as u8, 0);
        assert_eq!(UpdateType::Modify as u8, 1);
        assert_eq!(UpdateType::Delete as u8, 2);
        assert_eq!(UpdateType::Trade as u8, 3);
        assert_eq!(UpdateType::Snapshot as u8, 4);
        assert_eq!(UpdateType::Delta as u8, 5);
        assert_eq!(UpdateType::Clear as u8, 6);
    }

    #[test]
    fn test_market_event_type_values() {
        // Test that enum values are as expected
        assert_eq!(MarketEventType::TradingHalt as u8, 0);
        assert_eq!(MarketEventType::TradingResume as u8, 1);
        assert_eq!(MarketEventType::MarketOpen as u8, 2);
        assert_eq!(MarketEventType::MarketClose as u8, 3);
        assert_eq!(MarketEventType::CircuitBreaker as u8, 4);
        assert_eq!(MarketEventType::AuctionStart as u8, 5);
        assert_eq!(MarketEventType::AuctionEnd as u8, 6);
    }

    #[test]
    fn test_side_values() {
        // Test that enum values are as expected
        assert_eq!(Side::Buy as u8, 0);
        assert_eq!(Side::Sell as u8, 1);
    }
}

#[cfg(test)]
mod timestamp_and_latency_tests {
    use super::*;

    #[test]
    fn test_event_timestamp_consistency() {
        let exchange_time = Ts::from_nanos(1_000_000_000);
        let local_time = Ts::from_nanos(1_000_001_000); // 1ms later
        
        let update = OrderUpdate {
            order_id: 123,
            price: Px::from_i64(100_000),
            quantity: Qty::from_i64(10_000),
            side: Side::Buy,
            update_type: UpdateType::Add,
            exchange_time,
            local_time,
            sequence: 1,
        };
        
        assert_eq!(update.exchange_time, exchange_time);
        assert_eq!(update.local_time, local_time);
        
        // Calculate latency
        let latency_ns = local_time.as_nanos() - exchange_time.as_nanos();
        assert_eq!(latency_ns, 1_000_000); // 1ms in nanoseconds
    }

    #[test]
    fn test_event_ordering_by_sequence() {
        let mut events = vec![
            OrderBookEvent::Order(create_test_order_update(1, 100_000, 1000, Side::Buy, UpdateType::Add)),
            OrderBookEvent::Order(create_test_order_update(2, 100_000, 1000, Side::Buy, UpdateType::Add)),
            OrderBookEvent::Order(create_test_order_update(3, 100_000, 1000, Side::Buy, UpdateType::Add)),
        ];
        
        // Manually set different sequences (simulating out-of-order receipt)
        if let OrderBookEvent::Order(ref mut order1) = events[0] { order1.sequence = 3; }
        if let OrderBookEvent::Order(ref mut order2) = events[1] { order2.sequence = 1; }
        if let OrderBookEvent::Order(ref mut order3) = events[2] { order3.sequence = 2; }
        
        // Sort by sequence number
        events.sort_by_key(|e| e.sequence());
        
        assert_eq!(events[0].sequence(), 1);
        assert_eq!(events[1].sequence(), 2);
        assert_eq!(events[2].sequence(), 3);
    }

    #[test]
    fn test_event_ordering_by_timestamp() {
        let base_time = 1_000_000_000;
        let mut events = vec![
            OrderBookEvent::Order(OrderUpdate {
                order_id: 1,
                price: Px::from_i64(100_000),
                quantity: Qty::from_i64(1000),
                side: Side::Buy,
                update_type: UpdateType::Add,
                exchange_time: Ts::from_nanos(base_time + 2_000_000),
                local_time: Ts::from_nanos(base_time + 2_001_000),
                sequence: 1,
            }),
            OrderBookEvent::Order(OrderUpdate {
                order_id: 2,
                price: Px::from_i64(100_000),
                quantity: Qty::from_i64(1000),
                side: Side::Buy,
                update_type: UpdateType::Add,
                exchange_time: Ts::from_nanos(base_time + 1_000_000),
                local_time: Ts::from_nanos(base_time + 1_001_000),
                sequence: 2,
            }),
            OrderBookEvent::Order(OrderUpdate {
                order_id: 3,
                price: Px::from_i64(100_000),
                quantity: Qty::from_i64(1000),
                side: Side::Buy,
                update_type: UpdateType::Add,
                exchange_time: Ts::from_nanos(base_time + 3_000_000),
                local_time: Ts::from_nanos(base_time + 3_001_000),
                sequence: 3,
            }),
        ];
        
        // Sort by exchange timestamp
        events.sort_by_key(|e| e.exchange_time());
        
        // Should be ordered by exchange time (earliest first)
        assert!(events[0].exchange_time() < events[1].exchange_time());
        assert!(events[1].exchange_time() < events[2].exchange_time());
    }
}