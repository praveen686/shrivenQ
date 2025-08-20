//! Unit tests for PortfolioTracker functionality

use rstest::*;
use backtesting::*;
use chrono::{Utc, Duration};
use crate::test_utils::*;

#[rstest]
fn test_portfolio_tracker_creation() {
    let initial_capital = 100_000.0;
    let portfolio = PortfolioTracker::new(initial_capital);
    
    let state = portfolio.get_current_state();
    assert_eq!(state.cash, initial_capital);
    assert_eq!(state.positions.len(), 0);
    assert_eq!(state.total_value, initial_capital);
}

#[rstest]
#[case(50_000.0)]
#[case(100_000.0)]
#[case(1_000_000.0)]
fn test_portfolio_creation_with_different_capitals(#[case] capital: f64) {
    let portfolio = PortfolioTracker::new(capital);
    let state = portfolio.get_current_state();
    
    assert_eq!(state.cash, capital);
    assert_eq!(state.total_value, capital);
    TestAssertions::assert_portfolio_valid(&state);
}

#[rstest]
fn test_process_buy_fill() {
    let portfolio = PortfolioTracker::new(100_000.0);
    
    let fill = Fill {
        order_id: "test_buy_1".to_string(),
        symbol: "AAPL".to_string(),
        side: OrderSide::Buy,
        quantity: 100.0,
        price: 150.0,
        commission: 5.0,
        slippage: 2.0,
        timestamp: Utc::now(),
    };
    
    let result = portfolio.process_fill(&fill);
    assert!(result.is_ok(), "Should process buy fill successfully");
    
    let state = portfolio.get_current_state();
    
    // Check cash reduction
    let expected_cash = 100_000.0 - (150.0 * 100.0 + 5.0 + 2.0);
    TestAssertions::assert_approx_eq(state.cash, expected_cash, 0.01);
    
    // Check position creation
    assert_eq!(state.positions.len(), 1);
    let position = &state.positions[0];
    assert_eq!(position.symbol, "AAPL");
    assert_eq!(position.quantity, 100.0);
    assert_eq!(position.average_price, 150.0);
    assert_eq!(position.commission_paid, 5.0);
    
    TestAssertions::assert_portfolio_valid(&state);
}

#[rstest]
fn test_process_sell_fill() {
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // First buy to establish position
    let buy_fill = Fill {
        order_id: "buy_1".to_string(),
        symbol: "AAPL".to_string(),
        side: OrderSide::Buy,
        quantity: 100.0,
        price: 150.0,
        commission: 5.0,
        slippage: 2.0,
        timestamp: Utc::now(),
    };
    portfolio.process_fill(&buy_fill).unwrap();
    
    // Then sell part of position
    let sell_fill = Fill {
        order_id: "sell_1".to_string(),
        symbol: "AAPL".to_string(),
        side: OrderSide::Sell,
        quantity: 50.0,
        price: 160.0,
        commission: 3.0,
        slippage: 1.0,
        timestamp: Utc::now(),
    };
    
    let result = portfolio.process_fill(&sell_fill);
    assert!(result.is_ok(), "Should process sell fill successfully");
    
    let state = portfolio.get_current_state();
    
    // Check position reduction
    assert_eq!(state.positions.len(), 1);
    let position = &state.positions[0];
    assert_eq!(position.quantity, 50.0); // 100 - 50
    assert_eq!(position.average_price, 150.0); // Should remain same
    
    // Check realized P&L
    let expected_realized_pnl = (160.0 - 150.0) * 50.0; // $500 profit
    TestAssertions::assert_approx_eq(position.realized_pnl, expected_realized_pnl, 0.01);
    
    TestAssertions::assert_portfolio_valid(&state);
}

