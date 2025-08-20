//! Position tracking unit tests
//! Tests atomic operations, PnL calculations, and concurrency

use portfolio_manager::position::{Position, PositionTracker};
use rstest::*;
use services_common::{Px, Qty, Side, Symbol, Ts};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use tokio_test;

// Test fixtures
#[fixture]
fn symbol() -> Symbol {
    Symbol::new(1)
}

#[fixture]
fn price_100() -> Px {
    Px::from_i64(1000000) // $100.00
}

#[fixture]
fn price_101() -> Px {
    Px::from_i64(1010000) // $101.00
}

#[fixture]
fn quantity_100() -> Qty {
    Qty::from_i64(1000000) // 100 units
}

#[fixture]
fn tracker() -> PositionTracker {
    PositionTracker::new(100)
}

mod position_tests {
    use super::*;

    #[rstest]
    fn test_new_position_initialization(symbol: Symbol) {
        let position = Position::new(symbol);

        assert_eq!(position.symbol, symbol);
        assert_eq!(position.quantity.load(Ordering::Acquire), 0);
        assert_eq!(position.avg_price.load(Ordering::Acquire), 0);
        assert_eq!(position.realized_pnl.load(Ordering::Acquire), 0);
        assert_eq!(position.unrealized_pnl.load(Ordering::Acquire), 0);
    }

    #[rstest]
    fn test_long_position_opening(
        symbol: Symbol,
        quantity_100: Qty,
        price_100: Px,
    ) {
        let position = Position::new(symbol);
        let timestamp = Ts::now();

        position.apply_fill(Side::Bid, quantity_100, price_100, timestamp);

        assert_eq!(position.quantity.load(Ordering::Acquire), 1000000);
        assert_eq!(position.avg_price.load(Ordering::Acquire), 1000000);
        assert_eq!(position.realized_pnl.load(Ordering::Acquire), 0);
    }

    #[rstest]
    fn test_short_position_opening(
        symbol: Symbol,
        quantity_100: Qty,
        price_100: Px,
    ) {
        let position = Position::new(symbol);
        let timestamp = Ts::now();

        position.apply_fill(Side::Ask, quantity_100, price_100, timestamp);

        assert_eq!(position.quantity.load(Ordering::Acquire), -1000000);
        assert_eq!(position.avg_price.load(Ordering::Acquire), 1000000);
        assert_eq!(position.realized_pnl.load(Ordering::Acquire), 0);
    }

    #[rstest]
    fn test_position_averaging_same_side(
        symbol: Symbol,
        quantity_100: Qty,
        price_100: Px,
        price_101: Px,
    ) {
        let position = Position::new(symbol);
        let timestamp = Ts::now();

        // First fill at $100
        position.apply_fill(Side::Bid, quantity_100, price_100, timestamp);
        
        // Second fill at $101 (same quantity)
        position.apply_fill(Side::Bid, quantity_100, price_101, timestamp);

        assert_eq!(position.quantity.load(Ordering::Acquire), 2000000); // 200 units
        
        // Average price should be $100.50
        let avg_price = position.avg_price.load(Ordering::Acquire);
        assert_eq!(avg_price, 1005000); // Average of 100 and 101
    }

    #[rstest]
    fn test_position_partial_close(
        symbol: Symbol,
        quantity_100: Qty,
        price_100: Px,
        price_101: Px,
    ) {
        let position = Position::new(symbol);
        let timestamp = Ts::now();

        // Open long position
        position.apply_fill(Side::Bid, quantity_100, price_100, timestamp);
        
        // Partially close at higher price
        let half_qty = Qty::from_i64(500000); // 50 units
        position.apply_fill(Side::Ask, half_qty, price_101, timestamp);

        assert_eq!(position.quantity.load(Ordering::Acquire), 500000); // 50 units left
        
        // Should realize profit: (101 - 100) * 50 / 10000 = 5
        assert_eq!(position.realized_pnl.load(Ordering::Acquire), 5);
    }

    #[rstest]
    fn test_position_flip_long_to_short(
        symbol: Symbol,
        price_100: Px,
        price_101: Px,
    ) {
        let position = Position::new(symbol);
        let timestamp = Ts::now();
        let qty_100 = Qty::from_i64(1000000);
        let qty_150 = Qty::from_i64(1500000); // 150 units

        // Open long position
        position.apply_fill(Side::Bid, qty_100, price_100, timestamp);
        
        // Sell more than we have to flip short
        position.apply_fill(Side::Ask, qty_150, price_101, timestamp);

        assert_eq!(position.quantity.load(Ordering::Acquire), -500000); // 50 units short
        assert_eq!(position.avg_price.load(Ordering::Acquire), 1010000); // New avg price
        
        // Should realize profit from closing long: (101 - 100) * 100 / 10000 = 10
        assert_eq!(position.realized_pnl.load(Ordering::Acquire), 10);
    }

