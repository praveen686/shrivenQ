//! Order Management System (OMS)
//! 
//! Institutional-grade OMS with complete order lifecycle management,
//! persistence, audit trail, and recovery capabilities.
//!
//! Features:
//! - Order lifecycle management (New → Pending → Filled/Cancelled/Rejected)
//! - Parent/Child order relationships for algos
//! - Order versioning and amendments
//! - Complete audit trail
//! - Crash recovery from database
//! - Real-time order tracking

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use anyhow::Result;
use chrono::{DateTime, Utc};
use services_common::{Qty, Symbol};
use fxhash::FxHashMap;
use parking_lot::RwLock;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub mod error;
pub mod order;
pub mod lifecycle;
pub mod persistence;
pub mod audit;
pub mod matching;
pub mod recovery;

use error::{OmsError, OmsResult};
use order::{Order, OrderStatus, Fill, Amendment, OrderRequest};
use lifecycle::OrderLifecycleManager;
use persistence::PersistenceManager;
use audit::AuditTrail;

/// OMS Configuration
#[derive(Debug, Clone)]
pub struct OmsConfig {
    /// Database connection string
    pub database_url: String,
    /// Maximum orders in memory
    pub max_orders_memory: usize,
    /// Order retention period (days)
    pub retention_days: u32,
    /// Enable audit trail
    pub enable_audit: bool,
    /// Enable order matching
    pub enable_matching: bool,
    /// Persist every N orders
    pub persist_batch_size: usize,
}

impl Default for OmsConfig {
    fn default() -> Self {
        Self {
            database_url: "postgresql://localhost/shrivenquant_oms".to_string(),
            max_orders_memory: 100000,
            retention_days: 90,
            enable_audit: true,
            enable_matching: false,
            persist_batch_size: 100,
        }
    }
}

/// Main Order Management System
#[derive(Debug)]
pub struct OrderManagementSystem {
    /// Configuration
    config: Arc<OmsConfig>,
    /// Database connection pool
    db_pool: Arc<PgPool>,
    /// Active orders in memory
    active_orders: Arc<RwLock<FxHashMap<Uuid, Order>>>,
    /// Order ID generator
    order_sequence: AtomicU64,
    /// Lifecycle manager
    lifecycle_manager: Arc<OrderLifecycleManager>,
    /// Persistence manager
    persistence_manager: Arc<PersistenceManager>,
    /// Audit trail
    audit_trail: Arc<AuditTrail>,
    /// Event broadcaster
    event_bus: Arc<broadcast::Sender<OrderEvent>>,
    /// Order update channel
    update_tx: mpsc::UnboundedSender<OrderUpdate>,
    /// Metrics
    metrics: Arc<OmsMetrics>,
}

/// Order event for broadcasting
#[derive(Debug, Clone)]
pub enum OrderEvent {
    /// New order created
    OrderCreated(Order),
    /// Order status changed
    OrderStatusChanged {
        /// Order ID
        order_id: Uuid,
        /// Old status
        old_status: OrderStatus,
        /// New status
        new_status: OrderStatus,
        /// Timestamp
        timestamp: DateTime<Utc>,
    },
    /// Order filled
    OrderFilled {
        /// Order ID
        order_id: Uuid,
        /// Fill details
        fill: Fill,
    },
    /// Order amended
    OrderAmended {
        /// Order ID
        order_id: Uuid,
        /// Amendment details
        amendment: Amendment,
    },
    /// Order cancelled
    OrderCancelled {
        /// Order ID
        order_id: Uuid,
        /// Cancel reason
        reason: String,
        /// Timestamp
        timestamp: DateTime<Utc>,
    },
}

