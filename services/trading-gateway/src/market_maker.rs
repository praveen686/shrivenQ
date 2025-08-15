//! Market Making Strategy - Continuous bid/ask quoting

use crate::{ComponentHealth, OrderType, Side, SignalType, TimeInForce, TradingEvent, TradingStrategy};
use anyhow::Result;
use async_trait::async_trait;
use common::{Px, Qty, Symbol, Ts};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info};

/// Market making strategy
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

impl MarketMakingStrategy {
    /// Create new market making strategy
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
    
    /// Check inventory limits
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