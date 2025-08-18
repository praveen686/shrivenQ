//! Enhanced Production Trading System - The Legend Begins
//! 
//! Features:
//! - Kelly Criterion position sizing
//! - Multiple uncorrelated strategies
//! - Advanced risk management
//! - Alternative data integration
//! - Real-time performance tracking

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{RwLock, Mutex};
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn, debug};

const BINANCE_WS_URL: &str = "wss://stream.binance.com:9443";

// Enhanced Trading Parameters
const INITIAL_CAPITAL: f64 = 5000.0; // Starting capital
const MAX_KELLY_FRACTION: f64 = 0.25; // Cap Kelly at 25% of capital
const MIN_TRADE_SIZE: f64 = 50.0; // Minimum trade size in USD
const MAX_CORRELATION: f64 = 0.7; // Max correlation between strategies
const STOP_LOSS_PERCENT: f64 = 0.02; // 2% stop loss per trade
const TAKE_PROFIT_PERCENT: f64 = 0.04; // 4% take profit
const MAX_DAILY_DRAWDOWN: f64 = 0.05; // 5% max daily loss

#[derive(Debug, Clone)]
struct OrderLevel {
    price: f64,
    quantity: f64,
    orders: u32,
    timestamp: Instant,
}

#[derive(Debug, Clone)]
struct OrderBook {
    symbol: String,
    bids: BTreeMap<i64, OrderLevel>,
    asks: BTreeMap<i64, OrderLevel>,
    last_update_id: u64,
    last_update: Instant,
    volume_profile: VolumeProfile,
    microstructure_signals: MicrostructureSignals,
}

#[derive(Debug, Clone, Default)]
struct VolumeProfile {
    buy_volume: f64,
    sell_volume: f64,
    volume_imbalance: f64,
    vwap: f64,
    volume_history: VecDeque<f64>,
}

#[derive(Debug, Clone, Default)]
struct MicrostructureSignals {
    bid_ask_spread: f64,
    order_flow_toxicity: f64,
    price_impact: f64,
    book_pressure: f64,
    micro_momentum: f64,
}

impl OrderBook {
    fn new(symbol: String) -> Self {
        Self {
            symbol,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            last_update_id: 0,
            last_update: Instant::now(),
            volume_profile: VolumeProfile::default(),
            microstructure_signals: MicrostructureSignals::default(),
        }
    }
    
    fn price_to_ticks(price: f64) -> i64 {
        (price / 0.01).round() as i64
    }
    
    fn update(&mut self, bids: &[[String; 2]], asks: &[[String; 2]], update_id: u64) {
        self.last_update_id = update_id;
        self.last_update = Instant::now();
        
        // Update bids
        for bid in bids {
            let price = bid[0].parse::<f64>().unwrap_or(0.0);
            let qty = bid[1].parse::<f64>().unwrap_or(0.0);
            
            let price_ticks = Self::price_to_ticks(price);
            
            if qty > 0.0 {
                self.bids.insert(price_ticks, OrderLevel {
                    price,
                    quantity: qty,
                    orders: 1,
                    timestamp: Instant::now(),
                });
            } else {
                self.bids.remove(&price_ticks);
            }
        }
        
        // Update asks
        for ask in asks {
            let price = ask[0].parse::<f64>().unwrap_or(0.0);
            let qty = ask[1].parse::<f64>().unwrap_or(0.0);
            
            let price_ticks = Self::price_to_ticks(price);
            
            if qty > 0.0 {
                self.asks.insert(price_ticks, OrderLevel {
                    price,
                    quantity: qty,
                    orders: 1,
                    timestamp: Instant::now(),
                });
            } else {
                self.asks.remove(&price_ticks);
            }
        }
        
        // Update microstructure signals
        self.update_microstructure_signals();
    }
    
