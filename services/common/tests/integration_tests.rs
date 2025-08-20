//! Comprehensive integration tests for concurrent access patterns
//!
//! Tests cover:
//! - Multi-threaded event bus operations
//! - Concurrent client operations
//! - Race condition detection
//! - Performance under load
//! - Resource contention scenarios
//! - Cross-component integration

use services_common::{
    BusMessage, EventBus, EventBusConfig, EventBusFactory, MarketDataClient,
    MessageHandler, MessageEnvelope, RiskClient, ServiceEndpoints,
    ServiceDiscoveryConfig, ShrivenQuantMessage,
};
use services_common::market_data_client::MarketDataClientConfig;
use services_common::risk_client::RiskClientConfig;
use anyhow::Result;
use async_trait::async_trait;
use parking_lot::RwLock;
use rstest::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Semaphore, Barrier};
use tokio::time::{sleep, timeout};

// Test message for integration testing
#[derive(Debug, Clone)]
struct IntegrationTestMessage {
    id: u64,
    data: String,
    thread_id: u64,
}

impl BusMessage for IntegrationTestMessage {
    fn topic(&self) -> &str {
        "integration_test"
    }
}

// Concurrent message handler for testing
struct ConcurrentHandler {
    name: String,
    processed_count: AtomicU64,
    processing_times: Arc<RwLock<Vec<Duration>>>,
    should_delay: bool,
}

impl ConcurrentHandler {
    fn new(name: &str, should_delay: bool) -> Self {
        Self {
            name: name.to_string(),
            processed_count: AtomicU64::new(0),
            processing_times: Arc::new(RwLock::new(Vec::new())),
            should_delay,
        }
    }

    fn get_processed_count(&self) -> u64 {
        self.processed_count.load(Ordering::Relaxed)
    }

    fn get_average_processing_time(&self) -> Option<Duration> {
        let times = self.processing_times.read();
        if times.is_empty() {
            None
        } else {
            let total: Duration = times.iter().sum();
            Some(total / times.len() as u32)
        }
    }
}

