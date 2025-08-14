//! Risk Manager Service
//!
//! Centralized risk management for all trading operations:
//! - Position limits and exposure tracking
//! - Loss limits and drawdown control
//! - Rate limiting and circuit breakers
//! - Kill switch and emergency stop
//! - Multi-strategy risk aggregation

pub mod circuit_breaker;
pub mod config;
pub mod limits;
pub mod monitor;

use anyhow::Result;
use async_trait::async_trait;
use common::{Px, Qty, Side, Symbol};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, Ordering};
use tracing::{error, info, warn};

/// Risk check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskCheckResult {
    /// Order approved
    Approved,
    /// Order rejected with reason
    Rejected(String),
    /// Order requires manual approval
    RequiresApproval(String),
}

/// Position information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// Symbol
    pub symbol: Symbol,
    /// Net position (positive = long, negative = short)
    pub net_qty: i64,
    /// Average entry price
    pub avg_price: Px,
    /// Current market price
    pub mark_price: Px,
    /// Unrealized PnL
    pub unrealized_pnl: i64,
    /// Realized PnL
    pub realized_pnl: i64,
    /// Position value
    pub position_value: u64,
}

/// Risk metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RiskMetrics {
    /// Total exposure across all positions
    pub total_exposure: u64,
    /// Current drawdown from peak (fixed-point: 1000 = 10%)
    pub current_drawdown: i32,
    /// Daily PnL
    pub daily_pnl: i64,
    /// Number of open positions
    pub open_positions: u32,
    /// Number of orders today
    pub orders_today: u32,
    /// Circuit breaker status
    pub circuit_breaker_active: bool,
    /// Kill switch status
    pub kill_switch_active: bool,
}

/// Risk limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLimits {
    /// Maximum position size per symbol
    pub max_position_size: u64,
    /// Maximum position value per symbol
    pub max_position_value: u64,
    /// Maximum total exposure
    pub max_total_exposure: u64,
    /// Maximum order size
    pub max_order_size: u64,
    /// Maximum order value
    pub max_order_value: u64,
    /// Maximum orders per minute
    pub max_orders_per_minute: u32,
    /// Maximum daily loss
    pub max_daily_loss: i64,
    /// Maximum drawdown percentage (fixed-point: 1000 = 10%)
    pub max_drawdown_pct: i32,
    /// Circuit breaker threshold (consecutive losses)
    pub circuit_breaker_threshold: u32,
    /// Circuit breaker cooldown (seconds)
    pub circuit_breaker_cooldown: u64,
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            max_position_size: 10_000_000,     // 1000 units
            max_position_value: 100_000_000,   // 10M in value
            max_total_exposure: 1_000_000_000, // 100M total
            max_order_size: 1_000_000,         // 100 units
            max_order_value: 10_000_000,       // 1M per order
            max_orders_per_minute: 100,
            max_daily_loss: -5_000_000,    // 500K loss limit
            max_drawdown_pct: 1000,        // 10% drawdown in fixed-point
            circuit_breaker_threshold: 5,  // 5 consecutive losses
            circuit_breaker_cooldown: 300, // 5 minutes
        }
    }
}

/// Risk manager trait
#[async_trait]
pub trait RiskManager: Send + Sync {
    /// Check if order can be placed
    async fn check_order(&self, symbol: Symbol, side: Side, qty: Qty, price: Px)
    -> RiskCheckResult;

    /// Update position after fill
    async fn update_position(
        &mut self,
        symbol: Symbol,
        side: Side,
        qty: Qty,
        price: Px,
    ) -> Result<()>;

    /// Get current position
    async fn get_position(&self, symbol: Symbol) -> Option<Position>;

    /// Get all positions
    async fn get_all_positions(&self) -> Vec<Position>;

    /// Get risk metrics
    async fn get_metrics(&self) -> RiskMetrics;

    /// Update market prices
    async fn update_mark_price(&mut self, symbol: Symbol, price: Px) -> Result<()>;

    /// Trigger kill switch
    async fn activate_kill_switch(&mut self, reason: &str) -> Result<()>;

