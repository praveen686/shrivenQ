//! Position Manager - Real-time position and P&L tracking

use anyhow::Result;
use services_common::{Px, Qty, Symbol};
use dashmap::DashMap;
use std::sync::Arc;
use tracing::{debug, info};

/// Comprehensive position information for a trading symbol
///
/// Tracks complete position state including quantity, pricing, and profit/loss
/// calculations. Maintains both realized and unrealized P&L for accurate
/// portfolio valuation and risk assessment.
///
/// # Position Tracking
/// - Net position (positive = long, negative = short)
/// - Average entry price calculation
/// - Real-time P&L computation
/// - Current market price tracking
///
/// # Price and P&L Calculations
/// - Average entry prices are quantity-weighted
/// - Unrealized P&L uses current market prices
/// - Realized P&L from closed portions of positions
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

/// Real-time position and profit/loss management system
///
/// Maintains accurate position tracking across all trading symbols with
/// real-time P&L calculations. Handles complex position updates including
/// partial closes, position reversals, and average price calculations.
///
/// # Features
/// - Thread-safe concurrent position updates
/// - Accurate average price calculation for multiple fills
/// - Separate tracking of realized vs unrealized P&L
/// - Real-time market price updates
/// - Portfolio-level P&L aggregation
///
/// # Use Cases
/// - Risk management position monitoring
/// - Portfolio valuation and reporting
/// - Trade settlement and accounting
/// - Position-based order validation
///
/// # Example
/// ```rust
/// let position_manager = PositionManager::new();
/// position_manager.update_position(symbol, Side::Buy, qty, price).await?;
/// let total_pnl = position_manager.get_total_pnl().await;
/// ```
pub struct PositionManager {
    /// Positions by symbol
    positions: Arc<DashMap<Symbol, PositionInfo>>,
}

impl std::fmt::Debug for PositionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PositionManager")
            .field("positions_count", &self.positions.len())
            .field("positions", &format!("{} active positions", self.positions.len()))
            .finish()
    }
}

impl PositionManager {
    /// Creates a new position manager instance
    ///
    /// Initializes an empty position tracking system ready for trade updates.
    /// All positions start at zero with no P&L.
    ///
    /// # Returns
    /// A new PositionManager with empty position state
    ///
    /// # Example
    /// ```rust
    /// let position_manager = PositionManager::new();
    /// ```
    pub fn new() -> Self {
        Self {
            positions: Arc::new(DashMap::new()),
        }
    }
    
    /// Updates position state after a trade execution
    ///
    /// Handles complex position mathematics including average price calculations,
    /// position additions/reductions, and realized P&L computation for partial closes.
    /// Supports both position building and closing scenarios.
    ///
    /// # Arguments
    /// * `symbol` - Trading symbol for the executed trade
    /// * `side` - Buy or Sell side of the execution
    /// * `quantity` - Executed quantity
    /// * `price` - Execution price
    ///
    /// # Returns
    /// - `Ok(())` if position update succeeds
    /// - `Err` if update calculation fails
    ///
    /// # Position Logic
    /// - **Adding to position**: Updates quantity-weighted average price
    /// - **Closing position**: Calculates realized P&L for closed portion
    /// - **Position reversal**: Handles transition from long to short or vice versa
    ///
    /// # Example
    /// ```rust
    /// // Buy 100 shares at $50
    /// position_manager.update_position(
    ///     Symbol::from("AAPL"),
    ///     Side::Buy,
    ///     Qty::from(100),
    ///     Px::from_f64(50.0)
    /// ).await?;
    /// ```
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
    
    /// Updates current market price and recalculates unrealized P&L
    ///
    /// Updates the current market price for a position and automatically
    /// recalculates unrealized profit/loss based on the new price.
    ///
    /// # Arguments
    /// * `symbol` - Trading symbol to update
    /// * `price` - New current market price
    ///
    /// # Example
    /// ```rust
    /// // Update BTCUSDT price to $45,000
    /// position_manager.update_market_price(
    ///     Symbol::from("BTCUSDT"),
    ///     Px::from_f64(45000.0)
    /// ).await;
    /// ```
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
    
    /// Retrieves current position information for a specific symbol
    ///
    /// # Arguments
    /// * `symbol` - Trading symbol to query
    ///
    /// # Returns
    /// - `Some(PositionInfo)` if position exists
    /// - `None` if no position found for symbol
    ///
    /// # Example
    /// ```rust
    /// if let Some(position) = position_manager.get_position(symbol).await {
    ///     println!("Position: {} shares, P&L: ${}",
    ///              position.quantity, position.unrealized_pnl);
    /// }
    /// ```
    pub async fn get_position(&self, symbol: Symbol) -> Option<PositionInfo> {
        self.positions.get(&symbol).map(|p| p.clone())
    }
    
    /// Returns all current positions across all symbols
    ///
    /// Provides complete portfolio view with all active positions.
    /// Useful for portfolio reporting, risk analysis, and position monitoring.
    ///
    /// # Returns
    /// Vector of (Symbol, PositionInfo) tuples for all tracked positions
    ///
    /// # Example
    /// ```rust
    /// let all_positions = position_manager.get_all_positions().await;
    /// for (symbol, position) in all_positions {
    ///     println!("{}: {} shares", symbol, position.quantity);
    /// }
    /// ```
    pub async fn get_all_positions(&self) -> Vec<(Symbol, PositionInfo)> {
        self.positions
            .iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect()
    }
    
    /// Returns the total number of symbols with active positions
    ///
    /// # Returns
    /// Count of symbols currently being tracked
    ///
    /// # Example
    /// ```rust
    /// let count = position_manager.get_position_count().await;
    /// println!("Tracking {} different symbols", count);
    /// ```
    pub async fn get_position_count(&self) -> usize {
        self.positions.len()
    }
    
    /// Closes all positions (emergency function)
    ///
    /// Emergency function to clear all position tracking. In production,
    /// this would submit market orders to close each position. Currently
    /// only clears the tracking without actual trading.
    ///
    /// # Returns
    /// - `Ok(())` if all positions are cleared successfully
    /// - `Err` if clearing fails
    ///
    /// # Warning
    /// This is an emergency function that clears position tracking without
    /// actual trade execution. Use with caution.
    ///
    /// # Example
    /// ```rust
    /// // Emergency shutdown
    /// position_manager.close_all_positions().await?;
    /// ```
    pub async fn close_all_positions(&self) -> Result<()> {
        info!("Closing all {} positions", self.positions.len());
        
        // In production, would submit market orders to close each position
        // For now, just clear the positions
        self.positions.clear();
        
        Ok(())
    }
    
    /// Calculates total portfolio profit and loss
    ///
    /// Aggregates P&L across all positions to provide portfolio-level
    /// profit/loss summary. Returns both unrealized and realized P&L.
    ///
    /// # Returns
    /// Tuple of (unrealized_pnl, realized_pnl) in base currency units
    ///
    /// # Example
    /// ```rust
    /// let (unrealized, realized) = position_manager.get_total_pnl().await;
    /// let total_pnl = unrealized + realized;
    /// println!("Portfolio P&L: Unrealized ${}, Realized ${}, Total ${}",
    ///          unrealized, realized, total_pnl);
    /// ```
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