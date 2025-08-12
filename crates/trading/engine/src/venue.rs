//! Venue adapters - Zero-cost abstraction for different exchanges

use chrono::{Local, Timelike};
use common::{Px, Qty, Side, Symbol};
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Order status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    Accepted,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

/// Order information
#[derive(Debug, Clone)]
pub struct OrderInfo {
    pub order_id: u64,
    pub symbol: Symbol,
    pub side: Side,
    pub qty: Qty,
    pub price: Option<Px>,
    pub status: OrderStatus,
    pub filled_qty: Qty,
    pub avg_fill_price: Px,
}

/// Venue adapter trait - compile-time polymorphism preferred
pub trait VenueAdapter: Send + Sync + 'static {
    /// Send order to venue
    fn send_order(
        &self,
        symbol: Symbol,
        side: Side,
        qty: Qty,
        price: Option<Px>,
    ) -> Result<u64, String>;

    /// Cancel order
    fn cancel_order(&self, order_id: u64) -> Result<(), String>;

    /// Get order status
    fn get_order_status(&self, order_id: u64) -> Option<OrderStatus>;

    /// Get venue-specific symbol mapping
    fn map_symbol(&self, symbol: Symbol) -> u32;

    /// Check if market is open
    fn is_market_open(&self) -> bool;

    /// Get venue latency estimate (nanoseconds)
    fn get_latency_ns(&self) -> u64;
}

/// Zerodha adapter - NSE/BSE markets
#[repr(C, align(64))]
pub struct ZerodhaAdapter {
    auth: Arc<auth::ZerodhaAuth>,
    symbol_map: Arc<DashMap<Symbol, u32>>,

    // Order tracking
    orders: Arc<DashMap<u64, OrderInfo>>,
    order_id_counter: Arc<AtomicU64>,

    // Cached market timings (IST)
    market_open_hour: u8,  // 9
    market_open_min: u8,   // 15
    market_close_hour: u8, // 15
    market_close_min: u8,  // 30

    // Performance tracking
    avg_latency_ns: AtomicU64,

    _padding: [u8; 24],
}

impl Clone for ZerodhaAdapter {
    fn clone(&self) -> Self {
        Self {
            auth: self.auth.clone(),
            symbol_map: self.symbol_map.clone(),
            orders: self.orders.clone(),
            order_id_counter: self.order_id_counter.clone(),
            market_open_hour: self.market_open_hour,
            market_open_min: self.market_open_min,
            market_close_hour: self.market_close_hour,
            market_close_min: self.market_close_min,
            avg_latency_ns: AtomicU64::new(self.avg_latency_ns.load(Ordering::Relaxed)),
            _padding: [0; 24],
        }
    }
}

impl VenueAdapter for ZerodhaAdapter {
    #[inline(always)]
    fn send_order(
        &self,
        symbol: Symbol,
        side: Side,
        qty: Qty,
        price: Option<Px>,
    ) -> Result<u64, String> {
        // Check market hours
        if !self.is_market_open() {
            return Err("Market is closed".to_string());
        }

        // Generate order ID
        let order_id = self.order_id_counter.fetch_add(1, Ordering::SeqCst);

        // Create order info
        let order_info = OrderInfo {
            order_id,
            symbol,
            side,
            qty,
            price,
            status: OrderStatus::Pending,
            filled_qty: Qty::ZERO,
            avg_fill_price: Px::ZERO,
        };

        // Store order
        self.orders.insert(order_id, order_info);

        // In live mode, would send to Zerodha API here
        // For now, simulate acceptance
        if let Some(mut order) = self.orders.get_mut(&order_id) {
            order.status = OrderStatus::Accepted;
        }

        Ok(order_id)
    }

    #[inline(always)]
    fn cancel_order(&self, order_id: u64) -> Result<(), String> {
        match self.orders.get_mut(&order_id) {
            Some(mut order) => {
                if order.status == OrderStatus::Pending || order.status == OrderStatus::Accepted {
                    order.status = OrderStatus::Cancelled;
                    Ok(())
                } else {
                    Err(format!("Cannot cancel order in status {:?}", order.status))
                }
            }
            None => Err(format!("Order {} not found", order_id)),
        }
    }

    #[inline(always)]
    fn get_order_status(&self, order_id: u64) -> Option<OrderStatus> {
        self.orders.get(&order_id).map(|order| order.status)
    }

    #[inline(always)]
    fn map_symbol(&self, symbol: Symbol) -> u32 {
        self.symbol_map
            .entry(symbol)
            .or_insert(symbol.0)
            .value()
            .clone()
    }

    #[inline(always)]
    fn is_market_open(&self) -> bool {
        let now = Local::now();
        let hour = now.hour() as u8;
        let min = now.minute() as u8;

        // Branch-free comparison
        let open_mins = self.market_open_hour as u16 * 60 + self.market_open_min as u16;
        let close_mins = self.market_close_hour as u16 * 60 + self.market_close_min as u16;
        let current_mins = hour as u16 * 60 + min as u16;

        (current_mins >= open_mins) & (current_mins <= close_mins)
    }

    #[inline(always)]
    fn get_latency_ns(&self) -> u64 {
        self.avg_latency_ns.load(Ordering::Relaxed)
    }
}

