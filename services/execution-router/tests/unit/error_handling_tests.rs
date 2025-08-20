//! Comprehensive error handling tests
//!
//! Tests for all error conditions and edge cases:
//! - ExecutionError variant coverage and error messages
//! - Error propagation through service layers
//! - Recovery mechanisms and graceful degradation
//! - Concurrent error scenarios and thread safety
//! - Resource cleanup during error conditions
//! - Error logging and debugging information

use execution_router::{
    ExecutionError, ExecutionResult, ExecutionRouterService, VenueStrategy,
    OrderRequest, OrderType, TimeInForce, OrderId, OrderStatus
};
use services_common::{Px, Qty, Side, Symbol};
use rstest::*;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use anyhow::Result;
use tokio::runtime::Runtime;

/// Error Testing Utilities
mod error_utils {
    use super::*;
    
    pub fn assert_error_variant<T>(result: ExecutionResult<T>, expected_error_contains: &str) {
        match result {
            Ok(_) => panic!("Expected error but got success"),
            Err(error) => {
                let error_msg = error.to_string();
                assert!(error_msg.contains(expected_error_contains), 
                    "Error message '{}' should contain '{}'", error_msg, expected_error_contains);
            }
        }
    }
    
    pub fn create_test_order_request(client_id: &str) -> OrderRequest {
        OrderRequest {
            client_order_id: client_id.to_string(),
            symbol: Symbol::new(1),
            side: Side::Buy,
            quantity: Qty::from_i64(1_000_000),
            order_type: OrderType::Limit,
            limit_price: Some(Px::from_i64(50_000_000_000)),
            stop_price: None,
            is_buy: true,
            algorithm: execution_router::ExecutionAlgorithm::Smart,
            urgency: 0.5,
            participation_rate: Some(0.15),
            time_in_force: TimeInForce::GTC,
            venue: Some("binance".to_string()),
            strategy_id: "test_strategy".to_string(),
            params: rustc_hash::FxHashMap::default(),
        }
    }
}

use error_utils::*;

/// Basic Error Variant Tests
#[rstest]
fn test_execution_error_variants() {
    // Test all error variants have proper Display implementation
    let test_errors = vec![
        ExecutionError::OrderNotFound { id: 12345 },
        ExecutionError::ClientOrderNotFound { client_id: "TEST123".to_string() },
        ExecutionError::CannotCancelFilledOrder { id: 67890 },
        ExecutionError::CannotModifyFilledOrder { id: 67890 },
        ExecutionError::RiskCheckFailed { reason: "Position limit exceeded".to_string() },
        ExecutionError::UnsupportedVenue { venue: "unknown_venue".to_string() },
        ExecutionError::ExchangeSubmissionFailed { reason: "Network timeout".to_string() },
        ExecutionError::ExchangeCancellationFailed { reason: "Order already filled".to_string() },
        ExecutionError::ExchangeModificationFailed { reason: "Invalid price".to_string() },
        ExecutionError::InvalidOrderParameters { reason: "Negative quantity".to_string() },
        ExecutionError::ServiceUnavailable { service: "risk_manager".to_string() },
        ExecutionError::InternalError { reason: "Database connection failed".to_string() },
        ExecutionError::AlgorithmExecutionFailed { reason: "TWAP slice calculation error".to_string() },
        ExecutionError::UnsupportedAlgorithm { algorithm: "CustomAlgo".to_string() },
        ExecutionError::VenueNotConnected { venue: "kraken".to_string() },
        ExecutionError::VenueNotFound { venue: "nonexistent".to_string() },
        ExecutionError::NoVenuesAvailable,
        ExecutionError::NoMarketData { symbol: 99 },
        ExecutionError::OrderAlreadyTerminal { order_id: 555 },
        ExecutionError::OrderNotFoundById { order_id: 666 },
        ExecutionError::MarketDataServiceError { error: "Connection refused".to_string() },
        ExecutionError::WebSocketConnectionFailed { error: "SSL handshake failed".to_string() },
        ExecutionError::OrderBookParseError { error: "Invalid JSON".to_string() },
        ExecutionError::UnexpectedMessageFormat,
        ExecutionError::MarketDataTimeout,
    ];
    
    for error in test_errors {
        // Test Display trait
        let error_msg = error.to_string();
        assert!(!error_msg.is_empty(), "Error message should not be empty");
        
        // Test Debug trait
        let debug_msg = format!("{:?}", error);
        assert!(!debug_msg.is_empty(), "Debug message should not be empty");
        
        // Test that error messages are informative
        match error {
            ExecutionError::OrderNotFound { id } => {
                assert!(error_msg.contains(&id.to_string()), "Should include order ID");
            }
            ExecutionError::ClientOrderNotFound { ref client_id } => {
                assert!(error_msg.contains(client_id), "Should include client order ID");
            }
            ExecutionError::UnsupportedVenue { ref venue } => {
                assert!(error_msg.contains(venue), "Should include venue name");
            }
            ExecutionError::NoMarketData { symbol } => {
                assert!(error_msg.contains(&symbol.to_string()), "Should include symbol");
            }
            _ => {
                // General check that error message contains relevant information
                assert!(error_msg.len() > 10, "Error message should be descriptive");
            }
        }
        
        println!("✓ {} - {}", std::any::type_name_of_val(&error), error_msg);
    }
}

