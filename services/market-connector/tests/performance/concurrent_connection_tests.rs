//! Concurrent connection performance tests
//! 
//! These tests verify system behavior under concurrent load from multiple
//! connections, streams, and simultaneous operations.

use rstest::*;
use tokio::time::{Duration, Instant, sleep};
use tokio::sync::{mpsc, Semaphore};
use tokio_stream::StreamExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use futures::future::join_all;

use market_connector::grpc_service::MarketDataGrpcService;
use market_connector::{MarketDataEvent, MarketData};
use services_common::marketdata::v1::{
    market_data_service_server::MarketDataService,
    SubscribeRequest
};
use tonic::Request;

// Concurrency test constants
const CONCURRENT_CONNECTIONS: usize = 100;
const CONCURRENT_STREAMS_PER_CONNECTION: usize = 5;
const EVENTS_PER_STREAM: usize = 1000;
const MAX_CONCURRENT_OPERATIONS: usize = 50;
const CONNECTION_TIMEOUT_SECS: u64 = 10;

// Test data constants
const CONCURRENT_TEST_EXCHANGE: &str = "binance";
const BASE_SYMBOL_PREFIX: &str = "TEST_SYMBOL_";

/// Concurrency performance metrics
#[derive(Debug, Clone)]
struct ConcurrencyMetrics {
    successful_connections: AtomicUsize,
    failed_connections: AtomicUsize,
    total_events_sent: AtomicU64,
    total_events_received: AtomicU64,
    connection_errors: AtomicUsize,
    stream_errors: AtomicUsize,
    start_time: Instant,
}

impl ConcurrencyMetrics {
    fn new() -> Self {
        Self {
            successful_connections: AtomicUsize::new(0),
            failed_connections: AtomicUsize::new(0),
            total_events_sent: AtomicU64::new(0),
            total_events_received: AtomicU64::new(0),
            connection_errors: AtomicUsize::new(0),
            stream_errors: AtomicUsize::new(0),
            start_time: Instant::now(),
        }
    }
    
    fn record_successful_connection(&self) {
        self.successful_connections.fetch_add(1, Ordering::Relaxed);
    }
    
    fn record_failed_connection(&self) {
        self.failed_connections.fetch_add(1, Ordering::Relaxed);
    }
    
    fn record_event_sent(&self) {
        self.total_events_sent.fetch_add(1, Ordering::Relaxed);
    }
    
    fn record_event_received(&self) {
        self.total_events_received.fetch_add(1, Ordering::Relaxed);
    }
    
    fn record_connection_error(&self) {
        self.connection_errors.fetch_add(1, Ordering::Relaxed);
    }
    
    fn record_stream_error(&self) {
        self.stream_errors.fetch_add(1, Ordering::Relaxed);
    }
    
    fn get_summary(&self) -> ConcurrencySummary {
        let duration = self.start_time.elapsed();
        ConcurrencySummary {
            successful_connections: self.successful_connections.load(Ordering::Relaxed),
            failed_connections: self.failed_connections.load(Ordering::Relaxed),
            events_sent: self.total_events_sent.load(Ordering::Relaxed),
            events_received: self.total_events_received.load(Ordering::Relaxed),
            connection_errors: self.connection_errors.load(Ordering::Relaxed),
            stream_errors: self.stream_errors.load(Ordering::Relaxed),
            test_duration: duration,
            throughput_events_per_sec: if duration.as_secs() > 0 {
                self.total_events_received.load(Ordering::Relaxed) / duration.as_secs()
            } else {
                0
            },
        }
    }
}

#[derive(Debug)]
struct ConcurrencySummary {
    successful_connections: usize,
    failed_connections: usize,
    events_sent: u64,
    events_received: u64,
    connection_errors: usize,
    stream_errors: usize,
    test_duration: Duration,
    throughput_events_per_sec: u64,
}

