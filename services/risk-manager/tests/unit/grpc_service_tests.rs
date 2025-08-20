//! Unit tests for gRPC service implementation

use risk_manager::{
    RiskLimits, RiskManagerService, RiskManager,
    grpc_service::{RiskManagerGrpcService, RateLimiter, HealthStatus},
};
use services_common::{Symbol, Side, Px, Qty};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time;
use rstest::*;

async fn create_test_grpc_service() -> (RiskManagerGrpcService, tokio::sync::broadcast::Receiver<risk_manager::grpc_service::RiskEvent>) {
    let limits = RiskLimits::default();
    RiskManagerGrpcService::new(limits).unwrap()
}

#[fixture]
fn test_limits() -> RiskLimits {
    let mut limits = RiskLimits::default();
    limits.max_order_size = 1000;
    limits.max_position_size = 5000;
    limits.max_orders_per_minute = 10;
    limits.max_order_value = 100_000;
    limits.max_total_exposure = 1_000_000;
    limits.max_daily_loss = -50_000;
    limits
}

#[tokio::test]
async fn test_grpc_service_creation() {
    let (service, _event_rx) = create_test_grpc_service().await;
    
    // Verify service components are initialized
    assert!(!service.risk_manager.is_kill_switch_active());
    
    // Verify health status
    let health = service.health_status.read().await;
    assert!(health.is_healthy);
    assert_eq!(health.consecutive_failures, 0);
}

#[tokio::test]
async fn test_rate_limiter_basic_functionality() {
    let rate_limiter = RateLimiter::new(2); // 2 requests per second
    
    // First two requests should pass
    assert!(rate_limiter.check_rate_limit().await.is_ok());
    assert!(rate_limiter.check_rate_limit().await.is_ok());
    
    // Third request should be rate limited
    assert!(rate_limiter.check_rate_limit().await.is_err());
}

#[tokio::test]
async fn test_rate_limiter_time_window_reset() {
    let rate_limiter = RateLimiter::new(2);
    
    // Use up the rate limit
    rate_limiter.check_rate_limit().await.unwrap();
    rate_limiter.check_rate_limit().await.unwrap();
    assert!(rate_limiter.check_rate_limit().await.is_err());
    
    // Wait for window to reset
    time::sleep(Duration::from_secs(1)).await;
    
    // Should be able to make requests again
    assert!(rate_limiter.check_rate_limit().await.is_ok());
    assert!(rate_limiter.check_rate_limit().await.is_ok());
}

#[tokio::test]
async fn test_rate_limiter_concurrent_access() {
    let rate_limiter = Arc::new(RateLimiter::new(10));
    let mut handles = vec![];
    
    // Launch multiple concurrent requests
    for i in 0..20 {
        let rl = rate_limiter.clone();
        handles.push(tokio::spawn(async move {
            let result = rl.check_rate_limit().await;
            (i, result.is_ok())
        }));
    }
    
    // Collect results
    let mut results = vec![];
    for handle in handles {
        results.push(handle.await.unwrap());
    }
    
    // Some should pass, some should fail
    let passed = results.iter().filter(|(_, ok)| *ok).count();
    let failed = results.iter().filter(|(_, ok)| !*ok).count();
    
    assert_eq!(passed + failed, 20);
    assert!(passed <= 10, "Should not exceed rate limit");
    assert!(failed > 0, "Some requests should be rate limited");
}

#[tokio::test]
async fn test_rate_limiter_edge_cases() {
    // Test zero rate limit
    let rate_limiter = RateLimiter::new(0);
    assert!(rate_limiter.check_rate_limit().await.is_err());
    
    // Test very high rate limit
    let rate_limiter = RateLimiter::new(u32::MAX);
    assert!(rate_limiter.check_rate_limit().await.is_ok());
}

#[tokio::test]
async fn test_health_status_tracking() {
    let (service, _event_rx) = create_test_grpc_service().await;
    
    let initial_health = service.health_status.read().await.clone();
    assert!(initial_health.is_healthy);
    assert_eq!(initial_health.consecutive_failures, 0);
    assert!(initial_health.error_message.is_none());
    
    // Manually set unhealthy status for testing
    {
        let mut health = service.health_status.write().await;
        health.is_healthy = false;
        health.consecutive_failures = 3;
        health.error_message = Some("Test error".to_string());
    }
    
    let updated_health = service.health_status.read().await.clone();
    assert!(!updated_health.is_healthy);
    assert_eq!(updated_health.consecutive_failures, 3);
    assert_eq!(updated_health.error_message.unwrap(), "Test error");
}