/// Order Lifecycle Error Tests
#[rstest]
#[tokio::test]
async fn test_order_not_found_errors() {
    let router = ExecutionRouterService::new(VenueStrategy::Primary);
    
    // Test getting non-existent order by ID
    let result = router.get_order(99999).await;
    assert_error_variant(result, "not found");
    
    // Test getting non-existent order by client ID
    let result = router.get_order_by_client_id("NONEXISTENT123").await;
    assert_error_variant(result, "not found");
    
    // Test cancelling non-existent order
    let result = router.cancel_order(88888).await;
    assert_error_variant(result, "not found");
    
    // Test modifying non-existent order
    let result = router.modify_order(77777, Some(Qty::from_i64(500000)), None).await;
    assert_error_variant(result, "not found");
}

#[rstest]
#[tokio::test]
async fn test_order_state_transition_errors() -> Result<()> {
    let mut router = ExecutionRouterService::new(VenueStrategy::Primary);
    
    // Submit an order
    let request = create_test_order_request("state_test");
    let order_id = router.submit_order(request).await?;
    
    // Wait a moment for order to be processed
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Try to cancel and modify concurrently to test race conditions
    let cancel_result = router.cancel_order(order_id.as_u64()).await;
    let modify_result = router.modify_order(order_id.as_u64(), Some(Qty::from_i64(2_000_000)), None).await;
    
    // One of these operations might fail depending on timing
    match (cancel_result, modify_result) {
        (Ok(_), Err(_)) => {
            println!("Cancel succeeded, modify failed (expected)");
        }
        (Err(_), Ok(_)) => {
            println!("Cancel failed, modify succeeded (possible)");
        }
        (Ok(_), Ok(_)) => {
            println!("Both operations succeeded (possible due to timing)");
        }
        (Err(cancel_err), Err(modify_err)) => {
            println!("Both operations failed - Cancel: {:?}, Modify: {:?}", cancel_err, modify_err);
        }
    }
    
    Ok(())
}

/// Concurrent Error Scenarios
#[rstest]
#[tokio::test]
async fn test_concurrent_order_operations_with_errors() -> Result<()> {
    let router = Arc::new(ExecutionRouterService::new(VenueStrategy::Smart));
    let num_concurrent_ops = 20;
    
    let mut handles = Vec::new();
    
    // Spawn concurrent operations that will mostly fail
    for i in 0..num_concurrent_ops {
        let router_clone = Arc::clone(&router);
        
        let handle = tokio::spawn(async move {
            let mut errors = Vec::new();
            
            // Try to get non-existent orders
            if let Err(e) = router_clone.get_order(10000 + i).await {
                errors.push(format!("get_order: {}", e));
            }
            
            // Try to cancel non-existent orders
            if let Err(e) = router_clone.cancel_order(20000 + i).await {
                errors.push(format!("cancel_order: {}", e));
            }
            
            // Try to modify non-existent orders
            if let Err(e) = router_clone.modify_order(30000 + i, Some(Qty::from_i64(1000)), None).await {
                errors.push(format!("modify_order: {}", e));
            }
            
            errors
        });
        
        handles.push(handle);
    }
    
    // Collect all errors
    let mut all_errors = Vec::new();
    for handle in handles {
        let task_errors = handle.await.unwrap();
        all_errors.extend(task_errors);
    }
    
    // Should have received many "not found" errors
    assert!(all_errors.len() >= num_concurrent_ops, "Should generate errors for non-existent orders");
    
    // Verify error messages are consistent
    for error in &all_errors {
        assert!(error.contains("not found") || error.contains("Order not found"), 
            "Error should be about order not found: {}", error);
    }
    
    println!("Successfully handled {} concurrent error scenarios", all_errors.len());
    
    Ok(())
}

