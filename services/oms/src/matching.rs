//! Order matching engine
//!
//! High-performance order matching with price-time priority.

use anyhow::Result;
use services_common::{Px, Qty, Symbol};
use crossbeam::queue::SegQueue;
use fxhash::FxHashMap;
use parking_lot::RwLock;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::Arc;
use tracing::debug;
use uuid::Uuid;

use crate::order::{Order, OrderSide, OrderType, Fill, LiquidityIndicator};

/// Matching engine for order execution
#[derive(Debug)]
pub struct MatchingEngine {
    /// Order books by symbol
    order_books: Arc<RwLock<FxHashMap<Symbol, OrderBookMatcher>>>,
    /// Match ID generator
    match_sequence: AtomicU64,
    /// Pending matches
    pending_matches: Arc<SegQueue<Match>>,
}

/// Order book matcher for a single symbol
#[derive(Debug)]
pub struct OrderBookMatcher {
    /// Symbol
    symbol: Symbol,
    /// Buy orders (sorted by price descending, time ascending)
    buy_orders: Arc<RwLock<BTreeMap<OrderKey, Order>>>,
    /// Sell orders (sorted by price ascending, time ascending)
    sell_orders: Arc<RwLock<BTreeMap<OrderKey, Order>>>,
    /// Last match price
    last_price: AtomicU64,
    /// Total volume matched
    total_volume: AtomicU64,
}

/// Key for order sorting (price-time priority)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OrderKey {
    /// Price (negative for buy orders to sort descending)
    price: i64,
    /// Sequence number for time priority
    sequence: u64,
}

impl Ord for OrderKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.price.cmp(&other.price)
            .then_with(|| self.sequence.cmp(&other.sequence))
    }
}

impl PartialOrd for OrderKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Match result
#[derive(Debug, Clone)]
pub struct Match {
    /// Match ID
    pub id: u64,
    /// Symbol
    pub symbol: Symbol,
    /// Aggressive order (taker)
    pub aggressive_order: Uuid,
    /// Passive order (maker)
    pub passive_order: Uuid,
    /// Match quantity
    pub quantity: Qty,
    /// Match price
    pub price: Px,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Default for MatchingEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MatchingEngine {
    /// Create new matching engine
    #[must_use] pub fn new() -> Self {
        Self {
            order_books: Arc::new(RwLock::new(FxHashMap::default())),
            match_sequence: AtomicU64::new(1),
            pending_matches: Arc::new(SegQueue::new()),
        }
    }
    
    /// Add order to matching engine
    pub fn add_order(&self, order: &Order) -> Result<Vec<Match>> {
        // Get or create order book for symbol
        let mut order_books = self.order_books.write();
        let order_book = order_books.entry(order.symbol)
            .or_insert_with(|| OrderBookMatcher::new(order.symbol));
        
        // Match immediately for market orders
        if matches!(order.order_type, OrderType::Market) {
            return self.match_market_order(order, order_book);
        }
        
        // Add limit order to book
        if let Some(price) = order.price {
            let key = match order.side {
                OrderSide::Buy => OrderKey {
                    price: -price.as_i64(),  // Negative for descending sort
                    sequence: order.sequence_number,
                },
                OrderSide::Sell => OrderKey {
                    price: price.as_i64(),
                    sequence: order.sequence_number,
                },
            };
            
            match order.side {
                OrderSide::Buy => {
                    order_book.buy_orders.write().insert(key, order.clone());
                }
                OrderSide::Sell => {
                    order_book.sell_orders.write().insert(key, order.clone());
                }
            }
            
            // Try to match with existing orders
            self.match_limit_order(order, order_book)
        } else {
            Err(anyhow::anyhow!("Limit order requires price"))
        }
    }
    
