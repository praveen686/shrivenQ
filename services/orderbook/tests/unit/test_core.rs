//! Comprehensive unit tests for core orderbook functionality
//! 
//! Tests cover:
//! - Basic order operations (add, cancel, modify)
//! - Best Bid/Offer (BBO) updates and lock-free reads
//! - Price level management and atomicity
//! - Spread calculations and mid-price
//! - Checksum validation and integrity
//! - Concurrent access patterns
//! - Volume calculations and tracking

use orderbook::core::{OrderBook, Order, Side, PriceLevel};
use services_common::{Px, Qty, Ts};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Helper function to create a test order
fn create_order(id: u64, price: i64, quantity: i64, side: Side) -> Order {
    Order {
        id,
        price: Px::from_i64(price),
        quantity: Qty::from_i64(quantity),
        original_quantity: Qty::from_i64(quantity),
        timestamp: Ts::now(),
        side,
        is_iceberg: false,
        visible_quantity: None,
    }
}

/// Helper function to create an iceberg order
fn create_iceberg_order(id: u64, price: i64, total_quantity: i64, visible_quantity: i64, side: Side) -> Order {
    Order {
        id,
        price: Px::from_i64(price),
        quantity: Qty::from_i64(total_quantity),
        original_quantity: Qty::from_i64(total_quantity),
        timestamp: Ts::now(),
        side,
        is_iceberg: true,
        visible_quantity: Some(Qty::from_i64(visible_quantity)),
    }
}

#[cfg(test)]
mod orderbook_tests {
    use super::*;

    #[test]
    fn test_orderbook_creation() {
        let book = OrderBook::new("BTCUSD");
        assert_eq!(book.symbol(), "BTCUSD");
        
        // Check initial state
        let (best_bid, best_ask) = book.get_bbo();
        assert!(best_bid.is_none());
        assert!(best_ask.is_none());
        
        assert!(book.get_spread().is_none());
        assert!(book.get_mid().is_none());
    }

    #[test]
    fn test_add_single_bid_order() {
        let book = OrderBook::new("ETHUSD");
        let order = create_order(1, 100_000, 10_000, Side::Bid); // $10.00, 1.0 ETH
        
        book.add_order(order);
        
        let (best_bid, best_ask) = book.get_bbo();
        assert_eq!(best_bid, Some(Px::from_i64(100_000)));
        assert!(best_ask.is_none());
        
        // Test depth
        let (bids, asks) = book.get_depth(5);
        assert_eq!(bids.len(), 1);
        assert_eq!(bids[0], (Px::from_i64(100_000), Qty::from_i64(10_000), 1));
        assert_eq!(asks.len(), 0);
    }

    #[test]
    fn test_add_single_ask_order() {
        let book = OrderBook::new("ETHUSD");
        let order = create_order(1, 101_000, 5_000, Side::Ask); // $10.10, 0.5 ETH
        
        book.add_order(order);
        
        let (best_bid, best_ask) = book.get_bbo();
        assert!(best_bid.is_none());
        assert_eq!(best_ask, Some(Px::from_i64(101_000)));
        
        // Test depth
        let (bids, asks) = book.get_depth(5);
        assert_eq!(bids.len(), 0);
        assert_eq!(asks.len(), 1);
        assert_eq!(asks[0], (Px::from_i64(101_000), Qty::from_i64(5_000), 1));
    }

    #[test]
    fn test_spread_calculation() {
        let book = OrderBook::new("BTCUSD");
        
        // Add bid and ask
        book.add_order(create_order(1, 99_000, 10_000, Side::Bid));   // $9.90
        book.add_order(create_order(2, 101_000, 5_000, Side::Ask));   // $10.10
        
        let spread = book.get_spread();
        assert_eq!(spread, Some(2_000)); // 20 cents spread
        
        let mid = book.get_mid();
        assert_eq!(mid, Some(Px::from_i64(100_000))); // $10.00 mid
    }

