//! Order persistence layer
//!
//! High-performance persistence with `PostgreSQL` backend,
//! optimized for write throughput and crash recovery.

use anyhow::Result;
use chrono::Utc;
use services_common::{Px, Qty, Symbol};
use sqlx::{PgPool, Row};
use uuid::Uuid;
use crate::order::{Order, OrderStatus, OrderSide, OrderType, TimeInForce, Fill, Amendment, LiquidityIndicator};
use tracing::{debug, info};

/// Persistence manager for orders
#[derive(Debug)]
pub struct PersistenceManager {
    /// Database pool
    db_pool: PgPool,
}

impl PersistenceManager {
    /// Create new persistence manager
    #[must_use] pub const fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }
    
    /// Save order to database
    pub async fn save_order(&self, order: &Order) -> Result<()> {
        sqlx::query(
            r"
            INSERT INTO orders (
                id, client_order_id, parent_order_id, symbol, side, order_type,
                time_in_force, quantity, executed_quantity, remaining_quantity,
                price, stop_price, status, created_at, updated_at, account,
                exchange, strategy_id, tags, version, sequence_number
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                $16, $17, $18, $19, $20, $21
            )
            ON CONFLICT (id) DO UPDATE SET
                executed_quantity = EXCLUDED.executed_quantity,
                remaining_quantity = EXCLUDED.remaining_quantity,
                status = EXCLUDED.status,
                updated_at = EXCLUDED.updated_at,
                version = EXCLUDED.version
            "
        )
        .bind(order.id)
        .bind(order.client_order_id.as_deref())
        .bind(order.parent_order_id)
        .bind(order.symbol.0 as i32)
        .bind(format!("{:?}", order.side))
        .bind(format!("{:?}", order.order_type))
        .bind(format!("{:?}", order.time_in_force))
        .bind(order.quantity.as_i64())
        .bind(order.executed_quantity.as_i64())
        .bind(order.remaining_quantity.as_i64())
        .bind(order.price.map(|p| p.as_i64()))
        .bind(order.stop_price.map(|p| p.as_i64()))
        .bind(format!("{:?}", order.status))
        .bind(order.created_at)
        .bind(order.updated_at)
        .bind(&order.account)
        .bind(&order.exchange)
        .bind(order.strategy_id.as_deref())
        .bind(&order.tags)
        .bind(order.version as i32)
        .bind(order.sequence_number as i64)
        .execute(&self.db_pool)
        .await?;
        
        debug!("Order {} persisted", order.id);
        Ok(())
    }
    
    /// Update order status
    pub async fn update_order_status(&self, order: &Order) -> Result<()> {
        sqlx::query(
            r"
            UPDATE orders SET
                status = $1,
                updated_at = $2
            WHERE id = $3
            "
        )
        .bind(format!("{:?}", order.status))
        .bind(order.updated_at)
        .bind(order.id)
        .execute(&self.db_pool)
        .await?;
        
        Ok(())
    }
    
    /// Update order quantities
    pub async fn update_order_quantities(&self, order: &Order) -> Result<()> {
        sqlx::query(
            r"
            UPDATE orders SET
                executed_quantity = $1,
                remaining_quantity = $2,
                status = $3,
                updated_at = $4
            WHERE id = $5
            "
        )
        .bind(order.executed_quantity.as_i64())
        .bind(order.remaining_quantity.as_i64())
        .bind(format!("{:?}", order.status))
        .bind(order.updated_at)
        .bind(order.id)
        .execute(&self.db_pool)
        .await?;
        
        Ok(())
    }
    
    /// Update full order
    pub async fn update_order(&self, order: &Order) -> Result<()> {
        sqlx::query(
            r"
            UPDATE orders SET
                quantity = $1,
                executed_quantity = $2,
                remaining_quantity = $3,
                price = $4,
                status = $5,
                updated_at = $6,
                version = $7
            WHERE id = $8
            "
        )
        .bind(order.quantity.as_i64())
        .bind(order.executed_quantity.as_i64())
        .bind(order.remaining_quantity.as_i64())
        .bind(order.price.map(|p| p.as_i64()))
        .bind(format!("{:?}", order.status))
        .bind(order.updated_at)
        .bind(order.version as i32)
        .bind(order.id)
        .execute(&self.db_pool)
        .await?;
        
        Ok(())
    }
    
