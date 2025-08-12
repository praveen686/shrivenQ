//! Ultra-fast execution layer with mode switching

use crate::core::{EngineConfig, ExecutionMode};
use crate::memory::ObjectPool;
use crate::venue::VenueAdapter;
use common::{Px, Qty, Side, Symbol, Ts};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

/// Order structure - cache-aligned POD
#[repr(C, align(64))]
pub struct Order {
    pub id: u64,
    pub symbol: Symbol,
    pub side: u8,         // 0=Buy, 1=Sell
    pub order_type: u8,   // 0=Market, 1=Limit
    pub status: AtomicU8, // 0=New, 1=PartialFill, 2=Filled, 3=Cancelled, 4=Rejected
    pub quantity: Qty,
    pub filled_qty: AtomicU64,
    pub price: u64, // 0 for market orders
    pub timestamp: u64,
    pub venue_id: u64, // Exchange order ID
    _padding: [u8; 16],
}

impl Order {
    pub fn new(id: u64, symbol: Symbol, side: u8, qty: Qty, price: Option<Px>) -> Self {
        Self {
            id,
            symbol,
            side,
            order_type: if price.is_some() { 1 } else { 0 },
            status: AtomicU8::new(0),
            quantity: qty,
            filled_qty: AtomicU64::new(0),
            price: price.map(|p| (p.as_f64() * 100.0) as u64).unwrap_or(0),
            timestamp: Ts::now().nanos(),
            venue_id: 0,
            _padding: [0; 16],
        }
    }
}

impl Default for Order {
    fn default() -> Self {
        Self {
            id: 0,
            symbol: Symbol(0),
            side: 0,
            order_type: 0,
            status: AtomicU8::new(0),
            quantity: Qty::ZERO,
            filled_qty: AtomicU64::new(0),
            price: 0,
            timestamp: 0,
            venue_id: 0,
            _padding: [0; 16],
        }
    }
}

/// Order pool for zero-allocation order management
pub struct OrderPool {
    pool: ObjectPool<Order>,
}

impl OrderPool {
    pub fn new(capacity: usize) -> Self {
        Self {
            pool: ObjectPool::new(capacity),
        }
    }

    #[inline(always)]
    pub fn acquire(&self) -> Option<&mut Order> {
        self.pool.acquire()
    }

    #[inline(always)]
    pub fn release(&self, order: &mut Order) {
        self.pool.release(order);
    }
}

/// Execution layer - handles order routing based on mode
pub struct ExecutionLayer<V: VenueAdapter> {
    config: Arc<EngineConfig>,
    venue: V,
    order_pool: OrderPool,

    // Paper trading state
    paper_fills: dashmap::DashMap<u64, Vec<Fill>>,
    paper_order_counter: AtomicU64,

    // Backtest state
    backtest_time: AtomicU64,
    backtest_fills: dashmap::DashMap<u64, Vec<Fill>>,
}

impl<V: VenueAdapter> ExecutionLayer<V> {
    pub fn new(config: Arc<EngineConfig>, venue: V) -> Self {
        Self {
            config: config.clone(),
            venue,
            order_pool: OrderPool::new(config.max_positions * 10),
            paper_fills: dashmap::DashMap::new(),
            paper_order_counter: AtomicU64::new(1000000), // Start at 1M for paper orders
            backtest_time: AtomicU64::new(0),
            backtest_fills: dashmap::DashMap::new(),
        }
    }

    /// Simulate order execution (paper trading)
    #[inline(always)]
    pub fn simulate_order(
        &self,
        order_id: u64,
        symbol: Symbol,
        side: Side,
        qty: Qty,
        price: Option<Px>,
    ) -> Result<(), u8> {
        // Get order from pool
        let order = match self.order_pool.acquire() {
            Some(order) => order,
            None => {
                tracing::error!("Order pool exhausted");
                return Err(2);
            }
        };

        // Initialize order
        *order = Order::new(
            order_id,
            symbol,
            if side == Side::Bid { 0 } else { 1 },
            qty,
            price,
        );
        order.status.store(1, Ordering::Release); // Immediately accepted in paper mode

        // Get current market price (would come from market data feed)
        let fill_price = price.unwrap_or(Px::new(100.0)); // Mock price

        // Create immediate fill
        let fill = Fill {
            order_id,
            symbol,
            side: if side == Side::Bid { 0 } else { 1 },
            quantity: qty,
            price: fill_price,
            timestamp: Ts::now(),
            venue_id: self.paper_order_counter.fetch_add(1, Ordering::Relaxed),
        };

        // Update order as filled
        order.status.store(2, Ordering::Release); // Filled
        order.filled_qty.store(qty.raw() as u64, Ordering::Release);

        // Store fill
        self.paper_fills
            .entry(order_id)
            .or_insert_with(Vec::new)
            .push(fill);

        // Return order to pool
        self.order_pool.release(order);

        Ok(())
    }

