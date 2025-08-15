//! Order recovery and reconciliation
//!
//! Handles crash recovery, state reconciliation, and order replay.

use anyhow::Result;
use chrono::{DateTime, Utc};
use common::{Px, Qty, Symbol};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::order::{Order, OrderStatus, Fill};
use crate::persistence::PersistenceManager;

/// Recovery manager for handling system restarts
pub struct RecoveryManager {
    /// Database pool
    db_pool: PgPool,
    /// Persistence manager
    persistence: PersistenceManager,
}

/// Recovery statistics
#[derive(Debug, Clone, Default)]
pub struct RecoveryStats {
    /// Orders recovered
    pub orders_recovered: u32,
    /// Fills recovered
    pub fills_recovered: u32,
    /// Orders reconciled
    pub orders_reconciled: u32,
    /// Discrepancies found
    pub discrepancies_found: u32,
    /// Recovery time (ms)
    pub recovery_time_ms: u64,
}

/// Order discrepancy
#[derive(Debug, Clone)]
pub struct OrderDiscrepancy {
    /// Order ID
    pub order_id: Uuid,
    /// Discrepancy type
    pub discrepancy_type: DiscrepancyType,
    /// Description
    pub description: String,
    /// Suggested action
    pub suggested_action: RecoveryAction,
}

/// Discrepancy types
#[derive(Debug, Clone)]
pub enum DiscrepancyType {
    /// Missing fills
    MissingFills,
    /// Quantity mismatch
    QuantityMismatch,
    /// Status inconsistency
    StatusInconsistency,
    /// Missing order
    MissingOrder,
    /// Orphaned fill
    OrphanedFill,
}

/// Recovery actions
#[derive(Debug, Clone)]
pub enum RecoveryAction {
    /// Recalculate quantities
    RecalculateQuantities,
    /// Request fill replay
    RequestFillReplay,
    /// Update status
    UpdateStatus(OrderStatus),
    /// Cancel order
    CancelOrder,
    /// Manual intervention required
    ManualIntervention,
}

impl RecoveryManager {
    /// Create new recovery manager
    pub fn new(db_pool: PgPool) -> Self {
        let persistence = PersistenceManager::new(db_pool.clone());
        Self {
            db_pool,
            persistence,
        }
    }
    
    /// Perform full recovery
    pub async fn recover(&self) -> Result<RecoveryStats> {
        let start = std::time::Instant::now();
        let mut stats = RecoveryStats::default();
        
        info!("Starting order recovery process");
        
        // Load all active orders
        let orders = self.load_orders_for_recovery().await?;
        stats.orders_recovered = orders.len() as u32;
        
        // Load all fills
        let fills = self.load_fills_for_recovery().await?;
        stats.fills_recovered = fills.len() as u32;
        
        // Reconcile orders with fills
        let discrepancies = self.reconcile_orders_and_fills(&orders, &fills).await?;
        stats.discrepancies_found = discrepancies.len() as u32;
        
        // Process discrepancies
        for discrepancy in discrepancies {
            self.handle_discrepancy(&discrepancy).await?;
            stats.orders_reconciled += 1;
        }
        
        // Verify integrity
        self.verify_integrity(&orders).await?;
        
        stats.recovery_time_ms = start.elapsed().as_millis() as u64;
        
        info!(
            "Recovery completed: {} orders, {} fills, {} discrepancies in {}ms",
            stats.orders_recovered,
            stats.fills_recovered,
            stats.discrepancies_found,
            stats.recovery_time_ms
        );
        
        Ok(stats)
    }
    