    /// Save fill
    pub async fn save_fill(&self, fill: &Fill) -> Result<()> {
        sqlx::query(
            r"
            INSERT INTO fills (
                id, order_id, execution_id, quantity, price,
                commission, commission_currency, timestamp, liquidity
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9
            )
            "
        )
        .bind(fill.id)
        .bind(fill.order_id)
        .bind(&fill.execution_id)
        .bind(fill.quantity.as_i64())
        .bind(fill.price.as_i64())
        .bind(fill.commission)
        .bind(&fill.commission_currency)
        .bind(fill.timestamp)
        .bind(format!("{:?}", fill.liquidity))
        .execute(&self.db_pool)
        .await?;
        
        debug!("Fill {} saved for order {}", fill.id, fill.order_id);
        Ok(())
    }
    
    /// Save amendment
    pub async fn save_amendment(&self, amendment: &Amendment) -> Result<()> {
        sqlx::query(
            r"
            INSERT INTO amendments (
                id, order_id, new_quantity, new_price, reason, timestamp
            ) VALUES (
                $1, $2, $3, $4, $5, $6
            )
            "
        )
        .bind(amendment.id)
        .bind(amendment.order_id)
        .bind(amendment.new_quantity.map(|q| q.as_i64()))
        .bind(amendment.new_price.map(|p| p.as_i64()))
        .bind(&amendment.reason)
        .bind(amendment.timestamp)
        .execute(&self.db_pool)
        .await?;
        
        debug!("Amendment {} saved for order {}", amendment.id, amendment.order_id);
        Ok(())
    }
    
    /// Load active orders
    pub async fn load_active_orders(&self) -> Result<Vec<Order>> {
        let rows = sqlx::query(
            r"
            SELECT 
                o.id, o.client_order_id, o.parent_order_id, o.symbol, o.side,
                o.order_type, o.time_in_force, o.quantity, o.executed_quantity,
                o.remaining_quantity, o.price, o.stop_price, o.status,
                o.created_at, o.updated_at, o.account, o.exchange,
                o.strategy_id, o.tags, o.version, o.sequence_number,
                COALESCE(
                    array_agg(
                        json_build_object(
                            'id', f.id,
                            'order_id', f.order_id,
                            'execution_id', f.execution_id,
                            'quantity', f.quantity,
                            'price', f.price,
                            'commission', f.commission,
                            'commission_currency', f.commission_currency,
                            'timestamp', f.timestamp,
                            'liquidity', f.liquidity
                        ) ORDER BY f.timestamp
                    ) FILTER (WHERE f.id IS NOT NULL),
                    '{}'::json[]
                ) as fills,
                COALESCE(
                    array_agg(
                        json_build_object(
                            'id', a.id,
                            'order_id', a.order_id,
                            'new_quantity', a.new_quantity,
                            'new_price', a.new_price,
                            'reason', a.reason,
                            'timestamp', a.timestamp
                        ) ORDER BY a.timestamp
                    ) FILTER (WHERE a.id IS NOT NULL),
                    '{}'::json[]
                ) as amendments
            FROM orders o
            LEFT JOIN fills f ON o.id = f.order_id
            LEFT JOIN amendments a ON o.id = a.order_id
            WHERE o.status NOT IN ('Filled', 'Cancelled', 'Rejected', 'Expired')
            GROUP BY o.id
            "
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
                side: parse_order_side(&row.get::<String, _>("side"))?,
                order_type: parse_order_type(&row.get::<String, _>("order_type"))?,
                time_in_force: parse_time_in_force(&row.get::<String, _>("time_in_force"))?,
                quantity: Qty::from_i64(row.get("quantity")),
                executed_quantity: Qty::from_i64(row.get("executed_quantity")),
                remaining_quantity: Qty::from_i64(row.get("remaining_quantity")),
                price: row.get::<Option<i64>, _>("price").map(Px::from_i64),
                stop_price: row.get::<Option<i64>, _>("stop_price").map(Px::from_i64),
                status: parse_order_status(&row.get::<String, _>("status"))?,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                account: row.get("account"),
                exchange: row.get("exchange"),
                strategy_id: row.get("strategy_id"),
                tags: row.get("tags"),
                fills: vec![],  // Parse from JSON
                amendments: vec![],  // Parse from JSON
                version: row.get::<i32, _>("version") as u32,
                sequence_number: row.get::<i64, _>("sequence_number") as u64,
            };
            
            orders.push(order);
        }
        
