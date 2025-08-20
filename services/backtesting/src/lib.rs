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
/// 
/// Defines all parameters needed to run a historical strategy backtest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestConfig {
    /// Start date of the backtesting period (inclusive)
    pub start_date: DateTime<Utc>,
    /// End date of the backtesting period (inclusive)
    pub end_date: DateTime<Utc>,
    /// Initial capital amount in base currency (e.g., USD)
    /// Must be positive
    pub initial_capital: f64,
    /// Commission rate as a decimal (e.g., 0.001 = 0.1%)
    /// Applied to both buy and sell transactions
    pub commission_rate: f64,
    /// Model used to simulate market impact and slippage
    pub slippage_model: SlippageModel,
    /// Time frequency of the market data
    pub data_frequency: DataFrequency,
    /// Whether short selling is allowed in the backtest
    pub enable_shorting: bool,
    /// Margin requirement as a decimal (e.g., 0.5 = 50% margin)
    /// Only relevant when shorting is enabled
    pub margin_requirement: f64,
    /// Annual risk-free rate as a decimal (e.g., 0.02 = 2%)
    /// Used for calculating risk-adjusted returns like Sharpe ratio
    pub risk_free_rate: f64,
}

/// Models for simulating slippage and market impact
/// 
/// Determines how much the execution price deviates from the quoted price
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SlippageModel {
    /// Fixed slippage in basis points (1 bp = 0.01%)
    /// 
    /// Example: `Fixed { bps: 5.0 }` adds 5 basis points slippage
    Fixed { 
        /// Slippage amount in basis points (0.0 to 1000.0)
        bps: f64 
    },
    /// Linear market impact based on order size
    /// 
    /// Slippage = impact * sqrt(order_size / avg_volume)
    Linear { 
        /// Linear impact coefficient (typically 0.001 to 0.1)
        impact: f64 
    },
    /// Square-root market impact model (more realistic for large orders)
    /// 
    /// Slippage = impact * (order_size / avg_volume)^0.5
    Square { 
        /// Square-root impact coefficient (typically 0.01 to 1.0)
        impact: f64 
    },
}

/// Time frequency for market data and backtesting simulation
/// 
/// Determines the granularity of the backtesting time steps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataFrequency {
    /// Tick-by-tick data (highest granularity)
    /// Each market event is processed individually
    Tick,
    /// One-second intervals
    /// Suitable for high-frequency strategies
    Second,
    /// One-minute intervals
    /// Most common for intraday strategies
    Minute,
    /// Five-minute intervals
    /// Balance between granularity and computational efficiency
    FiveMinute,
    /// Hourly intervals
    /// Suitable for swing trading strategies
    Hour,
    /// Daily intervals
    /// Standard for long-term investment strategies
    Daily,
}

/// Current state of the backtest execution
/// 
/// Tracks the progress and status of a running backtest
#[derive(Debug, Clone)]
pub struct BacktestState {
    /// Current simulation timestamp
    pub current_time: DateTime<Utc>,
    /// Whether the backtest is currently running
    pub is_running: bool,
    /// Completion percentage (0.0 to 100.0)
    pub progress_pct: f64,
    /// Total number of orders processed so far
    pub orders_processed: u64,
    /// Total number of trades executed (filled orders)
    pub trades_executed: u64,
}

/// Market data storage for backtesting
#[derive(Debug)]
pub struct MarketDataStore {
    price_data: DashMap<String, BTreeMap<DateTime<Utc>, OHLCV>>,
    orderbook_snapshots: DashMap<String, BTreeMap<DateTime<Utc>, OrderbookSnapshot>>,
}

/// Open, High, Low, Close, Volume market data
/// 
/// Standard OHLCV bar representing price and volume data for a specific time period.
/// Used as the fundamental data structure for historical market data in backtesting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OHLCV {
    /// Opening price at the start of the time period
    pub open: f64,
    /// Highest price during the time period
    pub high: f64,
    /// Lowest price during the time period
    pub low: f64,
    /// Closing price at the end of the time period
    pub close: f64,
    /// Total volume traded during the time period
    pub volume: f64,
}