    /// Match market order
    fn match_market_order(
        &self,
        order: &Order,
        order_book: &mut OrderBookMatcher,
    ) -> Result<Vec<Match>> {
        let mut matches = Vec::new();
        let mut remaining = order.remaining_quantity;
        
        match order.side {
            OrderSide::Buy => {
                // Match against sell orders
                let mut sell_orders = order_book.sell_orders.write();
                let mut to_remove = Vec::new();
                
                for (key, sell_order) in sell_orders.iter_mut() {
                    if remaining == Qty::ZERO {
                        break;
                    }
                    
                    let match_qty = remaining.min(sell_order.remaining_quantity);
                    let match_price = sell_order.price.unwrap_or(Px::ZERO);
                    
                    // Create match
                    let match_id = self.match_sequence.fetch_add(1, AtomicOrdering::SeqCst);
                    let m = Match {
                        id: match_id,
                        symbol: order.symbol,
                        aggressive_order: order.id,
                        passive_order: sell_order.id,
                        quantity: match_qty,
                        price: match_price,
                        timestamp: chrono::Utc::now(),
                    };
                    
                    matches.push(m.clone());
                    self.pending_matches.push(m);
                    
                    // Update quantities
                    remaining = Qty::from_i64(remaining.as_i64() - match_qty.as_i64());
                    sell_order.remaining_quantity = Qty::from_i64(
                        sell_order.remaining_quantity.as_i64() - match_qty.as_i64()
                    );
                    
                    if sell_order.remaining_quantity == Qty::ZERO {
                        to_remove.push(*key);
                    }
                    
                    // Update last price
                    order_book.last_price.store(match_price.as_i64() as u64, AtomicOrdering::Release);
                    order_book.total_volume.fetch_add(match_qty.as_i64() as u64, AtomicOrdering::Relaxed);
                }
                
                // Remove fully filled orders
                for key in to_remove {
                    sell_orders.remove(&key);
                }
            }
            OrderSide::Sell => {
                // Match against buy orders
                let mut buy_orders = order_book.buy_orders.write();
                let mut to_remove = Vec::new();
                
                for (key, buy_order) in buy_orders.iter_mut() {
                    if remaining == Qty::ZERO {
                        break;
                    }
                    
                    let match_qty = remaining.min(buy_order.remaining_quantity);
                    let match_price = buy_order.price.unwrap_or(Px::ZERO);
                    
                    // Create match
                    let match_id = self.match_sequence.fetch_add(1, AtomicOrdering::SeqCst);
                    let m = Match {
                        id: match_id,
                        symbol: order.symbol,
                        aggressive_order: order.id,
                        passive_order: buy_order.id,
                        quantity: match_qty,
                        price: match_price,
                        timestamp: chrono::Utc::now(),
                    };
                    
                    matches.push(m.clone());
                    self.pending_matches.push(m);
                    
                    // Update quantities
                    remaining = Qty::from_i64(remaining.as_i64() - match_qty.as_i64());
                    buy_order.remaining_quantity = Qty::from_i64(
                        buy_order.remaining_quantity.as_i64() - match_qty.as_i64()
                    );
                    
                    if buy_order.remaining_quantity == Qty::ZERO {
                        to_remove.push(*key);
                    }
                    
                    // Update last price
                    order_book.last_price.store(match_price.as_i64() as u64, AtomicOrdering::Release);
                    order_book.total_volume.fetch_add(match_qty.as_i64() as u64, AtomicOrdering::Relaxed);
                }
                
                // Remove fully filled orders
                for key in to_remove {
                    buy_orders.remove(&key);
                }
            }
        }
        
        debug!("Market order {} matched {} times", order.id, matches.len());
        Ok(matches)
    }
    
