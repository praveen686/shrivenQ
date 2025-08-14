//! Integration tests for reporting service with event bus

use anyhow::Result;
use common::{Px, Qty, Symbol, Ts};
use reporting::*;
use services_common::event_bus::*;
use std::sync::Arc;
use tokio::time::{Duration, timeout};

#[derive(Debug, Clone)]
struct TestMessage {
    id: u64,
    content: String,
}

impl BusMessage for TestMessage {
    fn topic(&self) -> &str {
        "test_reports"
    }
}

#[tokio::test]
async fn test_reporting_service_integration() -> Result<()> {
    // Create reporting service
    let config = ReportingConfig::default();
    let reporting_service = ReportingServiceImpl::new(config);

    // Start monitoring
    reporting_service.start_monitoring().await?;

    // Test basic fill recording
    let symbol = Symbol::new(1);
    reporting_service
        .record_fill(
            1,
            symbol,
            Qty::from_i64(1000000), // 100 units
            Px::from_i64(1000000),  // $100
            Ts::now(),
        )
        .await?;

    // Test market update
    reporting_service
        .update_market(
            symbol,
            Px::from_i64(990000),  // $99
            Px::from_i64(1010000), // $101
            Ts::now(),
        )
        .await?;

    // Get metrics
    let metrics = reporting_service.get_metrics().await?;
    assert_eq!(metrics.total_trades, 1);
    assert!(metrics.total_volume > 0);
    assert!(!metrics.symbol_breakdown.is_empty());

    // Get performance report
    let report = reporting_service.get_performance_report().await?;
    assert_eq!(report.total_trades, 1);
    assert!(report.total_volume > 0);

    Ok(())
}

#[tokio::test]
async fn test_event_bus_with_reporting_messages() -> Result<()> {
    // Create event bus
    let config = EventBusConfig::default();
    let bus = Arc::new(EventBus::<ShrivenQuantMessage>::new(config));

    // Subscribe to performance metrics topic
    let mut subscriber = bus.subscribe("performance").await.unwrap();

    // Create a performance metrics message
    let perf_message = ShrivenQuantMessage::PerformanceMetrics {
        service: "reporting".to_string(),
        metric_name: "sharpe_ratio".to_string(),
        value: 2.5,
        unit: "ratio".to_string(),
        tags: [("symbol".to_string(), "BTCUSDT".to_string())]
            .into_iter()
            .collect(),
        timestamp: Ts::now().nanos(),
    };

    // Publish message
    bus.publish(perf_message.clone()).await.unwrap();

    // Receive message with timeout
    let received = timeout(Duration::from_millis(100), subscriber.recv())
        .await
        .expect("Timeout waiting for message")
        .expect("Failed to receive message");

    // Verify message content
    if let ShrivenQuantMessage::PerformanceMetrics {
        service,
        metric_name,
        value,
        ..
    } = received.message
    {
        assert_eq!(service, "reporting");
        assert_eq!(metric_name, "sharpe_ratio");
        assert_eq!(value, 2.5);
    } else {
        panic!("Expected PerformanceMetrics message");
    }

    Ok(())
}