/// Snapshot of limit order book at a specific point in time
/// 
/// Contains bid and ask levels with their respective prices and quantities.
/// Used for realistic execution simulation when limit order book data is available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderbookSnapshot {
    /// Bid levels as (price, quantity) tuples, sorted by price descending
    pub bids: Vec<(f64, f64)>,  // (price, quantity)
    /// Ask levels as (price, quantity) tuples, sorted by price ascending
    pub asks: Vec<(f64, f64)>,
    /// Timestamp when this orderbook snapshot was captured
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

/// Configuration for realistic order execution simulation
/// 
/// Controls how orders are processed and filled in the backtesting environment,
/// allowing for realistic modeling of market microstructure effects.
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    /// Whether to use limit order book data for execution simulation
    /// If true, orders are matched against actual orderbook levels
    pub use_limit_order_book: bool,
    /// Whether to allow partial fills of orders
    /// If true, large orders may be filled incrementally over time
    pub partial_fills: bool,
    /// Probability of order rejection (0.0 to 1.0)
    /// Simulates real-world order rejections due to various factors
    pub reject_rate: f64,  // Probability of order rejection (0.0 to 1.0)
}

/// Trading order with all necessary execution parameters
/// 
/// Represents a complete order specification including symbol, quantity,
/// price constraints, and execution instructions for the trading system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    /// Unique identifier for this order
    pub id: String,
    /// Trading symbol or instrument identifier (e.g., "AAPL", "EURUSD")
    pub symbol: String,
    /// Order side indicating buy or sell direction
    pub side: OrderSide,
    /// Type of order affecting execution behavior
    pub order_type: OrderType,
    /// Number of shares/units to trade (must be positive)
    pub quantity: f64,
    /// Limit price for limit orders (None for market orders)
    pub price: Option<f64>,
    /// When this order was submitted
    pub timestamp: DateTime<Utc>,
    /// Time-in-force instruction controlling order lifetime
    pub time_in_force: TimeInForce,
}

/// Direction of a trading order
/// 
/// Indicates whether the order is to purchase (buy) or sell securities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderSide {
    /// Purchase order - acquire a long position
    Buy,
    /// Sell order - dispose of a long position or create a short position
    Sell,
}

/// Type of trading order affecting execution priority and price
/// 
/// Determines how the order will be executed in the market,
/// including price constraints and timing considerations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderType {
    /// Execute immediately at current market price
    Market,
    /// Execute only at specified price or better
    Limit,
    /// Trigger market order when stop price is reached
    Stop,
    /// Trigger limit order when stop price is reached
    StopLimit,
}

/// Duration and cancellation behavior for trading orders
/// 
/// Specifies how long an order remains active and under what
/// conditions it should be cancelled if not immediately filled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimeInForce {
    /// Order valid until end of trading day
    Day,
    /// Good Till Cancelled - remains active until manually cancelled
    GTC,  // Good Till Cancelled
    /// Immediate or Cancel - fill immediately or cancel remainder
    IOC,  // Immediate or Cancel
    /// Fill or Kill - fill entire order immediately or cancel completely
    FOK,  // Fill or Kill
}

/// Record of an executed order fill with all transaction details
/// 
/// Represents the actual execution of an order, including the final price,
/// quantity filled, and associated costs like commission and slippage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    /// Unique identifier of the original order that was filled
    pub order_id: String,
    /// Symbol that was traded
    pub symbol: String,
    /// Side of the trade (buy or sell)
    pub side: OrderSide,
    /// Quantity that was actually filled
    pub quantity: f64,
    /// Actual execution price per unit
    pub price: f64,
    /// Commission cost charged for this fill
    pub commission: f64,
    /// Slippage cost incurred during execution
    pub slippage: f64,
    /// Timestamp when the fill occurred
    pub timestamp: DateTime<Utc>,
}