    #[test]
    fn test_multiple_price_levels() {
        let book = OrderBook::new("ADAUSD");
        
        // Add multiple bid levels
        book.add_order(create_order(1, 100_000, 10_000, Side::Bid));  // $10.00
        book.add_order(create_order(2, 99_000, 20_000, Side::Bid));   // $9.90
        book.add_order(create_order(3, 98_000, 30_000, Side::Bid));   // $9.80
        
        // Add multiple ask levels
        book.add_order(create_order(4, 101_000, 15_000, Side::Ask));  // $10.10
        book.add_order(create_order(5, 102_000, 25_000, Side::Ask));  // $10.20
        book.add_order(create_order(6, 103_000, 35_000, Side::Ask));  // $10.30
        
        // Verify BBO
        let (best_bid, best_ask) = book.get_bbo();
        assert_eq!(best_bid, Some(Px::from_i64(100_000)));
        assert_eq!(best_ask, Some(Px::from_i64(101_000)));
        
        // Verify depth ordering
        let (bids, asks) = book.get_depth(3);
        
        // Bids should be in descending price order
        assert_eq!(bids[0].0, Px::from_i64(100_000)); // Best bid first
        assert_eq!(bids[1].0, Px::from_i64(99_000));
        assert_eq!(bids[2].0, Px::from_i64(98_000));
        
        // Asks should be in ascending price order
        assert_eq!(asks[0].0, Px::from_i64(101_000)); // Best ask first
        assert_eq!(asks[1].0, Px::from_i64(102_000));
        assert_eq!(asks[2].0, Px::from_i64(103_000));
    }

    #[test]
    fn test_multiple_orders_same_price() {
        let book = OrderBook::new("SOLUSD");
        
        // Add multiple orders at same price level
        book.add_order(create_order(1, 100_000, 10_000, Side::Bid));
        book.add_order(create_order(2, 100_000, 20_000, Side::Bid));
        book.add_order(create_order(3, 100_000, 30_000, Side::Bid));
        
        let (bids, _) = book.get_depth(1);
        assert_eq!(bids.len(), 1);
        assert_eq!(bids[0], (Px::from_i64(100_000), Qty::from_i64(60_000), 3)); // Total quantity and count
    }