#[tokio::test]
async fn test_reporting_with_event_bus_alerts() -> Result<()> {
    // Create event bus and reporting service
    let bus_config = EventBusConfig::default();
    let bus = Arc::new(EventBus::<ShrivenQuantMessage>::new(bus_config));

    let reporting_config = ReportingConfig {
        alert_thresholds: AlertThresholds {
            max_drawdown_bp: 500, // 5% drawdown threshold
            min_sharpe_ratio: 1.0,
            max_daily_loss: 50000, // $500 loss threshold
        },
        ..Default::default()
    };
    let reporting_service = ReportingServiceImpl::new(reporting_config);

    // Subscribe to risk alerts
    let mut alert_subscriber = bus.subscribe("risk_alerts").await.unwrap();

    // Start monitoring
    reporting_service.start_monitoring().await?;

    // Subscribe to reporting events to simulate alert generation
    let mut event_receiver = reporting_service.subscribe_events().await?;

    // Record a large losing trade to trigger alerts
    let symbol = Symbol::new(1);
    reporting_service
        .record_fill(
            1,
            symbol,
            Qty::from_i64(-1000000), // Sell 100 units
            Px::from_i64(500000),    // At $50 (loss if bought at higher price)
            Ts::now(),
        )
        .await?;

    // Check for reporting events (this will contain alerts)
    let event = timeout(Duration::from_millis(200), event_receiver.recv()).await;

    // Event should have been received (either MetricsUpdated or Alert)
    assert!(event.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_simd_metrics_performance() -> Result<()> {
    // Create metrics engine with large buffer for SIMD testing
    let metrics_engine = reporting::metrics::MetricsEngine::new(10000);

    let symbol = Symbol::new(1);
    let start_time = std::time::Instant::now();

    // Record many fills to test SIMD performance
    for i in 0..1000 {
        metrics_engine.record_fill(
            i,
            symbol,
            Qty::from_i64(1000000 + i as i64 * 100), // Varying quantities
            Px::from_i64(1000000 + i as i64 * 10),   // Varying prices
            Ts::now(),
        );

        // Update market data
        metrics_engine.update_market(
            symbol,
            Px::from_i64(999000 + i as i64 * 10),
            Px::from_i64(1001000 + i as i64 * 10),
            Ts::now(),
        );
    }

    let elapsed = start_time.elapsed();
    println!("Processed 1000 fills + market updates in {:?}", elapsed);

    // Calculate Sharpe ratio (will use SIMD if available)
    let sharpe_start = std::time::Instant::now();
    let sharpe_ratio = metrics_engine.calculate_sharpe();
    let sharpe_elapsed = sharpe_start.elapsed();

    println!("Sharpe ratio calculation took {:?}", sharpe_elapsed);
    println!("Sharpe ratio: {:.3}", sharpe_ratio);

    // Get comprehensive metrics
    let metrics = metrics_engine.get_metrics();
    assert_eq!(metrics.total_trades, 1000);
    assert!(metrics.total_volume > 0);
    assert!(sharpe_ratio.is_finite());

    // Verify performance (should be very fast)
    assert!(
        elapsed < Duration::from_millis(100),
        "Metrics processing too slow"
    );
    assert!(
        sharpe_elapsed < Duration::from_millis(10),
        "Sharpe calculation too slow"
    );

    Ok(())
}

#[tokio::test]
async fn test_event_bus_performance() -> Result<()> {
    // Create high-performance event bus
    let config = EventBusConfig {
        capacity: 100000,
        ..Default::default()
    };
    let bus = Arc::new(EventBus::<ShrivenQuantMessage>::new(config));

    // Subscribe to market data
    let mut subscriber = bus.subscribe("market_data").await.unwrap();

    let start_time = std::time::Instant::now();

    // Publish many market data messages
    for i in 0..1000 {
        let message = ShrivenQuantMessage::MarketData {
            symbol: format!("SYMBOL{}", i % 10),
            exchange: "binance".to_string(),
            bid: 50000000 + i as i64,
            ask: 50000100 + i as i64,
            timestamp: Ts::now().nanos(),
        };

        bus.publish(message).await.unwrap();
    }

    let publish_elapsed = start_time.elapsed();
    println!(
        "Published 1000 market data messages in {:?}",
        publish_elapsed
    );

    // Receive messages
    let mut received_count = 0;
    let receive_start = std::time::Instant::now();

    while received_count < 1000 {
        if timeout(Duration::from_millis(10), subscriber.recv())
            .await
            .is_ok()
        {
            received_count += 1;
        } else {
            break; // Timeout, stop receiving
        }
    }

    let receive_elapsed = receive_start.elapsed();
    println!(
        "Received {} messages in {:?}",
        received_count, receive_elapsed
    );

    // Verify performance
    assert!(
        publish_elapsed < Duration::from_millis(500),
        "Publishing too slow"
    );
    assert!(received_count >= 990, "Didn't receive enough messages"); // Allow for some message loss

    // Check bus metrics
    let metrics = bus.metrics();
    assert!(metrics.get_publish_count("market_data") > 0);

    Ok(())
}

#[tokio::test]
async fn test_full_integration_with_all_message_types() -> Result<()> {
    // Create event bus and reporting service
    let config = EventBusConfig {
        enable_metrics: true,
        enable_dead_letter_queue: true,
        ..Default::default()
    };
    let bus = Arc::new(EventBus::<ShrivenQuantMessage>::new(config));
    let reporting_service = ReportingServiceImpl::new(ReportingConfig::default());

    // Subscribe to all relevant topics
    let mut market_data_rx = bus.subscribe("market_data").await.unwrap();
    let mut orders_rx = bus.subscribe("orders").await.unwrap();
    let mut fills_rx = bus.subscribe("fills").await.unwrap();
    let mut performance_rx = bus.subscribe("performance").await.unwrap();

    // Start reporting monitoring
    reporting_service.start_monitoring().await?;

    // Publish different types of messages

    // 1. Market data
    let market_msg = ShrivenQuantMessage::MarketData {
        symbol: "BTCUSDT".to_string(),
        exchange: "binance".to_string(),
        bid: 50000000,
        ask: 50001000,
        timestamp: Ts::now().nanos(),
    };
    bus.publish(market_msg).await.unwrap();

    // 2. Order event
    let order_msg = ShrivenQuantMessage::OrderEvent {
        order_id: 12345,
        symbol: "BTCUSDT".to_string(),
        side: "BUY".to_string(),
        quantity: 1000000, // 100 units
        price: 50000000,   // $5000
        status: "FILLED".to_string(),
        timestamp: Ts::now().nanos(),
    };
    bus.publish(order_msg).await.unwrap();

    // 3. Fill event
    let fill_msg = ShrivenQuantMessage::FillEvent {
        order_id: 12345,
        fill_id: "fill_001".to_string(),
        symbol: "BTCUSDT".to_string(),
        quantity: 1000000,
        price: 50000000,
        timestamp: Ts::now().nanos(),
    };
    bus.publish(fill_msg).await.unwrap();

    // 4. Performance metrics from reporting service
    reporting_service
        .record_fill(
            12345,
            Symbol::new(1),
            Qty::from_i64(1000000),
            Px::from_i64(50000000),
            Ts::now(),
        )
        .await?;

    // Verify messages are received
    let market_received = timeout(Duration::from_millis(100), market_data_rx.recv()).await;
    let order_received = timeout(Duration::from_millis(100), orders_rx.recv()).await;
    let fill_received = timeout(Duration::from_millis(100), fills_rx.recv()).await;

    assert!(market_received.is_ok(), "Market data not received");
    assert!(order_received.is_ok(), "Order event not received");
    assert!(fill_received.is_ok(), "Fill event not received");

    // Check reporting service has processed the data
    let metrics = reporting_service.get_metrics().await?;
    assert_eq!(metrics.total_trades, 1);

    // Check event bus metrics
    let bus_metrics = bus.metrics();
    let total_published = bus_metrics.get_publish_count("market_data")
        + bus_metrics.get_publish_count("orders")
        + bus_metrics.get_publish_count("fills");
    assert!(total_published >= 3);

    println!("Integration test completed successfully!");
    println!("Bus published: {}", total_published);
    println!(
        "Market data messages: {}",
        bus_metrics.get_publish_count("market_data")
    );
    println!("Reporting trades: {}", metrics.total_trades);
    println!("Reporting volume: {}", metrics.total_volume);

    Ok(())
}
