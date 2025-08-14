//! Execution Router Service
//!
//! Smart order routing and execution management:
//! - Route orders to optimal venues
//! - Handle order lifecycle (new, modify, cancel)
//! - Manage fills and partial fills
//! - Track execution quality
//! - Implement execution algorithms (TWAP, VWAP, Iceberg)

pub mod algorithms;
pub mod config;
pub mod memory;
pub mod router;
pub mod venue_manager;

use anyhow::Result;
use async_trait::async_trait;
use common::constants::{
    financial::PERCENT_SCALE,
    fixed_point::{BASIS_POINTS, SCALE_4},
    trading::{MAKER_FEE_BP, TAKER_FEE_BP},
};
use common::{Px, Qty, Side, Symbol, Ts};
use dashmap::DashMap;
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, error, info, warn};

/// Order ID wrapper for unique identification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrderId(pub u64);

impl OrderId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for OrderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Order types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    /// Market order - execute immediately at best price
    Market,
    /// Limit order - execute at specified price or better
    Limit,
    /// Stop order - trigger when price reaches stop level
    Stop,
    /// Stop limit order
    StopLimit,
    /// Iceberg order - show only part of total quantity
    Iceberg,
}

/// Time in force
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeInForce {
    /// Good till cancelled
    GTC,
    /// Immediate or cancel
    IOC,
    /// Fill or kill
    FOK,
    /// Good till date
    GTD,
    /// Day order
    DAY,
}

/// Order modification request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderModification {
    /// New price (optional)
    pub price: Option<Px>,
    /// New quantity (optional)
    pub quantity: Option<Qty>,
    /// New time in force (optional)
    pub time_in_force: Option<TimeInForce>,
}

/// Order status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    /// Order created but not sent
    Pending,
    /// Order sent to exchange
    Sent,
    /// Order acknowledged by exchange
    Acknowledged,
    /// Order partially filled
    PartiallyFilled,
    /// Order completely filled
    Filled,
    /// Order cancelled
    Cancelled,
    /// Order rejected by exchange
    Rejected,
    /// Order expired
    Expired,
}

/// Order request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    /// Client order ID
    pub client_order_id: String,
    /// Symbol
    pub symbol: Symbol,
    /// Buy or sell
    pub side: Side,
    /// Order quantity
    pub quantity: Qty,
    /// Order type
    pub order_type: OrderType,
    /// Limit price (for limit orders)
    pub limit_price: Option<Px>,
    /// Stop price (for stop orders)
    pub stop_price: Option<Px>,
    /// Time in force
    pub time_in_force: TimeInForce,
    /// Preferred venue (optional)
    pub venue: Option<String>,
    /// Strategy ID
    pub strategy_id: String,
    /// Additional parameters
    pub params: FxHashMap<String, String>,
}

/// Order state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    /// Internal order ID
    pub order_id: OrderId,
    /// Client order ID
    pub client_order_id: String,
    /// Exchange order ID
    pub exchange_order_id: Option<String>,
    /// Symbol
    pub symbol: Symbol,
    /// Side
    pub side: Side,
    /// Original quantity
    pub quantity: Qty,
    /// Filled quantity
    pub filled_quantity: Qty,
    /// Average fill price
    pub avg_fill_price: Px,
    /// Order status
    pub status: OrderStatus,
    /// Order type
    pub order_type: OrderType,
    /// Limit price
    pub limit_price: Option<Px>,
    /// Stop price
    pub stop_price: Option<Px>,
    /// Time in force
    pub time_in_force: TimeInForce,
    /// Venue routed to
    pub venue: String,
    /// Strategy ID
    pub strategy_id: String,
    /// Creation timestamp
    pub created_at: Ts,
    /// Last update timestamp
    pub updated_at: Ts,
    /// Fill events
    pub fills: Vec<Fill>,
}

/// Fill event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    /// Fill ID from exchange
    pub fill_id: String,
    /// Fill quantity
    pub quantity: Qty,
    /// Fill price
    pub price: Px,
    /// Fill timestamp
    pub timestamp: Ts,
    /// Liquidity flag (maker/taker)
    pub is_maker: bool,
    /// Commission
    pub commission: i64,
    /// Commission asset
    pub commission_asset: String,
}

