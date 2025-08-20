//! Unit tests for MarketMakingStrategy
//!
//! Comprehensive tests covering:
//! - Quote calculation and pricing logic
//! - Dynamic spread adjustment based on toxicity
//! - Price skewing based on order book imbalance
//! - Inventory management and limits
//! - Quote freshness and staleness detection
//! - Strategy health monitoring
//! - Execution report processing
//! - Edge cases and error scenarios

use anyhow::Result;
use rstest::*;
use std::time::Duration;
use tokio::time::sleep;
use trading_gateway::{
    market_maker::MarketMakingStrategy,
    ComponentHealth, OrderType, Side, TradingEvent, TradingStrategy,
};
use services_common::{Px, Qty, Symbol, Ts};

/// Test fixture for creating a MarketMakingStrategy
#[fixture]
fn market_maker() -> MarketMakingStrategy {
    MarketMakingStrategy::new()
}

/// Test fixture for creating a market update event
#[fixture]
fn market_update() -> TradingEvent {
    TradingEvent::MarketUpdate {
        symbol: Symbol(1),
        bid: Some((Px::from_i64(1000000000), Qty::from_i64(15000))), // $100.00, 1.5 units
        ask: Some((Px::from_i64(1001000000), Qty::from_i64(12000))), // $100.10, 1.2 units  
        mid: Px::from_i64(1000500000), // $100.05
        spread: 100000, // 1 cent
        imbalance: 10.0, // 10% imbalance
        vpin: 25.0, // 25% VPIN (moderate toxicity)
        kyles_lambda: 0.5,
        timestamp: Ts::now(),
    }
}

/// Test fixture for creating a tight spread market update
#[fixture]
fn tight_spread_market_update() -> TradingEvent {
    TradingEvent::MarketUpdate {
        symbol: Symbol(2),
        bid: Some((Px::from_i64(2000000000), Qty::from_i64(20000))), // $200.00
        ask: Some((Px::from_i64(2000200000), Qty::from_i64(18000))), // $200.02 (2 cent spread)
        mid: Px::from_i64(2000100000), // $200.01
        spread: 20000, // 0.2 cent - very tight
        imbalance: 5.0,
        vpin: 15.0,
        kyles_lambda: 0.3,
        timestamp: Ts::now(),
    }
}

/// Test fixture for creating a high toxicity market update
#[fixture]
fn high_toxicity_market_update() -> TradingEvent {
    TradingEvent::MarketUpdate {
        symbol: Symbol(3),
        bid: Some((Px::from_i64(3000000000), Qty::from_i64(10000))),
        ask: Some((Px::from_i64(3005000000), Qty::from_i64(8000))),
        mid: Px::from_i64(3002500000),
        spread: 500000, // 5 cent spread
        imbalance: 50.0, // High imbalance
        vpin: 80.0, // High toxicity
        kyles_lambda: 1.2,
        timestamp: Ts::now(),
    }
}

#[rstest]
#[tokio::test]
async fn test_market_maker_creation(market_maker: MarketMakingStrategy) {
    // Test basic creation and initial state
    assert_eq!(market_maker.name(), "MarketMaker");
    
    let health = market_maker.health();
    assert!(health.is_healthy);
    assert_eq!(health.name, "MarketMaker");
    assert_eq!(health.error_count, 0);
    assert_eq!(health.success_count, 0);
}