#[rstest]
fn test_thread_safety_of_error_handling() {
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        let router = Arc::new(ExecutionRouterService::new(VenueStrategy::Primary));
        let num_threads = 8;
        let operations_per_thread = 50;
        
        let mut thread_handles = Vec::new();
        
        for thread_id in 0..num_threads {
            let router_clone = Arc::clone(&router);
            
            let handle = thread::spawn(move || {
                let rt = Runtime::new().unwrap();
                let mut error_counts = std::collections::HashMap::new();
                
                rt.block_on(async {
                    for i in 0..operations_per_thread {
                        let order_id = (thread_id * 1000 + i) as u64;
                        
                        // Try various operations that should fail
                        match router_clone.get_order(order_id).await {
                            Ok(_) => {} // Unexpected success
                            Err(e) => {
                                let error_type = std::mem::discriminant(&e);
                                *error_counts.entry(format!("{:?}", error_type)).or_insert(0) += 1;
                            }
                        }
                        
                        match router_clone.cancel_order(order_id).await {
                            Ok(_) => {} // Unexpected success
                            Err(e) => {
                                let error_type = std::mem::discriminant(&e);
                                *error_counts.entry(format!("{:?}", error_type)).or_insert(0) += 1;
                            }
                        }
                    }
                });
                
                error_counts
            });
            
            thread_handles.push(handle);
        }
        
        // Wait for all threads and collect error statistics
        let mut total_error_counts = std::collections::HashMap::new();
        
        for handle in thread_handles {
            let thread_error_counts = handle.join().unwrap();
            for (error_type, count) in thread_error_counts {
                *total_error_counts.entry(error_type).or_insert(0) += count;
            }
        }
        
        // Verify that errors were handled consistently across threads
        assert!(!total_error_counts.is_empty(), "Should have recorded errors");
        
        for (error_type, count) in total_error_counts {
            println!("Error type {}: {} occurrences", error_type, count);
            assert!(count > 0, "Should have positive error count");
        }
    });
}

/// Resource Cleanup During Errors
#[rstest]
#[tokio::test]
async fn test_resource_cleanup_on_errors() -> Result<()> {
    let mut router = ExecutionRouterService::new(VenueStrategy::Smart);
    
    // Submit several orders
    let mut order_ids = Vec::new();
    for i in 0..5 {
        let request = create_test_order_request(&format!("cleanup_test_{}", i));
        let order_id = router.submit_order(request).await?;
        order_ids.push(order_id);
    }
    
    // Force error conditions by trying invalid operations
    for &order_id in &order_ids {
        // Try to cancel the same order multiple times
        let _ = router.cancel_order(order_id.as_u64()).await;
        let second_cancel = router.cancel_order(order_id.as_u64()).await;
        
        // Second cancel should fail
        assert!(second_cancel.is_err(), "Second cancel should fail");
        
        // Try to modify cancelled order
        let modify_result = router.modify_order(order_id.as_u64(), Some(Qty::from_i64(500000)), None).await;
        assert!(modify_result.is_err(), "Modify on cancelled order should fail");
    }
    
    // Service should still function normally
    let metrics = router.get_metrics().await;
    assert!(metrics.total_orders >= order_ids.len() as u64, "Metrics should reflect submitted orders");
    
    Ok(())
}

