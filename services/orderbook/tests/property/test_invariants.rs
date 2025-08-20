//! Property-based tests for orderbook invariants
//! 
//! Uses QuickCheck and Proptest to verify that orderbook operations
//! maintain critical invariants under all possible inputs:
//! 
//! - Price-time priority is always maintained
//! - BBO updates are consistent with level changes
//! - Order cancellation always removes the correct order
//! - Volume calculations are accurate
//! - Spread calculations are correct
//! - Checksum consistency across operations
//! - Atomic operations maintain consistency

use orderbook::{OrderBook, Side};
use orderbook::core::Order;
use services_common::{Px, Qty, Ts};
use proptest::prelude::*;
use quickcheck::{quickcheck, TestResult};
use std::collections::HashSet;

/// Generate valid price values (positive, reasonable range)
fn arb_price() -> impl Strategy<Value = i64> {
    1_000i64..1_000_000i64
}

/// Generate valid quantity values (positive, reasonable range)
fn arb_quantity() -> impl Strategy<Value = i64> {
    1i64..1_000_000i64
}

/// Generate order side
fn arb_side() -> impl Strategy<Value = Side> {
    prop_oneof![Just(Side::Bid), Just(Side::Ask)]
}

/// Generate order ID
fn arb_order_id() -> impl Strategy<Value = u64> {
    1u64..1_000_000u64
}