#[rstest]
fn test_complete_position_close() {
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Buy position
    let buy_fill = Fill {
        order_id: "buy_1".to_string(),
        symbol: "TSLA".to_string(),
        side: OrderSide::Buy,
        quantity: 50.0,
        price: 200.0,
        commission: 5.0,
        slippage: 1.0,
        timestamp: Utc::now(),
    };
    portfolio.process_fill(&buy_fill).unwrap();
    
    // Sell entire position
    let sell_fill = Fill {
        order_id: "sell_1".to_string(),
        symbol: "TSLA".to_string(),
        side: OrderSide::Sell,
        quantity: 50.0,
        price: 220.0,
        commission: 5.0,
        slippage: 1.0,
        timestamp: Utc::now(),
    };
    portfolio.process_fill(&sell_fill).unwrap();
    
    let state = portfolio.get_current_state();
    
    // Position should be removed when quantity reaches zero
    let has_position = state.positions.iter().any(|p| p.symbol == "TSLA");
    assert!(!has_position, "Position should be closed and removed");
    
    TestAssertions::assert_portfolio_valid(&state);
}

#[rstest]
fn test_multiple_buy_fills_average_price() {
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // First buy
    let buy1 = Fill {
        order_id: "buy_1".to_string(),
        symbol: "GOOGL".to_string(),
        side: OrderSide::Buy,
        quantity: 10.0,
        price: 2800.0,
        commission: 10.0,
        slippage: 5.0,
        timestamp: Utc::now(),
    };
    portfolio.process_fill(&buy1).unwrap();
    
    // Second buy at different price
    let buy2 = Fill {
        order_id: "buy_2".to_string(),
        symbol: "GOOGL".to_string(),
        side: OrderSide::Buy,
        quantity: 20.0,
        price: 2900.0,
        commission: 15.0,
        slippage: 8.0,
        timestamp: Utc::now(),
    };
    portfolio.process_fill(&buy2).unwrap();
    
    let state = portfolio.get_current_state();
    let position = state.positions.iter().find(|p| p.symbol == "GOOGL").unwrap();
    
    // Check total quantity
    assert_eq!(position.quantity, 30.0);
    
    // Check average price calculation: (10 * 2800 + 20 * 2900) / 30
    let expected_avg_price = (10.0 * 2800.0 + 20.0 * 2900.0) / 30.0;
    TestAssertions::assert_approx_eq(position.average_price, expected_avg_price, 0.01);
    
    // Check total commission
    TestAssertions::assert_approx_eq(position.commission_paid, 25.0, 0.01);
    
    TestAssertions::assert_portfolio_valid(&state);
}

#[rstest]
fn test_insufficient_funds_buy() {
    let portfolio = PortfolioTracker::new(1000.0); // Small initial capital
    
    // Try to buy more than we can afford
    let large_buy = Fill {
        order_id: "large_buy".to_string(),
        symbol: "EXPENSIVE".to_string(),
        side: OrderSide::Buy,
        quantity: 100.0,
        price: 50.0, // $5000 + commission/slippage > $1000 capital
        commission: 100.0,
        slippage: 50.0,
        timestamp: Utc::now(),
    };
    
    let result = portfolio.process_fill(&large_buy);
    assert!(result.is_err(), "Should reject insufficient funds trade");
    
    let state = portfolio.get_current_state();
    assert_eq!(state.cash, 1000.0); // Cash should be unchanged
    assert_eq!(state.positions.len(), 0); // No position should be created
}

#[rstest]
fn test_update_prices_unrealized_pnl() {
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Establish position
    let buy_fill = Fill {
        order_id: "buy_1".to_string(),
        symbol: "AAPL".to_string(),
        side: OrderSide::Buy,
        quantity: 100.0,
        price: 150.0,
        commission: 5.0,
        slippage: 2.0,
        timestamp: Utc::now(),
    };
    portfolio.process_fill(&buy_fill).unwrap();
    
    // Update prices - price went up
    let market = MarketSnapshotBuilder::new()
        .with_price("AAPL", 160.0)
        .build();
    
    portfolio.update_prices(&market).unwrap();
    
    let state = portfolio.get_current_state();
    let position = state.positions.iter().find(|p| p.symbol == "AAPL").unwrap();
    
    assert_eq!(position.current_price, 160.0);
    let expected_unrealized_pnl = (160.0 - 150.0) * 100.0; // $1000 profit
    TestAssertions::assert_approx_eq(position.unrealized_pnl, expected_unrealized_pnl, 0.01);
    
    TestAssertions::assert_portfolio_valid(&state);
}

