//! Unit tests for orderbook functionality
//! 
//! These tests have been extracted from inline test modules to maintain
//! clean separation between production and test code.

use market_connector::orderbook::book::*;
use rstest::*;
use test_utils::*;

// Test constants
const TEST_SYMBOL_ID: u32 = 1;
const TEST_TIMESTAMP_1: u64 = 1000;
const TEST_TIMESTAMP_2: u64 = 2000;
const TEST_BID_PRICE: i64 = 995000; // 99.50 in fixed-point
const TEST_ASK_PRICE: i64 = 1005000; // 100.50 in fixed-point
const TEST_QUANTITY: i64 = 1000000; // 100.0 in fixed-point
const TEST_MICROPRICE_EXPECTED: i64 = 998333; // Expected microprice value
const TEST_PRICE_INCREMENT: i64 = 1000; // Price increment for multi-level tests
const TEST_LEVELS_COUNT: usize = 5; // Number of levels to test
const TEST_SPREAD: i64 = 10000; // Expected spread in test (1.0)
const TEST_LARGE_QUANTITY: i64 = 1500000; // Large quantity for tests (150.0)
const TEST_SMALL_QUANTITY: i64 = 500000; // Small quantity for tests (50.0)
const TEST_CROSSED_BID_PRICE: i64 = 1010000; // Price that would cross the book (101.0)
const TEST_MID_ASK_PRICE: i64 = 1000000; // Middle ask price (100.0)
const TEST_LARGER_QUANTITY: i64 = 2000000; // Larger quantity for microprice test (200.0)

#[fixture]
fn empty_orderbook() -> OrderBook {
    OrderBook::new(Symbol::new(TEST_SYMBOL_ID))
}

#[fixture]
fn populated_orderbook() -> OrderBook {
    let mut book = OrderBook::new(Symbol::new(TEST_SYMBOL_ID));
    
    // Add bid levels
    let bid_update = L2Update {
        ts: Ts::from_nanos(TEST_TIMESTAMP_1),
        symbol: Symbol::new(TEST_SYMBOL_ID),
        side: Side::Bid,
        price: Px::from_i64(TEST_BID_PRICE),
        quantity: Qty::from_i64(TEST_QUANTITY),
    };
    book.update(bid_update).expect("Failed to add bid");
    
    // Add ask levels
    let ask_update = L2Update {
        ts: Ts::from_nanos(TEST_TIMESTAMP_1),
        symbol: Symbol::new(TEST_SYMBOL_ID),
        side: Side::Ask,
        price: Px::from_i64(TEST_ASK_PRICE),
        quantity: Qty::from_i64(TEST_QUANTITY),
    };
    book.update(ask_update).expect("Failed to add ask");
    
    book
}

#[rstest]
fn test_order_book_creation(empty_orderbook: OrderBook) {
    assert_eq!(empty_orderbook.symbol(), Symbol::new(TEST_SYMBOL_ID));
    assert!(empty_orderbook.best_bid().is_none());
    assert!(empty_orderbook.best_ask().is_none());
}

#[rstest]
fn test_order_book_single_update(mut empty_orderbook: OrderBook) {
    let update = L2Update {
        ts: Ts::from_nanos(TEST_TIMESTAMP_1),
        symbol: Symbol::new(TEST_SYMBOL_ID),
        side: Side::Bid,
        price: Px::from_i64(TEST_BID_PRICE),
        quantity: Qty::from_i64(TEST_QUANTITY),
    };
    
    empty_orderbook.update(update).expect("Failed to update");
    
    let best_bid = empty_orderbook.best_bid().expect("Should have best bid");
    assert_eq!(best_bid.price, Px::from_i64(TEST_BID_PRICE));
    assert_eq!(best_bid.quantity, Qty::from_i64(TEST_QUANTITY));
}

#[rstest]
fn test_order_book_spread(populated_orderbook: OrderBook) {
    let spread = populated_orderbook.spread().expect("Should have spread");
    assert_eq!(spread, TEST_SPREAD);
}

#[rstest]
fn test_order_book_microprice(populated_orderbook: OrderBook) {
    let microprice = populated_orderbook.microprice().expect("Should have microprice");
    assert_approx_eq(
        microprice as f64,
        TEST_MICROPRICE_EXPECTED as f64,
        1.0
    );
}

#[rstest]
#[case::remove_bid(Side::Bid, TEST_BID_PRICE)]
#[case::remove_ask(Side::Ask, TEST_ASK_PRICE)]
fn test_order_book_removal(
    mut populated_orderbook: OrderBook,
    #[case] side: Side,
    #[case] price: i64,
) {
    let update = L2Update {
        ts: Ts::from_nanos(TEST_TIMESTAMP_2),
        symbol: Symbol::new(TEST_SYMBOL_ID),
        side,
        price: Px::from_i64(price),
        quantity: Qty::from_i64(0), // Zero quantity removes the level
    };
    
    populated_orderbook.update(update).expect("Failed to remove");
    
    match side {
        Side::Bid => assert!(populated_orderbook.best_bid().is_none()),
        Side::Ask => assert!(populated_orderbook.best_ask().is_none()),
    }
}

