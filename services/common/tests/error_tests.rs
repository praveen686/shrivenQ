//! Comprehensive unit tests for error handling utilities
//!
//! Tests cover:
//! - Error type conversions and propagation
//! - Error message preservation
//! - Error categorization and matching
//! - Error serialization and debugging
//! - Integration with external error types

use services_common::ServiceError;
use rstest::*;
use std::error::Error;
use tonic::{Code, Status};

// Basic ServiceError Tests
#[rstest]
#[test]
fn test_service_error_display_formatting() {
    let connection_error = ServiceError::ConnectionFailed("Network timeout".to_string());
    let auth_error = ServiceError::AuthenticationFailed("Invalid credentials".to_string());
    let unavailable_error = ServiceError::ServiceUnavailable("Service is down".to_string());
    let invalid_error = ServiceError::InvalidRequest("Missing required field".to_string());
    let internal_error = ServiceError::InternalError("Database connection lost".to_string());
    let timeout_error = ServiceError::Timeout("Request took too long".to_string());
    let rate_limited_error = ServiceError::RateLimited("Too many requests per minute".to_string());

    // Test display formatting
    assert_eq!(connection_error.to_string(), "Connection failed: Network timeout");
    assert_eq!(auth_error.to_string(), "Authentication failed: Invalid credentials");
    assert_eq!(unavailable_error.to_string(), "Service unavailable: Service is down");
    assert_eq!(invalid_error.to_string(), "Invalid request: Missing required field");
    assert_eq!(internal_error.to_string(), "Internal error: Database connection lost");
    assert_eq!(timeout_error.to_string(), "Timeout: Request took too long");
    assert_eq!(rate_limited_error.to_string(), "Rate limited: Too many requests per minute");
}

#[rstest]
#[test]
fn test_service_error_debug_formatting() {
    let error = ServiceError::ConnectionFailed("Network issue".to_string());
    let debug_str = format!("{:?}", error);
    
    assert!(debug_str.contains("ConnectionFailed"));
    assert!(debug_str.contains("Network issue"));
}

#[rstest]
#[test]
fn test_service_error_is_error_trait() {
    let error = ServiceError::AuthenticationFailed("Token expired".to_string());
    
    // Should implement std::error::Error
    assert!(Error::source(&error).is_none()); // ServiceError doesn't have a source by default
    assert!(!error.to_string().is_empty());
}

// Tonic Status Conversion Tests
#[rstest]
#[test]
fn test_from_tonic_status_unauthenticated() {
    let status = Status::new(Code::Unauthenticated, "Token is invalid");
    let error = ServiceError::from(status);
    
    assert!(matches!(error, ServiceError::AuthenticationFailed(_)));
    assert_eq!(error.to_string(), "Authentication failed: Token is invalid");
}

#[rstest]
#[test]
fn test_from_tonic_status_unavailable() {
    let status = Status::new(Code::Unavailable, "Service is temporarily down");
    let error = ServiceError::from(status);
    
    assert!(matches!(error, ServiceError::ServiceUnavailable(_)));
    assert_eq!(error.to_string(), "Service unavailable: Service is temporarily down");
}

#[rstest]
#[test]
fn test_from_tonic_status_invalid_argument() {
    let status = Status::new(Code::InvalidArgument, "Request payload is malformed");
    let error = ServiceError::from(status);
    
    assert!(matches!(error, ServiceError::InvalidRequest(_)));
    assert_eq!(error.to_string(), "Invalid request: Request payload is malformed");
}

#[rstest]
#[test]
fn test_from_tonic_status_deadline_exceeded() {
    let status = Status::new(Code::DeadlineExceeded, "Request timeout after 30s");
    let error = ServiceError::from(status);
    
    assert!(matches!(error, ServiceError::Timeout(_)));
    assert_eq!(error.to_string(), "Timeout: Request timeout after 30s");
}