/// Order update message
#[derive(Debug, Clone)]
pub struct OrderUpdate {
    /// Order ID
    pub order_id: Uuid,
    /// Update type
    pub update_type: UpdateType,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Update type
#[derive(Debug, Clone)]
pub enum UpdateType {
    /// Status change
    StatusChange(OrderStatus),
    /// Partial fill
    PartialFill(Fill),
    /// Complete fill
    CompleteFill(Fill),
    /// Amendment
    Amendment(Amendment),
    /// Cancellation
    Cancellation(String),
}

/// OMS Metrics
#[derive(Debug)]
pub struct OmsMetrics {
    /// Total orders created
    pub orders_created: AtomicU64,
    /// Orders pending
    pub orders_pending: AtomicU64,
    /// Orders filled
    pub orders_filled: AtomicU64,
    /// Orders cancelled
    pub orders_cancelled: AtomicU64,
    /// Orders rejected
    pub orders_rejected: AtomicU64,
    /// Total volume
    pub total_volume: AtomicU64,
    /// Total fills
    pub total_fills: AtomicU64,
}

impl OrderManagementSystem {
    /// Create new OMS
    pub async fn new(config: OmsConfig) -> Result<Self> {
        info!("Initializing Order Management System");
        
        // Create database connection pool
        let db_pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(&config.database_url)
            .await?;
        
        let db_pool = Arc::new(db_pool);
        
        // Run migrations
        persistence::run_migrations(&db_pool).await?;
        
        // Create event bus
        let (event_tx, _) = broadcast::channel(10000);
        let event_bus = Arc::new(event_tx);
        
        // Create update channel
        let (update_tx, update_rx) = mpsc::unbounded_channel();
        
        // Create components
        let lifecycle_manager = Arc::new(OrderLifecycleManager::new());
        let persistence_manager = Arc::new(PersistenceManager::new((*db_pool).clone()));
        let audit_trail = Arc::new(AuditTrail::new((*db_pool).clone()));
        
        let oms = Self {
            config: Arc::new(config),
            db_pool,
            active_orders: Arc::new(RwLock::new(FxHashMap::default())),
            order_sequence: AtomicU64::new(1),
            lifecycle_manager,
            persistence_manager,
            audit_trail,
            event_bus,
            update_tx,
            metrics: Arc::new(OmsMetrics {
                orders_created: AtomicU64::new(0),
                orders_pending: AtomicU64::new(0),
                orders_filled: AtomicU64::new(0),
                orders_cancelled: AtomicU64::new(0),
                orders_rejected: AtomicU64::new(0),
                total_volume: AtomicU64::new(0),
                total_fills: AtomicU64::new(0),
            }),
        };
        
        // Start update processor
        oms.start_update_processor(update_rx);
        
        // Recover orders from database
        oms.recover_orders().await?;
        
        info!("OMS initialized successfully");
        Ok(oms)
    }
    
    /// Create new order
    pub async fn create_order(&self, request: OrderRequest) -> Result<Order> {
        let start = Instant::now();
        
        // Generate order ID
        let order_id = Uuid::new_v4();
        let sequence = self.order_sequence.fetch_add(1, Ordering::SeqCst);
        
        // Create order
        let order = Order {
            id: order_id,
            client_order_id: request.client_order_id,
            parent_order_id: request.parent_order_id,
            symbol: request.symbol,
            side: request.side,
            order_type: request.order_type,
            time_in_force: request.time_in_force,
            quantity: request.quantity,
            executed_quantity: Qty::ZERO,
            remaining_quantity: request.quantity,
            price: request.price,
            stop_price: request.stop_price,
            status: OrderStatus::New,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            account: request.account,
            exchange: request.exchange,
            strategy_id: request.strategy_id,
            tags: request.tags,
            fills: Vec::new(),
            amendments: Vec::new(),
            version: 1,
            sequence_number: sequence,
        };
        
        // Validate order
        self.lifecycle_manager.validate_order(&order)?;
        
        // Store in memory
        self.active_orders.write().insert(order_id, order.clone());
        
        // Send order update through update channel for async processing
        let order_update = OrderUpdate {
            order_id: order.id,
            update_type: UpdateType::StatusChange(order.status),
            timestamp: order.created_at,
        };
        
        if let Err(e) = self.update_tx.send(order_update) {
            error!("Failed to send order update: {}", e);
        }
        
        // Persist to database
        self.persistence_manager.save_order(&order).await?;
        
        // Audit trail
        if self.config.enable_audit {
            self.audit_trail.log_order_created(&order).await?;
        }
        
        // Update metrics
        self.metrics.orders_created.fetch_add(1, Ordering::Relaxed);
        self.metrics.orders_pending.fetch_add(1, Ordering::Relaxed);
        self.metrics.total_volume.fetch_add(request.quantity.as_i64() as u64, Ordering::Relaxed);
        
        // Broadcast event
        let _ = self.event_bus.send(OrderEvent::OrderCreated(order.clone()));
        
        let latency = start.elapsed();
        debug!("Order {} created in {:?}", order_id, latency);
        
        Ok(order)
    }
    