    /// Load orders for recovery
    async fn load_orders_for_recovery(&self) -> Result<Vec<Order>> {
        let rows = sqlx::query(
            r#"
            SELECT 
                id, client_order_id, parent_order_id, symbol, side,
                order_type, time_in_force, quantity, executed_quantity,
                remaining_quantity, price, stop_price, status,
                created_at, updated_at, account, exchange,
                strategy_id, tags, version, sequence_number
            FROM orders
            WHERE status NOT IN ('Filled', 'Cancelled', 'Rejected', 'Expired')
            ORDER BY created_at DESC
            "#
        )
        .fetch_all(&self.db_pool)
        .await?;
        
        let mut orders = Vec::with_capacity(rows.len());
        
        for row in rows {
            let order = Order {
                id: row.get("id"),
                client_order_id: row.get("client_order_id"),
                parent_order_id: row.get("parent_order_id"),
                symbol: Symbol(row.get::<i32, _>("symbol") as u32),
                side: crate::persistence::parse_order_side(&row.get::<String, _>("side"))?,
                order_type: crate::persistence::parse_order_type(&row.get::<String, _>("order_type"))?,
                time_in_force: crate::persistence::parse_time_in_force(&row.get::<String, _>("time_in_force"))?,
                quantity: Qty::from_i64(row.get("quantity")),
                executed_quantity: Qty::from_i64(row.get("executed_quantity")),
                remaining_quantity: Qty::from_i64(row.get("remaining_quantity")),
                price: row.get::<Option<i64>, _>("price").map(Px::from_i64),
                stop_price: row.get::<Option<i64>, _>("stop_price").map(Px::from_i64),
                status: crate::persistence::parse_order_status(&row.get::<String, _>("status"))?,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                account: row.get("account"),
                exchange: row.get("exchange"),
                strategy_id: row.get("strategy_id"),
                tags: row.get("tags"),
                fills: vec![],
                amendments: vec![],
                version: row.get::<i32, _>("version") as u32,
                sequence_number: row.get::<i64, _>("sequence_number") as u64,
            };
            
            orders.push(order);
        }
        
        debug!("Loaded {} orders for recovery", orders.len());
        Ok(orders)
    }
    
    /// Load fills for recovery
    async fn load_fills_for_recovery(&self) -> Result<HashMap<Uuid, Vec<Fill>>> {
        let rows = sqlx::query(
            r#"
            SELECT 
                id, order_id, execution_id, quantity, price,
                commission, commission_currency, timestamp, liquidity
            FROM fills
            ORDER BY timestamp DESC
            "#
        )
        .fetch_all(&self.db_pool)
        .await?;
        
        let mut fills_by_order: HashMap<Uuid, Vec<Fill>> = HashMap::new();
        
        for row in rows {
            let fill = Fill {
                id: row.get("id"),
                order_id: row.get("order_id"),
                execution_id: row.get("execution_id"),
                quantity: Qty::from_i64(row.get("quantity")),
                price: Px::from_i64(row.get("price")),
                commission: row.get("commission"),
                commission_currency: row.get("commission_currency"),
                timestamp: row.get("timestamp"),
                liquidity: crate::persistence::parse_liquidity(&row.get::<String, _>("liquidity"))?,
            };
            
            let order_id: Uuid = row.get("order_id");
            fills_by_order.entry(order_id)
                .or_insert_with(Vec::new)
                .push(fill);
        }
        
        debug!("Loaded fills for {} orders", fills_by_order.len());
        Ok(fills_by_order)
    }
    