    /// Reset kill switch
    async fn deactivate_kill_switch(&mut self) -> Result<()>;

    /// Reset daily metrics
    async fn reset_daily_metrics(&mut self) -> Result<()>;
}

/// Per-symbol risk tracking
struct SymbolRisk {
    position: RwLock<Position>,
    orders_sent: AtomicU64,
    _last_order_time: AtomicU64, // Reserved for symbol-specific rate limiting
    consecutive_losses: AtomicU32,
    circuit_breaker_until: AtomicU64,
}

impl SymbolRisk {
    fn new(symbol: Symbol) -> Self {
        Self {
            position: RwLock::new(Position {
                symbol,
                net_qty: 0,
                avg_price: Px::ZERO,
                mark_price: Px::ZERO,
                unrealized_pnl: 0,
                realized_pnl: 0,
                position_value: 0,
            }),
            orders_sent: AtomicU64::new(0),
            _last_order_time: AtomicU64::new(0),
            consecutive_losses: AtomicU32::new(0),
            circuit_breaker_until: AtomicU64::new(0),
        }
    }
}

/// Risk manager service implementation
pub struct RiskManagerService {
    /// Risk limits
    limits: RiskLimits,
    /// Per-symbol tracking
    symbol_risks: Arc<DashMap<Symbol, Arc<SymbolRisk>>>,
    /// Global metrics
    total_exposure: AtomicU64,
    daily_pnl: AtomicI64,
    peak_value: AtomicU64,
    orders_today: AtomicU32,
    /// Kill switch
    kill_switch: AtomicBool,
    /// Order timestamps for rate limiting
    order_timestamps: Arc<RwLock<Vec<u64>>>,
}

impl RiskManagerService {
    /// Create new risk manager
    pub fn new(limits: RiskLimits) -> Self {
        Self {
            limits,
            symbol_risks: Arc::new(DashMap::new()),
            total_exposure: AtomicU64::new(0),
            daily_pnl: AtomicI64::new(0),
            peak_value: AtomicU64::new(0),
            orders_today: AtomicU32::new(0),
            kill_switch: AtomicBool::new(false),
            order_timestamps: Arc::new(RwLock::new(Vec::with_capacity(1000))),
        }
    }

    /// Check rate limits
    fn check_rate_limit(&self) -> bool {
        let now = u64::try_from(chrono::Utc::now().timestamp_millis().max(0)).unwrap_or(0);
        let window_start = now.saturating_sub(60_000); // 1 minute window

        let timestamps = self.order_timestamps.read();
        let recent_orders = timestamps.iter().filter(|&&ts| ts > window_start).count();

        recent_orders < usize::try_from(self.limits.max_orders_per_minute).unwrap_or(usize::MAX)
    }

    /// Add order timestamp
    fn add_order_timestamp(&self) {
        let now = u64::try_from(chrono::Utc::now().timestamp_millis().max(0)).unwrap_or(0);
        let mut timestamps = self.order_timestamps.write();

        // Keep only recent timestamps (last 2 minutes)
        let cutoff = now.saturating_sub(120_000);
        timestamps.retain(|&ts| ts > cutoff);

        timestamps.push(now);
        self.orders_today.fetch_add(1, Ordering::Relaxed);
    }

    /// Get or create symbol risk
    fn get_symbol_risk(&self, symbol: Symbol) -> Arc<SymbolRisk> {
        self.symbol_risks
            .entry(symbol)
            .or_insert_with(|| Arc::new(SymbolRisk::new(symbol)))
            .clone()
    }
}