    /// Submit order to exchange
    pub async fn submit_order(&self, order_id: Uuid) -> OmsResult<()> {
        let mut orders = self.active_orders.write();
        let order = orders.get_mut(&order_id)
            .ok_or_else(|| OmsError::OrderNotFound { order_id: order_id.to_string() })?;
        
        // Validate transition
        self.lifecycle_manager.validate_transition(order, OrderStatus::Pending)?;
        
        // Update status
        let old_status = order.status;
        order.status = OrderStatus::Pending;
        order.updated_at = Utc::now();
        
        // Persist change
        self.persistence_manager.update_order_status(order).await?;
        
        // Audit trail
        if self.config.enable_audit {
            self.audit_trail.log_status_change(order_id, old_status, OrderStatus::Pending).await?;
        }
        
        // Update metrics
        self.metrics.orders_pending.fetch_sub(1, Ordering::Relaxed);
        
        // Broadcast event
        let _ = self.event_bus.send(OrderEvent::OrderStatusChanged {
            order_id,
            old_status,
            new_status: OrderStatus::Pending,
            timestamp: Utc::now(),
        });
        
        info!("Order {} submitted", order_id);
        Ok(())
    }
    
    /// Process fill
    pub async fn process_fill(&self, order_id: Uuid, fill: Fill) -> Result<()> {
        let mut orders = self.active_orders.write();
        let order = orders.get_mut(&order_id)
            .ok_or_else(|| anyhow::anyhow!("Order not found"))?;
        
        // Update quantities
        order.executed_quantity = Qty::from_i64(
            order.executed_quantity.as_i64() + fill.quantity.as_i64()
        );
        order.remaining_quantity = Qty::from_i64(
            order.quantity.as_i64() - order.executed_quantity.as_i64()
        );
        
        // Add fill
        order.fills.push(fill.clone());
        
        // Update status
        let old_status = order.status;
        let new_status = if order.remaining_quantity == Qty::ZERO {
            OrderStatus::Filled
        } else {
            OrderStatus::PartiallyFilled
        };
        
        if new_status != old_status {
            self.lifecycle_manager.validate_transition(order, new_status)?;
            order.status = new_status;
        }
        
        order.updated_at = Utc::now();
        
        // Persist
        self.persistence_manager.save_fill(&fill).await?;
        self.persistence_manager.update_order_quantities(order).await?;
        
        // Audit trail
        if self.config.enable_audit {
            self.audit_trail.log_fill(order_id, &fill).await?;
        }
        
        // Update metrics
        self.metrics.total_fills.fetch_add(1, Ordering::Relaxed);
        if new_status == OrderStatus::Filled {
            self.metrics.orders_filled.fetch_add(1, Ordering::Relaxed);
        }
        
        // Broadcast event
        let _ = self.event_bus.send(OrderEvent::OrderFilled {
            order_id,
            fill: fill.clone(),
        });
        
        info!("Fill processed for order {}: {} @ {}", 
              order_id, fill.quantity.as_f64(), fill.price.as_f64());
        
        Ok(())
    }
    