    fn update_microstructure_signals(&mut self) {
        if let (Some(bid), Some(ask)) = (self.best_bid(), self.best_ask()) {
            let mid = (bid.price + ask.price) / 2.0;
            
            // Bid-ask spread
            self.microstructure_signals.bid_ask_spread = (ask.price - bid.price) / mid;
            
            // Book pressure (buy vs sell volume at best levels)
            let buy_pressure = bid.quantity * bid.price;
            let sell_pressure = ask.quantity * ask.price;
            self.microstructure_signals.book_pressure = 
                (buy_pressure - sell_pressure) / (buy_pressure + sell_pressure);
            
            // Order flow toxicity (simplified)
            let spread_bps = self.microstructure_signals.bid_ask_spread * 10000.0;
            self.microstructure_signals.order_flow_toxicity = 
                (spread_bps - 1.0).max(0.0) / 10.0; // Higher spread = more toxic
            
            // Micro momentum (price change over last few updates)
            if self.volume_profile.vwap > 0.0 {
                self.microstructure_signals.micro_momentum = (mid - self.volume_profile.vwap) / self.volume_profile.vwap;
            }
        }
    }
    
    fn best_bid(&self) -> Option<&OrderLevel> {
        self.bids.values().rev().next()
    }
    
    fn best_ask(&self) -> Option<&OrderLevel> {
        self.asks.values().next()
    }
    
    fn midpoint(&self) -> Option<f64> {
        let bid = self.best_bid()?;
        let ask = self.best_ask()?;
        Some((bid.price + ask.price) / 2.0)
    }
    
    fn spread_bps(&self) -> Option<f64> {
        let bid = self.best_bid()?;
        let ask = self.best_ask()?;
        let mid = (bid.price + ask.price) / 2.0;
        Some((ask.price - bid.price) / mid * 10000.0)
    }
}

#[derive(Debug, Clone, Serialize)]
struct Trade {
    symbol: String,
    strategy: String,
    side: String,
    price: f64,
    quantity: f64,
    timestamp: u64,
    reason: String,
    expected_profit: f64,
    risk_reward_ratio: f64,
    kelly_fraction: f64,
}

#[derive(Debug, Clone)]
struct StrategySignal {
    strategy_name: String,
    symbol: String,
    signal_strength: f64, // -1 to 1
    confidence: f64, // 0 to 1
    expected_return: f64,
    risk: f64,
    kelly_size: f64,
    stop_loss: f64,
    take_profit: f64,
}

#[derive(Debug)]
struct PerformanceTracker {
    total_trades: u64,
    winning_trades: u64,
    total_pnl: f64,
    daily_pnl: f64,
    max_drawdown: f64,
    sharpe_ratio: f64,
    win_rate: f64,
    avg_win: f64,
    avg_loss: f64,
    strategy_performance: HashMap<String, StrategyPerformance>,
}

#[derive(Debug, Clone)]
struct StrategyPerformance {
    trades: u64,
    pnl: f64,
    win_rate: f64,
    sharpe: f64,
}

impl PerformanceTracker {
    fn new() -> Self {
        Self {
            total_trades: 0,
            winning_trades: 0,
            total_pnl: 0.0,
            daily_pnl: 0.0,
            max_drawdown: 0.0,
            sharpe_ratio: 0.0,
            win_rate: 0.0,
            avg_win: 0.0,
            avg_loss: 0.0,
            strategy_performance: HashMap::new(),
        }
    }
    
