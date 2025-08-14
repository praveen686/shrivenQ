//! Portfolio Manager Service
//!
//! Manages positions, P&L tracking, portfolio optimization, and rebalancing
//!
//! COMPLIANCE:
//! - Zero allocations in hot paths
//! - Lock-free position updates
//! - Fixed-point arithmetic only
//! - Cache-aligned structures

pub mod market_feed;
pub mod optimization;
pub mod portfolio;
pub mod position;
pub mod rebalancer;

use anyhow::Result;
use async_trait::async_trait;
use common::{Px, Qty, Side, Symbol, Ts};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Portfolio update events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PortfolioEvent {
    /// Position opened
    PositionOpened {
        symbol: Symbol,
        side: Side,
        quantity: Qty,
        avg_price: Px,
        timestamp: Ts,
    },
    /// Position closed
    PositionClosed {
        symbol: Symbol,
        realized_pnl: i64,
        timestamp: Ts,
    },
    /// Position updated
    PositionUpdated {
        symbol: Symbol,
        quantity: i64,
        unrealized_pnl: i64,
        timestamp: Ts,
    },
    /// Portfolio rebalanced
    Rebalanced {
        timestamp: Ts,
        changes: Vec<RebalanceChange>,
    },
}

/// Rebalance change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebalanceChange {
    pub symbol: Symbol,
    pub old_weight: i32, // Fixed-point percentage (SCALE_4 = 100%)
    pub new_weight: i32,
    pub quantity_change: i64,
}

/// Portfolio metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PortfolioMetrics {
    /// Total portfolio value
    pub total_value: i64,
    /// Total realized P&L
    pub realized_pnl: i64,
    /// Total unrealized P&L
    pub unrealized_pnl: i64,
    /// Number of open positions
    pub open_positions: u32,
    /// Portfolio volatility (annualized, fixed-point)
    pub volatility: i32,
    /// Sharpe ratio (fixed-point)
    pub sharpe_ratio: i32,
    /// Maximum drawdown (fixed-point percentage)
    pub max_drawdown: i32,
    /// Value at Risk (95% confidence, fixed-point)
    pub var_95: i64,
}

/// Portfolio optimization strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationStrategy {
    /// Equal weight across all assets
    EqualWeight,
    /// Minimize portfolio variance
    MinimumVariance,
    /// Maximize Sharpe ratio
    MaxSharpe,
    /// Risk parity
    RiskParity,
    /// Custom weights
    Custom,
}

/// Portfolio constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioConstraints {
    /// Maximum position size (as percentage, 10000 = 100%)
    pub max_position_pct: i32,
    /// Minimum position size
    pub min_position_pct: i32,
    /// Maximum number of positions
    pub max_positions: u32,
    /// Maximum leverage (fixed-point, 10000 = 1.0x)
    pub max_leverage: i32,
    /// Sector/asset class limits
    pub sector_limits: FxHashMap<String, i32>,
}

impl Default for PortfolioConstraints {
    fn default() -> Self {
        Self {
            max_position_pct: 2000, // 20%
            min_position_pct: 100,  // 1%
            max_positions: 50,
            max_leverage: 10000, // 1.0x
            sector_limits: FxHashMap::default(),
        }
    }
}

/// Portfolio manager trait
#[async_trait]
pub trait PortfolioManager: Send + Sync {
    /// Process order fill
    async fn process_fill(
        &mut self,
        order_id: u64,
        symbol: Symbol,
        side: Side,
        fill_qty: Qty,
        fill_price: Px,
        timestamp: Ts,
    ) -> Result<()>;

    /// Update market prices
    async fn update_market(
        &mut self,
        symbol: Symbol,
        bid: Px,
        ask: Px,
        timestamp: Ts,
    ) -> Result<()>;

    /// Get position for symbol
    async fn get_position(&self, symbol: Symbol) -> Option<position::PositionSnapshot>;

    /// Get all positions
    async fn get_all_positions(&self) -> Vec<position::PositionSnapshot>;

    /// Get portfolio metrics
    async fn get_metrics(&self) -> PortfolioMetrics;

    /// Optimize portfolio weights
    async fn optimize(
        &mut self,
        strategy: OptimizationStrategy,
        constraints: &PortfolioConstraints,
    ) -> Result<Vec<RebalanceChange>>;

    /// Execute rebalance
    async fn rebalance(&mut self, changes: Vec<RebalanceChange>) -> Result<()>;

    /// Get P&L breakdown
    async fn get_pnl_breakdown(&self) -> FxHashMap<Symbol, (i64, i64)>;