    /// Match limit order
    fn match_limit_order(
        &self,
        order: &Order,
        order_book: &mut OrderBookMatcher,
    ) -> Result<Vec<Match>> {
        let mut matches = Vec::new();
        let order_price = order.price.ok_or_else(|| anyhow::anyhow!("Limit order requires price"))?;
        
        match order.side {
            OrderSide::Buy => {
                // Match against sell orders at or below our price
                let mut sell_orders = order_book.sell_orders.write();
                let mut to_remove = Vec::new();
                
                for (key, sell_order) in sell_orders.iter_mut() {
                    let sell_price = sell_order.price.unwrap_or(Px::ZERO);
                    
                    // Stop if sell price is above our buy price
                    if sell_price > order_price {
                        break;
                    }
                    
                    let match_qty = order.remaining_quantity.min(sell_order.remaining_quantity);
                    
                    // Create match at passive order price
                    let match_id = self.match_sequence.fetch_add(1, AtomicOrdering::SeqCst);
                    let m = Match {
                        id: match_id,
                        symbol: order.symbol,
                        aggressive_order: order.id,
                        passive_order: sell_order.id,
                        quantity: match_qty,
                        price: sell_price,
                        timestamp: chrono::Utc::now(),
                    };
                    
                    matches.push(m.clone());
                    self.pending_matches.push(m);
                    
                    // Update quantities
                    sell_order.remaining_quantity = Qty::from_i64(
                        sell_order.remaining_quantity.as_i64() - match_qty.as_i64()
                    );
                    
                    if sell_order.remaining_quantity == Qty::ZERO {
                        to_remove.push(*key);
                    }
                    
                    // Update last price
                    order_book.last_price.store(sell_price.as_i64() as u64, AtomicOrdering::Release);
                    order_book.total_volume.fetch_add(match_qty.as_i64() as u64, AtomicOrdering::Relaxed);
                }
                
                // Remove fully filled orders
                for key in to_remove {
                    sell_orders.remove(&key);
                }
            }
            OrderSide::Sell => {
                // Match against buy orders at or above our price
                let mut buy_orders = order_book.buy_orders.write();
                let mut to_remove = Vec::new();
                
                for (key, buy_order) in buy_orders.iter_mut() {
                    let buy_price = buy_order.price.unwrap_or(Px::ZERO);
                    
                    // Stop if buy price is below our sell price
                    if buy_price < order_price {
                        break;
                    }
                    
                    let match_qty = order.remaining_quantity.min(buy_order.remaining_quantity);
                    
                    // Create match at passive order price
                    let match_id = self.match_sequence.fetch_add(1, AtomicOrdering::SeqCst);
                    let m = Match {
                        id: match_id,
                        symbol: order.symbol,
                        aggressive_order: order.id,
                        passive_order: buy_order.id,
                        quantity: match_qty,
                        price: buy_price,
                        timestamp: chrono::Utc::now(),
                    };
                    
                    matches.push(m.clone());
                    self.pending_matches.push(m);
                    
                    // Update quantities
                    buy_order.remaining_quantity = Qty::from_i64(
                        buy_order.remaining_quantity.as_i64() - match_qty.as_i64()
                    );
                    
                    if buy_order.remaining_quantity == Qty::ZERO {
                        to_remove.push(*key);
                    }
                    
                    // Update last price
                    order_book.last_price.store(buy_price.as_i64() as u64, AtomicOrdering::Release);
                    order_book.total_volume.fetch_add(match_qty.as_i64() as u64, AtomicOrdering::Relaxed);
                }
                
                // Remove fully filled orders
                for key in to_remove {
                    buy_orders.remove(&key);
                }
            }
        }
        
        if !matches.is_empty() {
            debug!("Limit order {} matched {} times", order.id, matches.len());
        }
        
        Ok(matches)
    }
    
    /// Cancel order
    pub fn cancel_order(&self, order_id: Uuid, symbol: Symbol) -> Result<bool> {
        let mut order_books = self.order_books.write();
        if let Some(order_book) = order_books.get_mut(&symbol) {
            // Check buy orders
            {
                let mut buy_orders = order_book.buy_orders.write();
                let mut found_key = None;
                
                for (key, order) in buy_orders.iter() {
                    if order.id == order_id {
                        found_key = Some(*key);
                        break;
                    }
                }
                
                if let Some(key) = found_key {
                    buy_orders.remove(&key);
                    return Ok(true);
                }
            }
            
            // Check sell orders
            {
                let mut sell_orders = order_book.sell_orders.write();
                let mut found_key = None;
                
                for (key, order) in sell_orders.iter() {
                    if order.id == order_id {
                        found_key = Some(*key);
                        break;
                    }
                }
                
                if let Some(key) = found_key {
                    sell_orders.remove(&key);
                    return Ok(true);
                }
            }
        }
        
        Ok(false)
    }
    
    /// Get pending matches
    pub fn get_pending_matches(&self) -> Vec<Match> {
        let mut matches = Vec::new();
        while let Some(m) = self.pending_matches.pop() {
            matches.push(m);
        }
        matches
    }
    
    /// Get order book depth
    pub fn get_depth(&self, symbol: Symbol, levels: usize) -> Option<OrderBookDepth> {
        let order_books = self.order_books.read();
        order_books.get(&symbol).map(|order_book| {
            let buy_orders = order_book.buy_orders.read();
            let sell_orders = order_book.sell_orders.read();
            
            let mut bids = Vec::new();
            let mut asks = Vec::new();
            
            // Aggregate buy orders by price
            let mut bid_levels: BTreeMap<i64, i64> = BTreeMap::new();
            for (key, order) in buy_orders.iter().take(levels * 10) {
                let price = -key.price;  // Reverse negative price
                *bid_levels.entry(price).or_insert(0) += order.remaining_quantity.as_i64();
            }
            
            for (price, quantity) in bid_levels.iter().rev().take(levels) {
                bids.push((*price, *quantity));
            }
            
            // Aggregate sell orders by price
            let mut ask_levels: BTreeMap<i64, i64> = BTreeMap::new();
            for (key, order) in sell_orders.iter().take(levels * 10) {
                let price = key.price;
                *ask_levels.entry(price).or_insert(0) += order.remaining_quantity.as_i64();
            }
            
            for (price, quantity) in ask_levels.iter().take(levels) {
                asks.push((*price, *quantity));
            }
            
            OrderBookDepth {
                symbol,
                bids,
                asks,
                last_price: order_book.last_price.load(AtomicOrdering::Acquire) as i64,
                total_volume: order_book.total_volume.load(AtomicOrdering::Acquire) as i64,
            }
        })
    }
}