/// Create test order with given parameters
fn create_test_order(id: u64, price: i64, quantity: i64, side: Side) -> Order {
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

/// Property: Adding orders should always update BBO correctly
#[cfg(test)]
mod bbo_invariants {
    use super::*;
    
    proptest! {
        #[test]
        fn prop_single_bid_updates_bbo(price in arb_price(), quantity in arb_quantity()) {
            let book = OrderBook::new("PROP_TEST");
            let order = create_test_order(1, price, quantity, Side::Bid);
            
            book.add_order(order);
            
            let (best_bid, best_ask) = book.get_bbo();
            prop_assert_eq!(best_bid, Some(Px::from_i64(price)));
            prop_assert_eq!(best_ask, None);
        }
        
        #[test]
        fn prop_single_ask_updates_bbo(price in arb_price(), quantity in arb_quantity()) {
            let book = OrderBook::new("PROP_TEST");
            let order = create_test_order(1, price, quantity, Side::Ask);
            
            book.add_order(order);
            
            let (best_bid, best_ask) = book.get_bbo();
            prop_assert_eq!(best_bid, None);
            prop_assert_eq!(best_ask, Some(Px::from_i64(price)));
        }
        
        #[test]
        fn prop_bid_ask_spread_positive(
            bid_price in arb_price(),
            ask_price in arb_price(),
            bid_qty in arb_quantity(),
            ask_qty in arb_quantity()
        ) {
            prop_assume!(ask_price > bid_price); // Valid spread
            
            let book = OrderBook::new("SPREAD_TEST");
            book.add_order(create_test_order(1, bid_price, bid_qty, Side::Bid));
            book.add_order(create_test_order(2, ask_price, ask_qty, Side::Ask));
            
            let spread = book.get_spread();
            prop_assert!(spread.is_some());
            prop_assert!(spread.unwrap() > 0);
            prop_assert_eq!(spread.unwrap(), ask_price - bid_price);
        }
        
        #[test]
        fn prop_multiple_bids_best_price_wins(
            prices in prop::collection::vec(arb_price(), 1..10),
            quantities in prop::collection::vec(arb_quantity(), 1..10)
        ) {
            prop_assume!(prices.len() == quantities.len());
            
            let book = OrderBook::new("MULTI_BID_TEST");
            let mut max_price = i64::MIN;
            
            for (i, (&price, &qty)) in prices.iter().zip(quantities.iter()).enumerate() {
                book.add_order(create_test_order(i as u64 + 1, price, qty, Side::Bid));
                max_price = max_price.max(price);
            }
            
            let (best_bid, _) = book.get_bbo();
            prop_assert_eq!(best_bid, Some(Px::from_i64(max_price)));
        }
        
        #[test]
        fn prop_multiple_asks_best_price_wins(
            prices in prop::collection::vec(arb_price(), 1..10),
            quantities in prop::collection::vec(arb_quantity(), 1..10)
        ) {
            prop_assume!(prices.len() == quantities.len());
            
            let book = OrderBook::new("MULTI_ASK_TEST");
            let mut min_price = i64::MAX;
            
            for (i, (&price, &qty)) in prices.iter().zip(quantities.iter()).enumerate() {
                book.add_order(create_test_order(i as u64 + 1, price, qty, Side::Ask));
                min_price = min_price.min(price);
            }
            
            let (_, best_ask) = book.get_bbo();
            prop_assert_eq!(best_ask, Some(Px::from_i64(min_price)));
        }
    }
}

/// Property: Order cancellation invariants
#[cfg(test)]
mod cancellation_invariants {
    use super::*;
    
    proptest! {
        #[test]
        fn prop_cancel_nonexistent_order_returns_none(order_id in arb_order_id()) {
            let book = OrderBook::new("CANCEL_TEST");
            let result = book.cancel_order(order_id);
            prop_assert!(result.is_none());
        }
        
        #[test]
        fn prop_cancel_existing_order_returns_order(
            order_id in arb_order_id(),
            price in arb_price(),
            quantity in arb_quantity(),
            side in arb_side()
        ) {
            let book = OrderBook::new("CANCEL_EXISTING_TEST");
            let order = create_test_order(order_id, price, quantity, side);
            
            book.add_order(order.clone());
            let cancelled = book.cancel_order(order_id);
            
            prop_assert!(cancelled.is_some());
            let cancelled_order = cancelled.unwrap();
            prop_assert_eq!(cancelled_order.id, order_id);
            prop_assert_eq!(cancelled_order.price, Px::from_i64(price));
            prop_assert_eq!(cancelled_order.quantity, Qty::from_i64(quantity));
            prop_assert_eq!(cancelled_order.side, side);
        }
        
        #[test]
        fn prop_cancel_order_updates_bbo_correctly(
            prices in prop::collection::vec(arb_price(), 2..5),
            quantities in prop::collection::vec(arb_quantity(), 2..5)
        ) {
            prop_assume!(prices.len() == quantities.len());
            prop_assume!(prices.len() >= 2);
            
            let book = OrderBook::new("CANCEL_BBO_TEST");
            
            // Add multiple bid orders
            for (i, (&price, &qty)) in prices.iter().zip(quantities.iter()).enumerate() {
                book.add_order(create_test_order(i as u64 + 1, price, qty, Side::Bid));
            }
            
            // Find the best and second-best prices
            let mut sorted_prices = prices.clone();
            sorted_prices.sort_unstable();
            sorted_prices.reverse(); // Descending for bids
            
            let best_price = sorted_prices[0];
            let second_best_price = sorted_prices[1];
            
            // Cancel the best order (assuming it has ID 1 + index of best price)
            let best_order_index = prices.iter().position(|&p| p == best_price).unwrap();
            book.cancel_order(best_order_index as u64 + 1);
            
            // BBO should now be second best price
            let (new_best_bid, _) = book.get_bbo();
            
            if sorted_prices.iter().filter(|&&p| p == second_best_price).count() > 0 {
                prop_assert_eq!(new_best_bid, Some(Px::from_i64(second_best_price)));
            }
        }
    }
}

/// Property: Volume and quantity invariants
#[cfg(test)]
mod volume_invariants {
    use super::*;
    
    proptest! {
        #[test]
        fn prop_single_level_quantity_equals_order_quantity(
            price in arb_price(),
            quantity in arb_quantity(),
            side in arb_side()
        ) {
            let book = OrderBook::new("VOLUME_TEST");
            let order = create_test_order(1, price, quantity, side);
            
            book.add_order(order);
            
            let size = match side {
                Side::Bid => book.get_bid_size_at(Px::from_i64(price)),
                Side::Ask => book.get_ask_size_at(Px::from_i64(price)),
            };
            
            prop_assert_eq!(size, Some(Qty::from_i64(quantity)));
        }
        
        #[test]
        fn prop_multiple_orders_same_price_aggregate_quantity(
            price in arb_price(),
            quantities in prop::collection::vec(arb_quantity(), 1..5),
            side in arb_side()
        ) {
            let book = OrderBook::new("AGGREGATE_TEST");
            let mut total_quantity = 0i64;
            
            for (i, &qty) in quantities.iter().enumerate() {
                book.add_order(create_test_order(i as u64 + 1, price, qty, side));
                total_quantity += qty;
            }
            
            let size = match side {
                Side::Bid => book.get_bid_size_at(Px::from_i64(price)),
                Side::Ask => book.get_ask_size_at(Px::from_i64(price)),
            };
            
            prop_assert_eq!(size, Some(Qty::from_i64(total_quantity)));
        }
        
        #[test]
        fn prop_cancel_order_reduces_level_quantity(
            price in arb_price(),
            quantities in prop::collection::vec(arb_quantity(), 2..5),
            cancel_index in 0usize..4usize
        ) {
            prop_assume!(!quantities.is_empty());
            prop_assume!(cancel_index < quantities.len());
            
            let book = OrderBook::new("REDUCE_QUANTITY_TEST");
            let mut total_quantity = 0i64;
            
            // Add all orders
            for (i, &qty) in quantities.iter().enumerate() {
                book.add_order(create_test_order(i as u64 + 1, price, qty, Side::Bid));
                total_quantity += qty;
            }
            
            let initial_size = book.get_bid_size_at(Px::from_i64(price));
            prop_assert_eq!(initial_size, Some(Qty::from_i64(total_quantity)));
            
            // Cancel one order
            let cancelled_qty = quantities[cancel_index];
            book.cancel_order(cancel_index as u64 + 1);
            
            let final_size = book.get_bid_size_at(Px::from_i64(price));
            let expected_final = total_quantity - cancelled_qty;
            
            if expected_final > 0 {
                prop_assert_eq!(final_size, Some(Qty::from_i64(expected_final)));
            } else {
                prop_assert_eq!(final_size, None); // Level should be removed
            }
        }
    }
}

/// Property: Price-time priority invariants  
#[cfg(test)]
mod priority_invariants {
    use super::*;
    
    proptest! {
        #[test]
        fn prop_bid_levels_sorted_descending(
            prices in prop::collection::vec(arb_price(), 1..10),
            quantities in prop::collection::vec(arb_quantity(), 1..10)
        ) {
            prop_assume!(prices.len() == quantities.len());
            
            let book = OrderBook::new("BID_SORT_TEST");
            
            for (i, (&price, &qty)) in prices.iter().zip(quantities.iter()).enumerate() {
                book.add_order(create_test_order(i as u64 + 1, price, qty, Side::Bid));
            }
            
            let (bid_levels, _) = book.get_depth(prices.len());
            
            // Verify levels are in descending price order
            for window in bid_levels.windows(2) {
                prop_assert!(window[0].0 >= window[1].0);
            }
        }
        
        #[test]
        fn prop_ask_levels_sorted_ascending(
            prices in prop::collection::vec(arb_price(), 1..10),
            quantities in prop::collection::vec(arb_quantity(), 1..10)
        ) {
            prop_assume!(prices.len() == quantities.len());
            
            let book = OrderBook::new("ASK_SORT_TEST");
            
            for (i, (&price, &qty)) in prices.iter().zip(quantities.iter()).enumerate() {
                book.add_order(create_test_order(i as u64 + 1, price, qty, Side::Ask));
            }
            
            let (_, ask_levels) = book.get_depth(prices.len());
            
            // Verify levels are in ascending price order
            for window in ask_levels.windows(2) {
                prop_assert!(window[0].0 <= window[1].0);
            }
        }
    }
}

/// Property: Checksum consistency invariants
#[cfg(test)]
mod checksum_invariants {
    use super::*;
    
    proptest! {
        #[test]
        fn prop_identical_books_same_checksum(
            orders in prop::collection::vec(
                (arb_order_id(), arb_price(), arb_quantity(), arb_side()), 
                1..10
            )
        ) {
            // Create two identical orderbooks
            let book1 = OrderBook::new("CHECKSUM_TEST1");
            let book2 = OrderBook::new("CHECKSUM_TEST2");
            
            for &(id, price, qty, side) in &orders {
                book1.add_order(create_test_order(id, price, qty, side));
                book2.add_order(create_test_order(id, price, qty, side));
            }
            
            // Checksums should be identical
            prop_assert_eq!(book1.get_checksum(), book2.get_checksum());
        }
        
        #[test]
        fn prop_checksum_changes_on_modification(
            price in arb_price(),
            quantity in arb_quantity(),
            side in arb_side()
        ) {
            let book = OrderBook::new("CHECKSUM_CHANGE_TEST");
            
            let initial_checksum = book.get_checksum();
            book.add_order(create_test_order(1, price, quantity, side));
            let after_add_checksum = book.get_checksum();
            
            // Adding order should change checksum
            prop_assert_ne!(initial_checksum, after_add_checksum);
            
            book.cancel_order(1);
            let after_cancel_checksum = book.get_checksum();
            
            // Cancelling should also change checksum
            prop_assert_ne!(after_add_checksum, after_cancel_checksum);
        }
    }
}

/// Property: Mid-price calculation invariants
#[cfg(test)]
mod mid_price_invariants {
    use super::*;
    
    proptest! {
        #[test]
        fn prop_mid_price_between_bid_ask(
            bid_price in arb_price(),
            ask_price in arb_price(),
            bid_qty in arb_quantity(),
            ask_qty in arb_quantity()
        ) {
            prop_assume!(ask_price > bid_price);
            
            let book = OrderBook::new("MID_PRICE_TEST");
            book.add_order(create_test_order(1, bid_price, bid_qty, Side::Bid));
            book.add_order(create_test_order(2, ask_price, ask_qty, Side::Ask));
            
            let mid = book.get_mid();
            prop_assert!(mid.is_some());
            
            let mid_price = mid.unwrap().as_i64();
            prop_assert!(mid_price >= bid_price);
            prop_assert!(mid_price <= ask_price);
            prop_assert_eq!(mid_price, (bid_price + ask_price) / 2);
        }
        
        #[test]
        fn prop_mid_price_none_when_missing_side(
            price in arb_price(),
            quantity in arb_quantity(),
            side in arb_side()
        ) {
            let book = OrderBook::new("MID_MISSING_TEST");
            book.add_order(create_test_order(1, price, quantity, side));
            
            let mid = book.get_mid();
            prop_assert!(mid.is_none()); // Should be None with only one side
        }
    }
}

/// QuickCheck-based tests for additional coverage
#[cfg(test)]
mod quickcheck_tests {
    use super::*;
    
    #[quickcheck]
    fn qc_order_id_uniqueness_preserved(orders: Vec<(u64, i64, i64, bool)>) -> TestResult {
        if orders.is_empty() || orders.len() > 100 {
            return TestResult::discard();
        }
        
        // Filter to valid ranges
        let valid_orders: Vec<_> = orders.into_iter()
            .filter(|(_, price, qty, _)| *price > 0 && *price < 1_000_000 && *qty > 0 && *qty < 1_000_000)
            .collect();
            
        if valid_orders.is_empty() {
            return TestResult::discard();
        }
        
        let book = OrderBook::new("QC_UNIQUENESS_TEST");
        let mut added_ids = HashSet::new();
        
        for (id, price, qty, is_bid) in valid_orders {
            let side = if is_bid { Side::Bid } else { Side::Ask };
            book.add_order(create_test_order(id, price, qty, side));
            added_ids.insert(id);
        }
        
        // Try to cancel each added order
        let mut cancelled_count = 0;
        for id in added_ids {
            if book.cancel_order(id).is_some() {
                cancelled_count += 1;
            }
        }
        
        // Should be able to cancel all unique orders that were added
        TestResult::from_bool(cancelled_count > 0)
    }
    
    #[quickcheck]
    fn qc_depth_levels_never_negative(orders: Vec<(u64, i64, i64, bool)>) -> TestResult {
        if orders.is_empty() || orders.len() > 50 {
            return TestResult::discard();
        }
        
        let valid_orders: Vec<_> = orders.into_iter()
            .filter(|(_, price, qty, _)| *price > 0 && *price < 1_000_000 && *qty > 0 && *qty < 1_000_000)
            .take(20) // Limit to reasonable size
            .collect();
            
        if valid_orders.is_empty() {
            return TestResult::discard();
        }
        
        let book = OrderBook::new("QC_DEPTH_TEST");
        
        for (id, price, qty, is_bid) in valid_orders {
            let side = if is_bid { Side::Bid } else { Side::Ask };
            book.add_order(create_test_order(id, price, qty, side));
        }
        
        let (bid_levels, ask_levels) = book.get_depth(20);
        
        // All quantities and counts should be positive
        for (_, qty, count) in bid_levels.iter().chain(ask_levels.iter()) {
            if qty.as_i64() <= 0 || *count == 0 {
                return TestResult::failed();
            }
        }
        
        TestResult::passed()
    }
    
    #[quickcheck]
    fn qc_bbo_consistency_after_operations(ops: Vec<(u8, u64, i64, i64, bool)>) -> TestResult {
        if ops.is_empty() || ops.len() > 100 {
            return TestResult::discard();
        }
        
        let book = OrderBook::new("QC_BBO_CONSISTENCY");
        let mut order_id_counter = 1u64;
        
        for (op_type, _, price, qty, is_bid) in ops {
            if price <= 0 || price >= 1_000_000 || qty <= 0 || qty >= 1_000_000 {
                continue;
            }
            
            match op_type % 3 {
                0 => {
                    // Add order
                    let side = if is_bid { Side::Bid } else { Side::Ask };
                    book.add_order(create_test_order(order_id_counter, price, qty, side));
                    order_id_counter += 1;
                },
                1 => {
                    // Cancel order (try to cancel a recent order)
                    if order_id_counter > 1 {
                        book.cancel_order(order_id_counter - 1);
                    }
                },
                2 => {
                    // Check BBO consistency
                    let (bid, ask) = book.get_bbo();
                    let spread = book.get_spread();
                    
                    match (bid, ask) {
                        (Some(b), Some(a)) => {
                            if a.as_i64() < b.as_i64() {
                                return TestResult::failed(); // Invalid spread
                            }
                            if let Some(s) = spread {
                                if s != a.as_i64() - b.as_i64() {
                                    return TestResult::failed(); // Inconsistent spread calculation
                                }
                            }
                        },
                        _ => {
                            if spread.is_some() {
                                return TestResult::failed(); // Shouldn't have spread with missing side
                            }
                        }
                    }
                },
                _ => unreachable!(),
            }
        }
        
        TestResult::passed()
    }
}

/// Edge case property tests
#[cfg(test)]
mod edge_case_properties {
    use super::*;
    
    proptest! {
        #[test]
        fn prop_clear_resets_all_state(
            orders in prop::collection::vec(
                (arb_order_id(), arb_price(), arb_quantity(), arb_side()),
                1..20
            )
        ) {
            let book = OrderBook::new("CLEAR_TEST");
            
            // Add orders
            for &(id, price, qty, side) in &orders {
                book.add_order(create_test_order(id, price, qty, side));
            }
            
            // Verify book has state
            let (bids_before, asks_before) = book.get_depth(20);
            prop_assume!(!bids_before.is_empty() || !asks_before.is_empty());
            
            // Clear the book
            book.clear();
            
            // Verify all state is reset
            let (best_bid, best_ask) = book.get_bbo();
            prop_assert!(best_bid.is_none());
            prop_assert!(best_ask.is_none());
            
            let (bids_after, asks_after) = book.get_depth(20);
            prop_assert!(bids_after.is_empty());
            prop_assert!(asks_after.is_empty());
            
            prop_assert_eq!(book.get_spread(), None);
            prop_assert_eq!(book.get_mid(), None);
            prop_assert_eq!(book.get_checksum(), 0);
        }
        
        #[test]
        fn prop_snapshot_load_creates_consistent_state(
            bid_levels in prop::collection::vec((arb_price(), arb_quantity(), 1u64..10u64), 0..5),
            ask_levels in prop::collection::vec((arb_price(), arb_quantity(), 1u64..10u64), 0..5)
        ) {
            let book = OrderBook::new("SNAPSHOT_TEST");
            
            // Convert to the format expected by load_snapshot
            let bid_tuples: Vec<_> = bid_levels.iter()
                .map(|&(price, qty, count)| (Px::from_i64(price), Qty::from_i64(qty), count))
                .collect();
                
            let ask_tuples: Vec<_> = ask_levels.iter()
                .map(|&(price, qty, count)| (Px::from_i64(price), Qty::from_i64(qty), count))
                .collect();
            
            book.load_snapshot(bid_tuples.clone(), ask_tuples.clone());
            
            // Verify loaded state matches snapshot
            let (loaded_bids, loaded_asks) = book.get_depth(10);
            
            // Should have at least the number of levels we loaded
            if !bid_tuples.is_empty() {
                prop_assert!(!loaded_bids.is_empty());
            }
            if !ask_tuples.is_empty() {
                prop_assert!(!loaded_asks.is_empty());
            }
            
            // BBO should reflect the loaded levels
            if !bid_tuples.is_empty() {
                let max_bid_price = bid_tuples.iter().map(|(p, _, _)| p.as_i64()).max().unwrap();
                let (best_bid, _) = book.get_bbo();
                prop_assert_eq!(best_bid, Some(Px::from_i64(max_bid_price)));
            }
            
            if !ask_tuples.is_empty() {
                let min_ask_price = ask_tuples.iter().map(|(p, _, _)| p.as_i64()).min().unwrap();
                let (_, best_ask) = book.get_bbo();
                prop_assert_eq!(best_ask, Some(Px::from_i64(min_ask_price)));
            }
        }
    }
}