#[rstest]
fn test_update_prices_loss_scenario() {
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Establish position
    let buy_fill = Fill {
        order_id: "buy_1".to_string(),
        symbol: "VOLATILE".to_string(),
        side: OrderSide::Buy,
        quantity: 200.0,
        price: 100.0,
        commission: 10.0,
        slippage: 5.0,
        timestamp: Utc::now(),
    };
    portfolio.process_fill(&buy_fill).unwrap();
    
    // Update prices - price went down
    let market = MarketSnapshotBuilder::new()
        .with_price("VOLATILE", 80.0)
        .build();
    
    portfolio.update_prices(&market).unwrap();
    
    let state = portfolio.get_current_state();
    let position = state.positions.iter().find(|p| p.symbol == "VOLATILE").unwrap();
    
    assert_eq!(position.current_price, 80.0);
    let expected_unrealized_pnl = (80.0 - 100.0) * 200.0; // -$4000 loss
    TestAssertions::assert_approx_eq(position.unrealized_pnl, expected_unrealized_pnl, 0.01);
    
    TestAssertions::assert_portfolio_valid(&state);
}

#[rstest]
fn test_record_equity_curve() {
    let portfolio = PortfolioTracker::new(50_000.0);
    
    // Record initial equity
    let time1 = Utc::now();
    portfolio.record_equity(time1).unwrap();
    
    // Make a trade and record again
    let buy_fill = Fill {
        order_id: "buy_1".to_string(),
        symbol: "TEST".to_string(),
        side: OrderSide::Buy,
        quantity: 100.0,
        price: 100.0,
        commission: 5.0,
        slippage: 2.0,
        timestamp: Utc::now(),
    };
    portfolio.process_fill(&buy_fill).unwrap();
    
    let time2 = time1 + Duration::hours(1);
    portfolio.record_equity(time2).unwrap();
    
    // Update price and record again
    let market = MarketSnapshotBuilder::new()
        .with_price("TEST", 110.0)
        .build();
    portfolio.update_prices(&market).unwrap();
    
    let time3 = time2 + Duration::hours(1);
    portfolio.record_equity(time3).unwrap();
    
    let equity_curve = portfolio.get_equity_curve();
    
    assert_eq!(equity_curve.len(), 3);
    
    // First point should be initial capital
    assert_eq!(equity_curve[0].0, time1);
    TestAssertions::assert_approx_eq(equity_curve[0].1, 50_000.0, 0.01);
    
    // Second point should reflect trade costs
    assert_eq!(equity_curve[1].0, time2);
    let expected_value_2 = 50_000.0 - 5.0 - 2.0; // After commission and slippage
    TestAssertions::assert_approx_eq(equity_curve[1].1, expected_value_2, 0.01);
    
    // Third point should reflect price appreciation
    assert_eq!(equity_curve[2].0, time3);
    let cash_after_trade = 50_000.0 - 100.0 * 100.0 - 5.0 - 2.0;
    let position_value = 100.0 * 110.0;
    let expected_value_3 = cash_after_trade + position_value;
    TestAssertions::assert_approx_eq(equity_curve[2].1, expected_value_3, 0.01);
}