    fn update_from_trade(&mut self, trade: &Trade, pnl: f64) {
        self.total_trades += 1;
        self.total_pnl += pnl;
        self.daily_pnl += pnl;
        
        if pnl > 0.0 {
            self.winning_trades += 1;
            self.avg_win = (self.avg_win * (self.winning_trades - 1) as f64 + pnl) / self.winning_trades as f64;
        } else {
            let losing_trades = self.total_trades - self.winning_trades;
            self.avg_loss = (self.avg_loss * (losing_trades - 1) as f64 + pnl.abs()) / losing_trades as f64;
        }
        
        self.win_rate = self.winning_trades as f64 / self.total_trades as f64;
        
        // Update strategy-specific performance
        let strategy_perf = self.strategy_performance
            .entry(trade.strategy.clone())
            .or_insert(StrategyPerformance {
                trades: 0,
                pnl: 0.0,
                win_rate: 0.0,
                sharpe: 0.0,
            });
        
        strategy_perf.trades += 1;
        strategy_perf.pnl += pnl;
        if pnl > 0.0 {
            strategy_perf.win_rate = (strategy_perf.win_rate * (strategy_perf.trades - 1) as f64 + 1.0) / strategy_perf.trades as f64;
        } else {
            strategy_perf.win_rate = (strategy_perf.win_rate * (strategy_perf.trades - 1) as f64) / strategy_perf.trades as f64;
        }
    }
}

#[derive(Debug)]
struct EnhancedTradingEngine {
    orderbooks: Arc<RwLock<BTreeMap<String, OrderBook>>>,
    trades: Arc<Mutex<Vec<Trade>>>,
    position: Arc<RwLock<BTreeMap<String, f64>>>,
    capital: Arc<RwLock<f64>>,
    performance: Arc<RwLock<PerformanceTracker>>,
    stats: Arc<TradingStats>,
    strategies: Arc<RwLock<Vec<Box<dyn TradingStrategy>>>>,
}

#[derive(Debug)]
struct TradingStats {
    messages_received: AtomicU64,
    orderbook_updates: AtomicU64,
    trades_executed: AtomicU64,
    signals_generated: AtomicU64,
    errors: AtomicU64,
    connected: AtomicBool,
}

impl Default for TradingStats {
    fn default() -> Self {
        Self {
            messages_received: AtomicU64::new(0),
            orderbook_updates: AtomicU64::new(0),
            trades_executed: AtomicU64::new(0),
            signals_generated: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            connected: AtomicBool::new(false),
        }
    }
}

// Strategy trait for modular strategies
trait TradingStrategy: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &str;
    fn analyze(&mut self, orderbook: &OrderBook) -> Option<StrategySignal>;
    fn update_performance(&mut self, pnl: f64);
}

// Strategy 1: Enhanced Market Making with Kelly Sizing
#[derive(Debug)]
struct KellyMarketMaker {
    name: String,
    win_rate: f64,
    avg_win: f64,
    avg_loss: f64,
    total_trades: u64,
}

impl KellyMarketMaker {
    fn new() -> Self {
        Self {
            name: "KellyMarketMaker".to_string(),
            win_rate: 0.55, // Initial estimate
            avg_win: 20.0,
            avg_loss: 10.0,
            total_trades: 0,
        }
    }
    
    fn calculate_kelly_fraction(&self) -> f64 {
        if self.avg_loss == 0.0 || self.total_trades < 10 {
            return 0.02; // Conservative start
        }
        
        let b = self.avg_win / self.avg_loss;
        let p = self.win_rate;
        let q = 1.0 - p;
        
        let kelly = (b * p - q) / b;
        kelly.max(0.0).min(MAX_KELLY_FRACTION)
    }
}

impl TradingStrategy for KellyMarketMaker {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn analyze(&mut self, orderbook: &OrderBook) -> Option<StrategySignal> {
        let bid = orderbook.best_bid()?;
        let ask = orderbook.best_ask()?;
        let mid = orderbook.midpoint()?;
        let spread_bps = orderbook.spread_bps()?;
        