impl ConcurrencySummary {
    fn print_summary(&self, test_name: &str) {
        println!("\n=== {} Concurrency Summary ===", test_name);
        println!("Test duration: {:?}", self.test_duration);
        println!("Successful connections: {}", self.successful_connections);
        println!("Failed connections: {}", self.failed_connections);
        println!("Events sent: {}", self.events_sent);
        println!("Events received: {}", self.events_received);
        println!("Connection errors: {}", self.connection_errors);
        println!("Stream errors: {}", self.stream_errors);
        println!("Throughput: {} events/sec", self.throughput_events_per_sec);
        println!("Success rate: {:.2}%", 
                self.successful_connections as f64 / (self.successful_connections + self.failed_connections) as f64 * 100.0);
        println!("=====================================\n");
    }
}

#[rstest]
#[tokio::test]
async fn test_concurrent_grpc_subscriptions() {
    let (service, event_sender) = MarketDataGrpcService::new();
    let service = Arc::new(service);
    let metrics = Arc::new(ConcurrencyMetrics::new());
    
    // Semaphore to limit concurrent operations
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_OPERATIONS));
    let mut subscription_handles = Vec::new();
    
    // Create concurrent subscriptions
    for i in 0..CONCURRENT_CONNECTIONS {
        let service_clone = Arc::clone(&service);
        let metrics_clone = Arc::clone(&metrics);
        let semaphore_clone = Arc::clone(&semaphore);
        let symbol = format!("{}{}", BASE_SYMBOL_PREFIX, i);
        
        let handle = tokio::spawn(async move {
            // Acquire semaphore permit
            let _permit = semaphore_clone.acquire().await.unwrap();
            
            let subscribe_request = SubscribeRequest {
                symbols: vec![symbol.clone()],
                exchange: CONCURRENT_TEST_EXCHANGE.to_string(),
                data_types: vec![1, 2, 3],
            };
            
            let request = Request::new(subscribe_request);
            match service_clone.subscribe(request).await {
                Ok(response) => {
                    metrics_clone.record_successful_connection();
                    
                    let mut stream = response.into_inner();
                    let mut received_count = 0;
                    
                    // Try to receive events from stream
                    while let Some(result) = tokio::time::timeout(
                        Duration::from_millis(100),
                        stream.next()
                    ).await.ok().flatten() {
                        match result {
                            Ok(_event) => {
                                metrics_clone.record_event_received();
                                received_count += 1;
                                if received_count >= 5 {
                                    break; // Don't wait too long per stream
                                }
                            },
                            Err(_) => {
                                metrics_clone.record_stream_error();
                                break;
                            }
                        }
                    }
                },
                Err(_) => {
                    metrics_clone.record_failed_connection();
                }
            }
        });
        
        subscription_handles.push(handle);
    }
    
    // Send events for all symbols concurrently
    let event_sender_task = tokio::spawn({
        let metrics = Arc::clone(&metrics);
        async move {
            for i in 0..EVENTS_PER_STREAM {
                for conn_id in 0..CONCURRENT_CONNECTIONS {
                    let symbol = format!("{}{}", BASE_SYMBOL_PREFIX, conn_id);
                    let event = MarketDataEvent {
                        symbol,
                        exchange: CONCURRENT_TEST_EXCHANGE.to_string(),
                        timestamp: chrono::Utc::now().timestamp_millis() as u64 + i as u64,
                        data: MarketData::OrderBook {
                            bids: vec![(45000.0 + (i % 100) as f64 * 0.01, 1.0)],
                            asks: vec![(45001.0 + (i % 100) as f64 * 0.01, 1.0)],
                            sequence: i as u64,
                        },
                    };
                    
                    if event_sender.send(event).await.is_err() {
                        break;
                    }
                    
                    metrics.record_event_sent();
                }
                
                // Small delay between batches
                if i % 10 == 0 {
                    tokio::task::yield_now().await;
                }
            }
        }
    });
    
    // Wait for all subscriptions to complete
    let subscription_results = join_all(subscription_handles).await;
    
    // Wait for event sending to complete
    let _ = tokio::time::timeout(Duration::from_secs(5), event_sender_task).await;
    
    let summary = metrics.get_summary();
    summary.print_summary("Concurrent gRPC Subscriptions");
    
    // Performance assertions
    let success_rate = summary.successful_connections as f64 / CONCURRENT_CONNECTIONS as f64;
    assert!(success_rate > 0.8, "Success rate too low: {:.2}%", success_rate * 100.0);
    
    assert!(summary.connection_errors < CONCURRENT_CONNECTIONS / 10, 
           "Too many connection errors: {}", summary.connection_errors);
}

