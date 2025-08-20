//! Unit tests for audit trail functionality

use chrono::{DateTime, Duration, Utc};
use rstest::*;
use services_common::{Px, Qty, Symbol};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use uuid::Uuid;
use tempfile::TempDir;
use testcontainers::{clients::Cli, images::postgres::Postgres, Container};

use oms::audit::{AuditTrail, AuditEvent, AuditRecord, ComplianceReporter, ComplianceReport, AuditStatistics};
use oms::order::{Order, OrderSide, OrderStatus, OrderType, TimeInForce, Fill, Amendment, LiquidityIndicator};

/// Test fixture for creating a test database
#[fixture]
async fn test_db() -> PgPool {
    // In real tests, you'd use testcontainers or similar
    // For now, we'll use an in-memory SQLite database for testing
    let pool = sqlx::PgPool::connect("postgresql://test:test@localhost/test_audit")
        .await
        .expect("Failed to connect to test database");
    
    // Create audit tables
    let audit_trail = AuditTrail::new(pool.clone());
    audit_trail.create_tables().await.expect("Failed to create tables");
    
    pool
}

/// Test fixture for audit trail
#[fixture]
fn audit_trail(#[future] test_db: PgPool) -> AuditTrail {
    AuditTrail::new(test_db)
}

/// Test fixture for test order
#[fixture]
fn test_order() -> Order {
    Order {
        id: Uuid::new_v4(),
        client_order_id: Some("AUDIT-TEST-001".to_string()),
        parent_order_id: None,
        symbol: Symbol(1),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Day,
        quantity: Qty::from_i64(10_000),
        executed_quantity: Qty::ZERO,
        remaining_quantity: Qty::from_i64(10_000),
        price: Some(Px::from_i64(1_000_000)),
        stop_price: None,
        status: OrderStatus::New,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        account: "test_audit_account".to_string(),
        exchange: "test_exchange".to_string(),
        strategy_id: Some("audit_test_strategy".to_string()),
        tags: vec!["audit".to_string(), "test".to_string()],
        fills: vec![],
        amendments: vec![],
        version: 1,
        sequence_number: 1,
    }
}

/// Test fixture for test fill
#[fixture]
fn test_fill(test_order: Order) -> Fill {
    Fill {
        id: Uuid::new_v4(),
        order_id: test_order.id,
        execution_id: "EXEC-12345".to_string(),
        quantity: Qty::from_i64(5_000),
        price: Px::from_i64(1_000_000),
        commission: 10,
        commission_currency: "USDT".to_string(),
        timestamp: Utc::now(),
        liquidity: LiquidityIndicator::Taker,
    }
}

/// Test fixture for test amendment
#[fixture]
fn test_amendment(test_order: Order) -> Amendment {
    Amendment {
        id: Uuid::new_v4(),
        order_id: test_order.id,
        new_quantity: Some(Qty::from_i64(15_000)),
        new_price: Some(Px::from_i64(1_010_000)),
        reason: "Strategy update".to_string(),
        timestamp: Utc::now(),
    }
}

#[rstest]
#[tokio::test]
async fn test_log_order_created(
    #[future] audit_trail: AuditTrail,
    test_order: Order,
) {
    let audit_trail = audit_trail.await;
    
    let result = audit_trail.log_order_created(&test_order).await;
    assert!(result.is_ok(), "Should successfully log order creation");
    
    // Verify the audit record was created
    let stats = audit_trail.get_audit_statistics(
        Utc::now() - Duration::minutes(1),
        Utc::now() + Duration::minutes(1)
    ).await.expect("Should get audit statistics");
    
    assert_eq!(stats.orders_created, 1, "Should have logged 1 order creation");
    assert_eq!(stats.total_events, 1, "Should have 1 total event");
}

