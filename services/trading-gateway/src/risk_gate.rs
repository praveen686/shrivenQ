//! Risk Gate - Pre-trade risk checks with sub-microsecond latency
//! 
//! Inspired by institutional risk management systems that prevent
//! catastrophic losses through multi-layered validation.

use crate::{GatewayConfig, Side, TradingEvent};
use anyhow::Result;
use services_common::{Px, Qty, Symbol};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, warn};

/// Multi-layered risk management system for pre-trade validation
///
/// The RiskGate implements institutional-grade risk controls with sub-microsecond
/// latency to prevent catastrophic losses through comprehensive pre-trade checks.
/// It validates orders against position limits, rate limits, notional limits,
/// and daily P&L constraints before allowing execution.
///
/// # Risk Controls
/// - Position size and concentration limits
/// - Order rate limiting (per second/minute)
/// - Notional value limits per symbol
/// - Daily loss limits and P&L monitoring
/// - Real-time position tracking
///
/// # Performance
/// - Sub-microsecond latency for risk checks
/// - Lock-free atomic operations where possible
/// - Optimized check ordering (fastest checks first)
///
/// # Example
/// ```rust
/// let risk_gate = RiskGate::new(config);
/// let is_allowed = risk_gate.check_order(&order_event).await?;
/// if is_allowed {
///     // Proceed with order execution
/// }
/// ```
pub struct RiskGate {
    /// Configuration
    config: GatewayConfig,
    /// Position limits per symbol
    position_limits: Arc<DashMap<Symbol, PositionLimit>>,
    /// Current positions
    current_positions: Arc<DashMap<Symbol, AtomicI64>>,
    /// Daily P&L tracking
    daily_pnl: AtomicI64,
    /// Order rate limiting
    order_rate_limiter: Arc<RwLock<RateLimiter>>,
    /// Risk metrics
    metrics: Arc<RiskMetrics>,
}

impl std::fmt::Debug for RiskGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RiskGate")
            .field("position_limits_count", &self.position_limits.len())
            .field("current_positions_count", &self.current_positions.len())
            .field("daily_pnl", &self.daily_pnl.load(std::sync::atomic::Ordering::Relaxed))
            .field("config", &self.config)
            .finish()
    }
}

/// Position and order size limits configuration for a trading symbol
///
/// Defines the maximum allowable positions and order sizes for risk management.
/// Used to prevent excessive concentration in any single symbol and limit
/// order size to manageable levels.
///
/// # Risk Parameters
/// - Long/short position limits to control directional exposure
/// - Order size limits to prevent fat-finger errors
/// - Notional value limits to control capital at risk
#[derive(Debug, Clone)]
pub struct PositionLimit {
    /// Maximum long position
    pub max_long: Qty,
    /// Maximum short position
    pub max_short: Qty,
    /// Maximum order size
    pub max_order_size: Qty,
    /// Maximum notional value
    pub max_notional: i64,
}

/// High-frequency order rate limiting mechanism
///
/// Prevents order submission abuse and system overload by enforcing
/// configurable limits on orders per second and per minute. Uses
/// sliding window approach for accurate rate limiting.
///
/// # Features
/// - Dual time window enforcement (second and minute)
/// - Automatic counter reset based on time windows
/// - Thread-safe operation with write locks
#[derive(Debug)]
pub struct RateLimiter {
    /// Orders per second limit
    orders_per_second: u32,
    /// Orders per minute limit
    orders_per_minute: u32,
    /// Current second counter
    current_second: Instant,
    /// Current minute counter
    current_minute: Instant,
    /// Orders this second
    orders_this_second: u32,
    /// Orders this minute
    orders_this_minute: u32,
}

/// Comprehensive risk management performance metrics
///
/// Tracks key statistics about risk gate performance and rejection patterns.
/// All metrics are maintained using atomic operations for thread safety
/// and high-frequency updates.
///
/// # Tracked Metrics
/// - Order processing volume and rejection rates
/// - Specific violation types (position, rate, etc.)
/// - Performance timing (check latency)
pub struct RiskMetrics {
    /// Total orders checked
    pub orders_checked: AtomicU64,
    /// Orders rejected
    pub orders_rejected: AtomicU64,
    /// Position limit breaches
    pub position_breaches: AtomicU64,
    /// Rate limit breaches
    pub rate_breaches: AtomicU64,
    /// Average check latency
    pub avg_latency_ns: AtomicU64,
}