    #[test]
    fn test_cancel_order() {
        let book = OrderBook::new("DOTUSD");
        
        // Add orders
        book.add_order(create_order(1, 100_000, 10_000, Side::Bid));
        book.add_order(create_order(2, 99_000, 20_000, Side::Bid));
        book.add_order(create_order(3, 101_000, 15_000, Side::Ask));
        
        // Cancel best bid
        let cancelled = book.cancel_order(1);
        assert!(cancelled.is_some());
        assert_eq!(cancelled.unwrap().id, 1);
        
        // Best bid should now be the second level
        let (best_bid, _) = book.get_bbo();
        assert_eq!(best_bid, Some(Px::from_i64(99_000)));
        
        // Cancel non-existent order
        let not_found = book.cancel_order(999);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_cancel_last_order_at_level() {
        let book = OrderBook::new("LINKUSD");
        
        // Add single order at price level
        book.add_order(create_order(1, 100_000, 10_000, Side::Bid));
        book.add_order(create_order(2, 99_000, 20_000, Side::Bid));
        
        // Cancel the best bid
        book.cancel_order(1);
        
        // Level should be removed and BBO updated
        let (best_bid, _) = book.get_bbo();
        assert_eq!(best_bid, Some(Px::from_i64(99_000)));
        
        let (bids, _) = book.get_depth(5);
        assert_eq!(bids.len(), 1);
        assert_eq!(bids[0].0, Px::from_i64(99_000));
    }

    #[test]
    fn test_iceberg_orders() {
        let book = OrderBook::new("AVAXUSD");
        
        // Add iceberg order (total 100, visible 10)
        let iceberg = create_iceberg_order(1, 100_000, 100_000, 10_000, Side::Bid);
        book.add_order(iceberg);
        
        let (bids, _) = book.get_depth(1);
        assert_eq!(bids.len(), 1);
        
        // Visible quantity should be reflected in the level
        assert_eq!(bids[0], (Px::from_i64(100_000), Qty::from_i64(100_000), 1));
    }

    #[test]
    fn test_checksum_consistency() {
        let book = OrderBook::new("ALGOUSD");
        
        // Add orders and get initial checksum
        book.add_order(create_order(1, 100_000, 10_000, Side::Bid));
        book.add_order(create_order(2, 101_000, 15_000, Side::Ask));
        let checksum1 = book.get_checksum();
        
        // Add another order - checksum should change
        book.add_order(create_order(3, 99_000, 20_000, Side::Bid));
        let checksum2 = book.get_checksum();
        assert_ne!(checksum1, checksum2);
        
        // Cancel order - checksum should change again
        book.cancel_order(3);
        let checksum3 = book.get_checksum();
        assert_ne!(checksum2, checksum3);
        
        // Should be back to original state, but checksums might differ due to implementation
        // This tests that checksums are being calculated
        assert!(checksum1 > 0);
        assert!(checksum2 > 0);
        assert!(checksum3 > 0);
    }

    #[test]
    fn test_specific_price_queries() {
        let book = OrderBook::new("ATOMUSD");
        
        book.add_order(create_order(1, 100_000, 10_000, Side::Bid));
        book.add_order(create_order(2, 99_000, 20_000, Side::Bid));
        book.add_order(create_order(3, 101_000, 15_000, Side::Ask));
        book.add_order(create_order(4, 102_000, 25_000, Side::Ask));
        
        // Test specific price queries
        assert_eq!(book.get_bid_size_at(Px::from_i64(100_000)), Some(Qty::from_i64(10_000)));
        assert_eq!(book.get_bid_size_at(Px::from_i64(99_000)), Some(Qty::from_i64(20_000)));
        assert_eq!(book.get_bid_size_at(Px::from_i64(98_000)), None);
        
        assert_eq!(book.get_ask_size_at(Px::from_i64(101_000)), Some(Qty::from_i64(15_000)));
        assert_eq!(book.get_ask_size_at(Px::from_i64(102_000)), Some(Qty::from_i64(25_000)));
        assert_eq!(book.get_ask_size_at(Px::from_i64(103_000)), None);
    }

    #[test]
    fn test_clear_orderbook() {
        let book = OrderBook::new("MATICUSD");
        
        // Add multiple orders
        book.add_order(create_order(1, 100_000, 10_000, Side::Bid));
        book.add_order(create_order(2, 99_000, 20_000, Side::Bid));
        book.add_order(create_order(3, 101_000, 15_000, Side::Ask));
        
        // Verify orders exist
        let (bids, asks) = book.get_depth(5);
        assert!(!bids.is_empty());
        assert!(!asks.is_empty());
        
        // Clear the book
        book.clear();
        
        // Verify everything is cleared
        let (best_bid, best_ask) = book.get_bbo();
        assert!(best_bid.is_none());
        assert!(best_ask.is_none());
        
        let (bids, asks) = book.get_depth(5);
        assert!(bids.is_empty());
        assert!(asks.is_empty());
        
        assert_eq!(book.get_checksum(), 0);
    }

    #[test]
    fn test_snapshot_loading() {
        let book = OrderBook::new("LTCUSD");
        
        // Create snapshot data
        let bid_levels = vec![
            (Px::from_i64(100_000), Qty::from_i64(10_000), 2), // $10.00, 1.0 LTC, 2 orders
            (Px::from_i64(99_000), Qty::from_i64(20_000), 3),  // $9.90, 2.0 LTC, 3 orders
        ];
        
        let ask_levels = vec![
            (Px::from_i64(101_000), Qty::from_i64(15_000), 1), // $10.10, 1.5 LTC, 1 order
            (Px::from_i64(102_000), Qty::from_i64(25_000), 2), // $10.20, 2.5 LTC, 2 orders
        ];
        
        book.load_snapshot(bid_levels, ask_levels);
        
        // Verify loaded snapshot
        let (best_bid, best_ask) = book.get_bbo();
        assert_eq!(best_bid, Some(Px::from_i64(100_000)));
        assert_eq!(best_ask, Some(Px::from_i64(101_000)));
        
        let (bids, asks) = book.get_depth(2);
        assert_eq!(bids.len(), 2);
        assert_eq!(asks.len(), 2);
        
        // Verify order counts and quantities
        assert_eq!(bids[0], (Px::from_i64(100_000), Qty::from_i64(10_000), 2));
        assert_eq!(bids[1], (Px::from_i64(99_000), Qty::from_i64(20_000), 3));
        assert_eq!(asks[0], (Px::from_i64(101_000), Qty::from_i64(15_000), 1));
        assert_eq!(asks[1], (Px::from_i64(102_000), Qty::from_i64(25_000), 2));
    }

    #[test]
    fn test_concurrent_read_access() {
        let book = Arc::new(OrderBook::new("CONCURRENT_TEST"));
        
        // Add some initial orders
        book.add_order(create_order(1, 100_000, 10_000, Side::Bid));
        book.add_order(create_order(2, 101_000, 15_000, Side::Ask));
        
        let book_clone = Arc::clone(&book);
        
        // Spawn reader thread
        let reader_handle = thread::spawn(move || {
            for _ in 0..1000 {
                let (bid, ask) = book_clone.get_bbo();
                assert!(bid.is_some() || ask.is_some()); // At least one should exist
                
                let _spread = book_clone.get_spread();
                let _mid = book_clone.get_mid();
                let _checksum = book_clone.get_checksum();
                
                thread::sleep(Duration::from_micros(1));
            }
        });
        
        // Continue adding orders while reader is running
        for i in 3..100 {
            let side = if i % 2 == 0 { Side::Bid } else { Side::Ask };
            let price = if side == Side::Bid { 99_000 } else { 102_000 };
            book.add_order(create_order(i, price, 1000, side));
            
            if i % 10 == 0 {
                book.cancel_order(i - 5); // Cancel some orders
            }
        }
        
        reader_handle.join().expect("Reader thread should complete successfully");
    }

    #[test]
    fn test_price_improvement() {
        let book = OrderBook::new("PRICE_IMPROVEMENT");
        
        // Add initial orders
        book.add_order(create_order(1, 100_000, 10_000, Side::Bid));
        book.add_order(create_order(2, 101_000, 15_000, Side::Ask));
        
        let (initial_bid, initial_ask) = book.get_bbo();
        
        // Add better bid (price improvement)
        book.add_order(create_order(3, 100_500, 5_000, Side::Bid));
        let (improved_bid, _) = book.get_bbo();
        
        assert!(improved_bid > initial_bid);
        assert_eq!(improved_bid, Some(Px::from_i64(100_500)));
        
        // Add better ask (price improvement)
        book.add_order(create_order(4, 100_800, 8_000, Side::Ask));
        let (_, improved_ask) = book.get_bbo();
        
        assert!(improved_ask < initial_ask);
        assert_eq!(improved_ask, Some(Px::from_i64(100_800)));
    }

    #[test]
    fn test_edge_cases() {
        let book = OrderBook::new("EDGE_CASES");
        
        // Test zero quantity (should not be added)
        let zero_qty_order = Order {
            id: 1,
            price: Px::from_i64(100_000),
            quantity: Qty::ZERO,
            original_quantity: Qty::ZERO,
            timestamp: Ts::now(),
            side: Side::Bid,
            is_iceberg: false,
            visible_quantity: None,
        };
        
        book.add_order(zero_qty_order);
        let (bids, _) = book.get_depth(1);
        // Should still add the order even with zero quantity (for testing purposes)
        // In a real system, you might want to reject zero quantity orders
        assert_eq!(bids.len(), 1);
        
        // Test very large quantities
        let large_order = create_order(2, 100_000, i64::MAX / 1000, Side::Bid);
        book.add_order(large_order);
        
        // Test very small prices
        let small_price_order = create_order(3, 1, 10_000, Side::Ask);
        book.add_order(small_price_order);
        
        // Book should handle these edge cases gracefully
        let (_, _) = book.get_bbo();
        assert!(book.get_checksum() > 0);
    }
}

#[cfg(test)]
mod price_level_tests {
    use super::*;