#[rstest]
#[tokio::test]
async fn test_log_status_change(
    #[future] audit_trail: AuditTrail,
    test_order: Order,
) {
    let audit_trail = audit_trail.await;
    
    let result = audit_trail.log_status_change(
        test_order.id,
        OrderStatus::New,
        OrderStatus::Pending,
    ).await;
    
    assert!(result.is_ok(), "Should successfully log status change");
    
    let stats = audit_trail.get_audit_statistics(
        Utc::now() - Duration::minutes(1),
        Utc::now() + Duration::minutes(1)
    ).await.expect("Should get audit statistics");
    
    assert_eq!(stats.status_changes, 1, "Should have logged 1 status change");
}

#[rstest]
#[tokio::test]
async fn test_log_fill(
    #[future] audit_trail: AuditTrail,
    test_order: Order,
    test_fill: Fill,
) {
    let audit_trail = audit_trail.await;
    
    let result = audit_trail.log_fill(test_order.id, &test_fill).await;
    assert!(result.is_ok(), "Should successfully log fill");
    
    let stats = audit_trail.get_audit_statistics(
        Utc::now() - Duration::minutes(1),
        Utc::now() + Duration::minutes(1)
    ).await.expect("Should get audit statistics");
    
    assert_eq!(stats.order_fills, 1, "Should have logged 1 fill");
}

#[rstest]
#[tokio::test]
async fn test_log_amendment(
    #[future] audit_trail: AuditTrail,
    test_order: Order,
    test_amendment: Amendment,
) {
    let audit_trail = audit_trail.await;
    
    let result = audit_trail.log_amendment(test_order.id, &test_amendment).await;
    assert!(result.is_ok(), "Should successfully log amendment");
}

#[rstest]
#[tokio::test]
async fn test_log_cancellation(
    #[future] audit_trail: AuditTrail,
    test_order: Order,
) {
    let audit_trail = audit_trail.await;
    
    let result = audit_trail.log_cancellation(
        test_order.id,
        "User requested cancellation",
    ).await;
    
    assert!(result.is_ok(), "Should successfully log cancellation");
    
    let stats = audit_trail.get_audit_statistics(
        Utc::now() - Duration::minutes(1),
        Utc::now() + Duration::minutes(1)
    ).await.expect("Should get audit statistics");
    
    assert_eq!(stats.cancellations, 1, "Should have logged 1 cancellation");
}

#[rstest]
#[tokio::test]
async fn test_log_risk_check_failure(
    #[future] audit_trail: AuditTrail,
    test_order: Order,
) {
    let audit_trail = audit_trail.await;
    
    let result = audit_trail.log_risk_check_failure(
        test_order.id,
        "position_limit",
        "Position would exceed maximum allowed",
    ).await;
    
    assert!(result.is_ok(), "Should successfully log risk check failure");
}

#[rstest]
#[tokio::test]
async fn test_log_position_update(
    #[future] audit_trail: AuditTrail,
) {
    let audit_trail = audit_trail.await;
    
    let result = audit_trail.log_position_update(
        1, // symbol
        100_000, // old position
        150_000, // new position  
        5_000, // pnl
    ).await;
    
    assert!(result.is_ok(), "Should successfully log position update");
}

#[rstest]
#[tokio::test]
async fn test_comprehensive_order_lifecycle_audit(
    #[future] audit_trail: AuditTrail,
    test_order: Order,
    test_fill: Fill,
    test_amendment: Amendment,
) {
    let audit_trail = audit_trail.await;
    
    // Log complete order lifecycle
    audit_trail.log_order_created(&test_order).await.expect("Should log creation");
    audit_trail.log_status_change(test_order.id, OrderStatus::New, OrderStatus::Pending).await.expect("Should log status change");
    audit_trail.log_amendment(test_order.id, &test_amendment).await.expect("Should log amendment");
    audit_trail.log_fill(test_order.id, &test_fill).await.expect("Should log fill");
    audit_trail.log_cancellation(test_order.id, "Strategy closed").await.expect("Should log cancellation");
    
    // Verify all events were logged
    let stats = audit_trail.get_audit_statistics(
        Utc::now() - Duration::minutes(1),
        Utc::now() + Duration::minutes(1)
    ).await.expect("Should get audit statistics");
    
    assert_eq!(stats.total_events, 5, "Should have logged 5 events");
    assert_eq!(stats.orders_created, 1, "Should have 1 order creation");
    assert_eq!(stats.status_changes, 1, "Should have 1 status change");
    assert_eq!(stats.order_fills, 1, "Should have 1 fill");
    assert_eq!(stats.cancellations, 1, "Should have 1 cancellation");
}