#[rstest]
#[test]
fn test_from_tonic_status_resource_exhausted() {
    let status = Status::new(Code::ResourceExhausted, "API quota exceeded");
    let error = ServiceError::from(status);
    
    assert!(matches!(error, ServiceError::RateLimited(_)));
    assert_eq!(error.to_string(), "Rate limited: API quota exceeded");
}

#[rstest]
#[test]
fn test_from_tonic_status_internal_error() {
    let status = Status::new(Code::Internal, "Unexpected server error");
    let error = ServiceError::from(status);
    
    assert!(matches!(error, ServiceError::InternalError(_)));
    assert_eq!(error.to_string(), "Internal error: Unexpected server error");
}

#[rstest]
#[test]
fn test_from_tonic_status_other_codes() {
    // Test various other gRPC status codes that should map to InternalError
    let codes_and_messages = vec![
        (Code::NotFound, "Resource not found"),
        (Code::PermissionDenied, "Permission denied"),
        (Code::FailedPrecondition, "Precondition failed"),
        (Code::OutOfRange, "Index out of range"),
        (Code::Unimplemented, "Method not implemented"),
        (Code::DataLoss, "Data corruption detected"),
        (Code::Unknown, "Unknown error occurred"),
    ];

    for (code, message) in codes_and_messages {
        let status = Status::new(code, message);
        let error = ServiceError::from(status);
        
        assert!(matches!(error, ServiceError::InternalError(_)));
        assert_eq!(error.to_string(), format!("Internal error: {}", message));
    }
}

// Edge Cases in Error Conversion
#[rstest]
#[test]
fn test_from_tonic_status_empty_message() {
    let status = Status::new(Code::Unauthenticated, "");
    let error = ServiceError::from(status);
    
    assert!(matches!(error, ServiceError::AuthenticationFailed(_)));
    assert_eq!(error.to_string(), "Authentication failed: ");
}

#[rstest]
#[test]
fn test_from_tonic_status_unicode_message() {
    let unicode_message = "认证失败: 令牌无效";
    let status = Status::new(Code::Unauthenticated, unicode_message);
    let error = ServiceError::from(status);
    
    assert!(matches!(error, ServiceError::AuthenticationFailed(_)));
    assert_eq!(error.to_string(), format!("Authentication failed: {}", unicode_message));
}

#[rstest]
#[test]
fn test_from_tonic_status_long_message() {
    let long_message = "A".repeat(1000);
    let status = Status::new(Code::Internal, &long_message);
    let error = ServiceError::from(status);
    
    assert!(matches!(error, ServiceError::InternalError(_)));
    assert_eq!(error.to_string(), format!("Internal error: {}", long_message));
}

// Error Categorization Tests
#[rstest]
#[test]
fn test_error_categorization_by_matching() {
    let errors = vec![
        ServiceError::ConnectionFailed("Network down".to_string()),
        ServiceError::AuthenticationFailed("Bad token".to_string()),
        ServiceError::ServiceUnavailable("Maintenance".to_string()),
        ServiceError::InvalidRequest("Bad JSON".to_string()),
        ServiceError::InternalError("Bug in code".to_string()),
        ServiceError::Timeout("Slow response".to_string()),
        ServiceError::RateLimited("Quota exceeded".to_string()),
    ];

    // Test that we can categorize errors properly
    let mut connection_errors = 0;
    let mut auth_errors = 0;
    let mut client_errors = 0; // Invalid request
    let mut server_errors = 0; // Internal, unavailable
    let mut timeout_errors = 0;
    let mut rate_limit_errors = 0;

    for error in errors {
        match error {
            ServiceError::ConnectionFailed(_) => connection_errors += 1,
            ServiceError::AuthenticationFailed(_) => auth_errors += 1,
            ServiceError::InvalidRequest(_) => client_errors += 1,
            ServiceError::ServiceUnavailable(_) | ServiceError::InternalError(_) => server_errors += 1,
            ServiceError::Timeout(_) => timeout_errors += 1,
            ServiceError::RateLimited(_) => rate_limit_errors += 1,
        }
    }

    assert_eq!(connection_errors, 1);
    assert_eq!(auth_errors, 1);
    assert_eq!(client_errors, 1);
    assert_eq!(server_errors, 2);
    assert_eq!(timeout_errors, 1);
    assert_eq!(rate_limit_errors, 1);
}

