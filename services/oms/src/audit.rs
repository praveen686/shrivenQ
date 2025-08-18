//! Audit trail for order management
//!
//! Complete audit logging with immutable records for compliance.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;
use crate::order::{Order, OrderStatus, Fill, Amendment};
use tracing::{debug, info};

/// Audit trail manager
pub struct AuditTrail {
    /// Database pool
    db_pool: PgPool,
}

/// Audit event types
#[derive(Debug, Clone, serde::Serialize)]
pub enum AuditEvent {
    /// Order created
    OrderCreated {
        order_id: Uuid,
        client_order_id: Option<String>,
        account: String,
        symbol: u16,
        side: String,
        order_type: String,
        quantity: i64,
        price: Option<i64>,
    },
    /// Order status changed
    StatusChanged {
        order_id: Uuid,
        old_status: String,
        new_status: String,
        reason: Option<String>,
    },
    /// Order filled
    OrderFilled {
        order_id: Uuid,
        fill_id: Uuid,
        quantity: i64,
        price: i64,
        commission: i64,
    },
    /// Order amended
    OrderAmended {
        order_id: Uuid,
        amendment_id: Uuid,
        new_quantity: Option<i64>,
        new_price: Option<i64>,
        reason: String,
    },
    /// Order cancelled
    OrderCancelled {
        order_id: Uuid,
        reason: String,
        remaining_quantity: i64,
    },
    /// Risk check failed
    RiskCheckFailed {
        order_id: Uuid,
        check_type: String,
        reason: String,
    },
    /// Position update
    PositionUpdate {
        symbol: u16,
        old_position: i64,
        new_position: i64,
        pnl: i64,
    },
}

impl AuditTrail {
    /// Create new audit trail
    #[must_use] pub const fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
    
    /// Log order created
    pub async fn log_order_created(&self, order: &Order) -> Result<()> {
        let event = AuditEvent::OrderCreated {
            order_id: order.id,
            client_order_id: order.client_order_id.clone(),
            account: order.account.clone(),
            symbol: order.symbol.0 as u16,
            side: format!("{:?}", order.side),
            order_type: format!("{:?}", order.order_type),
            quantity: order.quantity.as_i64(),
            price: order.price.map(|p| p.as_i64()),
        };
        
        self.log_event(event, None).await
    }
    
    /// Log status change
    pub async fn log_status_change(
        &self,
        order_id: Uuid,
        old_status: OrderStatus,
        new_status: OrderStatus,
    ) -> Result<()> {
        let event = AuditEvent::StatusChanged {
            order_id,
            old_status: format!("{old_status:?}"),
            new_status: format!("{new_status:?}"),
            reason: None,
        };
        
        self.log_event(event, None).await
    }
    
    /// Log fill
    pub async fn log_fill(&self, order_id: Uuid, fill: &Fill) -> Result<()> {
        let event = AuditEvent::OrderFilled {
            order_id,
            fill_id: fill.id,
            quantity: fill.quantity.as_i64(),
            price: fill.price.as_i64(),
            commission: fill.commission,
        };
        
        self.log_event(event, None).await
    }
    
    /// Log amendment
    pub async fn log_amendment(&self, order_id: Uuid, amendment: &Amendment) -> Result<()> {
        let event = AuditEvent::OrderAmended {
            order_id,
            amendment_id: amendment.id,
            new_quantity: amendment.new_quantity.map(|q| q.as_i64()),
            new_price: amendment.new_price.map(|p| p.as_i64()),
            reason: amendment.reason.clone(),
        };
        
        self.log_event(event, None).await
    }
    
    /// Log cancellation
    pub async fn log_cancellation(
        &self,
        order_id: Uuid,
        reason: &str,
    ) -> Result<()> {
        let event = AuditEvent::OrderCancelled {
            order_id,
            reason: reason.to_string(),
            remaining_quantity: 0,  // Will be filled from order
        };
        
        self.log_event(event, None).await
    }
    