#[rstest]
#[tokio::test]
async fn test_audit_event_serialization() {
    let order_id = Uuid::new_v4();
    
    let event = AuditEvent::OrderCreated {
        order_id,
        client_order_id: Some("TEST-123".to_string()),
        account: "test_account".to_string(),
        symbol: 1,
        side: "Buy".to_string(),
        order_type: "Limit".to_string(),
        quantity: 10_000,
        price: Some(1_000_000),
    };
    
    let serialized = serde_json::to_value(&event).expect("Should serialize");
    assert!(serialized.is_object(), "Should be JSON object");
    assert_eq!(serialized["order_id"], order_id.to_string());
    assert_eq!(serialized["quantity"], 10_000);
}

#[rstest]
#[tokio::test]
async fn test_query_audit_log_by_order_id(
    #[future] audit_trail: AuditTrail,
    test_order: Order,
) {
    let audit_trail = audit_trail.await;
    
    // Log several events for the order
    audit_trail.log_order_created(&test_order).await.expect("Should log creation");
    audit_trail.log_status_change(test_order.id, OrderStatus::New, OrderStatus::Pending).await.expect("Should log status change");
    
    // Log event for different order
    let other_order_id = Uuid::new_v4();
    audit_trail.log_cancellation(other_order_id, "Different order").await.expect("Should log other cancellation");
    
    let records = audit_trail.query_audit_log(
        Some(test_order.id),
        None,
        None,
        None,
        100,
    ).await.expect("Should query audit log");
    
    // Should only return events for the specific order
    assert_eq!(records.len(), 2, "Should return 2 events for the order");
}

#[rstest]
#[tokio::test]
async fn test_query_audit_log_by_event_type(
    #[future] audit_trail: AuditTrail,
    test_order: Order,
) {
    let audit_trail = audit_trail.await;
    
    // Log different types of events
    audit_trail.log_order_created(&test_order).await.expect("Should log creation");
    audit_trail.log_status_change(test_order.id, OrderStatus::New, OrderStatus::Pending).await.expect("Should log status change");
    audit_trail.log_cancellation(test_order.id, "Test cancellation").await.expect("Should log cancellation");
    
    let records = audit_trail.query_audit_log(
        None,
        Some("StatusChanged"),
        None,
        None,
        100,
    ).await.expect("Should query audit log");
    
    // Should only return status change events
    assert_eq!(records.len(), 1, "Should return 1 status change event");
}

#[rstest]
#[tokio::test]
async fn test_query_audit_log_by_time_range(
    #[future] audit_trail: AuditTrail,
    test_order: Order,
) {
    let audit_trail = audit_trail.await;
    
    let start_time = Utc::now();
    
    // Log an event
    audit_trail.log_order_created(&test_order).await.expect("Should log creation");
    
    // Wait a bit and log another event
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    let mid_time = Utc::now();
    audit_trail.log_cancellation(test_order.id, "Later event").await.expect("Should log cancellation");
    
    // Query for events only in first half
    let records = audit_trail.query_audit_log(
        None,
        None,
        Some(start_time),
        Some(mid_time),
        100,
    ).await.expect("Should query audit log");
    
    // Should only return the first event
    assert_eq!(records.len(), 1, "Should return 1 event in time range");
}