/// Portfolio tracker for position and P&L management
pub struct PortfolioTracker {
    positions: Arc<DashMap<String, Position>>,
    cash_balance: Arc<RwLock<f64>>,
    equity_curve: Arc<RwLock<Vec<(DateTime<Utc>, f64)>>>,
    transaction_history: Arc<RwLock<Vec<Transaction>>>,
}

/// Portfolio position in a specific security
/// 
/// Tracks all details of a position including entry price, current value,
/// and profit/loss calculations both realized and unrealized.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Position {
    /// Symbol or instrument identifier for this position
    pub symbol: String,
    /// Current number of shares/units held (positive for long, negative for short)
    pub quantity: f64,
    /// Volume-weighted average price of all purchases for this position
    pub average_price: f64,
    /// Most recent market price for this security
    pub current_price: f64,
    /// Profit/loss from closed portions of this position
    pub realized_pnl: f64,
    /// Mark-to-market profit/loss on current holdings
    pub unrealized_pnl: f64,
    /// Total commission costs paid for this position
    pub commission_paid: f64,
}

/// Record of a portfolio transaction for accounting and analysis
/// 
/// Represents a completed trade transaction with its impact on cash balance,
/// including all fees and costs associated with the trade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// When this transaction occurred
    pub timestamp: DateTime<Utc>,
    /// Symbol that was traded in this transaction
    pub symbol: String,
    /// Direction of the trade (buy or sell)
    pub side: OrderSide,
    /// Number of shares/units traded
    pub quantity: f64,
    /// Price per unit for this transaction
    pub price: f64,
    /// Commission cost for this transaction
    pub commission: f64,
    /// Net cash impact (negative for purchases, positive for sales)
    pub net_amount: f64,
}

/// Performance analyzer for strategy metrics
pub struct PerformanceAnalyzer {
    metrics_cache: Arc<RwLock<PerformanceMetrics>>,
    trade_log: Arc<RwLock<Vec<CompletedTrade>>>,
}

/// Comprehensive performance and risk metrics for strategy evaluation
/// 
/// Contains all key statistics needed to assess a trading strategy's performance,
/// including return metrics, risk measures, and trading statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    // Returns
    /// Total return over the entire backtest period as a decimal (e.g., 0.15 = 15%)
    pub total_return: f64,
    /// Annualized return extrapolated from backtest period
    pub annualized_return: f64,
    /// Annualized volatility (standard deviation of returns)
    pub volatility: f64,
    /// Risk-adjusted return metric (excess return / volatility)
    pub sharpe_ratio: f64,
    /// Risk-adjusted return focusing on downside volatility
    pub sortino_ratio: f64,
    /// Risk-adjusted return using maximum drawdown (return / max drawdown)
    pub calmar_ratio: f64,
    
    // Risk metrics
    /// Maximum peak-to-trough decline as a decimal (e.g., 0.2 = 20% drawdown)
    pub max_drawdown: f64,
    /// Duration of maximum drawdown period in days
    pub max_drawdown_duration: i64,  // days
    /// Value at Risk at 95% confidence level (5th percentile of returns)
    pub value_at_risk: f64,          // 95% VaR
    /// Conditional Value at Risk (expected loss beyond VaR threshold)
    pub conditional_var: f64,         // CVaR
    
    // Trade statistics
    /// Total number of completed round-trip trades
    pub total_trades: u64,
    /// Number of profitable trades
    pub winning_trades: u64,
    /// Number of losing trades
    pub losing_trades: u64,
    /// Percentage of winning trades (winning_trades / total_trades)
    pub win_rate: f64,
    /// Average profit per winning trade
    pub average_win: f64,
    /// Average loss per losing trade (positive number)
    pub average_loss: f64,
    /// Ratio of gross profits to gross losses
    pub profit_factor: f64,
    /// Expected value per trade (average win * win_rate - average loss * loss_rate)
    pub expectancy: f64,
    
    // Execution stats
    /// Total commission costs paid throughout the backtest
    pub total_commission: f64,
    /// Total slippage costs incurred during execution
    pub total_slippage: f64,
    /// Average time each trade was held in minutes
    pub average_trade_duration: i64,  // minutes
}

