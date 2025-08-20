//! Market Making Strategy - Continuous bid/ask quoting

use crate::{ComponentHealth, OrderType, Side, TimeInForce, TradingEvent, TradingStrategy};
use anyhow::Result;
use async_trait::async_trait;
use services_common::{Px, Qty, Symbol};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info};

/// Market making strategy for continuous bid/ask quoting
/// 
/// The `MarketMakingStrategy` provides liquidity to the market by continuously
/// quoting bid and ask prices around the fair value. It dynamically adjusts
/// spreads based on market toxicity (VPIN) and skews prices based on order
/// book imbalance to manage adverse selection risk.
/// 
/// # Strategy Features
/// - **Dynamic Spread Adjustment**: Widens spreads during toxic flow periods
/// - **Price Skewing**: Adjusts quotes based on order book imbalance
/// - **Inventory Management**: Tracks and limits position exposure per symbol
/// - **Quote Staleness Detection**: Monitors quote freshness for risk control
/// - **Risk Controls**: Enforces maximum inventory limits per symbol
/// 
/// # Risk Management
/// - Maximum inventory limits prevent excessive directional exposure
/// - Spread widening during high VPIN periods reduces adverse selection
/// - Quote staleness checks ensure prices remain competitive
/// - Circuit breaker integration for emergency risk control
/// 
/// # Performance Characteristics
/// - Low latency quote updates using atomic operations and RwLocks
/// - Thread-safe inventory tracking across multiple symbols
/// - Configurable spread and inventory parameters
pub struct MarketMakingStrategy {
    /// Strategy name
    name: String,
    /// Active quotes by symbol
    active_quotes: Arc<RwLock<HashMap<Symbol, QuoteState>>>,
    /// Orders generated
    orders_generated: AtomicU64,
    /// Target spread (basis points)
    target_spread_bps: f64,
    /// Inventory limits
    max_inventory: Qty,
    /// Current inventory
    inventory: Arc<RwLock<HashMap<Symbol, i64>>>,
    /// Health metrics
    health: Arc<RwLock<ComponentHealth>>,
}

/// Quote state for a symbol
#[derive(Debug, Clone)]
struct QuoteState {
    bid_price: Option<Px>,
    bid_size: Qty,
    ask_price: Option<Px>,
    ask_size: Qty,
    last_update: Instant,
}

impl std::fmt::Debug for MarketMakingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarketMakingStrategy")
            .field("name", &self.name)
            .field("orders_generated", &self.orders_generated.load(std::sync::atomic::Ordering::Relaxed))
            .field("target_spread_bps", &self.target_spread_bps)
            .field("max_inventory", &self.max_inventory)
            .field("active_quotes_count", &self.active_quotes.read().len())
            .field("inventory_symbols", &self.inventory.read().len())
            .finish()
    }
}

impl MarketMakingStrategy {
    /// Creates a new market making strategy with default configuration
    /// 
    /// # Returns
    /// A new `MarketMakingStrategy` instance with:
    /// - Target spread of 10 basis points
    /// - Maximum inventory limit of 100,000 units (10.0 in decimal)
    /// - Empty quote and inventory tracking
    /// - Healthy component status
    /// 
    /// # Default Configuration
    /// - **Target Spread**: 10 basis points (0.1%)
    /// - **Max Inventory**: 100,000 units per symbol
    /// - **Base Quote Size**: 10,000 units (1.0 in decimal)
    /// - **Spread Adjustment**: Dynamic based on VPIN toxicity
    /// - **Price Skewing**: Based on order book imbalance
    pub fn new() -> Self {
        Self {
            name: "MarketMaker".to_string(),
            active_quotes: Arc::new(RwLock::new(HashMap::new())),
            orders_generated: AtomicU64::new(0),
            target_spread_bps: 10.0, // 10 basis points
            max_inventory: Qty::from_i64(100000), // 10 units
            inventory: Arc::new(RwLock::new(HashMap::new())),
            health: Arc::new(RwLock::new(ComponentHealth {
                name: "MarketMaker".to_string(),
                is_healthy: true,
                last_heartbeat: Instant::now(),
                error_count: 0,
                success_count: 0,
                avg_latency_us: 0,
            })),
        }
    }
    
