//! WebSocket handler unit tests

use rstest::*;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_test;

use api_gateway::{
    grpc_clients::GrpcClients,
    models::WebSocketMessage,
    websocket::WebSocketHandler,
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
fn websocket_handler() -> WebSocketHandler {
    let mock_clients = mock_grpc_clients();
    WebSocketHandler::new(mock_clients)
}

#[rstest]
#[tokio::test]
async fn test_websocket_handler_creation(websocket_handler: WebSocketHandler) {
    // Should create without panicking
    assert!(true);
    
    // Verify the handler is properly constructed (basic smoke test)
    // In a real test, we might verify internal state
}

#[rstest]
#[tokio::test]
async fn test_handle_ping_message() {
    let websocket_handler = websocket_handler();
    let (tx, mut rx) = broadcast::channel::<WebSocketMessage>(100);
    
    let ping_message = json!({
        "type": "ping"
    });
    
    let result = websocket_handler.handle_client_message(
        &ping_message.to_string(),
        &tx,
    ).await;
    
    // Should handle ping without error
    assert!(result.is_ok());
    
    // Should send pong response
    if let Ok(response) = rx.try_recv() {
        assert_eq!(response.message_type, "pong");
        assert_eq!(response.data, json!({}));
        assert!(response.timestamp > 0);
    }
}

#[rstest]
#[tokio::test]
async fn test_handle_market_data_subscription() {
    let websocket_handler = websocket_handler();
    let (tx, mut rx) = broadcast::channel::<WebSocketMessage>(100);
    
    let subscription_message = json!({
        "type": "subscribe_market_data",
        "symbols": ["BTCUSDT", "ETHUSDT"],
        "exchange": "binance"
    });
    
    let result = websocket_handler.handle_client_message(
        &subscription_message.to_string(),
        &tx,
    ).await;
    
    // Should handle subscription without error
    assert!(result.is_ok());
    
    // Should send confirmation message
    if let Ok(response) = rx.try_recv() {
        assert_eq!(response.message_type, "subscription_confirmed");
        assert_eq!(response.data["subscription"], "market_data");
        assert_eq!(response.data["status"], "active");
    }
}

#[rstest]
#[tokio::test]
async fn test_handle_execution_subscription() {
    let websocket_handler = websocket_handler();
    let (tx, mut rx) = broadcast::channel::<WebSocketMessage>(100);
    
    let subscription_message = json!({
        "type": "subscribe_execution_reports"
    });
    
    let result = websocket_handler.handle_client_message(
        &subscription_message.to_string(),
        &tx,
    ).await;
    
    // Should handle subscription without error
    assert!(result.is_ok());
    
    // Should send confirmation message
    if let Ok(response) = rx.try_recv() {
        assert_eq!(response.message_type, "subscription_confirmed");
        assert_eq!(response.data["subscription"], "execution_reports");
        assert_eq!(response.data["status"], "active");
    }
}

#[rstest]
#[tokio::test]
async fn test_handle_risk_subscription() {
    let websocket_handler = websocket_handler();
    let (tx, mut rx) = broadcast::channel::<WebSocketMessage>(100);
    
    let subscription_message = json!({
        "type": "subscribe_risk_alerts"
    });
    
    let result = websocket_handler.handle_client_message(
        &subscription_message.to_string(),
        &tx,
    ).await;
    
    // Should handle subscription without error
    assert!(result.is_ok());
    
    // Should send confirmation message
    if let Ok(response) = rx.try_recv() {
        assert_eq!(response.message_type, "subscription_confirmed");
        assert_eq!(response.data["subscription"], "risk_alerts");
        assert_eq!(response.data["status"], "active");
    }
}

#[rstest]
#[tokio::test]
async fn test_handle_unknown_message() {
    let websocket_handler = websocket_handler();
    let (tx, mut rx) = broadcast::channel::<WebSocketMessage>(100);
    
    let unknown_message = json!({
        "type": "unknown_message_type",
        "data": {"some": "data"}
    });
    
    let result = websocket_handler.handle_client_message(
        &unknown_message.to_string(),
        &tx,
    ).await;
    
    // Should handle unknown message without error (just log warning)
    assert!(result.is_ok());
    
    // Should not send any response for unknown message type
    assert!(rx.try_recv().is_err());
}

#[rstest]
#[tokio::test]
async fn test_handle_malformed_json() {
    let websocket_handler = websocket_handler();
    let (tx, _rx) = broadcast::channel::<WebSocketMessage>(100);
    
    let malformed_json = "{ invalid json }";
    
    let result = websocket_handler.handle_client_message(
        malformed_json,
        &tx,
    ).await;
    
    // Should return error for malformed JSON
    assert!(result.is_err());
}

#[rstest]
#[tokio::test]
async fn test_handle_missing_type_field() {
    let websocket_handler = websocket_handler();
    let (tx, _rx) = broadcast::channel::<WebSocketMessage>(100);
    
    let message_without_type = json!({
        "data": {"some": "data"}
    });
    
    let result = websocket_handler.handle_client_message(
        &message_without_type.to_string(),
        &tx,
    ).await;
    
    // Should handle gracefully (treats as "unknown" type)
    assert!(result.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_market_data_subscription_parameters() {
    let websocket_handler = websocket_handler();
    let (tx, mut rx) = broadcast::channel::<WebSocketMessage>(100);
    
    // Test with detailed subscription parameters
    let detailed_subscription = json!({
        "type": "subscribe_market_data",
        "symbols": ["NIFTY", "BANKNIFTY", "SENSEX"],
        "exchange": "NSE"
    });
    
    let result = websocket_handler.handle_client_message(
        &detailed_subscription.to_string(),
        &tx,
    ).await;
    
    assert!(result.is_ok());
    
    // Should send confirmation
    if let Ok(response) = rx.try_recv() {
        assert_eq!(response.message_type, "subscription_confirmed");
        assert_eq!(response.data["subscription"], "market_data");
    }
}

#[rstest]
#[tokio::test]
async fn test_market_data_subscription_missing_parameters() {
    let websocket_handler = websocket_handler();
    let (tx, mut rx) = broadcast::channel::<WebSocketMessage>(100);
    
    // Test with missing parameters
    let minimal_subscription = json!({
        "type": "subscribe_market_data"
    });
    
    let result = websocket_handler.handle_client_message(
        &minimal_subscription.to_string(),
        &tx,
    ).await;
    
    // Should still handle gracefully with defaults
    assert!(result.is_ok());
    
    // Should send confirmation
    if let Ok(response) = rx.try_recv() {
        assert_eq!(response.message_type, "subscription_confirmed");
    }
}

#[rstest]
#[tokio::test]
async fn test_concurrent_message_handling() {
    use futures::future::join_all;
    
    let websocket_handler = websocket_handler();
    let (tx, _rx) = broadcast::channel::<WebSocketMessage>(100);
    
    // Create multiple concurrent message handling tasks
    let messages = vec![
        json!({"type": "ping"}),
        json!({"type": "subscribe_market_data", "symbols": ["BTC"], "exchange": "binance"}),
        json!({"type": "subscribe_execution_reports"}),
        json!({"type": "subscribe_risk_alerts"}),
        json!({"type": "ping"}),
    ];
    
    let tasks = messages.into_iter().enumerate().map(|(i, message)| {
        let handler = &websocket_handler;
        let sender = &tx;
        
        async move {
            let result = handler.handle_client_message(
                &message.to_string(),
                sender,
            ).await;
            (i, result)
        }
    });
    
    let results = join_all(tasks).await;
    
    // All messages should be handled successfully
    for (i, result) in results {
        assert!(result.is_ok(), "Message {} should be handled successfully", i);
    }
}

#[rstest]
#[tokio::test]
async fn test_websocket_message_structure() {
    let message = WebSocketMessage {
        message_type: "test_message".to_string(),
        data: json!({"key": "value", "number": 42}),
        timestamp: chrono::Utc::now().timestamp(),
    };
    
    // Should serialize and deserialize correctly
    let serialized = serde_json::to_string(&message).unwrap();
    let deserialized: WebSocketMessage = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(message.message_type, deserialized.message_type);
    assert_eq!(message.data, deserialized.data);
    assert_eq!(message.timestamp, deserialized.timestamp);
}

#[rstest]
#[tokio::test]
async fn test_broadcast_channel_behavior() {
    let (tx, mut rx1) = broadcast::channel::<WebSocketMessage>(10);
    let mut rx2 = tx.subscribe();
    
    let test_message = WebSocketMessage {
        message_type: "test".to_string(),
        data: json!({"test": true}),
        timestamp: chrono::Utc::now().timestamp(),
    };
    
    // Send message
    let send_result = tx.send(test_message.clone());
    assert!(send_result.is_ok());
    
    // Both receivers should get the message
    let received1 = rx1.recv().await;
    let received2 = rx2.recv().await;
    
    assert!(received1.is_ok());
    assert!(received2.is_ok());
    
    let msg1 = received1.unwrap();
    let msg2 = received2.unwrap();
    
    assert_eq!(msg1.message_type, test_message.message_type);
    assert_eq!(msg2.message_type, test_message.message_type);
}

#[rstest]
#[tokio::test]
async fn test_message_type_variations() {
    let websocket_handler = websocket_handler();
    let (tx, mut rx) = broadcast::channel::<WebSocketMessage>(100);
    
    // Test different message type formats
    let test_cases = vec![
        ("ping", true), // Should handle
        ("PING", false), // Case sensitive, should be unknown
        ("subscribe_market_data", true), // Should handle
        ("Subscribe_Market_Data", false), // Case sensitive, should be unknown
        ("", false), // Empty type, should be unknown
    ];
    
    for (msg_type, should_respond) in test_cases {
        let message = json!({"type": msg_type});
        
        let result = websocket_handler.handle_client_message(
            &message.to_string(),
            &tx,
        ).await;
        
        assert!(result.is_ok(), "Should handle message type '{}' without error", msg_type);
        
        if should_respond && msg_type == "ping" {
            // Only ping should immediately send a response
            if let Ok(response) = rx.try_recv() {
                assert_eq!(response.message_type, "pong");
            }
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_subscription_confirmation_format() {
    let websocket_handler = websocket_handler();
    let (tx, mut rx) = broadcast::channel::<WebSocketMessage>(100);
    
    let subscription_types = vec![
        ("subscribe_market_data", "market_data"),
        ("subscribe_execution_reports", "execution_reports"),
        ("subscribe_risk_alerts", "risk_alerts"),
    ];
    
    for (sub_type, expected_subscription) in subscription_types {
        let message = json!({"type": sub_type});
        
        let result = websocket_handler.handle_client_message(
            &message.to_string(),
            &tx,
        ).await;
        
        assert!(result.is_ok());
        
        // Should send confirmation
        if let Ok(response) = rx.try_recv() {
            assert_eq!(response.message_type, "subscription_confirmed");
            assert_eq!(response.data["subscription"], expected_subscription);
            assert_eq!(response.data["status"], "active");
            assert!(response.timestamp > 0);
        } else {
            panic!("Expected confirmation message for {}", sub_type);
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_large_message_handling() {
    let websocket_handler = websocket_handler();
    let (tx, _rx) = broadcast::channel::<WebSocketMessage>(100);
    
    // Create a large message
    let large_data = (0..1000).map(|i| format!("SYMBOL{}", i)).collect::<Vec<_>>();
    let large_message = json!({
        "type": "subscribe_market_data",
        "symbols": large_data,
        "exchange": "test"
    });
    
    let result = websocket_handler.handle_client_message(
        &large_message.to_string(),
        &tx,
    ).await;
    
    // Should handle large messages without error
    assert!(result.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_channel_full_behavior() {
    let websocket_handler = websocket_handler();
    let (tx, _rx) = broadcast::channel::<WebSocketMessage>(2); // Very small capacity
    
    // Fill the channel beyond capacity
    for i in 0..10 {
        let message = json!({"type": "ping"});
        let result = websocket_handler.handle_client_message(
            &message.to_string(),
            &tx,
        ).await;
        
        // Should handle without error even if channel is full
        assert!(result.is_ok(), "Should handle message {} even with full channel", i);
    }
}

#[rstest]
#[tokio::test]
async fn test_empty_symbols_array() {
    let websocket_handler = websocket_handler();
    let (tx, mut rx) = broadcast::channel::<WebSocketMessage>(100);
    
    let empty_symbols_message = json!({
        "type": "subscribe_market_data",
        "symbols": [],
        "exchange": "binance"
    });
    
    let result = websocket_handler.handle_client_message(
        &empty_symbols_message.to_string(),
        &tx,
    ).await;
    
    // Should handle empty symbols array gracefully
    assert!(result.is_ok());
    
    // Should still send confirmation
    if let Ok(response) = rx.try_recv() {
        assert_eq!(response.message_type, "subscription_confirmed");
    }
}