/// Complete record of a round-trip trade from entry to exit
/// 
/// Captures all details of a completed trade including timing, prices,
/// and profit/loss calculations for performance analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedTrade {
    /// Timestamp when the position was opened
    pub entry_time: DateTime<Utc>,
    /// Timestamp when the position was closed
    pub exit_time: DateTime<Utc>,
    /// Symbol that was traded
    pub symbol: String,
    /// Direction of the initial trade (Buy for long, Sell for short)
    pub side: OrderSide,
    /// Price at which the position was entered
    pub entry_price: f64,
    /// Price at which the position was exited
    pub exit_price: f64,
    /// Number of shares/units traded
    pub quantity: f64,
    /// Profit or loss in base currency (negative for losses)
    pub pnl: f64,
    /// Return as a percentage (e.g., 0.05 = 5% return)
    pub return_pct: f64,
}

impl std::fmt::Debug for BacktestEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BacktestEngine")
            .field("config", &self.config)
            .field("market_data", &"Arc<MarketDataStore>")
            .field("execution_simulator", &"Arc<ExecutionSimulator>")
            .field("portfolio_tracker", &"Arc<PortfolioTracker>")
            .field("performance_analyzer", &"Arc<PerformanceAnalyzer>")
            .field("state", &*self.state.read())
            .finish()
    }
}

impl std::fmt::Debug for ExecutionSimulator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionSimulator")
            .field("pending_orders", &self.pending_orders.len())
            .field("fill_history", &self.fill_history.read().len())
            .field("config", &self.config)
            .field("rejection_counter", &self.rejection_counter.load(std::sync::atomic::Ordering::Relaxed))
            .finish()
    }
}

impl std::fmt::Debug for PortfolioTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PortfolioTracker")
            .field("positions", &self.positions.len())
            .field("cash_balance", &*self.cash_balance.read())
            .field("equity_curve", &self.equity_curve.read().len())
            .field("transaction_history", &self.transaction_history.read().len())
            .finish()
    }
}

impl std::fmt::Debug for PerformanceAnalyzer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PerformanceAnalyzer")
            .field("metrics_cache", &*self.metrics_cache.read())
            .field("trade_log", &self.trade_log.read().len())
            .finish()
    }
}