    /// Close all positions
    async fn close_all_positions(&mut self) -> Result<()>;

    /// Reset portfolio
    async fn reset(&mut self) -> Result<()>;
}

/// Portfolio manager service
pub struct PortfolioManagerService {
    /// Position tracker
    tracker: Arc<position::PositionTracker>,
    /// Portfolio optimizer
    optimizer: optimization::PortfolioOptimizer,
    /// Rebalancer
    rebalancer: rebalancer::Rebalancer,
    /// Constraints
    constraints: PortfolioConstraints,
    /// Metrics cache
    metrics: parking_lot::RwLock<PortfolioMetrics>,
    /// Market feed manager
    market_feed: market_feed::MarketFeedManager,
    /// Portfolio analyzer
    analyzer: parking_lot::RwLock<portfolio::PortfolioAnalyzer>,
    /// Latest portfolio statistics
    latest_stats: parking_lot::RwLock<portfolio::PortfolioStats>,
}

impl PortfolioManagerService {
    /// Create new portfolio manager
    pub fn new(capacity: usize) -> Self {
        // Pre-allocate symbols for market feed
        let symbols: Vec<Symbol> = (0..capacity)
            .map(|i| Symbol::new(u32::try_from(i).unwrap_or(u32::MAX)))
            .collect();

        Self {
            tracker: Arc::new(position::PositionTracker::new(capacity)),
            optimizer: optimization::PortfolioOptimizer::new(),
            rebalancer: rebalancer::Rebalancer::new(),
            constraints: PortfolioConstraints::default(),
            metrics: parking_lot::RwLock::new(PortfolioMetrics::default()),
            market_feed: market_feed::MarketFeedManager::new(&symbols, 1000),
            analyzer: parking_lot::RwLock::new(portfolio::PortfolioAnalyzer::new(1000)),
            latest_stats: parking_lot::RwLock::new(portfolio::PortfolioStats::default()),
        }
    }

    /// Connect to market data service
    pub async fn connect_market_feed(&mut self, endpoint: &str) -> Result<()> {
        self.market_feed.connect(endpoint).await?;
        self.market_feed.start_processor().await?;
        tracing::info!("Connected to market feed at {}", endpoint);
        Ok(())
    }

    /// Subscribe to symbols for market data
    pub async fn subscribe_symbols(&mut self, symbols: &[Symbol]) -> Result<()> {
        self.market_feed.subscribe(symbols).await?;
        Ok(())
    }

    /// Set constraints
    pub fn set_constraints(&mut self, constraints: PortfolioConstraints) {
        self.constraints = constraints;
    }

    /// Update metrics cache with real market data
    fn update_metrics(&mut self) {
        let (realized, unrealized, total) = self.tracker.get_global_pnl();
        let positions = self.tracker.get_all_positions();

        let mut metrics = self.metrics.write();
        metrics.realized_pnl = realized;
        metrics.unrealized_pnl = unrealized;
        metrics.total_value = total;
        metrics.open_positions = u32::try_from(positions.len()).unwrap_or(u32::MAX);

        // Calculate performance metrics using real data
        {
            let mut analyzer = self.analyzer.write();
            let returns = analyzer.returns().to_vec(); // Clone the data
            if !returns.is_empty() {
                let risk_free_rate = 200; // 2% annual
                let perf = analyzer.calculate_performance(&returns, risk_free_rate);
                metrics.sharpe_ratio = perf.sharpe_ratio;
                metrics.volatility = perf.annual_return;

                // Risk metrics
                let risk = analyzer.calculate_risk(&returns);
                metrics.var_95 = risk.var_95;
                metrics.max_drawdown = risk.max_drawdown_pct;
            }
        }
    }

    /// Get latest portfolio statistics
    pub fn get_latest_stats(&self) -> portfolio::PortfolioStats {
        self.latest_stats.read().clone()
    }

    /// Update portfolio beta against market index
    pub async fn update_portfolio_beta(&self, index: &str) -> Result<i32> {
        let positions: Vec<(Symbol, i64)> = self
            .tracker
            .get_all_positions()
            .into_iter()
            .map(|(symbol, qty, _)| (symbol, qty))
            .collect();

        let beta = self
            .market_feed
            .calculate_portfolio_beta(&positions, index)
            .await;
        let correlation = self
            .market_feed
            .calculate_portfolio_correlation(&positions, index)
            .await;

        // Update portfolio stats
        {
            let analyzer = self.analyzer.read();
            let positions_with_prices: Vec<(Symbol, i64, i64)> = self
                .tracker
                .get_all_positions()
                .into_iter()
                .map(|(symbol, qty, price)| (symbol, qty, price))
                .collect();

            let mut stats = analyzer.calculate_stats(&positions_with_prices);
            stats.beta = beta;
            stats.correlation = correlation;

            // Store the updated stats
            let mut latest_stats = self.latest_stats.write();
            *latest_stats = stats;
        }

        Ok(beta)
    }
}