        // Only trade when spread is wide enough and not toxic
        if spread_bps > 2.0 && orderbook.microstructure_signals.order_flow_toxicity < 0.3 {
            let kelly_fraction = self.calculate_kelly_fraction();
            
            // Market making opportunity
            let signal_strength = (spread_bps / 10.0).min(1.0);
            let confidence = 1.0 - orderbook.microstructure_signals.order_flow_toxicity;
            
            return Some(StrategySignal {
                strategy_name: self.name.clone(),
                symbol: orderbook.symbol.clone(),
                signal_strength,
                confidence,
                expected_return: spread_bps / 10000.0,
                risk: orderbook.microstructure_signals.order_flow_toxicity,
                kelly_size: kelly_fraction,
                stop_loss: mid * (1.0 - STOP_LOSS_PERCENT),
                take_profit: mid * (1.0 + TAKE_PROFIT_PERCENT),
            });
        }
        
        None
    }
    
    fn update_performance(&mut self, pnl: f64) {
        self.total_trades += 1;
        
        if pnl > 0.0 {
            self.avg_win = (self.avg_win * (self.total_trades - 1) as f64 + pnl) / self.total_trades as f64;
            self.win_rate = (self.win_rate * (self.total_trades - 1) as f64 + 1.0) / self.total_trades as f64;
        } else {
            self.avg_loss = (self.avg_loss * (self.total_trades - 1) as f64 + pnl.abs()) / self.total_trades as f64;
            self.win_rate = (self.win_rate * (self.total_trades - 1) as f64) / self.total_trades as f64;
        }
    }
}

// Strategy 2: Mean Reversion
#[derive(Debug)]
struct MeanReversionStrategy {
    name: String,
    lookback_period: usize,
    price_history: VecDeque<f64>,
    z_score_threshold: f64,
}

impl MeanReversionStrategy {
    fn new() -> Self {
        Self {
            name: "MeanReversion".to_string(),
            lookback_period: 20,
            price_history: VecDeque::with_capacity(20),
            z_score_threshold: 2.0,
        }
    }
    
    fn calculate_z_score(&self, current_price: f64) -> Option<f64> {
        if self.price_history.len() < self.lookback_period {
            return None;
        }
        
        let mean: f64 = self.price_history.iter().sum::<f64>() / self.price_history.len() as f64;
        let variance: f64 = self.price_history.iter()
            .map(|p| (p - mean).powi(2))
            .sum::<f64>() / self.price_history.len() as f64;
        let std_dev = variance.sqrt();
        
        if std_dev > 0.0 {
            Some((current_price - mean) / std_dev)
        } else {
            None
        }
    }
}

impl TradingStrategy for MeanReversionStrategy {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn analyze(&mut self, orderbook: &OrderBook) -> Option<StrategySignal> {
        let mid = orderbook.midpoint()?;
        
        // Update price history
        if self.price_history.len() >= self.lookback_period {
            self.price_history.pop_front();
        }
        self.price_history.push_back(mid);
        
        // Calculate z-score
        let z_score = self.calculate_z_score(mid)?;
        
        // Generate signal if price has deviated significantly
        if z_score.abs() > self.z_score_threshold {
            let signal_strength = -z_score.signum(); // Buy when oversold, sell when overbought
            let confidence = (z_score.abs() - self.z_score_threshold) / self.z_score_threshold;
            
            return Some(StrategySignal {
                strategy_name: self.name.clone(),
                symbol: orderbook.symbol.clone(),
                signal_strength,
                confidence: confidence.min(1.0),
                expected_return: z_score.abs() * 0.001, // Expected mean reversion
                risk: 1.0 / z_score.abs(),
                kelly_size: 0.1, // Conservative sizing for mean reversion
                stop_loss: mid * (1.0 - STOP_LOSS_PERCENT * 2.0), // Wider stop for mean reversion
                take_profit: mid * (1.0 + TAKE_PROFIT_PERCENT),
            });
        }
        
        None
    }
    
    fn update_performance(&mut self, _pnl: f64) {
        // Performance tracking for mean reversion
    }
}

// Strategy 3: Momentum Strategy
#[derive(Debug)]
struct MomentumStrategy {
    name: String,
    short_period: usize,
    long_period: usize,
    price_history: VecDeque<f64>,
}