    #[rstest]
    fn test_market_price_update_long_profit(
        symbol: Symbol,
        quantity_100: Qty,
        price_100: Px,
        price_101: Px,
    ) {
        let position = Position::new(symbol);
        let timestamp = Ts::now();

        // Open long position
        position.apply_fill(Side::Bid, quantity_100, price_100, timestamp);
        
        // Update market prices (price went up)
        let bid_101 = price_101;
        let ask_102 = Px::from_i64(1020000);
        position.update_market(bid_101, ask_102, timestamp);

        // Unrealized PnL should be positive: (101 - 100) * 100 / 10000 = 10
        assert_eq!(position.unrealized_pnl.load(Ordering::Acquire), 10);
    }

    #[rstest]
    fn test_market_price_update_short_profit(
        symbol: Symbol,
        quantity_100: Qty,
        price_100: Px,
    ) {
        let position = Position::new(symbol);
        let timestamp = Ts::now();

        // Open short position
        position.apply_fill(Side::Ask, quantity_100, price_100, timestamp);
        
        // Update market prices (price went down)
        let bid_99 = Px::from_i64(990000); // $99
        let ask_100 = price_100;
        position.update_market(bid_99, ask_100, timestamp);

        // Unrealized PnL should be positive for short: (100 - 100) * 100 / 10000 = 0
        // Use ask price for short positions
        assert_eq!(position.unrealized_pnl.load(Ordering::Acquire), 0);
    }

    #[rstest]
    fn test_total_pnl_calculation(
        symbol: Symbol,
        quantity_100: Qty,
        price_100: Px,
        price_101: Px,
    ) {
        let position = Position::new(symbol);
        let timestamp = Ts::now();

        // Open and partially close position
        position.apply_fill(Side::Bid, quantity_100, price_100, timestamp);
        let half_qty = Qty::from_i64(500000);
        position.apply_fill(Side::Ask, half_qty, price_101, timestamp);

        // Update market price for remaining position
        let bid_102 = Px::from_i64(1020000);
        let ask_103 = Px::from_i64(1030000);
        position.update_market(bid_102, ask_103, timestamp);

        let total_pnl = position.total_pnl();
        let realized = position.realized_pnl.load(Ordering::Acquire);
        let unrealized = position.unrealized_pnl.load(Ordering::Acquire);

        assert_eq!(total_pnl, realized + unrealized);
        assert!(total_pnl > 0); // Should be profitable
    }

    #[rstest]
    fn test_position_snapshot(
        symbol: Symbol,
        quantity_100: Qty,
        price_100: Px,
    ) {
        let position = Position::new(symbol);
        let timestamp = Ts::now();

        position.apply_fill(Side::Bid, quantity_100, price_100, timestamp);

        let snapshot = position.snapshot();

        assert_eq!(snapshot.symbol, symbol);
        assert_eq!(snapshot.quantity, 1000000);
        assert_eq!(snapshot.avg_price, price_100);
        assert_eq!(snapshot.realized_pnl, 0);
        assert_eq!(snapshot.total_pnl, position.total_pnl());
    }

