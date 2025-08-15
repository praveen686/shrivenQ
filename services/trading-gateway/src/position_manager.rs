//! Position Manager - Real-time position and P&L tracking

use anyhow::Result;
use common::{Px, Qty, Symbol};
use dashmap::DashMap;
use std::sync::Arc;
use tracing::{debug, info};

/// Position information
#[derive(Debug, Clone)]
pub struct PositionInfo {
    /// Symbol
    pub symbol: Symbol,
    /// Net quantity (positive = long, negative = short)
    pub quantity: i64,
    /// Average entry price
    pub avg_entry_price: i64,
    /// Current market price
    pub current_price: i64,
    /// Unrealized P&L
    pub unrealized_pnl: i64,
    /// Realized P&L
    pub realized_pnl: i64,
}

/// Position manager
pub struct PositionManager {
    /// Positions by symbol
    positions: Arc<DashMap<Symbol, PositionInfo>>,
}

impl PositionManager {
    /// Create new position manager
    pub fn new() -> Self {
        Self {
            positions: Arc::new(DashMap::new()),
        }
    }
    
    /// Update position after trade
    pub async fn update_position(
        &self,
        symbol: Symbol,
        side: crate::Side,
        quantity: Qty,
        price: Px,
    ) -> Result<()> {
        let mut position = self.positions.entry(symbol).or_insert(PositionInfo {
            symbol,
            quantity: 0,
            avg_entry_price: 0,
            current_price: price.as_i64(),
            unrealized_pnl: 0,
            realized_pnl: 0,
        });
        
        let qty = quantity.as_i64();
        let px = price.as_i64();
        
        // Update position
        match side {
            crate::Side::Buy => {
                if position.quantity >= 0 {
                    // Adding to long position
                    let total_value = position.avg_entry_price * position.quantity + px * qty;
                    position.quantity += qty;
                    if position.quantity > 0 {
                        position.avg_entry_price = total_value / position.quantity;
                    }
                } else {
                    // Closing short position
                    let closed_qty = qty.min(-position.quantity);
                    let pnl = closed_qty * (position.avg_entry_price - px) / 10000;
                    position.realized_pnl += pnl;
                    position.quantity += qty;
                    
                    if position.quantity > 0 {
                        position.avg_entry_price = px;
                    }
                }
            }
            crate::Side::Sell => {
                if position.quantity <= 0 {
                    // Adding to short position
                    let total_value = position.avg_entry_price * (-position.quantity) + px * qty;
                    position.quantity -= qty;
                    if position.quantity < 0 {
                        position.avg_entry_price = total_value / (-position.quantity);
                    }
                } else {
                    // Closing long position
                    let closed_qty = qty.min(position.quantity);
                    let pnl = closed_qty * (px - position.avg_entry_price) / 10000;
                    position.realized_pnl += pnl;
                    position.quantity -= qty;
                    
                    if position.quantity < 0 {
                        position.avg_entry_price = px;
                    }
                }
            }
        }
        
        // Update current price and unrealized P&L
        position.current_price = px;
        if position.quantity != 0 {
            position.unrealized_pnl = if position.quantity > 0 {
                position.quantity * (px - position.avg_entry_price) / 10000
            } else {
                -position.quantity * (position.avg_entry_price - px) / 10000
            };
        } else {
            position.unrealized_pnl = 0;
        }
        
        debug!("Updated position for {}: qty={} avg_price={} pnl={}", 
               symbol, position.quantity, position.avg_entry_price, position.unrealized_pnl);
        
        Ok(())
    }
    
    /// Update market price for position
    pub async fn update_market_price(&self, symbol: Symbol, price: Px) {
        if let Some(mut position) = self.positions.get_mut(&symbol) {
            position.current_price = price.as_i64();
            
            // Recalculate unrealized P&L
            if position.quantity != 0 {
                position.unrealized_pnl = if position.quantity > 0 {
                    position.quantity * (price.as_i64() - position.avg_entry_price) / 10000
                } else {
                    -position.quantity * (position.avg_entry_price - price.as_i64()) / 10000
                };
            }
        }
    }
    
    /// Get position for symbol
    pub async fn get_position(&self, symbol: Symbol) -> Option<PositionInfo> {
        self.positions.get(&symbol).map(|p| p.clone())
    }
    
    /// Get all positions
    pub async fn get_all_positions(&self) -> Vec<(Symbol, PositionInfo)> {
        self.positions
            .iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect()
    }
    
    /// Get position count
    pub async fn get_position_count(&self) -> usize {
        self.positions.len()
    }
    
    /// Close all positions
    pub async fn close_all_positions(&self) -> Result<()> {
        info!("Closing all {} positions", self.positions.len());
        
        // In production, would submit market orders to close each position
        // For now, just clear the positions
        self.positions.clear();
        
        Ok(())
    }
    
    /// Get total P&L
    pub async fn get_total_pnl(&self) -> (i64, i64) {
        let mut total_unrealized = 0i64;
        let mut total_realized = 0i64;
        
        for entry in self.positions.iter() {
            total_unrealized += entry.unrealized_pnl;
            total_realized += entry.realized_pnl;
        }
        
        (total_unrealized, total_realized)
    }
}