//! Integration tests for position management
//! This file contains key integration tests to verify the test framework works

use portfolio_manager::position::{Position, PositionTracker};
use services_common::{Px, Qty, Side, Symbol, Ts};
use std::sync::atomic::Ordering;

#[tokio::test]
async fn test_position_basic_functionality() {
    let symbol = Symbol::new(1);
    let position = Position::new(symbol);

    // Test initial state
    assert_eq!(position.quantity.load(Ordering::Acquire), 0);
    assert_eq!(position.avg_price.load(Ordering::Acquire), 0);

    // Apply fill
    position.apply_fill(
        Side::Bid,
        Qty::from_i64(1000000),
        Px::from_i64(1000000),
        Ts::now(),
    );

    // Verify position updated
    assert_eq!(position.quantity.load(Ordering::Acquire), 1000000);
    assert_eq!(position.avg_price.load(Ordering::Acquire), 1000000);
}

#[tokio::test]
async fn test_position_tracker_integration() {
    let tracker = PositionTracker::new(10);
    let symbol = Symbol::new(1);

    // Add pending order
    tracker.add_pending(1, symbol, Side::Bid, Qty::from_i64(1000000));

    // Apply fill
    tracker.apply_fill(1, Qty::from_i64(1000000), Px::from_i64(1000000), Ts::now());

    // Check position was created
    let position = tracker.get_position(symbol);
    assert!(position.is_some());

    let pos = position.unwrap();
    assert_eq!(pos.quantity.load(Ordering::Acquire), 1000000);

    // Check global PnL
    let (realized, unrealized, total) = tracker.get_global_pnl();
    assert_eq!(total, realized + unrealized);
}

#[tokio::test]
async fn test_position_pnl_calculation() {
    let symbol = Symbol::new(1);
    let position = Position::new(symbol);

    // Open long position
    position.apply_fill(
        Side::Bid,
        Qty::from_i64(1000000), // 100 units
        Px::from_i64(1000000),  // $100
        Ts::now(),
    );

    // Update market price to show profit
    position.update_market(
        Px::from_i64(1100000), // $110 bid
        Px::from_i64(1101000), // $110.1 ask
        Ts::now(),
    );

    // Should show unrealized profit
    let unrealized = position.unrealized_pnl.load(Ordering::Acquire);
    assert!(unrealized > 0);

    // Close half position at profit
    position.apply_fill(
        Side::Ask,
        Qty::from_i64(500000), // Close 50 units
        Px::from_i64(1100000), // At $110
        Ts::now(),
    );

    // Should have realized profit
    let realized = position.realized_pnl.load(Ordering::Acquire);
    assert!(realized > 0);

    // Should still have remaining position
    assert_eq!(position.quantity.load(Ordering::Acquire), 500000);
}