#[rstest]
#[tokio::test]
async fn test_archive_old_records(
    #[future] audit_trail: AuditTrail,
    test_order: Order,
) {
    let audit_trail = audit_trail.await;
    
    // Log some events
    audit_trail.log_order_created(&test_order).await.expect("Should log creation");
    audit_trail.log_cancellation(test_order.id, "Test").await.expect("Should log cancellation");
    
    // Archive records older than 0 days (all records)
    let archived_count = audit_trail.archive_old_records(0).await.expect("Should archive records");
    
    assert_eq!(archived_count, 2, "Should archive 2 records");
    
    // Main table should be empty
    let stats = audit_trail.get_audit_statistics(
        Utc::now() - Duration::days(1),
        Utc::now() + Duration::days(1)
    ).await.expect("Should get stats");
    
    assert_eq!(stats.total_events, 0, "Main table should be empty after archiving");
}

// Compliance reporter tests

#[rstest]
#[tokio::test]
async fn test_compliance_reporter_creation(
    #[future] audit_trail: AuditTrail,
) {
    let audit_trail = audit_trail.await;
    let reporter = ComplianceReporter::new(audit_trail);
    
    // Just verify it can be created
    assert!(!std::mem::size_of_val(&reporter) == 0, "Reporter should be created");
}

#[rstest]
#[tokio::test]
async fn test_generate_daily_compliance_report(
    #[future] audit_trail: AuditTrail,
    test_order: Order,
    test_fill: Fill,
) {
    let audit_trail = audit_trail.await;
    let reporter = ComplianceReporter::new(audit_trail);
    
    let report_date = Utc::now();
    
    // Generate some audit events for today
    let audit_trail = &reporter.audit_trail;
    audit_trail.log_order_created(&test_order).await.expect("Should log creation");
    audit_trail.log_fill(test_order.id, &test_fill).await.expect("Should log fill");
    audit_trail.log_cancellation(test_order.id, "End of day").await.expect("Should log cancellation");
    audit_trail.log_risk_check_failure(test_order.id, "test_check", "Test failure").await.expect("Should log risk failure");
    
    let report = reporter.generate_daily_report(report_date).await.expect("Should generate report");
    
    assert_eq!(report.orders_created, 1, "Should report 1 order created");
    assert_eq!(report.orders_filled, 1, "Should report 1 order filled");
    assert_eq!(report.orders_cancelled, 1, "Should report 1 order cancelled");
    assert_eq!(report.risk_violations, 1, "Should report 1 risk violation");
    assert_eq!(report.total_volume, test_fill.quantity.as_i64(), "Should report correct volume");
    assert!(report.audit_records > 0, "Should have audit records");
}

#[rstest]
#[tokio::test]
async fn test_compliance_report_empty_day(
    #[future] audit_trail: AuditTrail,
) {
    let audit_trail = audit_trail.await;
    let reporter = ComplianceReporter::new(audit_trail);
    
    let report_date = Utc::now() - Duration::days(30); // Date with no activity
    let report = reporter.generate_daily_report(report_date).await.expect("Should generate empty report");
    
    assert_eq!(report.orders_created, 0, "Should report 0 orders created");
    assert_eq!(report.orders_filled, 0, "Should report 0 orders filled");
    assert_eq!(report.orders_cancelled, 0, "Should report 0 orders cancelled");
    assert_eq!(report.risk_violations, 0, "Should report 0 risk violations");
    assert_eq!(report.total_volume, 0, "Should report 0 volume");
    assert_eq!(report.audit_records, 0, "Should have 0 audit records");
}

// Performance tests

