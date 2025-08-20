//! Test suite for Order Management System (OMS)
//! 
//! This test suite provides comprehensive coverage of the OMS functionality including:
//! - Unit tests for individual components
//! - Integration tests for complete workflows  
//! - Performance tests for concurrent operations
//! - Error handling and edge case tests

// Common test utilities
pub mod common;

// Re-export commonly used test utilities
pub use common::*;

// Test configuration
use std::sync::Once;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

static INIT: Once = Once::new();

/// Initialize logging for tests
pub fn init_test_logging() {
    INIT.call_once(|| {
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "oms=debug,warn".into()))
            .with(tracing_subscriber::fmt::layer().with_test_writer())
            .init();
    });
}

/// Test configuration constants
pub mod test_config {
    use std::time::Duration;
    
    /// Default test timeout
    pub const TEST_TIMEOUT: Duration = Duration::from_secs(30);
    
    /// Performance test timeout  
    pub const PERF_TEST_TIMEOUT: Duration = Duration::from_secs(60);
    
    /// Database URLs for different test types
    pub const UNIT_TEST_DB: &str = "postgresql://test:test@localhost/oms_unit_tests";
    pub const INTEGRATION_TEST_DB: &str = "postgresql://test:test@localhost/oms_integration_tests";
    pub const PERFORMANCE_TEST_DB: &str = "postgresql://test:test@localhost/oms_performance_tests";
    
    /// Test data limits
    pub const MAX_TEST_ORDERS: usize = 10000;
    pub const MAX_CONCURRENT_THREADS: usize = 16;
}

/// Test result summary structure
#[derive(Debug, Clone)]
pub struct TestSummary {
    pub total_tests: usize,
    pub passed: usize,
    pub failed: usize,
    pub duration_ms: u64,
}

impl TestSummary {
    pub fn new() -> Self {
        Self {
            total_tests: 0,
            passed: 0,
            failed: 0,
            duration_ms: 0,
        }
    }
    
    pub fn success_rate(&self) -> f64 {
        if self.total_tests == 0 {
            0.0
        } else {
            (self.passed as f64 / self.total_tests as f64) * 100.0
        }
    }
}

impl Default for TestSummary {
    fn default() -> Self {
        Self::new()
    }
}

/// Test utilities for async operations
pub mod async_utils {
    use std::time::Duration;
    use tokio::time::{timeout, sleep};
    
    /// Wait for condition with timeout
    pub async fn wait_for_condition<F, Fut>(
        mut condition: F,
        check_interval: Duration,
        max_wait: Duration,
    ) -> bool
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let result = timeout(max_wait, async {
            loop {
                if condition().await {
                    return true;
                }
                sleep(check_interval).await;
            }
        }).await;
        
        result.unwrap_or(false)
    }
    
    /// Retry operation with exponential backoff
    pub async fn retry_with_backoff<F, Fut, T, E>(
        mut operation: F,
        max_retries: usize,
        initial_delay: Duration,
    ) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        let mut delay = initial_delay;
        
        for attempt in 0..max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if attempt == max_retries - 1 {
                        return Err(e);
                    }
                    sleep(delay).await;
                    delay = delay * 2; // Exponential backoff
                }
            }
        }
        
        unreachable!()
    }
}

/// Test data factories
pub mod factories {
    use super::*;
    use chrono::Utc;
    use services_common::{Px, Qty, Symbol};
    use uuid::Uuid;
    use oms::order::{Order, OrderRequest, OrderSide, OrderStatus, OrderType, TimeInForce};
    
    /// Factory for creating test orders in bulk
    pub struct OrderFactory {
        sequence: u64,
    }
    
    impl OrderFactory {
        pub fn new() -> Self {
            Self { sequence: 1 }
        }
        
        /// Create a batch of test orders
        pub fn create_batch(&mut self, count: usize) -> Vec<OrderRequest> {
            (0..count).map(|i| self.create_order_request(i)).collect()
        }
        
        /// Create a test order request with incremental sequence
        pub fn create_order_request(&mut self, variant: usize) -> OrderRequest {
            let request = OrderRequest {
                client_order_id: Some(format!("FACTORY-{:06}-{}", self.sequence, variant)),
                parent_order_id: None,
                symbol: Symbol((variant % 5) as u32 + 1),
                side: if variant % 2 == 0 { OrderSide::Buy } else { OrderSide::Sell },
                order_type: OrderType::Limit,
                time_in_force: TimeInForce::Day,
                quantity: Qty::from_i64(1000 + (variant as i64 * 100)),
                price: Some(Px::from_i64(1_000_000 + (variant as i64 * 1000))),
                stop_price: None,
                account: format!("factory_account_{}", variant % 10),
                exchange: "factory_exchange".to_string(),
                strategy_id: Some("factory_strategy".to_string()),
                tags: vec!["factory".to_string(), format!("batch_{}", variant / 100)],
            };
            
            self.sequence += 1;
            request
        }
        