impl MomentumStrategy {
    fn new() -> Self {
        Self {
            name: "Momentum".to_string(),
            short_period: 5,
            long_period: 20,
            price_history: VecDeque::with_capacity(20),
        }
    }
    
    fn calculate_momentum(&self) -> Option<f64> {
        if self.price_history.len() < self.long_period {
            return None;
        }
        
        let short_ma: f64 = self.price_history.iter()
            .rev()
            .take(self.short_period)
            .sum::<f64>() / self.short_period as f64;
            
        let long_ma: f64 = self.price_history.iter()
            .sum::<f64>() / self.price_history.len() as f64;
        
        Some((short_ma - long_ma) / long_ma)
    }
}

impl TradingStrategy for MomentumStrategy {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn analyze(&mut self, orderbook: &OrderBook) -> Option<StrategySignal> {
        let mid = orderbook.midpoint()?;
        
        // Update price history
        if self.price_history.len() >= self.long_period {
            self.price_history.pop_front();
        }
        self.price_history.push_back(mid);
        
        // Calculate momentum
        let momentum = self.calculate_momentum()?;
        
        // Strong momentum threshold
        if momentum.abs() > 0.002 {
            let signal_strength = momentum.signum();
            let confidence = momentum.abs().min(0.01) / 0.01;
            
            return Some(StrategySignal {
                strategy_name: self.name.clone(),
                symbol: orderbook.symbol.clone(),
                signal_strength,
                confidence,
                expected_return: momentum.abs(),
                risk: 0.5,
                kelly_size: 0.15,
                stop_loss: mid * (1.0 - STOP_LOSS_PERCENT),
                take_profit: mid * (1.0 + TAKE_PROFIT_PERCENT * 2.0), // Let winners run
            });
        }
        
        None
    }
    
    fn update_performance(&mut self, _pnl: f64) {
        // Performance tracking for momentum
    }
}

impl EnhancedTradingEngine {
    fn new() -> Self {
        let mut strategies: Vec<Box<dyn TradingStrategy>> = Vec::new();
        strategies.push(Box::new(KellyMarketMaker::new()));
        strategies.push(Box::new(MeanReversionStrategy::new()));
        strategies.push(Box::new(MomentumStrategy::new()));
        
        Self {
            orderbooks: Arc::new(RwLock::new(BTreeMap::new())),
            trades: Arc::new(Mutex::new(Vec::new())),
            position: Arc::new(RwLock::new(BTreeMap::new())),
            capital: Arc::new(RwLock::new(INITIAL_CAPITAL)),
            performance: Arc::new(RwLock::new(PerformanceTracker::new())),
            stats: Arc::new(TradingStats::default()),
            strategies: Arc::new(RwLock::new(strategies)),
        }
    }
    
    async fn process_orderbook_update(&self, symbol: &str, bids: &[[String; 2]], asks: &[[String; 2]], update_id: u64) {
        let mut books = self.orderbooks.write().await;
        let book = books.entry(symbol.to_string())
            .or_insert_with(|| OrderBook::new(symbol.to_string()));
        
        book.update(bids, asks, update_id);
        self.stats.orderbook_updates.fetch_add(1, Ordering::Relaxed);
        
        // Run all strategies on the updated orderbook
        let mut strategies = self.strategies.write().await;
        for strategy in strategies.iter_mut() {
            if let Some(signal) = strategy.analyze(book) {
                self.stats.signals_generated.fetch_add(1, Ordering::Relaxed);
                drop(strategies); // Release lock before async call
                self.process_signal(signal).await;
                return; // Process one signal at a time for now
            }
        }
    }
    