    /// Reconcile orders and fills
    async fn reconcile_orders_and_fills(
        &self,
        orders: &[Order],
        fills: &HashMap<Uuid, Vec<Fill>>,
    ) -> Result<Vec<OrderDiscrepancy>> {
        let mut discrepancies = Vec::new();
        
        for order in orders {
            // Check if order has fills
            if let Some(order_fills) = fills.get(&order.id) {
                // Calculate executed quantity from fills
                let total_filled: i64 = order_fills
                    .iter()
                    .map(|f| f.quantity.as_i64())
                    .sum();
                
                // Check quantity consistency
                if total_filled != order.executed_quantity.as_i64() {
                    discrepancies.push(OrderDiscrepancy {
                        order_id: order.id,
                        discrepancy_type: DiscrepancyType::QuantityMismatch,
                        description: format!(
                            "Executed quantity mismatch: order={}, fills={}",
                            order.executed_quantity.as_i64(),
                            total_filled
                        ),
                        suggested_action: RecoveryAction::RecalculateQuantities,
                    });
                }
                
                // Check status consistency
                if total_filled > 0 && order.status == OrderStatus::New {
                    discrepancies.push(OrderDiscrepancy {
                        order_id: order.id,
                        discrepancy_type: DiscrepancyType::StatusInconsistency,
                        description: "Order has fills but status is New".to_string(),
                        suggested_action: if total_filled == order.quantity.as_i64() {
                            RecoveryAction::UpdateStatus(OrderStatus::Filled)
                        } else {
                            RecoveryAction::UpdateStatus(OrderStatus::PartiallyFilled)
                        },
                    });
                }
            } else if order.executed_quantity.as_i64() > 0 {
                // Order claims to be executed but has no fills
                discrepancies.push(OrderDiscrepancy {
                    order_id: order.id,
                    discrepancy_type: DiscrepancyType::MissingFills,
                    description: format!(
                        "Order shows {} executed but no fills found",
                        order.executed_quantity.as_i64()
                    ),
                    suggested_action: RecoveryAction::RequestFillReplay,
                });
            }
            
            // Check remaining quantity consistency
            let expected_remaining = order.quantity.as_i64() - order.executed_quantity.as_i64();
            if expected_remaining != order.remaining_quantity.as_i64() {
                discrepancies.push(OrderDiscrepancy {
                    order_id: order.id,
                    discrepancy_type: DiscrepancyType::QuantityMismatch,
                    description: format!(
                        "Remaining quantity mismatch: expected={}, actual={}",
                        expected_remaining,
                        order.remaining_quantity.as_i64()
                    ),
                    suggested_action: RecoveryAction::RecalculateQuantities,
                });
            }
        }
        
        // Check for orphaned fills
        for (order_id, _) in fills {
            if !orders.iter().any(|o| o.id == *order_id) {
                discrepancies.push(OrderDiscrepancy {
                    order_id: *order_id,
                    discrepancy_type: DiscrepancyType::OrphanedFill,
                    description: "Fills found for non-existent or terminated order".to_string(),
                    suggested_action: RecoveryAction::ManualIntervention,
                });
            }
        }
        
        if !discrepancies.is_empty() {
            warn!("Found {} discrepancies during reconciliation", discrepancies.len());
        }
        
        Ok(discrepancies)
    }
    
    /// Handle discrepancy
    async fn handle_discrepancy(&self, discrepancy: &OrderDiscrepancy) -> Result<()> {
        info!(
            "Handling discrepancy for order {}: {:?}",
            discrepancy.order_id, discrepancy.discrepancy_type
        );
        
        match &discrepancy.suggested_action {
            RecoveryAction::RecalculateQuantities => {
                self.recalculate_order_quantities(discrepancy.order_id).await?;
            }
            RecoveryAction::UpdateStatus(new_status) => {
                self.update_order_status(discrepancy.order_id, *new_status).await?;
            }
            RecoveryAction::RequestFillReplay => {
                warn!(
                    "Fill replay required for order {} - manual intervention needed",
                    discrepancy.order_id
                );
            }
            RecoveryAction::CancelOrder => {
                self.cancel_order_for_recovery(discrepancy.order_id).await?;
            }
            RecoveryAction::ManualIntervention => {
                error!(
                    "Manual intervention required for order {}: {}",
                    discrepancy.order_id, discrepancy.description
                );
            }
        }
        
        Ok(())
    }
    
    /// Recalculate order quantities
    async fn recalculate_order_quantities(&self, order_id: Uuid) -> Result<()> {
        let fills_row = sqlx::query(
            r#"
            SELECT SUM(quantity) as total_filled
            FROM fills
            WHERE order_id = $1
            "#
        )
        .bind(order_id)
        .fetch_one(&self.db_pool)
        .await?;
        
        let total_filled: i64 = fills_row.get::<Option<i64>, _>("total_filled").unwrap_or(0);
        
        let order_row = sqlx::query(
            r#"
            SELECT quantity FROM orders WHERE id = $1
            "#
        )
        .bind(order_id)
        .fetch_one(&self.db_pool)
        .await?;
        
        let quantity: i64 = order_row.get("quantity");
        let remaining = quantity - total_filled;
        
        sqlx::query(
            r#"
            UPDATE orders SET
                executed_quantity = $2,
                remaining_quantity = $3,
                updated_at = $4
            WHERE id = $1
            "#
        )
        .bind(order_id)
        .bind(total_filled)
        .bind(remaining)
        .bind(Utc::now())
        .execute(&self.db_pool)
        .await?;
        
        debug!("Recalculated quantities for order {}", order_id);
        Ok(())
    }
    
