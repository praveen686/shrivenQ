//! Trading Strategy Implementations
//! 
//! Sophisticated strategies inspired by leading quant firms

use crate::{ComponentHealth, Side, SignalType, TradingEvent, TradingStrategy};
use anyhow::Result;
use async_trait::async_trait;
use common::{Px, Qty, Ts};
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info};

/// Momentum trading strategy
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

impl MomentumStrategy {
    /// Create new momentum strategy
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
    
    /// Calculate moving averages
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
    
    /// Detect momentum signal
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
                history.push_back((mid, *timestamp));
                if history.len() > 200 {
                    history.pop_front();
                }
            }
            
            // Calculate moving averages
            self.calculate_moving_averages();
            
            // Check for signal
            if let Some((side, strength, confidence)) = self.detect_signal(mid) {
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

/// Arbitrage trading strategy
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

impl ArbitrageStrategy {
    /// Create new arbitrage strategy
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
    
    /// Detect arbitrage opportunity
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
                let size = bid_qty.min(ask_qty);
                
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