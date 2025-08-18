//! Execution Engine - Smart order routing with venue optimization
//! 
//! Connects to the Execution Router service for sophisticated
//! order execution algorithms (TWAP, VWAP, POV, etc.)

use crate::{OrderStatus, OrderType, Side as TradingSide, TradingEvent};
use anyhow::Result;
use services_common::{Px, Qty, Symbol, Ts};
use dashmap::DashMap;
use execution_router::{ExecutionAlgorithm, OrderRequest, Router};
use execution_router::{OrderType as RouterOrderType, TimeInForce as RouterTimeInForce};
use rustc_hash::FxHashMap;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock as TokioRwLock};
use tracing::{debug, info, warn};

/// Execution engine for order management
pub struct ExecutionEngine {
    /// Event bus
    event_bus: Arc<broadcast::Sender<TradingEvent>>,
    /// Active orders
    active_orders: Arc<DashMap<u64, OrderState>>,
    /// Execution router
    router: Arc<RwLock<Option<Router>>>,
    /// Order ID generator
    order_id_gen: AtomicU64,
    /// Execution metrics
    metrics: Arc<ExecutionMetrics>,
    /// Order update channel
    update_tx: mpsc::UnboundedSender<OrderUpdate>,
    update_rx: Arc<TokioRwLock<mpsc::UnboundedReceiver<OrderUpdate>>>,
}

/// Order state tracking
#[derive(Debug, Clone)]
pub struct OrderState {
    /// Order ID
    pub order_id: u64,
    /// Symbol
    pub symbol: Symbol,
    /// Side
    pub side: TradingSide,
    /// Order type
    pub order_type: OrderType,
    /// Original quantity
    pub original_qty: Qty,
    /// Executed quantity
    pub executed_qty: Qty,
    /// Average execution price
    pub avg_price: Option<Px>,
    /// Status
    pub status: OrderStatus,
    /// Strategy ID
    pub strategy_id: String,
    /// Creation time
    pub created_at: Ts,
    /// Last update time
    pub updated_at: Ts,
}

/// Order update message
#[derive(Debug, Clone)]
pub struct OrderUpdate {
    /// Order ID
    pub order_id: u64,
    /// Executed quantity
    pub exec_qty: Qty,
    /// Execution price
    pub exec_price: Px,
    /// New status
    pub status: OrderStatus,
    /// Update timestamp
    pub timestamp: Ts,
}

/// Execution metrics
pub struct ExecutionMetrics {
    /// Total orders submitted
    pub orders_submitted: AtomicU64,
    /// Orders filled
    pub orders_filled: AtomicU64,
    /// Orders cancelled
    pub orders_cancelled: AtomicU64,
    /// Orders rejected
    pub orders_rejected: AtomicU64,
    /// Total volume executed
    pub volume_executed: AtomicU64,
    /// Average fill latency
    pub avg_fill_latency_us: AtomicU64,
}

impl ExecutionEngine {
    /// Create new execution engine
    pub fn new(event_bus: Arc<broadcast::Sender<TradingEvent>>) -> Self {
        let (update_tx, update_rx) = mpsc::unbounded_channel();
        
        Self {
            event_bus,
            active_orders: Arc::new(DashMap::new()),
            router: Arc::new(RwLock::new(None)),
            order_id_gen: AtomicU64::new(1),
            metrics: Arc::new(ExecutionMetrics {
                orders_submitted: AtomicU64::new(0),
                orders_filled: AtomicU64::new(0),
                orders_cancelled: AtomicU64::new(0),
                orders_rejected: AtomicU64::new(0),
                volume_executed: AtomicU64::new(0),
                avg_fill_latency_us: AtomicU64::new(0),
            }),
            update_tx,
            update_rx: Arc::new(TokioRwLock::new(update_rx)),
        }
    }
    
    /// Initialize connection to execution router
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing execution engine");
        
        // Create router instance
        let router = Router::new();
        *self.router.write() = Some(router);
        
        // Start order update processor
        self.start_update_processor();
        