#[rstest]
#[tokio::test]
async fn test_audit_logging_performance(
    #[future] audit_trail: AuditTrail,
    test_order: Order,
) {
    let audit_trail = audit_trail.await;
    
    let start = std::time::Instant::now();
    
    // Log 1000 events
    for i in 0..1000 {
        let mut order = test_order.clone();
        order.id = Uuid::new_v4();
        order.sequence_number = i;
        
        audit_trail.log_order_created(&order).await.expect("Should log creation");
    }
    
    let duration = start.elapsed();
    println!("Logged 1000 audit events in {}ms", duration.as_millis());
    
    // Should log 1000 events in reasonable time
    assert!(duration.as_millis() < 2000, "Audit logging should be fast");
    
    // Verify all events were logged
    let stats = audit_trail.get_audit_statistics(
        Utc::now() - Duration::minutes(1),
        Utc::now() + Duration::minutes(1)
    ).await.expect("Should get stats");
    
    assert_eq!(stats.orders_created, 1000, "Should have logged 1000 order creations");
}

#[rstest]
#[tokio::test]
async fn test_audit_query_performance(
    #[future] audit_trail: AuditTrail,
    test_order: Order,
) {
    let audit_trail = audit_trail.await;
    
    // Create many audit records
    for i in 0..100 {
        let mut order = test_order.clone();
        order.id = Uuid::new_v4();
        order.sequence_number = i;
        
        audit_trail.log_order_created(&order).await.expect("Should log creation");
        audit_trail.log_status_change(order.id, OrderStatus::New, OrderStatus::Pending).await.expect("Should log status change");
    }
    
    let start = std::time::Instant::now();
    
    // Perform 100 queries
    for _ in 0..100 {
        let _stats = audit_trail.get_audit_statistics(
            Utc::now() - Duration::hours(1),
            Utc::now() + Duration::hours(1)
        ).await.expect("Should get stats");
    }
    
    let duration = start.elapsed();
    println!("Performed 100 audit queries in {}ms", duration.as_millis());
    
    // Queries should be fast
    assert!(duration.as_millis() < 1000, "Audit queries should be fast");
}

// Error handling tests

#[rstest]
#[tokio::test]
async fn test_audit_with_invalid_data() {
    // Test with invalid database connection
    let invalid_pool = sqlx::PgPool::connect("postgresql://invalid:invalid@localhost/nonexistent")
        .await;
    
    assert!(invalid_pool.is_err(), "Should fail to connect to invalid database");
}

#[rstest]
fn test_audit_event_variants() {
    // Test all variants of AuditEvent can be created and serialized
    let order_id = Uuid::new_v4();
    let fill_id = Uuid::new_v4();
    let amendment_id = Uuid::new_v4();
    
    let events = vec![
        AuditEvent::OrderCreated {
            order_id,
            client_order_id: Some("TEST".to_string()),
            account: "test".to_string(),
            symbol: 1,
            side: "Buy".to_string(),
            order_type: "Limit".to_string(),
            quantity: 1000,
            price: Some(1000000),
        },
        AuditEvent::StatusChanged {
            order_id,
            old_status: "New".to_string(),
            new_status: "Pending".to_string(),
            reason: Some("Submitted".to_string()),
        },
        AuditEvent::OrderFilled {
            order_id,
            fill_id,
            quantity: 500,
            price: 1000000,
            commission: 10,
        },
        AuditEvent::OrderAmended {
            order_id,
            amendment_id,
            new_quantity: Some(2000),
            new_price: Some(1010000),
            reason: "Strategy update".to_string(),
        },
        AuditEvent::OrderCancelled {
            order_id,
            reason: "User request".to_string(),
            remaining_quantity: 500,
        },
        AuditEvent::RiskCheckFailed {
            order_id,
            check_type: "position_limit".to_string(),
            reason: "Exceeds limit".to_string(),
        },
        AuditEvent::PositionUpdate {
            symbol: 1,
            old_position: 1000,
            new_position: 1500,
            pnl: 100,
        },
    ];
    
    for event in events {
        let serialized = serde_json::to_value(&event).expect("Should serialize event");
        assert!(serialized.is_object(), "Event should serialize to JSON object");
    }
}

// Integration-style tests