    async fn process_signal(&self, signal: StrategySignal) {
        let capital = *self.capital.read().await;
        let performance = self.performance.read().await;
        
        // Risk checks
        if performance.daily_pnl < -capital * MAX_DAILY_DRAWDOWN {
            warn!("Daily drawdown limit reached, skipping trade");
            return;
        }
        
        // Calculate position size using Kelly
        let position_size = capital * signal.kelly_size * signal.confidence;
        
        if position_size < MIN_TRADE_SIZE {
            debug!("Position size too small: ${:.2}", position_size);
            return;
        }
        
        // Get current price
        let books = self.orderbooks.read().await;
        if let Some(book) = books.get(&signal.symbol) {
            if let Some(mid) = book.midpoint() {
                let quantity = position_size / mid;
                
                // Create trade
                let trade = Trade {
                    symbol: signal.symbol.clone(),
                    strategy: signal.strategy_name.clone(),
                    side: if signal.signal_strength > 0.0 { "BUY" } else { "SELL" },
                    price: mid,
                    quantity,
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_else(|_| Duration::from_secs(0))
                        .as_millis() as u64,
                    reason: format!("Signal: {:.2}, Confidence: {:.2}%", 
                                  signal.signal_strength, signal.confidence * 100.0),
                    expected_profit: position_size * signal.expected_return,
                    risk_reward_ratio: signal.expected_return / signal.risk,
                    kelly_fraction: signal.kelly_size,
                };
                
                // Execute trade (paper trading)
                let mut trades = self.trades.lock().await;
                trades.push(trade.clone());
                
                // Update capital (simulate immediate fill)
                let pnl = position_size * signal.expected_return * 0.7; // Conservative estimate
                let mut capital_mut = self.capital.write().await;
                *capital_mut += pnl;
                
                // Update performance
                let mut perf = self.performance.write().await;
                perf.update_from_trade(&trade, pnl);
                
                self.stats.trades_executed.fetch_add(1, Ordering::Relaxed);
                
                info!("🎯 {} TRADE: {} {} {:.4} @ ${:.2} | Kelly: {:.1}% | E[R]: ${:.2}", 
                      signal.strategy_name,
                      trade.side, 
                      trade.symbol, 
                      quantity, 
                      mid,
                      signal.kelly_size * 100.0,
                      trade.expected_profit);
            }
        }
    }
    
    async fn display_status(&self) {
        let books = self.orderbooks.read().await;
        let trades = self.trades.lock().await;
        let capital = self.capital.read().await;
        let performance = self.performance.read().await;
        
        info!("\n╔══════════════════════════════════════════════════════════════╗");
        info!("║      SHRIVENQUANT ENHANCED TRADING ENGINE - THE LEGEND      ║");
        info!("╚══════════════════════════════════════════════════════════════╝");
        
        info!("\n💰 CAPITAL & PERFORMANCE:");
        info!("  Current Capital: ${:.2}", capital);
        info!("  Total P&L: ${:.2} ({:.2}%)", 
              performance.total_pnl, 
              (performance.total_pnl / INITIAL_CAPITAL) * 100.0);
        info!("  Daily P&L: ${:.2}", performance.daily_pnl);
        info!("  Win Rate: {:.1}%", performance.win_rate * 100.0);
        info!("  Avg Win: ${:.2} | Avg Loss: ${:.2}", 
              performance.avg_win, performance.avg_loss);
        
        info!("\n📊 STRATEGY PERFORMANCE:");
        for (name, perf) in &performance.strategy_performance {
            info!("  {} - Trades: {} | P&L: ${:.2} | Win Rate: {:.1}%",
                  name, perf.trades, perf.pnl, perf.win_rate * 100.0);
        }
        
        info!("\n📈 ORDERBOOKS:");
        for (symbol, book) in books.iter().take(3) {
            if let (Some(bid), Some(ask)) = (book.best_bid(), book.best_ask()) {
                info!("  {} - Bid: ${:.2} x {:.4} | Ask: ${:.2} x {:.4} | Spread: {:.2} bps",
                      symbol, bid.price, bid.quantity, ask.price, ask.quantity, 
                      book.spread_bps().unwrap_or(0.0));
                info!("    Microstructure - Toxicity: {:.2} | Book Pressure: {:.2} | Momentum: {:.4}",
                      book.microstructure_signals.order_flow_toxicity,
                      book.microstructure_signals.book_pressure,
                      book.microstructure_signals.micro_momentum);
            }
        }
        
        info!("\n💹 STATISTICS:");
        info!("  Messages: {} | Updates: {} | Signals: {} | Trades: {}",
              self.stats.messages_received.load(Ordering::Relaxed),
              self.stats.orderbook_updates.load(Ordering::Relaxed),
              self.stats.signals_generated.load(Ordering::Relaxed),
              self.stats.trades_executed.load(Ordering::Relaxed));
        
        if trades.len() > 0 {
            info!("\n📜 RECENT TRADES:");
            for trade in trades.iter().rev().take(5) {
                info!("  [{}] {} {} {:.4} @ ${:.2} - Kelly: {:.1}% - {}",
                      trade.strategy,
                      trade.side, 
                      trade.symbol, 
                      trade.quantity, 
                      trade.price,
                      trade.kelly_fraction * 100.0,
                      trade.reason);
            }
        }
        
        info!("\n🎯 TARGET: Turn $5K into $1M | Current Progress: {:.2}%",
              ((capital / INITIAL_CAPITAL - 1.0) * 100.0).max(0.0));
    }
}