    /// Cancel order
    pub async fn cancel_order(&self, order_id: Uuid, reason: String) -> Result<()> {
        let mut orders = self.active_orders.write();
        let order = orders.get_mut(&order_id)
            .ok_or_else(|| anyhow::anyhow!("Order not found"))?;
        
        // Validate cancellation
        if !self.lifecycle_manager.can_cancel(order) {
            return Err(anyhow::anyhow!("Order cannot be cancelled in current state"));
        }
        
        // Update status
        let _old_status = order.status;
        order.status = OrderStatus::Cancelled;
        order.updated_at = Utc::now();
        
        // Persist
        self.persistence_manager.update_order_status(order).await?;
        
        // Audit trail
        if self.config.enable_audit {
            self.audit_trail.log_cancellation(order_id, &reason).await?;
        }
        
        // Update metrics
        self.metrics.orders_cancelled.fetch_add(1, Ordering::Relaxed);
        
        // Broadcast event
        let _ = self.event_bus.send(OrderEvent::OrderCancelled {
            order_id,
            reason,
            timestamp: Utc::now(),
        });
        
        info!("Order {} cancelled", order_id);
        Ok(())
    }
    
    /// Amend order
    pub async fn amend_order(&self, order_id: Uuid, amendment: Amendment) -> Result<()> {
        let mut orders = self.active_orders.write();
        let order = orders.get_mut(&order_id)
            .ok_or_else(|| anyhow::anyhow!("Order not found"))?;
        
        // Validate amendment
        if !self.lifecycle_manager.can_amend(order) {
            return Err(anyhow::anyhow!("Order cannot be amended in current state"));
        }
        
        // Apply amendment
        if let Some(new_quantity) = amendment.new_quantity {
            if new_quantity < order.executed_quantity {
                return Err(anyhow::anyhow!("Cannot reduce quantity below executed amount"));
            }
            order.quantity = new_quantity;
            order.remaining_quantity = Qty::from_i64(
                new_quantity.as_i64() - order.executed_quantity.as_i64()
            );
        }
        
        if let Some(new_price) = amendment.new_price {
            order.price = Some(new_price);
        }
        
        // Update version
        order.version += 1;
        order.updated_at = Utc::now();
        order.amendments.push(amendment.clone());
        
        // Persist
        self.persistence_manager.save_amendment(&amendment).await?;
        self.persistence_manager.update_order(order).await?;
        
        // Audit trail
        if self.config.enable_audit {
            self.audit_trail.log_amendment(order_id, &amendment).await?;
        }
        
        // Broadcast event
        let _ = self.event_bus.send(OrderEvent::OrderAmended {
            order_id,
            amendment,
        });
        
        info!("Order {} amended (version {})", order_id, order.version);
        Ok(())
    }
    
    /// Get order by ID
    pub fn get_order(&self, order_id: &Uuid) -> Option<Order> {
        self.active_orders.read().get(order_id).cloned()
    }
    
    /// Get all active orders
    pub fn get_active_orders(&self) -> Vec<Order> {
        self.active_orders
            .read()
            .values()
            .filter(|o| matches!(o.status, 
                OrderStatus::New | OrderStatus::Pending | OrderStatus::PartiallyFilled))
            .cloned()
            .collect()
    }
    
    /// Get orders by symbol
    pub fn get_orders_by_symbol(&self, symbol: Symbol) -> Vec<Order> {
        self.active_orders
            .read()
            .values()
            .filter(|o| o.symbol == symbol)
            .cloned()
            .collect()
    }
    
    /// Get child orders
    pub fn get_child_orders(&self, parent_id: &Uuid) -> Vec<Order> {
        self.active_orders
            .read()
            .values()
            .filter(|o| o.parent_order_id.as_ref() == Some(parent_id))
            .cloned()
            .collect()
    }
    