        /// Create orders with specific characteristics
        pub fn create_market_orders(&mut self, count: usize) -> Vec<OrderRequest> {
            (0..count).map(|i| {
                let mut request = self.create_order_request(i);
                request.order_type = OrderType::Market;
                request.price = None;
                request.time_in_force = TimeInForce::Ioc;
                request
            }).collect()
        }
        
        /// Create algorithmic parent orders
        pub fn create_algo_orders(&mut self, count: usize) -> Vec<OrderRequest> {
            let algo_types = [OrderType::Twap, OrderType::Vwap, OrderType::Pov];
            
            (0..count).map(|i| {
                let mut request = self.create_order_request(i);
                request.order_type = algo_types[i % algo_types.len()];
                request.quantity = Qty::from_i64(50_000 + (i as i64 * 10_000)); // Larger quantities
                request
            }).collect()
        }
    }
    
    impl Default for OrderFactory {
        fn default() -> Self {
            Self::new()
        }
    }
}

/// Performance measurement utilities
pub mod perf_utils {
    use std::time::{Duration, Instant};
    
    #[derive(Debug, Clone)]
    pub struct PerfMeasurement {
        pub operation: String,
        pub duration: Duration,
        pub operations_count: usize,
        pub throughput: f64, // operations per second
        pub success: bool,
    }
    
    impl PerfMeasurement {
        pub fn new(operation: String, duration: Duration, operations_count: usize, success: bool) -> Self {
            let throughput = if duration.as_secs_f64() > 0.0 {
                operations_count as f64 / duration.as_secs_f64()
            } else {
                0.0
            };
            
            Self {
                operation,
                duration,
                operations_count,
                throughput,
                success,
            }
        }
    }
    
    /// Measure performance of an async operation
    pub async fn measure_async<F, Fut, T>(
        operation_name: &str,
        operations_count: usize,
        operation: F,
    ) -> (Result<T, Box<dyn std::error::Error + Send + Sync>>, PerfMeasurement)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>>,
    {
        let start = Instant::now();
        let result = operation().await;
        let duration = start.elapsed();
        
        let measurement = PerfMeasurement::new(
            operation_name.to_string(),
            duration,
            operations_count,
            result.is_ok(),
        );
        
        (result, measurement)
    }
    
    /// Measure memory usage (simplified - would need more sophisticated tooling in production)
    pub fn estimate_memory_usage() -> usize {
        // Placeholder implementation - in real testing you'd use proper memory profiling
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_config::*;
    
    #[test]
    fn test_common_utilities() {
        init_test_logging();
        
        let config = create_test_config();
        assert!(!config.database_url.is_empty());
        assert!(config.max_orders_memory > 0);
        
        let request = create_test_order_request(1);
        assert!(request.client_order_id.is_some());
        assert!(request.quantity.as_i64() > 0);
        
        let market_request = create_market_order_request(1);
        assert_eq!(market_request.order_type, oms::order::OrderType::Market);
        assert!(market_request.price.is_none());
    }
    
    #[test]
    fn test_factories() {
        let mut factory = factories::OrderFactory::new();
        
        let batch = factory.create_batch(10);
        assert_eq!(batch.len(), 10);
        
        // All orders should have unique client IDs
        let client_ids: std::collections::HashSet<_> = batch
            .iter()
            .filter_map(|r| r.client_order_id.as_ref())
            .collect();
        assert_eq!(client_ids.len(), 10);
    }
    
    #[test]
    fn test_summary() {
        let mut summary = TestSummary::new();
        assert_eq!(summary.success_rate(), 0.0);
        
        summary.total_tests = 10;
        summary.passed = 8;
        summary.failed = 2;
        assert_eq!(summary.success_rate(), 80.0);
    }
    
    #[tokio::test]
    async fn test_async_utils() {
        use std::time::Duration;
        
        let result = async_utils::wait_for_condition(
            || async { true },
            Duration::from_millis(10),
            Duration::from_millis(100),
        ).await;
        assert!(result);
        
        let retry_result = async_utils::retry_with_backoff(
            || async { Ok::<_, ()>("success") },
            3,
            Duration::from_millis(1),
        ).await;
        assert!(retry_result.is_ok());
    }
}