//! Unit tests for PositionManager
//!
//! Comprehensive tests covering:
//! - Position tracking and state updates
//! - Average price calculations
//! - Profit and loss computations
//! - Position building and closing scenarios
//! - Market price updates and P&L recalculation
//! - Portfolio-level aggregations
//! - Concurrent position updates
//! - Edge cases and error scenarios

use anyhow::Result;
use rstest::*;
use std::sync::Arc;
use trading_gateway::{
    position_manager::{PositionInfo, PositionManager},
    Side,
};
use services_common::{Px, Qty, Symbol};

/// Test fixture for creating a PositionManager
#[fixture]
fn position_manager() -> PositionManager {
    PositionManager::new()
}

/// Test fixture for a sample symbol
#[fixture]
fn sample_symbol() -> Symbol {
    Symbol(1) // BTCUSDT
}

/// Test fixture for another symbol
#[fixture]
fn eth_symbol() -> Symbol {
    Symbol(2) // ETHUSDT
}

#[rstest]
#[tokio::test]
async fn test_position_manager_creation(position_manager: PositionManager) {
    // Test basic creation and initial state
    let count = position_manager.get_position_count().await;
    assert_eq!(count, 0);
    
    let positions = position_manager.get_all_positions().await;
    assert!(positions.is_empty());
    
    let (unrealized, realized) = position_manager.get_total_pnl().await;
    assert_eq!(unrealized, 0);
    assert_eq!(realized, 0);
}