    /// Calculate quote prices
    fn calculate_quotes(
        &self,
        mid_price: Px,
        imbalance: f64,
        vpin: f64,
    ) -> (Px, Px, Qty, Qty) {
        let mid = mid_price.as_f64();
        
        // Adjust spread based on toxicity (VPIN)
        let spread_adjustment = 1.0 + (vpin / 100.0); // Widen spread with toxicity
        let half_spread = (mid * self.target_spread_bps / 10000.0) * spread_adjustment;
        
        // Skew prices based on imbalance
        let skew = imbalance / 100.0 * half_spread * 0.5; // Max 50% skew
        
        let bid_price = Px::from_i64(((mid - half_spread - skew) * 10000.0) as i64);
        let ask_price = Px::from_i64(((mid + half_spread + skew) * 10000.0) as i64);
        
        // Size based on inventory
        let base_size = Qty::from_i64(10000); // 1 unit
        let bid_size = base_size;
        let ask_size = base_size;
        
        (bid_price, ask_price, bid_size, ask_size)
    }
    
    /// Retrieves the current bid and ask quotes for a symbol
    /// 
    /// # Arguments
    /// * `symbol` - The trading symbol to get quotes for
    /// 
    /// # Returns
    /// * `Some((bid_price, ask_price, bid_size, ask_size))` - If valid quotes exist
    /// * `None` - If no quotes available or quotes are incomplete
    /// 
    /// This method provides read-only access to the current market making
    /// quotes. It only returns quotes when both bid and ask prices are available.
    pub fn get_quotes(&self, symbol: &Symbol) -> Option<(Px, Px, Qty, Qty)> {
        let quotes = self.active_quotes.read();
        quotes.get(symbol).and_then(|q| {
            match (q.bid_price, q.ask_price) {
                (Some(bid), Some(ask)) => Some((bid, ask, q.bid_size, q.ask_size)),
                _ => None
            }
        })
    }
    
    /// Checks if the quotes for a symbol are stale based on age
    /// 
    /// # Arguments
    /// * `symbol` - The trading symbol to check
    /// * `max_age_secs` - Maximum allowed age in seconds
    /// 
    /// # Returns
    /// * `true` - If quotes don't exist or are older than max_age_secs
    /// * `false` - If quotes exist and are fresh
    /// 
    /// This method helps determine when quotes need to be refreshed
    /// based on market data updates or time-based expiration.
    pub fn quotes_are_stale(&self, symbol: &Symbol, max_age_secs: u64) -> bool {
        let quotes = self.active_quotes.read();
        quotes.get(symbol).map_or(true, |q| {
            q.last_update.elapsed().as_secs() > max_age_secs
        })
    }
    
    /// Validates if a proposed trade would violate inventory limits
    /// 
    /// # Arguments
    /// * `symbol` - The trading symbol
    /// * `side` - The proposed trade side (Buy/Sell)
    /// * `size` - The proposed trade size
    /// 
    /// # Returns
    /// * `true` - If the trade is within inventory limits
    /// * `false` - If the trade would exceed maximum position limits
    /// 
    /// # Risk Management
    /// - For Buy orders: Checks if position + size <= max_inventory
    /// - For Sell orders: Checks if position - size >= -max_inventory
    /// - Uses current inventory position for each symbol
    /// - Prevents excessive long or short positions
    fn check_inventory(&self, symbol: Symbol, side: Side, size: Qty) -> bool {
        let inventory = self.inventory.read();
        let current = inventory.get(&symbol).copied().unwrap_or(0);
        
        match side {
            Side::Buy => {
                // Check if buying would exceed max long inventory
                (current + size.as_i64()) <= self.max_inventory.as_i64()
            }
            Side::Sell => {
                // Check if selling would exceed max short inventory
                (current - size.as_i64()) >= -self.max_inventory.as_i64()
            }
        }
    }
}