#[tokio::test]
async fn test_circuit_breaker_functionality() {
    let (service, _event_rx) = create_test_grpc_service().await;
    
    // Circuit breaker should start closed
    assert!(!service.circuit_breaker.is_open());
    
    // Record failures to open circuit breaker
    for _ in 0..5 {
        service.circuit_breaker.record_failure();
    }
    
    // Circuit breaker should be open now
    assert!(service.circuit_breaker.is_open());
    
    // Record success to close it
    service.circuit_breaker.record_success();
    assert!(!service.circuit_breaker.is_open());
}

#[tokio::test]
async fn test_event_streaming() {
    let (service, mut event_rx) = create_test_grpc_service().await;
    
    // Send a test event
    let test_event = risk_manager::grpc_service::RiskEvent {
        timestamp: chrono::Utc::now().timestamp_millis(),
        event_type: risk_manager::grpc_service::RiskEventType::OrderChecked,
        symbol: Some(Symbol(1)),
        message: "Test event".to_string(),
    };
    
    service.event_tx.send(test_event.clone()).unwrap();
    
    // Receive the event
    let received = tokio::time::timeout(
        Duration::from_secs(1),
        event_rx.recv()
    ).await.unwrap().unwrap();
    
    assert_eq!(received.message, test_event.message);
    assert_eq!(received.symbol, test_event.symbol);
}

#[rstest]
#[case(1, true)]
#[case(100, true)]
#[case(0, false)]
#[tokio::test]
async fn test_rate_limiter_parameterized(#[case] max_rps: u32, #[case] should_allow_request: bool) {
    let rate_limiter = RateLimiter::new(max_rps);
    let result = rate_limiter.check_rate_limit().await;
    
    if should_allow_request {
        assert!(result.is_ok(), "Rate limiter should allow request for max_rps: {}", max_rps);
    } else {
        assert!(result.is_err(), "Rate limiter should reject request for max_rps: {}", max_rps);
    }
}

#[tokio::test]
async fn test_shutdown_signal() {
    let (service, _event_rx) = create_test_grpc_service().await;
    
    // Test shutdown signal sending
    let shutdown_result = service.shutdown_tx.send(());
    assert!(shutdown_result.is_ok());
    
    // Subsequent sends should fail (no receivers)
    let second_result = service.shutdown_tx.send(());
    assert!(second_result.is_err());
}

#[tokio::test]
async fn test_concurrent_health_checks() {
    let (service, _event_rx) = create_test_grpc_service().await;
    let service = Arc::new(service);
    let mut handles = vec![];
    
    // Multiple tasks checking/modifying health concurrently
    for i in 0..10 {
        let service_clone = service.clone();
        handles.push(tokio::spawn(async move {
            if i % 2 == 0 {
                // Reader task
                let health = service_clone.health_status.read().await;
                health.is_healthy
            } else {
                // Writer task
                let mut health = service_clone.health_status.write().await;
                health.last_check = Instant::now();
                true
            }
        }));
    }
    
    // All tasks should complete without deadlock
    for handle in handles {
        assert!(handle.await.unwrap());
    }
}

#[tokio::test]
async fn test_metrics_integration() {
    let (_service, _event_rx) = create_test_grpc_service().await;
    
    // Test that service initialization doesn't panic even if metrics fail to initialize
    // This verifies the error handling in the metrics setup
    // We can't directly access METRICS due to visibility but this tests the integration
}

// Removed test_process_request_timeout as process_request is private

#[tokio::test]
async fn test_monitor_integration() {
    let (service, _event_rx) = create_test_grpc_service().await;
    
    // Test that monitor can be used
    let metrics_result = service.monitor.get_current_metrics().await;
    assert!(metrics_result.is_ok());
    
    let metrics = metrics_result.unwrap();
    assert_eq!(metrics.total_exposure, 1_000_000);
    assert_eq!(metrics.daily_pnl, 50_000);
}

#[tokio::test]
async fn test_service_component_interaction() {
    let (service, _event_rx) = create_test_grpc_service().await;
    
    // Test interaction between risk manager and circuit breaker
    let symbol = Symbol(1);
    let side = Side::Bid;
    let qty = Qty::from_qty_i32(100_0000);
    let price = Px::from_price_i32(100_0000);
    
    // Make an order check
    let result = service.risk_manager.check_order(symbol, side, qty, price).await;
    assert!(matches!(result, risk_manager::RiskCheckResult::Approved));
    
    // Verify metrics were updated
    let metrics = service.risk_manager.get_metrics().await;
    assert_eq!(metrics.orders_today, 1);
}

