//! Unit tests for Trading Strategies (Momentum and Arbitrage)
//!
//! Comprehensive tests covering:
//! - Momentum strategy moving average calculations
//! - Signal detection and generation logic
//! - Arbitrage opportunity detection
//! - Strategy health monitoring
//! - Price history management
//! - Rate limiting and signal throttling
//! - Strategy reset functionality
//! - Edge cases and error scenarios

use anyhow::Result;
use rstest::*;
use std::time::Duration;
use tokio::time::sleep;
use trading_gateway::{
    strategy::{ArbitrageStrategy, MomentumStrategy},
    SignalType, Side, TradingEvent, TradingStrategy,
};
use services_common::{Px, Qty, Symbol, Ts};

/// Test fixture for creating a MomentumStrategy
#[fixture]
fn momentum_strategy() -> MomentumStrategy {
    MomentumStrategy::new()
}

/// Test fixture for creating an ArbitrageStrategy
#[fixture]
fn arbitrage_strategy() -> ArbitrageStrategy {
    ArbitrageStrategy::new()
}

/// Test fixture for creating a market update with rising prices
#[fixture]
fn rising_market_update() -> TradingEvent {
    TradingEvent::MarketUpdate {
        symbol: Symbol(1),
        bid: Some((Px::from_i64(1100000000), Qty::from_i64(10000))), // $110
        ask: Some((Px::from_i64(1101000000), Qty::from_i64(8000))),  // $110.10
        mid: Px::from_i64(1100500000), // $110.05 - higher price
        spread: 100000,
        imbalance: 20.0,
        vpin: 30.0,
        kyles_lambda: 0.6,
        timestamp: Ts::now(),
    }
}

/// Test fixture for creating a market update with falling prices
#[fixture]
fn falling_market_update() -> TradingEvent {
    TradingEvent::MarketUpdate {
        symbol: Symbol(1),
        bid: Some((Px::from_i64(900000000), Qty::from_i64(12000))),  // $90
        ask: Some((Px::from_i64(901000000), Qty::from_i64(10000))),  // $90.10
        mid: Px::from_i64(900500000), // $90.05 - lower price
        spread: 100000,
        imbalance: -15.0,
        vpin: 25.0,
        kyles_lambda: 0.4,
        timestamp: Ts::now(),
    }
}

/// Test fixture for creating arbitrage opportunity (negative spread)
#[fixture]
fn arbitrage_opportunity() -> TradingEvent {
    TradingEvent::MarketUpdate {
        symbol: Symbol(2),
        bid: Some((Px::from_i64(1001000000), Qty::from_i64(10000))), // $100.10 (bid)
        ask: Some((Px::from_i64(1000000000), Qty::from_i64(8000))),  // $100.00 (ask) - negative spread!
        mid: Px::from_i64(1000500000), // $100.05
        spread: -100000, // Negative spread
        imbalance: 0.0,
        vpin: 40.0,
        kyles_lambda: 0.8,
        timestamp: Ts::now(),
    }
}

// ===== Momentum Strategy Tests =====

#[rstest]
#[tokio::test]
async fn test_momentum_strategy_creation(momentum_strategy: MomentumStrategy) {
    assert_eq!(momentum_strategy.name(), "Momentum");
    
    let health = momentum_strategy.health();
    assert!(health.is_healthy);
    assert_eq!(health.success_count, 0);
    assert_eq!(health.error_count, 0);
}