#[rstest]
#[tokio::test]
async fn test_concurrent_stream_processing() {
    let (service, event_sender) = MarketDataGrpcService::new();
    let service = Arc::new(service);
    let metrics = Arc::new(ConcurrencyMetrics::new());
    
    // Create multiple streams per connection
    let total_streams = CONCURRENT_CONNECTIONS * CONCURRENT_STREAMS_PER_CONNECTION;
    let mut stream_handles = Vec::new();
    
    for i in 0..total_streams {
        let service_clone = Arc::clone(&service);
        let metrics_clone = Arc::clone(&metrics);
        let symbol = format!("{}_{}", BASE_SYMBOL_PREFIX, i / CONCURRENT_STREAMS_PER_CONNECTION);
        let stream_id = i % CONCURRENT_STREAMS_PER_CONNECTION;
        
        let handle = tokio::spawn(async move {
            let subscribe_request = SubscribeRequest {
                symbols: vec![format!("{}_{}", symbol, stream_id)],
                exchange: CONCURRENT_TEST_EXCHANGE.to_string(),
                data_types: vec![1 + (stream_id % 3) as i32], // Different data types
            };
            
            let request = Request::new(subscribe_request);
            match service_clone.subscribe(request).await {
                Ok(response) => {
                    metrics_clone.record_successful_connection();
                    
                    let mut stream = response.into_inner();
                    let timeout_duration = Duration::from_secs(CONNECTION_TIMEOUT_SECS);
                    let start_time = Instant::now();
                    
                    while start_time.elapsed() < timeout_duration {
                        match tokio::time::timeout(
                            Duration::from_millis(50),
                            stream.next()
                        ).await {
                            Ok(Some(Ok(_event))) => {
                                metrics_clone.record_event_received();
                            },
                            Ok(Some(Err(_))) => {
                                metrics_clone.record_stream_error();
                                break;
                            },
                            Ok(None) => break, // Stream ended
                            Err(_) => continue, // Timeout, try again
                        }
                    }
                },
                Err(_) => {
                    metrics_clone.record_failed_connection();
                }
            }
        });
        
        stream_handles.push(handle);
    }
    
    // Generate events for all stream combinations
    let event_generation_task = tokio::spawn({
        let metrics = Arc::clone(&metrics);
        async move {
            for round in 0..EVENTS_PER_STREAM {
                for conn_id in 0..CONCURRENT_CONNECTIONS {
                    for stream_id in 0..CONCURRENT_STREAMS_PER_CONNECTION {
                        let symbol = format!("{}{}__{}", BASE_SYMBOL_PREFIX, conn_id, stream_id);
                        let event = MarketDataEvent {
                            symbol,
                            exchange: CONCURRENT_TEST_EXCHANGE.to_string(),
                            timestamp: chrono::Utc::now().timestamp_millis() as u64 + round as u64,
                            data: match stream_id % 3 {
                                0 => MarketData::OrderBook {
                                    bids: vec![(45000.0 + round as f64 * 0.01, 1.0)],
                                    asks: vec![(45001.0 + round as f64 * 0.01, 1.0)],
                                    sequence: round as u64,
                                },
                                1 => MarketData::Trade {
                                    price: 45000.0 + round as f64 * 0.01,
                                    quantity: 1.0,
                                    side: if round % 2 == 0 { "buy" } else { "sell" }.to_string(),
                                    trade_id: format!("{}_{}", conn_id, round),
                                },
                                _ => MarketData::Quote {
                                    bid_price: 45000.0 + round as f64 * 0.01,
                                    bid_size: 1.0,
                                    ask_price: 45001.0 + round as f64 * 0.01,
                                    ask_size: 1.0,
                                },
                            },
                        };
                        
                        if event_sender.send(event).await.is_err() {
                            return;
                        }
                        
                        metrics.record_event_sent();
                    }
                }
                
                // Rate limiting
                if round % 50 == 0 {
                    sleep(Duration::from_millis(10)).await;
                }
            }
        }
    });
    
    // Wait for all streams to process
    let _ = join_all(stream_handles).await;
    
    // Wait for event generation to complete
    let _ = tokio::time::timeout(Duration::from_secs(30), event_generation_task).await;
    
    let summary = metrics.get_summary();
    summary.print_summary("Concurrent Stream Processing");
    
    // Performance assertions
    assert!(summary.successful_connections > total_streams / 2,
           "Too few successful connections: {}", summary.successful_connections);
    
    assert!(summary.events_received > 0, "No events received");
    
    // Error rate should be reasonable
    let error_rate = summary.stream_errors as f64 / summary.successful_connections as f64;
    assert!(error_rate < 0.1, "Stream error rate too high: {:.2}%", error_rate * 100.0);
}