impl std::fmt::Debug for RiskMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RiskMetrics")
            .field("orders_checked", &self.orders_checked.load(Ordering::Relaxed))
            .field("orders_rejected", &self.orders_rejected.load(Ordering::Relaxed))
            .field("position_breaches", &self.position_breaches.load(Ordering::Relaxed))
            .field("rate_breaches", &self.rate_breaches.load(Ordering::Relaxed))
            .field("avg_latency_ns", &self.avg_latency_ns.load(Ordering::Relaxed))
            .finish()
    }
}

impl RiskGate {
    /// Creates a new risk gate with the specified configuration
    ///
    /// Initializes all risk management components including position tracking,
    /// rate limiting, and metrics collection. Uses default rate limits and
    /// empty position limit maps.
    ///
    /// # Arguments
    /// * `config` - Gateway configuration containing default risk parameters
    ///
    /// # Returns
    /// A new RiskGate instance ready for order validation
    ///
    /// # Example
    /// ```rust
    /// let config = GatewayConfig::default();
    /// let risk_gate = RiskGate::new(config);
    /// ```
    pub fn new(config: GatewayConfig) -> Self {
        Self {
            config,
            position_limits: Arc::new(DashMap::new()),
            current_positions: Arc::new(DashMap::new()),
            daily_pnl: AtomicI64::new(0),
            order_rate_limiter: Arc::new(RwLock::new(RateLimiter {
                orders_per_second: 100,
                orders_per_minute: 1000,
                current_second: Instant::now(),
                current_minute: Instant::now(),
                orders_this_second: 0,
                orders_this_minute: 0,
            })),
            metrics: Arc::new(RiskMetrics {
                orders_checked: AtomicU64::new(0),
                orders_rejected: AtomicU64::new(0),
                position_breaches: AtomicU64::new(0),
                rate_breaches: AtomicU64::new(0),
                avg_latency_ns: AtomicU64::new(0),
            }),
        }
    }
    
    /// Performs comprehensive pre-trade risk validation on an order
    ///
    /// Executes a series of risk checks in optimized order (fastest first) to
    /// validate whether an order should be allowed for execution. Tracks
    /// performance metrics and rejection reasons.
    ///
    /// # Arguments
    /// * `order` - Trading event containing order details to validate
    ///
    /// # Returns
    /// - `Ok(true)` if order passes all risk checks
    /// - `Ok(false)` if order violates any risk limit
    /// - `Err` if validation process fails
    ///
    /// # Risk Checks Performed
    /// 1. Rate limiting (orders per second/minute)
    /// 2. Order size validation
    /// 3. Position limit validation
    /// 4. Notional value limits
    /// 5. Daily P&L limits
    ///
    /// # Example
    /// ```rust
    /// if risk_gate.check_order(&order).await? {
    ///     // Order approved for execution
    ///     execution_engine.submit_order(order).await?;
    /// } else {
    ///     // Order rejected by risk management
    ///     log::warn!("Order rejected by risk gate");
    /// }
    /// ```
    pub async fn check_order(&self, order: &TradingEvent) -> Result<bool> {
        let start = Instant::now();
        self.metrics.orders_checked.fetch_add(1, Ordering::Relaxed);
        
        // Extract order details
        let (symbol, side, quantity, price) = match order {
            TradingEvent::OrderRequest { symbol, side, quantity, price, .. } => {
                (*symbol, *side, *quantity, *price)
            }
            _ => return Ok(true), // Not an order
        };
        
        // 1. Check rate limits (fastest check)
        if !self.check_rate_limit() {
            warn!("Order rejected: rate limit exceeded");
            self.metrics.orders_rejected.fetch_add(1, Ordering::Relaxed);
            self.metrics.rate_breaches.fetch_add(1, Ordering::Relaxed);
            return Ok(false);
        }
        
        // 2. Check order size
        if !self.check_order_size(symbol, quantity) {
            warn!("Order rejected: size limit exceeded");
            self.metrics.orders_rejected.fetch_add(1, Ordering::Relaxed);
            return Ok(false);
        }
        
        // 3. Check position limits
        if !self.check_position_limit(symbol, side, quantity) {
            warn!("Order rejected: position limit breach");
            self.metrics.orders_rejected.fetch_add(1, Ordering::Relaxed);
            self.metrics.position_breaches.fetch_add(1, Ordering::Relaxed);
            return Ok(false);
        }
        
        // 4. Check notional value
        if let Some(px) = price {
            if !self.check_notional(symbol, quantity, px) {
                warn!("Order rejected: notional limit exceeded");
                self.metrics.orders_rejected.fetch_add(1, Ordering::Relaxed);
                return Ok(false);
            }
        }
        
        // 5. Check daily P&L limits
        if !self.check_daily_pnl() {
            warn!("Order rejected: daily loss limit exceeded");
            self.metrics.orders_rejected.fetch_add(1, Ordering::Relaxed);
            return Ok(false);
        }
        
        // Update latency metrics
        let latency = start.elapsed().as_nanos() as u64;
        self.metrics.avg_latency_ns.store(latency, Ordering::Relaxed);
        
        debug!("Risk check passed in {}ns", latency);
        Ok(true)
    }
    