/// Binance adapter - Crypto markets
#[repr(C, align(64))]
pub struct BinanceAdapter {
    auth: Arc<auth::BinanceAuth>,
    symbol_map: Arc<DashMap<Symbol, String>>,

    // Order tracking
    orders: Arc<DashMap<u64, OrderInfo>>,
    order_id_counter: Arc<AtomicU64>,

    testnet: bool,

    // Performance tracking
    avg_latency_ns: AtomicU64,

    _padding: [u8; 30],
}

impl Clone for BinanceAdapter {
    fn clone(&self) -> Self {
        Self {
            auth: self.auth.clone(),
            symbol_map: self.symbol_map.clone(),
            orders: self.orders.clone(),
            order_id_counter: self.order_id_counter.clone(),
            testnet: self.testnet,
            avg_latency_ns: AtomicU64::new(self.avg_latency_ns.load(Ordering::Relaxed)),
            _padding: [0; 30],
        }
    }
}

impl VenueAdapter for BinanceAdapter {
    #[inline(always)]
    fn send_order(
        &self,
        symbol: Symbol,
        side: Side,
        qty: Qty,
        price: Option<Px>,
    ) -> Result<u64, String> {
        // Generate order ID
        let order_id = self.order_id_counter.fetch_add(1, Ordering::SeqCst);

        // Create order info
        let order_info = OrderInfo {
            order_id,
            symbol,
            side,
            qty,
            price,
            status: OrderStatus::Pending,
            filled_qty: Qty::ZERO,
            avg_fill_price: Px::ZERO,
        };

        // Store order
        self.orders.insert(order_id, order_info);

        // In live mode, would send to Binance API here
        // For now, simulate acceptance
        if let Some(mut order) = self.orders.get_mut(&order_id) {
            order.status = OrderStatus::Accepted;
        }

        Ok(order_id)
    }

    #[inline(always)]
    fn cancel_order(&self, order_id: u64) -> Result<(), String> {
        match self.orders.get_mut(&order_id) {
            Some(mut order) => {
                if order.status == OrderStatus::Pending || order.status == OrderStatus::Accepted {
                    order.status = OrderStatus::Cancelled;
                    Ok(())
                } else {
                    Err(format!("Cannot cancel order in status {:?}", order.status))
                }
            }
            None => Err(format!("Order {} not found", order_id)),
        }
    }

    #[inline(always)]
    fn get_order_status(&self, order_id: u64) -> Option<OrderStatus> {
        self.orders.get(&order_id).map(|order| order.status)
    }

    #[inline(always)]
    fn map_symbol(&self, symbol: Symbol) -> u32 {
        // For Binance, we use string symbols internally
        symbol.0
    }

    #[inline(always)]
    fn is_market_open(&self) -> bool {
        true // Crypto markets are 24/7
    }

    #[inline(always)]
    fn get_latency_ns(&self) -> u64 {
        self.avg_latency_ns.load(Ordering::Relaxed)
    }
}

/// Venue configuration
#[derive(Clone)]
pub struct VenueConfig {
    pub api_key: String,
    pub api_secret: String,
    pub testnet: bool,
}

/// Create Zerodha adapter
pub fn create_zerodha_adapter(config: VenueConfig) -> ZerodhaAdapter {
    let auth_config = auth::ZerodhaConfig::new(
        String::with_capacity(100), // Will be loaded from env
        String::with_capacity(100),
        String::with_capacity(100),
        config.api_key,
        config.api_secret,
    );
    ZerodhaAdapter {
        auth: Arc::new(auth::ZerodhaAuth::new(auth_config)),
        symbol_map: Arc::new(DashMap::new()),
        orders: Arc::new(DashMap::new()),
        order_id_counter: Arc::new(AtomicU64::new(1)),
        market_open_hour: 9,
        market_open_min: 15,
        market_close_hour: 15,
        market_close_min: 30,
        avg_latency_ns: AtomicU64::new(1_000_000), // 1ms default
        _padding: [0; 24],
    }
}

/// Create Binance adapter
pub fn create_binance_adapter(config: VenueConfig) -> BinanceAdapter {
    let auth_config = auth::BinanceConfig {
        api_key: config.api_key.clone(),
        api_secret: config.api_secret.clone(),
        testnet: config.testnet,
        market: if config.testnet {
            auth::BinanceMarket::UsdFutures // Use futures for testnet as it's more commonly used
        } else {
            auth::BinanceMarket::Spot // Use spot for production
        },
    };

    let mut binance_auth = auth::BinanceAuth::new();
    let _ = binance_auth.add_market(auth_config);

    BinanceAdapter {
        auth: Arc::new(binance_auth),
        symbol_map: Arc::new(DashMap::new()),
        orders: Arc::new(DashMap::new()),
        order_id_counter: Arc::new(AtomicU64::new(1)),
        testnet: config.testnet,
        avg_latency_ns: AtomicU64::new(500_000), // 500μs default
        _padding: [0; 30],
    }
}

/// Create venue adapter based on type
pub fn create_venue(
    venue_type: super::core::VenueType,
    config: VenueConfig,
) -> Box<dyn VenueAdapter> {
    match venue_type {
        super::core::VenueType::Zerodha => Box::new(create_zerodha_adapter(config)),
        super::core::VenueType::Binance => Box::new(create_binance_adapter(config)),
    }
}
