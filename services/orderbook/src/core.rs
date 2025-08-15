//! Core order book implementation with lock-free operations
//!
//! This module contains the heart of our institutional-grade orderbook.
//! It implements a price-time priority order book with support for:
//! - L2 (aggregated price levels) 
//! - L3 (individual orders)
//! - Order-by-order reconstruction
//! - Deterministic checksum validation

use common::{Px, Qty, Ts};
use std::sync::atomic::{AtomicU64, AtomicI64, Ordering};
use std::collections::BTreeMap;
use parking_lot::RwLock;
use ahash::AHashMap;
use smallvec::SmallVec;
// ArrayVec import removed - not currently used

/// Side of the order book (Bid or Ask)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// Buy side (bids)
    Bid,
    /// Sell side (asks/offers)
    Ask,
}

/// Individual order in the order book (L3 data)
#[derive(Debug, Clone)]
pub struct Order {
    /// Unique order identifier
    pub id: u64,
    /// Price of the order
    pub price: Px,
    /// Remaining quantity
    pub quantity: Qty,
    /// Original quantity when order was placed
    pub original_quantity: Qty,
    /// Timestamp when order was placed
    pub timestamp: Ts,
    /// Side of the order
    pub side: Side,
    /// Is this an iceberg order (hidden quantity)
    pub is_iceberg: bool,
    /// Visible quantity for iceberg orders
    pub visible_quantity: Option<Qty>,
}

/// A price level in the order book containing multiple orders
#[repr(align(64))] // Cache-line aligned for performance
pub struct PriceLevel {
    /// Price of this level
    price: Px,
    /// Total quantity at this level (atomic for lock-free reads)
    total_quantity: AtomicI64,
    /// Number of orders at this level
    order_count: AtomicU64,
    /// Individual orders at this price (L3 data)
    orders: RwLock<SmallVec<[Order; 8]>>,
    /// Last update timestamp
    last_update: AtomicU64,
    /// Iceberg quantity (hidden volume)
    hidden_quantity: AtomicI64,
}

impl PriceLevel {
    /// Create a new price level
    pub fn new(price: Px) -> Self {
        Self {
            price,
            total_quantity: AtomicI64::new(0),
            order_count: AtomicU64::new(0),
            orders: RwLock::new(SmallVec::new()),
            last_update: AtomicU64::new(0),
            hidden_quantity: AtomicI64::new(0),
        }
    }

    /// Add an order to this price level
    pub fn add_order(&self, order: Order) {
        let qty = order.quantity.as_i64();
        let hidden = if order.is_iceberg {
            order.quantity.as_i64() - order.visible_quantity.unwrap_or(order.quantity).as_i64()
        } else {
            0
        };

        // Update atomics first for lock-free readers
        self.total_quantity.fetch_add(qty, Ordering::Release);
        self.order_count.fetch_add(1, Ordering::Release);
        if hidden > 0 {
            self.hidden_quantity.fetch_add(hidden, Ordering::Release);
        }
        self.last_update.store(order.timestamp.as_nanos() as u64, Ordering::Release);

        // Then update the order list
        let mut orders = self.orders.write();
        orders.push(order);
    }

    /// Remove an order by ID
    pub fn remove_order(&self, order_id: u64) -> Option<Order> {
        let mut orders = self.orders.write();
        if let Some(pos) = orders.iter().position(|o| o.id == order_id) {
            let order = orders.swap_remove(pos);
            
            // Update atomics
            self.total_quantity.fetch_sub(order.quantity.as_i64(), Ordering::Release);
            self.order_count.fetch_sub(1, Ordering::Release);
            
            if order.is_iceberg {
                let hidden = order.quantity.as_i64() - 
                    order.visible_quantity.unwrap_or(order.quantity).as_i64();
                if hidden > 0 {
                    self.hidden_quantity.fetch_sub(hidden, Ordering::Release);
                }
            }
            
            Some(order)
        } else {
            None
        }
    }

