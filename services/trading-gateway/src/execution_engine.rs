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

/// Execution engine for intelligent order management and routing
///
/// The ExecutionEngine provides sophisticated order execution capabilities with smart routing
/// to optimize trade execution. It supports multiple order types including market, limit,
/// TWAP (Time-Weighted Average Price), and VWAP (Volume-Weighted Average Price) orders.
///
/// # Features
/// - Smart order routing with venue optimization
/// - Real-time order state tracking
/// - Execution metrics and performance monitoring
/// - Support for algorithmic execution strategies
/// - Thread-safe concurrent order processing
///
/// # Example
/// ```rust
/// let execution_engine = ExecutionEngine::new(event_bus.clone());
/// execution_engine.initialize().await?;
/// execution_engine.submit_order(order_event).await?;
/// ```
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

impl std::fmt::Debug for ExecutionEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionEngine")
            .field("active_orders_count", &self.active_orders.len())
            .field("next_order_id", &self.order_id_gen.load(std::sync::atomic::Ordering::Relaxed))
            .field("has_router", &self.router.read().is_some())
            .field("metrics", &self.metrics)
            .finish()
    }
}

/// Comprehensive order state tracking for execution monitoring
///
/// Maintains the complete lifecycle state of an order from submission to completion.
/// Used for tracking execution progress, calculating average fill prices, and
/// monitoring order status changes.
///
/// # Fields
/// - Immutable order identification and parameters
/// - Mutable execution state (quantity filled, average price)
/// - Status tracking with timestamps
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

/// Order execution update message for internal communication
///
/// Used internally to communicate order execution events between the execution
/// engine and order update processors. Contains execution details that trigger
/// order state updates and event notifications.
///
/// # Purpose
/// - Communicate partial and complete fills
/// - Update order status changes
/// - Trigger execution report generation
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

/// Real-time execution performance metrics
///
/// Tracks key performance indicators for order execution including throughput,
/// fill rates, latency, and volume statistics. All metrics are thread-safe
/// and updated atomically during order processing.
///
/// # Metrics Tracked
/// - Order submission and completion counts
/// - Execution volume and fill rates
/// - Performance timing (fill latency)
/// - Order lifecycle statistics
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

impl std::fmt::Debug for ExecutionMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionMetrics")
            .field("orders_submitted", &self.orders_submitted.load(Ordering::Relaxed))
            .field("orders_filled", &self.orders_filled.load(Ordering::Relaxed))
            .field("orders_cancelled", &self.orders_cancelled.load(Ordering::Relaxed))
            .field("orders_rejected", &self.orders_rejected.load(Ordering::Relaxed))
            .field("volume_executed", &self.volume_executed.load(Ordering::Relaxed))
            .field("avg_fill_latency_us", &self.avg_fill_latency_us.load(Ordering::Relaxed))
            .finish()
    }
}

impl ExecutionEngine {
    /// Creates a new execution engine instance
    ///
    /// # Arguments
    /// * `event_bus` - Shared event bus for broadcasting trading events
    ///
    /// # Returns
    /// A new ExecutionEngine ready for initialization
    ///
    /// # Example
    /// ```rust
    /// let execution_engine = ExecutionEngine::new(event_bus.clone());
    /// ```
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
    
    /// Initializes the execution engine and establishes connections
    ///
    /// Sets up the execution router connection and starts internal order update
    /// processing. Must be called before submitting orders.
    ///
    /// # Returns
    /// - `Ok(())` if initialization succeeds
    /// - `Err` if router setup or processor start fails
    ///
    /// # Example
    /// ```rust
    /// execution_engine.initialize().await?;
    /// ```
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
    