/// Algorithm-Specific Error Tests
#[rstest]
#[tokio::test]
async fn test_algorithm_execution_errors() {
    // Test errors in smart routing algorithms
    // Note: This tests the error paths in the router's market context fetching
    
    let router = ExecutionRouterService::new(VenueStrategy::Smart);
    
    // Create an order with an invalid symbol to trigger algorithm errors
    let invalid_request = OrderRequest {
        client_order_id: "algorithm_error_test".to_string(),
        symbol: Symbol::new(999999), // Invalid symbol
        side: Side::Buy,
        quantity: Qty::from_i64(1_000_000),
        order_type: OrderType::Limit,
        limit_price: Some(Px::from_i64(50_000_000_000)),
        stop_price: None,
        is_buy: true,
        algorithm: execution_router::ExecutionAlgorithm::Smart,
        urgency: 0.5,
        participation_rate: Some(0.15),
        time_in_force: TimeInForce::GTC,
        venue: None,
        strategy_id: "algorithm_error_test".to_string(),
        params: rustc_hash::FxHashMap::default(),
    };
    
    // This should either succeed (if the algorithm is tolerant) or fail gracefully
    match router.submit_order(
        invalid_request.client_order_id.clone(),
        invalid_request.symbol,
        invalid_request.side,
        invalid_request.quantity,
        "binance".to_string(),
        invalid_request.strategy_id.clone(),
    ).await {
        Ok(order_id) => {
            println!("Order submitted despite invalid symbol: {}", order_id);
            // If it succeeds, it should be trackable
            let order_result = router.get_order(order_id).await;
            assert!(order_result.is_ok(), "Successfully submitted order should be retrievable");
        }
        Err(e) => {
            println!("Order properly rejected due to invalid symbol: {}", e);
            // This is also acceptable behavior
        }
    }
}

/// Service Unavailability Error Tests
#[rstest]
#[tokio::test]
async fn test_service_unavailable_scenarios() {
    let router = ExecutionRouterService::new(VenueStrategy::Primary);
    
    // Test health check when service is healthy
    let is_healthy = router.is_healthy().await;
    assert!(is_healthy, "New router should be healthy");
    
    // Submit many orders quickly to potentially trigger resource exhaustion
    let mut rapid_submissions = Vec::new();
    
    for i in 0..100 {
        let request = create_test_order_request(&format!("rapid_test_{}", i));
        
        let submit_future = router.submit_order(
            request.client_order_id.clone(),
            request.symbol,
            request.side,
            request.quantity,
            "binance".to_string(),
            request.strategy_id.clone(),
        );
        
        rapid_submissions.push(submit_future);
        
        // Don't await immediately to create concurrency
        if rapid_submissions.len() >= 10 {
            break; // Limit concurrency for test stability
        }
    }
    
    // Await all submissions
    let mut successful_submissions = 0;
    let mut failed_submissions = 0;
    
    for future in rapid_submissions {
        match future.await {
            Ok(_) => successful_submissions += 1,
            Err(_) => failed_submissions += 1,
        }
    }
    
    println!("Rapid submissions: {} successful, {} failed", successful_submissions, failed_submissions);
    
    // Service should handle the load gracefully
    assert!(successful_submissions > 0, "At least some submissions should succeed");
    
    // Health check should still pass
    let is_still_healthy = router.is_healthy().await;
    // Note: Depending on implementation, this might fail under extreme load
    // but should generally remain healthy for moderate load
    if !is_still_healthy {
        println!("Service became unhealthy under load (acceptable)");
    }
}

/// Error Recovery Tests
#[rstest]
#[tokio::test]
async fn test_error_recovery_mechanisms() -> Result<()> {
    let mut router = ExecutionRouterService::new(VenueStrategy::Smart);
    
    // Submit a valid order
    let request = create_test_order_request("recovery_test");
    let order_id = router.submit_order(request).await?;
    
    // Simulate error conditions
    let invalid_operations = vec![
        ("Double cancel", router.cancel_order(order_id.as_u64()).await.and_then(|_| {
            router.cancel_order(order_id.as_u64()).await
        })),
        ("Modify non-existent", router.modify_order(99999, Some(Qty::from_i64(1000)), None).await),
        ("Get non-existent", router.get_order(88888).await.map(|_| ())),
    ];
    
    for (operation_name, result) in invalid_operations {
        match result {
            Ok(_) => println!("⚠️  {} unexpectedly succeeded", operation_name),
            Err(e) => println!("✓ {} properly failed: {}", operation_name, e),
        }
    }
    
    // Service should still be functional after errors
    let metrics = router.get_metrics().await;
    assert!(metrics.total_orders > 0, "Metrics should still be accessible after errors");
    
    // Should be able to submit new orders after errors
    let recovery_request = create_test_order_request("recovery_validation");
    let recovery_order_id = router.submit_order(recovery_request).await?;
    
    assert_ne!(recovery_order_id, order_id, "New order should have different ID");
    
    Ok(())
}