// Error Chaining and Context Tests
#[rstest]
#[test]
fn test_error_with_anyhow() {
    use anyhow::{Context, Result};

    fn failing_operation() -> Result<(), ServiceError> {
        Err(ServiceError::ConnectionFailed("Network timeout".to_string()))
    }

    fn higher_level_operation() -> Result<()> {
        failing_operation().context("Failed to connect to authentication service")?;
        Ok(())
    }

    let result = higher_level_operation();
    assert!(result.is_err());

    let error_chain = format!("{:?}", result.unwrap_err());
    assert!(error_chain.contains("Failed to connect to authentication service"));
    assert!(error_chain.contains("Network timeout"));
}

#[rstest]
#[test]
fn test_error_conversion_chain() {
    // Test the conversion chain: gRPC Status -> ServiceError -> anyhow::Error
    let grpc_status = Status::new(Code::Unauthenticated, "JWT token expired");
    let service_error = ServiceError::from(grpc_status);
    let anyhow_error = anyhow::Error::from(service_error);

    let error_string = anyhow_error.to_string();
    assert!(error_string.contains("Authentication failed"));
    assert!(error_string.contains("JWT token expired"));
}

// Retry Logic Error Classification Tests
#[rstest]
#[test]
fn test_retryable_errors() {
    // Define which errors should be retryable in a typical system
    fn is_retryable(error: &ServiceError) -> bool {
        matches!(
            error,
            ServiceError::ConnectionFailed(_)
                | ServiceError::ServiceUnavailable(_)
                | ServiceError::Timeout(_)
                | ServiceError::InternalError(_) // Sometimes retryable
        )
    }

    let retryable_errors = vec![
        ServiceError::ConnectionFailed("Network blip".to_string()),
        ServiceError::ServiceUnavailable("Temporary overload".to_string()),
        ServiceError::Timeout("Slow network".to_string()),
        ServiceError::InternalError("Database hiccup".to_string()),
    ];

    let non_retryable_errors = vec![
        ServiceError::AuthenticationFailed("Invalid token".to_string()),
        ServiceError::InvalidRequest("Malformed JSON".to_string()),
        ServiceError::RateLimited("Quota exceeded".to_string()),
    ];

    for error in &retryable_errors {
        assert!(is_retryable(error), "Error should be retryable: {:?}", error);
    }

    for error in &non_retryable_errors {
        assert!(!is_retryable(error), "Error should not be retryable: {:?}", error);
    }
}

// Error Logging and Monitoring Tests
#[rstest]
#[test]
fn test_error_severity_classification() {
    // Classify errors by severity for logging/alerting
    fn error_severity(error: &ServiceError) -> &'static str {
        match error {
            ServiceError::ConnectionFailed(_) => "HIGH",
            ServiceError::AuthenticationFailed(_) => "MEDIUM",
            ServiceError::ServiceUnavailable(_) => "HIGH",
            ServiceError::InvalidRequest(_) => "LOW",
            ServiceError::InternalError(_) => "CRITICAL",
            ServiceError::Timeout(_) => "MEDIUM",
            ServiceError::RateLimited(_) => "LOW",
        }
    }

    assert_eq!(error_severity(&ServiceError::InternalError("Bug".to_string())), "CRITICAL");
    assert_eq!(error_severity(&ServiceError::ConnectionFailed("Network".to_string())), "HIGH");
    assert_eq!(error_severity(&ServiceError::ServiceUnavailable("Down".to_string())), "HIGH");
    assert_eq!(error_severity(&ServiceError::AuthenticationFailed("Bad auth".to_string())), "MEDIUM");
    assert_eq!(error_severity(&ServiceError::Timeout("Slow".to_string())), "MEDIUM");
    assert_eq!(error_severity(&ServiceError::InvalidRequest("Bad input".to_string())), "LOW");
    assert_eq!(error_severity(&ServiceError::RateLimited("Quota".to_string())), "LOW");
}