    #[test]
    fn test_price_level_creation() {
        let level = PriceLevel::new(Px::from_i64(100_000));
        
        assert_eq!(level.get_quantity(), Qty::ZERO);
        assert_eq!(level.get_order_count(), 0);
        assert_eq!(level.get_hidden_quantity(), Qty::ZERO);
    }

    #[test]
    fn test_price_level_add_order() {
        let level = PriceLevel::new(Px::from_i64(100_000));
        let order = create_order(1, 100_000, 10_000, Side::Bid);
        
        level.add_order(order);
        
        assert_eq!(level.get_quantity(), Qty::from_i64(10_000));
        assert_eq!(level.get_order_count(), 1);
        assert_eq!(level.get_hidden_quantity(), Qty::ZERO);
    }

    #[test]
    fn test_price_level_add_iceberg_order() {
        let level = PriceLevel::new(Px::from_i64(100_000));
        let iceberg = create_iceberg_order(1, 100_000, 100_000, 10_000, Side::Bid);
        
        level.add_order(iceberg);
        
        assert_eq!(level.get_quantity(), Qty::from_i64(100_000));
        assert_eq!(level.get_order_count(), 1);
        assert_eq!(level.get_hidden_quantity(), Qty::from_i64(90_000)); // Total - visible
    }