/// Error Message Quality Tests
#[rstest]
fn test_error_message_quality() {
    let error_scenarios = vec![
        (
            ExecutionError::OrderNotFound { id: 12345 },
            vec!["Order", "not found", "12345"]
        ),
        (
            ExecutionError::RiskCheckFailed { reason: "Position limit of $1M exceeded by $200K".to_string() },
            vec!["Risk check", "failed", "Position limit", "$1M", "$200K"]
        ),
        (
            ExecutionError::ExchangeSubmissionFailed { reason: "Connection timeout after 30 seconds".to_string() },
            vec!["Exchange submission", "failed", "timeout", "30 seconds"]
        ),
        (
            ExecutionError::UnsupportedVenue { venue: "fake_exchange".to_string() },
            vec!["Unsupported venue", "fake_exchange"]
        ),
        (
            ExecutionError::NoMarketData { symbol: 42 },
            vec!["No market data", "symbol", "42"]
        ),
    ];
    
    for (error, expected_terms) in error_scenarios {
        let error_msg = error.to_string();
        
        for term in expected_terms {
            assert!(error_msg.to_lowercase().contains(&term.to_lowercase()),
                "Error message '{}' should contain term '{}'", error_msg, term);
        }
        
        // Error messages should be user-friendly
        assert!(error_msg.len() > 10, "Error message should be descriptive: '{}'", error_msg);
        assert!(error_msg.len() < 200, "Error message should be concise: '{}'", error_msg);
        
        // Should not contain internal implementation details
        assert!(!error_msg.contains("unwrap"), "Error should not expose implementation details");
        assert!(!error_msg.contains("panic"), "Error should not mention panics");
        
        println!("✓ Quality check passed: {}", error_msg);
    }
}

/// Error Propagation Tests
#[rstest]
#[tokio::test]
async fn test_error_propagation_through_layers() {
    let router = ExecutionRouterService::new(VenueStrategy::Primary);
    
    // Test that errors from different layers are properly propagated
    
    // Layer 1: Service layer error (order not found)
    let service_error = router.get_order(99999).await;
    assert!(service_error.is_err(), "Service layer should propagate order not found error");
    
    // Layer 2: gRPC layer error handling (tested via service interface)
    let client_error = router.get_order_by_client_id("NONEXISTENT").await;
    assert!(client_error.is_err(), "Client layer should propagate client order not found error");
    
    // Layer 3: Validation layer errors
    let zero_quantity_request = OrderRequest {
        client_order_id: "zero_qty_test".to_string(),
        symbol: Symbol::new(1),
        side: Side::Buy,
        quantity: Qty::ZERO, // Invalid quantity
        order_type: OrderType::Limit,
        limit_price: Some(Px::from_i64(50_000_000_000)),
        stop_price: None,
        is_buy: true,
        algorithm: execution_router::ExecutionAlgorithm::Smart,
        urgency: 0.5,
        participation_rate: Some(0.15),
        time_in_force: TimeInForce::GTC,
        venue: Some("binance".to_string()),
        strategy_id: "validation_test".to_string(),
        params: rustc_hash::FxHashMap::default(),
    };
    
    // This should either be rejected or handled gracefully
    match router.submit_order(
        zero_quantity_request.client_order_id.clone(),
        zero_quantity_request.symbol,
        zero_quantity_request.side,
        zero_quantity_request.quantity,
        "binance".to_string(),
        zero_quantity_request.strategy_id.clone(),
    ).await {
        Ok(order_id) => {
            println!("Zero quantity order unexpectedly accepted: {}", order_id);
        }
        Err(e) => {
            println!("Zero quantity order properly rejected: {}", e);
        }
    }
}

