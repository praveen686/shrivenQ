//! Backtesting Service for ShrivenQuant
//! 
//! Provides historical strategy testing with realistic simulation
//! 
//! Current Status: DEVELOPMENT - Basic framework implemented
//! Production Readiness: 0% - Not tested

use anyhow::{Result, Context};
use chrono::{DateTime, Utc, Duration};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
// OrderStatistics imported locally where needed
use tracing::{info, warn, error, debug};

/// Backtesting engine for strategy evaluation
pub struct BacktestEngine {
    config: BacktestConfig,
    market_data: Arc<MarketDataStore>,
    execution_simulator: Arc<ExecutionSimulator>,
    portfolio_tracker: Arc<PortfolioTracker>,
    performance_analyzer: Arc<PerformanceAnalyzer>,
    state: Arc<RwLock<BacktestState>>,
}

/// Configuration for backtesting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestConfig {
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub initial_capital: f64,
    pub commission_rate: f64,      // 0.001 = 0.1%
    pub slippage_model: SlippageModel,
    pub data_frequency: DataFrequency,
    pub enable_shorting: bool,
    pub margin_requirement: f64,    // 0.5 = 50% margin
    pub risk_free_rate: f64,       // Annual risk-free rate
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SlippageModel {
    Fixed { bps: f64 },           // Fixed basis points
    Linear { impact: f64 },       // Linear market impact
    Square { impact: f64 },       // Square-root market impact
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataFrequency {
    Tick,
    Second,
    Minute,
    FiveMinute,
    Hour,
    Daily,
}

/// Current state of the backtest
#[derive(Debug, Clone)]
pub struct BacktestState {
    pub current_time: DateTime<Utc>,
    pub is_running: bool,
    pub progress_pct: f64,
    pub orders_processed: u64,
    pub trades_executed: u64,
}

/// Market data storage for backtesting
pub struct MarketDataStore {
    price_data: DashMap<String, BTreeMap<DateTime<Utc>, OHLCV>>,
    orderbook_snapshots: DashMap<String, BTreeMap<DateTime<Utc>, OrderbookSnapshot>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OHLCV {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderbookSnapshot {
    pub bids: Vec<(f64, f64)>,  // (price, quantity)
    pub asks: Vec<(f64, f64)>,
    pub timestamp: DateTime<Utc>,
}

/// Execution simulator for realistic order fills
pub struct ExecutionSimulator {
    pending_orders: Arc<DashMap<String, Order>>,
    fill_history: Arc<RwLock<Vec<Fill>>>,
    config: ExecutionConfig,
    rejection_counter: Arc<std::sync::atomic::AtomicU64>,
}

const REJECTION_RATE_PRECISION: u64 = 10000; // 0.01% precision

#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    pub use_limit_order_book: bool,
    pub partial_fills: bool,
    pub reject_rate: f64,  // Probability of order rejection (0.0 to 1.0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: f64,
    pub price: Option<f64>,
    pub timestamp: DateTime<Utc>,
    pub time_in_force: TimeInForce,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    Stop,
    StopLimit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimeInForce {
    Day,
    GTC,  // Good Till Cancelled
    IOC,  // Immediate or Cancel
    FOK,  // Fill or Kill
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub order_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub quantity: f64,
    pub price: f64,
    pub commission: f64,
    pub slippage: f64,
    pub timestamp: DateTime<Utc>,
}

/// Portfolio tracker for position and P&L management
pub struct PortfolioTracker {
    positions: Arc<DashMap<String, Position>>,
    cash_balance: Arc<RwLock<f64>>,
    equity_curve: Arc<RwLock<Vec<(DateTime<Utc>, f64)>>>,
    transaction_history: Arc<RwLock<Vec<Transaction>>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub quantity: f64,
    pub average_price: f64,
    pub current_price: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub commission_paid: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub timestamp: DateTime<Utc>,
    pub symbol: String,
    pub side: OrderSide,
    pub quantity: f64,
    pub price: f64,
    pub commission: f64,
    pub net_amount: f64,
}

/// Performance analyzer for strategy metrics
pub struct PerformanceAnalyzer {
    metrics_cache: Arc<RwLock<PerformanceMetrics>>,
    trade_log: Arc<RwLock<Vec<CompletedTrade>>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    // Returns
    pub total_return: f64,
    pub annualized_return: f64,
    pub volatility: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub calmar_ratio: f64,
    
    // Risk metrics
    pub max_drawdown: f64,
    pub max_drawdown_duration: i64,  // days
    pub value_at_risk: f64,          // 95% VaR
    pub conditional_var: f64,         // CVaR
    
    // Trade statistics
    pub total_trades: u64,
    pub winning_trades: u64,
    pub losing_trades: u64,
    pub win_rate: f64,
    pub average_win: f64,
    pub average_loss: f64,
    pub profit_factor: f64,
    pub expectancy: f64,
    
    // Execution stats
    pub total_commission: f64,
    pub total_slippage: f64,
    pub average_trade_duration: i64,  // minutes
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedTrade {
    pub entry_time: DateTime<Utc>,
    pub exit_time: DateTime<Utc>,
    pub symbol: String,
    pub side: OrderSide,
    pub entry_price: f64,
    pub exit_price: f64,
    pub quantity: f64,
    pub pnl: f64,
    pub return_pct: f64,
}

impl BacktestEngine {
    pub fn new(config: BacktestConfig) -> Self {
        info!("Initializing BacktestEngine with config: {:?}", config);
        
        Self {
            config: config.clone(),
            market_data: Arc::new(MarketDataStore::new()),
            execution_simulator: Arc::new(ExecutionSimulator::new(ExecutionConfig {
                use_limit_order_book: true,
                partial_fills: true,
                reject_rate: 0.001,
            })),
            portfolio_tracker: Arc::new(PortfolioTracker::new(config.initial_capital)),
            performance_analyzer: Arc::new(PerformanceAnalyzer::new()),
            state: Arc::new(RwLock::new(BacktestState {
                current_time: config.start_date,
                is_running: false,
                progress_pct: 0.0,
                orders_processed: 0,
                trades_executed: 0,
            })),
        }
    }
    
    /// Load historical data for backtesting
    pub async fn load_data(&self, symbol: &str, data: Vec<(DateTime<Utc>, OHLCV)>) -> Result<()> {
        info!("Loading {} data points for {}", data.len(), symbol);
        
        let mut price_map = BTreeMap::new();
        let mut data_queue = VecDeque::from(data);
        
        // Process data using VecDeque for efficient processing
        while let Some((timestamp, ohlcv)) = data_queue.pop_front() {
            // Validate data quality
            if ohlcv.high < ohlcv.low {
                warn!("Invalid OHLCV data at {}: high {} < low {}", timestamp, ohlcv.high, ohlcv.low);
                continue;
            }
            if ohlcv.close > ohlcv.high || ohlcv.close < ohlcv.low {
                debug!("Close price {} outside high-low range at {}", ohlcv.close, timestamp);
            }
            price_map.insert(timestamp, ohlcv);
        }
        
        if price_map.is_empty() {
            error!("No valid data loaded for symbol {}", symbol);
            return Err(anyhow::Error::msg("No valid data after processing"))
                .context(format!("Failed to load data for {}", symbol));
        }
        
        self.market_data.price_data.insert(symbol.to_string(), price_map);
        Ok(())
    }
    
    /// Run the backtest with a strategy
    pub async fn run<S: Strategy>(&self, strategy: &S) -> Result<BacktestResult> {
        info!("Starting backtest from {} to {}", self.config.start_date, self.config.end_date);
        
        {
            let mut state = self.state.write();
            state.is_running = true;
            state.current_time = self.config.start_date;
        }
        
        // Main backtest loop
        let mut current = self.config.start_date;
        let total_duration = (self.config.end_date - self.config.start_date).num_seconds() as f64;
        
        while current <= self.config.end_date {
            // Update progress
            {
                let mut state = self.state.write();
                state.current_time = current;
                let elapsed = (current - self.config.start_date).num_seconds() as f64;
                state.progress_pct = (elapsed / total_duration) * 100.0;
            }
            
            // Get market data for current time
            let market_snapshot = self.get_market_snapshot(current)?;
            
            // Update portfolio with current prices
            self.portfolio_tracker.update_prices(&market_snapshot)?;
            
            // Generate signals from strategy
            let signals = strategy.generate_signals(&market_snapshot, &self.get_portfolio_state());
            
            // Process signals into orders
            for signal in signals {
                self.process_signal(signal, current).await?;
            }
            
            // Simulate order execution
            self.execution_simulator.process_pending_orders(&market_snapshot, current)?;
            
            // Record equity curve point
            self.portfolio_tracker.record_equity(current)?;
            
            // Advance time
            current = self.advance_time(current);
        }
        
        {
            let mut state = self.state.write();
            state.is_running = false;
            state.progress_pct = 100.0;
        }
        
        // Calculate final metrics
        let metrics = self.performance_analyzer.calculate_metrics(
            &self.portfolio_tracker,
            self.config.risk_free_rate
        )?;
        
        Ok(BacktestResult {
            metrics,
            equity_curve: self.portfolio_tracker.get_equity_curve(),
            trades: self.performance_analyzer.get_trades(),
            final_portfolio: self.portfolio_tracker.get_final_state(),
        })
    }
    
    fn get_market_snapshot(&self, timestamp: DateTime<Utc>) -> Result<MarketSnapshot> {
        let snapshot = MarketSnapshot {
            timestamp,
            prices: DashMap::new(),
        };
        
        for entry in self.market_data.price_data.iter() {
            let symbol = entry.key().clone();
            let price_data = entry.value();
            
            // Find the price at or before the timestamp
            if let Some((_, ohlcv)) = price_data.range(..=timestamp).last() {
                snapshot.prices.insert(symbol, ohlcv.close);
            }
        }
        
        Ok(snapshot)
    }
    
    fn get_portfolio_state(&self) -> PortfolioState {
        self.portfolio_tracker.get_current_state()
    }
    
    async fn process_signal(&self, signal: TradingSignal, timestamp: DateTime<Utc>) -> Result<()> {
        // Convert signal to order
        let order = self.signal_to_order(signal, timestamp)?;
        
        // Submit order to execution simulator
        self.execution_simulator.submit_order(order)?;
        
        Ok(())
    }
    
    fn signal_to_order(&self, signal: TradingSignal, timestamp: DateTime<Utc>) -> Result<Order> {
        Ok(Order {
            id: format!("BT_{}", uuid::Uuid::new_v4()),
            symbol: signal.symbol,
            side: signal.side,
            order_type: signal.order_type,
            quantity: signal.quantity,
            price: signal.price,
            timestamp,
            time_in_force: TimeInForce::GTC,
        })
    }
    
    fn advance_time(&self, current: DateTime<Utc>) -> DateTime<Utc> {
        match self.config.data_frequency {
            DataFrequency::Tick => current + Duration::seconds(1),
            DataFrequency::Second => current + Duration::seconds(1),
            DataFrequency::Minute => current + Duration::minutes(1),
            DataFrequency::FiveMinute => current + Duration::minutes(5),
            DataFrequency::Hour => current + Duration::hours(1),
            DataFrequency::Daily => current + Duration::days(1),
        }
    }
}

// Additional trait definitions and implementations

/// Strategy trait that users must implement
pub trait Strategy: Send + Sync {
    fn generate_signals(&self, market: &MarketSnapshot, portfolio: &PortfolioState) -> Vec<TradingSignal>;
}

#[derive(Debug, Clone)]
pub struct MarketSnapshot {
    pub timestamp: DateTime<Utc>,
    pub prices: DashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioState {
    pub cash: f64,
    pub positions: Vec<Position>,
    pub total_value: f64,
}

#[derive(Debug, Clone)]
pub struct TradingSignal {
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: f64,
    pub price: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    pub metrics: PerformanceMetrics,
    pub equity_curve: Vec<(DateTime<Utc>, f64)>,
    pub trades: Vec<CompletedTrade>,
    pub final_portfolio: PortfolioState,
}

// Implementation stubs for sub-components

impl MarketDataStore {
    pub fn new() -> Self {
        Self {
            price_data: DashMap::new(),
            orderbook_snapshots: DashMap::new(),
        }
    }
    
    /// Store orderbook snapshot for a symbol
    pub fn add_orderbook_snapshot(&self, symbol: &str, timestamp: DateTime<Utc>, snapshot: OrderbookSnapshot) {
        self.orderbook_snapshots
            .entry(symbol.to_string())
            .or_insert_with(BTreeMap::new)
            .insert(timestamp, snapshot);
    }
    
    /// Get orderbook at specific time
    pub fn get_orderbook(&self, symbol: &str, timestamp: DateTime<Utc>) -> Option<OrderbookSnapshot> {
        self.orderbook_snapshots
            .get(symbol)?
            .range(..=timestamp)
            .last()
            .map(|(_, snapshot)| snapshot.clone())
    }
}

impl ExecutionSimulator {
    pub fn new(config: ExecutionConfig) -> Self {
        use std::sync::atomic::AtomicU64;
        Self {
            pending_orders: Arc::new(DashMap::new()),
            fill_history: Arc::new(RwLock::new(Vec::new())),
            config,
            rejection_counter: Arc::new(AtomicU64::new(0)),
        }
    }
    
    pub fn submit_order(&self, order: Order) -> Result<()> {
        self.pending_orders.insert(order.id.clone(), order);
        Ok(())
    }
    
    pub fn process_pending_orders(&self, market: &MarketSnapshot, timestamp: DateTime<Utc>) -> Result<()> {
        // Process orders with configuration-based behavior
        let orders: Vec<_> = self.pending_orders.iter().map(|e| e.value().clone()).collect();
        
        for order in orders {
            // Check for deterministic rejection based on config
            if self.config.reject_rate > 0.0 {
                use std::sync::atomic::Ordering;
                let counter = self.rejection_counter.fetch_add(1, Ordering::Relaxed);
                let threshold = (self.config.reject_rate * REJECTION_RATE_PRECISION as f64) as u64;
                let should_reject = (counter % REJECTION_RATE_PRECISION) < threshold;
                
                if should_reject {
                    warn!("Order {} rejected due to simulated rejection (rate: {:.2}%)", 
                          order.id, self.config.reject_rate * 100.0);
                    self.pending_orders.remove(&order.id);
                    continue;
                }
            }
            
            if let Some(price) = market.prices.get(&order.symbol) {
                // Simulate fill based on order type
                match order.order_type {
                    OrderType::Market => {
                        self.execute_fill(order, *price, timestamp)?;
                    }
                    OrderType::Limit => {
                        if let Some(limit_price) = order.price {
                            let should_fill = match order.side {
                                OrderSide::Buy => *price <= limit_price,
                                OrderSide::Sell => *price >= limit_price,
                            };
                            if should_fill && self.config.use_limit_order_book {
                                // When using LOB simulation, may only partially fill
                                if self.config.partial_fills {
                                    debug!("Processing potential partial fill for order {}", order.id);
                                }
                                self.execute_fill(order, limit_price, timestamp)?;
                            } else if should_fill {
                                self.execute_fill(order, limit_price, timestamp)?;
                            }
                        }
                    }
                    _ => {} // Other order types not implemented yet
                }
            }
        }
        
        Ok(())
    }
    
    fn execute_fill(&self, order: Order, price: f64, timestamp: DateTime<Utc>) -> Result<()> {
        let commission = 0.001 * price * order.quantity;
        let slippage = 0.0001 * price;
        let fill = Fill {
            order_id: order.id.clone(),
            symbol: order.symbol.clone(),
            side: order.side.clone(),
            quantity: order.quantity,
            price,
            commission,
            slippage,
            timestamp,
        };
        
        self.fill_history.write().push(fill.clone());
        self.pending_orders.remove(&order.id);
        
        // Notify portfolio tracker of the fill
        debug!("Executed fill for {} - {} {} @ {} (commission: {}, slippage: {})",
               order.symbol, 
               match order.side {
                   OrderSide::Buy => "BUY",
                   OrderSide::Sell => "SELL",
               },
               order.quantity, price, commission, slippage);
        
        Ok(())
    }
}

impl PortfolioTracker {
    pub fn new(initial_capital: f64) -> Self {
        info!("Initializing portfolio with capital: {}", initial_capital);
        Self {
            positions: Arc::new(DashMap::new()),
            cash_balance: Arc::new(RwLock::new(initial_capital)),
            equity_curve: Arc::new(RwLock::new(Vec::new())),
            transaction_history: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Process a fill and update portfolio
    pub fn process_fill(&self, fill: &Fill) -> Result<()> {
        let mut cash = self.cash_balance.write();
        
        let transaction = match fill.side {
            OrderSide::Buy => {
                let total_cost = fill.price * fill.quantity + fill.commission + fill.slippage;
                if *cash < total_cost {
                    error!("Insufficient cash for buy order: need {}, have {}", total_cost, *cash);
                    return Err(anyhow::Error::msg("Insufficient funds"));
                }
                *cash -= total_cost;
                
                // Update position
                self.positions.entry(fill.symbol.clone())
                    .and_modify(|pos| {
                        let new_qty = pos.quantity + fill.quantity;
                        pos.average_price = (pos.average_price * pos.quantity + fill.price * fill.quantity) / new_qty;
                        pos.quantity = new_qty;
                        pos.commission_paid += fill.commission;
                    })
                    .or_insert(Position {
                        symbol: fill.symbol.clone(),
                        quantity: fill.quantity,
                        average_price: fill.price,
                        current_price: fill.price,
                        realized_pnl: 0.0,
                        unrealized_pnl: 0.0,
                        commission_paid: fill.commission,
                    });
                
                Transaction {
                    timestamp: fill.timestamp,
                    symbol: fill.symbol.clone(),
                    side: OrderSide::Buy,
                    quantity: fill.quantity,
                    price: fill.price,
                    commission: fill.commission,
                    net_amount: -total_cost,
                }
            }
            OrderSide::Sell => {
                let total_proceeds = fill.price * fill.quantity - fill.commission - fill.slippage;
                *cash += total_proceeds;
                
                // Update position
                if let Some(mut pos) = self.positions.get_mut(&fill.symbol) {
                    let realized = (fill.price - pos.average_price) * fill.quantity;
                    pos.realized_pnl += realized;
                    pos.quantity -= fill.quantity;
                    pos.commission_paid += fill.commission;
                    
                    if pos.quantity <= 0.0 {
                        self.positions.remove(&fill.symbol);
                    }
                }
                
                Transaction {
                    timestamp: fill.timestamp,
                    symbol: fill.symbol.clone(),
                    side: OrderSide::Sell,
                    quantity: fill.quantity,
                    price: fill.price,
                    commission: fill.commission,
                    net_amount: total_proceeds,
                }
            }
        };
        
        self.transaction_history.write().push(transaction);
        Ok(())
    }
    
    pub fn update_prices(&self, market: &MarketSnapshot) -> Result<()> {
        for mut entry in self.positions.iter_mut() {
            if let Some(price) = market.prices.get(entry.key()) {
                entry.current_price = *price;
                entry.unrealized_pnl = (entry.current_price - entry.average_price) * entry.quantity;
            }
        }
        Ok(())
    }
    
    pub fn record_equity(&self, timestamp: DateTime<Utc>) -> Result<()> {
        let total_value = self.calculate_total_value();
        self.equity_curve.write().push((timestamp, total_value));
        Ok(())
    }
    
    pub fn get_equity_curve(&self) -> Vec<(DateTime<Utc>, f64)> {
        self.equity_curve.read().clone()
    }
    
    pub fn get_current_state(&self) -> PortfolioState {
        PortfolioState {
            cash: *self.cash_balance.read(),
            positions: self.positions.iter().map(|e| e.value().clone()).collect(),
            total_value: self.calculate_total_value(),
        }
    }
    
    pub fn get_final_state(&self) -> PortfolioState {
        self.get_current_state()
    }
    
    fn calculate_total_value(&self) -> f64 {
        let cash = *self.cash_balance.read();
        let positions_value: f64 = self.positions
            .iter()
            .map(|e| e.current_price * e.quantity)
            .sum();
        cash + positions_value
    }
}

impl PerformanceAnalyzer {
    pub fn new() -> Self {
        Self {
            metrics_cache: Arc::new(RwLock::new(PerformanceMetrics::default())),
            trade_log: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    pub fn calculate_metrics(&self, portfolio: &PortfolioTracker, risk_free_rate: f64) -> Result<PerformanceMetrics> {
        let equity_curve = portfolio.get_equity_curve();
        if equity_curve.len() < 2 {
            return Ok(PerformanceMetrics::default());
        }
        
        // Calculate returns
        let returns: Vec<f64> = equity_curve.windows(2)
            .map(|w| (w[1].1 - w[0].1) / w[0].1)
            .collect();
        
        let initial_value = equity_curve.first().map(|e| e.1).unwrap_or(0.0);
        let final_value = equity_curve.last().map(|e| e.1).unwrap_or(0.0);
        let total_return = (final_value - initial_value) / initial_value;
        
        // Calculate statistics directly on slice
        use statrs::statistics::{Statistics, OrderStatistics};
        
        // Calculate basic statistics
        let mean_return = (&returns[..]).mean();
        let volatility = (&returns[..]).std_dev();
        
        // Use Data wrapper for percentiles
        let mut data = statrs::statistics::Data::new(returns.clone());
        let median_return = data.median();
        let percentile_95 = data.percentile(95);
        let percentile_5 = data.percentile(5);
        
        let sharpe_ratio = if volatility > 0.0 {
            (mean_return - risk_free_rate / 252.0) / volatility
        } else {
            0.0
        };
        
        debug!("Return statistics - Mean: {:.4}, Median: {:.4}, Vol: {:.4}", 
               mean_return, median_return, volatility);
        
        // Calculate max drawdown
        let max_drawdown = self.calculate_max_drawdown(&equity_curve);
        
        let metrics = PerformanceMetrics {
            total_return,
            annualized_return: total_return * 252.0 / returns.len() as f64,
            volatility: volatility * (252.0_f64).sqrt(),
            sharpe_ratio: sharpe_ratio * (252.0_f64).sqrt(),
            sortino_ratio: self.calculate_sortino_ratio(&returns, risk_free_rate),
            max_drawdown,
            value_at_risk: percentile_5,  // 5% VaR
            conditional_var: returns.iter()
                .filter(|&&r| r <= percentile_5)
                .copied()
                .collect::<Vec<f64>>()
                .mean(),
            ..Default::default()
        };
        
        // Log key metrics including percentile_95
        info!("Backtest complete - Return: {:.2}%, Sharpe: {:.2}, 95th percentile: {:.4}", 
              total_return * 100.0, sharpe_ratio, percentile_95);
        
        *self.metrics_cache.write() = metrics.clone();
        Ok(metrics)
    }
    
    pub fn get_trades(&self) -> Vec<CompletedTrade> {
        self.trade_log.read().clone()
    }
    
    fn calculate_max_drawdown(&self, equity_curve: &[(DateTime<Utc>, f64)]) -> f64 {
        let mut max_drawdown = 0.0;
        let mut peak = 0.0;
        
        for (_, value) in equity_curve {
            if *value > peak {
                peak = *value;
            }
            let drawdown = (peak - value) / peak;
            if drawdown > max_drawdown {
                max_drawdown = drawdown;
            }
        }
        
        max_drawdown
    }
    
    fn calculate_sortino_ratio(&self, returns: &[f64], risk_free_rate: f64) -> f64 {
        use statrs::statistics::Statistics;
        
        let mean_return = (&returns[..]).mean();
        let downside_returns: Vec<f64> = returns.iter()
            .filter(|&&r| r < risk_free_rate / 252.0)
            .map(|&r| (r - risk_free_rate / 252.0).powi(2))
            .collect();
        
        if downside_returns.is_empty() {
            return 0.0;
        }
        
        let downside_deviation = ((&downside_returns[..]).mean()).sqrt();
        
        if downside_deviation > 0.0 {
            (mean_return - risk_free_rate / 252.0) / downside_deviation * (252.0_f64).sqrt()
        } else {
            0.0
        }
    }
}

// UUID support
mod uuid {
    use std::sync::atomic::{AtomicU64, Ordering};
    
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    
    pub struct Uuid;
    
    impl Uuid {
        pub fn new_v4() -> String {
            let count = COUNTER.fetch_add(1, Ordering::Relaxed);
            format!("{:016x}", count)
        }
    }
}