#[rstest]
#[tokio::test]
async fn test_concurrent_subscription_and_unsubscription() {
    let (service, event_sender) = MarketDataGrpcService::new();
    let service = Arc::new(service);
    let metrics = Arc::new(ConcurrencyMetrics::new());
    
    let operations_count = CONCURRENT_CONNECTIONS;
    let mut operation_handles = Vec::new();
    
    for i in 0..operations_count {
        let service_clone = Arc::clone(&service);
        let metrics_clone = Arc::clone(&metrics);
        let symbol = format!("{}{}", BASE_SYMBOL_PREFIX, i);
        
        let handle = tokio::spawn(async move {
            // Subscribe
            let subscribe_request = SubscribeRequest {
                symbols: vec![symbol.clone()],
                exchange: CONCURRENT_TEST_EXCHANGE.to_string(),
                data_types: vec![1, 2],
            };
            
            let request = Request::new(subscribe_request);
            match service_clone.subscribe(request).await {
                Ok(response) => {
                    metrics_clone.record_successful_connection();
                    
                    let mut stream = response.into_inner();
                    
                    // Receive a few events
                    let mut received_count = 0;
                    while received_count < 3 {
                        match tokio::time::timeout(
                            Duration::from_millis(100),
                            stream.next()
                        ).await {
                            Ok(Some(Ok(_event))) => {
                                metrics_clone.record_event_received();
                                received_count += 1;
                            },
                            Ok(Some(Err(_))) => {
                                metrics_clone.record_stream_error();
                                break;
                            },
                            Ok(None) => break,
                            Err(_) => break, // Timeout
                        }
                    }
                    
                    // Drop stream (simulates unsubscription)
                    drop(stream);
                    
                    // Try unsubscribe call
                    let unsubscribe_request = services_common::marketdata::v1::UnsubscribeRequest {
                        symbols: vec![symbol],
                        exchange: CONCURRENT_TEST_EXCHANGE.to_string(),
                    };
                    
                    let request = Request::new(unsubscribe_request);
                    match service_clone.unsubscribe(request).await {
                        Ok(_) => {
                            // Unsubscribe successful
                        },
                        Err(_) => {
                            metrics_clone.record_connection_error();
                        }
                    }
                },
                Err(_) => {
                    metrics_clone.record_failed_connection();
                }
            }
        });
        
        operation_handles.push(handle);
    }
    
    // Generate events while subscriptions are active
    let event_task = tokio::spawn({
        let metrics = Arc::clone(&metrics);
        async move {
            for i in 0..EVENTS_PER_STREAM {
                for conn_id in 0..operations_count {
                    let symbol = format!("{}{}", BASE_SYMBOL_PREFIX, conn_id);
                    let event = MarketDataEvent {
                        symbol,
                        exchange: CONCURRENT_TEST_EXCHANGE.to_string(),
                        timestamp: chrono::Utc::now().timestamp_millis() as u64 + i as u64,
                        data: MarketData::Trade {
                            price: 45000.0 + i as f64 * 0.01,
                            quantity: 1.0,
                            side: "buy".to_string(),
                            trade_id: format!("{}_{}", conn_id, i),
                        },
                    };
                    
                    if event_sender.send(event).await.is_err() {
                        break;
                    }
                    
                    metrics.record_event_sent();
                }
                
                sleep(Duration::from_millis(5)).await;
            }
        }
    });
    
    // Wait for all operations to complete
    let _ = join_all(operation_handles).await;
    
    // Wait for event generation
    let _ = tokio::time::timeout(Duration::from_secs(10), event_task).await;
    
    let summary = metrics.get_summary();
    summary.print_summary("Concurrent Subscribe/Unsubscribe");
    
    // Performance assertions
    let success_rate = summary.successful_connections as f64 / operations_count as f64;
    assert!(success_rate > 0.7, "Subscribe success rate too low: {:.2}%", success_rate * 100.0);
    
    assert!(summary.events_received > 0, "No events received during concurrent operations");
}