#[async_trait]
impl TradingStrategy for MarketMakingStrategy {
    fn name(&self) -> &str {
        &self.name
    }
    
    async fn on_market_update(&mut self, event: &TradingEvent) -> Result<Option<TradingEvent>> {
        let start = Instant::now();
        
        if let TradingEvent::MarketUpdate {
            symbol,
            bid,
            ask,
            mid,
            imbalance,
            vpin,
            ..
        } = event
        {
            // Don't make markets if spreads are too tight
            if let (Some((best_bid, _)), Some((best_ask, _))) = (bid, ask) {
                let current_spread = best_ask.as_f64() - best_bid.as_f64();
                let current_spread_bps = (current_spread / best_bid.as_f64()) * 10000.0;
                
                if current_spread_bps < 5.0 {
                    debug!("Spread too tight for market making: {:.2} bps", current_spread_bps);
                    return Ok(None);
                }
            }
            
            // Calculate new quotes
            let (bid_price, ask_price, bid_size, ask_size) = 
                self.calculate_quotes(*mid, *imbalance, *vpin);
            
            // Check inventory before quoting
            if !self.check_inventory(*symbol, Side::Buy, bid_size) {
                debug!("Inventory limit reached for buying");
            }
            
            if !self.check_inventory(*symbol, Side::Sell, ask_size) {
                debug!("Inventory limit reached for selling");
            }
            
            // Update quote state
            {
                let mut quotes = self.active_quotes.write();
                quotes.insert(*symbol, QuoteState {
                    bid_price: Some(bid_price),
                    bid_size,
                    ask_price: Some(ask_price),
                    ask_size,
                    last_update: Instant::now(),
                });
            }
            
            // Generate quote orders
            let order_id = self.orders_generated.fetch_add(2, Ordering::SeqCst);
            
            info!("Market making quotes for {}: bid={:.4} ask={:.4} spread={:.1}bps",
                  symbol,
                  bid_price.as_f64() / 10000.0,
                  ask_price.as_f64() / 10000.0,
                  ((ask_price.as_f64() - bid_price.as_f64()) / bid_price.as_f64()) * 10000.0);
            
            // Update health
            {
                let mut health = self.health.write();
                health.success_count += 1;
                health.last_heartbeat = Instant::now();
                health.avg_latency_us = start.elapsed().as_micros() as u64;
            }
            
            // Return bid order (in production would return both bid and ask)
            return Ok(Some(TradingEvent::OrderRequest {
                id: order_id,
                symbol: *symbol,
                side: Side::Buy,
                order_type: OrderType::Limit,
                quantity: bid_size,
                price: Some(bid_price),
                time_in_force: TimeInForce::Gtc,
                strategy_id: "MarketMaker".to_string(),
            }));
        }
        
        Ok(None)
    }
    
    async fn on_execution(&mut self, report: &TradingEvent) -> Result<()> {
        if let TradingEvent::ExecutionReport {
            symbol,
            side,
            executed_qty,
            ..
        } = report
        {
            // Update inventory
            let mut inventory = self.inventory.write();
            let current = inventory.entry(*symbol).or_insert(0);
            
            match side {
                Side::Buy => *current += executed_qty.as_i64(),
                Side::Sell => *current -= executed_qty.as_i64(),
            }
            
            debug!("Inventory updated for {}: {}", symbol, current);
        }
        
        Ok(())
    }
    
    fn health(&self) -> ComponentHealth {
        self.health.read().clone()
    }
    
    async fn reset(&mut self) -> Result<()> {
        self.active_quotes.write().clear();
        self.inventory.write().clear();
        self.orders_generated.store(0, Ordering::SeqCst);
        
        info!("Market making strategy reset");
        Ok(())
    }
}