    /// Get total quantity at this level (lock-free)
    #[inline]
    pub fn get_quantity(&self) -> Qty {
        Qty::from_i64(self.total_quantity.load(Ordering::Acquire))
    }

    /// Get number of orders at this level (lock-free)
    #[inline]
    pub fn get_order_count(&self) -> u64 {
        self.order_count.load(Ordering::Acquire)
    }

    /// Get hidden (iceberg) quantity
    #[inline]
    pub fn get_hidden_quantity(&self) -> Qty {
        Qty::from_i64(self.hidden_quantity.load(Ordering::Acquire))
    }
}

/// The main order book structure
pub struct OrderBook {
    /// Symbol for this order book
    symbol: String,
    
    /// Bid levels (buy orders) - BTreeMap keeps them sorted
    bids: RwLock<BTreeMap<i64, PriceLevel>>, // Key is negative price for reverse order
    
    /// Ask levels (sell orders) - BTreeMap keeps them sorted
    asks: RwLock<BTreeMap<i64, PriceLevel>>, // Key is positive price
    
    /// Best bid price (atomic for lock-free access)
    best_bid: AtomicI64,
    
    /// Best ask price (atomic for lock-free access)
    best_ask: AtomicI64,
    
    /// Total bid volume
    total_bid_volume: AtomicI64,
    
    /// Total ask volume
    total_ask_volume: AtomicI64,
    
    /// Last update timestamp
    last_update: AtomicU64,
    
    /// Sequence number for updates (for deterministic replay)
    sequence: AtomicU64,
    
    /// Order ID to price mapping for fast lookups (L3 support)
    order_map: RwLock<AHashMap<u64, (Side, Px)>>,
    
    /// Checksum for integrity validation
    checksum: AtomicU64,
}

