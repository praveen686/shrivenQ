//! Unit tests for Orchestrator
//!
//! Comprehensive tests covering:
//! - Event processing and distribution
//! - Event logging and categorization
//! - Event counting and metrics
//! - Thread-safe concurrent event handling
//! - Performance characteristics
//! - Error handling scenarios

use anyhow::Result;
use rstest::*;
use std::sync::Arc;
use tokio::sync::broadcast;
use trading_gateway::{
    orchestrator::Orchestrator, 
    OrderStatus, OrderType, RiskAction, Severity, Side, SignalType, TradingEvent
};
use services_common::{Px, Qty, Symbol, Ts};

/// Test fixture for creating an Orchestrator with event bus
#[fixture]
fn orchestrator_setup() -> (Orchestrator, broadcast::Receiver<TradingEvent>) {
    let (event_tx, event_rx) = broadcast::channel(1000);
    let orchestrator = Orchestrator::new(Arc::new(event_tx));
    (orchestrator, event_rx)
}

/// Test fixture for creating a market update event
#[fixture]
fn market_update_event() -> TradingEvent {
    TradingEvent::MarketUpdate {
        symbol: Symbol(1),
        bid: Some((Px::from_i64(1000000000), Qty::from_i64(10000))),
        ask: Some((Px::from_i64(1001000000), Qty::from_i64(12000))),
        mid: Px::from_i64(1000500000),
        spread: 100000,
        imbalance: 15.0,
        vpin: 25.0,
        kyles_lambda: 0.4,
        timestamp: Ts::now(),
    }
}

/// Test fixture for creating a trading signal event
#[fixture]
fn signal_event() -> TradingEvent {
    TradingEvent::Signal {
        id: 42,
        symbol: Symbol(2),
        side: Side::Buy,
        signal_type: SignalType::Momentum,
        strength: 0.75,
        confidence: 0.85,
        timestamp: Ts::now(),
    }
}

/// Test fixture for creating an order request event
#[fixture]
fn order_request_event() -> TradingEvent {
    TradingEvent::OrderRequest {
        id: 123,
        symbol: Symbol(3),
        side: Side::Sell,
        order_type: OrderType::Limit,
        quantity: Qty::from_i64(25000),
        price: Some(Px::from_i64(2000000000)),
        time_in_force: trading_gateway::TimeInForce::Gtc,
        strategy_id: "momentum_strategy".to_string(),
    }
}

/// Test fixture for creating an execution report event
#[fixture]
fn execution_report_event() -> TradingEvent {
    TradingEvent::ExecutionReport {
        order_id: 456,
        symbol: Symbol(4),
        side: Side::Buy,
        executed_qty: Qty::from_i64(15000),
        executed_price: Px::from_i64(1500000000),
        remaining_qty: Qty::from_i64(5000),
        status: OrderStatus::PartiallyFilled,
        timestamp: Ts::now(),
    }
}

/// Test fixture for creating a risk alert event
#[fixture]
fn risk_alert_event() -> TradingEvent {
    TradingEvent::RiskAlert {
        severity: Severity::Warning,
        message: "Position limit approaching".to_string(),
        action: RiskAction::ReducePosition,
        timestamp: Ts::now(),
    }
}

#[rstest]
#[tokio::test]
async fn test_orchestrator_creation(orchestrator_setup: (Orchestrator, broadcast::Receiver<TradingEvent>)) {
    let (orchestrator, _) = orchestrator_setup;
    
    // Test initial state
    assert_eq!(orchestrator.get_events_processed(), 0);
}