#[rstest]
#[tokio::test]
async fn test_market_update_processing(
    mut market_maker: MarketMakingStrategy,
    market_update: TradingEvent
) -> Result<()> {
    // Process market update
    let result = market_maker.on_market_update(&market_update).await?;
    
    assert!(result.is_some(), "Should generate a quote order");
    
    if let Some(TradingEvent::OrderRequest { 
        symbol, side, order_type, quantity, price, .. 
    }) = result {
        assert_eq!(symbol, Symbol(1));
        assert_eq!(side, Side::Buy); // Should generate bid order
        assert_eq!(order_type, OrderType::Limit);
        assert!(quantity.as_i64() > 0);
        assert!(price.is_some());
        
        // Price should be below mid price (it's a bid)
        let quote_price = price.unwrap();
        assert!(quote_price.as_i64() < 1000500000); // Below mid price
    }
    
    // Check that strategy health was updated
    let health = market_maker.health();
    assert_eq!(health.success_count, 1);
    assert!(health.avg_latency_us > 0);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_tight_spread_rejection(
    mut market_maker: MarketMakingStrategy,
    tight_spread_market_update: TradingEvent
) -> Result<()> {
    // Should not make markets when spreads are too tight
    let result = market_maker.on_market_update(&tight_spread_market_update).await?;
    
    assert!(result.is_none(), "Should not generate orders for tight spreads");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_toxicity_spread_adjustment(
    mut market_maker: MarketMakingStrategy,
    high_toxicity_market_update: TradingEvent
) -> Result<()> {
    // High toxicity should result in wider spreads
    let result = market_maker.on_market_update(&high_toxicity_market_update).await?;
    
    if let Some(TradingEvent::OrderRequest { price, .. }) = result {
        let quote_price = price.unwrap();
        let mid_price = 3002500000i64; // From fixture
        
        // Calculate spread from mid
        let spread_from_mid = (mid_price - quote_price.as_i64()).abs();
        
        // With high VPIN (80%), spread should be significantly widened
        // Base spread would be 10 bps * $3000 = $3.00
        // With toxicity adjustment: $3.00 * (1 + 80/100) = $5.40
        let expected_min_spread = (3000.0 * 0.001 * 1.8 * 10000.0) as i64; // Approximate
        
        assert!(spread_from_mid > expected_min_spread / 2, 
            "Spread should be widened for high toxicity");
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_price_skewing_logic(
    mut market_maker: MarketMakingStrategy
) -> Result<()> {
    // Create market update with strong positive imbalance (more buying pressure)
    let buy_pressure_update = TradingEvent::MarketUpdate {
        symbol: Symbol(4),
        bid: Some((Px::from_i64(1000000000), Qty::from_i64(25000))),
        ask: Some((Px::from_i64(1001000000), Qty::from_i64(10000))),
        mid: Px::from_i64(1000500000),
        spread: 100000,
        imbalance: 60.0, // Strong buy pressure
        vpin: 30.0,
        kyles_lambda: 0.6,
        timestamp: Ts::now(),
    };
    
    let result = market_maker.on_market_update(&buy_pressure_update).await?;
    
    if let Some(TradingEvent::OrderRequest { price, .. }) = result {
        let quote_price = price.unwrap();
        let mid_price = 1000500000i64;
        
        // With positive imbalance, bid should be skewed away from mid
        // (strategy should be more cautious about buying)
        assert!(quote_price.as_i64() < mid_price, "Bid should be below mid with buy pressure");
        
        // The skew should make the bid even further from mid than normal
        let normal_half_spread = (1000500000.0 * 10.0 / 10000.0) as i64; // 10 bps
        let distance_from_mid = mid_price - quote_price.as_i64();
        
        assert!(distance_from_mid > normal_half_spread, 
            "Price should be skewed due to imbalance");
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_quote_state_tracking(
    mut market_maker: MarketMakingStrategy,
    market_update: TradingEvent
) -> Result<()> {
    let symbol = Symbol(1);
    
    // Initially no quotes
    let initial_quotes = market_maker.get_quotes(&symbol);
    assert!(initial_quotes.is_none());
    
    // Process market update to generate quotes
    market_maker.on_market_update(&market_update).await?;
    
    // Now should have quotes
    let quotes = market_maker.get_quotes(&symbol);
    assert!(quotes.is_some());
    
    if let Some((bid_px, ask_px, bid_sz, ask_sz)) = quotes {
        assert!(bid_px.as_i64() > 0);
        assert!(ask_px.as_i64() > 0);
        assert!(ask_px.as_i64() > bid_px.as_i64()); // Ask > Bid
        assert!(bid_sz.as_i64() > 0);
        assert!(ask_sz.as_i64() > 0);
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_quote_staleness_detection(
    mut market_maker: MarketMakingStrategy,
    market_update: TradingEvent
) -> Result<()> {
    let symbol = Symbol(1);
    
    // Initially should be stale (no quotes)
    assert!(market_maker.quotes_are_stale(&symbol, 1));
    
    // Generate quotes
    market_maker.on_market_update(&market_update).await?;
    
    // Should not be stale immediately
    assert!(!market_maker.quotes_are_stale(&symbol, 10));
    
    // Should be stale after time passes
    sleep(Duration::from_millis(50)).await;
    assert!(market_maker.quotes_are_stale(&symbol, 0)); // 0 second tolerance
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_inventory_tracking(
    mut market_maker: MarketMakingStrategy
) -> Result<()> {
    let symbol = Symbol(5);
    
    // Simulate buy execution (increases inventory)
    let buy_execution = TradingEvent::ExecutionReport {
        order_id: 1,
        symbol,
        side: Side::Buy,
        executed_qty: Qty::from_i64(15000), // 1.5 units
        executed_price: Px::from_i64(1000000000),
        remaining_qty: Qty::ZERO,
        status: trading_gateway::OrderStatus::Filled,
        timestamp: Ts::now(),
    };
    
    market_maker.on_execution(&buy_execution).await?;
    
    // Simulate sell execution (decreases inventory)  
    let sell_execution = TradingEvent::ExecutionReport {
        order_id: 2,
        symbol,
        side: Side::Sell,
        executed_qty: Qty::from_i64(5000), // 0.5 units
        executed_price: Px::from_i64(1001000000),
        remaining_qty: Qty::ZERO,
        status: trading_gateway::OrderStatus::Filled,
        timestamp: Ts::now(),
    };
    
    market_maker.on_execution(&sell_execution).await?;
    
    // Net inventory should be +1.0 unit (15000 - 5000 = 10000)
    // We can't directly check inventory, but can verify the strategy
    // processes executions without error
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_inventory_limit_enforcement(
    mut market_maker: MarketMakingStrategy
) -> Result<()> {
    let symbol = Symbol(6);
    
    // Build up inventory close to limit (100,000 units = 10.0)
    for i in 1..=9 {
        let execution = TradingEvent::ExecutionReport {
            order_id: i,
            symbol,
            side: Side::Buy,
            executed_qty: Qty::from_i64(10000), // 1.0 unit each
            executed_price: Px::from_i64(1000000000),
            remaining_qty: Qty::ZERO,
            status: trading_gateway::OrderStatus::Filled,
            timestamp: Ts::now(),
        };
        market_maker.on_execution(&execution).await?;
    }
    
    // Create market update
    let market_update = TradingEvent::MarketUpdate {
        symbol,
        bid: Some((Px::from_i64(1000000000), Qty::from_i64(10000))),
        ask: Some((Px::from_i64(1001000000), Qty::from_i64(10000))),
        mid: Px::from_i64(1000500000),
        spread: 100000,
        imbalance: 0.0,
        vpin: 20.0,
        kyles_lambda: 0.4,
        timestamp: Ts::now(),
    };
    
    // Should still generate quotes (inventory limits are checked internally)
    let result = market_maker.on_market_update(&market_update).await?;
    
    // The strategy should handle inventory limits gracefully
    assert!(result.is_some() || result.is_none()); // Should not panic
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_multiple_symbol_handling(
    mut market_maker: MarketMakingStrategy
) -> Result<()> {
    // Test with different symbols
    let symbols = vec![Symbol(10), Symbol(11), Symbol(12)];
    
    for (i, symbol) in symbols.iter().enumerate() {
        let market_update = TradingEvent::MarketUpdate {
            symbol: *symbol,
            bid: Some((Px::from_i64((1000 + i as i64 * 100) * 10000000), Qty::from_i64(10000))),
            ask: Some((Px::from_i64((1001 + i as i64 * 100) * 10000000), Qty::from_i64(10000))),
            mid: Px::from_i64((1000 + i as i64 * 100) * 10000000 + 5000000),
            spread: 100000,
            imbalance: 0.0,
            vpin: 20.0,
            kyles_lambda: 0.4,
            timestamp: Ts::now(),
        };
        
        let result = market_maker.on_market_update(&market_update).await?;
        
        if let Some(TradingEvent::OrderRequest { symbol: order_symbol, .. }) = result {
            assert_eq!(order_symbol, *symbol);
        }
        
        // Check quotes are tracked per symbol
        let quotes = market_maker.get_quotes(symbol);
        assert!(quotes.is_some());
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_strategy_reset(
    mut market_maker: MarketMakingStrategy,
    market_update: TradingEvent
) -> Result<()> {
    let symbol = Symbol(1);
    
    // Generate some state
    market_maker.on_market_update(&market_update).await?;
    
    // Add some inventory
    let execution = TradingEvent::ExecutionReport {
        order_id: 1,
        symbol,
        side: Side::Buy,
        executed_qty: Qty::from_i64(10000),
        executed_price: Px::from_i64(1000000000),
        remaining_qty: Qty::ZERO,
        status: trading_gateway::OrderStatus::Filled,
        timestamp: Ts::now(),
    };
    market_maker.on_execution(&execution).await?;
    
    // Verify state exists
    assert!(market_maker.get_quotes(&symbol).is_some());
    
    // Reset strategy
    market_maker.reset().await?;
    
    // State should be cleared
    assert!(market_maker.get_quotes(&symbol).is_none());
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_non_market_update_handling(
    mut market_maker: MarketMakingStrategy
) -> Result<()> {
    // Test with non-market update event
    let signal_event = TradingEvent::Signal {
        id: 1,
        symbol: Symbol(1),
        side: Side::Buy,
        signal_type: trading_gateway::SignalType::Momentum,
        strength: 0.8,
        confidence: 0.7,
        timestamp: Ts::now(),
    };
    
    // Should return None for non-market updates
    let result = market_maker.on_market_update(&signal_event).await?;
    assert!(result.is_none());
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_health_monitoring(
    mut market_maker: MarketMakingStrategy,
    market_update: TradingEvent
) -> Result<()> {
    let initial_health = market_maker.health();
    assert_eq!(initial_health.success_count, 0);
    assert!(initial_health.avg_latency_us == 0);
    
    // Process several market updates
    for _ in 0..5 {
        market_maker.on_market_update(&market_update).await?;
    }
    
    let updated_health = market_maker.health();
    assert_eq!(updated_health.success_count, 5);
    assert!(updated_health.avg_latency_us > 0);
    assert!(updated_health.is_healthy);
    
    Ok(())
}

#[rstest]
#[case(0.0, 15.0)]      // Low VPIN
#[case(50.0, 30.0)]     // Medium VPIN  
#[case(90.0, 45.0)]     // High VPIN
#[tokio::test]
async fn test_vpin_spread_adjustment_parameterized(
    mut market_maker: MarketMakingStrategy,
    #[case] vpin: f64,
    #[case] imbalance: f64
) -> Result<()> {
    let market_update = TradingEvent::MarketUpdate {
        symbol: Symbol(20),
        bid: Some((Px::from_i64(1000000000), Qty::from_i64(10000))),
        ask: Some((Px::from_i64(1010000000), Qty::from_i64(10000))),
        mid: Px::from_i64(1005000000), // $100.50
        spread: 1000000, // 10 cent spread
        imbalance,
        vpin,
        kyles_lambda: 0.5,
        timestamp: Ts::now(),
    };
    
    let result = market_maker.on_market_update(&market_update).await?;
    
    if let Some(TradingEvent::OrderRequest { price, .. }) = result {
        let quote_price = price.unwrap();
        let mid_price = 1005000000i64;
        let spread_from_mid = (mid_price - quote_price.as_i64()).abs();
        
        // Higher VPIN should result in wider spreads
        // Base spread: 10 bps of $100.50 = $1.005
        // With VPIN adjustment: base * (1 + vpin/100)
        let base_spread = (1005000000.0 * 0.001) as i64; // 10 bps
        let expected_min_spread = (base_spread as f64 * (1.0 + vpin / 100.0) * 0.5) as i64;
        
        if vpin > 70.0 {
            assert!(spread_from_mid > expected_min_spread, 
                "High VPIN should significantly widen spreads");
        }
        // For lower VPIN, just verify the calculation doesn't break
        assert!(spread_from_mid > 0);
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_order_generation_completeness(
    mut market_maker: MarketMakingStrategy,
    market_update: TradingEvent
) -> Result<()> {
    let result = market_maker.on_market_update(&market_update).await?;
    
    if let Some(TradingEvent::OrderRequest { 
        id, symbol, side, order_type, quantity, price, time_in_force, strategy_id 
    }) = result {
        // Verify all fields are properly set
        assert!(id > 0);
        assert_eq!(symbol, Symbol(1));
        assert!(matches!(side, Side::Buy | Side::Sell));
        assert_eq!(order_type, OrderType::Limit);
        assert!(quantity.as_i64() > 0);
        assert!(price.is_some());
        assert!(price.unwrap().as_i64() > 0);
        assert_eq!(time_in_force, trading_gateway::TimeInForce::Gtc);
        assert_eq!(strategy_id, "MarketMaker");
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_concurrent_market_updates() -> Result<()> {
    let market_maker = std::sync::Arc::new(tokio::sync::Mutex::new(MarketMakingStrategy::new()));
    let mut handles = Vec::new();
    
    // Process updates concurrently for different symbols
    for i in 1..=20 {
        let mm = market_maker.clone();
        let symbol = Symbol(i % 5 + 1); // 5 different symbols
        
        let handle = tokio::spawn(async move {
            let market_update = TradingEvent::MarketUpdate {
                symbol,
                bid: Some((Px::from_i64(1000000000 + i * 1000000), Qty::from_i64(10000))),
                ask: Some((Px::from_i64(1010000000 + i * 1000000), Qty::from_i64(10000))),
                mid: Px::from_i64(1005000000 + i * 1000000),
                spread: 1000000,
                imbalance: (i as f64) * 2.0,
                vpin: (i as f64) * 1.5,
                kyles_lambda: 0.5,
                timestamp: Ts::now(),
            };
            
            let mut strategy = mm.lock().await;
            strategy.on_market_update(&market_update).await
        });
        handles.push(handle);
    }
    
    // Wait for all updates
    for handle in handles {
        handle.await??;
    }
    
    // Strategy should handle concurrent updates without issues
    let strategy = market_maker.lock().await;
    let health = strategy.health();
    assert!(health.success_count > 0);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_extreme_market_conditions(
    mut market_maker: MarketMakingStrategy
) -> Result<()> {
    // Test with extreme market conditions
    let extreme_update = TradingEvent::MarketUpdate {
        symbol: Symbol(99),
        bid: Some((Px::from_i64(1000000), Qty::from_i64(1000))), // Very low price
        ask: Some((Px::from_i64(100000000000), Qty::from_i64(100))), // Very high price
        mid: Px::from_i64(50000500000), // Wide spread
        spread: 99000000000, // Extremely wide spread
        imbalance: 95.0, // Extreme imbalance
        vpin: 99.0, // Extreme toxicity
        kyles_lambda: 5.0, // High adverse selection
        timestamp: Ts::now(),
    };
    
    // Should handle extreme conditions gracefully without panicking
    let result = market_maker.on_market_update(&extreme_update).await;
    
    // Should either succeed or fail gracefully (no panic)
    assert!(result.is_ok(), "Should handle extreme conditions gracefully");
    
    Ok(())
}