impl OrderBookMatcher {
    /// Create new order book matcher
    fn new(symbol: Symbol) -> Self {
        Self {
            symbol,
            buy_orders: Arc::new(RwLock::new(BTreeMap::new())),
            sell_orders: Arc::new(RwLock::new(BTreeMap::new())),
            last_price: AtomicU64::new(0),
            total_volume: AtomicU64::new(0),
        }
    }
    
    /// Get the symbol for this matcher
    pub fn get_symbol(&self) -> Symbol {
        self.symbol
    }
    
    /// Get order book depth with symbol information
    pub fn get_depth(&self, levels: usize) -> OrderBookDepth {
        let buy_orders = self.buy_orders.read();
        let sell_orders = self.sell_orders.read();
        
        let bids: Vec<(i64, i64)> = buy_orders
            .iter()
            .take(levels)
            .filter_map(|(key, order)| {
                order.price.map(|price| {
                    // Use key for validation - ensure price matches the BTreeMap key
                    let price_i64 = price.as_i64();
                    // For buy orders, key.price is negative of actual price
                    let expected_key_price = if key.price < 0 { -price_i64 } else { price_i64 };
                    debug_assert_eq!(key.price, expected_key_price, "Price mismatch in orderbook key");
                    (price_i64, order.quantity.as_i64())
                })
            })
            .collect();
            
        let asks: Vec<(i64, i64)> = sell_orders
            .iter()
            .take(levels)
            .filter_map(|(key, order)| {
                order.price.map(|price| {
                    // Use key for validation - ensure price matches the BTreeMap key
                    let price_i64 = price.as_i64();
                    // For buy orders, key.price is negative of actual price
                    let expected_key_price = if key.price < 0 { -price_i64 } else { price_i64 };
                    debug_assert_eq!(key.price, expected_key_price, "Price mismatch in orderbook key");
                    (price_i64, order.quantity.as_i64())
                })
            })
            .collect();
        
        OrderBookDepth {
            symbol: self.symbol, // Using the symbol field
            bids,
            asks,
            last_price: self.last_price.load(AtomicOrdering::Relaxed) as i64,
            total_volume: self.total_volume.load(AtomicOrdering::Relaxed) as i64,
        }
    }
    
    /// Get symbol-specific statistics
    pub fn get_symbol_stats(&self) -> SymbolStats {
        SymbolStats {
            symbol: self.symbol,
            last_price: self.last_price.load(AtomicOrdering::Relaxed) as i64,
            total_volume: self.total_volume.load(AtomicOrdering::Relaxed),
            bid_count: self.buy_orders.read().len(),
            ask_count: self.sell_orders.read().len(),
        }
    }
}

/// Symbol-specific statistics
#[derive(Debug, Clone)]
pub struct SymbolStats {
    /// Symbol identifier
    pub symbol: Symbol,
    /// Last traded price
    pub last_price: i64,
    /// Total volume traded
    pub total_volume: u64,
    /// Number of bid orders
    pub bid_count: usize,
    /// Number of ask orders
    pub ask_count: usize,
}

/// Order book depth
#[derive(Debug, Clone)]
pub struct OrderBookDepth {
    /// Symbol
    pub symbol: Symbol,
    /// Bid levels (price, quantity)
    pub bids: Vec<(i64, i64)>,
    /// Ask levels (price, quantity)
    pub asks: Vec<(i64, i64)>,
    /// Last match price
    pub last_price: i64,
    /// Total volume matched
    pub total_volume: i64,
}