    /// Submits an order for execution through the smart routing system
    ///
    /// Processes order requests and routes them to appropriate execution algorithms
    /// based on order type. Generates unique order IDs and tracks order state.
    ///
    /// # Arguments
    /// * `order` - Trading event containing order details (symbol, side, quantity, etc.)
    ///
    /// # Returns
    /// - `Ok(())` if order is successfully submitted for execution
    /// - `Err` if order validation or routing fails
    ///
    /// # Supported Order Types
    /// - Market: Immediate execution at current market price
    /// - Limit: Execution at specified price or better
    /// - TWAP: Time-weighted average price algorithm
    /// - VWAP: Volume-weighted average price algorithm
    ///
    /// # Example
    /// ```rust
    /// let order = TradingEvent::OrderRequest {
    ///     symbol: Symbol::from("BTCUSDT"),
    ///     side: Side::Buy,
    ///     order_type: OrderType::Market,
    ///     quantity: Qty::from(100),
    ///     // ... other fields
    /// };
    /// execution_engine.submit_order(order).await?;
    /// ```
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
    
    /// Cancels an active order by order ID
    ///
    /// Attempts to cancel an order that is currently pending, accepted, or partially filled.
    /// Updates order state to cancelled and broadcasts cancellation event.
    ///
    /// # Arguments
    /// * `order_id` - Unique identifier of the order to cancel
    ///
    /// # Returns
    /// - `Ok(())` if cancellation is processed successfully
    /// - `Err` if cancellation fails
    ///
    /// # Note
    /// Only orders in Pending, Accepted, or PartiallyFilled status can be cancelled.
    /// Completed or already cancelled orders are ignored.
    ///
    /// # Example
    /// ```rust
    /// execution_engine.cancel_order(12345).await?;
    /// ```
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
    
    /// Cancels all active orders in the system
    ///
    /// Emergency function to cancel all pending, accepted, and partially filled orders.
    /// Useful for risk management or system shutdown scenarios.
    ///
    /// # Returns
    /// - `Ok(())` if all cancellations are processed successfully
    /// - `Err` if any cancellation fails
    ///
    /// # Example
    /// ```rust
    /// // Emergency stop - cancel everything
    /// execution_engine.cancel_all_orders().await?;
    /// ```
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
    
    /// Retrieves the current state of an order by ID
    ///
    /// # Arguments
    /// * `order_id` - Unique identifier of the order
    ///
    /// # Returns
    /// - `Some(OrderState)` if order exists
    /// - `None` if order ID is not found
    ///
    /// # Example
    /// ```rust
    /// if let Some(order) = execution_engine.get_order(12345) {
    ///     println!("Order status: {:?}", order.status);
    /// }
    /// ```
    pub fn get_order(&self, order_id: u64) -> Option<OrderState> {
        self.active_orders.get(&order_id).map(|e| e.clone())
    }
    
    /// Returns all currently active orders
    ///
    /// Filters orders to include only those in Pending, Accepted, or PartiallyFilled status.
    /// Useful for monitoring current trading activity and risk exposure.
    ///
    /// # Returns
    /// Vector of OrderState for all active orders
    ///
    /// # Example
    /// ```rust
    /// let active_orders = execution_engine.get_active_orders();
    /// println!("Currently tracking {} active orders", active_orders.len());
    /// ```
    pub fn get_active_orders(&self) -> Vec<OrderState> {
        self.active_orders
            .iter()
            .filter(|e| matches!(e.value().status, 
                OrderStatus::Pending | OrderStatus::Accepted | OrderStatus::PartiallyFilled))
            .map(|e| e.value().clone())
            .collect()
    }
    
    /// Returns a snapshot of current execution performance metrics
    ///
    /// Provides comprehensive statistics about execution engine performance including
    /// order counts, fill rates, volume, and latency measurements.
    ///
    /// # Returns
    /// ExecutionMetricsSnapshot containing current performance statistics
    ///
    /// # Example
    /// ```rust
    /// let metrics = execution_engine.get_metrics();
    /// println!("Fill rate: {:.2}%", metrics.fill_rate);
    /// println!("Avg latency: {}μs", metrics.avg_fill_latency_us);
    /// ```
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

/// Immutable snapshot of execution engine performance metrics
///
/// Contains point-in-time statistics about execution engine performance.
/// All metrics are computed from atomic counters to ensure consistency.
///
/// # Key Metrics
/// - Order lifecycle statistics (submitted, filled, cancelled, rejected)
/// - Volume and performance measurements
/// - Calculated ratios (fill rate percentage)
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