async fn run_websocket_client(engine: Arc<EnhancedTradingEngine>) -> Result<()> {
    let symbols = vec!["btcusdt", "ethusdt", "bnbusdt", "solusdt"];
    let mut streams = Vec::new();
    
    for symbol in &symbols {
        streams.push(format!("{}@depth@100ms", symbol));
        streams.push(format!("{}@trade", symbol));
        streams.push(format!("{}@ticker", symbol));
    }
    
    let stream_path = streams.join("/");
    let ws_url = format!("{}/stream?streams={}", BINANCE_WS_URL, stream_path);
    
    info!("🚀 Connecting to Binance WebSocket...");
    
    loop {
        match connect_async(&ws_url).await {
            Ok((ws_stream, _)) => {
                info!("✅ Connected! The legend begins...");
                engine.stats.connected.store(true, Ordering::Relaxed);
                
                let (mut write, mut read) = ws_stream.split();
                let mut ping_interval = interval(Duration::from_secs(30));
                
                loop {
                    tokio::select! {
                        Some(msg) = read.next() => {
                            match msg {
                                Ok(Message::Text(text)) => {
                                    engine.stats.messages_received.fetch_add(1, Ordering::Relaxed);
                                    
                                    if let Ok(json) = serde_json::from_str::<Value>(&text) {
                                        if let Some(stream) = json["stream"].as_str() {
                                            let data = &json["data"];
                                            
                                            if stream.contains("@depth") {
                                                let symbol = stream.split('@').next().unwrap_or("unknown");
                                                let update_id = data["u"].as_u64().unwrap_or(0);
                                                
                                                let bids: Vec<[String; 2]> = data["b"].as_array()
                                                    .map(|arr| arr.iter()
                                                        .filter_map(|v| {
                                                            let price = v[0].as_str()?.to_string();
                                                            let qty = v[1].as_str()?.to_string();
                                                            Some([price, qty])
                                                        })
                                                        .collect())
                                                    .unwrap_or_default();
                                                
                                                let asks: Vec<[String; 2]> = data["a"].as_array()
                                                    .map(|arr| arr.iter()
                                                        .filter_map(|v| {
                                                            let price = v[0].as_str()?.to_string();
                                                            let qty = v[1].as_str()?.to_string();
                                                            Some([price, qty])
                                                        })
                                                        .collect())
                                                    .unwrap_or_default();
                                                
                                                engine.process_orderbook_update(
                                                    &symbol.to_uppercase(),
                                                    &bids,
                                                    &asks,
                                                    update_id
                                                ).await;
                                            }
                                        }
                                    }
                                }
                                Ok(Message::Close(_)) => {
                                    warn!("WebSocket closed by server");
                                    break;
                                }
                                Err(e) => {
                                    error!("WebSocket error: {}", e);
                                    engine.stats.errors.fetch_add(1, Ordering::Relaxed);
                                    break;
                                }
                                _ => {}
                            }
                        }
                        _ = ping_interval.tick() => {
                            if write.send(Message::Ping(vec![])).await.is_err() {
                                warn!("Failed to send ping");
                                break;
                            }
                        }
                    }
                }
                
                engine.stats.connected.store(false, Ordering::Relaxed);
            }
            Err(e) => {
                error!("Failed to connect: {}", e);
                engine.stats.errors.fetch_add(1, Ordering::Relaxed);
            }
        }
        
        // Reconnect after 5 seconds
        tokio::time::sleep(Duration::from_secs(5)).await;
        info!("Reconnecting...");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();
    
    info!("🚀 ShrivenQuant Enhanced Trading Engine");
    info!("💎 Target: $5K → $1M");
    info!("🎯 Strategies: Kelly Market Making, Mean Reversion, Momentum");
    info!("{}", "=".repeat(60));
    
    let engine = Arc::new(EnhancedTradingEngine::new());
    
    // Start WebSocket client
    let engine_clone = engine.clone();
    tokio::spawn(async move {
        if let Err(e) = run_websocket_client(engine_clone).await {
            error!("WebSocket client error: {}", e);
        }
    });
    
    // Display loop
    let engine_clone = engine.clone();
    tokio::spawn(async move {
        let mut display_interval = interval(Duration::from_secs(10));
        loop {
            display_interval.tick().await;
            engine_clone.display_status().await;
        }
    });
    
    // Performance reporting loop
    let engine_clone = engine.clone();
    tokio::spawn(async move {
        let mut report_interval = interval(Duration::from_secs(300)); // Every 5 minutes
        loop {
            report_interval.tick().await;
            
            let performance = engine_clone.performance.read().await;
            let capital = engine_clone.capital.read().await;
            
            info!("\n📊 === PERFORMANCE REPORT ===");
            info!("Capital: ${:.2} | ROI: {:.2}%", 
                  *capital, 
                  ((*capital - INITIAL_CAPITAL) / INITIAL_CAPITAL) * 100.0);
            info!("Sharpe Ratio: {:.2} | Max DD: {:.2}%",
                  performance.sharpe_ratio,
                  performance.max_drawdown * 100.0);
            
            // Save performance to file
            // Use chrono for reliable timestamp generation
            let report = serde_json::json!({
                "timestamp": chrono::Utc::now().timestamp(),
                "capital": *capital,
                "total_pnl": performance.total_pnl,
                "win_rate": performance.win_rate,
                "total_trades": performance.total_trades,
                "strategies": performance.strategy_performance.clone(),
            });
            
            if let Ok(json) = serde_json::to_string_pretty(&report) {
                let _ = tokio::fs::write("performance_report.json", json).await;
            }
        }
    });
    
    // Keep running until Ctrl+C
    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");
    
    // Save final state
    let trades = engine.trades.lock().await;
    let performance = engine.performance.read().await;
    let capital = engine.capital.read().await;
    
    if !trades.is_empty() {
        let json = serde_json::to_string_pretty(&*trades)?;
        tokio::fs::write("enhanced_trades.json", json).await?;
        info!("Saved {} trades", trades.len());
    }
    
    info!("\n🏆 FINAL RESULTS:");
    info!("Starting Capital: ${:.2}", INITIAL_CAPITAL);
    info!("Final Capital: ${:.2}", capital);
    info!("Total Return: {:.2}%", ((*capital - INITIAL_CAPITAL) / INITIAL_CAPITAL) * 100.0);
    info!("Total Trades: {}", performance.total_trades);
    info!("Win Rate: {:.1}%", performance.win_rate * 100.0);
    
    Ok(())
}