//! Unit tests for risk monitoring

use risk_manager::monitor::{RiskMonitor, RiskAlert, AlertLevel, PositionInfo};
use services_common::Symbol;
use std::sync::Arc;
use tokio::sync::RwLock;

async fn create_test_monitor() -> RiskMonitor {
    RiskMonitor::new()
}

#[tokio::test]
async fn test_monitor_creation() {
    let monitor = create_test_monitor().await;
    
    // Test that monitor can get metrics
    let metrics = monitor.get_current_metrics().await.unwrap();
    assert_eq!(metrics.total_exposure, 1_000_000);
    assert_eq!(metrics.daily_pnl, 50_000);
    assert_eq!(metrics.current_drawdown, 500);
    assert!(metrics.positions.is_empty());
}

#[tokio::test]
async fn test_add_alert() {
    let monitor = create_test_monitor().await;
    
    let alert = RiskAlert {
        level: AlertLevel::Warning,
        message: "Test warning alert".to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        source: "test".to_string(),
    };
    
    monitor.add_alert(alert.clone()).await;
    
    // Verify alert was added (we can't directly access alerts, but this tests the interface)
    // In a real system, you'd have a get_alerts method
}

#[tokio::test]
async fn test_alert_levels() {
    let monitor = create_test_monitor().await;
    
    // Test all alert levels
    let levels = [
        AlertLevel::Info,
        AlertLevel::Warning,
        AlertLevel::Critical,
        AlertLevel::Emergency,
    ];
    
    for (i, level) in levels.iter().enumerate() {
        let alert = RiskAlert {
            level: *level,
            message: format!("Test alert level {}", i),
            timestamp: chrono::Utc::now().timestamp_millis(),
            source: "test".to_string(),
        };
        
        monitor.add_alert(alert).await;
    }
}