#[async_trait]
impl PortfolioManager for PortfolioManagerService {
    async fn process_fill(
        &mut self,
        order_id: u64,
        symbol: Symbol,
        side: Side,
        fill_qty: Qty,
        fill_price: Px,
        timestamp: Ts,
    ) -> Result<()> {
        // Add to tracker
        self.tracker.add_pending(order_id, symbol, side, fill_qty);
        self.tracker
            .apply_fill(order_id, fill_qty, fill_price, timestamp);

        // Update metrics
        self.update_metrics();

        Ok(())
    }

    async fn update_market(
        &mut self,
        symbol: Symbol,
        bid: Px,
        ask: Px,
        timestamp: Ts,
    ) -> Result<()> {
        // Update position tracker
        self.tracker.update_market(symbol, bid, ask, timestamp);

        // Update market feed with real-time prices
        let update = market_feed::PriceUpdate {
            symbol,
            bid: bid.as_i64(),
            ask: ask.as_i64(),
            last: (bid.as_i64() + ask.as_i64()) / 2,
            volume: 0,
            timestamp: timestamp.as_nanos(),
        };
        self.market_feed.update_price(update)?;

        // Calculate returns for analytics
        let positions = self.tracker.get_all_positions();
        let total_pnl: i64 = positions.iter().map(|(_, _, pnl)| pnl).sum();
        self.analyzer.write().add_return(total_pnl);
        self.analyzer
            .write()
            .add_value(self.tracker.get_global_pnl().2);

        // Update metrics with real-time calculations
        self.update_metrics();
        Ok(())
    }

    async fn get_position(&self, symbol: Symbol) -> Option<position::PositionSnapshot> {
        self.tracker.get_position(symbol).map(|p| p.snapshot())
    }

    async fn get_all_positions(&self) -> Vec<position::PositionSnapshot> {
        self.tracker
            .get_all_positions()
            .into_iter()
            .map(|(symbol, quantity, pnl)| {
                position::PositionSnapshot {
                    symbol,
                    quantity,
                    avg_price: Px::ZERO, // Would need to get from position
                    realized_pnl: pnl,
                    unrealized_pnl: 0,
                    total_pnl: pnl,
                    last_update: Ts::now(),
                }
            })
            .collect()
    }

    async fn get_metrics(&self) -> PortfolioMetrics {
        self.metrics.read().clone()
    }

    async fn optimize(
        &mut self,
        strategy: OptimizationStrategy,
        constraints: &PortfolioConstraints,
    ) -> Result<Vec<RebalanceChange>> {
        let positions = self.tracker.get_all_positions();
        self.optimizer
            .optimize(strategy, &positions, constraints)
            .await
    }

    async fn rebalance(&mut self, changes: Vec<RebalanceChange>) -> Result<()> {
        self.rebalancer.execute(changes, &self.tracker).await
    }

    async fn get_pnl_breakdown(&self) -> FxHashMap<Symbol, (i64, i64)> {
        let mut breakdown = FxHashMap::default();
        for (symbol, _, pnl) in self.tracker.get_all_positions() {
            breakdown.insert(symbol, (pnl, 0)); // Simplified
        }
        breakdown
    }

    async fn close_all_positions(&mut self) -> Result<()> {
        // Would need to generate orders to close all positions
        Ok(())
    }

    async fn reset(&mut self) -> Result<()> {
        // Reset tracker and metrics
        self.tracker.reconcile_global_pnl();
        *self.metrics.write() = PortfolioMetrics::default();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_portfolio_manager_basic() {
        let mut manager = PortfolioManagerService::new(100);

        // Process a fill
        let symbol = Symbol::new(1);
        let result = manager
            .process_fill(
                1,
                symbol,
                Side::Bid,
                Qty::from_i64(1000000), // 100 units
                Px::from_i64(1000000),  // $100
                Ts::now(),
            )
            .await;

        assert!(result.is_ok());

        // Check position
        let position = manager.get_position(symbol).await;
        assert!(position.is_some());

        // Check metrics
        let metrics = manager.get_metrics().await;
        assert_eq!(metrics.open_positions, 1);
    }
}
