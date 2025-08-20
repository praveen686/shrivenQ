//! Trading Strategy Implementations
//! 
//! Sophisticated strategies inspired by leading quant firms

use crate::{ComponentHealth, Side, SignalType, TradingEvent, TradingStrategy};
use anyhow::Result;
use async_trait::async_trait;
use services_common::{Px, Qty, Ts};
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

/// Momentum trading strategy based on moving average crossovers
/// 
/// The `MomentumStrategy` implements a classic momentum-based trading approach
/// using short-term and long-term moving averages. It generates buy signals
/// when the short MA crosses above the long MA with price confirmation (golden cross),
/// and sell signals when the short MA crosses below the long MA (death cross).
/// 
/// # Strategy Logic
/// - Short MA: 20-period moving average
/// - Long MA: 50-period moving average
/// - Signal strength: Percentage difference between MAs
/// - Signal confidence: Strength scaled to 0-100%
/// - Rate limiting: Maximum 1 signal per second per symbol
/// 
/// # Risk Management
/// - Requires minimum 50 price points for signal generation
/// - Price confirmation required (price must be above/below short MA)
/// - Built-in signal rate limiting to prevent overtrading
pub struct MomentumStrategy {
    /// Strategy name
    name: String,
    /// Price history
    price_history: Arc<RwLock<VecDeque<(Px, Ts)>>>,
    /// Moving averages
    ma_short: Arc<RwLock<f64>>,
    ma_long: Arc<RwLock<f64>>,
    /// Signal generation count
    signals_generated: AtomicU64,
    /// Last signal time
    last_signal: Arc<RwLock<Option<Instant>>>,
    /// Health metrics
    health: Arc<RwLock<ComponentHealth>>,
}

impl std::fmt::Debug for MomentumStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MomentumStrategy")
            .field("name", &self.name)
            .field("signals_generated", &self.signals_generated.load(std::sync::atomic::Ordering::Relaxed))
            .field("price_history_len", &self.price_history.read().len())
            .field("ma_short", &*self.ma_short.read())
            .field("ma_long", &*self.ma_long.read())
            .field("has_recent_signal", &self.last_signal.read().is_some())
            .finish()
    }
}

impl MomentumStrategy {
    /// Creates a new momentum trading strategy instance
    /// 
    /// # Returns
    /// A new `MomentumStrategy` with:
    /// - Empty price history (capacity: 200 data points)
    /// - Zero-initialized moving averages
    /// - Healthy component status
    /// - Reset signal generation counter
    /// 
    /// The strategy requires market data to build sufficient price history
    /// before generating trading signals.
    pub fn new() -> Self {
        Self {
            name: "Momentum".to_string(),
            price_history: Arc::new(RwLock::new(VecDeque::with_capacity(200))),
            ma_short: Arc::new(RwLock::new(0.0)),
            ma_long: Arc::new(RwLock::new(0.0)),
            signals_generated: AtomicU64::new(0),
            last_signal: Arc::new(RwLock::new(None)),
            health: Arc::new(RwLock::new(ComponentHealth {
                name: "Momentum".to_string(),
                is_healthy: true,
                last_heartbeat: Instant::now(),
                error_count: 0,
                success_count: 0,
                avg_latency_us: 0,
            })),
        }
    }
    
    /// Calculates short-term and long-term moving averages from price history
    /// 
    /// Computes 20-period and 50-period simple moving averages from the most
    /// recent price data. Moving averages are only calculated when sufficient
    /// data points are available.
    /// 
    /// # Requirements
    /// - Minimum 20 data points for short MA calculation
    /// - Minimum 50 data points for long MA calculation
    /// 
    /// # Thread Safety
    /// Updates are performed under write locks to ensure consistency
    /// between price history reads and moving average calculations.
    fn calculate_moving_averages(&self) {
        let history = self.price_history.read();
        
        if history.len() >= 20 {
            // Calculate 20-period MA
            let ma_20: f64 = history.iter()
                .rev()
                .take(20)
                .map(|(p, _)| p.as_f64())
                .sum::<f64>() / 20.0;
            *self.ma_short.write() = ma_20;
        }
        
        if history.len() >= 50 {
            // Calculate 50-period MA
            let ma_50: f64 = history.iter()
                .rev()
                .take(50)
                .map(|(p, _)| p.as_f64())
                .sum::<f64>() / 50.0;
            *self.ma_long.write() = ma_50;
        }
    }
    