#[rstest]
#[tokio::test]
async fn test_connection_pool_exhaustion_recovery() {
    let (service, event_sender) = MarketDataGrpcService::new();
    let service = Arc::new(service);
    let metrics = Arc::new(ConcurrencyMetrics::new());
    
    // Try to exhaust connection pool
    let exhaustion_connections = CONCURRENT_CONNECTIONS * 2;
    let mut first_wave_handles = Vec::new();
    
    // First wave: create many connections
    for i in 0..exhaustion_connections {
        let service_clone = Arc::clone(&service);
        let metrics_clone = Arc::clone(&metrics);
        let symbol = format!("EXHAUST_{}", i);
        
        let handle = tokio::spawn(async move {
            let subscribe_request = SubscribeRequest {
                symbols: vec![symbol],
                exchange: CONCURRENT_TEST_EXCHANGE.to_string(),
                data_types: vec![1],
            };
            
            let request = Request::new(subscribe_request);
            match service_clone.subscribe(request).await {
                Ok(response) => {
                    metrics_clone.record_successful_connection();
                    
                    let mut stream = response.into_inner();
                    
                    // Hold connection briefly
                    let _ = tokio::time::timeout(
                        Duration::from_millis(500),
                        stream.next()
                    ).await;
                },
                Err(_) => {
                    metrics_clone.record_failed_connection();
                }
            }
        });
        
        first_wave_handles.push(handle);
        
        // Small delay between connections to avoid overwhelming
        if i % 10 == 0 {
            tokio::task::yield_now().await;
        }
    }
    
    // Wait for first wave to complete (connections should be released)
    let _ = join_all(first_wave_handles).await;
    
    // Brief recovery period
    sleep(Duration::from_millis(100)).await;
    
    // Second wave: test recovery
    let mut second_wave_handles = Vec::new();
    for i in 0..CONCURRENT_CONNECTIONS / 2 {
        let service_clone = Arc::clone(&service);
        let metrics_clone = Arc::clone(&metrics);
        let symbol = format!("RECOVERY_{}", i);
        
        let handle = tokio::spawn(async move {
            let subscribe_request = SubscribeRequest {
                symbols: vec![symbol],
                exchange: CONCURRENT_TEST_EXCHANGE.to_string(),
                data_types: vec![1, 2],
            };
            
            let request = Request::new(subscribe_request);
            match service_clone.subscribe(request).await {
                Ok(response) => {
                    metrics_clone.record_successful_connection();
                    
                    let mut stream = response.into_inner();
                    
                    // Try to receive events
                    let _ = tokio::time::timeout(
                        Duration::from_millis(200),
                        stream.next()
                    ).await;
                },
                Err(_) => {
                    metrics_clone.record_failed_connection();
                }
            }
        });
        
        second_wave_handles.push(handle);
    }
    
    // Send some events during recovery
    for i in 0..50 {
        let event = MarketDataEvent {
            symbol: format!("RECOVERY_{}", i % (CONCURRENT_CONNECTIONS / 2)),
            exchange: CONCURRENT_TEST_EXCHANGE.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64 + i as u64,
            data: MarketData::OrderBook {
                bids: vec![(45000.0, 1.0)],
                asks: vec![(45001.0, 1.0)],
                sequence: i as u64,
            },
        };
        
        if event_sender.send(event).await.is_err() {
            break;
        }
        
        metrics.record_event_sent();
    }
    
    // Wait for recovery wave
    let _ = join_all(second_wave_handles).await;
    
    let summary = metrics.get_summary();
    summary.print_summary("Connection Pool Exhaustion Recovery");
    
    // Should recover after exhaustion
    assert!(summary.successful_connections > exhaustion_connections / 4,
           "Poor recovery after connection exhaustion: {}", summary.successful_connections);
}