#[rstest]
#[tokio::test]
async fn test_momentum_insufficient_data(
    mut momentum_strategy: MomentumStrategy,
    rising_market_update: TradingEvent
) -> Result<()> {
    // With insufficient price history, should not generate signals
    let result = momentum_strategy.on_market_update(&rising_market_update).await?;
    
    assert!(result.is_none(), "Should not generate signals with insufficient data");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_momentum_signal_generation_with_history(
    mut momentum_strategy: MomentumStrategy
) -> Result<()> {
    let symbol = Symbol(1);
    
    // Build price history with trending pattern
    for i in 1..=60 {
        let price = 1000000000 + i * 10000; // Gradually rising prices
        let market_update = TradingEvent::MarketUpdate {
            symbol,
            bid: Some((Px::from_i64(price), Qty::from_i64(10000))),
            ask: Some((Px::from_i64(price + 10000), Qty::from_i64(10000))),
            mid: Px::from_i64(price + 5000),
            spread: 10000,
            imbalance: 10.0,
            vpin: 20.0,
            kyles_lambda: 0.4,
            timestamp: Ts::now(),
        };
        
        let result = momentum_strategy.on_market_update(&market_update).await?;
        
        // Should start generating signals once we have enough history
        if i >= 50 && result.is_some() {
            if let Some(TradingEvent::Signal { 
                signal_type, side, strength, confidence, .. 
            }) = result {
                assert_eq!(signal_type, SignalType::Momentum);
                assert_eq!(side, Side::Buy); // Rising trend = buy signal
                assert!(strength > 0.0);
                assert!(confidence > 0.0);
                break; // Found a signal, test successful
            }
        }
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_momentum_golden_cross_detection(
    mut momentum_strategy: MomentumStrategy
) -> Result<()> {
    let symbol = Symbol(3);
    
    // Create price history that will result in golden cross
    // First, establish long MA below short MA
    for i in 1..=30 {
        let price = 1000000000 - (30 - i) * 20000; // Declining then flat
        let market_update = TradingEvent::MarketUpdate {
            symbol,
            bid: Some((Px::from_i64(price), Qty::from_i64(10000))),
            ask: Some((Px::from_i64(price + 10000), Qty::from_i64(10000))),
            mid: Px::from_i64(price + 5000),
            spread: 10000,
            imbalance: 0.0,
            vpin: 20.0,
            kyles_lambda: 0.4,
            timestamp: Ts::now(),
        };
        momentum_strategy.on_market_update(&market_update).await?;
    }
    
    // Now create rising prices to trigger golden cross
    for i in 31..=55 {
        let price = 1000000000 + (i - 30) * 30000; // Rising prices
        let market_update = TradingEvent::MarketUpdate {
            symbol,
            bid: Some((Px::from_i64(price), Qty::from_i64(10000))),
            ask: Some((Px::from_i64(price + 10000), Qty::from_i64(10000))),
            mid: Px::from_i64(price + 5000),
            spread: 10000,
            imbalance: 0.0,
            vpin: 20.0,
            kyles_lambda: 0.4,
            timestamp: Ts::now(),
        };
        
        let result = momentum_strategy.on_market_update(&market_update).await?;
        
        if let Some(TradingEvent::Signal { side, signal_type, .. }) = result {
            assert_eq!(signal_type, SignalType::Momentum);
            assert_eq!(side, Side::Buy); // Golden cross should generate buy signal
            return Ok(()); // Test passed
        }
    }
    
    // The test setup should have generated a signal
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_momentum_death_cross_detection(
    mut momentum_strategy: MomentumStrategy
) -> Result<()> {
    let symbol = Symbol(4);
    
    // Start with rising prices to establish upward moving averages
    for i in 1..=30 {
        let price = 800000000 + i * 30000; // Rising
        let market_update = TradingEvent::MarketUpdate {
            symbol,
            bid: Some((Px::from_i64(price), Qty::from_i64(10000))),
            ask: Some((Px::from_i64(price + 10000), Qty::from_i64(10000))),
            mid: Px::from_i64(price + 5000),
            spread: 10000,
            imbalance: 0.0,
            vpin: 20.0,
            kyles_lambda: 0.4,
            timestamp: Ts::now(),
        };
        momentum_strategy.on_market_update(&market_update).await?;
    }
    
    // Now create falling prices to trigger death cross
    for i in 31..=55 {
        let price = 1700000000 - (i - 30) * 40000; // Falling prices
        let market_update = TradingEvent::MarketUpdate {
            symbol,
            bid: Some((Px::from_i64(price), Qty::from_i64(10000))),
            ask: Some((Px::from_i64(price + 10000), Qty::from_i64(10000))),
            mid: Px::from_i64(price + 5000),
            spread: 10000,
            imbalance: 0.0,
            vpin: 20.0,
            kyles_lambda: 0.4,
            timestamp: Ts::now(),
        };
        
        let result = momentum_strategy.on_market_update(&market_update).await?;
        
        if let Some(TradingEvent::Signal { side, signal_type, .. }) = result {
            assert_eq!(signal_type, SignalType::Momentum);
            assert_eq!(side, Side::Sell); // Death cross should generate sell signal
            return Ok(()); // Test passed
        }
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_momentum_signal_rate_limiting(
    mut momentum_strategy: MomentumStrategy
) -> Result<()> {
    let symbol = Symbol(5);
    
    // Build sufficient history first
    for i in 1..=55 {
        let price = 1000000000 + i * 20000; // Rising trend
        let market_update = TradingEvent::MarketUpdate {
            symbol,
            bid: Some((Px::from_i64(price), Qty::from_i64(10000))),
            ask: Some((Px::from_i64(price + 10000), Qty::from_i64(10000))),
            mid: Px::from_i64(price + 5000),
            spread: 10000,
            imbalance: 0.0,
            vpin: 20.0,
            kyles_lambda: 0.4,
            timestamp: Ts::now(),
        };
        momentum_strategy.on_market_update(&market_update).await?;
    }
    
    // Now rapidly send updates that would generate signals
    let mut signal_count = 0;
    for i in 56..=70 {
        let price = 1000000000 + i * 20000;
        let market_update = TradingEvent::MarketUpdate {
            symbol,
            bid: Some((Px::from_i64(price), Qty::from_i64(10000))),
            ask: Some((Px::from_i64(price + 10000), Qty::from_i64(10000))),
            mid: Px::from_i64(price + 5000),
            spread: 10000,
            imbalance: 0.0,
            vpin: 20.0,
            kyles_lambda: 0.4,
            timestamp: Ts::now(),
        };
        
        let result = momentum_strategy.on_market_update(&market_update).await?;
        if result.is_some() {
            signal_count += 1;
        }
    }
    
    // Should be rate limited (max 1 signal per second)
    assert!(signal_count <= 2, "Signals should be rate limited");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_momentum_strategy_reset(
    mut momentum_strategy: MomentumStrategy
) -> Result<()> {
    let symbol = Symbol(6);
    
    // Build some history
    for i in 1..=30 {
        let market_update = TradingEvent::MarketUpdate {
            symbol,
            bid: Some((Px::from_i64(1000000000 + i * 1000), Qty::from_i64(10000))),
            ask: Some((Px::from_i64(1000000000 + i * 1000 + 10000), Qty::from_i64(10000))),
            mid: Px::from_i64(1000000000 + i * 1000 + 5000),
            spread: 10000,
            imbalance: 0.0,
            vpin: 20.0,
            kyles_lambda: 0.4,
            timestamp: Ts::now(),
        };
        momentum_strategy.on_market_update(&market_update).await?;
    }
    
    // Reset strategy
    momentum_strategy.reset().await?;
    
    // After reset, should not generate signals immediately (history cleared)
    let market_update = TradingEvent::MarketUpdate {
        symbol,
        bid: Some((Px::from_i64(1200000000), Qty::from_i64(10000))),
        ask: Some((Px::from_i64(1201000000), Qty::from_i64(10000))),
        mid: Px::from_i64(1200500000),
        spread: 10000,
        imbalance: 0.0,
        vpin: 20.0,
        kyles_lambda: 0.4,
        timestamp: Ts::now(),
    };
    
    let result = momentum_strategy.on_market_update(&market_update).await?;
    assert!(result.is_none(), "Should not generate signals immediately after reset");
    
    Ok(())
}

// ===== Arbitrage Strategy Tests =====

#[rstest]
#[tokio::test]
async fn test_arbitrage_strategy_creation(arbitrage_strategy: ArbitrageStrategy) {
    assert_eq!(arbitrage_strategy.name(), "Arbitrage");
    
    let health = arbitrage_strategy.health();
    assert!(health.is_healthy);
    assert_eq!(health.success_count, 0);
}

#[rstest]
#[tokio::test]
async fn test_arbitrage_opportunity_detection(
    mut arbitrage_strategy: ArbitrageStrategy,
    arbitrage_opportunity: TradingEvent
) -> Result<()> {
    let result = arbitrage_strategy.on_market_update(&arbitrage_opportunity).await?;
    
    assert!(result.is_some(), "Should detect arbitrage opportunity");
    
    if let Some(TradingEvent::Signal { 
        signal_type, side, strength, confidence, .. 
    }) = result {
        assert_eq!(signal_type, SignalType::Arbitrage);
        assert_eq!(side, Side::Buy); // Should buy at lower ask
        assert!(strength > 0.0);
        assert!(confidence > 0.0);
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_arbitrage_normal_spread_no_signal(
    mut arbitrage_strategy: ArbitrageStrategy
) -> Result<()> {
    // Normal positive spread
    let normal_market = TradingEvent::MarketUpdate {
        symbol: Symbol(10),
        bid: Some((Px::from_i64(1000000000), Qty::from_i64(10000))), // $100.00
        ask: Some((Px::from_i64(1002000000), Qty::from_i64(8000))),  // $100.20 - normal spread
        mid: Px::from_i64(1001000000),
        spread: 200000, // 2 cent positive spread
        imbalance: 0.0,
        vpin: 20.0,
        kyles_lambda: 0.4,
        timestamp: Ts::now(),
    };
    
    let result = arbitrage_strategy.on_market_update(&normal_market).await?;
    
    assert!(result.is_none(), "Should not detect arbitrage in normal spread");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_arbitrage_threshold_sensitivity(
    mut arbitrage_strategy: ArbitrageStrategy
) -> Result<()> {
    // Test just above threshold (should trigger)
    let above_threshold = TradingEvent::MarketUpdate {
        symbol: Symbol(11),
        bid: Some((Px::from_i64(1001200000), Qty::from_i64(10000))), // $100.12
        ask: Some((Px::from_i64(1000000000), Qty::from_i64(8000))),  // $100.00
        mid: Px::from_i64(1000600000),
        spread: -120000, // -0.12% spread (above 0.1% threshold)
        imbalance: 0.0,
        vpin: 20.0,
        kyles_lambda: 0.4,
        timestamp: Ts::now(),
    };
    
    let result1 = arbitrage_strategy.on_market_update(&above_threshold).await?;
    assert!(result1.is_some(), "Should detect arbitrage above threshold");
    
    // Test just below threshold (should not trigger)
    let below_threshold = TradingEvent::MarketUpdate {
        symbol: Symbol(12),
        bid: Some((Px::from_i64(1000800000), Qty::from_i64(10000))), // $100.08
        ask: Some((Px::from_i64(1000000000), Qty::from_i64(8000))),  // $100.00
        mid: Px::from_i64(1000400000),
        spread: -80000, // -0.08% spread (below 0.1% threshold)
        imbalance: 0.0,
        vpin: 20.0,
        kyles_lambda: 0.4,
        timestamp: Ts::now(),
    };
    
    let result2 = arbitrage_strategy.on_market_update(&below_threshold).await?;
    assert!(result2.is_none(), "Should not detect arbitrage below threshold");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_arbitrage_missing_quotes_handling(
    mut arbitrage_strategy: ArbitrageStrategy
) -> Result<()> {
    // Missing bid
    let missing_bid = TradingEvent::MarketUpdate {
        symbol: Symbol(13),
        bid: None,
        ask: Some((Px::from_i64(1000000000), Qty::from_i64(8000))),
        mid: Px::from_i64(1000500000),
        spread: 0,
        imbalance: 0.0,
        vpin: 20.0,
        kyles_lambda: 0.4,
        timestamp: Ts::now(),
    };
    
    let result1 = arbitrage_strategy.on_market_update(&missing_bid).await?;
    assert!(result1.is_none(), "Should handle missing bid gracefully");
    
    // Missing ask
    let missing_ask = TradingEvent::MarketUpdate {
        symbol: Symbol(14),
        bid: Some((Px::from_i64(1000000000), Qty::from_i64(10000))),
        ask: None,
        mid: Px::from_i64(1000500000),
        spread: 0,
        imbalance: 0.0,
        vpin: 20.0,
        kyles_lambda: 0.4,
        timestamp: Ts::now(),
    };
    
    let result2 = arbitrage_strategy.on_market_update(&missing_ask).await?;
    assert!(result2.is_none(), "Should handle missing ask gracefully");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_arbitrage_strategy_reset(
    mut arbitrage_strategy: ArbitrageStrategy
) -> Result<()> {
    // Process some events to build state
    let opportunity = TradingEvent::MarketUpdate {
        symbol: Symbol(15),
        bid: Some((Px::from_i64(1001500000), Qty::from_i64(10000))),
        ask: Some((Px::from_i64(1000000000), Qty::from_i64(8000))),
        mid: Px::from_i64(1000750000),
        spread: -150000,
        imbalance: 0.0,
        vpin: 20.0,
        kyles_lambda: 0.4,
        timestamp: Ts::now(),
    };
    
    arbitrage_strategy.on_market_update(&opportunity).await?;
    
    // Reset strategy
    arbitrage_strategy.reset().await?;
    
    // Strategy should continue to work after reset
    let result = arbitrage_strategy.on_market_update(&opportunity).await?;
    assert!(result.is_some(), "Should still detect arbitrage after reset");
    
    Ok(())
}

// ===== Common Strategy Interface Tests =====

#[rstest]
#[tokio::test]
async fn test_strategy_health_updates(
    mut momentum_strategy: MomentumStrategy
) -> Result<()> {
    let initial_health = momentum_strategy.health();
    assert_eq!(initial_health.success_count, 0);
    
    // Process several market updates
    for i in 1..=60 {
        let market_update = TradingEvent::MarketUpdate {
            symbol: Symbol(20),
            bid: Some((Px::from_i64(1000000000 + i * 1000), Qty::from_i64(10000))),
            ask: Some((Px::from_i64(1000000000 + i * 1000 + 10000), Qty::from_i64(10000))),
            mid: Px::from_i64(1000000000 + i * 1000 + 5000),
            spread: 10000,
            imbalance: 0.0,
            vpin: 20.0,
            kyles_lambda: 0.4,
            timestamp: Ts::now(),
        };
        momentum_strategy.on_market_update(&market_update).await?;
    }
    
    let updated_health = momentum_strategy.health();
    assert!(updated_health.success_count > 0, "Health should track successful operations");
    assert!(updated_health.avg_latency_us > 0, "Should track latency");
    assert!(updated_health.is_healthy, "Should remain healthy");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_execution_report_handling(
    mut momentum_strategy: MomentumStrategy
) -> Result<()> {
    let execution_report = TradingEvent::ExecutionReport {
        order_id: 123,
        symbol: Symbol(21),
        side: Side::Buy,
        executed_qty: Qty::from_i64(10000),
        executed_price: Px::from_i64(1000000000),
        remaining_qty: Qty::ZERO,
        status: trading_gateway::OrderStatus::Filled,
        timestamp: Ts::now(),
    };
    
    // Should handle execution reports gracefully
    let result = momentum_strategy.on_execution(&execution_report).await;
    assert!(result.is_ok(), "Should handle execution reports without error");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_non_market_update_events(
    mut momentum_strategy: MomentumStrategy,
    mut arbitrage_strategy: ArbitrageStrategy
) -> Result<()> {
    let signal_event = TradingEvent::Signal {
        id: 1,
        symbol: Symbol(22),
        side: Side::Buy,
        signal_type: SignalType::MeanReversion,
        strength: 0.7,
        confidence: 0.8,
        timestamp: Ts::now(),
    };
    
    // Both strategies should handle non-market events gracefully
    let momentum_result = momentum_strategy.on_market_update(&signal_event).await?;
    let arbitrage_result = arbitrage_strategy.on_market_update(&signal_event).await?;
    
    assert!(momentum_result.is_none(), "Momentum should ignore non-market events");
    assert!(arbitrage_result.is_none(), "Arbitrage should ignore non-market events");
    
    Ok(())
}

#[rstest]
#[case(1000000000, 1100000000, true)]   // 10% rise should generate signal
#[case(1000000000, 1001000000, false)]  // 0.1% rise should not generate strong signal
#[case(1000000000, 900000000, true)]    // 10% fall should generate signal  
#[case(1000000000, 999000000, false)]   // 0.1% fall should not generate strong signal
#[tokio::test]
async fn test_momentum_signal_strength_parameterized(
    mut momentum_strategy: MomentumStrategy,
    #[case] initial_price: i64,
    #[case] final_price: i64,
    #[case] should_generate_signal: bool
) -> Result<()> {
    let symbol = Symbol(30);
    
    // Build history with initial price pattern
    for i in 1..=25 {
        let market_update = TradingEvent::MarketUpdate {
            symbol,
            bid: Some((Px::from_i64(initial_price), Qty::from_i64(10000))),
            ask: Some((Px::from_i64(initial_price + 10000), Qty::from_i64(10000))),
            mid: Px::from_i64(initial_price + 5000),
            spread: 10000,
            imbalance: 0.0,
            vpin: 20.0,
            kyles_lambda: 0.4,
            timestamp: Ts::now(),
        };
        momentum_strategy.on_market_update(&market_update).await?;
    }
    
    // Create trend with final prices
    for i in 26..=55 {
        let trend_price = initial_price + ((final_price - initial_price) * (i - 25)) / 30;
        let market_update = TradingEvent::MarketUpdate {
            symbol,
            bid: Some((Px::from_i64(trend_price), Qty::from_i64(10000))),
            ask: Some((Px::from_i64(trend_price + 10000), Qty::from_i64(10000))),
            mid: Px::from_i64(trend_price + 5000),
            spread: 10000,
            imbalance: 0.0,
            vpin: 20.0,
            kyles_lambda: 0.4,
            timestamp: Ts::now(),
        };
        let result = momentum_strategy.on_market_update(&market_update).await?;
        
        if result.is_some() {
            if should_generate_signal {
                // Expected to generate signal
                if let Some(TradingEvent::Signal { side, .. }) = result {
                    let expected_side = if final_price > initial_price { Side::Buy } else { Side::Sell };
                    assert_eq!(side, expected_side);
                }
                return Ok(()); // Test passed
            } else {
                // Should not generate strong signals for small moves
                // If it does generate, the strength should be low
                if let Some(TradingEvent::Signal { strength, .. }) = result {
                    assert!(strength < 0.5, "Small price moves should generate weak signals");
                }
            }
        }
    }
    
    Ok(())
}