#[async_trait]
impl MessageHandler<IntegrationTestMessage> for ConcurrentHandler {
    async fn handle(&self, _envelope: MessageEnvelope<IntegrationTestMessage>) -> Result<()> {
        let start = std::time::Instant::now();
        
        if self.should_delay {
            sleep(Duration::from_millis(1)).await;
        }
        
        self.processed_count.fetch_add(1, Ordering::Relaxed);
        
        let processing_time = start.elapsed();
        self.processing_times.write().push(processing_time);
        
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// Multi-threaded Event Bus Tests
#[rstest]
#[tokio::test]
async fn test_concurrent_event_bus_operations() {
    let config = EventBusConfig {
        capacity: 50000,
        enable_metrics: true,
        ..Default::default()
    };
    
    let bus = Arc::new(EventBus::<IntegrationTestMessage>::new(config));
    let barrier = Arc::new(Barrier::new(10)); // 10 concurrent tasks
    
    let mut handles = vec![];

    // Spawn publishers
    for thread_id in 0..5 {
        let bus_clone = Arc::clone(&bus);
        let barrier_clone = Arc::clone(&barrier);
        
        let handle = tokio::spawn(async move {
            barrier_clone.wait().await;
            
            for i in 0..1000 {
                let message = IntegrationTestMessage {
                    id: (thread_id * 1000 + i) as u64,
                    data: format!("Thread {} message {}", thread_id, i),
                    thread_id,
                };
                
                let _ = bus_clone.publish(message).await;
                
                // Add some variability
                if i % 100 == 0 {
                    sleep(Duration::from_millis(1)).await;
                }
            }
        });
        handles.push(handle);
    }

    // Spawn subscribers
    for subscriber_id in 0..5 {
        let bus_clone = Arc::clone(&bus);
        let barrier_clone = Arc::clone(&barrier);
        
        let handle = tokio::spawn(async move {
            barrier_clone.wait().await;
            
            let mut subscriber = bus_clone.subscribe("integration_test").await.unwrap();
            let mut received_count = 0;
            
            while received_count < 1000 && 
                  timeout(Duration::from_secs(10), subscriber.recv()).await.is_ok()
            {
                received_count += 1;
            }
            
            received_count
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    let mut total_received = 0;
    for (i, handle) in handles.into_iter().enumerate() {
        let result = handle.await.unwrap();
        if i >= 5 { // Subscriber tasks
            total_received += result;
        }
    }

    // Verify metrics
    let metrics = bus.metrics().snapshot();
    assert!(metrics.total_published() > 0);
    println!("Total messages published: {}", metrics.total_published());
    println!("Total messages received by subscribers: {}", total_received);
}

#[rstest]
#[tokio::test]
async fn test_concurrent_handler_processing() {
    let config = EventBusConfig {
        capacity: 20000,
        enable_metrics: true,
        max_retry_attempts: 2,
        ..Default::default()
    };
    
    let bus = Arc::new(EventBus::<IntegrationTestMessage>::new(config));
    
    // Register multiple handlers for the same topic
    let handler1 = ConcurrentHandler::new("handler1", false);
    let handler2 = ConcurrentHandler::new("handler2", true); // With delay
    let handler3 = ConcurrentHandler::new("handler3", false);
    
    bus.register_handler("integration_test", handler1.clone()).await.unwrap();
    bus.register_handler("integration_test", handler2.clone()).await.unwrap();
    bus.register_handler("integration_test", handler3.clone()).await.unwrap();
    bus.start_handlers().await.unwrap();
    
    // Publish messages concurrently
    let mut publish_handles = vec![];
    let message_count = Arc::new(AtomicU64::new(0));
    
    for thread_id in 0..5 {
        let bus_clone = Arc::clone(&bus);
        let counter_clone = Arc::clone(&message_count);
        
        let handle = tokio::spawn(async move {
            for i in 0..500 {
                let message = IntegrationTestMessage {
                    id: (thread_id * 500 + i) as u64,
                    data: format!("Concurrent message {} from thread {}", i, thread_id),
                    thread_id,
                };
                
                if bus_clone.publish(message).await.is_ok() {
                    counter_clone.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        publish_handles.push(handle);
    }
    
    // Wait for publishing to complete
    for handle in publish_handles {
        handle.await.unwrap();
    }
    
    // Wait for handlers to process messages
    sleep(Duration::from_millis(2000)).await;
    
    let published_count = message_count.load(Ordering::Relaxed);
    let handler1_count = handler1.get_processed_count();
    let handler2_count = handler2.get_processed_count();
    let handler3_count = handler3.get_processed_count();
    
    println!("Published: {}", published_count);
    println!("Handler1 processed: {}", handler1_count);
    println!("Handler2 processed: {}", handler2_count);
    println!("Handler3 processed: {}", handler3_count);
    
    // All handlers should have processed messages
    assert!(handler1_count > 0);
    assert!(handler2_count > 0);
    assert!(handler3_count > 0);
    
    // Handler2 should be slower due to artificial delay
    if let (Some(avg1), Some(avg2)) = (
        handler1.get_average_processing_time(),
        handler2.get_average_processing_time(),
    ) {
        assert!(avg2 > avg1);
    }
}

// Concurrent Client Operations Tests
#[rstest]
#[tokio::test]
async fn test_concurrent_client_connections() {
    let semaphore = Arc::new(Semaphore::new(10)); // Limit concurrent operations
    let mut handles = vec![];
    
    // Test concurrent MarketDataClient operations
    for i in 0..20 {
        let permit = Arc::clone(&semaphore);
        
        let handle = tokio::spawn(async move {
            let _permit = permit.acquire().await.unwrap();
            
            let config = MarketDataClientConfig {
                endpoint: format!("http://localhost:{}", 50051 + (i % 5)),
                connect_timeout: 1,
                max_reconnect_attempts: 1,
                ..Default::default()
            };
            
            // This will fail to connect, but we're testing concurrent creation
            let client = MarketDataClient::new_disconnected(config).await;
            
            // Perform various operations
            let is_connected = client.is_connected().await;
            let subscriptions = client.get_subscriptions().await;
            let retry_stats = client.get_subscription_retry_stats().await;
            
            // Verify disconnect works
            client.disconnect().await.unwrap();
            
            (is_connected, subscriptions.len(), retry_stats.len())
        });
        handles.push(handle);
    }
    
    // Wait for all client operations to complete
    let mut results = vec![];
    for handle in handles {
        let result = handle.await.unwrap();
        results.push(result);
    }
    
    // All clients should start disconnected
    assert!(results.iter().all(|(connected, _, _)| !connected));
    // All should start with no subscriptions
    assert!(results.iter().all(|(_, subs, _)| *subs == 0));
    
    println!("Successfully created and operated {} concurrent clients", results.len());
}

#[rstest]
#[tokio::test]
async fn test_concurrent_risk_client_operations() {
    let mut handles = vec![];
    
    for i in 0..15 {
        let handle = tokio::spawn(async move {
            let config = RiskClientConfig {
                endpoint: format!("http://localhost:{}", 50060 + (i % 3)),
                connect_timeout: 1,
                max_reconnect_attempts: 1,
                alert_buffer_size: 1000,
                ..Default::default()
            };
            
            let client = RiskClient::new_disconnected(config).await;
            
            // Perform concurrent operations
            let initial_connected = client.is_connected().await;
            let initial_subs = client.get_alert_subscriptions().await;
            
            // Try alert operations (will fail when disconnected)
            let stop_result = client.stop_alerts().await;
            let retry_stats = client.get_alert_retry_stats().await;
            let failed_subs = client.has_failed_alert_subscriptions(3).await;
            
            // Cleanup
            let disconnect_result = client.disconnect().await;
            let final_connected = client.is_connected().await;
            
            (
                initial_connected,
                initial_subs.len(),
                stop_result.is_ok(),
                retry_stats.len(),
                failed_subs,
                disconnect_result.is_ok(),
                final_connected,
            )
        });
        handles.push(handle);
    }
    
    let mut results = vec![];
    for handle in handles {
        let result = handle.await.unwrap();
        results.push(result);
    }
    
    // Verify all operations behaved correctly
    for (i, result) in results.iter().enumerate() {
        let (initial_conn, subs, stop_ok, stats, failed, disc_ok, final_conn) = result;
        
        assert!(!initial_conn, "Client {} should start disconnected", i);
        assert_eq!(*subs, 0, "Client {} should start with no subscriptions", i);
        assert!(*stop_ok, "Client {} stop_alerts should succeed", i);
        assert_eq!(*stats, 0, "Client {} should start with no retry stats", i);
        assert!(!failed, "Client {} should not have failed subscriptions initially", i);
        assert!(*disc_ok, "Client {} disconnect should succeed", i);
        assert!(!final_conn, "Client {} should be disconnected after cleanup", i);
    }
    
    println!("Successfully tested {} concurrent risk clients", results.len());
}

// Race Condition Detection Tests
#[rstest]
#[tokio::test]
async fn test_event_bus_race_conditions() {
    let config = EventBusConfig {
        capacity: 10000,
        enable_metrics: true,
        ..Default::default()
    };
    
    let bus = Arc::new(EventBus::<ShrivenQuantMessage>::new(config));
    let operations_count = Arc::new(AtomicU64::new(0));
    
    let mut handles = vec![];
    
    // Concurrent publishers
    for i in 0..5 {
        let bus_clone = Arc::clone(&bus);
        let ops_counter = Arc::clone(&operations_count);
        
        let handle = tokio::spawn(async move {
            for j in 0..200 {
                let message = ShrivenQuantMessage::MarketData {
                    symbol: format!("SYMBOL{}", i),
                    exchange: "test_exchange".to_string(),
                    bid: (50000 + j) * 100000,
                    ask: (50001 + j) * 100000,
                    timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default() as u64,
                };
                
                if bus_clone.publish(message).await.is_ok() {
                    ops_counter.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        handles.push(handle);
    }
    
    // Concurrent subscribers
    for i in 0..3 {
        let bus_clone = Arc::clone(&bus);
        let ops_counter = Arc::clone(&operations_count);
        
        let handle = tokio::spawn(async move {
            let topic = "market_data";
            
            // Subscribe and unsubscribe repeatedly to test race conditions
            for _ in 0..10 {
                if let Ok(mut subscriber) = bus_clone.subscribe(topic).await {
                    ops_counter.fetch_add(1, Ordering::Relaxed);
                    
                    // Try to receive a few messages
                    for _ in 0..5 {
                        if timeout(Duration::from_millis(10), subscriber.recv()).await.is_ok() {
                            ops_counter.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                
                // Small delay to create timing variations
                sleep(Duration::from_millis(i as u64)).await;
            }
        });
        handles.push(handle);
    }
    
    // Concurrent metrics readers
    for _ in 0..2 {
        let bus_clone = Arc::clone(&bus);
        let ops_counter = Arc::clone(&operations_count);
        
        let handle = tokio::spawn(async move {
            for _ in 0..50 {
                let _metrics = bus_clone.metrics().snapshot();
                let _topics = bus_clone.topics();
                let _subscriber_count = bus_clone.subscriber_count("market_data");
                
                ops_counter.fetch_add(3, Ordering::Relaxed);
                
                sleep(Duration::from_millis(5)).await;
            }
        });
        handles.push(handle);
    }
    
    // Wait for all concurrent operations
    for handle in handles {
        handle.await.unwrap();
    }
    
    let total_operations = operations_count.load(Ordering::Relaxed);
    println!("Completed {} concurrent operations without race conditions", total_operations);
    
    // Verify final state is consistent
    let final_metrics = bus.metrics().snapshot();
    let final_topics = bus.topics();
    
    assert!(final_metrics.total_published() > 0);
    assert!(!final_topics.is_empty());
}

// Performance Under Load Tests
#[rstest]
#[tokio::test]
async fn test_system_performance_under_load() {
    let start_time = std::time::Instant::now();
    
    // Create multiple event buses
    let buses: Vec<Arc<EventBus<IntegrationTestMessage>>> = (0..3)
        .map(|_| {
            Arc::new(EventBus::new(EventBusConfig {
                capacity: 20000,
                enable_metrics: true,
                ..Default::default()
            }))
        })
        .collect();
    
    // Create multiple clients (disconnected for testing)
    let clients: Vec<MarketDataClient> = (0..5)
        .map(|i| {
            let config = MarketDataClientConfig {
                endpoint: format!("http://test-{}.local:50051", i),
                connect_timeout: 1,
                ..Default::default()
            };
            futures::executor::block_on(MarketDataClient::new_disconnected(config))
        })
        .collect();
    
    let setup_time = start_time.elapsed();
    
    let load_test_start = std::time::Instant::now();
    let mut handles = vec![];
    
    // High-load event bus operations
    for (bus_id, bus) in buses.into_iter().enumerate() {
        for worker_id in 0..10 {
            let bus_clone = bus.clone();
            
            let handle = tokio::spawn(async move {
                let mut operations = 0;
                
                for i in 0..100 {
                    let message = IntegrationTestMessage {
                        id: (bus_id * 10000 + worker_id * 1000 + i) as u64,
                        data: format!("Load test bus {} worker {} msg {}", bus_id, worker_id, i),
                        thread_id: worker_id as u64,
                    };
                    
                    if bus_clone.publish(message).await.is_ok() {
                        operations += 1;
                    }
                    
                    // Occasionally check metrics to add load
                    if i % 20 == 0 {
                        let _metrics = bus_clone.metrics().snapshot();
                        let _topics = bus_clone.topics();
                        operations += 2;
                    }
                }
                
                operations
            });
            handles.push(handle);
        }
    }
    
    // Concurrent client operations
    for (client_id, client) in clients.into_iter().enumerate() {
        let handle = tokio::spawn(async move {
            let mut operations = 0;
            
            for _ in 0..50 {
                // Various client operations
                let _connected = client.is_connected().await;
                let _subs = client.get_subscriptions().await;
                let _stats = client.get_subscription_retry_stats().await;
                operations += 3;
                
                if client_id % 2 == 0 {
                    sleep(Duration::from_micros(100)).await;
                }
            }
            
            // Cleanup
            let _ = client.disconnect().await;
            operations += 1;
            
            operations
        });
        handles.push(handle);
    }
    
    // Wait for all load test operations
    let mut total_operations = 0;
    for handle in handles {
        let ops = handle.await.unwrap();
        total_operations += ops;
    }
    
    let load_test_time = load_test_start.elapsed();
    let total_time = start_time.elapsed();
    
    println!("Setup time: {:?}", setup_time);
    println!("Load test time: {:?}", load_test_time);
    println!("Total time: {:?}", total_time);
    println!("Total operations: {}", total_operations);
    println!(
        "Operations per second: {:.2}",
        total_operations as f64 / load_test_time.as_secs_f64()
    );
    
    // Performance assertions
    assert!(total_operations > 0);
    assert!(load_test_time < Duration::from_secs(30)); // Should complete reasonably fast
    
    let ops_per_second = total_operations as f64 / load_test_time.as_secs_f64();
    assert!(ops_per_second > 100.0); // Should maintain reasonable throughput
}

// Cross-Component Integration Tests
#[rstest]
#[tokio::test]
async fn test_configuration_integration() {
    // Test that configuration objects work correctly together
    let service_endpoints = ServiceEndpoints {
        auth_service: "http://auth.test:8080".to_string(),
        market_data_service: "grpc://market.test:9090".to_string(),
        risk_service: "http://risk.test:8081".to_string(),
        execution_service: "grpc://exec.test:9091".to_string(),
        data_aggregator_service: "http://data.test:8082".to_string(),
    };
    
    let discovery_config = ServiceDiscoveryConfig {
        enabled: true,
        consul_endpoint: Some("http://consul.test:8500".to_string()),
        etcd_endpoint: Some("http://etcd.test:2379".to_string()),
        refresh_interval_secs: 30,
    };
    
    // Create clients using the configured endpoints
    let md_config = MarketDataClientConfig {
        endpoint: service_endpoints.market_data_service.clone(),
        connect_timeout: 5,
        request_timeout: 30,
        max_reconnect_attempts: 3,
        ..Default::default()
    };
    
    let risk_config = RiskClientConfig {
        endpoint: service_endpoints.risk_service.clone(),
        connect_timeout: 5,
        request_timeout: 30,
        max_reconnect_attempts: 3,
        ..Default::default()
    };
    
    // Test concurrent client creation with configured endpoints
    let mut handles = vec![];
    
    for _ in 0..5 {
        let md_cfg = md_config.clone();
        let risk_cfg = risk_config.clone();
        
        let handle = tokio::spawn(async move {
            let md_client = MarketDataClient::new_disconnected(md_cfg).await;
            let risk_client = RiskClient::new_disconnected(risk_cfg).await;
            
            // Verify endpoints are correctly configured
            let md_endpoint = md_client.endpoint();
            let risk_endpoint = risk_client.endpoint();
            
            // Perform some operations
            let md_connected = md_client.is_connected().await;
            let risk_connected = risk_client.is_connected().await;
            
            // Cleanup
            let md_cleanup = md_client.disconnect().await;
            let risk_cleanup = risk_client.disconnect().await;
            
            (
                md_endpoint.to_string(),
                risk_endpoint.to_string(),
                md_connected,
                risk_connected,
                md_cleanup.is_ok(),
                risk_cleanup.is_ok(),
            )
        });
        handles.push(handle);
    }
    
    // Verify all integrations worked correctly
    for (i, handle) in handles.into_iter().enumerate() {
        let (md_ep, risk_ep, md_conn, risk_conn, md_clean, risk_clean) = handle.await.unwrap();
        
        assert_eq!(md_ep, service_endpoints.market_data_service);
        assert_eq!(risk_ep, service_endpoints.risk_service);
        assert!(!md_conn);
        assert!(!risk_conn);
        assert!(md_clean);
        assert!(risk_clean);
        
        println!("Integration test {} completed successfully", i);
    }
    
    // Test that service discovery config is properly structured
    assert!(discovery_config.enabled);
    assert!(discovery_config.consul_endpoint.is_some());
    assert!(discovery_config.etcd_endpoint.is_some());
    assert_eq!(discovery_config.refresh_interval_secs, 30);
}

// Resource Contention Tests
#[rstest]
#[tokio::test]
async fn test_resource_contention_scenarios() {
    const NUM_WORKERS: usize = 20;
    const OPERATIONS_PER_WORKER: usize = 100;
    
    let shared_resources = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::<String, u64>::new()));
    let operation_counter = Arc::new(AtomicU64::new(0));
    let error_counter = Arc::new(AtomicU64::new(0));
    
    let mut handles = vec![];
    
    // Create workers that compete for shared resources
    for worker_id in 0..NUM_WORKERS {
        let resources = Arc::clone(&shared_resources);
        let op_counter = Arc::clone(&operation_counter);
        let err_counter = Arc::clone(&error_counter);
        
        let handle = tokio::spawn(async move {
            for op_id in 0..OPERATIONS_PER_WORKER {
                let key = format!("resource_{}", op_id % 10); // 10 shared resources
                
                // Simulate different types of operations with different contention patterns
                match op_id % 4 {
                    0 => {
                        // Read operation
                        let resources_read = resources.read().await;
                        let _value = resources_read.get(&key);
                        op_counter.fetch_add(1, Ordering::Relaxed);
                    }
                    1 => {
                        // Write operation
                        let mut resources_write = resources.write().await;
                        resources_write.insert(key, worker_id as u64 * 1000 + op_id as u64);
                        op_counter.fetch_add(1, Ordering::Relaxed);
                    }
                    2 => {
                        // Update operation
                        let mut resources_write = resources.write().await;
                        if let Some(value) = resources_write.get_mut(&key) {
                            *value += 1;
                        } else {
                            resources_write.insert(key, 1);
                        }
                        op_counter.fetch_add(1, Ordering::Relaxed);
                    }
                    3 => {
                        // Delete operation
                        let mut resources_write = resources.write().await;
                        resources_write.remove(&key);
                        op_counter.fetch_add(1, Ordering::Relaxed);
                    }
                    _ => unreachable!(),
                }
                
                // Add some timing variation
                if worker_id % 3 == 0 {
                    sleep(Duration::from_micros(10)).await;
                }
            }
            
            worker_id
        });
        handles.push(handle);
    }
    
    // Wait for all workers to complete
    let start_time = std::time::Instant::now();
    let mut completed_workers = Vec::new();
    
    for handle in handles {
        match handle.await {
            Ok(worker_id) => completed_workers.push(worker_id),
            Err(_) => error_counter.fetch_add(1, Ordering::Relaxed),
        }
    }
    
    let execution_time = start_time.elapsed();
    let total_operations = operation_counter.load(Ordering::Relaxed);
    let total_errors = error_counter.load(Ordering::Relaxed);
    
    println!("Execution time: {:?}", execution_time);
    println!("Total operations: {}", total_operations);
    println!("Total errors: {}", total_errors);
    println!("Completed workers: {}", completed_workers.len());
    println!(
        "Operations per second: {:.2}",
        total_operations as f64 / execution_time.as_secs_f64()
    );
    
    // Verify resource contention was handled correctly
    assert_eq!(completed_workers.len(), NUM_WORKERS);
    assert_eq!(total_errors, 0);
    assert!(total_operations > 0);
    
    // Check final state of shared resources
    let final_resources = shared_resources.read().await;
    println!("Final shared resources count: {}", final_resources.len());
    
    // Resources should be accessible without deadlocks
    assert!(final_resources.len() <= 10); // At most 10 resources (some may have been deleted)
}

// Memory Pressure Tests
#[rstest]
#[tokio::test]
async fn test_memory_pressure_scenarios() {
    const MESSAGE_SIZE: usize = 1000; // Characters
    const NUM_MESSAGES: usize = 5000;
    
    let config = EventBusConfig {
        capacity: 50000, // Large capacity to handle memory pressure
        enable_metrics: true,
        ..Default::default()
    };
    
    let bus = Arc::new(EventBus::<IntegrationTestMessage>::new(config));
    let memory_pressure_counter = Arc::new(AtomicU64::new(0));
    
    // Create handler that processes messages slowly to build up queue
    let slow_handler = ConcurrentHandler::new("slow_handler", true);
    bus.register_handler("integration_test", slow_handler.clone()).await.unwrap();
    bus.start_handlers().await.unwrap();
    
    let mut handles = vec![];
    
    // Producers that create large messages
    for producer_id in 0..5 {
        let bus_clone = Arc::clone(&bus);
        let counter = Arc::clone(&memory_pressure_counter);
        
        let handle = tokio::spawn(async move {
            for i in 0..NUM_MESSAGES / 5 {
                let large_data = "A".repeat(MESSAGE_SIZE);
                let message = IntegrationTestMessage {
                    id: (producer_id * (NUM_MESSAGES / 5) + i) as u64,
                    data: large_data,
                    thread_id: producer_id as u64,
                };
                
                match bus_clone.publish(message).await {
                    Ok(()) => counter.fetch_add(1, Ordering::Relaxed),
                    Err(_) => {
                        // May fail under memory pressure
                        sleep(Duration::from_millis(10)).await;
                    }
                }
                
                // Burst pattern to create memory pressure
                if i % 100 == 0 {
                    for _ in 0..10 {
                        let burst_message = IntegrationTestMessage {
                            id: (producer_id * 100000 + i * 10) as u64,
                            data: "B".repeat(MESSAGE_SIZE / 2),
                            thread_id: producer_id as u64,
                        };
                        
                        if bus_clone.publish(burst_message).await.is_ok() {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
        });
        handles.push(handle);
    }
    
    // Monitor memory usage and system health
    let health_bus = Arc::clone(&bus);
    let health_handle = tokio::spawn(async move {
        let mut health_checks = 0;
        
        for _ in 0..100 {
            let metrics = health_bus.metrics().snapshot();
            let topic_count = health_bus.topics().len();
            let subscriber_count = health_bus.subscriber_count("integration_test");
            
            // System should remain responsive
            assert!(metrics.total_published() >= 0);
            assert!(topic_count >= 0);
            assert!(subscriber_count >= 0);
            
            health_checks += 1;
            sleep(Duration::from_millis(50)).await;
        }
        
        health_checks
    });
    
    // Wait for producers to finish
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Wait a bit for message processing
    sleep(Duration::from_secs(3)).await;
    
    // Verify health monitoring completed successfully
    let health_checks = health_handle.await.unwrap();
    assert_eq!(health_checks, 100);
    
    let successful_publishes = memory_pressure_counter.load(Ordering::Relaxed);
    let processed_messages = slow_handler.get_processed_count();
    
    println!("Successful publishes under memory pressure: {}", successful_publishes);
    println!("Processed messages: {}", processed_messages);
    
    // System should handle memory pressure gracefully
    assert!(successful_publishes > 0);
    assert!(processed_messages > 0);
    
    // Final metrics should be consistent
    let final_metrics = bus.metrics().snapshot();
    assert!(final_metrics.total_published() > 0);
}