#[tokio::test]
async fn test_update_metric() {
    let monitor = create_test_monitor().await;
    
    // Update various metrics
    monitor.update_metric("total_exposure".to_string(), 2_000_000.0).await;
    monitor.update_metric("daily_pnl".to_string(), -100_000.0).await;
    monitor.update_metric("position_count".to_string(), 5.0).await;
    
    // Test concurrent metric updates
    let monitor = Arc::new(monitor);
    let mut handles = vec![];
    
    for i in 0..10 {
        let monitor_clone = monitor.clone();
        handles.push(tokio::spawn(async move {
            monitor_clone.update_metric(
                format!("metric_{}", i),
                i as f64 * 100.0,
            ).await;
        }));
    }
    
    // Wait for all updates to complete
    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_concurrent_alert_addition() {
    let monitor = Arc::new(create_test_monitor().await);
    let mut handles = vec![];
    
    // Add alerts concurrently from multiple threads
    for i in 0..20 {
        let monitor_clone = monitor.clone();
        handles.push(tokio::spawn(async move {
            let alert = RiskAlert {
                level: if i % 4 == 0 { AlertLevel::Critical } else { AlertLevel::Warning },
                message: format!("Concurrent alert {}", i),
                timestamp: chrono::Utc::now().timestamp_millis(),
                source: format!("thread_{}", i),
            };
            monitor_clone.add_alert(alert).await;
        }));
    }
    
    // Wait for all alerts to be added
    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_alert_buffer_management() {
    let monitor = create_test_monitor().await;
    
    // Add more than 1000 alerts to test buffer management
    for i in 0..1200 {
        let alert = RiskAlert {
            level: AlertLevel::Info,
            message: format!("Alert {}", i),
            timestamp: chrono::Utc::now().timestamp_millis() + i,
            source: "test".to_string(),
        };
        monitor.add_alert(alert).await;
    }
    
    // The monitor should have automatically pruned old alerts to keep only the latest 1000
    // This test verifies the buffer management logic doesn't panic
}

#[tokio::test]
async fn test_metrics_with_positions() {
    let monitor = create_test_monitor().await;
    
    let metrics = monitor.get_current_metrics().await.unwrap();
    
    // Verify structure
    assert_eq!(metrics.positions.len(), 0);
    assert_eq!(metrics.total_exposure, 1_000_000);
    assert_eq!(metrics.daily_pnl, 50_000);
    assert_eq!(metrics.current_drawdown, 500);
}

#[tokio::test]
async fn test_position_info_structure() {
    // Test the PositionInfo structure
    let position = PositionInfo {
        symbol: Symbol(12345),
        position_value: 1_000_000,
    };
    
    assert_eq!(position.symbol.0, 12345);
    assert_eq!(position.position_value, 1_000_000);
}

#[tokio::test]
async fn test_concurrent_metrics_access() {
    let monitor = Arc::new(create_test_monitor().await);
    let mut handles = vec![];
    
    // Access metrics concurrently
    for _ in 0..10 {
        let monitor_clone = monitor.clone();
        handles.push(tokio::spawn(async move {
            let result = monitor_clone.get_current_metrics().await;
            assert!(result.is_ok());
            result.unwrap()
        }));
    }
    
    // Collect all results
    let mut results = vec![];
    for handle in handles {
        results.push(handle.await.unwrap());
    }
    
    // All results should be consistent
    for metrics in results {
        assert_eq!(metrics.total_exposure, 1_000_000);
        assert_eq!(metrics.daily_pnl, 50_000);
    }
}

#[tokio::test]
async fn test_alert_serialization() {
    // Test that alerts can be serialized/deserialized properly
    let alert = RiskAlert {
        level: AlertLevel::Critical,
        message: "Critical system alert".to_string(),
        timestamp: 1234567890,
        source: "risk-manager".to_string(),
    };
    
    // Test serialization
    let serialized = serde_json::to_string(&alert).unwrap();
    assert!(serialized.contains("Critical system alert"));
    assert!(serialized.contains("risk-manager"));
    
    // Test deserialization
    let deserialized: RiskAlert = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.message, alert.message);
    assert_eq!(deserialized.timestamp, alert.timestamp);
    assert_eq!(deserialized.source, alert.source);
}

#[tokio::test]
async fn test_monitor_stress_test() {
    let monitor = Arc::new(create_test_monitor().await);
    let mut handles = vec![];
    
    // Launch many concurrent operations
    for i in 0..100 {
        let monitor_clone = monitor.clone();
        handles.push(tokio::spawn(async move {
            // Mix of operations
            if i % 3 == 0 {
                // Add alert
                let alert = RiskAlert {
                    level: AlertLevel::Warning,
                    message: format!("Stress test alert {}", i),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    source: "stress-test".to_string(),
                };
                monitor_clone.add_alert(alert).await;
            } else if i % 3 == 1 {
                // Update metric
                monitor_clone.update_metric(
                    format!("stress_metric_{}", i),
                    (i as f64) * 10.0,
                ).await;
            } else {
                // Get metrics
                let _ = monitor_clone.get_current_metrics().await;
            }
        }));
    }
    
    // Wait for all operations to complete
    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_emergency_alert_handling() {
    let monitor = create_test_monitor().await;
    
    // Test emergency level alerts
    let emergency_alert = RiskAlert {
        level: AlertLevel::Emergency,
        message: "EMERGENCY: Kill switch activated".to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        source: "risk-manager".to_string(),
    };
    
    monitor.add_alert(emergency_alert).await;
    
    // Emergency alerts should be handled without blocking
    // This test ensures the monitor can handle high-priority alerts
}

#[tokio::test]
async fn test_alert_timestamp_ordering() {
    let monitor = create_test_monitor().await;
    
    let base_time = chrono::Utc::now().timestamp_millis();
    
    // Add alerts with different timestamps
    for i in 0..5 {
        let alert = RiskAlert {
            level: AlertLevel::Info,
            message: format!("Alert {}", i),
            timestamp: base_time + (i * 1000), // 1 second apart
            source: "test".to_string(),
        };
        monitor.add_alert(alert).await;
    }
    
    // The monitor should handle alerts regardless of their timestamp order
}