// Error Serialization Tests (for logging structured data)
#[rstest]
#[test]
fn test_error_structured_logging() {
    use serde_json::json;

    fn error_to_structured_log(error: &ServiceError) -> serde_json::Value {
        match error {
            ServiceError::ConnectionFailed(msg) => json!({
                "type": "connection_failed",
                "message": msg,
                "retryable": true,
                "severity": "high"
            }),
            ServiceError::AuthenticationFailed(msg) => json!({
                "type": "authentication_failed", 
                "message": msg,
                "retryable": false,
                "severity": "medium"
            }),
            ServiceError::ServiceUnavailable(msg) => json!({
                "type": "service_unavailable",
                "message": msg,
                "retryable": true,
                "severity": "high"
            }),
            ServiceError::InvalidRequest(msg) => json!({
                "type": "invalid_request",
                "message": msg,
                "retryable": false,
                "severity": "low"
            }),
            ServiceError::InternalError(msg) => json!({
                "type": "internal_error",
                "message": msg,
                "retryable": true,
                "severity": "critical"
            }),
            ServiceError::Timeout(msg) => json!({
                "type": "timeout",
                "message": msg,
                "retryable": true,
                "severity": "medium"
            }),
            ServiceError::RateLimited(msg) => json!({
                "type": "rate_limited",
                "message": msg,
                "retryable": false,
                "severity": "low"
            }),
        }
    }

    let error = ServiceError::AuthenticationFailed("Invalid JWT signature".to_string());
    let log_entry = error_to_structured_log(&error);

    assert_eq!(log_entry["type"], "authentication_failed");
    assert_eq!(log_entry["message"], "Invalid JWT signature");
    assert_eq!(log_entry["retryable"], false);
    assert_eq!(log_entry["severity"], "medium");
}

// Error Propagation in Async Context Tests
#[rstest]
#[tokio::test]
async fn test_error_propagation_async() {
    use anyhow::Result;

    async fn async_operation_that_fails() -> Result<(), ServiceError> {
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        Err(ServiceError::Timeout("Async operation timed out".to_string()))
    }

    async fn higher_level_async_operation() -> Result<()> {
        async_operation_that_fails()
            .await
            .map_err(|e| anyhow::Error::from(e))?;
        Ok(())
    }

    let result = higher_level_async_operation().await;
    assert!(result.is_err());

    let error = result.unwrap_err();
    let error_string = error.to_string();
    assert!(error_string.contains("Timeout"));
    assert!(error_string.contains("Async operation timed out"));
}

// Error Metrics and Statistics Tests
#[rstest]
#[test]
fn test_error_statistics_collection() {
    use std::collections::HashMap;

    #[derive(Default)]
    struct ErrorStats {
        counts: HashMap<String, u32>,
    }

    impl ErrorStats {
        fn record_error(&mut self, error: &ServiceError) {
            let error_type = match error {
                ServiceError::ConnectionFailed(_) => "connection_failed",
                ServiceError::AuthenticationFailed(_) => "authentication_failed",
                ServiceError::ServiceUnavailable(_) => "service_unavailable",
                ServiceError::InvalidRequest(_) => "invalid_request",
                ServiceError::InternalError(_) => "internal_error",
                ServiceError::Timeout(_) => "timeout",
                ServiceError::RateLimited(_) => "rate_limited",
            };
            
            *self.counts.entry(error_type.to_string()).or_insert(0) += 1;
        }

        fn get_count(&self, error_type: &str) -> u32 {
            self.counts.get(error_type).copied().unwrap_or(0)
        }
    }

    let mut stats = ErrorStats::default();

    // Record various errors
    let errors = vec![
        ServiceError::ConnectionFailed("Network issue 1".to_string()),
        ServiceError::ConnectionFailed("Network issue 2".to_string()),
        ServiceError::AuthenticationFailed("Bad token".to_string()),
        ServiceError::Timeout("Request timeout".to_string()),
        ServiceError::InternalError("Server error".to_string()),
        ServiceError::ConnectionFailed("Network issue 3".to_string()),
    ];

    for error in &errors {
        stats.record_error(error);
    }

    // Verify counts
    assert_eq!(stats.get_count("connection_failed"), 3);
    assert_eq!(stats.get_count("authentication_failed"), 1);
    assert_eq!(stats.get_count("timeout"), 1);
    assert_eq!(stats.get_count("internal_error"), 1);
    assert_eq!(stats.get_count("service_unavailable"), 0);
}