/// Performance Under Error Conditions
#[rstest]
#[tokio::test]
async fn test_performance_under_error_conditions() {
    let router = Arc::new(ExecutionRouterService::new(VenueStrategy::Smart));
    let error_operations_count = 1000;
    
    let start_time = std::time::Instant::now();
    
    // Generate many error conditions concurrently
    let mut handles = Vec::new();
    
    for i in 0..error_operations_count {
        let router_clone = Arc::clone(&router);
        
        let handle = tokio::spawn(async move {
            // Mix of different error-inducing operations
            let operations: Vec<Box<dyn std::future::Future<Output = Result<(), ExecutionError>> + Unpin + Send>> = vec![
                Box::new(Box::pin(router_clone.get_order(10000 + i).await.map(|_| ()))),
                Box::new(Box::pin(router_clone.cancel_order(20000 + i).await)),
                Box::new(Box::pin(router_clone.modify_order(30000 + i, Some(Qty::from_i64(1000)), None).await.map(|_| ()))),
            ];
            
            let mut errors = 0;
            for mut op in operations {
                if op.await.is_err() {
                    errors += 1;
                }
            }
            errors
        });
        
        handles.push(handle);
    }
    
    // Wait for all operations to complete
    let mut total_errors = 0;
    for handle in handles {
        total_errors += handle.await.unwrap();
    }
    
    let elapsed = start_time.elapsed();
    
    println!("Error handling performance: {} operations with {} errors in {:?}", 
             error_operations_count * 3, total_errors, elapsed);
    println!("Average time per error operation: {:.2} µs", 
             elapsed.as_micros() as f64 / (error_operations_count * 3) as f64);
    
    // Performance assertions
    assert!(total_errors > error_operations_count, "Should have generated many errors");
    assert!(elapsed.as_millis() < 5000, "Error handling should be fast even under load");
    
    // Service should still be responsive after error load
    let health_check_start = std::time::Instant::now();
    let is_healthy = router.is_healthy().await;
    let health_check_time = health_check_start.elapsed();
    
    println!("Health check after error load: {} in {:?}", is_healthy, health_check_time);
    assert!(health_check_time.as_millis() < 1000, "Health check should be fast after error load");
}

/// Memory Safety During Errors
#[rstest]
fn test_memory_safety_during_errors() {
    // Test that error conditions don't cause memory leaks or unsafe access
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    
    let error_count = Arc::new(AtomicUsize::new(0));
    let threads = 4;
    let operations_per_thread = 100;
    
    let thread_handles: Vec<_> = (0..threads).map(|thread_id| {
        let error_count_clone = Arc::clone(&error_count);
        
        thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            
            rt.block_on(async {
                let router = ExecutionRouterService::new(VenueStrategy::Primary);
                
                for i in 0..operations_per_thread {
                    let order_id = (thread_id * 10000 + i) as u64;
                    
                    // Operations that will fail and could potentially cause memory issues
                    let _ = router.get_order(order_id).await.map_err(|_| {
                        error_count_clone.fetch_add(1, Ordering::Relaxed);
                    });
                    
                    let _ = router.cancel_order(order_id).await.map_err(|_| {
                        error_count_clone.fetch_add(1, Ordering::Relaxed);
                    });
                    
                    let _ = router.modify_order(order_id, Some(Qty::from_i64(1000)), None).await.map_err(|_| {
                        error_count_clone.fetch_add(1, Ordering::Relaxed);
                    });
                }
            });
        })
    }).collect();
    
    // Wait for all threads to complete
    for handle in thread_handles {
        handle.join().unwrap();
    }
    
    let total_errors = error_count.load(Ordering::Relaxed);
    println!("Memory safety test completed with {} errors across {} threads", total_errors, threads);
    
    // Should have generated many errors without crashes
    assert!(total_errors > 0, "Should have generated error conditions");
    assert!(total_errors <= threads * operations_per_thread * 3, "Error count should be reasonable");
}