impl OrderBook {
    /// Create a new order book for a symbol
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            bids: RwLock::new(BTreeMap::new()),
            asks: RwLock::new(BTreeMap::new()),
            best_bid: AtomicI64::new(0),
            best_ask: AtomicI64::new(i64::MAX),
            total_bid_volume: AtomicI64::new(0),
            total_ask_volume: AtomicI64::new(0),
            last_update: AtomicU64::new(0),
            sequence: AtomicU64::new(0),
            order_map: RwLock::new(AHashMap::new()),
            checksum: AtomicU64::new(0),
        }
    }

    /// Add a new order to the book
    pub fn add_order(&self, order: Order) -> u64 {
        let seq = self.sequence.fetch_add(1, Ordering::AcqRel);
        let price_key = self.get_price_key(order.price, order.side);
        
        // Update order map first
        {
            let mut map = self.order_map.write();
            map.insert(order.id, (order.side, order.price));
        }
        
        // Add to appropriate side
        match order.side {
            Side::Bid => {
                let mut bids = self.bids.write();
                let level = bids.entry(price_key)
                    .or_insert_with(|| PriceLevel::new(order.price));
                
                level.add_order(order.clone());
                
                // Update best bid if necessary
                if order.price.as_i64() > self.best_bid.load(Ordering::Acquire) {
                    self.best_bid.store(order.price.as_i64(), Ordering::Release);
                }
                
                // Update total volume
                self.total_bid_volume.fetch_add(order.quantity.as_i64(), Ordering::Release);
            }
            Side::Ask => {
                let mut asks = self.asks.write();
                let level = asks.entry(price_key)
                    .or_insert_with(|| PriceLevel::new(order.price));
                
                level.add_order(order.clone());
                
                // Update best ask if necessary
                if order.price.as_i64() < self.best_ask.load(Ordering::Acquire) {
                    self.best_ask.store(order.price.as_i64(), Ordering::Release);
                }
                
                // Update total volume
                self.total_ask_volume.fetch_add(order.quantity.as_i64(), Ordering::Release);
            }
        }
        
        // Update timestamp and checksum
        self.last_update.store(order.timestamp.as_nanos() as u64, Ordering::Release);
        self.update_checksum();
        
        seq
    }

    /// Cancel an order by ID
    pub fn cancel_order(&self, order_id: u64) -> Option<Order> {
        let _seq = self.sequence.fetch_add(1, Ordering::AcqRel);
        
        // Look up order location
        let (side, price) = {
            let map = self.order_map.read();
            map.get(&order_id).copied()?
        };
        
        let price_key = self.get_price_key(price, side);
        
        // Remove from appropriate side
        let removed_order = match side {
            Side::Bid => {
                let mut bids = self.bids.write();
                if let Some(level) = bids.get_mut(&price_key) {
                    let order = level.remove_order(order_id);
                    
                    // Remove level if empty
                    if level.get_order_count() == 0 {
                        bids.remove(&price_key);
                        
                        // Update best bid if this was the best
                        if price.as_i64() == self.best_bid.load(Ordering::Acquire) {
                            let new_best = bids.keys().next().map(|k| -k).unwrap_or(0);
                            self.best_bid.store(new_best, Ordering::Release);
                        }
                    }
                    
                    if let Some(ref o) = order {
                        self.total_bid_volume.fetch_sub(o.quantity.as_i64(), Ordering::Release);
                    }
                    
                    order
                } else {
                    None
                }
            }
            Side::Ask => {
                let mut asks = self.asks.write();
                if let Some(level) = asks.get_mut(&price_key) {
                    let order = level.remove_order(order_id);
                    
                    // Remove level if empty
                    if level.get_order_count() == 0 {
                        asks.remove(&price_key);
                        
                        // Update best ask if this was the best
                        if price.as_i64() == self.best_ask.load(Ordering::Acquire) {
                            let new_best = asks.keys().next().copied().unwrap_or(i64::MAX);
                            self.best_ask.store(new_best, Ordering::Release);
                        }
                    }
                    
                    if let Some(ref o) = order {
                        self.total_ask_volume.fetch_sub(o.quantity.as_i64(), Ordering::Release);
                    }
                    
                    order
                } else {
                    None
                }
            }
        };
        
        // Remove from order map
        if removed_order.is_some() {
            let mut map = self.order_map.write();
            map.remove(&order_id);
            self.update_checksum();
        }
        
        removed_order
    }

    /// Get the best bid and ask prices (lock-free)
    #[inline]
    pub fn get_bbo(&self) -> (Option<Px>, Option<Px>) {
        let bid = self.best_bid.load(Ordering::Acquire);
        let ask = self.best_ask.load(Ordering::Acquire);
        
        let best_bid = if bid > 0 { Some(Px::from_i64(bid)) } else { None };
        let best_ask = if ask < i64::MAX { Some(Px::from_i64(ask)) } else { None };
        
        (best_bid, best_ask)
    }

    /// Get the spread in ticks
    #[inline]
    pub fn get_spread(&self) -> Option<i64> {
        let (bid, ask) = self.get_bbo();
        match (bid, ask) {
            (Some(b), Some(a)) => Some(a.as_i64() - b.as_i64()),
            _ => None,
        }
    }

    /// Get top N levels of the book (for L2 data)
    pub fn get_depth(&self, levels: usize) -> (Vec<(Px, Qty, u64)>, Vec<(Px, Qty, u64)>) {
        let bids = self.bids.read();
        let asks = self.asks.read();
        
        let bid_levels: Vec<_> = bids.iter()
            .take(levels)
            .map(|(_, level)| {
                (level.price, level.get_quantity(), level.get_order_count())
            })
            .collect();
            
        let ask_levels: Vec<_> = asks.iter()
            .take(levels)
            .map(|(_, level)| {
                (level.price, level.get_quantity(), level.get_order_count())
            })
            .collect();
            
        (bid_levels, ask_levels)
    }

    /// Calculate checksum for integrity validation (Binance-style)
    fn update_checksum(&self) {
        let (bids, asks) = self.get_depth(25);
        
        let mut checksum_str = String::new();
        for (price, qty, _) in bids.iter().chain(asks.iter()).take(50) {
            checksum_str.push_str(&format!("{:.2}:{:.4}", 
                price.as_f64(), qty.as_f64()));
        }
        
        let checksum = crc32fast::hash(checksum_str.as_bytes());
        self.checksum.store(checksum as u64, Ordering::Release);
    }

    /// Get current checksum for validation
    #[inline]
    pub fn get_checksum(&self) -> u64 {
        self.checksum.load(Ordering::Acquire)
    }

    /// Get price key for BTreeMap storage
    #[inline]
    fn get_price_key(&self, price: Px, side: Side) -> i64 {
        match side {
            Side::Bid => -price.as_i64(), // Negative for reverse order
            Side::Ask => price.as_i64(),   // Positive for normal order
        }
    }

    /// Clear all orders from the orderbook
    pub fn clear(&self) {
        // Clear bid side
        {
            let mut bids = self.bids.write();
            bids.clear();
        }
        
        // Clear ask side
        {
            let mut asks = self.asks.write();
            asks.clear();
        }
        
        // Clear order map
        {
            let mut order_map = self.order_map.write();
            order_map.clear();
        }
        
        // Reset atomics
        self.best_bid.store(0, Ordering::Release);
        self.best_ask.store(i64::MAX, Ordering::Release);
        self.total_bid_volume.store(0, Ordering::Release);
        self.total_ask_volume.store(0, Ordering::Release);
        self.checksum.store(0, Ordering::Release);
    }

    /// Load orderbook from snapshot levels
    pub fn load_snapshot(&self, bid_levels: Vec<(Px, Qty, u64)>, ask_levels: Vec<(Px, Qty, u64)>) {
        // Clear existing state
        self.clear();
        
        // Recreate bid levels
        let mut bids = self.bids.write();
        let mut best_bid = 0i64;
        let mut total_bid_vol = 0i64;
        
        for (price, quantity, order_count) in bid_levels {
            let price_key = self.get_price_key(price, Side::Bid);
            let level = PriceLevel::new(price);
            
            // Create synthetic orders to match count and quantity
            let qty_per_order = quantity.as_i64() / order_count.max(1) as i64;
            for i in 0..order_count {
                let order = Order {
                    id: u64::MAX - i, // Synthetic IDs
                    price,
                    quantity: Qty::from_i64(qty_per_order),
                    original_quantity: Qty::from_i64(qty_per_order),
                    timestamp: Ts::now(),
                    side: Side::Bid,
                    is_iceberg: false,
                    visible_quantity: None,
                };
                level.add_order(order);
            }
            
            if price.as_i64() > best_bid {
                best_bid = price.as_i64();
            }
            total_bid_vol += quantity.as_i64();
            
            bids.insert(price_key, level);
        }
        
        drop(bids);
        
        // Recreate ask levels
        let mut asks = self.asks.write();
        let mut best_ask = i64::MAX;
        let mut total_ask_vol = 0i64;
        
        for (price, quantity, order_count) in ask_levels {
            let price_key = self.get_price_key(price, Side::Ask);
            let level = PriceLevel::new(price);
            
            // Create synthetic orders to match count and quantity
            let qty_per_order = quantity.as_i64() / order_count.max(1) as i64;
            for i in 0..order_count {
                let order = Order {
                    id: u64::MAX / 2 - i, // Different synthetic IDs for asks
                    price,
                    quantity: Qty::from_i64(qty_per_order),
                    original_quantity: Qty::from_i64(qty_per_order),
                    timestamp: Ts::now(),
                    side: Side::Ask,
                    is_iceberg: false,
                    visible_quantity: None,
                };
                level.add_order(order);
            }
            
            if price.as_i64() < best_ask {
                best_ask = price.as_i64();
            }
            total_ask_vol += quantity.as_i64();
            
            asks.insert(price_key, level);
        }
        
        drop(asks);
        
        // Update atomics
        self.best_bid.store(best_bid, Ordering::Release);
        self.best_ask.store(best_ask, Ordering::Release);
        self.total_bid_volume.store(total_bid_vol, Ordering::Release);
        self.total_ask_volume.store(total_ask_vol, Ordering::Release);
        
        // Update checksum
        self.update_checksum();
    }
}