#[rstest]
#[tokio::test]
async fn test_simple_buy_position(
    position_manager: PositionManager,
    sample_symbol: Symbol
) -> Result<()> {
    // Execute a simple buy order
    let quantity = Qty::from_i64(10000); // 1 unit
    let price = Px::from_i64(1000000000); // $100.00
    
    position_manager.update_position(sample_symbol, Side::Buy, quantity, price).await?;
    
    let position = position_manager.get_position(sample_symbol).await;
    assert!(position.is_some());
    
    let pos = position.unwrap();
    assert_eq!(pos.symbol, sample_symbol);
    assert_eq!(pos.quantity, 10000); // Long 1 unit
    assert_eq!(pos.avg_entry_price, 1000000000); // $100.00
    assert_eq!(pos.current_price, 1000000000);
    assert_eq!(pos.unrealized_pnl, 0); // No price movement
    assert_eq!(pos.realized_pnl, 0); // No closed positions
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_simple_sell_position(
    position_manager: PositionManager,
    sample_symbol: Symbol
) -> Result<()> {
    // Execute a simple sell order (short)
    let quantity = Qty::from_i64(20000); // 2 units
    let price = Px::from_i64(2000000000); // $200.00
    
    position_manager.update_position(sample_symbol, Side::Sell, quantity, price).await?;
    
    let position = position_manager.get_position(sample_symbol).await.unwrap();
    assert_eq!(position.quantity, -20000); // Short 2 units
    assert_eq!(position.avg_entry_price, 2000000000); // $200.00
    assert_eq!(position.unrealized_pnl, 0); // No price movement
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_position_averaging_same_side(
    position_manager: PositionManager,
    sample_symbol: Symbol
) -> Result<()> {
    // First buy: 1 unit at $100
    position_manager.update_position(
        sample_symbol, 
        Side::Buy, 
        Qty::from_i64(10000), 
        Px::from_i64(1000000000)
    ).await?;
    
    // Second buy: 1 unit at $120
    position_manager.update_position(
        sample_symbol, 
        Side::Buy, 
        Qty::from_i64(10000), 
        Px::from_i64(1200000000)
    ).await?;
    
    let position = position_manager.get_position(sample_symbol).await.unwrap();
    
    // Should have 2 units total
    assert_eq!(position.quantity, 20000);
    
    // Average price should be $110.00 ((100 * 1 + 120 * 1) / 2)
    assert_eq!(position.avg_entry_price, 1100000000);
    
    // Current price should be the last price
    assert_eq!(position.current_price, 1200000000);
    
    // Unrealized P&L should reflect price appreciation on entire position
    // (120 - 110) * 2 units = $20.00
    let expected_pnl = (1200000000 - 1100000000) * 20000 / 10000;
    assert_eq!(position.unrealized_pnl, expected_pnl);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_position_partial_close_with_profit(
    position_manager: PositionManager,
    sample_symbol: Symbol
) -> Result<()> {
    // Open long position: 3 units at $100
    position_manager.update_position(
        sample_symbol, 
        Side::Buy, 
        Qty::from_i64(30000), 
        Px::from_i64(1000000000)
    ).await?;
    
    // Close 1 unit at $150 (profit)
    position_manager.update_position(
        sample_symbol, 
        Side::Sell, 
        Qty::from_i64(10000), 
        Px::from_i64(1500000000)
    ).await?;
    
    let position = position_manager.get_position(sample_symbol).await.unwrap();
    
    // Should have 2 units remaining
    assert_eq!(position.quantity, 20000);
    
    // Average entry price should remain $100 for remaining position
    assert_eq!(position.avg_entry_price, 1000000000);
    
    // Realized P&L should show profit from closed portion
    // (150 - 100) * 1 unit = $50.00
    let expected_realized_pnl = (1500000000 - 1000000000) * 10000 / 10000;
    assert_eq!(position.realized_pnl, expected_realized_pnl);
    
    // Unrealized P&L on remaining position
    // (150 - 100) * 2 units = $100.00
    let expected_unrealized_pnl = (1500000000 - 1000000000) * 20000 / 10000;
    assert_eq!(position.unrealized_pnl, expected_unrealized_pnl);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_position_full_close(
    position_manager: PositionManager,
    sample_symbol: Symbol
) -> Result<()> {
    // Open position: 2 units at $100
    position_manager.update_position(
        sample_symbol, 
        Side::Buy, 
        Qty::from_i64(20000), 
        Px::from_i64(1000000000)
    ).await?;
    
    // Close entire position at $90 (loss)
    position_manager.update_position(
        sample_symbol, 
        Side::Sell, 
        Qty::from_i64(20000), 
        Px::from_i64(900000000)
    ).await?;
    
    let position = position_manager.get_position(sample_symbol).await.unwrap();
    
    // Should have zero quantity
    assert_eq!(position.quantity, 0);
    
    // Unrealized P&L should be zero
    assert_eq!(position.unrealized_pnl, 0);
    
    // Realized P&L should show the loss
    // (90 - 100) * 2 units = -$20.00
    let expected_realized_pnl = (900000000 - 1000000000) * 20000 / 10000;
    assert_eq!(position.realized_pnl, expected_realized_pnl);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_position_reversal(
    position_manager: PositionManager,
    sample_symbol: Symbol
) -> Result<()> {
    // Start with long position: 1 unit at $100
    position_manager.update_position(
        sample_symbol, 
        Side::Buy, 
        Qty::from_i64(10000), 
        Px::from_i64(1000000000)
    ).await?;
    
    // Reverse to short: sell 3 units at $120
    // This closes the 1 long and creates 2 short
    position_manager.update_position(
        sample_symbol, 
        Side::Sell, 
        Qty::from_i64(30000), 
        Px::from_i64(1200000000)
    ).await?;
    
    let position = position_manager.get_position(sample_symbol).await.unwrap();
    
    // Should be short 2 units
    assert_eq!(position.quantity, -20000);
    
    // New average price should be the reversal price
    assert_eq!(position.avg_entry_price, 1200000000);
    
    // Realized P&L from closing the long position
    // (120 - 100) * 1 unit = $20.00 profit
    let expected_realized_pnl = (1200000000 - 1000000000) * 10000 / 10000;
    assert_eq!(position.realized_pnl, expected_realized_pnl);
    
    // No unrealized P&L yet (no price movement from reversal price)
    assert_eq!(position.unrealized_pnl, 0);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_short_position_mechanics(
    position_manager: PositionManager,
    sample_symbol: Symbol
) -> Result<()> {
    // Open short position: 2 units at $200
    position_manager.update_position(
        sample_symbol, 
        Side::Sell, 
        Qty::from_i64(20000), 
        Px::from_i64(2000000000)
    ).await?;
    
    // Add to short position: 1 more unit at $180
    position_manager.update_position(
        sample_symbol, 
        Side::Sell, 
        Qty::from_i64(10000), 
        Px::from_i64(1800000000)
    ).await?;
    
    let position = position_manager.get_position(sample_symbol).await.unwrap();
    
    // Should be short 3 units
    assert_eq!(position.quantity, -30000);
    
    // Average price should be weighted average of short entries
    // ((200 * 2) + (180 * 1)) / 3 = 193.33
    let expected_avg = (2000000000 * 20000 + 1800000000 * 10000) / 30000;
    assert_eq!(position.avg_entry_price, expected_avg);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_market_price_updates_and_pnl(
    position_manager: PositionManager,
    sample_symbol: Symbol
) -> Result<()> {
    // Open long position: 2 units at $100
    position_manager.update_position(
        sample_symbol, 
        Side::Buy, 
        Qty::from_i64(20000), 
        Px::from_i64(1000000000)
    ).await?;
    
    // Update market price to $110
    position_manager.update_market_price(sample_symbol, Px::from_i64(1100000000)).await;
    
    let position1 = position_manager.get_position(sample_symbol).await.unwrap();
    assert_eq!(position1.current_price, 1100000000);
    
    // Unrealized P&L should show profit
    // (110 - 100) * 2 units = $20.00
    let expected_pnl1 = (1100000000 - 1000000000) * 20000 / 10000;
    assert_eq!(position1.unrealized_pnl, expected_pnl1);
    
    // Update market price to $90 (loss)
    position_manager.update_market_price(sample_symbol, Px::from_i64(900000000)).await;
    
    let position2 = position_manager.get_position(sample_symbol).await.unwrap();
    assert_eq!(position2.current_price, 900000000);
    
    // Unrealized P&L should show loss
    // (90 - 100) * 2 units = -$20.00
    let expected_pnl2 = (900000000 - 1000000000) * 20000 / 10000;
    assert_eq!(position2.unrealized_pnl, expected_pnl2);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_multiple_symbols_independence(
    position_manager: PositionManager,
    sample_symbol: Symbol,
    eth_symbol: Symbol
) -> Result<()> {
    // BTC position: long 1 unit at $100
    position_manager.update_position(
        sample_symbol, 
        Side::Buy, 
        Qty::from_i64(10000), 
        Px::from_i64(1000000000)
    ).await?;
    
    // ETH position: short 2 units at $50
    position_manager.update_position(
        eth_symbol, 
        Side::Sell, 
        Qty::from_i64(20000), 
        Px::from_i64(500000000)
    ).await?;
    
    // Verify positions are independent
    let btc_pos = position_manager.get_position(sample_symbol).await.unwrap();
    let eth_pos = position_manager.get_position(eth_symbol).await.unwrap();
    
    assert_eq!(btc_pos.quantity, 10000); // Long
    assert_eq!(eth_pos.quantity, -20000); // Short
    
    assert_eq!(btc_pos.avg_entry_price, 1000000000);
    assert_eq!(eth_pos.avg_entry_price, 500000000);
    
    // Update BTC price, should not affect ETH
    position_manager.update_market_price(sample_symbol, Px::from_i64(1200000000)).await;
    
    let btc_pos_updated = position_manager.get_position(sample_symbol).await.unwrap();
    let eth_pos_unchanged = position_manager.get_position(eth_symbol).await.unwrap();
    
    assert_eq!(btc_pos_updated.current_price, 1200000000);
    assert_eq!(eth_pos_unchanged.current_price, 500000000); // Unchanged
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_portfolio_pnl_aggregation(
    position_manager: PositionManager,
    sample_symbol: Symbol,
    eth_symbol: Symbol
) -> Result<()> {
    // BTC: long 1 unit at $100, current $120 (+$20 unrealized)
    position_manager.update_position(
        sample_symbol, 
        Side::Buy, 
        Qty::from_i64(10000), 
        Px::from_i64(1000000000)
    ).await?;
    position_manager.update_market_price(sample_symbol, Px::from_i64(1200000000)).await;
    
    // ETH: short 2 units at $50, current $45 (+$10 unrealized profit on short)
    position_manager.update_position(
        eth_symbol, 
        Side::Sell, 
        Qty::from_i64(20000), 
        Px::from_i64(500000000)
    ).await?;
    position_manager.update_market_price(eth_symbol, Px::from_i64(450000000)).await;
    
    // Close half of BTC position at $110 (profit on closed portion)
    position_manager.update_position(
        sample_symbol, 
        Side::Sell, 
        Qty::from_i64(5000), 
        Px::from_i64(1100000000)
    ).await?;
    
    let (total_unrealized, total_realized) = position_manager.get_total_pnl().await;
    
    // Calculate expected values
    let btc_unrealized = (1100000000 - 1000000000) * 5000 / 10000; // Remaining 0.5 units
    let btc_realized = (1100000000 - 1000000000) * 5000 / 10000; // Closed 0.5 units
    let eth_unrealized = -20000 * (450000000 - 500000000) / 10000; // Short profit
    
    let expected_unrealized = btc_unrealized + eth_unrealized;
    let expected_realized = btc_realized;
    
    assert_eq!(total_unrealized, expected_unrealized);
    assert_eq!(total_realized, expected_realized);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_position_count_tracking(
    position_manager: PositionManager
) -> Result<()> {
    // Start with no positions
    assert_eq!(position_manager.get_position_count().await, 0);
    
    // Add first position
    position_manager.update_position(
        Symbol(1), 
        Side::Buy, 
        Qty::from_i64(10000), 
        Px::from_i64(1000000000)
    ).await?;
    
    assert_eq!(position_manager.get_position_count().await, 1);
    
    // Add second symbol
    position_manager.update_position(
        Symbol(2), 
        Side::Sell, 
        Qty::from_i64(20000), 
        Px::from_i64(2000000000)
    ).await?;
    
    assert_eq!(position_manager.get_position_count().await, 2);
    
    // Update existing position (should not increase count)
    position_manager.update_position(
        Symbol(1), 
        Side::Buy, 
        Qty::from_i64(5000), 
        Px::from_i64(1100000000)
    ).await?;
    
    assert_eq!(position_manager.get_position_count().await, 2);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_get_all_positions(
    position_manager: PositionManager
) -> Result<()> {
    // Add multiple positions
    let symbols_and_data = vec![
        (Symbol(1), Side::Buy, 10000i64, 1000000000i64),
        (Symbol(2), Side::Sell, 20000, 2000000000),
        (Symbol(3), Side::Buy, 15000, 1500000000),
    ];
    
    for (symbol, side, qty, price) in &symbols_and_data {
        position_manager.update_position(
            *symbol, 
            *side, 
            Qty::from_i64(*qty), 
            Px::from_i64(*price)
        ).await?;
    }
    
    let all_positions = position_manager.get_all_positions().await;
    assert_eq!(all_positions.len(), 3);
    
    // Verify each position exists in the results
    for (expected_symbol, expected_side, expected_qty, expected_price) in symbols_and_data {
        let found = all_positions.iter().find(|(symbol, _)| *symbol == expected_symbol);
        assert!(found.is_some());
        
        let (_, position) = found.unwrap();
        let expected_signed_qty = match expected_side {
            Side::Buy => expected_qty,
            Side::Sell => -expected_qty,
        };
        
        assert_eq!(position.quantity, expected_signed_qty);
        assert_eq!(position.avg_entry_price, expected_price);
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_close_all_positions(
    position_manager: PositionManager
) -> Result<()> {
    // Add several positions
    for i in 1..=5 {
        position_manager.update_position(
            Symbol(i), 
            Side::Buy, 
            Qty::from_i64(i * 10000), 
            Px::from_i64(i * 1000000000)
        ).await?;
    }
    
    assert_eq!(position_manager.get_position_count().await, 5);
    
    // Close all positions
    position_manager.close_all_positions().await?;
    
    assert_eq!(position_manager.get_position_count().await, 0);
    
    let all_positions = position_manager.get_all_positions().await;
    assert!(all_positions.is_empty());
    
    Ok(())
}

#[rstest]
#[case(Side::Buy, 10000, 1000000000, 1100000000)]    // Long position gains
#[case(Side::Buy, 20000, 2000000000, 1900000000)]    // Long position loses  
#[case(Side::Sell, 15000, 1500000000, 1400000000)]   // Short position gains
#[case(Side::Sell, 25000, 2500000000, 2600000000)]   // Short position loses
#[tokio::test]
async fn test_pnl_calculations_parameterized(
    position_manager: PositionManager,
    #[case] side: Side,
    #[case] quantity: i64,
    #[case] entry_price: i64,
    #[case] current_price: i64
) -> Result<()> {
    let symbol = Symbol(99);
    
    // Open position
    position_manager.update_position(
        symbol, 
        side, 
        Qty::from_i64(quantity), 
        Px::from_i64(entry_price)
    ).await?;
    
    // Update market price
    position_manager.update_market_price(symbol, Px::from_i64(current_price)).await;
    
    let position = position_manager.get_position(symbol).await.unwrap();
    
    // Calculate expected P&L
    let expected_pnl = match side {
        Side::Buy => {
            // Long position: profit when price goes up
            quantity * (current_price - entry_price) / 10000
        },
        Side::Sell => {
            // Short position: profit when price goes down
            -quantity * (current_price - entry_price) / 10000
        }
    };
    
    assert_eq!(position.unrealized_pnl, expected_pnl,
        "P&L calculation failed for {:?} position", side);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_concurrent_position_updates() -> Result<()> {
    let position_manager = Arc::new(PositionManager::new());
    let mut handles = Vec::new();
    
    // Concurrent updates to different symbols
    for i in 1..=20 {
        let pm = position_manager.clone();
        let symbol = Symbol(i % 5 + 1); // Use 5 different symbols
        
        let handle = tokio::spawn(async move {
            pm.update_position(
                symbol,
                if i % 2 == 0 { Side::Buy } else { Side::Sell },
                Qty::from_i64(i * 1000),
                Px::from_i64(i * 100000000)
            ).await
        });
        handles.push(handle);
    }
    
    // Wait for all updates
    for handle in handles {
        handle.await??;
    }
    
    // Verify final state
    let count = position_manager.get_position_count().await;
    assert!(count <= 5, "Should not exceed 5 symbols");
    
    let positions = position_manager.get_all_positions().await;
    assert!(!positions.is_empty());
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_zero_quantity_position_handling(
    position_manager: PositionManager,
    sample_symbol: Symbol
) -> Result<()> {
    // Open and immediately close position
    position_manager.update_position(
        sample_symbol, 
        Side::Buy, 
        Qty::from_i64(10000), 
        Px::from_i64(1000000000)
    ).await?;
    
    position_manager.update_position(
        sample_symbol, 
        Side::Sell, 
        Qty::from_i64(10000), 
        Px::from_i64(1100000000)
    ).await?;
    
    let position = position_manager.get_position(sample_symbol).await.unwrap();
    
    // Position should exist but with zero quantity
    assert_eq!(position.quantity, 0);
    assert_eq!(position.unrealized_pnl, 0);
    
    // Should have realized P&L
    assert_ne!(position.realized_pnl, 0);
    
    // Position should still be tracked
    assert_eq!(position_manager.get_position_count().await, 1);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_complex_trading_sequence(
    position_manager: PositionManager,
    sample_symbol: Symbol
) -> Result<()> {
    // Complex sequence: build, add, partial close, reverse, close
    
    // 1. Initial long position: 2 units at $100
    position_manager.update_position(
        sample_symbol, Side::Buy, Qty::from_i64(20000), Px::from_i64(1000000000)
    ).await?;
    
    // 2. Add to position: 1 unit at $110 (average becomes $103.33)
    position_manager.update_position(
        sample_symbol, Side::Buy, Qty::from_i64(10000), Px::from_i64(1100000000)
    ).await?;
    
    // 3. Partial close: sell 1 unit at $120 (realize profit)
    position_manager.update_position(
        sample_symbol, Side::Sell, Qty::from_i64(10000), Px::from_i64(1200000000)
    ).await?;
    
    // 4. Reverse to short: sell 4 units at $90 (close 2 long, open 2 short)
    position_manager.update_position(
        sample_symbol, Side::Sell, Qty::from_i64(40000), Px::from_i64(900000000)
    ).await?;
    
    // 5. Final close: buy 2 units at $85
    position_manager.update_position(
        sample_symbol, Side::Buy, Qty::from_i64(20000), Px::from_i64(850000000)
    ).await?;
    
    let position = position_manager.get_position(sample_symbol).await.unwrap();
    
    // Final position should be flat
    assert_eq!(position.quantity, 0);
    assert_eq!(position.unrealized_pnl, 0);
    
    // Should have realized P&L from all the trades
    assert_ne!(position.realized_pnl, 0);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_large_numbers_precision(
    position_manager: PositionManager
) -> Result<()> {
    let symbol = Symbol(100);
    
    // Test with large quantities and prices
    let large_qty = Qty::from_i64(1000000000); // 100k units
    let large_price = Px::from_i64(500000000000); // $50k per unit
    
    position_manager.update_position(symbol, Side::Buy, large_qty, large_price).await?;
    
    let position = position_manager.get_position(symbol).await.unwrap();
    
    // Verify no overflow or precision loss
    assert_eq!(position.quantity, 1000000000);
    assert_eq!(position.avg_entry_price, 500000000000);
    
    // Update market price and verify P&L calculation
    let new_price = Px::from_i64(510000000000); // +$1k per unit
    position_manager.update_market_price(symbol, new_price).await;
    
    let updated_position = position_manager.get_position(symbol).await.unwrap();
    
    // P&L should be calculated correctly even with large numbers
    let expected_pnl = (510000000000 - 500000000000) * 1000000000 / 10000;
    assert_eq!(updated_position.unrealized_pnl, expected_pnl);
    
    Ok(())
}