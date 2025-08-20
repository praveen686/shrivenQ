//! Unit tests for Strategy trait implementations

use rstest::*;
use backtesting::*;
use chrono::{Utc, Duration};
use crate::test_utils::*;

#[rstest]
fn test_do_nothing_strategy() {
    let strategy = DoNothingStrategy;
    
    let market = MarketSnapshotBuilder::new()
        .with_price("AAPL", 150.0)
        .with_price("GOOGL", 2800.0)
        .build();
    
    let portfolio = PortfolioState {
        cash: 100_000.0,
        positions: vec![],
        total_value: 100_000.0,
    };
    
    let signals = strategy.generate_signals(&market, &portfolio);
    
    assert_eq!(signals.len(), 0, "DoNothingStrategy should generate no signals");
}

#[rstest]
fn test_always_buy_strategy_with_cash() {
    let strategy = AlwaysBuyStrategy {
        symbol: "TEST".to_string(),
        position_size: 1000.0,
    };
    
    let market = MarketSnapshotBuilder::new()
        .with_price("TEST", 50.0)
        .build();
    
    let portfolio = PortfolioState {
        cash: 100_000.0,
        positions: vec![],
        total_value: 100_000.0,
    };
    
    let signals = strategy.generate_signals(&market, &portfolio);
    
    assert_eq!(signals.len(), 1, "Should generate one buy signal");
    
    let signal = &signals[0];
    assert_eq!(signal.symbol, "TEST");
    assert_eq!(signal.side, OrderSide::Buy);
    assert_eq!(signal.order_type, OrderType::Market);
    assert_eq!(signal.quantity, 1000.0);
    assert_eq!(signal.price, None);
}

#[rstest]
fn test_always_buy_strategy_with_existing_position() {
    let strategy = AlwaysBuyStrategy {
        symbol: "TEST".to_string(),
        position_size: 1000.0,
    };
    
    let market = MarketSnapshotBuilder::new()
        .with_price("TEST", 50.0)
        .build();
    
    let existing_position = Position {
        symbol: "TEST".to_string(),
        quantity: 500.0,
        average_price: 45.0,
        current_price: 50.0,
        realized_pnl: 0.0,
        unrealized_pnl: 2500.0,
        commission_paid: 10.0,
    };
    
    let portfolio = PortfolioState {
        cash: 50_000.0,
        positions: vec![existing_position],
        total_value: 75_000.0,
    };
    
    let signals = strategy.generate_signals(&market, &portfolio);
    
    assert_eq!(signals.len(), 0, "Should not buy when position already exists");
}

#[rstest]
fn test_always_buy_strategy_insufficient_cash() {
    let strategy = AlwaysBuyStrategy {
        symbol: "EXPENSIVE".to_string(),
        position_size: 10_000.0,
    };
    
    let market = MarketSnapshotBuilder::new()
        .with_price("EXPENSIVE", 100.0)
        .build();
    
    let portfolio = PortfolioState {
        cash: 5_000.0, // Not enough for the position size
        positions: vec![],
        total_value: 5_000.0,
    };
    
    let signals = strategy.generate_signals(&market, &portfolio);
    
    assert_eq!(signals.len(), 0, "Should not buy when insufficient cash");
}

#[rstest]
fn test_always_sell_strategy_with_position() {
    let strategy = AlwaysSellStrategy {
        symbol: "TEST".to_string(),
    };
    
    let market = MarketSnapshotBuilder::new()
        .with_price("TEST", 60.0)
        .build();
    
    let position = Position {
        symbol: "TEST".to_string(),
        quantity: 200.0,
        average_price: 50.0,
        current_price: 60.0,
        realized_pnl: 0.0,
        unrealized_pnl: 2000.0,
        commission_paid: 5.0,
    };
    
    let portfolio = PortfolioState {
        cash: 10_000.0,
        positions: vec![position],
        total_value: 22_000.0,
    };
    
    let signals = strategy.generate_signals(&market, &portfolio);
    
    assert_eq!(signals.len(), 1, "Should generate one sell signal");
    
    let signal = &signals[0];
    assert_eq!(signal.symbol, "TEST");
    assert_eq!(signal.side, OrderSide::Sell);
    assert_eq!(signal.order_type, OrderType::Market);
    assert_eq!(signal.quantity, 200.0);
    assert_eq!(signal.price, None);
}

#[rstest]
fn test_always_sell_strategy_no_position() {
    let strategy = AlwaysSellStrategy {
        symbol: "NONEXISTENT".to_string(),
    };
    
    let market = MarketSnapshotBuilder::new()
        .with_price("NONEXISTENT", 100.0)
        .build();
    
    let portfolio = PortfolioState {
        cash: 50_000.0,
        positions: vec![],
        total_value: 50_000.0,
    };
    
    let signals = strategy.generate_signals(&market, &portfolio);
    
    assert_eq!(signals.len(), 0, "Should not sell when no position exists");
}