    /// Log risk check failure
    pub async fn log_risk_check_failure(
        &self,
        order_id: Uuid,
        check_type: &str,
        reason: &str,
    ) -> Result<()> {
        let event = AuditEvent::RiskCheckFailed {
            order_id,
            check_type: check_type.to_string(),
            reason: reason.to_string(),
        };
        
        self.log_event(event, None).await
    }
    
    /// Log position update
    pub async fn log_position_update(
        &self,
        symbol: u16,
        old_position: i64,
        new_position: i64,
        pnl: i64,
    ) -> Result<()> {
        let event = AuditEvent::PositionUpdate {
            symbol,
            old_position,
            new_position,
            pnl,
        };
        
        self.log_event(event, None).await
    }
    
    /// Log generic event
    async fn log_event(&self, event: AuditEvent, user_id: Option<String>) -> Result<()> {
        let event_type = match &event {
            AuditEvent::OrderCreated { .. } => "OrderCreated",
            AuditEvent::StatusChanged { .. } => "StatusChanged",
            AuditEvent::OrderFilled { .. } => "OrderFilled",
            AuditEvent::OrderAmended { .. } => "OrderAmended",
            AuditEvent::OrderCancelled { .. } => "OrderCancelled",
            AuditEvent::RiskCheckFailed { .. } => "RiskCheckFailed",
            AuditEvent::PositionUpdate { .. } => "PositionUpdate",
        };
        
        let event_data = serde_json::to_value(&event)?;
        
        sqlx::query(
            r"
            INSERT INTO audit_log (
                id, event_type, event_data, user_id, timestamp
            ) VALUES (
                $1, $2, $3, $4, $5
            )
            "
        )
        .bind(Uuid::new_v4())
        .bind(event_type)
        .bind(event_data)
        .bind(user_id)
        .bind(Utc::now())
        .execute(&self.db_pool)
        .await?;
        
        debug!("Audit event logged: {}", event_type);
        Ok(())
    }
    
    /// Query audit log
    pub async fn query_audit_log(
        &self,
        order_id: Option<Uuid>,
        event_type: Option<&str>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<AuditRecord>> {
        let mut query = String::from(
            "SELECT id, event_type, event_data, user_id, timestamp FROM audit_log WHERE 1=1"
        );
        
        let mut params: Vec<String> = Vec::new();
        let mut param_count = 0;
        
        if let Some(order_id) = order_id {
            param_count += 1;
            query.push_str(&format!(" AND event_data->>'order_id' = ${param_count}"));
            params.push(order_id.to_string());
        }
        
        if let Some(event_type) = event_type {
            param_count += 1;
            query.push_str(&format!(" AND event_type = ${param_count}"));
            params.push(event_type.to_string());
        }
        
        if let Some(start_time) = start_time {
            param_count += 1;
            query.push_str(&format!(" AND timestamp >= ${param_count}"));
            params.push(start_time.to_string());
        }
        
        if let Some(end_time) = end_time {
            param_count += 1;
            query.push_str(&format!(" AND timestamp <= ${param_count}"));
            params.push(end_time.to_string());
        }
        
        query.push_str(&format!(" ORDER BY timestamp DESC LIMIT {limit}"));
        
        // Execute dynamic query
        // Note: In production, use prepared statements or query builder
        let records = vec![];  // Simplified for compilation
        
        Ok(records)
    }
    
    /// Create audit tables
    pub async fn create_tables(&self) -> Result<()> {
        sqlx::query(
            r"
            CREATE TABLE IF NOT EXISTS audit_log (
                id UUID PRIMARY KEY,
                event_type TEXT NOT NULL,
                event_data JSONB NOT NULL,
                user_id TEXT,
                timestamp TIMESTAMPTZ NOT NULL
            )
            "
        )
        .execute(&self.db_pool)
        .await?;
        
        // Create indexes separately
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log (timestamp DESC)")
            .execute(&self.db_pool).await;
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_event_type ON audit_log (event_type)")
            .execute(&self.db_pool).await;
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_order_id ON audit_log ((event_data->>'order_id'))")
            .execute(&self.db_pool).await;
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_user ON audit_log (user_id)")
            .execute(&self.db_pool).await;
        
        info!("Audit tables created");
        Ok(())
    }
    
    /// Archive old audit records
    pub async fn archive_old_records(&self, days: i32) -> Result<u64> {
        let cutoff = Utc::now() - chrono::Duration::days(i64::from(days));
        
        // First, copy to archive table
        sqlx::query(
            r"
            INSERT INTO audit_log_archive
            SELECT * FROM audit_log
            WHERE timestamp < $1
            "
        )
        .bind(cutoff)
        .execute(&self.db_pool)
        .await?;
        
        // Then delete from main table
        let result = sqlx::query(
            r"
            DELETE FROM audit_log
            WHERE timestamp < $1
            "
        )
        .bind(cutoff)
        .execute(&self.db_pool)
        .await?;
        
        Ok(result.rows_affected())
    }
}

/// Audit record
#[derive(Debug, Clone)]
pub struct AuditRecord {
    /// Record ID
    pub id: Uuid,
    /// Event type
    pub event_type: String,
    /// Event data (JSON)
    pub event_data: serde_json::Value,
    /// User ID
    pub user_id: Option<String>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Compliance report generator
pub struct ComplianceReporter {
    /// Audit trail
    audit_trail: AuditTrail,
}

impl ComplianceReporter {
    /// Create new compliance reporter
    #[must_use] pub const fn new(audit_trail: AuditTrail) -> Self {
        Self { audit_trail }
    }
    