#[async_trait]
impl RiskManager for RiskManagerService {
    async fn check_order(
        &self,
        symbol: Symbol,
        side: Side,
        qty: Qty,
        price: Px,
    ) -> RiskCheckResult {
        // Check kill switch
        if self.kill_switch.load(Ordering::Relaxed) {
            warn!("Order rejected for {:?}: Kill switch active", symbol);
            return RiskCheckResult::Rejected("Kill switch active".to_string());
        }

        // Check rate limits
        if !self.check_rate_limit() {
            warn!(
                "Order rejected for {:?}: Rate limit exceeded ({} orders/min)",
                symbol, self.limits.max_orders_per_minute
            );
            return RiskCheckResult::Rejected(format!(
                "Rate limit exceeded: {} orders/min",
                self.limits.max_orders_per_minute
            ));
        }

        // Check order size limits
        let order_qty = qty.as_i64().unsigned_abs();
        if order_qty > self.limits.max_order_size {
            warn!(
                "Order rejected for {:?}: Size {} exceeds limit {}",
                symbol, order_qty, self.limits.max_order_size
            );
            return RiskCheckResult::Rejected(format!(
                "Order size {} exceeds limit {}",
                order_qty, self.limits.max_order_size
            ));
        }

        // Check order value limits
        let order_value = (price.as_i64().unsigned_abs() * order_qty) / 10000;
        if order_value > self.limits.max_order_value {
            warn!(
                "Order rejected for {:?}: Value {} exceeds limit {}",
                symbol, order_value, self.limits.max_order_value
            );
            return RiskCheckResult::Rejected(format!(
                "Order value {} exceeds limit {}",
                order_value, self.limits.max_order_value
            ));
        }

        // Check symbol-specific limits
        let symbol_risk = self.get_symbol_risk(symbol);

        // Check circuit breaker
        let now = u64::try_from(chrono::Utc::now().timestamp().max(0)).unwrap_or(0);
        let breaker_until = symbol_risk.circuit_breaker_until.load(Ordering::Relaxed);
        if breaker_until > now {
            return RiskCheckResult::Rejected(format!(
                "Circuit breaker active for {} more seconds",
                breaker_until - now
            ));
        }

        // Check position limits
        let position = symbol_risk.position.read();
        let new_position = match side {
            Side::Bid => position.net_qty + qty.as_i64(),
            Side::Ask => position.net_qty - qty.as_i64(),
        };

        if new_position.unsigned_abs() > self.limits.max_position_size {
            return RiskCheckResult::Rejected(format!(
                "Position size {} would exceed limit {}",
                new_position.unsigned_abs(),
                self.limits.max_position_size
            ));
        }

        // Check exposure limits
        let total_exposure = self.total_exposure.load(Ordering::Relaxed);
        let additional_exposure = order_value;

        if total_exposure + additional_exposure > self.limits.max_total_exposure {
            return RiskCheckResult::Rejected(format!(
                "Total exposure {} would exceed limit {}",
                total_exposure + additional_exposure,
                self.limits.max_total_exposure
            ));
        }

        // Check daily loss limit
        let daily_pnl = self.daily_pnl.load(Ordering::Relaxed);
        if daily_pnl < self.limits.max_daily_loss {
            return RiskCheckResult::RequiresApproval(format!(
                "Daily loss {} exceeds limit {}",
                daily_pnl, self.limits.max_daily_loss
            ));
        }

        // All checks passed
        self.add_order_timestamp();
        RiskCheckResult::Approved
    }

    async fn update_position(
        &mut self,
        symbol: Symbol,
        side: Side,
        qty: Qty,
        price: Px,
    ) -> Result<()> {
        let symbol_risk = self.get_symbol_risk(symbol);
        let mut position = symbol_risk.position.write();

        let fill_qty = qty.as_i64();
        let fill_price = price.as_i64();

        // Update position
        let old_qty = position.net_qty;
        position.net_qty = match side {
            Side::Bid => old_qty + fill_qty,
            Side::Ask => old_qty - fill_qty,
        };

        // Update average price
        if position.net_qty != 0 {
            if old_qty == 0 {
                position.avg_price = price;
            } else if (old_qty > 0 && side == Side::Bid) || (old_qty < 0 && side == Side::Ask) {
                // Adding to position
                let total_value = old_qty * position.avg_price.as_i64() + fill_qty * fill_price;
                position.avg_price = Px::from_i64(total_value / position.net_qty);
            }
        }

        // Update exposure
        let new_value =
            (position.net_qty.unsigned_abs() * position.mark_price.as_i64().unsigned_abs()) / 10000;
        let old_value = position.position_value;
        position.position_value = new_value;

        self.total_exposure
            .fetch_add(new_value.wrapping_sub(old_value), Ordering::Relaxed);

        info!(
            "Updated position for {:?}: {} @ {}",
            symbol, position.net_qty, position.avg_price
        );

        Ok(())
    }