/// Execution report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReport {
    /// Order ID
    pub order_id: OrderId,
    /// Client order ID
    pub client_order_id: String,
    /// Exchange order ID
    pub exchange_order_id: Option<String>,
    /// Report type
    pub report_type: ExecutionReportType,
    /// Order status
    pub status: OrderStatus,
    /// Filled quantity (cumulative)
    pub filled_qty: Qty,
    /// Last fill quantity
    pub last_qty: Option<Qty>,
    /// Last fill price
    pub last_price: Option<Px>,
    /// Average price
    pub avg_price: Option<Px>,
    /// Reject reason
    pub reject_reason: Option<String>,
    /// Report timestamp
    pub timestamp: Ts,
}

/// Execution report types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionReportType {
    /// New order acknowledged
    New,
    /// Order fill
    Fill,
    /// Order partially filled
    PartialFill,
    /// Order cancelled
    Cancelled,
    /// Order replaced/modified
    Replaced,
    /// Order rejected
    Rejected,
    /// Order expired
    Expired,
    /// Order status
    Status,
}

/// Venue selection strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VenueStrategy {
    /// Send to primary venue
    Primary,
    /// Smart order routing - best execution
    Smart,
    /// Split across venues
    Split,
    /// Route based on liquidity
    Liquidity,
    /// Route based on fees
    CostOptimal,
}

/// Execution router trait
#[async_trait]
pub trait ExecutionRouter: Send + Sync {
    /// Submit new order
    async fn submit_order(&mut self, request: OrderRequest) -> Result<OrderId>;

    /// Cancel order
    async fn cancel_order(&mut self, order_id: OrderId) -> Result<()>;

    /// Modify order
    async fn modify_order(
        &mut self,
        order_id: OrderId,
        new_qty: Option<Qty>,
        new_price: Option<Px>,
    ) -> Result<()>;

    /// Get order status
    async fn get_order(&self, order_id: OrderId) -> Option<Order>;

    /// Get all orders
    async fn get_all_orders(&self) -> Vec<Order>;

    /// Get orders by status
    async fn get_orders_by_status(&self, status: OrderStatus) -> Vec<Order>;

    /// Process execution report from exchange
    async fn process_execution_report(&mut self, report: ExecutionReport) -> Result<()>;

    /// Get execution metrics
    async fn get_metrics(&self) -> ExecutionMetrics;
}

/// Execution metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    /// Total orders sent
    pub total_orders: u64,
    /// Filled orders
    pub filled_orders: u64,
    /// Cancelled orders
    pub cancelled_orders: u64,
    /// Rejected orders
    pub rejected_orders: u64,
    /// Average fill time (ms)
    pub avg_fill_time_ms: u64,
    /// Total volume traded
    pub total_volume: u64,
    /// Total commission paid
    pub total_commission: i64,
    /// Fill rate percentage (fixed-point: SCALE_4 = 100%)
    pub fill_rate: i32,
    /// Venues used
    pub venues_used: FxHashMap<String, u64>,
}

/// Execution router service
pub struct ExecutionRouterService {
    /// Order ID generator
    next_order_id: AtomicU64,
    /// Active orders
    orders: Arc<DashMap<OrderId, Arc<RwLock<Order>>>>,
    /// Client order ID to internal ID mapping
    client_order_map: Arc<DashMap<String, OrderId>>,
    /// Exchange order ID to internal ID mapping
    exchange_order_map: Arc<DashMap<String, OrderId>>,
    /// Venue strategy
    venue_strategy: VenueStrategy,
    /// Execution metrics
    metrics: Arc<RwLock<ExecutionMetrics>>,
    /// Risk manager reference
    risk_manager: Option<Arc<dyn risk_manager::RiskManager>>,
}

impl ExecutionRouterService {
    /// Create new execution router
    pub fn new(venue_strategy: VenueStrategy) -> Self {
        Self {
            next_order_id: AtomicU64::new(1),
            orders: Arc::new(DashMap::new()),
            client_order_map: Arc::new(DashMap::new()),
            exchange_order_map: Arc::new(DashMap::new()),
            venue_strategy,
            metrics: Arc::new(RwLock::new(ExecutionMetrics::default())),
            risk_manager: None,
        }
    }

    /// Set risk manager
    pub fn set_risk_manager(&mut self, risk_manager: Arc<dyn risk_manager::RiskManager>) {
        self.risk_manager = Some(risk_manager);
    }

