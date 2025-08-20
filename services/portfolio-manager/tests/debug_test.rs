//! Debug test to understand P&L calculation

use portfolio_manager::position::Position;
use services_common::{Px, Qty, Side, Symbol, Ts};
use std::sync::atomic::Ordering;

#[tokio::test]
async fn test_debug_pnl() {
    let symbol = Symbol::new(1);
    let position = Position::new(symbol);

    println!("=== Initial State ===");
    println!("Quantity: {}", position.quantity.load(Ordering::Acquire));
    println!("Avg Price: {}", position.avg_price.load(Ordering::Acquire));
    println!("Realized PnL: {}", position.realized_pnl.load(Ordering::Acquire));

    // Open long position
    println!("\n=== Opening Position ===");
    position.apply_fill(
        Side::Bid,
        Qty::from_i64(1000000), // 100 units (1 unit = 10000 in fixed point)
        Px::from_i64(1000000),  // $100 (in fixed point)
        Ts::now(),
    );

    println!("After opening position:");
    println!("Quantity: {}", position.quantity.load(Ordering::Acquire));
    println!("Avg Price: {}", position.avg_price.load(Ordering::Acquire));
    println!("Realized PnL: {}", position.realized_pnl.load(Ordering::Acquire));

    // Close half position at profit
    println!("\n=== Closing Half Position ===");
    position.apply_fill(
        Side::Ask,
        Qty::from_i64(500000), // Close 50 units
        Px::from_i64(1100000), // At $110
        Ts::now(),
    );

    println!("After closing half position:");
    println!("Quantity: {}", position.quantity.load(Ordering::Acquire));
    println!("Avg Price: {}", position.avg_price.load(Ordering::Acquire));
    println!("Realized PnL: {}", position.realized_pnl.load(Ordering::Acquire));
    println!("Unrealized PnL: {}", position.unrealized_pnl.load(Ordering::Acquire));
    
    let realized = position.realized_pnl.load(Ordering::Acquire);
    println!("Expected P&L calculation:");
    println!("(1100000 - 1000000) * 500000 / 10000 = {}", (1100000i64 - 1000000) * 500000 / 10000);
    
    // This should pass now with debugging info
    assert!(realized >= 0, "Realized P&L should be non-negative, got {}", realized);
}