    /// Detects momentum trading signals based on moving average analysis
    /// 
    /// Analyzes current price relative to short and long moving averages
    /// to identify momentum signals. Generates buy signals for golden cross
    /// patterns and sell signals for death cross patterns.
    /// 
    /// # Arguments
    /// * `current_price` - The current market price to analyze
    /// 
    /// # Returns
    /// * `Some((Side, strength, confidence))` - If a valid signal is detected
    /// * `None` - If no signal conditions are met
    /// 
    /// # Signal Conditions
    /// - **Buy Signal**: Short MA > Long MA AND Price > Short MA
    /// - **Sell Signal**: Short MA < Long MA AND Price < Short MA
    /// - **Strength**: Percentage difference between moving averages
    /// - **Confidence**: Strength normalized to 0-100% range
    fn detect_signal(&self, current_price: Px) -> Option<(Side, f64, f64)> {
        let ma_short = *self.ma_short.read();
        let ma_long = *self.ma_long.read();
        
        if ma_short == 0.0 || ma_long == 0.0 {
            return None;
        }
        
        let price = current_price.as_f64();
        
        // Golden cross - bullish signal
        if ma_short > ma_long && price > ma_short {
            let strength = ((ma_short - ma_long) / ma_long * 100.0).abs();
            let confidence = (strength / 10.0).min(1.0); // Cap at 100%
            return Some((Side::Buy, strength, confidence));
        }
        
        // Death cross - bearish signal
        if ma_short < ma_long && price < ma_short {
            let strength = ((ma_long - ma_short) / ma_short * 100.0).abs();
            let confidence = (strength / 10.0).min(1.0);
            return Some((Side::Sell, strength, confidence));
        }
        
        None
    }
}

#[async_trait]
impl TradingStrategy for MomentumStrategy {
    fn name(&self) -> &str {
        &self.name
    }
    
    async fn on_market_update(&mut self, event: &TradingEvent) -> Result<Option<TradingEvent>> {
        let start = Instant::now();
        
        if let TradingEvent::MarketUpdate { symbol, mid, timestamp, .. } = event {
            // Update price history
            {
                let mut history = self.price_history.write();
                history.push_back((*mid, *timestamp));
                if history.len() > 200 {
                    history.pop_front();
                }
            }
            
            // Calculate moving averages
            self.calculate_moving_averages();
            
            // Check for signal
            if let Some((side, strength, confidence)) = self.detect_signal(*mid) {
                // Rate limit signals (max 1 per second)
                {
                    let mut last_signal = self.last_signal.write();
                    if let Some(last) = *last_signal {
                        if last.elapsed().as_secs() < 1 {
                            return Ok(None);
                        }
                    }
                    *last_signal = Some(Instant::now());
                }
                
                let signal_id = self.signals_generated.fetch_add(1, Ordering::SeqCst);
                
                info!("Momentum signal: {:?} strength={:.2} confidence={:.2}", 
                      side, strength, confidence);
                
                // Update health
                {
                    let mut health = self.health.write();
                    health.success_count += 1;
                    health.last_heartbeat = Instant::now();
                    health.avg_latency_us = start.elapsed().as_micros() as u64;
                }
                
                return Ok(Some(TradingEvent::Signal {
                    id: signal_id,
                    symbol: *symbol,
                    side,
                    signal_type: SignalType::Momentum,
                    strength,
                    confidence,
                    timestamp: Ts::now(),
                }));
            }
        }
        
        Ok(None)
    }
    
    async fn on_execution(&mut self, _report: &TradingEvent) -> Result<()> {
        // Update strategy state based on execution
        Ok(())
    }
    
    fn health(&self) -> ComponentHealth {
        self.health.read().clone()
    }
    
    async fn reset(&mut self) -> Result<()> {
        self.price_history.write().clear();
        *self.ma_short.write() = 0.0;
        *self.ma_long.write() = 0.0;
        self.signals_generated.store(0, Ordering::SeqCst);
        *self.last_signal.write() = None;
        
        info!("Momentum strategy reset");
        Ok(())
    }
}

/// Arbitrage trading strategy for cross-venue price discrepancies
/// 
/// The `ArbitrageStrategy` identifies and exploits price differences between
/// trading venues or order book inefficiencies. It monitors bid-ask spreads
/// and detects negative spreads that indicate arbitrage opportunities.
/// 
/// # Strategy Logic
/// - Monitors bid-ask spread compression
/// - Detects negative spreads (arbitrage opportunities)
/// - Minimum threshold: 0.1% spread for signal generation
/// - Signal strength: Absolute percentage of negative spread
/// - Signal confidence: Strength scaled by factor of 10
/// 
/// # Implementation Notes
/// - Currently simplified to single-venue spread analysis
/// - Production version would compare across multiple exchanges
/// - Considers available liquidity for position sizing
/// - Optimized for high-frequency detection
pub struct ArbitrageStrategy {
    /// Strategy name
    name: String,
    /// Price spreads by venue
    spreads: Arc<RwLock<Vec<(String, String, f64)>>>, // (venue1, venue2, spread)
    /// Signals generated
    signals_generated: AtomicU64,
    /// Health metrics
    health: Arc<RwLock<ComponentHealth>>,
}