#[rstest]
fn test_multiple_symbols_portfolio() {
    let portfolio = PortfolioTracker::new(200_000.0);
    
    let symbols_and_prices = vec![
        ("AAPL", 150.0, 100.0),
        ("GOOGL", 2800.0, 10.0),
        ("TSLA", 200.0, 50.0),
        ("MSFT", 300.0, 66.0),
    ];
    
    // Buy positions in multiple symbols
    for (symbol, price, quantity) in &symbols_and_prices {
        let fill = Fill {
            order_id: format!("buy_{}", symbol),
            symbol: symbol.to_string(),
            side: OrderSide::Buy,
            quantity: *quantity,
            price: *price,
            commission: 10.0,
            slippage: 5.0,
            timestamp: Utc::now(),
        };
        portfolio.process_fill(&fill).unwrap();
    }
    
    let state = portfolio.get_current_state();
    
    // Should have 4 positions
    assert_eq!(state.positions.len(), 4);
    
    // Verify each position
    for (symbol, price, quantity) in &symbols_and_prices {
        let position = state.positions.iter()
            .find(|p| p.symbol == *symbol)
            .unwrap();
        
        assert_eq!(position.quantity, *quantity);
        assert_eq!(position.average_price, *price);
        TestAssertions::assert_approx_eq(position.commission_paid, 10.0, 0.01);
    }
    
    TestAssertions::assert_portfolio_valid(&state);
}

#[rstest]
fn test_portfolio_total_value_calculation() {
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Buy two different stocks
    let fills = vec![
        Fill {
            order_id: "buy_1".to_string(),
            symbol: "STOCK1".to_string(),
            side: OrderSide::Buy,
            quantity: 100.0,
            price: 50.0,
            commission: 5.0,
            slippage: 2.0,
            timestamp: Utc::now(),
        },
        Fill {
            order_id: "buy_2".to_string(),
            symbol: "STOCK2".to_string(),
            side: OrderSide::Buy,
            quantity: 200.0,
            price: 25.0,
            commission: 7.0,
            slippage: 3.0,
            timestamp: Utc::now(),
        },
    ];
    
    for fill in fills {
        portfolio.process_fill(&fill).unwrap();
    }
    
    // Update prices
    let market = MarketSnapshotBuilder::new()
        .with_price("STOCK1", 60.0) // +$10/share * 100 = +$1000
        .with_price("STOCK2", 20.0) // -$5/share * 200 = -$1000
        .build();
    
    portfolio.update_prices(&market).unwrap();
    
    let state = portfolio.get_current_state();
    
    // Calculate expected values
    let cash_spent = 100.0 * 50.0 + 5.0 + 2.0 + 200.0 * 25.0 + 7.0 + 3.0;
    let remaining_cash = 100_000.0 - cash_spent;
    let position1_value = 100.0 * 60.0;
    let position2_value = 200.0 * 20.0;
    let expected_total = remaining_cash + position1_value + position2_value;
    
    TestAssertions::assert_approx_eq(state.total_value, expected_total, 0.01);
    TestAssertions::assert_portfolio_valid(&state);
}

#[rstest]
fn test_sell_without_position() {
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Try to sell without having a position (short selling not implemented)
    let sell_fill = Fill {
        order_id: "sell_1".to_string(),
        symbol: "NONEXISTENT".to_string(),
        side: OrderSide::Sell,
        quantity: 100.0,
        price: 150.0,
        commission: 5.0,
        slippage: 2.0,
        timestamp: Utc::now(),
    };
    
    // This should complete without error (portfolio handles missing positions gracefully)
    let result = portfolio.process_fill(&sell_fill);
    assert!(result.is_ok(), "Portfolio should handle sell of non-existent position");
    
    let state = portfolio.get_current_state();
    // Cash should increase from the sale proceeds
    let expected_cash = 100_000.0 + 150.0 * 100.0 - 5.0 - 2.0;
    TestAssertions::assert_approx_eq(state.cash, expected_cash, 0.01);
}