    /// Update order status
    async fn update_order_status(&self, order_id: Uuid, status: OrderStatus) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE orders SET
                status = $2,
                updated_at = $3
            WHERE id = $1
            "#
        )
        .bind(order_id)
        .bind(format!("{:?}", status))
        .bind(Utc::now())
        .execute(&self.db_pool)
        .await?;
        
        debug!("Updated order {} status to {:?}", order_id, status);
        Ok(())
    }
    
    /// Cancel order for recovery
    async fn cancel_order_for_recovery(&self, order_id: Uuid) -> Result<()> {
        self.update_order_status(order_id, OrderStatus::Cancelled).await?;
        
        // Log cancellation in audit trail
        sqlx::query(
            r#"
            INSERT INTO audit_log (id, event_type, event_data, timestamp)
            VALUES ($1, 'OrderCancelled', $2, $3)
            "#
        )
        .bind(Uuid::new_v4())
        .bind(serde_json::json!({
            "order_id": order_id,
            "reason": "Cancelled during recovery due to discrepancy"
        }))
        .bind(Utc::now())
        .execute(&self.db_pool)
        .await?;
        
        info!("Cancelled order {} during recovery", order_id);
        Ok(())
    }
    
    /// Verify integrity after recovery
    async fn verify_integrity(&self, orders: &[Order]) -> Result<()> {
        let mut integrity_issues = 0;
        
        for order in orders {
            // Verify parent-child relationships
            if let Some(parent_id) = order.parent_order_id {
                let parent_exists_row = sqlx::query(
                    r#"
                    SELECT COUNT(*) as count FROM orders WHERE id = $1
                    "#
                )
                .bind(parent_id)
                .fetch_one(&self.db_pool)
                .await?;
                
                let parent_count: i64 = parent_exists_row.get::<Option<i64>, _>("count").unwrap_or(0);
                if parent_count == 0 {
                    warn!("Order {} references non-existent parent {}", order.id, parent_id);
                    integrity_issues += 1;
                }
            }
            
            // Verify account exists
            // Verify exchange connectivity
            // Additional integrity checks as needed
        }
        
        if integrity_issues > 0 {
            warn!("Found {} integrity issues during verification", integrity_issues);
        } else {
            info!("Integrity verification passed");
        }
        
        Ok(())
    }
    
    /// Create recovery checkpoint
    pub async fn create_checkpoint(&self) -> Result<String> {
        let checkpoint_id = Uuid::new_v4().to_string();
        
        sqlx::query(
            r#"
            INSERT INTO recovery_checkpoints (id, created_at, order_count, fill_count)
            SELECT $1, $2,
                (SELECT COUNT(*) FROM orders WHERE status NOT IN ('Filled', 'Cancelled', 'Rejected', 'Expired')),
                (SELECT COUNT(*) FROM fills)
            "#
        )
        .bind(checkpoint_id.clone())
        .bind(Utc::now())
        .execute(&self.db_pool)
        .await?;
        
        info!("Created recovery checkpoint: {}", checkpoint_id);
        Ok(checkpoint_id)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_discrepancy_types() {
        let discrepancy = OrderDiscrepancy {
            order_id: Uuid::new_v4(),
            discrepancy_type: DiscrepancyType::QuantityMismatch,
            description: "Test discrepancy".to_string(),
            suggested_action: RecoveryAction::RecalculateQuantities,
        };
        
        assert!(matches!(discrepancy.discrepancy_type, DiscrepancyType::QuantityMismatch));
        assert!(matches!(discrepancy.suggested_action, RecoveryAction::RecalculateQuantities));
    }
}