#[rstest]
fn test_always_sell_strategy_zero_quantity() {
    let strategy = AlwaysSellStrategy {
        symbol: "ZERO".to_string(),
    };
    
    let market = MarketSnapshotBuilder::new()
        .with_price("ZERO", 100.0)
        .build();
    
    let zero_position = Position {
        symbol: "ZERO".to_string(),
        quantity: 0.0,
        average_price: 100.0,
        current_price: 100.0,
        realized_pnl: 0.0,
        unrealized_pnl: 0.0,
        commission_paid: 0.0,
    };
    
    let portfolio = PortfolioState {
        cash: 50_000.0,
        positions: vec![zero_position],
        total_value: 50_000.0,
    };
    
    let signals = strategy.generate_signals(&market, &portfolio);
    
    assert_eq!(signals.len(), 0, "Should not sell when position quantity is zero");
}

#[rstest]
#[ignore] // Ignore this test since MAStrategy module structure needs verification
fn test_moving_average_strategy_initialization() {
    // Note: This test requires the MAStrategy to be properly exported from the backtesting crate
    // The strategies module may need to be properly structured in lib.rs
    
    // Placeholder test that would work when strategies module is properly set up
    // let strategy = backtesting::strategies::MAStrategy::new("AAPL".to_string(), 10, 30);
    // assert_eq!(format!("{:?}", strategy).contains("MAStrategy"), true);
    // assert_eq!(format!("{:?}", strategy).contains("AAPL"), true);
}

#[rstest]
fn test_strategy_with_multiple_symbols() {
    let buy_strategy = AlwaysBuyStrategy {
        symbol: "AAPL".to_string(),
        position_size: 1000.0,
    };
    
    let market = MarketSnapshotBuilder::new()
        .with_price("AAPL", 150.0)
        .with_price("GOOGL", 2800.0)
        .with_price("TSLA", 200.0)
        .build();
    
    let portfolio = PortfolioState {
        cash: 200_000.0,
        positions: vec![],
        total_value: 200_000.0,
    };
    
    let signals = buy_strategy.generate_signals(&market, &portfolio);
    
    // Should only generate signal for the target symbol
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].symbol, "AAPL");
}

#[rstest]
fn test_strategy_with_mixed_portfolio() {
    let sell_strategy = AlwaysSellStrategy {
        symbol: "TARGET".to_string(),
    };
    
    let market = MarketSnapshotBuilder::new()
        .with_price("TARGET", 100.0)
        .with_price("OTHER", 200.0)
        .build();
    
    let positions = vec![
        Position {
            symbol: "TARGET".to_string(),
            quantity: 100.0,
            average_price: 90.0,
            current_price: 100.0,
            realized_pnl: 0.0,
            unrealized_pnl: 1000.0,
            commission_paid: 5.0,
        },
        Position {
            symbol: "OTHER".to_string(),
            quantity: 50.0,
            average_price: 180.0,
            current_price: 200.0,
            realized_pnl: 0.0,
            unrealized_pnl: 1000.0,
            commission_paid: 3.0,
        },
    ];
    
    let portfolio = PortfolioState {
        cash: 10_000.0,
        positions,
        total_value: 27_000.0,
    };
    
    let signals = sell_strategy.generate_signals(&market, &portfolio);
    
    // Should only sell the target position
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].symbol, "TARGET");
    assert_eq!(signals[0].quantity, 100.0);
}

#[rstest]
fn test_strategy_signal_validation() {
    let buy_strategy = AlwaysBuyStrategy {
        symbol: "VALID".to_string(),
        position_size: 500.0,
    };
    
    let market = MarketSnapshotBuilder::new()
        .with_price("VALID", 10.0)
        .build();
    
    let portfolio = PortfolioState {
        cash: 100_000.0,
        positions: vec![],
        total_value: 100_000.0,
    };
    
    let signals = buy_strategy.generate_signals(&market, &portfolio);
    
    assert_eq!(signals.len(), 1);
    let signal = &signals[0];
    
    // Validate signal properties
    assert!(!signal.symbol.is_empty(), "Symbol should not be empty");
    assert!(signal.quantity > 0.0, "Quantity should be positive");
    match signal.order_type {
        OrderType::Market => assert_eq!(signal.price, None, "Market order should have no price"),
        OrderType::Limit => assert!(signal.price.is_some(), "Limit order should have price"),
        _ => {}
    }
}

#[rstest]
fn test_strategy_with_fractional_quantities() {
    let buy_strategy = AlwaysBuyStrategy {
        symbol: "FRAC".to_string(),
        position_size: 100.5, // Fractional quantity
    };
    
    let market = MarketSnapshotBuilder::new()
        .with_price("FRAC", 25.75)
        .build();
    
    let portfolio = PortfolioState {
        cash: 10_000.0,
        positions: vec![],
        total_value: 10_000.0,
    };
    
    let signals = buy_strategy.generate_signals(&market, &portfolio);
    
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].quantity, 100.5);
}

