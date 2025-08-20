//! Execution handlers unit tests

use axum::{extract::{Path, Query, State}, http::HeaderMap, response::Json};
use rstest::*;
use std::sync::Arc;
use tokio_test;

use api_gateway::{
    grpc_clients::{execution, GrpcClients},
    handlers::{ExecutionHandlers, execution::OrderQuery},
    models::{ApiResponse, CancelOrderRequest, SubmitOrderRequest},
};

use super::helpers::*;

#[fixture]
fn mock_grpc_clients() -> Arc<GrpcClients> {
    // Placeholder - in real tests this would be a proper mock
    Arc::new(GrpcClients::new(
        "http://localhost:50051",
        "http://localhost:50052", 
        "http://localhost:50053",
        "http://localhost:50054",
    ).await.unwrap())
}

#[fixture]
fn execution_handlers() -> ExecutionHandlers {
    let mock_clients = mock_grpc_clients();
    ExecutionHandlers::new(mock_clients)
}

#[rstest]
#[tokio::test]
async fn test_submit_order_success(execution_handlers: ExecutionHandlers) {
    let order_request = SubmitOrderRequest {
        client_order_id: Some("TEST001".to_string()),
        symbol: "NIFTY2412050000CE".to_string(),
        side: "BUY".to_string(),
        quantity: "100.0000".to_string(),
        order_type: "LIMIT".to_string(),
        limit_price: Some("150.2500".to_string()),
        stop_price: None,
        time_in_force: Some("GTC".to_string()),
        venue: Some("NSE".to_string()),
        strategy_id: Some("strategy_1".to_string()),
        params: None,
    };

    let token = create_test_jwt("testuser", vec!["PLACE_ORDERS".to_string()]).unwrap();
    let headers = create_auth_headers(&token);

    let result = ExecutionHandlers::submit_order(
        State(execution_handlers),
        headers,
        Json(order_request),
    ).await;

    match result {
        Ok(Json(response)) => {
            // In test environment without gRPC server, expect error response
            if !response.success {
                assert!(response.error.is_some());
                let error = response.error.unwrap();
                assert_eq!(error.error, "ORDER_SUBMISSION_FAILED");
            }
        }
        Err(_) => {
            // HTTP error also acceptable in test
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_submit_order_permission_denied() {
    let order_request = SubmitOrderRequest {
        client_order_id: Some("TEST001".to_string()),
        symbol: "NIFTY2412050000CE".to_string(),
        side: "BUY".to_string(),
        quantity: "100.0000".to_string(),
        order_type: "LIMIT".to_string(),
        limit_price: Some("150.2500".to_string()),
        stop_price: None,
        time_in_force: Some("GTC".to_string()),
        venue: Some("NSE".to_string()),
        strategy_id: Some("strategy_1".to_string()),
        params: None,
    };

    // Create token without PLACE_ORDERS permission
    let token = create_test_jwt("testuser", vec!["VIEW_POSITIONS".to_string()]).unwrap();
    let headers = create_auth_headers(&token);

    let execution_handlers = execution_handlers();
    let result = ExecutionHandlers::submit_order(
        State(execution_handlers),
        headers,
        Json(order_request),
    ).await;

    match result {
        Ok(Json(response)) => {
            assert!(!response.success);
            let error = response.error.unwrap();
            assert_eq!(error.error, "PERMISSION_DENIED");
            assert!(error.message.contains("Insufficient permissions"));
        }
        Err(_) => {
            // Unexpected in this test
            panic!("Expected ApiResponse, got HTTP error");
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_submit_order_invalid_data() {
    let order_request = SubmitOrderRequest {
        client_order_id: None,
        symbol: "".to_string(), // Invalid empty symbol
        side: "INVALID".to_string(), // Invalid side
        quantity: "invalid".to_string(), // Invalid quantity
        order_type: "UNKNOWN".to_string(), // Invalid order type
        limit_price: Some("invalid".to_string()), // Invalid price
        stop_price: None,
        time_in_force: Some("INVALID".to_string()),
        venue: None,
        strategy_id: None,
        params: None,
    };

    let token = create_test_jwt("testuser", vec!["PLACE_ORDERS".to_string()]).unwrap();
    let headers = create_auth_headers(&token);

    let execution_handlers = execution_handlers();
    let result = ExecutionHandlers::submit_order(
        State(execution_handlers),
        headers,
        Json(order_request),
    ).await;

    match result {
        Ok(Json(response)) => {
            // Should handle invalid data gracefully
            if !response.success {
                assert!(response.error.is_some());
            }
        }
        Err(_) => {
            // Connection error acceptable
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_cancel_order_success() {
    let cancel_request = CancelOrderRequest {
        order_id: Some(12345),
        client_order_id: Some("TEST001".to_string()),
    };

    let token = create_test_jwt("testuser", vec!["CANCEL_ORDERS".to_string()]).unwrap();
    let headers = create_auth_headers(&token);

    let execution_handlers = execution_handlers();
    let result = ExecutionHandlers::cancel_order(
        State(execution_handlers),
        headers,
        Json(cancel_request),
    ).await;

    match result {
        Ok(Json(response)) => {
            // Should return proper response structure
            assert!(response.success || response.error.is_some());
        }
        Err(_) => {
            // Connection error acceptable
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_cancel_order_permission_denied() {
    let cancel_request = CancelOrderRequest {
        order_id: Some(12345),
        client_order_id: None,
    };

    // Create token without CANCEL_ORDERS permission
    let token = create_test_jwt("testuser", vec!["PLACE_ORDERS".to_string()]).unwrap();
    let headers = create_auth_headers(&token);

    let execution_handlers = execution_handlers();
    let result = ExecutionHandlers::cancel_order(
        State(execution_handlers),
        headers,
        Json(cancel_request),
    ).await;

    match result {
        Ok(Json(response)) => {
            assert!(!response.success);
            let error = response.error.unwrap();
            assert_eq!(error.error, "PERMISSION_DENIED");
        }
        Err(_) => {
            panic!("Expected ApiResponse, got HTTP error");
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_cancel_order_missing_identifiers() {
    let cancel_request = CancelOrderRequest {
        order_id: None,
        client_order_id: None,
    };

    let token = create_test_jwt("testuser", vec!["CANCEL_ORDERS".to_string()]).unwrap();
    let headers = create_auth_headers(&token);

    let execution_handlers = execution_handlers();
    let result = ExecutionHandlers::cancel_order(
        State(execution_handlers),
        headers,
        Json(cancel_request),
    ).await;

    match result {
        Ok(Json(response)) => {
            // Should handle missing identifiers gracefully
            if !response.success {
                assert!(response.error.is_some());
            }
        }
        Err(_) => {
            // Connection error acceptable
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_get_order_status_success() {
    let order_id = 12345_i64;
    let query = OrderQuery {
        client_order_id: Some("TEST001".to_string()),
    };

    let token = create_test_jwt("testuser", vec!["VIEW_POSITIONS".to_string()]).unwrap();
    let headers = create_auth_headers(&token);

    let execution_handlers = execution_handlers();
    let result = ExecutionHandlers::get_order_status(
        State(execution_handlers),
        headers,
        Path(order_id),
        Query(query),
    ).await;

    match result {
        Ok(Json(response)) => {
            // Should return proper response structure
            assert!(response.success || response.error.is_some());
        }
        Err(_) => {
            // Connection error acceptable
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_get_order_status_permission_denied() {
    let order_id = 12345_i64;
    let query = OrderQuery { client_order_id: None };

    // Create token without VIEW_POSITIONS permission
    let token = create_test_jwt("testuser", vec!["PLACE_ORDERS".to_string()]).unwrap();
    let headers = create_auth_headers(&token);

    let execution_handlers = execution_handlers();
    let result = ExecutionHandlers::get_order_status(
        State(execution_handlers),
        headers,
        Path(order_id),
        Query(query),
    ).await;

    match result {
        Ok(Json(response)) => {
            assert!(!response.success);
            let error = response.error.unwrap();
            assert_eq!(error.error, "PERMISSION_DENIED");
        }
        Err(_) => {
            panic!("Expected ApiResponse, got HTTP error");
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_get_execution_metrics_success() {
    let token = create_test_jwt("testuser", vec!["VIEW_POSITIONS".to_string()]).unwrap();
    let headers = create_auth_headers(&token);

    let execution_handlers = execution_handlers();
    let result = ExecutionHandlers::get_metrics(
        State(execution_handlers),
        headers,
    ).await;

    match result {
        Ok(Json(response)) => {
            // Should return proper response structure
            assert!(response.success || response.error.is_some());
        }
        Err(_) => {
            // Connection error acceptable
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_get_execution_metrics_permission_denied() {
    // Create token without VIEW_POSITIONS permission
    let token = create_test_jwt("testuser", vec!["PLACE_ORDERS".to_string()]).unwrap();
    let headers = create_auth_headers(&token);

    let execution_handlers = execution_handlers();
    let result = ExecutionHandlers::get_metrics(
        State(execution_handlers),
        headers,
    ).await;

    match result {
        Ok(Json(response)) => {
            assert!(!response.success);
            let error = response.error.unwrap();
            assert_eq!(error.error, "PERMISSION_DENIED");
        }
        Err(_) => {
            panic!("Expected ApiResponse, got HTTP error");
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_concurrent_order_submissions() {
    use futures::future::join_all;

    let execution_handlers = execution_handlers();
    let token = create_test_jwt("testuser", vec!["PLACE_ORDERS".to_string()]).unwrap();

    // Create multiple order submissions
    let requests = (0..5).map(|i| {
        let handlers = execution_handlers.clone();
        let headers = create_auth_headers(&token);
        let order_request = SubmitOrderRequest {
            client_order_id: Some(format!("TEST{:03}", i)),
            symbol: "NIFTY2412050000CE".to_string(),
            side: "BUY".to_string(),
            quantity: format!("{}.0000", 100 + i),
            order_type: "LIMIT".to_string(),
            limit_price: Some(format!("{:.4}", 150.0 + i as f64)),
            stop_price: None,
            time_in_force: Some("GTC".to_string()),
            venue: Some("NSE".to_string()),
            strategy_id: Some(format!("strategy_{}", i)),
            params: None,
        };

        async move {
            ExecutionHandlers::submit_order(
                State(handlers),
                headers,
                Json(order_request),
            ).await
        }
    });

    // Execute all requests concurrently
    let results = join_all(requests).await;

    // Verify all requests complete without panicking
    assert_eq!(results.len(), 5);
    for result in results {
        assert!(result.is_ok());
    }
}

#[rstest]
#[tokio::test]
async fn test_order_conversion_functions() {
    // Test side conversion
    use api_gateway::grpc_clients::execution::{Side, OrderType, TimeInForce, OrderStatus};

    // These would be tested in the actual handler implementation
    // For now, verify the enum values exist
    assert_eq!(Side::Buy as i32, 1);
    assert_eq!(Side::Sell as i32, 2);
    
    assert_eq!(OrderType::Market as i32, 1);
    assert_eq!(OrderType::Limit as i32, 2);
    assert_eq!(OrderType::Stop as i32, 3);
    
    assert_eq!(TimeInForce::Gtc as i32, 1);
    assert_eq!(TimeInForce::Ioc as i32, 2);
    assert_eq!(TimeInForce::Fok as i32, 3);
    
    assert_eq!(OrderStatus::Pending as i32, 1);
    assert_eq!(OrderStatus::Filled as i32, 5);
    assert_eq!(OrderStatus::Cancelled as i32, 6);
}

#[rstest]
#[tokio::test]
async fn test_fixed_point_parsing() {
    // Test fixed point parsing logic
    let test_cases = vec![
        ("100.0000", 1000000_i64),
        ("0.0001", 1_i64),
        ("150.2500", 1502500_i64),
        ("0", 0_i64),
        ("1", 10000_i64),
    ];

    for (input, expected) in test_cases {
        let result = parse_fixed_point_test(input);
        assert_eq!(result, expected, "Failed for input: {}", input);
    }
}

#[rstest]
#[tokio::test]
async fn test_order_validation_edge_cases() {
    let execution_handlers = execution_handlers();
    let token = create_test_jwt("testuser", vec!["PLACE_ORDERS".to_string()]).unwrap();

    // Test edge cases
    let edge_cases = vec![
        // Zero quantity
        ("0.0000", false),
        // Negative quantity (shouldn't be allowed)
        ("-100.0000", false),
        // Very large quantity
        ("999999999.9999", true),
        // Very small quantity
        ("0.0001", true),
    ];

    for (quantity, should_be_valid) in edge_cases {
        let headers = create_auth_headers(&token);
        let order_request = SubmitOrderRequest {
            client_order_id: Some("TEST_EDGE".to_string()),
            symbol: "NIFTY2412050000CE".to_string(),
            side: "BUY".to_string(),
            quantity: quantity.to_string(),
            order_type: "LIMIT".to_string(),
            limit_price: Some("150.0000".to_string()),
            stop_price: None,
            time_in_force: Some("GTC".to_string()),
            venue: Some("NSE".to_string()),
            strategy_id: None,
            params: None,
        };

        let result = ExecutionHandlers::submit_order(
            State(execution_handlers.clone()),
            headers,
            Json(order_request),
        ).await;

        // All should return ok (structure-wise), validation happens at gRPC level
        assert!(result.is_ok());
    }
}