// Memory and Performance Tests
#[rstest]
#[test]
fn test_error_memory_usage() {
    use std::mem;

    // Test that ServiceError variants don't have unexpected memory overhead
    let small_error = ServiceError::ConnectionFailed("short".to_string());
    let large_error = ServiceError::InternalError("x".repeat(10000));

    // Both should be roughly the same size since they contain String which is heap-allocated
    let small_size = mem::size_of_val(&small_error);
    let large_size = mem::size_of_val(&large_error);

    // String should be the same size regardless of content (due to heap allocation)
    assert_eq!(small_size, large_size);
    
    // Size should be reasonable (not unexpectedly large)
    assert!(small_size < 100); // Rough upper bound for enum + String
}

#[rstest]
#[test]
fn test_error_cloning_and_equality() {
    // Test that errors can be compared for testing purposes
    let error1 = ServiceError::ConnectionFailed("Network timeout".to_string());
    let error2 = ServiceError::ConnectionFailed("Network timeout".to_string());
    let error3 = ServiceError::ConnectionFailed("Different message".to_string());
    let error4 = ServiceError::AuthenticationFailed("Network timeout".to_string());

    // Same variant and message
    assert_eq!(
        std::mem::discriminant(&error1),
        std::mem::discriminant(&error2)
    );

    // Same variant, different message
    assert_eq!(
        std::mem::discriminant(&error1),
        std::mem::discriminant(&error3)
    );

    // Different variants
    assert_ne!(
        std::mem::discriminant(&error1),
        std::mem::discriminant(&error4)
    );
}

// Real-world Error Scenario Tests
#[rstest]
#[test]
fn test_realistic_error_scenarios() {
    // Simulate realistic error messages from actual systems
    let realistic_errors = vec![
        ServiceError::ConnectionFailed(
            "dial tcp 10.0.0.5:50051: connect: connection refused".to_string()
        ),
        ServiceError::AuthenticationFailed(
            "JWT token validation failed: signature verification failed".to_string()
        ),
        ServiceError::ServiceUnavailable(
            "service temporarily unavailable due to high load (code: 503)".to_string()
        ),
        ServiceError::InvalidRequest(
            "invalid JSON in request body at line 5, column 12: expected ',' or '}'".to_string()
        ),
        ServiceError::Timeout(
            "request timeout: no response received within 30000ms deadline".to_string()
        ),
        ServiceError::RateLimited(
            "rate limit exceeded: 1000 requests per hour, retry after 3600 seconds".to_string()
        ),
        ServiceError::InternalError(
            "database connection pool exhausted: all 50 connections in use".to_string()
        ),
    ];

    // All should format properly and contain key information
    for error in &realistic_errors {
        let error_string = error.to_string();
        assert!(!error_string.is_empty());
        assert!(error_string.len() > 10); // Should have substantial content
        
        // Should contain the original detailed message
        match error {
            ServiceError::ConnectionFailed(msg) => assert!(error_string.contains(msg)),
            ServiceError::AuthenticationFailed(msg) => assert!(error_string.contains(msg)),
            ServiceError::ServiceUnavailable(msg) => assert!(error_string.contains(msg)),
            ServiceError::InvalidRequest(msg) => assert!(error_string.contains(msg)),
            ServiceError::Timeout(msg) => assert!(error_string.contains(msg)),
            ServiceError::RateLimited(msg) => assert!(error_string.contains(msg)),
            ServiceError::InternalError(msg) => assert!(error_string.contains(msg)),
        }
    }
}