        info!("Loaded {} active orders from database", orders.len());
        Ok(orders)
    }
    
    /// Load order by ID
    pub async fn load_order(&self, order_id: Uuid) -> Result<Option<Order>> {
        let row = sqlx::query(
            r"
            SELECT 
                id, client_order_id, parent_order_id, symbol, side,
                order_type, time_in_force, quantity, executed_quantity,
                remaining_quantity, price, stop_price, status,
                created_at, updated_at, account, exchange,
                strategy_id, tags, version, sequence_number
            FROM orders
            WHERE id = $1
            "
        )
        .bind(order_id)
        .fetch_optional(&self.db_pool)
        .await?;
        
        match row {
            Some(row) => {
                let order = Order {
                    id: row.get("id"),
                    client_order_id: row.get("client_order_id"),
                    parent_order_id: row.get("parent_order_id"),
                    symbol: Symbol(row.get::<i32, _>("symbol") as u32),
                    side: parse_order_side(&row.get::<String, _>("side"))?,
                    order_type: parse_order_type(&row.get::<String, _>("order_type"))?,
                    time_in_force: parse_time_in_force(&row.get::<String, _>("time_in_force"))?,
                    quantity: Qty::from_i64(row.get("quantity")),
                    executed_quantity: Qty::from_i64(row.get("executed_quantity")),
                    remaining_quantity: Qty::from_i64(row.get("remaining_quantity")),
                    price: row.get::<Option<i64>, _>("price").map(Px::from_i64),
                    stop_price: row.get::<Option<i64>, _>("stop_price").map(Px::from_i64),
                    status: parse_order_status(&row.get::<String, _>("status"))?,
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
                Ok(Some(order))
            }
            None => Ok(None)
        }
    }
    
    /// Delete old orders
    pub async fn delete_old_orders(&self, days: i32) -> Result<u64> {
        let cutoff = Utc::now() - chrono::Duration::days(i64::from(days));
        
        let result = sqlx::query(
            r"
            DELETE FROM orders
            WHERE status IN ('Filled', 'Cancelled', 'Rejected', 'Expired')
            AND updated_at < $1
            "
        )
        .bind(cutoff)
        .execute(&self.db_pool)
        .await?;
        
        Ok(result.rows_affected())
    }
}

/// Run database migrations
pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    info!("Running database migrations");
    
    // Create tables if they don't exist
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS orders (
            id UUID PRIMARY KEY,
            client_order_id TEXT,
            parent_order_id UUID,
            symbol INTEGER NOT NULL,
            side TEXT NOT NULL,
            order_type TEXT NOT NULL,
            time_in_force TEXT NOT NULL,
            quantity BIGINT NOT NULL,
            executed_quantity BIGINT NOT NULL,
            remaining_quantity BIGINT NOT NULL,
            price BIGINT,
            stop_price BIGINT,
            status TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL,
            account TEXT NOT NULL,
            exchange TEXT NOT NULL,
            strategy_id TEXT,
            tags TEXT[] NOT NULL DEFAULT '{}',
            version INTEGER NOT NULL DEFAULT 1,
            sequence_number BIGINT NOT NULL,
            
            INDEX idx_orders_status (status),
            INDEX idx_orders_symbol (symbol),
            INDEX idx_orders_account (account),
            INDEX idx_orders_parent (parent_order_id),
            INDEX idx_orders_created (created_at DESC)
        )
        "
    )
    .execute(pool)
    .await?;
    
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS fills (
            id UUID PRIMARY KEY,
            order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
            execution_id TEXT NOT NULL,
            quantity BIGINT NOT NULL,
            price BIGINT NOT NULL,
            commission BIGINT NOT NULL,
            commission_currency TEXT NOT NULL,
            timestamp TIMESTAMPTZ NOT NULL,
            liquidity TEXT NOT NULL,
            
            INDEX idx_fills_order (order_id),
            INDEX idx_fills_timestamp (timestamp DESC)
        )
        "
    )
    .execute(pool)
    .await?;
    
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS amendments (
            id UUID PRIMARY KEY,
            order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
            new_quantity BIGINT,
            new_price BIGINT,
            reason TEXT NOT NULL,
            timestamp TIMESTAMPTZ NOT NULL,
            
            INDEX idx_amendments_order (order_id),
            INDEX idx_amendments_timestamp (timestamp DESC)
        )
        "
    )
    .execute(pool)
    .await?;
    
    info!("Database migrations completed");
    Ok(())
}