#[rstest]
#[tokio::test]
async fn test_concurrent_different_exchange_subscriptions() {
    let (service, event_sender) = MarketDataGrpcService::new();
    let service = Arc::new(service);
    let metrics = Arc::new(ConcurrencyMetrics::new());
    
    let exchanges = vec!["binance", "zerodha", "coinbase", "kraken"];
    let connections_per_exchange = CONCURRENT_CONNECTIONS / exchanges.len();
    let mut subscription_handles = Vec::new();
    
    for (exchange_idx, exchange) in exchanges.iter().enumerate() {
        for i in 0..connections_per_exchange {
            let service_clone = Arc::clone(&service);
            let metrics_clone = Arc::clone(&metrics);
            let exchange_name = exchange.to_string();
            let symbol = format!("{}_{}", exchange.to_uppercase(), i);
            
            let handle = tokio::spawn(async move {
                let subscribe_request = SubscribeRequest {
                    symbols: vec![symbol.clone()],
                    exchange: exchange_name.clone(),
                    data_types: vec![1, 2, 3],
                };
                
                let request = Request::new(subscribe_request);
                match service_clone.subscribe(request).await {
                    Ok(response) => {
                        metrics_clone.record_successful_connection();
                        
                        let mut stream = response.into_inner();
                        let mut received_count = 0;
                        
                        while received_count < 3 {
                            match tokio::time::timeout(
                                Duration::from_millis(100),
                                stream.next()
                            ).await {
                                Ok(Some(Ok(_event))) => {
                                    metrics_clone.record_event_received();
                                    received_count += 1;
                                },
                                Ok(Some(Err(_))) => {
                                    metrics_clone.record_stream_error();
                                    break;
                                },
                                Ok(None) => break,
                                Err(_) => break,
                            }
                        }
                    },
                    Err(_) => {
                        metrics_clone.record_failed_connection();
                    }
                }
            });
            
            subscription_handles.push(handle);
        }
    }
    
    // Send events for all exchanges
    let event_task = tokio::spawn({
        let metrics = Arc::clone(&metrics);
        async move {
            for round in 0..EVENTS_PER_STREAM / 10 {
                for (exchange_idx, exchange) in exchanges.iter().enumerate() {
                    for i in 0..connections_per_exchange {
                        let symbol = format!("{}_{}", exchange.to_uppercase(), i);
                        let event = MarketDataEvent {
                            symbol,
                            exchange: exchange.to_string(),
                            timestamp: chrono::Utc::now().timestamp_millis() as u64 + round as u64,
                            data: MarketData::Quote {
                                bid_price: 45000.0 + (exchange_idx * 1000) as f64 + round as f64,
                                bid_size: 1.0,
                                ask_price: 45001.0 + (exchange_idx * 1000) as f64 + round as f64,
                                ask_size: 1.0,
                            },
                        };
                        
                        if event_sender.send(event).await.is_err() {
                            return;
                        }
                        
                        metrics.record_event_sent();
                    }
                }
                
                sleep(Duration::from_millis(10)).await;
            }
        }
    });
    
    // Wait for all subscriptions
    let _ = join_all(subscription_handles).await;
    
    // Wait for events
    let _ = tokio::time::timeout(Duration::from_secs(10), event_task).await;
    
    let summary = metrics.get_summary();
    summary.print_summary("Concurrent Multi-Exchange Subscriptions");
    
    // Should handle multi-exchange subscriptions well
    let expected_connections = exchanges.len() * connections_per_exchange;
    assert!(summary.successful_connections > expected_connections / 2,
           "Too few successful multi-exchange connections: {}", summary.successful_connections);
}

