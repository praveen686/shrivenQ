//! Comprehensive test suite for services-common
//!
//! This test suite provides comprehensive coverage for all components
//! in the common services utilities library including:
//!
//! - Event bus functionality and message routing
//! - gRPC client implementations and connection management
//! - Configuration parsing and validation
//! - Error handling and propagation
//! - Storage utilities and WAL operations
//! - Concurrent access patterns and integration scenarios
//!
//! ## Test Organization
//!
//! Tests are organized into separate modules by functionality:
//!
//! - `event_bus_tests`: Event bus core functionality, routing, metrics
//! - `client_tests`: gRPC client behavior, timeouts, retries
//! - `config_tests`: Configuration parsing, defaults, serialization
//! - `error_tests`: Error conversion, propagation, categorization
//! - `storage_tests`: WAL operations, data serialization, iterators
//! - `integration_tests`: Cross-component tests, concurrency, performance
//!
//! ## Running Tests
//!
//! Run all tests:
//! ```bash
//! cargo test
//! ```
//!
//! Run specific test module:
//! ```bash
//! cargo test event_bus_tests
//! cargo test client_tests
//! cargo test config_tests
//! cargo test error_tests
//! cargo test storage_tests
//! cargo test integration_tests
//! ```
//!
//! Run tests with output:
//! ```bash
//! cargo test -- --nocapture
//! ```
//!
//! Run performance tests:
//! ```bash
//! cargo test test_high_volume --release -- --nocapture
//! cargo test performance --release -- --nocapture
//! ```

// Re-export common test utilities
pub use services_common::*;

// Test modules
mod event_bus_tests;
mod client_tests;
mod config_tests;
mod error_tests;
mod storage_tests;
mod integration_tests;

#[cfg(test)]
mod test_utils {
    //! Common test utilities and helper functions
    
    use std::time::Duration;
    use tokio::time::timeout;
    
    /// Helper for testing timeout scenarios
    pub async fn with_timeout<F, T>(duration: Duration, future: F) -> Result<T, &'static str>
    where
        F: std::future::Future<Output = T>,
    {
        timeout(duration, future)
            .await
            .map_err(|_| "Operation timed out")
    }
    
    /// Helper for generating test data
    pub fn generate_test_data(size: usize) -> String {
        (0..size).map(|i| (b'A' + (i % 26) as u8) as char).collect()
    }
    
    /// Helper for creating test timestamps
    pub fn test_timestamp() -> i64 {
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    }
    
    /// Helper for testing concurrent operations
    pub async fn run_concurrent_test<F, T>(num_tasks: usize, task_fn: F) -> Vec<T>
    where
        F: Fn(usize) -> std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>> + Send + Sync,
        T: Send + 'static,
    {
        let mut handles = Vec::new();
        
        for i in 0..num_tasks {
            let future = task_fn(i);
            let handle = tokio::spawn(future);
            handles.push(handle);
        }
        
        let mut results = Vec::new();
        for handle in handles {
            if let Ok(result) = handle.await {
                results.push(result);
            }
        }
        
        results
    }
}

// Integration test configuration
#[cfg(test)]
mod test_config {
    //! Test configuration constants and settings
    
    use std::time::Duration;
    
    // Timeout settings for tests
    pub const DEFAULT_TEST_TIMEOUT: Duration = Duration::from_secs(30);
    pub const QUICK_TEST_TIMEOUT: Duration = Duration::from_secs(5);
    pub const PERFORMANCE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
    
    // Test data sizes
    pub const SMALL_DATA_SIZE: usize = 100;
    pub const MEDIUM_DATA_SIZE: usize = 10_000;
    pub const LARGE_DATA_SIZE: usize = 100_000;
    
    // Concurrency settings
    pub const DEFAULT_CONCURRENT_TASKS: usize = 10;
    pub const HIGH_CONCURRENCY_TASKS: usize = 100;
    pub const STRESS_TEST_TASKS: usize = 1000;
    
    // Performance thresholds
    pub const MIN_OPERATIONS_PER_SEC: f64 = 1000.0;
    pub const MAX_ACCEPTABLE_LATENCY_MS: u64 = 100;
    
    // Test endpoints (should not conflict with real services)
    pub const TEST_AUTH_ENDPOINT: &str = "http://localhost:59901";
    pub const TEST_MARKET_DATA_ENDPOINT: &str = "http://localhost:59902";
    pub const TEST_RISK_ENDPOINT: &str = "http://localhost:59903";
    pub const TEST_EXECUTION_ENDPOINT: &str = "http://localhost:59904";
    pub const TEST_DATA_AGGREGATOR_ENDPOINT: &str = "http://localhost:59905";
}

#[cfg(test)]
mod benchmarks {
    //! Performance benchmarks for critical components
    
    use super::*;
    use std::time::Instant;
    use test_config::*;
    