    /// Send live order to venue
    #[inline(always)]
    pub fn send_live_order(
        &self,
        order_id: u64,
        symbol: Symbol,
        side: Side,
        qty: Qty,
        price: Option<Px>,
    ) -> Result<(), u8> {
        // Get order from pool for zero-allocation
        let order = match self.order_pool.acquire() {
            Some(order) => order,
            None => {
                tracing::error!("Order pool exhausted");
                return Err(2);
            }
        };

        // Initialize order
        *order = Order::new(
            order_id,
            symbol,
            if side == Side::Bid { 0 } else { 1 },
            qty,
            price,
        );

        // Route to venue adapter
        match self.venue.send_order(symbol, side, qty, price) {
            Ok(venue_order_id) => {
                // Update order with venue ID
                order.venue_id = venue_order_id;
                order.status.store(1, Ordering::Release); // Accepted

                // Store venue order ID mapping
                tracing::debug!("Order {} sent to venue as {}", order_id, venue_order_id);

                // Return order to pool when done
                self.order_pool.release(order);
                Ok(())
            }
            Err(e) => {
                tracing::error!("Failed to send order: {}", e);
                order.status.store(4, Ordering::Release); // Rejected
                self.order_pool.release(order);
                Err(1)
            }
        }
    }

    /// Replay order for backtesting
    #[inline(always)]
    pub fn replay_order(
        &self,
        order_id: u64,
        symbol: Symbol,
        side: Side,
        qty: Qty,
        price: Option<Px>,
    ) -> Result<(), u8> {
        // Get order from pool for zero-allocation
        let order = match self.order_pool.acquire() {
            Some(order) => order,
            None => {
                tracing::error!("Order pool exhausted");
                return Err(2);
            }
        };

        // Initialize order
        *order = Order::new(
            order_id,
            symbol,
            if side == Side::Bid { 0 } else { 1 },
            qty,
            price,
        );

        // In backtest, check historical data for fill
        // This would integrate with historical data replay
        let current_time = self.backtest_time.load(Ordering::Acquire);

        // Update order status for backtest
        order.status.store(2, Ordering::Release); // Immediately filled in backtest
        order.filled_qty.store(qty.raw() as u64, Ordering::Release);

        // Mock fill based on historical data
        let fill = Fill {
            order_id,
            symbol,
            side: if side == Side::Bid { 0 } else { 1 },
            quantity: qty,
            price: price.unwrap_or(Px::new(100.0)),
            timestamp: Ts::from_nanos(current_time),
            venue_id: 0,
        };

        self.backtest_fills
            .entry(order_id)
            .or_insert_with(Vec::new)
            .push(fill);

        // Return order to pool
        self.order_pool.release(order);

        Ok(())
    }

    /// Get fills for an order
    pub fn get_fills(&self, order_id: u64) -> Vec<Fill> {
        match self.config.mode {
            ExecutionMode::Paper => self
                .paper_fills
                .get(&order_id)
                .map(|v| v.clone())
                .unwrap_or_default(),
            ExecutionMode::Live => {
                // Would query venue for fills
                vec![]
            }
            ExecutionMode::Backtest => self
                .backtest_fills
                .get(&order_id)
                .map(|v| v.clone())
                .unwrap_or_default(),
        }
    }

    /// Advance backtest time
    pub fn advance_backtest_time(&self, ts: Ts) {
        self.backtest_time.store(ts.nanos(), Ordering::Release);
    }
}

/// Fill information
#[repr(C, align(64))]
#[derive(Clone)]
pub struct Fill {
    pub order_id: u64,
    pub symbol: Symbol,
    pub side: u8,
    pub quantity: Qty,
    pub price: Px,
    pub timestamp: Ts,
    pub venue_id: u64,
}

/// Execution report
#[repr(C)]
pub struct ExecutionReport {
    pub order_id: u64,
    pub status: OrderStatus,
    pub filled_qty: Qty,
    pub avg_price: Px,
    pub timestamp: Ts,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum OrderType {
    Market = 0,
    Limit = 1,
    StopLoss = 2,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum OrderStatus {
    New = 0,
    PartiallyFilled = 1,
    Filled = 2,
    Cancelled = 3,
    Rejected = 4,
}