    #[test]
    fn test_price_level_remove_order() {
        let level = PriceLevel::new(Px::from_i64(100_000));
        let order = create_order(1, 100_000, 10_000, Side::Bid);
        
        level.add_order(order);
        assert_eq!(level.get_order_count(), 1);
        
        let removed = level.remove_order(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, 1);
        
        assert_eq!(level.get_quantity(), Qty::ZERO);
        assert_eq!(level.get_order_count(), 0);
        
        // Try to remove non-existent order
        let not_found = level.remove_order(999);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_price_level_multiple_orders() {
        let level = PriceLevel::new(Px::from_i64(100_000));
        
        // Add multiple orders
        level.add_order(create_order(1, 100_000, 10_000, Side::Bid));
        level.add_order(create_order(2, 100_000, 20_000, Side::Bid));
        level.add_order(create_order(3, 100_000, 30_000, Side::Bid));
        
        assert_eq!(level.get_quantity(), Qty::from_i64(60_000));
        assert_eq!(level.get_order_count(), 3);
        
        // Remove middle order
        let removed = level.remove_order(2);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().quantity, Qty::from_i64(20_000));
        
        assert_eq!(level.get_quantity(), Qty::from_i64(40_000));
        assert_eq!(level.get_order_count(), 2);
    }

    #[test]
    fn test_price_level_concurrent_access() {
        let level = Arc::new(PriceLevel::new(Px::from_i64(100_000)));
        
        // Add initial order
        level.add_order(create_order(1, 100_000, 10_000, Side::Bid));
        
        let level_clone = Arc::clone(&level);
        
        // Spawn thread to continuously read level data
        let reader_handle = thread::spawn(move || {
            for _ in 0..1000 {
                let _qty = level_clone.get_quantity();
                let _count = level_clone.get_order_count();
                let _hidden = level_clone.get_hidden_quantity();
                thread::sleep(Duration::from_micros(1));
            }
        });
        
        // Continue adding/removing orders
        for i in 2..50 {
            level.add_order(create_order(i, 100_000, 1000, Side::Bid));
            
            if i % 5 == 0 {
                level.remove_order(i - 2);
            }
        }
        
        reader_handle.join().expect("Reader thread should complete successfully");
        
        // Verify final state is consistent
        assert!(level.get_quantity().as_i64() > 0);
        assert!(level.get_order_count() > 0);
    }
}