//! Integration tests for Trading Gateway

use anyhow::Result;
use common::{Px, Qty, Symbol, Ts};
use orderbook::OrderBook;
use orderbook::analytics::MicrostructureAnalytics;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use trading_gateway::{
    GatewayConfig, TradingGateway, TradingEvent, Side, OrderType, TimeInForce,
    SignalType, OrderStatus, Severity, RiskAction,
};

#[tokio::test]
async fn test_gateway_creation_and_startup() -> Result<()> {
    let config = GatewayConfig::default();
    let gateway = Arc::new(TradingGateway::new(config).await?);
    
    gateway.start().await?;
    
    let status = gateway.get_status().await;
    assert!(status.is_running);
    assert!(status.active_strategies > 0);
    
    Ok(())
}

#[tokio::test]
async fn test_orderbook_integration() -> Result<()> {
    let config = GatewayConfig::default();
    let gateway = Arc::new(TradingGateway::new(config).await?);
    gateway.start().await?;
    
    // Create orderbook and analytics
    let orderbook = OrderBook::new("BTCUSDT");
    let analytics = MicrostructureAnalytics::new();
    
    // Simulate orderbook update
    let symbol = Symbol(1); // BTCUSDT
    
    // Load some test data into orderbook
    let bid_levels = vec![
        (Px::from_i64(1000000000), Qty::from_i64(10000), 1),
        (Px::from_i64(999900000), Qty::from_i64(20000), 2),
    ];
    let ask_levels = vec![
        (Px::from_i64(1000100000), Qty::from_i64(15000), 1),
        (Px::from_i64(1000200000), Qty::from_i64(25000), 2),
    ];
    orderbook.load_snapshot(bid_levels, ask_levels);
    
    // Process orderbook update
    gateway.process_orderbook_update(symbol, &orderbook, &analytics).await?;
    
    // Give strategies time to process
    sleep(Duration::from_millis(100)).await;
    
    Ok(())
}

#[tokio::test]
async fn test_risk_gate() -> Result<()> {
    let mut config = GatewayConfig::default();
    config.max_position_size = Qty::from_i64(50000); // 5 units
    
    let gateway = Arc::new(TradingGateway::new(config).await?);
    gateway.start().await?;
    
    // Create order request
    let order = TradingEvent::OrderRequest {
        id: 1,
        symbol: Symbol(1),
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(10000), // 1 unit - should pass
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "Test".to_string(),
    };
    
    // Should pass risk check
    let passed = gateway.risk_gate.check_order(&order).await?;
    assert!(passed);
    
    // Create large order
    let large_order = TradingEvent::OrderRequest {
        id: 2,
        symbol: Symbol(1),
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(100000), // 10 units - should fail
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "Test".to_string(),
    };
    
    // Should fail risk check
    let passed = gateway.risk_gate.check_order(&large_order).await?;
    assert!(!passed);
    
    let metrics = gateway.risk_gate.get_metrics();
    assert_eq!(metrics.orders_checked, 2);
    assert_eq!(metrics.orders_rejected, 1);
    
    Ok(())
}

#[tokio::test]
async fn test_execution_engine() -> Result<()> {
    let config = GatewayConfig::default();
    let gateway = Arc::new(TradingGateway::new(config).await?);
    gateway.start().await?;
    
    // Submit market order
    let order = TradingEvent::OrderRequest {
        id: 1,
        symbol: Symbol(1),
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(10000),
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "Test".to_string(),
    };
    
    gateway.execution_engine.submit_order(order).await?;
    
    // Give time for processing
    sleep(Duration::from_millis(100)).await;
    
    let metrics = gateway.execution_engine.get_metrics();
    assert_eq!(metrics.orders_submitted, 1);
    
    // Get order state
    let order_state = gateway.execution_engine.get_order(1);
    assert!(order_state.is_some());
    
    Ok(())
}

#[tokio::test]
async fn test_circuit_breaker() -> Result<()> {
    let config = GatewayConfig::default();
    let gateway = Arc::new(TradingGateway::new(config).await?);
    gateway.start().await?;
    
    assert!(!gateway.is_circuit_breaker_tripped());
    
    // Trigger emergency stop
    gateway.emergency_stop().await?;
    
    assert!(gateway.is_circuit_breaker_tripped());
    
    // Should reject new orders
    let order = TradingEvent::OrderRequest {
        id: 1,
        symbol: Symbol(1),
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(10000),
        price: None,
        time_in_force: TimeInForce::Ioc,
        strategy_id: "Test".to_string(),
    };
    
    // Process should be blocked by circuit breaker
    let orderbook = OrderBook::new("BTCUSDT");
    let analytics = MicrostructureAnalytics::new();
    let result = gateway.process_orderbook_update(Symbol(1), &orderbook, &analytics).await;
    assert!(result.is_ok()); // Should succeed but do nothing
    
    Ok(())
}