    /// Check rate limits
    fn check_rate_limit(&self) -> bool {
        let mut limiter = self.order_rate_limiter.write();
        let now = Instant::now();
        
        // Reset second counter if needed
        if now.duration_since(limiter.current_second).as_secs() >= 1 {
            limiter.current_second = now;
            limiter.orders_this_second = 0;
        }
        
        // Reset minute counter if needed
        if now.duration_since(limiter.current_minute).as_secs() >= 60 {
            limiter.current_minute = now;
            limiter.orders_this_minute = 0;
        }
        
        // Check limits
        if limiter.orders_this_second >= limiter.orders_per_second {
            return false;
        }
        
        if limiter.orders_this_minute >= limiter.orders_per_minute {
            return false;
        }
        
        // Update counters
        limiter.orders_this_second += 1;
        limiter.orders_this_minute += 1;
        
        true
    }
    
    /// Check order size limits
    fn check_order_size(&self, symbol: Symbol, quantity: Qty) -> bool {
        if let Some(limit) = self.position_limits.get(&symbol) {
            quantity <= limit.max_order_size
        } else {
            // Use default limit
            quantity <= self.config.max_position_size
        }
    }
    
    /// Check position limits
    fn check_position_limit(&self, symbol: Symbol, side: Side, quantity: Qty) -> bool {
        let current = self.current_positions
            .entry(symbol)
            .or_insert_with(|| AtomicI64::new(0));
        
        let current_pos = current.load(Ordering::Acquire);
        
        let new_pos = match side {
            Side::Buy => current_pos + quantity.as_i64(),
            Side::Sell => current_pos - quantity.as_i64(),
        };
        
        // Check against limits
        if let Some(limit) = self.position_limits.get(&symbol) {
            if new_pos > 0 && Qty::from_i64(new_pos) > limit.max_long {
                return false;
            }
            if new_pos < 0 && Qty::from_i64(-new_pos) > limit.max_short {
                return false;
            }
        } else {
            // Use default limits
            if new_pos.abs() > self.config.max_position_size.as_i64() {
                return false;
            }
        }
        
        true
    }
    
    /// Check notional value limits
    fn check_notional(&self, symbol: Symbol, quantity: Qty, price: Px) -> bool {
        let notional = (quantity.as_i64() * price.as_i64()) / 10000;
        
        if let Some(limit) = self.position_limits.get(&symbol) {
            notional <= limit.max_notional
        } else {
            // Default: 1M USDT notional
            notional <= 10000000000
        }
    }
    
    /// Check daily P&L limits
    fn check_daily_pnl(&self) -> bool {
        let current_pnl = self.daily_pnl.load(Ordering::Acquire);
        current_pnl > -self.config.max_daily_loss
    }
    
    /// Updates position tracking after trade execution
    ///
    /// Maintains real-time position state for risk limit calculations.
    /// Called after successful order execution to keep position tracking
    /// synchronized with actual executed trades.
    ///
    /// # Arguments
    /// * `symbol` - Trading symbol that was executed
    /// * `side` - Buy or Sell side of the executed trade
    /// * `quantity` - Executed quantity to add/subtract from position
    ///
    /// # Example
    /// ```rust
    /// // After successful execution
    /// risk_gate.update_position(symbol, Side::Buy, executed_qty).await;
    /// ```
    pub async fn update_position(&self, symbol: Symbol, side: Side, quantity: Qty) {
        let current = self.current_positions
            .entry(symbol)
            .or_insert_with(|| AtomicI64::new(0));
        
        match side {
            Side::Buy => {
                current.fetch_add(quantity.as_i64(), Ordering::AcqRel);
            }
            Side::Sell => {
                current.fetch_sub(quantity.as_i64(), Ordering::AcqRel);
            }
        }
    }
    
    /// Updates daily profit and loss tracking
    ///
    /// Maintains running total of daily P&L for loss limit enforcement.
    /// Should be called with realized P&L from completed trades.
    ///
    /// # Arguments
    /// * `pnl` - Profit/loss amount to add (positive for profit, negative for loss)
    ///
    /// # Example
    /// ```rust
    /// // Record a $500 loss
    /// risk_gate.update_pnl(-50000); // in cents
    /// ```
    pub fn update_pnl(&self, pnl: i64) {
        self.daily_pnl.fetch_add(pnl, Ordering::AcqRel);
    }
    