#[rstest]
#[tokio::test]
async fn test_rapid_connection_cycling() {
    let (service, event_sender) = MarketDataGrpcService::new();
    let service = Arc::new(service);
    let metrics = Arc::new(ConcurrencyMetrics::new());
    
    let cycles = 20;
    let connections_per_cycle = 10;
    
    for cycle in 0..cycles {
        let mut cycle_handles = Vec::new();
        
        // Create connections
        for i in 0..connections_per_cycle {
            let service_clone = Arc::clone(&service);
            let metrics_clone = Arc::clone(&metrics);
            let symbol = format!("CYCLE_{}_{}", cycle, i);
            
            let handle = tokio::spawn(async move {
                let subscribe_request = SubscribeRequest {
                    symbols: vec![symbol],
                    exchange: CONCURRENT_TEST_EXCHANGE.to_string(),
                    data_types: vec![1],
                };
                
                let request = Request::new(subscribe_request);
                match service_clone.subscribe(request).await {
                    Ok(response) => {
                        metrics_clone.record_successful_connection();
                        
                        let mut stream = response.into_inner();
                        
                        // Very brief connection time
                        let _ = tokio::time::timeout(
                            Duration::from_millis(20),
                            stream.next()
                        ).await;
                    },
                    Err(_) => {
                        metrics_clone.record_failed_connection();
                    }
                }
            });
            
            cycle_handles.push(handle);
        }
        
        // Send some events for this cycle
        for i in 0..5 {
            let event = MarketDataEvent {
                symbol: format!("CYCLE_{}_{}", cycle, i % connections_per_cycle),
                exchange: CONCURRENT_TEST_EXCHANGE.to_string(),
                timestamp: chrono::Utc::now().timestamp_millis() as u64 + i as u64,
                data: MarketData::OrderBook {
                    bids: vec![(45000.0, 1.0)],
                    asks: vec![(45001.0, 1.0)],
                    sequence: (cycle * 10 + i) as u64,
                },
            };
            
            if event_sender.send(event).await.is_err() {
                break;
            }
            
            metrics.record_event_sent();
        }
        
        // Wait for cycle to complete
        let _ = join_all(cycle_handles).await;
        
        // Brief pause between cycles
        sleep(Duration::from_millis(50)).await;
    }
    
    let summary = metrics.get_summary();
    summary.print_summary("Rapid Connection Cycling");
    
    let expected_total_connections = cycles * connections_per_cycle;
    let success_rate = summary.successful_connections as f64 / expected_total_connections as f64;
    
    assert!(success_rate > 0.8, "Success rate too low during rapid cycling: {:.2}%", success_rate * 100.0);
    assert!(summary.connection_errors < expected_total_connections / 5,
           "Too many errors during rapid cycling: {}", summary.connection_errors);
}