impl BacktestEngine {
    /// Creates a new BacktestEngine with the specified configuration
    /// 
    /// Initializes all components needed for backtesting including market data storage,
    /// execution simulation, portfolio tracking, and performance analysis.
    /// 
    /// # Parameters
    /// 
    /// * `config` - Configuration defining the backtest parameters including
    ///   time period, initial capital, commission rates, and simulation settings
    /// 
    /// # Returns
    /// 
    /// A fully initialized BacktestEngine ready to load data and run strategies
    /// 
    /// # Examples
    /// 
    /// ```
    /// use backtesting::{BacktestEngine, BacktestConfig, SlippageModel, DataFrequency};
    /// use chrono::{Utc, Duration};
    /// 
    /// let config = BacktestConfig {
    ///     start_date: Utc::now() - Duration::days(365),
    ///     end_date: Utc::now(),
    ///     initial_capital: 100_000.0,
    ///     commission_rate: 0.001,
    ///     slippage_model: SlippageModel::Fixed { bps: 5.0 },
    ///     data_frequency: DataFrequency::Daily,
    ///     enable_shorting: false,
    ///     margin_requirement: 0.5,
    ///     risk_free_rate: 0.02,
    /// };
    /// 
    /// let engine = BacktestEngine::new(config);
    /// ```
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
    /// Generates trading signals based on current market conditions and portfolio state
    /// 
    /// This is the core method that strategy implementations must provide. It analyzes
    /// the current market snapshot and portfolio state to make trading decisions.
    /// 
    /// # Parameters
    /// 
    /// * `market` - Current market snapshot containing prices and timestamps for all symbols
    /// * `portfolio` - Current portfolio state including cash, positions, and total value
    /// 
    /// # Returns
    /// 
    /// A vector of TradingSignal structs representing the strategy's trading decisions.
    /// An empty vector indicates no trading action should be taken.
    /// 
    /// # Examples
    /// 
    /// ```
    /// use backtesting::{Strategy, TradingSignal, OrderSide, OrderType};
    /// 
    /// struct SimpleStrategy;
    /// 
    /// impl Strategy for SimpleStrategy {
    ///     fn generate_signals(&self, market: &MarketSnapshot, portfolio: &PortfolioState) -> Vec<TradingSignal> {
    ///         // Example: Buy AAPL if we have cash and no position
    ///         if portfolio.cash > 10000.0 && !portfolio.positions.iter().any(|p| p.symbol == "AAPL") {
    ///             vec![TradingSignal {
    ///                 symbol: "AAPL".to_string(),
    ///                 side: OrderSide::Buy,
    ///                 order_type: OrderType::Market,
    ///                 quantity: 100.0,
    ///                 price: None,
    ///             }]
    ///         } else {
    ///             vec![]
    ///         }
    ///     }
    /// }
    /// ```
    fn generate_signals(&self, market: &MarketSnapshot, portfolio: &PortfolioState) -> Vec<TradingSignal>;
}

/// Point-in-time snapshot of market conditions
/// 
/// Contains current prices for all tracked symbols at a specific timestamp.
/// Used by strategies to make trading decisions based on current market state.
#[derive(Debug, Clone)]
pub struct MarketSnapshot {
    /// Timestamp of this market snapshot
    pub timestamp: DateTime<Utc>,
    /// Current prices for all symbols (symbol -> price mapping)
    pub prices: DashMap<String, f64>,
}

/// Current state of the portfolio including cash and positions
/// 
/// Provides a complete view of portfolio holdings at a specific point in time,
/// used by strategies for position sizing and risk management decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioState {
    /// Available cash balance in base currency
    pub cash: f64,
    /// All current open positions in the portfolio
    pub positions: Vec<Position>,
    /// Total portfolio value (cash + position values)
    pub total_value: f64,
}

/// Trading signal generated by a strategy
/// 
/// Represents a strategy's decision to enter or exit a position
#[derive(Debug, Clone)]
pub struct TradingSignal {
    /// Symbol/instrument to trade (e.g., "AAPL", "EURUSD")
    pub symbol: String,
    /// Order side - buy or sell
    pub side: OrderSide,
    /// Type of order to place
    pub order_type: OrderType,
    /// Quantity to trade (must be positive)
    pub quantity: f64,
    /// Limit price for limit orders (None for market orders)
    pub price: Option<f64>,
}

/// Complete results of a backtesting run
/// 
/// Contains all performance metrics, trade history, and portfolio evolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    /// Comprehensive performance and risk metrics
    pub metrics: PerformanceMetrics,
    /// Time series of portfolio value over the backtest period
    pub equity_curve: Vec<(DateTime<Utc>, f64)>,
    /// Complete history of all executed trades
    pub trades: Vec<CompletedTrade>,
    /// Final state of the portfolio at backtest completion
    pub final_portfolio: PortfolioState,
}

// Implementation stubs for sub-components