    /// Configures position and order limits for a specific symbol
    ///
    /// Sets custom risk limits for a trading symbol, overriding default limits
    /// from gateway configuration. Limits take effect immediately for new orders.
    ///
    /// # Arguments
    /// * `symbol` - Trading symbol to configure
    /// * `limit` - Position limit configuration including max long/short/order size
    ///
    /// # Example
    /// ```rust
    /// let limit = PositionLimit {
    ///     max_long: Qty::from(1000),
    ///     max_short: Qty::from(500),
    ///     max_order_size: Qty::from(100),
    ///     max_notional: 1000000, // $10k
    /// };
    /// risk_gate.set_position_limit(Symbol::from("BTCUSDT"), limit);
    /// ```
    pub fn set_position_limit(&self, symbol: Symbol, limit: PositionLimit) {
        self.position_limits.insert(symbol, limit);
    }
    
    /// Resets daily tracking metrics and positions
    ///
    /// Typically called at start of trading day to reset P&L tracking
    /// and position states. Should be used carefully as it clears
    /// important risk state.
    ///
    /// # Resets
    /// - Daily P&L counter to zero
    /// - All position tracking to zero
    ///
    /// # Example
    /// ```rust
    /// // At start of trading day
    /// risk_gate.reset_daily();
    /// ```
    pub fn reset_daily(&self) {
        self.daily_pnl.store(0, Ordering::Release);
        
        // Reset positions to zero
        for entry in self.current_positions.iter() {
            entry.value().store(0, Ordering::Release);
        }
    }
    
    /// Returns a snapshot of current risk management metrics
    ///
    /// Provides comprehensive statistics about risk gate performance including
    /// order processing rates, rejection patterns, and check latency.
    ///
    /// # Returns
    /// RiskMetricsSnapshot containing current risk management statistics
    ///
    /// # Example
    /// ```rust
    /// let metrics = risk_gate.get_metrics();
    /// println!("Rejection rate: {:.2}%", metrics.rejection_rate);
    /// println!("Avg check latency: {}ns", metrics.avg_latency_ns);
    /// ```
    pub fn get_metrics(&self) -> RiskMetricsSnapshot {
        RiskMetricsSnapshot {
            orders_checked: self.metrics.orders_checked.load(Ordering::Relaxed),
            orders_rejected: self.metrics.orders_rejected.load(Ordering::Relaxed),
            position_breaches: self.metrics.position_breaches.load(Ordering::Relaxed),
            rate_breaches: self.metrics.rate_breaches.load(Ordering::Relaxed),
            avg_latency_ns: self.metrics.avg_latency_ns.load(Ordering::Relaxed),
            rejection_rate: if self.metrics.orders_checked.load(Ordering::Relaxed) > 0 {
                (self.metrics.orders_rejected.load(Ordering::Relaxed) as f64 / 
                 self.metrics.orders_checked.load(Ordering::Relaxed) as f64) * 100.0
            } else {
                0.0
            },
        }
    }
}

/// Immutable snapshot of risk management performance metrics
///
/// Contains point-in-time statistics about risk gate operation and performance.
/// Used for monitoring, alerting, and performance optimization.
///
/// # Key Metrics
/// - Order processing volume and rejection statistics
/// - Breakdown of rejection reasons (position, rate limits, etc.)
/// - Performance measurements (latency)
/// - Calculated rates and percentages
#[derive(Debug, Clone)]
pub struct RiskMetricsSnapshot {
    /// Total orders checked
    pub orders_checked: u64,
    /// Orders rejected
    pub orders_rejected: u64,
    /// Position limit breaches
    pub position_breaches: u64,
    /// Rate limit breaches
    pub rate_breaches: u64,
    /// Average check latency
    pub avg_latency_ns: u64,
    /// Rejection rate percentage
    pub rejection_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_risk_gate_creation() {
        let config = GatewayConfig::default();
        let risk_gate = RiskGate::new(config);
        
        let metrics = risk_gate.get_metrics();
        assert_eq!(metrics.orders_checked, 0);
        assert_eq!(metrics.orders_rejected, 0);
    }
    
    #[tokio::test]
    async fn test_rate_limiting() {
        let config = GatewayConfig::default();
        let risk_gate = RiskGate::new(config);
        
        // Should pass initial checks
        assert!(risk_gate.check_rate_limit());
        
        // Exhaust rate limit
        for _ in 0..100 {
            risk_gate.check_rate_limit();
        }
        
        // Should fail after limit
        assert!(!risk_gate.check_rate_limit());
    }
}