    /// Generate next order ID
    fn next_order_id(&self) -> OrderId {
        OrderId::new(self.next_order_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Select venue for order
    fn select_venue(&self, _request: &OrderRequest) -> String {
        // Simplified venue selection
        match self.venue_strategy {
            VenueStrategy::Primary => "binance".to_string(),
            VenueStrategy::Smart => {
                // Advanced smart routing with liquidity-based venue selection
                self.select_optimal_venue(_request)
            }
            _ => "binance".to_string(),
        }
    }

    /// Advanced smart routing algorithm with liquidity analysis
    fn select_optimal_venue(&self, request: &OrderRequest) -> String {
        let quantity_value = request.quantity.as_f64();

        // High-volume threshold for institutional routing
        const INSTITUTIONAL_THRESHOLD: f64 = 50000.0;
        // Medium-volume threshold for cross-venue routing
        const CROSS_VENUE_THRESHOLD: f64 = SCALE_4 as f64;

        let symbol_str = format!("{}", request.symbol); // Convert Symbol to string
        match symbol_str.as_str() {
            // Major crypto pairs: route based on liquidity and size
            symbol if symbol.ends_with("USDT") || symbol.ends_with("BUSD") => {
                if quantity_value > INSTITUTIONAL_THRESHOLD {
                    // Large institutional orders: prefer deep liquidity venues
                    "binance_institutional".to_string()
                } else if quantity_value > CROSS_VENUE_THRESHOLD {
                    // Medium orders: balance between cost and speed
                    "binance_spot".to_string()
                } else {
                    // Small retail orders: optimize for speed and low fees
                    "binance".to_string()
                }
            }
            // Futures contracts: route to appropriate futures venues
            symbol if symbol.ends_with("PERP") => "binance_futures".to_string(),
            // Default routing for other instruments
            _ => {
                if quantity_value > CROSS_VENUE_THRESHOLD {
                    "binance_spot".to_string()
                } else {
                    "binance".to_string()
                }
            }
        }
    }

    /// Send order to exchange connector
    async fn send_order_to_exchange(&self, order: &Order) -> Result<()> {
        // Route to appropriate exchange connector based on venue
        match order.venue.as_str() {
            "binance" | "binance_spot" | "binance_futures" | "binance_institutional" => {
                self.send_to_binance_connector(order).await
            }
            "zerodha" => self.send_to_zerodha_connector(order).await,
            _ => Err(anyhow::anyhow!("Unsupported venue: {}", order.venue)),
        }
    }

    /// Send order to Binance connector
    async fn send_to_binance_connector(&self, order: &Order) -> Result<()> {
        // In a real implementation, this would send via gRPC to the Binance connector service
        // For now, simulate successful submission
        tracing::info!(
            "Simulating Binance order submission - Order ID: {}, Symbol: {}, Side: {:?}, Quantity: {}, Price: {:?}",
            order.order_id.0,
            order.symbol,
            order.side,
            order.quantity,
            order.limit_price
        );

        // Simulate network delay
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // In production: send gRPC request to market-connector service
        // let request = ExecuteOrderRequest { ... };
        // self.market_connector_client.execute_order(request).await?;

        Ok(())
    }

    /// Send order to Zerodha connector  
    async fn send_to_zerodha_connector(&self, order: &Order) -> Result<()> {
        // In a real implementation, this would send via gRPC to the Zerodha connector service
        tracing::info!(
            "Simulating Zerodha order submission - Order ID: {}, Symbol: {}, Side: {:?}, Quantity: {}",
            order.order_id.0,
            order.symbol,
            order.side,
            order.quantity
        );

        // Simulate network delay
        tokio::time::sleep(tokio::time::Duration::from_millis(15)).await;

        Ok(())
    }

    /// Send cancel request to exchange connector
    async fn send_cancel_to_exchange(&self, order: &Order) -> Result<()> {
        match order.venue.as_str() {
            "binance" | "binance_spot" | "binance_futures" | "binance_institutional" => {
                tracing::info!(
                    "Simulating Binance cancel request - Order ID: {}, Exchange Order ID: {:?}",
                    order.order_id.0,
                    order.exchange_order_id
                );
                // Simulate network delay
                tokio::time::sleep(tokio::time::Duration::from_millis(8)).await;
                Ok(())
            }
            "zerodha" => {
                tracing::info!(
                    "Simulating Zerodha cancel request - Order ID: {}, Exchange Order ID: {:?}",
                    order.order_id.0,
                    order.exchange_order_id
                );
                // Simulate network delay
                tokio::time::sleep(tokio::time::Duration::from_millis(12)).await;
                Ok(())
            }
            _ => Err(anyhow::anyhow!(
                "Cancel not supported for venue: {}",
                order.venue
            )),
        }
    }

    /// Send modify request to exchange connector
    async fn send_modify_to_exchange(
        &self,
        order: &Order,
        modifications: &OrderModification,
    ) -> Result<()> {
        match order.venue.as_str() {
            "binance" | "binance_spot" | "binance_futures" | "binance_institutional" => {
                tracing::info!(
                    "Simulating Binance modify request - Order ID: {}, New Price: {:?}, New Quantity: {:?}",
                    order.order_id.0,
                    modifications.price,
                    modifications.quantity
                );
                // Simulate network delay
                tokio::time::sleep(tokio::time::Duration::from_millis(12)).await;
                Ok(())
            }
            "zerodha" => {
                tracing::info!(
                    "Simulating Zerodha modify request - Order ID: {}, New Price: {:?}, New Quantity: {:?}",
                    order.order_id.0,
                    modifications.price,
                    modifications.quantity
                );
                // Simulate network delay
                tokio::time::sleep(tokio::time::Duration::from_millis(18)).await;
                Ok(())
            }
            _ => Err(anyhow::anyhow!(
                "Modify not supported for venue: {}",
                order.venue
            )),
        }
    }

    /// Determine if the fill was maker or taker based on execution report
    fn determine_maker_status(&self, _report: &ExecutionReport) -> bool {
        // In a real implementation, this would parse the execution report for maker/taker info
        // For now, implement simple heuristic:
        // - Limit orders that sit on the book are typically makers (aggressive pricing)
        // - Market orders are always takers
        // - For this simulation, we'll use a simple probability
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        _report.order_id.hash(&mut hasher);
        let hash = hasher.finish();

        // 60% chance of being a maker (institutional trading typically provides liquidity)
        (hash % PERCENT_SCALE as u64) < 60
    }

    /// Calculate commission based on execution report and fill details
    fn calculate_commission(&self, _report: &ExecutionReport, quantity: Qty, price: Px) -> i64 {
        // Commission calculation based on trade value using fixed-point arithmetic
        // Trade value = (quantity * price) / SCALE_4
        // Both quantity and price are already in fixed-point with SCALE_4
        let trade_value_fixed = (quantity.as_i64() * price.as_i64()) / SCALE_4;

        // Standard maker/taker fee structure (in basis points)
        let fee_bp = if self.determine_maker_status(_report) {
            MAKER_FEE_BP // Maker fee (rebate in real scenario)
        } else {
            TAKER_FEE_BP // Taker fee
        };

        // Calculate commission: (trade_value * fee_bp) / BASIS_POINTS
        // Result is in fixed-point format
        (trade_value_fixed * fee_bp) / BASIS_POINTS
    }

    /// Update metrics
    fn update_metrics(&self, order: &Order) {
        let mut metrics = self.metrics.write();

        match order.status {
            OrderStatus::Filled => {
                metrics.filled_orders += 1;
                metrics.total_volume += order.filled_quantity.as_i64().unsigned_abs();
            }
            OrderStatus::Cancelled => metrics.cancelled_orders += 1,
            OrderStatus::Rejected => metrics.rejected_orders += 1,
            _ => {}
        }

        *metrics.venues_used.entry(order.venue.clone()).or_insert(0) += 1;

        // Calculate fill rate
        if metrics.total_orders > 0 {
            let percentage =
                (metrics.filled_orders.saturating_mul(SCALE_4 as u64)) / metrics.total_orders;
            // SAFETY: Explicit bounds check ensures percentage fits in i32
            // Cast is safe because we explicitly check bounds
            metrics.fill_rate = if percentage <= i32::MAX as u64 {
                percentage as i32
            } else {
                i32::MAX // Cap at maximum
            };
        }
    }
}

#[async_trait]
impl ExecutionRouter for ExecutionRouterService {
    async fn submit_order(&mut self, request: OrderRequest) -> Result<OrderId> {
        // Check risk limits if risk manager is set
        if let Some(ref risk_manager) = self.risk_manager {
            let price = request.limit_price.unwrap_or(Px::ZERO);
            let check = risk_manager
                .check_order(request.symbol, request.side, request.quantity, price)
                .await;

            match check {
                risk_manager::RiskCheckResult::Rejected(reason) => {
                    error!("Order rejected by risk: {}", reason);
                    return Err(anyhow::anyhow!("Risk check failed: {}", reason));
                }
                risk_manager::RiskCheckResult::RequiresApproval(reason) => {
                    warn!("Order requires approval: {}", reason);
                }
                _ => {}
            }
        }

        // Generate order ID
        let order_id = self.next_order_id();

        // Select venue - avoid clone by using as_ref()
        let venue = request
            .venue
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_else(|| self.select_venue(&request));

        // Create order
        let order = Order {
            order_id,
            client_order_id: request.client_order_id.clone(),
            exchange_order_id: None,
            symbol: request.symbol,
            side: request.side,
            quantity: request.quantity,
            filled_quantity: Qty::ZERO,
            avg_fill_price: Px::ZERO,
            status: OrderStatus::Pending,
            order_type: request.order_type,
            limit_price: request.limit_price,
            stop_price: request.stop_price,
            time_in_force: request.time_in_force,
            venue,
            strategy_id: request.strategy_id,
            created_at: Ts::now(),
            updated_at: Ts::now(),
            fills: Vec::new(),
        };

        // Store order - create Arc once to avoid clone
        let order_arc = Arc::new(RwLock::new(order));
        self.orders.insert(order_id, Arc::clone(&order_arc));
        self.client_order_map
            .insert(request.client_order_id, order_id);

        // Update metrics
        self.metrics.write().total_orders += 1;

        // Get venue for logging (drop lock before await)
        let venue = {
            let order_ref = order_arc.read();
            info!("Order {} submitted to {}", order_id, order_ref.venue);
            order_ref.venue.clone()
        };

        // Send order to appropriate exchange connector
        // Clone the order for sending (avoiding lock across await)
        let order_for_send = order_arc.read().clone();
        match self.send_order_to_exchange(&order_for_send).await {
            Ok(_) => {
                info!("Order {} successfully sent to exchange {}", order_id, venue);
                Ok(order_id)
            }
            Err(e) => {
                error!(
                    "Failed to send order {} to exchange {}: {}",
                    order_id, venue, e
                );
                // Update order status to failed
                if let Some(stored_order) = self.orders.get(&order_id) {
                    stored_order.write().status = OrderStatus::Rejected;
                }
                Err(anyhow::anyhow!("Exchange submission failed: {}", e))
            }
        }
    }

    async fn cancel_order(&mut self, order_id: OrderId) -> Result<()> {
        if let Some(order_ref) = self.orders.get(&order_id) {
            // Clone the order data we need before the await
            let order_data = {
                let mut order = order_ref.write();

                if order.status == OrderStatus::Filled {
                    return Err(anyhow::anyhow!("Cannot cancel filled order"));
                }

                order.status = OrderStatus::Cancelled;
                order.updated_at = Ts::now();

                self.update_metrics(&order);

                info!("Order {} cancelled", order_id);

                // Clone the order to use after dropping the lock
                order.clone()
            }; // Lock is dropped here

            // Send cancel request to exchange (no lock held)
            if let Err(e) = self.send_cancel_to_exchange(&order_data).await {
                error!(
                    "Failed to send cancel to exchange for order {}: {}",
                    order_id, e
                );
                // Note: Order is still marked as cancelled locally even if exchange cancel fails
                // This prevents further modifications and ensures consistent local state
            }

            Ok(())
        } else {
            Err(anyhow::anyhow!("Order not found"))
        }
    }

    async fn modify_order(
        &mut self,
        order_id: OrderId,
        new_qty: Option<Qty>,
        new_price: Option<Px>,
    ) -> Result<()> {
        if let Some(order_ref) = self.orders.get(&order_id) {
            // Clone the order data we need before the await
            let order_data = {
                let mut order = order_ref.write();

                if order.status != OrderStatus::Acknowledged
                    && order.status != OrderStatus::PartiallyFilled
                {
                    return Err(anyhow::anyhow!("Order cannot be modified in current state"));
                }

                if let Some(qty) = new_qty {
                    order.quantity = qty;
                }

                if let Some(price) = new_price {
                    order.limit_price = Some(price);
                }

                order.updated_at = Ts::now();

                info!("Order {} modified", order_id);

                // Clone the order to use after dropping the lock
                order.clone()
            }; // Lock is dropped here

            // Send modify request to exchange (no lock held)
            let modification = OrderModification {
                price: new_price,
                quantity: new_qty,
                time_in_force: None,
            };
            if let Err(e) = self
                .send_modify_to_exchange(&order_data, &modification)
                .await
            {
                error!(
                    "Failed to send modify to exchange for order {}: {}",
                    order_id, e
                );
                // Note: Local order is still modified even if exchange modify fails
                // This ensures consistency with the requested modification
            }

            Ok(())
        } else {
            Err(anyhow::anyhow!("Order not found"))
        }
    }

    async fn get_order(&self, order_id: OrderId) -> Option<Order> {
        self.orders.get(&order_id).map(|o| o.read().clone())
    }

    async fn get_all_orders(&self) -> Vec<Order> {
        self.orders
            .iter()
            .map(|entry| entry.value().read().clone())
            .collect()
    }

    async fn get_orders_by_status(&self, status: OrderStatus) -> Vec<Order> {
        self.orders
            .iter()
            .map(|entry| entry.value().read().clone())
            .filter(|o| o.status == status)
            .collect()
    }

    async fn process_execution_report(&mut self, report: ExecutionReport) -> Result<()> {
        let order_id = if let Some(id) = self
            .exchange_order_map
            .get(&report.exchange_order_id.clone().unwrap_or_default())
        {
            *id
        } else if let Some(id) = self.client_order_map.get(&report.client_order_id) {
            *id
        } else {
            return Err(anyhow::anyhow!("Order not found for execution report"));
        };

        if let Some(order_ref) = self.orders.get(&order_id) {
            let mut order = order_ref.write();

            // Update order based on report
            order.status = report.status;
            order.updated_at = report.timestamp;

            if let Some(ref exchange_id) = report.exchange_order_id {
                order.exchange_order_id = Some(exchange_id.clone());
                self.exchange_order_map
                    .insert(exchange_id.clone(), order_id);
            }

            // Handle fills
            if let (Some(last_qty), Some(last_price)) = (report.last_qty, report.last_price) {
                let fill = Fill {
                    fill_id: format!("{}_{}", order_id, order.fills.len()),
                    quantity: last_qty,
                    price: last_price,
                    timestamp: report.timestamp,
                    is_maker: self.determine_maker_status(&report),
                    commission: self.calculate_commission(&report, last_qty, last_price),
                    commission_asset: "USDT".to_string(),
                };

                order.fills.push(fill);
                order.filled_quantity =
                    Qty::from_i64(order.filled_quantity.as_i64() + last_qty.as_i64());

                // Update average price
                if order.filled_quantity > Qty::ZERO {
                    let total_value = order
                        .fills
                        .iter()
                        .map(|f| f.price.as_i64() * f.quantity.as_i64())
                        .sum::<i64>();
                    order.avg_fill_price =
                        Px::from_i64(total_value / order.filled_quantity.as_i64());
                }
            }

            self.update_metrics(&order);

            debug!("Processed execution report for order {}", order_id);
        }

        Ok(())
    }

    async fn get_metrics(&self) -> ExecutionMetrics {
        self.metrics.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_order_submission() {
        let mut router = ExecutionRouterService::new(VenueStrategy::Primary);

        let request = OrderRequest {
            client_order_id: "test_001".to_string(),
            symbol: Symbol::new(1),
            side: Side::Bid,
            quantity: Qty::from_qty_i32(100_0000),
            order_type: OrderType::Limit,
            limit_price: Some(Px::from_price_i32(100_0000)),
            stop_price: None,
            time_in_force: TimeInForce::GTC,
            venue: None,
            strategy_id: "test_strategy".to_string(),
            params: FxHashMap::default(),
        };

        let order_id = router.submit_order(request).await.unwrap();
        assert!(order_id.as_u64() > 0);

        let order = router.get_order(order_id).await.unwrap();
        assert_eq!(order.status, OrderStatus::Pending);
    }

    #[tokio::test]
    async fn test_order_cancellation() {
        let mut router = ExecutionRouterService::new(VenueStrategy::Primary);

        let request = OrderRequest {
            client_order_id: "test_002".to_string(),
            symbol: Symbol::new(1),
            side: Side::Ask,
            quantity: Qty::from_qty_i32(50_0000),
            order_type: OrderType::Limit,
            limit_price: Some(Px::from_price_i32(101_0000)),
            stop_price: None,
            time_in_force: TimeInForce::DAY,
            venue: None,
            strategy_id: "test_strategy".to_string(),
            params: FxHashMap::default(),
        };

        let order_id = router.submit_order(request).await.unwrap();
        router.cancel_order(order_id).await.unwrap();

        let order = router.get_order(order_id).await.unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);
    }
}