#[rstest]
#[tokio::test]
async fn test_audit_trail_table_creation(
    #[future] test_db: PgPool,
) {
    let db = test_db.await;
    let audit_trail = AuditTrail::new(db.clone());
    
    let result = audit_trail.create_tables().await;
    assert!(result.is_ok(), "Should create audit tables successfully");
    
    // Verify table exists by querying it
    let count_result = sqlx::query("SELECT COUNT(*) as count FROM audit_log")
        .fetch_one(&db)
        .await;
        
    assert!(count_result.is_ok(), "Should be able to query audit_log table");
}

#[rstest]
#[tokio::test]
async fn test_audit_statistics_calculation(
    #[future] audit_trail: AuditTrail,
    test_order: Order,
    test_fill: Fill,
) {
    let audit_trail = audit_trail.await;
    
    let start_time = Utc::now() - Duration::minutes(5);
    let end_time = Utc::now() + Duration::minutes(5);
    
    // Log various events
    audit_trail.log_order_created(&test_order).await.expect("Should log creation");
    audit_trail.log_status_change(test_order.id, OrderStatus::New, OrderStatus::Pending).await.expect("Should log status change");
    audit_trail.log_fill(test_order.id, &test_fill).await.expect("Should log fill");
    audit_trail.log_cancellation(test_order.id, "Test").await.expect("Should log cancellation");
    
    let stats = audit_trail.get_audit_statistics(start_time, end_time).await.expect("Should get statistics");
    
    assert_eq!(stats.total_events, 4, "Should count all events");
    assert_eq!(stats.orders_created, 1, "Should count order creations");
    assert_eq!(stats.status_changes, 1, "Should count status changes");
    assert_eq!(stats.order_fills, 1, "Should count fills");
    assert_eq!(stats.cancellations, 1, "Should count cancellations");
    assert_eq!(stats.period_start, start_time, "Should have correct start time");
    assert_eq!(stats.period_end, end_time, "Should have correct end time");
}

// Concurrent access tests

#[rstest]
#[tokio::test]
async fn test_concurrent_audit_logging(
    #[future] audit_trail: AuditTrail,
) {
    let audit_trail = std::sync::Arc::new(audit_trail.await);
    let mut handles = vec![];
    
    // Spawn multiple tasks logging concurrently
    for i in 0..10 {
        let audit_clone = std::sync::Arc::clone(&audit_trail);
        let handle = tokio::spawn(async move {
            for j in 0..10 {
                let order_id = Uuid::new_v4();
                let result = audit_clone.log_order_created(&Order {
                    id: order_id,
                    client_order_id: Some(format!("CONCURRENT-{}-{}", i, j)),
                    parent_order_id: None,
                    symbol: Symbol(1),
                    side: OrderSide::Buy,
                    order_type: OrderType::Limit,
                    time_in_force: TimeInForce::Day,
                    quantity: Qty::from_i64(1000),
                    executed_quantity: Qty::ZERO,
                    remaining_quantity: Qty::from_i64(1000),
                    price: Some(Px::from_i64(1000000)),
                    stop_price: None,
                    status: OrderStatus::New,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    account: "concurrent_test".to_string(),
                    exchange: "test".to_string(),
                    strategy_id: None,
                    tags: vec![],
                    fills: vec![],
                    amendments: vec![],
                    version: 1,
                    sequence_number: (i * 10 + j) as u64,
                }).await;
                
                assert!(result.is_ok(), "Should log successfully");
            }
        });
        handles.push(handle);
    }
    
    // Wait for all tasks to complete
    for handle in handles {
        handle.await.expect("Task should complete successfully");
    }
    
    // Verify all events were logged
    let stats = audit_trail.get_audit_statistics(
        Utc::now() - Duration::minutes(1),
        Utc::now() + Duration::minutes(1)
    ).await.expect("Should get statistics");
    
    assert_eq!(stats.orders_created, 100, "Should have logged 100 order creations");
}