    #[tokio::test]
    async fn benchmark_event_bus_throughput() {
        let config = EventBusConfig {
            capacity: 100_000,
            enable_metrics: true,
            ..Default::default()
        };
        
        let bus = std::sync::Arc::new(EventBus::<ShrivenQuantMessage>::new(config));
        
        let start_time = Instant::now();
        let message_count = 50_000;
        
        // Benchmark publishing
        let mut publish_handles = vec![];
        for thread_id in 0..10 {
            let bus_clone = std::sync::Arc::clone(&bus);
            let handle = tokio::spawn(async move {
                for i in 0..message_count / 10 {
                    let message = ShrivenQuantMessage::MarketData {
                        symbol: format!("BENCH{}", thread_id),
                        exchange: "benchmark".to_string(),
                        bid: (50000 + i) * 100000,
                        ask: (50001 + i) * 100000,
                        timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default() as u64,
                    };
                    
                    let _ = bus_clone.publish(message).await;
                }
            });
            publish_handles.push(handle);
        }
        
        for handle in publish_handles {
            handle.await.unwrap();
        }
        
        let duration = start_time.elapsed();
        let throughput = message_count as f64 / duration.as_secs_f64();
        
        println!("Event Bus Benchmark Results:");
        println!("Messages: {}", message_count);
        println!("Duration: {:?}", duration);
        println!("Throughput: {:.2} messages/second", throughput);
        
        assert!(
            throughput >= MIN_OPERATIONS_PER_SEC,
            "Event bus throughput too low: {} < {}",
            throughput,
            MIN_OPERATIONS_PER_SEC
        );
    }
    
    #[tokio::test]
    async fn benchmark_wal_operations() {
        use tempfile::TempDir;
        
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("benchmark_wal");
        
        let mut wal = Wal::new(&wal_path, Some(10 * 1024 * 1024)).unwrap(); // 10MB segments
        
        let start_time = Instant::now();
        let entry_count = 10_000;
        
        // Benchmark WAL writes
        for i in 0..entry_count {
            let trade = TradeEvent {
                ts: Ts::now(),
                symbol: Symbol::new((i % 10) as u32),
                price: Px::from_i64(50000_00000000 + (i as i64 * 1000)),
                quantity: Qty::from_i64(100_000_000),
                is_buy: i % 2 == 0,
                trade_id: i as u64,
            };
            
            wal.append(&trade).unwrap();
            
            if i % 1000 == 0 {
                wal.flush().unwrap();
            }
        }
        
        wal.flush().unwrap();
        let write_duration = start_time.elapsed();
        let write_throughput = entry_count as f64 / write_duration.as_secs_f64();
        
        // Benchmark WAL reads
        let read_start = Instant::now();
        let mut iterator: WalIterator<TradeEvent> = wal.stream(None).unwrap();
        let mut read_count = 0;
        
        while iterator.read_next_entry().unwrap().is_some() {
            read_count += 1;
        }
        
        let read_duration = read_start.elapsed();
        let read_throughput = read_count as f64 / read_duration.as_secs_f64();
        
        println!("WAL Benchmark Results:");
        println!("Entries: {}", entry_count);
        println!("Write Duration: {:?}", write_duration);
        println!("Write Throughput: {:.2} entries/second", write_throughput);
        println!("Read Duration: {:?}", read_duration);
        println!("Read Throughput: {:.2} entries/second", read_throughput);
        
        assert_eq!(read_count, entry_count);
        assert!(
            write_throughput >= MIN_OPERATIONS_PER_SEC,
            "WAL write throughput too low: {} < {}",
            write_throughput,
            MIN_OPERATIONS_PER_SEC
        );
    }
    
    #[tokio::test]
    async fn benchmark_error_handling_overhead() {
        use tonic::{Code, Status};
        
        let start_time = Instant::now();
        let conversion_count = 100_000;
        
        // Benchmark error conversions
        for i in 0..conversion_count {
            let status = match i % 6 {
                0 => Status::new(Code::Unauthenticated, "Token expired"),
                1 => Status::new(Code::Unavailable, "Service down"),
                2 => Status::new(Code::InvalidArgument, "Bad request"),
                3 => Status::new(Code::DeadlineExceeded, "Timeout"),
                4 => Status::new(Code::ResourceExhausted, "Rate limited"),
                5 => Status::new(Code::Internal, "Server error"),
                _ => unreachable!(),
            };
            
            let _service_error = ServiceError::from(status);
        }
        
        let duration = start_time.elapsed();
        let conversion_rate = conversion_count as f64 / duration.as_secs_f64();
        
        println!("Error Handling Benchmark Results:");
        println!("Conversions: {}", conversion_count);
        println!("Duration: {:?}", duration);
        println!("Conversion Rate: {:.2} conversions/second", conversion_rate);
        
        // Error conversion should be very fast
        assert!(
            conversion_rate >= 10_000.0,
            "Error conversion too slow: {} < 10,000",
            conversion_rate
        );
    }
}