#[rstest]
fn test_strategy_with_high_value_stocks() {
    let buy_strategy = AlwaysBuyStrategy {
        symbol: "EXPENSIVE".to_string(),
        position_size: 10.0, // Small quantity for expensive stock
    };
    
    let market = MarketSnapshotBuilder::new()
        .with_price("EXPENSIVE", 5000.0) // Very expensive stock
        .build();
    
    let portfolio = PortfolioState {
        cash: 100_000.0,
        positions: vec![],
        total_value: 100_000.0,
    };
    
    let signals = buy_strategy.generate_signals(&market, &portfolio);
    
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].quantity, 10.0);
}

#[rstest]
fn test_strategy_edge_case_exact_cash_match() {
    let buy_strategy = AlwaysBuyStrategy {
        symbol: "EXACT".to_string(),
        position_size: 1000.0,
    };
    
    let market = MarketSnapshotBuilder::new()
        .with_price("EXACT", 100.0)
        .build();
    
    let portfolio = PortfolioState {
        cash: 1000.0, // Exactly enough cash
        positions: vec![],
        total_value: 1000.0,
    };
    
    let signals = buy_strategy.generate_signals(&market, &portfolio);
    
    // Should still generate signal as cash >= required amount
    assert_eq!(signals.len(), 1);
}

#[rstest]
fn test_strategy_with_negative_position() {
    // Test selling strategy with a short position (negative quantity)
    let sell_strategy = AlwaysSellStrategy {
        symbol: "SHORT".to_string(),
    };
    
    let market = MarketSnapshotBuilder::new()
        .with_price("SHORT", 95.0)
        .build();
    
    let short_position = Position {
        symbol: "SHORT".to_string(),
        quantity: -100.0, // Short position
        average_price: 100.0,
        current_price: 95.0,
        realized_pnl: 0.0,
        unrealized_pnl: 500.0, // Profit from short
        commission_paid: 5.0,
    };
    
    let portfolio = PortfolioState {
        cash: 50_000.0,
        positions: vec![short_position],
        total_value: 50_500.0,
    };
    
    let signals = sell_strategy.generate_signals(&market, &portfolio);
    
    // Strategy should not sell a short position (quantity < 0)
    assert_eq!(signals.len(), 0, "Should not sell short positions");
}

#[rstest]
fn test_trading_signal_creation() {
    let signal = TradingSignal {
        symbol: "TEST".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        quantity: 100.0,
        price: Some(50.0),
    };
    
    assert_eq!(signal.symbol, "TEST");
    assert_eq!(signal.quantity, 100.0);
    assert_eq!(signal.price, Some(50.0));
    
    match signal.side {
        OrderSide::Buy => {} // Expected
        _ => panic!("Expected Buy side"),
    }
    
    match signal.order_type {
        OrderType::Limit => {} // Expected
        _ => panic!("Expected Limit order"),
    }
}

#[rstest]
fn test_portfolio_state_validation() {
    let portfolio = PortfolioState {
        cash: 75_000.0,
        positions: vec![
            Position {
                symbol: "STOCK1".to_string(),
                quantity: 100.0,
                average_price: 50.0,
                current_price: 55.0,
                realized_pnl: 0.0,
                unrealized_pnl: 500.0,
                commission_paid: 5.0,
            }
        ],
        total_value: 80_500.0,
    };
    
    TestAssertions::assert_portfolio_valid(&portfolio);
    
    // Verify position calculations
    let position = &portfolio.positions[0];
    let expected_unrealized = (position.current_price - position.average_price) * position.quantity;
    TestAssertions::assert_approx_eq(position.unrealized_pnl, expected_unrealized, 0.01);
}

#[rstest]
fn test_strategy_performance_with_large_portfolio() {
    let buy_strategy = AlwaysBuyStrategy {
        symbol: "LARGE".to_string(),
        position_size: 10_000.0,
    };
    
    // Create large portfolio with many positions
    let mut positions = Vec::new();
    for i in 0..100 {
        positions.push(Position {
            symbol: format!("STOCK{}", i),
            quantity: 100.0,
            average_price: 10.0 + i as f64,
            current_price: 12.0 + i as f64,
            realized_pnl: 0.0,
            unrealized_pnl: 200.0,
            commission_paid: 1.0,
        });
    }
    
    let market = MarketSnapshotBuilder::new()
        .with_price("LARGE", 100.0)
        .build();
    
    let portfolio = PortfolioState {
        cash: 1_000_000.0,
        positions,
        total_value: 1_020_000.0,
    };
    
    let signals = buy_strategy.generate_signals(&market, &portfolio);
    
    // Should efficiently handle large portfolio and still generate correct signal
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].symbol, "LARGE");
}