impl MarketDataStore {
    /// Creates a new MarketDataStore for managing historical market data
    /// 
    /// Initializes empty storage for price data and orderbook snapshots.
    /// The store uses DashMap for thread-safe concurrent access to data.
    /// 
    /// # Returns
    /// 
    /// A new MarketDataStore instance with empty data collections
    /// 
    /// # Examples
    /// 
    /// ```
    /// use backtesting::MarketDataStore;
    /// 
    /// let store = MarketDataStore::new();
    /// // Ready to load OHLCV data and orderbook snapshots
    /// ```
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
    /// Creates a new ExecutionSimulator with the specified configuration
    /// 
    /// The execution simulator handles realistic order processing including
    /// rejections, partial fills, and market impact simulation.
    /// 
    /// # Parameters
    /// 
    /// * `config` - Configuration controlling execution behavior including
    ///   rejection rates, partial fills, and limit order book usage
    /// 
    /// # Returns
    /// 
    /// A new ExecutionSimulator ready to process orders
    /// 
    /// # Examples
    /// 
    /// ```
    /// use backtesting::{ExecutionSimulator, ExecutionConfig};
    /// 
    /// let config = ExecutionConfig {
    ///     use_limit_order_book: true,
    ///     partial_fills: true,
    ///     reject_rate: 0.001, // 0.1% rejection rate
    /// };
    /// let simulator = ExecutionSimulator::new(config);
    /// ```
    pub fn new(config: ExecutionConfig) -> Self {
        use std::sync::atomic::AtomicU64;
        Self {
            pending_orders: Arc::new(DashMap::new()),
            fill_history: Arc::new(RwLock::new(Vec::new())),
            config,
            rejection_counter: Arc::new(AtomicU64::new(0)),
        }
    }
    
    /// Submits an order to the execution simulator
    /// 
    /// Adds the order to the pending orders queue for processing. Orders are
    /// processed during market data updates based on order type and market conditions.
    /// 
    /// # Parameters
    /// 
    /// * `order` - The order to submit for execution
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` if the order was successfully queued
    /// * `Err` if there was an error queuing the order
    /// 
    /// # Examples
    /// 
    /// ```
    /// use backtesting::{Order, OrderSide, OrderType, TimeInForce};
    /// use chrono::Utc;
    /// 
    /// let order = Order {
    ///     id: "order_1".to_string(),
    ///     symbol: "AAPL".to_string(),
    ///     side: OrderSide::Buy,
    ///     order_type: OrderType::Market,
    ///     quantity: 100.0,
    ///     price: None,
    ///     timestamp: Utc::now(),
    ///     time_in_force: TimeInForce::GTC,
    /// };
    /// 
    /// simulator.submit_order(order)?;
    /// ```
    pub fn submit_order(&self, order: Order) -> Result<()> {
        self.pending_orders.insert(order.id.clone(), order);
        Ok(())
    }
    
    /// Processes all pending orders against current market conditions
    /// 
    /// Evaluates each pending order to determine if it should be filled based on
    /// current market prices and order parameters. Handles rejection simulation,
    /// partial fills, and different order types.
    /// 
    /// # Parameters
    /// 
    /// * `market` - Current market snapshot containing prices for all symbols
    /// * `timestamp` - Current simulation timestamp for order processing
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` if all orders were processed successfully
    /// * `Err` if there was an error during order processing
    /// 
    /// # Behavior
    /// 
    /// - Market orders are filled immediately at current market price
    /// - Limit orders are filled only when price conditions are met
    /// - Orders may be rejected based on configured rejection rate
    /// - Partial fills may occur if configured
    /// 
    /// # Examples
    /// 
    /// ```
    /// use backtesting::MarketSnapshot;
    /// use chrono::Utc;
    /// 
    /// let market = MarketSnapshot {
    ///     timestamp: Utc::now(),
    ///     prices: DashMap::new(),
    /// };
    /// market.prices.insert("AAPL".to_string(), 150.0);
    /// 
    /// simulator.process_pending_orders(&market, Utc::now())?;
    /// ```
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
    /// Creates a new PortfolioTracker with the specified initial capital
    /// 
    /// Initializes a portfolio tracker to manage positions, cash balance, 
    /// and transaction history throughout the backtest simulation.
    /// 
    /// # Parameters
    /// 
    /// * `initial_capital` - Starting cash amount for the portfolio
    /// 
    /// # Returns
    /// 
    /// A new PortfolioTracker with the specified initial capital and empty positions
    /// 
    /// # Examples
    /// 
    /// ```
    /// use backtesting::PortfolioTracker;
    /// 
    /// let portfolio = PortfolioTracker::new(100_000.0);
    /// // Portfolio starts with $100,000 cash and no positions
    /// ```
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
    