#[tokio::test]
async fn test_strategy_signals() -> Result<()> {
    let config = GatewayConfig {
        enable_momentum: true,
        enable_market_making: false,
        enable_arbitrage: false,
        ..Default::default()
    };
    
    let gateway = Arc::new(TradingGateway::new(config).await?);
    gateway.start().await?;
    
    // Create market update that should trigger momentum signal
    let event = TradingEvent::MarketUpdate {
        symbol: Symbol(1),
        bid: Some((Px::from_i64(1000000000), Qty::from_i64(10000))),
        ask: Some((Px::from_i64(1000100000), Qty::from_i64(10000))),
        mid: Px::from_i64(1000050000),
        spread: 100000,
        imbalance: 10.0,
        vpin: 30.0,
        kyles_lambda: 0.5,
        timestamp: Ts::now(),
    };
    
    // Process multiple updates to build history
    for _ in 0..50 {
        let strategies = gateway.strategies.read();
        for strategy in strategies.iter() {
            let _ = strategy.on_market_update(&event).await;
        }
        sleep(Duration::from_millis(10)).await;
    }
    
    Ok(())
}

#[tokio::test]
async fn test_position_management() -> Result<()> {
    let config = GatewayConfig::default();
    let gateway = Arc::new(TradingGateway::new(config).await?);
    gateway.start().await?;
    
    let symbol = Symbol(1);
    
    // Update position after buy
    gateway.position_manager.update_position(
        symbol,
        Side::Buy,
        Qty::from_i64(10000),
        Px::from_i64(1000000000),
    ).await?;
    
    let position = gateway.position_manager.get_position(symbol).await;
    assert!(position.is_some());
    
    let pos = position.unwrap();
    assert_eq!(pos.quantity, 10000);
    assert_eq!(pos.avg_entry_price, 1000000000);
    
    // Update market price
    gateway.position_manager.update_market_price(
        symbol,
        Px::from_i64(1001000000),
    ).await;
    
    let position = gateway.position_manager.get_position(symbol).await.unwrap();
    assert!(position.unrealized_pnl > 0); // Should have profit
    
    // Close position
    gateway.position_manager.update_position(
        symbol,
        Side::Sell,
        Qty::from_i64(10000),
        Px::from_i64(1001000000),
    ).await?;
    
    let position = gateway.position_manager.get_position(symbol).await.unwrap();
    assert_eq!(position.quantity, 0);
    assert!(position.realized_pnl > 0);
    
    Ok(())
}

#[tokio::test]
async fn test_signal_aggregation() -> Result<()> {
    let config = GatewayConfig::default();
    let gateway = Arc::new(TradingGateway::new(config).await?);
    gateway.start().await?;
    
    // Send multiple signals
    let signal1 = TradingEvent::Signal {
        id: 1,
        symbol: Symbol(1),
        side: Side::Buy,
        signal_type: SignalType::Momentum,
        strength: 0.8,
        confidence: 0.7,
        timestamp: Ts::now(),
    };
    
    let signal2 = TradingEvent::Signal {
        id: 2,
        symbol: Symbol(1),
        side: Side::Buy,
        signal_type: SignalType::Arbitrage,
        strength: 0.9,
        confidence: 0.85,
        timestamp: Ts::now(),
    };
    
    // Process signals
    let aggregated1 = gateway.signal_aggregator.aggregate(signal1).await?;
    assert!(aggregated1.is_none()); // First signal alone may not meet threshold
    
    let aggregated2 = gateway.signal_aggregator.aggregate(signal2).await?;
    assert!(aggregated2.is_some()); // Combined signals should trigger order
    
    if let Some(TradingEvent::OrderRequest { side, .. }) = aggregated2 {
        assert_eq!(side, Side::Buy);
    }
    
    Ok(())
}

#[tokio::test]
async fn test_end_to_end_flow() -> Result<()> {
    // Complete end-to-end test
    let config = GatewayConfig {
        max_position_size: Qty::from_i64(100000),
        max_daily_loss: 1000000,
        enable_market_making: true,
        enable_momentum: true,
        enable_arbitrage: true,
        ..Default::default()
    };
    
    let gateway = Arc::new(TradingGateway::new(config).await?);
    gateway.start().await?;
    
    // Create orderbook
    let orderbook = OrderBook::new("BTCUSDT");
    let analytics = MicrostructureAnalytics::new();
    let symbol = Symbol(1);
    
    // Simulate market data updates
    for i in 0..10 {
        let price_base = 1000000000 + i * 100000;
        
        let bid_levels = vec![
            (Px::from_i64(price_base), Qty::from_i64(10000 + i * 1000), 1),
            (Px::from_i64(price_base - 100000), Qty::from_i64(20000), 2),
        ];
        let ask_levels = vec![
            (Px::from_i64(price_base + 100000), Qty::from_i64(15000), 1),
            (Px::from_i64(price_base + 200000), Qty::from_i64(25000), 2),
        ];
        
        orderbook.load_snapshot(bid_levels, ask_levels);
        
        // Update analytics
        analytics.update_trade(
            Px::from_i64(price_base + 50000),
            Qty::from_i64(5000),
            i % 2 == 0,
            Ts::now(),
        );
        
        // Process update
        gateway.process_orderbook_update(symbol, &orderbook, &analytics).await?;
        
        sleep(Duration::from_millis(50)).await;
    }
    
    // Check final status
    let status = gateway.get_status().await;
    assert!(status.is_running);
    assert!(status.active_strategies > 0);
    
    // Check metrics
    let risk_metrics = gateway.risk_gate.get_metrics();
    let exec_metrics = gateway.execution_engine.get_metrics();
    let telemetry = gateway.telemetry.get_stats().await;
    
    println!("Risk checks: {}", risk_metrics.orders_checked);
    println!("Orders submitted: {}", exec_metrics.orders_submitted);
    println!("Signals generated: {}", telemetry.signals_generated);
    
    Ok(())
}