#[rstest]
#[tokio::test]
async fn test_market_update_processing(
    orchestrator_setup: (Orchestrator, broadcast::Receiver<TradingEvent>),
    market_update_event: TradingEvent
) -> Result<()> {
    let (orchestrator, mut event_rx) = orchestrator_setup;
    
    // Process market update event
    orchestrator.process_event(market_update_event.clone()).await?;
    
    // Verify event was processed
    assert_eq!(orchestrator.get_events_processed(), 1);
    
    // Verify event was broadcast
    let received_event = event_rx.recv().await?;
    
    // Match the event type and key fields
    if let (
        TradingEvent::MarketUpdate { symbol: orig_sym, mid: orig_mid, .. },
        TradingEvent::MarketUpdate { symbol: recv_sym, mid: recv_mid, .. }
    ) = (&market_update_event, &received_event) {
        assert_eq!(orig_sym, recv_sym);
        assert_eq!(orig_mid, recv_mid);
    } else {
        panic!("Event type mismatch");
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_signal_processing(
    orchestrator_setup: (Orchestrator, broadcast::Receiver<TradingEvent>),
    signal_event: TradingEvent
) -> Result<()> {
    let (orchestrator, mut event_rx) = orchestrator_setup;
    
    orchestrator.process_event(signal_event.clone()).await?;
    
    assert_eq!(orchestrator.get_events_processed(), 1);
    
    let received_event = event_rx.recv().await?;
    
    if let (
        TradingEvent::Signal { id: orig_id, signal_type: orig_type, .. },
        TradingEvent::Signal { id: recv_id, signal_type: recv_type, .. }
    ) = (&signal_event, &received_event) {
        assert_eq!(orig_id, recv_id);
        assert_eq!(orig_type, recv_type);
    } else {
        panic!("Signal event type mismatch");
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_order_request_processing(
    orchestrator_setup: (Orchestrator, broadcast::Receiver<TradingEvent>),
    order_request_event: TradingEvent
) -> Result<()> {
    let (orchestrator, mut event_rx) = orchestrator_setup;
    
    orchestrator.process_event(order_request_event.clone()).await?;
    
    assert_eq!(orchestrator.get_events_processed(), 1);
    
    let received_event = event_rx.recv().await?;
    
    if let (
        TradingEvent::OrderRequest { id: orig_id, symbol: orig_sym, .. },
        TradingEvent::OrderRequest { id: recv_id, symbol: recv_sym, .. }
    ) = (&order_request_event, &received_event) {
        assert_eq!(orig_id, recv_id);
        assert_eq!(orig_sym, recv_sym);
    } else {
        panic!("Order request event type mismatch");
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_execution_report_processing(
    orchestrator_setup: (Orchestrator, broadcast::Receiver<TradingEvent>),
    execution_report_event: TradingEvent
) -> Result<()> {
    let (orchestrator, mut event_rx) = orchestrator_setup;
    
    orchestrator.process_event(execution_report_event.clone()).await?;
    
    assert_eq!(orchestrator.get_events_processed(), 1);
    
    let received_event = event_rx.recv().await?;
    
    if let (
        TradingEvent::ExecutionReport { order_id: orig_id, status: orig_status, .. },
        TradingEvent::ExecutionReport { order_id: recv_id, status: recv_status, .. }
    ) = (&execution_report_event, &received_event) {
        assert_eq!(orig_id, recv_id);
        assert!(matches!(orig_status, OrderStatus::PartiallyFilled));
        assert!(matches!(recv_status, OrderStatus::PartiallyFilled));
    } else {
        panic!("Execution report event type mismatch");
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_risk_alert_processing(
    orchestrator_setup: (Orchestrator, broadcast::Receiver<TradingEvent>),
    risk_alert_event: TradingEvent
) -> Result<()> {
    let (orchestrator, mut event_rx) = orchestrator_setup;
    
    orchestrator.process_event(risk_alert_event.clone()).await?;
    
    assert_eq!(orchestrator.get_events_processed(), 1);
    
    let received_event = event_rx.recv().await?;
    
    if let (
        TradingEvent::RiskAlert { severity: orig_sev, message: orig_msg, .. },
        TradingEvent::RiskAlert { severity: recv_sev, message: recv_msg, .. }
    ) = (&risk_alert_event, &received_event) {
        assert!(matches!(orig_sev, Severity::Warning));
        assert!(matches!(recv_sev, Severity::Warning));
        assert_eq!(orig_msg, recv_msg);
    } else {
        panic!("Risk alert event type mismatch");
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_multiple_event_processing(
    orchestrator_setup: (Orchestrator, broadcast::Receiver<TradingEvent>),
    market_update_event: TradingEvent,
    signal_event: TradingEvent,
    order_request_event: TradingEvent,
    execution_report_event: TradingEvent,
    risk_alert_event: TradingEvent
) -> Result<()> {
    let (orchestrator, mut event_rx) = orchestrator_setup;
    
    let events = vec![
        market_update_event,
        signal_event,
        order_request_event,
        execution_report_event,
        risk_alert_event,
    ];
    
    // Process all events
    for event in &events {
        orchestrator.process_event(event.clone()).await?;
    }
    
    // Verify counter
    assert_eq!(orchestrator.get_events_processed(), 5);
    
    // Verify all events were broadcast
    for expected_event in &events {
        let received_event = event_rx.recv().await?;
        
        // Verify event types match (simplified check)
        match (expected_event, &received_event) {
            (TradingEvent::MarketUpdate { .. }, TradingEvent::MarketUpdate { .. }) => {},
            (TradingEvent::Signal { .. }, TradingEvent::Signal { .. }) => {},
            (TradingEvent::OrderRequest { .. }, TradingEvent::OrderRequest { .. }) => {},
            (TradingEvent::ExecutionReport { .. }, TradingEvent::ExecutionReport { .. }) => {},
            (TradingEvent::RiskAlert { .. }, TradingEvent::RiskAlert { .. }) => {},
            _ => panic!("Event type mismatch in sequence"),
        }
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_concurrent_event_processing() -> Result<()> {
    let (event_tx, mut event_rx) = broadcast::channel(1000);
    let orchestrator = Arc::new(Orchestrator::new(Arc::new(event_tx)));
    let mut handles = Vec::new();
    
    // Process events concurrently
    for i in 1..=50 {
        let orch = orchestrator.clone();
        let handle = tokio::spawn(async move {
            let event = TradingEvent::MarketUpdate {
                symbol: Symbol(i % 10 + 1), // Cycle through symbols
                bid: Some((Px::from_i64(1000000000 + i * 1000), Qty::from_i64(10000))),
                ask: Some((Px::from_i64(1001000000 + i * 1000), Qty::from_i64(10000))),
                mid: Px::from_i64(1000500000 + i * 1000),
                spread: 100000,
                imbalance: (i as f64) * 0.5,
                vpin: (i as f64) * 0.3,
                kyles_lambda: 0.4,
                timestamp: Ts::now(),
            };
            orch.process_event(event).await
        });
        handles.push(handle);
    }
    
    // Wait for all processing to complete
    for handle in handles {
        handle.await??;
    }
    
    // Verify all events were processed
    assert_eq!(orchestrator.get_events_processed(), 50);
    
    // Verify events were broadcast (read some of them)
    for _ in 0..10 {
        let received_event = event_rx.recv().await;
        assert!(received_event.is_ok());
        
        if let Ok(TradingEvent::MarketUpdate { .. }) = received_event {
            // Expected event type
        } else {
            panic!("Unexpected event type received");
        }
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_event_counter_accuracy(
    orchestrator_setup: (Orchestrator, broadcast::Receiver<TradingEvent>)
) -> Result<()> {
    let (orchestrator, _) = orchestrator_setup;
    
    // Process events in batches and verify counter
    for batch in 1..=5 {
        for i in 1..=10 {
            let event = TradingEvent::Signal {
                id: (batch * 10 + i) as u64,
                symbol: Symbol(i as u32),
                side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
                signal_type: SignalType::Momentum,
                strength: 0.5,
                confidence: 0.7,
                timestamp: Ts::now(),
            };
            
            orchestrator.process_event(event).await?;
        }
        
        // Verify counter after each batch
        let expected_count = (batch * 10) as u64;
        assert_eq!(orchestrator.get_events_processed(), expected_count);
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_performance_characteristics(
    orchestrator_setup: (Orchestrator, broadcast::Receiver<TradingEvent>)
) -> Result<()> {
    let (orchestrator, _) = orchestrator_setup;
    
    let start = std::time::Instant::now();
    
    // Process many events rapidly
    for i in 1..=1000 {
        let event = TradingEvent::MarketUpdate {
            symbol: Symbol((i % 20) as u32 + 1),
            bid: Some((Px::from_i64(1000000000), Qty::from_i64(10000))),
            ask: Some((Px::from_i64(1001000000), Qty::from_i64(10000))),
            mid: Px::from_i64(1000500000),
            spread: 100000,
            imbalance: 0.0,
            vpin: 20.0,
            kyles_lambda: 0.4,
            timestamp: Ts::now(),
        };
        
        orchestrator.process_event(event).await?;
    }
    
    let duration = start.elapsed();
    
    // Should process 1000 events very quickly
    assert!(duration < std::time::Duration::from_millis(100), 
        "Event processing should be fast");
    
    assert_eq!(orchestrator.get_events_processed(), 1000);
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_broadcast_failure_handling(
    market_update_event: TradingEvent
) -> Result<()> {
    // Create orchestrator with very small buffer to test overflow
    let (event_tx, _event_rx) = broadcast::channel(1);
    let orchestrator = Orchestrator::new(Arc::new(event_tx));
    
    // Drop the receiver to cause broadcast failures
    drop(_event_rx);
    
    // Process events - should handle broadcast failures gracefully
    for i in 1..=5 {
        let mut event = market_update_event.clone();
        if let TradingEvent::MarketUpdate { ref mut symbol, .. } = event {
            *symbol = Symbol(i as u32);
        }
        
        // Should not panic even if broadcast fails
        let result = orchestrator.process_event(event).await;
        assert!(result.is_ok(), "Should handle broadcast failures gracefully");
    }
    
    // Counter should still work even if broadcasts fail
    assert_eq!(orchestrator.get_events_processed(), 5);
    
    Ok(())
}

#[rstest]
#[case(
    TradingEvent::MarketUpdate {
        symbol: Symbol(1),
        bid: Some((Px::from_i64(1000000000), Qty::from_i64(10000))),
        ask: Some((Px::from_i64(1001000000), Qty::from_i64(10000))),
        mid: Px::from_i64(1000500000),
        spread: 100000,
        imbalance: 0.0,
        vpin: 20.0,
        kyles_lambda: 0.4,
        timestamp: Ts::now(),
    }
)]
#[case(
    TradingEvent::Signal {
        id: 1,
        symbol: Symbol(1),
        side: Side::Buy,
        signal_type: SignalType::Arbitrage,
        strength: 0.8,
        confidence: 0.9,
        timestamp: Ts::now(),
    }
)]
#[case(
    TradingEvent::OrderRequest {
        id: 1,
        symbol: Symbol(1),
        side: Side::Sell,
        order_type: OrderType::Market,
        quantity: Qty::from_i64(10000),
        price: None,
        time_in_force: trading_gateway::TimeInForce::Ioc,
        strategy_id: "test".to_string(),
    }
)]
#[case(
    TradingEvent::ExecutionReport {
        order_id: 1,
        symbol: Symbol(1),
        side: Side::Buy,
        executed_qty: Qty::from_i64(10000),
        executed_price: Px::from_i64(1000000000),
        remaining_qty: Qty::ZERO,
        status: OrderStatus::Filled,
        timestamp: Ts::now(),
    }
)]
#[case(
    TradingEvent::RiskAlert {
        severity: Severity::Critical,
        message: "Test alert".to_string(),
        action: RiskAction::HaltTrading,
        timestamp: Ts::now(),
    }
)]
#[tokio::test]
async fn test_all_event_types_parameterized(
    orchestrator_setup: (Orchestrator, broadcast::Receiver<TradingEvent>),
    #[case] event: TradingEvent
) -> Result<()> {
    let (orchestrator, mut event_rx) = orchestrator_setup;
    
    orchestrator.process_event(event.clone()).await?;
    
    assert_eq!(orchestrator.get_events_processed(), 1);
    
    let received_event = event_rx.recv().await?;
    
    // Verify event type matches
    match (&event, &received_event) {
        (TradingEvent::MarketUpdate { .. }, TradingEvent::MarketUpdate { .. }) => {},
        (TradingEvent::Signal { .. }, TradingEvent::Signal { .. }) => {},
        (TradingEvent::OrderRequest { .. }, TradingEvent::OrderRequest { .. }) => {},
        (TradingEvent::ExecutionReport { .. }, TradingEvent::ExecutionReport { .. }) => {},
        (TradingEvent::RiskAlert { .. }, TradingEvent::RiskAlert { .. }) => {},
        _ => panic!("Event type mismatch for parameterized test"),
    }
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_high_frequency_event_stream(
    orchestrator_setup: (Orchestrator, broadcast::Receiver<TradingEvent>)
) -> Result<()> {
    let (orchestrator, mut event_rx) = orchestrator_setup;
    
    // Simulate high-frequency market data stream
    let start = std::time::Instant::now();
    let event_count = 10000;
    
    for i in 1..=event_count {
        let event = TradingEvent::MarketUpdate {
            symbol: Symbol((i % 100) as u32 + 1), // 100 symbols
            bid: Some((Px::from_i64(1000000000 + (i % 1000) * 1000), Qty::from_i64(10000 + i % 5000))),
            ask: Some((Px::from_i64(1001000000 + (i % 1000) * 1000), Qty::from_i64(10000 + i % 5000))),
            mid: Px::from_i64(1000500000 + (i % 1000) * 1000),
            spread: 100000,
            imbalance: (i as f64 % 100.0) - 50.0, // -50 to +50
            vpin: (i as f64 % 80.0), // 0 to 80
            kyles_lambda: ((i % 10) as f64) / 10.0,
            timestamp: Ts::now(),
        };
        
        orchestrator.process_event(event).await?;
    }
    
    let duration = start.elapsed();
    
    // Verify performance (should handle high-frequency stream)
    assert!(duration < std::time::Duration::from_secs(1), 
        "Should handle high-frequency stream efficiently");
    
    assert_eq!(orchestrator.get_events_processed(), event_count);
    
    // Sample some events from the stream
    for _ in 0..100 {
        let received = event_rx.recv().await?;
        assert!(matches!(received, TradingEvent::MarketUpdate { .. }));
    }
    
    Ok(())
}