    /// Updates current prices for all positions and recalculates unrealized P&L
    /// 
    /// Iterates through all open positions and updates their current market prices
    /// from the provided market snapshot. Recalculates unrealized profit/loss
    /// based on the difference between current price and average entry price.
    /// 
    /// # Parameters
    /// 
    /// * `market` - Market snapshot containing current prices for all symbols
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` if all positions were updated successfully
    /// * `Err` if there was an error updating positions
    /// 
    /// # Examples
    /// 
    /// ```
    /// use backtesting::MarketSnapshot;
    /// use dashmap::DashMap;
    /// 
    /// let market = MarketSnapshot {
    ///     timestamp: chrono::Utc::now(),
    ///     prices: DashMap::new(),
    /// };
    /// market.prices.insert("AAPL".to_string(), 155.0);
    /// 
    /// portfolio.update_prices(&market)?;
    /// ```
    pub fn update_prices(&self, market: &MarketSnapshot) -> Result<()> {
        for mut entry in self.positions.iter_mut() {
            if let Some(price) = market.prices.get(entry.key()) {
                entry.current_price = *price;
                entry.unrealized_pnl = (entry.current_price - entry.average_price) * entry.quantity;
            }
        }
        Ok(())
    }
    
    /// Records the current portfolio value at the given timestamp
    /// 
    /// Calculates the total portfolio value (cash + position values) and
    /// adds it to the equity curve for performance tracking and analysis.
    /// 
    /// # Parameters
    /// 
    /// * `timestamp` - The timestamp to associate with this equity point
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` if the equity point was recorded successfully
    /// * `Err` if there was an error recording the equity
    /// 
    /// # Examples
    /// 
    /// ```
    /// use chrono::Utc;
    /// 
    /// let now = Utc::now();
    /// portfolio.record_equity(now)?;
    /// ```
    pub fn record_equity(&self, timestamp: DateTime<Utc>) -> Result<()> {
        let total_value = self.calculate_total_value();
        self.equity_curve.write().push((timestamp, total_value));
        Ok(())
    }
    
    /// Returns a copy of the complete equity curve
    /// 
    /// The equity curve shows portfolio value over time, useful for
    /// performance visualization and drawdown analysis.
    /// 
    /// # Returns
    /// 
    /// A vector of (timestamp, portfolio_value) tuples representing
    /// the portfolio's value progression throughout the backtest
    /// 
    /// # Examples
    /// 
    /// ```
    /// let equity_curve = portfolio.get_equity_curve();
    /// for (timestamp, value) in equity_curve {
    ///     println!("{}: ${:.2}", timestamp, value);
    /// }
    /// ```
    pub fn get_equity_curve(&self) -> Vec<(DateTime<Utc>, f64)> {
        self.equity_curve.read().clone()
    }
    
    /// Returns the current portfolio state snapshot
    /// 
    /// Provides a point-in-time view of the portfolio including cash balance,
    /// all open positions, and total portfolio value. This is useful for
    /// strategy decision-making and portfolio analysis.
    /// 
    /// # Returns
    /// 
    /// A PortfolioState containing current cash, positions, and total value
    /// 
    /// # Examples
    /// 
    /// ```
    /// let state = portfolio.get_current_state();
    /// println!("Cash: ${:.2}, Total Value: ${:.2}", state.cash, state.total_value);
    /// for position in state.positions {
    ///     println!("{}: {} shares @ ${:.2}", position.symbol, position.quantity, position.current_price);
    /// }
    /// ```
    pub fn get_current_state(&self) -> PortfolioState {
        PortfolioState {
            cash: *self.cash_balance.read(),
            positions: self.positions.iter().map(|e| e.value().clone()).collect(),
            total_value: self.calculate_total_value(),
        }
    }
    