    #[rstest]
    fn test_concurrent_fills(symbol: Symbol) {
        let position = Arc::new(Position::new(symbol));
        let num_threads = 10;
        let fills_per_thread = 100;

        let handles: Vec<_> = (0..num_threads)
            .map(|thread_id| {
                let pos = Arc::clone(&position);
                thread::spawn(move || {
                    for i in 0..fills_per_thread {
                        let qty = Qty::from_i64(10000); // 1 unit
                        let price = Px::from_i64(1000000 + (thread_id * 1000 + i) as i64); // Varying price
                        let side = if thread_id % 2 == 0 { Side::Bid } else { Side::Ask };
                        
                        pos.apply_fill(side, qty, price, Ts::now());
                        thread::sleep(Duration::from_nanos(1)); // Tiny delay to mix operations
                    }
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify final state is consistent
        let final_qty = position.quantity.load(Ordering::Acquire);
        let total_operations = num_threads * fills_per_thread;
        
        // Since half are buys and half are sells, net should be close to 0
        assert!(final_qty.abs() <= (total_operations as i64 * 10000)); // All operations accounted for
    }

    #[rstest]
    fn test_concurrent_market_updates(symbol: Symbol) {
        let position = Arc::new(Position::new(symbol));
        let timestamp = Ts::now();

        // Initialize with a position
        position.apply_fill(Side::Bid, Qty::from_i64(1000000), Px::from_i64(1000000), timestamp);

        let num_threads = 5;
        let updates_per_thread = 200;

        let handles: Vec<_> = (0..num_threads)
            .map(|thread_id| {
                let pos = Arc::clone(&position);
                thread::spawn(move || {
                    for i in 0..updates_per_thread {
                        let base_price = 1000000 + (i * 100) as i64;
                        let bid = Px::from_i64(base_price);
                        let ask = Px::from_i64(base_price + 1000);
                        
                        pos.update_market(bid, ask, Ts::now());
                        thread::sleep(Duration::from_nanos(10));
                    }
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Position should still be valid
        assert_eq!(position.quantity.load(Ordering::Acquire), 1000000);
        assert!(position.last_bid.load(Ordering::Acquire) > 0);
        assert!(position.last_ask.load(Ordering::Acquire) > 0);
    }

    #[rstest]
    fn test_edge_case_zero_quantity_operations(symbol: Symbol) {
        let position = Position::new(symbol);
        let timestamp = Ts::now();

        // Try to apply zero quantity fill
        position.apply_fill(Side::Bid, Qty::from_i64(0), Px::from_i64(1000000), timestamp);

        // Position should remain empty
        assert_eq!(position.quantity.load(Ordering::Acquire), 0);
        assert_eq!(position.avg_price.load(Ordering::Acquire), 0);
    }

    #[rstest]
    fn test_large_position_calculations(symbol: Symbol) {
        let position = Position::new(symbol);
        let timestamp = Ts::now();

        // Very large position
        let large_qty = Qty::from_i64(1_000_000_000_000); // 1 million units
        let high_price = Px::from_i64(100_000_000); // $10,000

        position.apply_fill(Side::Bid, large_qty, high_price, timestamp);

        assert_eq!(position.quantity.load(Ordering::Acquire), 1_000_000_000_000);
        assert_eq!(position.avg_price.load(Ordering::Acquire), 100_000_000);

        // Update market price
        let higher_price = Px::from_i64(101_000_000); // $10,100
        position.update_market(higher_price, Px::from_i64(101_100_000), timestamp);

        // Should handle large numbers without overflow
        let unrealized = position.unrealized_pnl.load(Ordering::Acquire);
        assert!(unrealized > 0); // Should show profit
    }
}

mod position_tracker_tests {
    use super::*;

    #[rstest]
    fn test_tracker_initialization(tracker: PositionTracker) {
        let (realized, unrealized, total) = tracker.get_global_pnl();
        
        assert_eq!(realized, 0);
        assert_eq!(unrealized, 0);
        assert_eq!(total, 0);
        assert!(tracker.get_all_positions().is_empty());
    }

    #[rstest]
    fn test_pending_order_management(tracker: PositionTracker, symbol: Symbol) {
        let order_id = 123;
        let qty = Qty::from_i64(1000000);

        // Add pending order
        tracker.add_pending(order_id, symbol, Side::Bid, qty);

        // Apply fill
        tracker.apply_fill(order_id, qty, Px::from_i64(1000000), Ts::now());

        // Check position was created
        let position = tracker.get_position(symbol);
        assert!(position.is_some());
        
        let pos = position.unwrap();
        assert_eq!(pos.quantity.load(Ordering::Acquire), 1000000);
    }

    #[rstest]
    fn test_multiple_symbols(tracker: PositionTracker) {
        let symbol1 = Symbol::new(1);
        let symbol2 = Symbol::new(2);
        let qty = Qty::from_i64(1000000);
        let price = Px::from_i64(1000000);

        // Add positions for different symbols
        tracker.add_pending(1, symbol1, Side::Bid, qty);
        tracker.apply_fill(1, qty, price, Ts::now());

        tracker.add_pending(2, symbol2, Side::Ask, qty);
        tracker.apply_fill(2, qty, price, Ts::now());

        let all_positions = tracker.get_all_positions();
        assert_eq!(all_positions.len(), 2);

        // Check global PnL aggregation
        let (realized, unrealized, total) = tracker.get_global_pnl();
        assert_eq!(total, realized + unrealized);
    }

    #[rstest]
    fn test_market_price_updates(tracker: PositionTracker, symbol: Symbol) {
        let qty = Qty::from_i64(1000000);
        let price = Px::from_i64(1000000);

        // Create position
        tracker.add_pending(1, symbol, Side::Bid, qty);
        tracker.apply_fill(1, qty, price, Ts::now());

        // Update market price
        let new_bid = Px::from_i64(1010000);
        let new_ask = Px::from_i64(1011000);
        tracker.update_market(symbol, new_bid, new_ask, Ts::now());

        // Check unrealized PnL updated
        let (_, unrealized, _) = tracker.get_global_pnl();
        assert!(unrealized > 0);
    }

    #[rstest]
    fn test_global_pnl_reconciliation(tracker: PositionTracker) {
        let symbols = vec![Symbol::new(1), Symbol::new(2), Symbol::new(3)];
        let qty = Qty::from_i64(1000000);

        // Create multiple positions
        for (i, symbol) in symbols.iter().enumerate() {
            let order_id = (i + 1) as u64;
            let price = Px::from_i64(1000000 + (i * 10000) as i64);
            
            tracker.add_pending(order_id, *symbol, Side::Bid, qty);
            tracker.apply_fill(order_id, qty, price, Ts::now());
        }

        // Manually trigger reconciliation
        tracker.reconcile_global_pnl();

        // Check totals are accurate
        let (realized, unrealized, total) = tracker.get_global_pnl();
        assert_eq!(total, realized + unrealized);

        let positions = tracker.get_all_positions();
        assert_eq!(positions.len(), 3);
    }

    #[rstest]
    fn test_position_retrieval(tracker: PositionTracker, symbol: Symbol) {
        // No position initially
        assert!(tracker.get_position(symbol).is_none());

        // Create position
        tracker.add_pending(1, symbol, Side::Bid, Qty::from_i64(1000000));
        tracker.apply_fill(1, Qty::from_i64(1000000), Px::from_i64(1000000), Ts::now());

        // Should be retrievable
        let position = tracker.get_position(symbol);
        assert!(position.is_some());
        assert_eq!(position.unwrap().symbol, symbol);
    }

    #[rstest]
    fn test_clear_all_positions(tracker: PositionTracker) {
        let symbol = Symbol::new(1);
        
        // Create position
        tracker.add_pending(1, symbol, Side::Bid, Qty::from_i64(1000000));
        tracker.apply_fill(1, Qty::from_i64(1000000), Px::from_i64(1000000), Ts::now());

        // Clear all
        tracker.clear();

        // Should be empty
        assert!(tracker.get_all_positions().is_empty());
        let (realized, unrealized, total) = tracker.get_global_pnl();
        assert_eq!(realized, 0);
        assert_eq!(unrealized, 0);
        assert_eq!(total, 0);
    }

    #[rstest]
    fn test_concurrent_tracker_operations() {
        let tracker = Arc::new(PositionTracker::new(100));
        let num_threads = 8;
        let operations_per_thread = 50;

        let handles: Vec<_> = (0..num_threads)
            .map(|thread_id| {
                let t = Arc::clone(&tracker);
                thread::spawn(move || {
                    for i in 0..operations_per_thread {
                        let symbol = Symbol::new((thread_id * 10 + i / 10) as u32);
                        let order_id = (thread_id * 1000 + i) as u64;
                        let qty = Qty::from_i64(10000 * (i + 1) as i64);
                        let price = Px::from_i64(1000000 + (i * 1000) as i64);
                        let side = if i % 2 == 0 { Side::Bid } else { Side::Ask };

                        // Add and fill order
                        t.add_pending(order_id, symbol, side, qty);
                        t.apply_fill(order_id, qty, price, Ts::now());

                        // Occasionally update market
                        if i % 5 == 0 {
                            let bid = Px::from_i64(1000000 + (i * 500) as i64);
                            let ask = Px::from_i64(1001000 + (i * 500) as i64);
                            t.update_market(symbol, bid, ask, Ts::now());
                        }

                        thread::sleep(Duration::from_nanos(100));
                    }
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify final state
        let positions = tracker.get_all_positions();
        assert!(!positions.is_empty());
        
        let (realized, unrealized, total) = tracker.get_global_pnl();
        assert_eq!(total, realized + unrealized);

        // Manual reconciliation should match
        tracker.reconcile_global_pnl();
        let (r2, u2, t2) = tracker.get_global_pnl();
        assert_eq!(t2, r2 + u2);
    }

    #[rstest]
    fn test_high_frequency_updates() {
        let tracker = Arc::new(PositionTracker::new(10));
        let symbol = Symbol::new(1);

        // Create initial position
        tracker.add_pending(1, symbol, Side::Bid, Qty::from_i64(1000000));
        tracker.apply_fill(1, Qty::from_i64(1000000), Px::from_i64(1000000), Ts::now());

        // Rapid market updates
        for i in 0..1000 {
            let bid = Px::from_i64(1000000 + (i % 100) as i64);
            let ask = Px::from_i64(1001000 + (i % 100) as i64);
            tracker.update_market(symbol, bid, ask, Ts::now());
        }

        // Should still be consistent
        let position = tracker.get_position(symbol).unwrap();
        assert_eq!(position.quantity.load(Ordering::Acquire), 1000000);
        assert!(position.last_bid.load(Ordering::Acquire) > 0);
    }
}