    async fn get_position(&self, symbol: Symbol) -> Option<Position> {
        self.symbol_risks
            .get(&symbol)
            .map(|risk| risk.position.read().clone())
    }

    async fn get_all_positions(&self) -> Vec<Position> {
        self.symbol_risks
            .iter()
            .map(|entry| entry.position.read().clone())
            .collect()
    }

    async fn get_metrics(&self) -> RiskMetrics {
        let total_exposure = self.total_exposure.load(Ordering::Relaxed);
        let daily_pnl = self.daily_pnl.load(Ordering::Relaxed);
        let peak = self.peak_value.load(Ordering::Relaxed);

        // Calculate drawdown in fixed-point (10000 = 100%)
        let current_drawdown = if peak > 0 && peak > total_exposure {
            let diff = peak - total_exposure;
            // Calculate percentage: (diff * 10000) / peak with overflow protection
            let percentage = diff.saturating_mul(10000) / peak;
            i32::try_from(percentage).unwrap_or(i32::MAX)
        } else {
            0
        };

        RiskMetrics {
            total_exposure,
            current_drawdown,
            daily_pnl,
            open_positions: u32::try_from(self.symbol_risks.len()).unwrap_or(u32::MAX),
            orders_today: self.orders_today.load(Ordering::Relaxed),
            circuit_breaker_active: false,
            kill_switch_active: self.kill_switch.load(Ordering::Relaxed),
        }
    }

    async fn update_mark_price(&mut self, symbol: Symbol, price: Px) -> Result<()> {
        if let Some(risk) = self.symbol_risks.get(&symbol) {
            let mut position = risk.position.write();
            position.mark_price = price;

            // Update unrealized PnL
            if position.net_qty != 0 {
                let mark_value = position.net_qty * price.as_i64();
                let cost_basis = position.net_qty * position.avg_price.as_i64();
                position.unrealized_pnl = (mark_value - cost_basis) / 10000;
            }
        }
        Ok(())
    }

    async fn activate_kill_switch(&mut self, reason: &str) -> Result<()> {
        self.kill_switch.store(true, Ordering::Relaxed);
        error!("KILL SWITCH ACTIVATED: {}", reason);
        Ok(())
    }

    async fn deactivate_kill_switch(&mut self) -> Result<()> {
        self.kill_switch.store(false, Ordering::Relaxed);
        info!("Kill switch deactivated");
        Ok(())
    }

    async fn reset_daily_metrics(&mut self) -> Result<()> {
        self.daily_pnl.store(0, Ordering::Relaxed);
        self.orders_today.store(0, Ordering::Relaxed);
        self.order_timestamps.write().clear();

        // Reset symbol-specific metrics
        for entry in self.symbol_risks.iter() {
            entry.orders_sent.store(0, Ordering::Relaxed);
            entry.consecutive_losses.store(0, Ordering::Relaxed);
        }

        info!("Daily risk metrics reset");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_risk_limits() {
        let limits = RiskLimits::default();
        let risk_manager = RiskManagerService::new(limits);

        // Test order within limits
        let result = risk_manager
            .check_order(
                Symbol::from(1),
                Side::Bid,
                Qty::from_qty_i32(100_0000),
                Px::from_price_i32(100_0000),
            )
            .await;

        assert!(matches!(result, RiskCheckResult::Approved));
    }

    #[tokio::test]
    async fn test_kill_switch() {
        let mut risk_manager = RiskManagerService::new(RiskLimits::default());

        // Activate kill switch
        risk_manager.activate_kill_switch("Test reason").await.ok();

        // Check order should be rejected
        let result = risk_manager
            .check_order(
                Symbol::from(1),
                Side::Bid,
                Qty::from_qty_i32(100_0000),
                Px::from_price_i32(100_0000),
            )
            .await;

        assert!(matches!(result, RiskCheckResult::Rejected(_)));
    }
}