    /// Returns the final portfolio state at the end of the backtest
    /// 
    /// Provides the same information as get_current_state() but semantically
    /// indicates this is the final state after backtest completion.
    /// 
    /// # Returns
    /// 
    /// A PortfolioState representing the final portfolio composition
    /// 
    /// # Examples
    /// 
    /// ```
    /// let final_state = portfolio.get_final_state();
    /// println!("Final Portfolio Value: ${:.2}", final_state.total_value);
    /// ```
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
    /// Creates a new PerformanceAnalyzer for calculating backtest metrics
    /// 
    /// Initializes the analyzer with empty metrics cache and trade log.
    /// The analyzer calculates comprehensive performance statistics including
    /// returns, risk metrics, and trading statistics.
    /// 
    /// # Returns
    /// 
    /// A new PerformanceAnalyzer ready to compute performance metrics
    /// 
    /// # Examples
    /// 
    /// ```
    /// use backtesting::PerformanceAnalyzer;
    /// 
    /// let analyzer = PerformanceAnalyzer::new();
    /// // Ready to calculate Sharpe ratio, max drawdown, etc.
    /// ```
    pub fn new() -> Self {
        Self {
            metrics_cache: Arc::new(RwLock::new(PerformanceMetrics::default())),
            trade_log: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Calculates comprehensive performance metrics from portfolio data
    /// 
    /// Analyzes the portfolio's equity curve to compute risk-adjusted returns,
    /// drawdown statistics, volatility measures, and other key performance indicators.
    /// 
    /// # Parameters
    /// 
    /// * `portfolio` - Portfolio tracker containing equity curve and transaction data
    /// * `risk_free_rate` - Annual risk-free rate for Sharpe ratio calculation
    /// 
    /// # Returns
    /// 
    /// * `Ok(PerformanceMetrics)` containing calculated metrics
    /// * `Err` if there was insufficient data or calculation errors
    /// 
    /// # Metrics Calculated
    /// 
    /// - Total and annualized returns
    /// - Volatility and Sharpe ratio
    /// - Maximum drawdown and duration
    /// - Value at Risk (VaR) and Conditional VaR
    /// - Sortino ratio for downside risk
    /// 
    /// # Examples
    /// 
    /// ```
    /// let risk_free_rate = 0.02; // 2% annual
    /// let metrics = analyzer.calculate_metrics(&portfolio, risk_free_rate)?;
    /// println!("Sharpe Ratio: {:.2}", metrics.sharpe_ratio);
    /// println!("Max Drawdown: {:.2}%", metrics.max_drawdown * 100.0);
    /// ```
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
    
    /// Returns a copy of all completed trades from the backtest
    /// 
    /// Provides access to the complete trade log showing entry/exit details,
    /// profit/loss, and timing for each completed round-trip trade.
    /// 
    /// # Returns
    /// 
    /// A vector of CompletedTrade structs containing trade details
    /// 
    /// # Examples
    /// 
    /// ```
    /// let trades = analyzer.get_trades();
    /// for trade in trades {
    ///     println!("Trade: {} {} @ {} -> {} (PnL: {:.2})",
    ///         trade.symbol,
    ///         match trade.side {
    ///             OrderSide::Buy => "LONG",
    ///             OrderSide::Sell => "SHORT",
    ///         },
    ///         trade.entry_price,
    ///         trade.exit_price,
    ///         trade.pnl
    ///     );
    /// }
    /// ```
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
    
    pub(crate) struct Uuid;
    
    impl Uuid {
        pub(crate) fn new_v4() -> String {
            let count = COUNTER.fetch_add(1, Ordering::Relaxed);
            format!("{:016x}", count)
        }
    }
}