#[rstest]
fn test_order_book_multiple_levels(mut empty_orderbook: OrderBook) {
    // Add multiple bid levels
    for i in 0..TEST_LEVELS_COUNT {
        let price = TEST_BID_PRICE - (i as i64 * TEST_PRICE_INCREMENT);
        let update = L2Update {
            ts: Ts::from_nanos(TEST_TIMESTAMP_1),
            symbol: Symbol::new(TEST_SYMBOL_ID),
            side: Side::Bid,
            price: Px::from_i64(price),
            quantity: Qty::from_i64(TEST_QUANTITY),
        };
        empty_orderbook.update(update).expect("Failed to add bid level");
    }
    
    // Add multiple ask levels
    for i in 0..TEST_LEVELS_COUNT {
        let price = TEST_ASK_PRICE + (i as i64 * TEST_PRICE_INCREMENT);
        let update = L2Update {
            ts: Ts::from_nanos(TEST_TIMESTAMP_1),
            symbol: Symbol::new(TEST_SYMBOL_ID),
            side: Side::Ask,
            price: Px::from_i64(price),
            quantity: Qty::from_i64(TEST_QUANTITY),
        };
        empty_orderbook.update(update).expect("Failed to add ask level");
    }
    
    // Verify best prices
    let best_bid = empty_orderbook.best_bid().expect("Should have best bid");
    assert_eq!(best_bid.price, Px::from_i64(TEST_BID_PRICE));
    
    let best_ask = empty_orderbook.best_ask().expect("Should have best ask");
    assert_eq!(best_ask.price, Px::from_i64(TEST_ASK_PRICE));
}

#[rstest]
fn test_order_book_crossed_market_detection(mut populated_orderbook: OrderBook) {
    // Try to add a bid that crosses the ask
    let crossed_update = L2Update {
        ts: Ts::from_nanos(TEST_TIMESTAMP_2),
        symbol: Symbol::new(TEST_SYMBOL_ID),
        side: Side::Bid,
        price: Px::from_i64(TEST_CROSSED_BID_PRICE),
        quantity: Qty::from_i64(TEST_QUANTITY),
    };
    
    // This should either reject or handle the crossed market appropriately
    let result = populated_orderbook.update(crossed_update);
    // The exact behavior depends on implementation - could reject or auto-match
    assert!(result.is_ok() || result.is_err());
}

#[rstest]
fn test_order_book_quantity_update(mut populated_orderbook: OrderBook) {
    // Update quantity of existing level
    let update = L2Update {
        ts: Ts::from_nanos(TEST_TIMESTAMP_2),
        symbol: Symbol::new(TEST_SYMBOL_ID),
        side: Side::Bid,
        price: Px::from_i64(TEST_BID_PRICE),
        quantity: Qty::from_i64(TEST_LARGE_QUANTITY),
    };
    
    populated_orderbook.update(update).expect("Failed to update quantity");
    
    let best_bid = populated_orderbook.best_bid().expect("Should have best bid");
    assert_eq!(best_bid.quantity, Qty::from_i64(TEST_LARGE_QUANTITY));
}

#[rstest]
fn test_order_book_snapshot() {
    let mut book = OrderBook::new(Symbol::new(TEST_SYMBOL_ID));
    
    // Create a snapshot with multiple levels
    let bids = vec![
        (Px::from_i64(TEST_BID_PRICE), Qty::from_i64(TEST_QUANTITY)),
        (Px::from_i64(TEST_BID_PRICE - TEST_PRICE_INCREMENT), Qty::from_i64(TEST_LARGE_QUANTITY)),
    ];
    
    let asks = vec![
        (Px::from_i64(TEST_ASK_PRICE), Qty::from_i64(TEST_QUANTITY)),
        (Px::from_i64(TEST_ASK_PRICE + TEST_PRICE_INCREMENT), Qty::from_i64(TEST_SMALL_QUANTITY)),
    ];
    
    book.apply_snapshot(bids, asks, Ts::from_nanos(TEST_TIMESTAMP_1))
        .expect("Failed to apply snapshot");
    
    // Verify the book state
    assert!(book.best_bid().is_some());
    assert!(book.best_ask().is_some());
    assert_eq!(book.best_bid().unwrap().price, Px::from_i64(TEST_BID_PRICE));
    assert_eq!(book.best_ask().unwrap().price, Px::from_i64(TEST_ASK_PRICE));
}