#[rstest]
fn test_position_partial_sales() {
    let portfolio = PortfolioTracker::new(100_000.0);
    
    // Buy 1000 shares
    let buy_fill = Fill {
        order_id: "buy_1".to_string(),
        symbol: "PARTIAL".to_string(),
        side: OrderSide::Buy,
        quantity: 1000.0,
        price: 10.0,
        commission: 10.0,
        slippage: 5.0,
        timestamp: Utc::now(),
    };
    portfolio.process_fill(&buy_fill).unwrap();
    
    // Sell in multiple batches
    let sell_quantities = vec![200.0, 300.0, 100.0];
    
    for (i, quantity) in sell_quantities.iter().enumerate() {
        let sell_fill = Fill {
            order_id: format!("sell_{}", i),
            symbol: "PARTIAL".to_string(),
            side: OrderSide::Sell,
            quantity: *quantity,
            price: 12.0,
            commission: 5.0,
            slippage: 2.0,
            timestamp: Utc::now(),
        };
        portfolio.process_fill(&sell_fill).unwrap();
    }
    
    let state = portfolio.get_current_state();
    let position = state.positions.iter().find(|p| p.symbol == "PARTIAL").unwrap();
    
    // Should have 400 shares remaining (1000 - 200 - 300 - 100)
    assert_eq!(position.quantity, 400.0);
    
    // Realized P&L should reflect the sold quantities
    let total_sold = sell_quantities.iter().sum::<f64>();
    let expected_realized_pnl = (12.0 - 10.0) * total_sold; // $2/share profit
    TestAssertions::assert_approx_eq(position.realized_pnl, expected_realized_pnl, 0.01);
    
    TestAssertions::assert_portfolio_valid(&state);
}

#[rstest]
fn test_final_state_consistency() {
    let portfolio = PortfolioTracker::new(75_000.0);
    
    // Record some equity points
    portfolio.record_equity(Utc::now()).unwrap();
    
    // Make trades
    let buy_fill = Fill {
        order_id: "final_test".to_string(),
        symbol: "FINAL".to_string(),
        side: OrderSide::Buy,
        quantity: 500.0,
        price: 20.0,
        commission: 15.0,
        slippage: 8.0,
        timestamp: Utc::now(),
    };
    portfolio.process_fill(&buy_fill).unwrap();
    
    portfolio.record_equity(Utc::now()).unwrap();
    
    // Get both current and final state
    let current_state = portfolio.get_current_state();
    let final_state = portfolio.get_final_state();
    
    // They should be identical
    assert_eq!(current_state.cash, final_state.cash);
    assert_eq!(current_state.positions.len(), final_state.positions.len());
    assert_eq!(current_state.total_value, final_state.total_value);
    
    TestAssertions::assert_portfolio_valid(&current_state);
    TestAssertions::assert_portfolio_valid(&final_state);
}

#[rstest]
fn test_edge_case_zero_quantity_fill() {
    let portfolio = PortfolioTracker::new(100_000.0);
    
    let zero_fill = Fill {
        order_id: "zero_fill".to_string(),
        symbol: "ZERO".to_string(),
        side: OrderSide::Buy,
        quantity: 0.0,
        price: 100.0,
        commission: 0.0,
        slippage: 0.0,
        timestamp: Utc::now(),
    };
    
    // Should handle zero quantity gracefully
    let result = portfolio.process_fill(&zero_fill);
    assert!(result.is_ok(), "Should handle zero quantity fill");
    
    let state = portfolio.get_current_state();
    assert_eq!(state.cash, 100_000.0); // No change
    assert_eq!(state.positions.len(), 0); // No position created
}

#[rstest]
fn test_very_small_quantities() {
    let portfolio = PortfolioTracker::new(100_000.0);
    
    let small_fill = Fill {
        order_id: "small_fill".to_string(),
        symbol: "SMALL".to_string(),
        side: OrderSide::Buy,
        quantity: 0.001,
        price: 1000.0,
        commission: 0.01,
        slippage: 0.005,
        timestamp: Utc::now(),
    };
    
    let result = portfolio.process_fill(&small_fill);
    assert!(result.is_ok(), "Should handle very small quantities");
    
    let state = portfolio.get_current_state();
    TestAssertions::assert_portfolio_valid(&state);
    
    if let Some(position) = state.positions.iter().find(|p| p.symbol == "SMALL") {
        assert_eq!(position.quantity, 0.001);
    }
}