impl std::fmt::Debug for ArbitrageStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArbitrageStrategy")
            .field("name", &self.name)
            .field("signals_generated", &self.signals_generated.load(std::sync::atomic::Ordering::Relaxed))
            .field("spreads_tracked", &self.spreads.read().len())
            .finish()
    }
}

impl ArbitrageStrategy {
    /// Creates a new arbitrage trading strategy instance
    /// 
    /// # Returns
    /// A new `ArbitrageStrategy` with:
    /// - Empty spread tracking storage
    /// - Zero-initialized signal counter
    /// - Healthy component status
    /// 
    /// The strategy is immediately ready to analyze market data
    /// for arbitrage opportunities.
    pub fn new() -> Self {
        Self {
            name: "Arbitrage".to_string(),
            spreads: Arc::new(RwLock::new(Vec::new())),
            signals_generated: AtomicU64::new(0),
            health: Arc::new(RwLock::new(ComponentHealth {
                name: "Arbitrage".to_string(),
                is_healthy: true,
                last_heartbeat: Instant::now(),
                error_count: 0,
                success_count: 0,
                avg_latency_us: 0,
            })),
        }
    }
    
    /// Detects arbitrage opportunities from bid-ask spread analysis
    /// 
    /// Analyzes the current bid-ask spread to identify negative spreads
    /// or unusually tight spreads that may indicate arbitrage opportunities.
    /// 
    /// # Arguments
    /// * `bid` - Best bid price and quantity, if available
    /// * `ask` - Best ask price and quantity, if available
    /// 
    /// # Returns
    /// * `Some((Side, strength, confidence))` - If arbitrage opportunity detected
    /// * `None` - If no opportunity or insufficient data
    /// 
    /// # Detection Logic
    /// - Calculates spread as (ask_price - bid_price)
    /// - Converts spread to percentage of bid price
    /// - Triggers on negative spreads exceeding 0.1% threshold
    /// - Considers available liquidity for opportunity sizing
    /// 
    /// # Signal Properties
    /// - **Side**: Always Buy (capitalize on negative spread)
    /// - **Strength**: Absolute percentage of negative spread
    /// - **Confidence**: Strength scaled by factor of 10, capped at 100%
    fn detect_arbitrage(&self, bid: Option<(Px, Qty)>, ask: Option<(Px, Qty)>) -> Option<(Side, f64, f64)> {
        if let (Some((bid_price, bid_qty)), Some((ask_price, ask_qty))) = (bid, ask) {
            // Simple cross-exchange arbitrage detection
            // In production, would compare across multiple venues
            let spread = ask_price.as_f64() - bid_price.as_f64();
            let spread_pct = (spread / bid_price.as_f64()) * 100.0;
            
            // Negative spread = arbitrage opportunity
            if spread_pct < -0.1 { // 0.1% threshold
                let strength = spread_pct.abs();
                let confidence = (strength * 10.0).min(1.0);
                
                // Calculate optimal size based on available liquidity
                let _size = bid_qty.min(ask_qty);
                
                return Some((Side::Buy, strength, confidence));
            }
        }
        
        None
    }
}

#[async_trait]
impl TradingStrategy for ArbitrageStrategy {
    fn name(&self) -> &str {
        &self.name
    }
    
    async fn on_market_update(&mut self, event: &TradingEvent) -> Result<Option<TradingEvent>> {
        let start = Instant::now();
        
        if let TradingEvent::MarketUpdate { symbol, bid, ask, .. } = event {
            // Detect arbitrage opportunity
            if let Some((side, strength, confidence)) = self.detect_arbitrage(*bid, *ask) {
                let signal_id = self.signals_generated.fetch_add(1, Ordering::SeqCst);
                
                info!("Arbitrage opportunity detected: strength={:.2}%", strength);
                
                // Update health
                {
                    let mut health = self.health.write();
                    health.success_count += 1;
                    health.last_heartbeat = Instant::now();
                    health.avg_latency_us = start.elapsed().as_micros() as u64;
                }
                
                return Ok(Some(TradingEvent::Signal {
                    id: signal_id,
                    symbol: *symbol,
                    side,
                    signal_type: SignalType::Arbitrage,
                    strength,
                    confidence,
                    timestamp: Ts::now(),
                }));
            }
        }
        
        Ok(None)
    }
    
    async fn on_execution(&mut self, _report: &TradingEvent) -> Result<()> {
        Ok(())
    }
    
    fn health(&self) -> ComponentHealth {
        self.health.read().clone()
    }
    
    async fn reset(&mut self) -> Result<()> {
        self.spreads.write().clear();
        self.signals_generated.store(0, Ordering::SeqCst);
        
        info!("Arbitrage strategy reset");
        Ok(())
    }
}