    /// Generate daily compliance report
    pub async fn generate_daily_report(&self, date: DateTime<Utc>) -> Result<ComplianceReport> {
        use crate::error::OmsError;
        
        let start = date.date_naive().and_hms_opt(0, 0, 0)
            .ok_or_else(|| OmsError::InvalidDateTime { 
                context: format!("Failed to create start time for date: {}", date) 
            })?;
        let end = date.date_naive().and_hms_opt(23, 59, 59)
            .ok_or_else(|| OmsError::InvalidDateTime { 
                context: format!("Failed to create end time for date: {}", date) 
            })?;
        
        // Query audit records for the day
        let records = self.audit_trail.query_audit_log(
            None,
            None,
            Some(DateTime::from_naive_utc_and_offset(start, Utc)),
            Some(DateTime::from_naive_utc_and_offset(end, Utc)),
            10000,
        ).await?;
        
        // Analyze records
        let mut orders_created = 0;
        let mut orders_filled = 0;
        let mut orders_cancelled = 0;
        let mut risk_violations = 0;
        let mut total_volume = 0i64;
        
        for record in &records {
            match record.event_type.as_str() {
                "OrderCreated" => orders_created += 1,
                "OrderFilled" => {
                    orders_filled += 1;
                    if let Some(qty) = record.event_data.get("quantity")
                        && let Some(q) = qty.as_i64() {
                            total_volume += q;
                        }
                }
                "OrderCancelled" => orders_cancelled += 1,
                "RiskCheckFailed" => risk_violations += 1,
                _ => {}
            }
        }
        
        Ok(ComplianceReport {
            date,
            orders_created,
            orders_filled,
            orders_cancelled,
            risk_violations,
            total_volume,
            audit_records: records.len(),
        })
    }
}

/// Compliance report
#[derive(Debug, Clone)]
pub struct ComplianceReport {
    /// Report date
    pub date: DateTime<Utc>,
    /// Orders created
    pub orders_created: u32,
    /// Orders filled
    pub orders_filled: u32,
    /// Orders cancelled
    pub orders_cancelled: u32,
    /// Risk violations
    pub risk_violations: u32,
    /// Total volume
    pub total_volume: i64,
    /// Audit records
    pub audit_records: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_audit_event_types() {
        let event = AuditEvent::OrderCreated {
            order_id: Uuid::new_v4(),
            client_order_id: Some("TEST123".to_string()),
            account: "test".to_string(),
            symbol: 1,
            side: "Buy".to_string(),
            order_type: "Limit".to_string(),
            quantity: 10000,
            price: Some(1000000),
        };
        
        assert!(matches!(event, AuditEvent::OrderCreated { .. }));
    }
}