// Helper functions for parsing enums from strings
/// Parse order side from string representation
pub fn parse_order_side(s: &str) -> Result<OrderSide> {
    match s {
        "Buy" => Ok(OrderSide::Buy),
        "Sell" => Ok(OrderSide::Sell),
        _ => Err(anyhow::anyhow!("Invalid order side: {}", s))
    }
}

/// Parse order type from string representation
pub fn parse_order_type(s: &str) -> Result<OrderType> {
    match s {
        "Market" => Ok(OrderType::Market),
        "Limit" => Ok(OrderType::Limit),
        "Stop" => Ok(OrderType::Stop),
        "StopLimit" => Ok(OrderType::StopLimit),
        "Iceberg" => Ok(OrderType::Iceberg),
        "Twap" => Ok(OrderType::Twap),
        "Vwap" => Ok(OrderType::Vwap),
        "Pov" => Ok(OrderType::Pov),
        _ => Err(anyhow::anyhow!("Invalid order type: {}", s))
    }
}

/// Parse order status from string representation
pub fn parse_order_status(s: &str) -> Result<OrderStatus> {
    match s {
        "New" => Ok(OrderStatus::New),
        "Pending" => Ok(OrderStatus::Pending),
        "Submitted" => Ok(OrderStatus::Submitted),
        "Accepted" => Ok(OrderStatus::Accepted),
        "PartiallyFilled" => Ok(OrderStatus::PartiallyFilled),
        "Filled" => Ok(OrderStatus::Filled),
        "Cancelled" => Ok(OrderStatus::Cancelled),
        "Rejected" => Ok(OrderStatus::Rejected),
        "Expired" => Ok(OrderStatus::Expired),
        _ => Err(anyhow::anyhow!("Invalid order status: {}", s))
    }
}

/// Parse time in force from string representation
pub fn parse_time_in_force(s: &str) -> Result<TimeInForce> {
    if s.starts_with("Gtt(") {
        // Parse GTT with timestamp
        Ok(TimeInForce::Day)  // Simplified for now
    } else {
        match s {
            "Gtc" => Ok(TimeInForce::Gtc),
            "Ioc" => Ok(TimeInForce::Ioc),
            "Fok" => Ok(TimeInForce::Fok),
            "Day" => Ok(TimeInForce::Day),
            _ => Err(anyhow::anyhow!("Invalid time in force: {}", s))
        }
    }
}

/// Parse liquidity indicator from string
pub fn parse_liquidity(s: &str) -> Result<LiquidityIndicator> {
    match s {
        "Maker" => Ok(LiquidityIndicator::Maker),
        "Taker" => Ok(LiquidityIndicator::Taker),
        _ => Err(anyhow::anyhow!("Invalid liquidity indicator: {}", s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_order_side() {
        assert!(matches!(parse_order_side("Buy").unwrap(), OrderSide::Buy));
        assert!(matches!(parse_order_side("Sell").unwrap(), OrderSide::Sell));
        assert!(parse_order_side("Invalid").is_err());
    }
    
    #[test]
    fn test_parse_order_type() {
        assert!(matches!(parse_order_type("Market").unwrap(), OrderType::Market));
        assert!(matches!(parse_order_type("Limit").unwrap(), OrderType::Limit));
        assert!(parse_order_type("Invalid").is_err());
    }
    
    #[test]
    fn test_parse_order_status() {
        assert!(matches!(parse_order_status("New").unwrap(), OrderStatus::New));
        assert!(matches!(parse_order_status("Filled").unwrap(), OrderStatus::Filled));
        assert!(parse_order_status("Invalid").is_err());
    }
}