    /// Start update processor
    fn start_update_processor(&self, mut update_rx: mpsc::UnboundedReceiver<OrderUpdate>) {
        let active_orders = self.active_orders.clone();
        let persistence_manager = self.persistence_manager.clone();
        let audit_trail = self.audit_trail.clone();
        let event_bus = self.event_bus.clone();
        let enable_audit = self.config.enable_audit;
        
        tokio::spawn(async move {
            while let Some(update) = update_rx.recv().await {
                let order_clone = {
                    let mut orders = active_orders.write();
                    if let Some(order) = orders.get_mut(&update.order_id) {
                        match update.update_type {
                            UpdateType::StatusChange(new_status) => {
                                let old_status = order.status;
                                order.status = new_status;
                                order.updated_at = update.timestamp;
                                Some((order.clone(), old_status, new_status))
                            }
                            UpdateType::PartialFill(ref fill) | UpdateType::CompleteFill(ref fill) => {
                                order.fills.push(fill.clone());
                                order.executed_quantity = Qty::from_i64(
                                    order.executed_quantity.as_i64() + fill.quantity.as_i64()
                                );
                                order.remaining_quantity = Qty::from_i64(
                                    order.quantity.as_i64() - order.executed_quantity.as_i64()
                                );
                                Some((order.clone(), OrderStatus::New, OrderStatus::New)) // Dummy statuses
                            }
                            _ => None,
                        }
                    } else {
                        None
                    }
                };
                
                if let Some((order, old_status, new_status)) = order_clone {
                    match update.update_type {
                        UpdateType::StatusChange(_) => {
                            let _ = persistence_manager.update_order_status(&order).await;
                            
                            if enable_audit {
                                let _ = audit_trail.log_status_change(
                                    update.order_id, 
                                    old_status, 
                                    new_status
                                ).await;
                            }
                            
                            let _ = event_bus.send(OrderEvent::OrderStatusChanged {
                                order_id: update.order_id,
                                old_status,
                                new_status,
                                timestamp: update.timestamp,
                            });
                        }
                        UpdateType::PartialFill(ref fill) | UpdateType::CompleteFill(ref fill) => {
                            let _ = persistence_manager.save_fill(fill).await;
                            
                            if enable_audit {
                                let _ = audit_trail.log_fill(update.order_id, fill).await;
                            }
                            
                            let _ = event_bus.send(OrderEvent::OrderFilled {
                                order_id: update.order_id,
                                fill: fill.clone(),
                            });
                        }
                        _ => {}
                    }
                }
            }
        });
    }
    
    /// Recover orders from database
    async fn recover_orders(&self) -> Result<()> {
        info!("Recovering orders from database");
        
        // Use direct database access for complex recovery queries
        let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE status IN ('pending', 'partially_filled')")
            .fetch_one(&*self.db_pool)
            .await?;
        
        info!("Found {} active orders in database", row_count);
        
        let orders = self.persistence_manager.load_active_orders().await?;
        
        let mut active_orders = self.active_orders.write();
        for order in orders {
            active_orders.insert(order.id, order);
        }
        
        info!("Recovered {} orders into memory", active_orders.len());
        Ok(())
    }
    
    /// Get metrics
    pub fn get_metrics(&self) -> OmsMetricsSnapshot {
        OmsMetricsSnapshot {
            orders_created: self.metrics.orders_created.load(Ordering::Relaxed),
            orders_pending: self.metrics.orders_pending.load(Ordering::Relaxed),
            orders_filled: self.metrics.orders_filled.load(Ordering::Relaxed),
            orders_cancelled: self.metrics.orders_cancelled.load(Ordering::Relaxed),
            orders_rejected: self.metrics.orders_rejected.load(Ordering::Relaxed),
            total_volume: self.metrics.total_volume.load(Ordering::Relaxed),
            total_fills: self.metrics.total_fills.load(Ordering::Relaxed),
            active_orders: self.active_orders.read().len(),
        }
    }
    
    /// Subscribe to order events
    pub fn subscribe(&self) -> broadcast::Receiver<OrderEvent> {
        self.event_bus.subscribe()
    }
}

/// OMS metrics snapshot
#[derive(Debug, Clone)]
pub struct OmsMetricsSnapshot {
    /// Total orders created
    pub orders_created: u64,
    /// Orders pending
    pub orders_pending: u64,
    /// Orders filled
    pub orders_filled: u64,
    /// Orders cancelled
    pub orders_cancelled: u64,
    /// Orders rejected
    pub orders_rejected: u64,
    /// Total volume
    pub total_volume: u64,
    /// Total fills
    pub total_fills: u64,
    /// Active orders in memory
    pub active_orders: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_oms_creation() {
        let config = OmsConfig {
            database_url: "postgresql://localhost/test_oms".to_string(),
            ..Default::default()
        };
        
        // Would need test database for this
        // let oms = OrderManagementSystem::new(config).await;
        // assert!(oms.is_ok());
    }
}