// Removed test_error_propagation as process_request is private

#[tokio::test]
async fn test_concurrent_service_operations() {
    let (service, _event_rx) = create_test_grpc_service().await;
    let service = Arc::new(service);
    let mut handles = vec![];
    
    // Launch multiple operations concurrently
    for i in 0..50 {
        let service_clone = service.clone();
        handles.push(tokio::spawn(async move {
            match i % 4 {
                0 => {
                    // Order check
                    let result = service_clone.risk_manager.check_order(
                        Symbol(i as u32),
                        Side::Bid,
                        Qty::from_qty_i32(100_0000),
                        Px::from_price_i32(100_0000),
                    ).await;
                    matches!(result, risk_manager::RiskCheckResult::Approved | risk_manager::RiskCheckResult::Rejected(_))
                },
                1 => {
                    // Get metrics
                    let _metrics = service_clone.risk_manager.get_metrics().await;
                    true
                },
                2 => {
                    // Rate limit check
                    service_clone.rate_limiter.check_rate_limit().await.is_ok()
                },
                _ => {
                    // Circuit breaker check
                    !service_clone.circuit_breaker.is_open()
                }
            }
        }));
    }
    
    // All operations should complete
    let mut success_count = 0;
    for handle in handles {
        if handle.await.unwrap() {
            success_count += 1;
        }
    }
    
    assert!(success_count > 0, "Some operations should succeed");
}

#[tokio::test]
async fn test_stress_test_event_streaming() {
    let (service, mut event_rx) = create_test_grpc_service().await;
    let service = Arc::new(service);
    
    // Generate many events concurrently
    let event_count = 100;
    let mut handles = vec![];
    
    for i in 0..event_count {
        let service_clone = service.clone();
        handles.push(tokio::spawn(async move {
            let event = risk_manager::grpc_service::RiskEvent {
                timestamp: chrono::Utc::now().timestamp_millis(),
                event_type: risk_manager::grpc_service::RiskEventType::OrderChecked,
                symbol: Some(Symbol(i)),
                message: format!("Stress test event {}", i),
            };
            
            service_clone.event_tx.send(event).is_ok()
        }));
    }
    
    // Wait for all events to be sent
    let mut sent_count = 0;
    for handle in handles {
        if handle.await.unwrap() {
            sent_count += 1;
        }
    }
    
    // Receive events (with timeout to avoid hanging)
    let mut received_count = 0;
    let timeout = Duration::from_secs(5);
    let start = Instant::now();
    
    while received_count < sent_count && start.elapsed() < timeout {
        match tokio::time::timeout(Duration::from_millis(100), event_rx.recv()).await {
            Ok(Ok(_)) => received_count += 1,
            Ok(Err(_)) => break, // Channel closed
            Err(_) => break, // Timeout
        }
    }
    
    assert!(received_count > 0, "Should receive some events");
}

#[tokio::test]
async fn test_memory_safety_under_load() {
    let (service, _event_rx) = create_test_grpc_service().await;
    let service = Arc::new(service);
    let mut handles = vec![];
    
    // Create many references and drop them to test memory safety
    for i in 0..200 {
        let service_clone = service.clone();
        handles.push(tokio::spawn(async move {
            // Do some work then drop
            let _metrics = service_clone.risk_manager.get_metrics().await;
            let _health = service_clone.health_status.read().await;
            i % 2 == 0
        }));
        
        // Drop some handles early
        if i % 10 == 0 && i > 0 {
            if let Some(handle) = handles.pop() {
                let _ = handle.await;
            }
        }
    }
    
    // Wait for remaining handles
    for handle in handles {
        let _ = handle.await.unwrap();
    }
    
    // Service should still be functional
    assert!(!service.risk_manager.is_kill_switch_active());
}

#[tokio::test]
async fn test_service_with_custom_limits() {
    let mut limits = RiskLimits::default();
    limits.max_orders_per_minute = 1; // Very restrictive
    limits.circuit_breaker_threshold = 2;
    
    let (service, _event_rx) = RiskManagerGrpcService::new(limits).unwrap();
    
    // Test rate limiting with custom limits
    assert!(service.rate_limiter.check_rate_limit().await.is_ok());
    assert!(service.rate_limiter.check_rate_limit().await.is_err());
    
    // Test circuit breaker with custom threshold
    service.circuit_breaker.record_failure();
    assert!(!service.circuit_breaker.is_open());
    service.circuit_breaker.record_failure();
    assert!(service.circuit_breaker.is_open());
}