/// Convert match to fill
#[must_use] pub fn match_to_fills(m: &Match) -> (Fill, Fill) {
    // Aggressive order fill (taker)
    let aggressive_fill = Fill {
        id: Uuid::new_v4(),
        order_id: m.aggressive_order,
        execution_id: format!("MATCH-{}", m.id),
        quantity: m.quantity,
        price: m.price,
        commission: (m.quantity.as_i64() * m.price.as_i64() * 20) / 1000000,  // 0.02% taker fee
        commission_currency: "USDT".to_string(),
        timestamp: m.timestamp,
        liquidity: LiquidityIndicator::Taker,
    };
    
    // Passive order fill (maker)
    let passive_fill = Fill {
        id: Uuid::new_v4(),
        order_id: m.passive_order,
        execution_id: format!("MATCH-{}", m.id),
        quantity: m.quantity,
        price: m.price,
        commission: (m.quantity.as_i64() * m.price.as_i64() * 10) / 1000000,  // 0.01% maker fee
        commission_currency: "USDT".to_string(),
        timestamp: m.timestamp,
        liquidity: LiquidityIndicator::Maker,
    };
    
    (aggressive_fill, passive_fill)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::{OrderSide, OrderStatus, OrderType, TimeInForce};
    
    fn create_test_order(side: OrderSide, price: Option<i64>, qty: i64, seq: u64) -> Order {
        Order {
            id: Uuid::new_v4(),
            client_order_id: None,
            parent_order_id: None,
            symbol: Symbol(1),
            side,
            order_type: if price.is_some() { OrderType::Limit } else { OrderType::Market },
            time_in_force: TimeInForce::Gtc,
            quantity: Qty::from_i64(qty),
            executed_quantity: Qty::ZERO,
            remaining_quantity: Qty::from_i64(qty),
            price: price.map(Px::from_i64),
            stop_price: None,
            status: OrderStatus::New,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            account: "test".to_string(),
            exchange: "internal".to_string(),
            strategy_id: None,
            tags: vec![],
            fills: vec![],
            amendments: vec![],
            version: 1,
            sequence_number: seq,
        }
    }
    
    #[test]
    fn test_order_key_sorting() {
        // Buy orders should sort by price descending (higher first)
        let buy_key1 = OrderKey { price: -1000000, sequence: 1 };
        let buy_key2 = OrderKey { price: -1100000, sequence: 2 };
        assert!(buy_key2 < buy_key1);  // Higher price comes first
        
        // Sell orders should sort by price ascending (lower first)
        let sell_key1 = OrderKey { price: 1000000, sequence: 1 };
        let sell_key2 = OrderKey { price: 1100000, sequence: 2 };
        assert!(sell_key1 < sell_key2);  // Lower price comes first
        
        // Same price should sort by sequence (time priority)
        let key1 = OrderKey { price: 1000000, sequence: 1 };
        let key2 = OrderKey { price: 1000000, sequence: 2 };
        assert!(key1 < key2);
    }
    
    #[test]
    fn test_limit_order_matching() {
        let engine = MatchingEngine::new();
        
        // Add sell order at 100
        let sell_order = create_test_order(OrderSide::Sell, Some(1000000), 10000, 1);
        let matches = engine.add_order(&sell_order).unwrap();
        assert_eq!(matches.len(), 0);  // No match yet
        
        // Add buy order at 101 (crosses the spread)
        let buy_order = create_test_order(OrderSide::Buy, Some(1010000), 5000, 2);
        let matches = engine.add_order(&buy_order).unwrap();
        assert_eq!(matches.len(), 1);  // Should match
        assert_eq!(matches[0].quantity.as_i64(), 5000);
        assert_eq!(matches[0].price.as_i64(), 1000000);  // Match at passive (sell) price
    }
    
    #[test]
    fn test_market_order_matching() {
        let engine = MatchingEngine::new();
        
        // Add limit orders
        let sell1 = create_test_order(OrderSide::Sell, Some(1000000), 5000, 1);
        let sell2 = create_test_order(OrderSide::Sell, Some(1010000), 5000, 2);
        engine.add_order(&sell1).unwrap();
        engine.add_order(&sell2).unwrap();
        
        // Market buy should match both
        let market_buy = create_test_order(OrderSide::Buy, None, 8000, 3);
        let matches = engine.add_order(&market_buy).unwrap();
        
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].quantity.as_i64(), 5000);
        assert_eq!(matches[0].price.as_i64(), 1000000);  // Best sell price
        assert_eq!(matches[1].quantity.as_i64(), 3000);
        assert_eq!(matches[1].price.as_i64(), 1010000);  // Next best sell price
    }
}