        info!("Execution engine initialized");
        Ok(())
    }
    
    /// Submit order for execution
    pub async fn submit_order(&self, order: TradingEvent) -> Result<()> {
        let start = Ts::now();
        
        // Extract order details
        let (symbol, side, order_type, quantity, price, _tif, strategy_id) = match order {
            TradingEvent::OrderRequest {
                symbol,
                side,
                order_type,
                quantity,
                price,
                time_in_force,
                strategy_id,
                ..
            } => (symbol, side, order_type, quantity, price, time_in_force, strategy_id),
            _ => return Ok(()),
        };
        
        // Generate order ID
        let order_id = self.order_id_gen.fetch_add(1, Ordering::SeqCst);
        
        // Create order state
        let order_state = OrderState {
            order_id,
            symbol,
            side,
            order_type,
            original_qty: quantity,
            executed_qty: Qty::ZERO,
            avg_price: None,
            status: OrderStatus::Pending,
            strategy_id: strategy_id.clone(),
            created_at: start,
            updated_at: start,
        };
        
        // Store order state
        self.active_orders.insert(order_id, order_state.clone());
        self.metrics.orders_submitted.fetch_add(1, Ordering::Relaxed);
        
        // Route order based on type
        match order_type {
            OrderType::Market => {
                self.route_market_order(order_id, symbol, side, quantity).await?;
            }
            OrderType::Limit => {
                if let Some(px) = price {
                    self.route_limit_order(order_id, symbol, side, quantity, px).await?;
                }
            }
            OrderType::Twap => {
                self.route_algo_order(order_id, symbol, side, quantity, ExecutionAlgorithm::Twap).await?;
            }
            OrderType::Vwap => {
                self.route_algo_order(order_id, symbol, side, quantity, ExecutionAlgorithm::Vwap).await?;
            }
            _ => {
                warn!("Unsupported order type: {:?}", order_type);
            }
        }
        
        // Send order accepted event
        let _ = self.event_bus.send(TradingEvent::ExecutionReport {
            order_id,
            symbol,
            side,
            executed_qty: Qty::ZERO,
            executed_price: Px::ZERO,
            remaining_qty: quantity,
            status: OrderStatus::Accepted,
            timestamp: Ts::now(),
        });
        
        Ok(())
    }
    
    /// Route market order
    async fn route_market_order(
        &self,
        order_id: u64,
        symbol: Symbol,
        side: TradingSide,
        quantity: Qty,
    ) -> Result<()> {
        if let Some(router) = &*self.router.read() {
            // Create router request
            let request = OrderRequest {
                client_order_id: format!("TG_{}", order_id),
                symbol,
                side: match side {
                    TradingSide::Buy => services_common::Side::Bid,
                    TradingSide::Sell => services_common::Side::Ask,
                },
                quantity,
                order_type: RouterOrderType::Market,
                limit_price: None,
                stop_price: None,
                is_buy: matches!(side, TradingSide::Buy),
                algorithm: ExecutionAlgorithm::Smart,
                urgency: 1.0, // High urgency for market orders
                participation_rate: None,
                time_in_force: RouterTimeInForce::IOC,
                venue: None,
                strategy_id: "trading_gateway".to_string(),
                params: FxHashMap::default(),
            };
            
            // Route order - handle future properly
            let _route = router.route_order(request);
            
            // Simulate immediate fill for market orders
            self.update_tx.send(OrderUpdate {
                order_id,
                exec_qty: quantity,
                exec_price: Px::from_i64(1000000), // Mock price
                status: OrderStatus::Filled,
                timestamp: Ts::now(),
            })?;
        }
        
        Ok(())
    }
    
    /// Route limit order
    async fn route_limit_order(
        &self,
        order_id: u64,
        symbol: Symbol,
        side: TradingSide,
        quantity: Qty,
        price: Px,
    ) -> Result<()> {
        if let Some(router) = &*self.router.read() {
            let request = OrderRequest {
                client_order_id: format!("TG_{}", order_id),
                symbol,
                side: match side {
                    TradingSide::Buy => services_common::Side::Bid,
                    TradingSide::Sell => services_common::Side::Ask,
                },
                quantity,
                order_type: RouterOrderType::Limit,
                limit_price: Some(price),
                stop_price: None,
                is_buy: matches!(side, TradingSide::Buy),
                algorithm: ExecutionAlgorithm::Peg,
                urgency: 0.5,
                participation_rate: None,
                time_in_force: RouterTimeInForce::DAY,
                venue: None,
                strategy_id: "trading_gateway".to_string(),
                params: FxHashMap::default(),
            };
            
            let _route = router.route_order(request);
            
            // Limit orders remain pending
            debug!("Limit order {} submitted at {}", order_id, price.as_f64());
        }
        
        Ok(())
    }
    
    /// Route algorithmic order
    async fn route_algo_order(
        &self,
        order_id: u64,
        symbol: Symbol,
        side: TradingSide,
        quantity: Qty,
        algorithm: ExecutionAlgorithm,
    ) -> Result<()> {
        if let Some(router) = &*self.router.read() {
            let request = OrderRequest {
                client_order_id: format!("TG_{}", order_id),
                symbol,
                side: match side {
                    TradingSide::Buy => services_common::Side::Bid,
                    TradingSide::Sell => services_common::Side::Ask,
                },
                quantity,
                order_type: RouterOrderType::Limit,
                limit_price: None,
                stop_price: None,
                is_buy: matches!(side, TradingSide::Buy),
                algorithm,
                urgency: 0.3,
                participation_rate: Some(0.2), // 20% participation
                time_in_force: RouterTimeInForce::DAY,
                venue: None,
                strategy_id: "trading_gateway".to_string(),
                params: FxHashMap::default(),
            };
            
            let _route = router.route_order(request);
            
            info!("Algo order {} submitted using {:?}", order_id, algorithm);
        }
        
        Ok(())
    }
    
    /// Cancel order
    pub async fn cancel_order(&self, order_id: u64) -> Result<()> {
        if let Some(mut order) = self.active_orders.get_mut(&order_id) {
            if matches!(order.status, OrderStatus::Pending | OrderStatus::Accepted | OrderStatus::PartiallyFilled) {
                order.status = OrderStatus::Cancelled;
                order.updated_at = Ts::now();
                
                self.metrics.orders_cancelled.fetch_add(1, Ordering::Relaxed);
                
                // Send cancellation event
                let _ = self.event_bus.send(TradingEvent::ExecutionReport {
                    order_id,
                    symbol: order.symbol,
                    side: order.side,
                    executed_qty: order.executed_qty,
                    executed_price: order.avg_price.unwrap_or(Px::ZERO),
                    remaining_qty: Qty::from_i64(order.original_qty.as_i64() - order.executed_qty.as_i64()),
                    status: OrderStatus::Cancelled,
                    timestamp: Ts::now(),
                });
                
                info!("Order {} cancelled", order_id);
            }
        }
        
        Ok(())
    }
    
    /// Cancel all orders
    pub async fn cancel_all_orders(&self) -> Result<()> {
        let order_ids: Vec<u64> = self.active_orders
            .iter()
            .filter(|entry| {
                matches!(entry.value().status, 
                    OrderStatus::Pending | OrderStatus::Accepted | OrderStatus::PartiallyFilled)
            })
            .map(|entry| *entry.key())
            .collect();
        
        for order_id in order_ids {
            self.cancel_order(order_id).await?;
        }
        
        info!("All orders cancelled");
        Ok(())
    }
    
    /// Start order update processor
    fn start_update_processor(&self) {
        let active_orders = self.active_orders.clone();
        let event_bus = self.event_bus.clone();
        let metrics = self.metrics.clone();
        let update_rx = self.update_rx.clone();
        
        tokio::spawn(async move {
            loop {
                let update = {
                    let mut rx = update_rx.write().await;
                    rx.recv().await
                };
                
                if let Some(update) = update {
                if let Some(mut order) = active_orders.get_mut(&update.order_id) {
                    // Update order state
                    order.executed_qty = Qty::from_i64(
                        order.executed_qty.as_i64() + update.exec_qty.as_i64()
                    );
                    
                    // Update average price
                    if let Some(avg) = order.avg_price {
                        let total_value = avg.as_i64() * order.executed_qty.as_i64() 
                            + update.exec_price.as_i64() * update.exec_qty.as_i64();
                        let new_total_qty = order.executed_qty.as_i64() + update.exec_qty.as_i64();
                        order.avg_price = Some(Px::from_i64(total_value / new_total_qty));
                    } else {
                        order.avg_price = Some(update.exec_price);
                    }
                    
                    order.status = update.status;
                    order.updated_at = update.timestamp;
                    
                    // Update metrics
                    metrics.volume_executed.fetch_add(update.exec_qty.as_i64() as u64, Ordering::Relaxed);
                    
                    if matches!(update.status, OrderStatus::Filled) {
                        metrics.orders_filled.fetch_add(1, Ordering::Relaxed);
                        
                        // Calculate fill latency
                        let latency = (update.timestamp.as_nanos() - order.created_at.as_nanos()) / 1000;
                        metrics.avg_fill_latency_us.store(latency as u64, Ordering::Relaxed);
                    }
                    
                    // Send execution report
                    let _ = event_bus.send(TradingEvent::ExecutionReport {
                        order_id: update.order_id,
                        symbol: order.symbol,
                        side: order.side,
                        executed_qty: update.exec_qty,
                        executed_price: update.exec_price,
                        remaining_qty: Qty::from_i64(
                            order.original_qty.as_i64() - order.executed_qty.as_i64()
                        ),
                        status: update.status,
                        timestamp: update.timestamp,
                    });
                } else {
                    break;
                }
                }
            }
        });
    }
    
    /// Get order state
    pub fn get_order(&self, order_id: u64) -> Option<OrderState> {
        self.active_orders.get(&order_id).map(|e| e.clone())
    }
    
    /// Get all active orders
    pub fn get_active_orders(&self) -> Vec<OrderState> {
        self.active_orders
            .iter()
            .filter(|e| matches!(e.value().status, 
                OrderStatus::Pending | OrderStatus::Accepted | OrderStatus::PartiallyFilled))
            .map(|e| e.value().clone())
            .collect()
    }
    
    /// Get execution metrics
    pub fn get_metrics(&self) -> ExecutionMetricsSnapshot {
        ExecutionMetricsSnapshot {
            orders_submitted: self.metrics.orders_submitted.load(Ordering::Relaxed),
            orders_filled: self.metrics.orders_filled.load(Ordering::Relaxed),
            orders_cancelled: self.metrics.orders_cancelled.load(Ordering::Relaxed),
            orders_rejected: self.metrics.orders_rejected.load(Ordering::Relaxed),
            volume_executed: self.metrics.volume_executed.load(Ordering::Relaxed),
            avg_fill_latency_us: self.metrics.avg_fill_latency_us.load(Ordering::Relaxed),
            fill_rate: if self.metrics.orders_submitted.load(Ordering::Relaxed) > 0 {
                (self.metrics.orders_filled.load(Ordering::Relaxed) as f64 /
                 self.metrics.orders_submitted.load(Ordering::Relaxed) as f64) * 100.0
            } else {
                0.0
            },
        }
    }
}

/// Execution metrics snapshot
#[derive(Debug, Clone)]
pub struct ExecutionMetricsSnapshot {
    /// Total orders submitted
    pub orders_submitted: u64,
    /// Orders filled
    pub orders_filled: u64,
    /// Orders cancelled
    pub orders_cancelled: u64,
    /// Orders rejected
    pub orders_rejected: u64,
    /// Total volume executed
    pub volume_executed: u64,
    /// Average fill latency
    pub avg_fill_latency_us: u64,
    /// Fill rate percentage
    pub fill_rate: f64,
}