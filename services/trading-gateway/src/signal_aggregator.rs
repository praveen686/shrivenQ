//! Signal Aggregator - Combines signals from multiple strategies

use crate::{OrderType, Side, SignalType, TimeInForce, TradingEvent};
use anyhow::Result;
use services_common::{Qty, Symbol};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::info;

/// Signal aggregator for combining strategy signals
pub struct SignalAggregator {
    /// Recent signals by symbol
    recent_signals: Arc<RwLock<HashMap<Symbol, Vec<SignalEntry>>>>,
    /// Signal weights by type
    signal_weights: HashMap<SignalType, f64>,
    /// Minimum confidence threshold
    min_confidence: f64,
    /// Signal expiry duration
    signal_expiry: Duration,
}

/// Signal entry with metadata
#[derive(Debug, Clone)]
struct SignalEntry {
    signal_type: SignalType,
    side: Side,
    strength: f64,
    confidence: f64,
    timestamp: Instant,
}

impl SignalAggregator {
    /// Create new signal aggregator
    pub fn new() -> Self {
        let mut signal_weights = HashMap::new();
        signal_weights.insert(SignalType::Momentum, 0.3);
        signal_weights.insert(SignalType::MeanReversion, 0.2);
        signal_weights.insert(SignalType::MarketMaking, 0.15);
        signal_weights.insert(SignalType::Arbitrage, 0.5);
        signal_weights.insert(SignalType::ToxicFlow, -0.3); // Negative weight
        signal_weights.insert(SignalType::Microstructure, 0.25);
        
        Self {
            recent_signals: Arc::new(RwLock::new(HashMap::new())),
            signal_weights,
            min_confidence: 0.6,
            signal_expiry: Duration::from_secs(5),
        }
    }
    
    /// Aggregate signal with existing signals
    pub async fn aggregate(&self, signal: TradingEvent) -> Result<Option<TradingEvent>> {
        if let TradingEvent::Signal {
            id,
            symbol,
            side,
            signal_type,
            strength,
            confidence,
            ..
        } = signal {
            // Store signal
            {
                let mut signals = self.recent_signals.write();
                let symbol_signals = signals.entry(symbol).or_insert_with(Vec::new);
                
                // Remove expired signals
                let now = Instant::now();
                symbol_signals.retain(|s| now.duration_since(s.timestamp) < self.signal_expiry);
                
                // Add new signal
                symbol_signals.push(SignalEntry {
                    signal_type,
                    side,
                    strength,
                    confidence,
                    timestamp: now,
                });
            }
            
            // Aggregate signals
            let (should_trade, final_side, final_strength) = self.calculate_aggregate(symbol)?;
            
            if should_trade {
                info!("Aggregated signal for {}: {:?} strength={:.2}", 
                      symbol, final_side, final_strength);
                
                // Generate order request
                let quantity = self.calculate_position_size(final_strength);
                
                return Ok(Some(TradingEvent::OrderRequest {
                    id,
                    symbol,
                    side: final_side,
                    order_type: OrderType::Market,
                    quantity,
                    price: None,
                    time_in_force: TimeInForce::Ioc,
                    strategy_id: "Aggregated".to_string(),
                }));
            }
        }
        
        Ok(None)
    }
    
    /// Calculate aggregate signal
    fn calculate_aggregate(&self, symbol: Symbol) -> Result<(bool, Side, f64)> {
        let signals = self.recent_signals.read();
        
        if let Some(symbol_signals) = signals.get(&symbol) {
            if symbol_signals.is_empty() {
                return Ok((false, Side::Buy, 0.0));
            }
            
            let mut buy_score = 0.0;
            let mut sell_score = 0.0;
            let mut total_weight = 0.0;
            
            for signal in symbol_signals {
                let weight = self.signal_weights
                    .get(&signal.signal_type)
                    .copied()
                    .unwrap_or(0.1);
                
                let score = weight * signal.strength * signal.confidence;
                
                match signal.side {
                    Side::Buy => buy_score += score,
                    Side::Sell => sell_score += score,
                }
                
                total_weight += weight.abs();
            }
            
            // Normalize scores
            if total_weight > 0.0 {
                buy_score /= total_weight;
                sell_score /= total_weight;
            }
            
            // Determine final signal
            let net_score = buy_score - sell_score;
            let abs_score = net_score.abs();
            
            // Check confidence threshold
            if abs_score >= self.min_confidence {
                let side = if net_score > 0.0 { Side::Buy } else { Side::Sell };
                return Ok((true, side, abs_score));
            }
        }
        
        Ok((false, Side::Buy, 0.0))
    }
    
    /// Calculate position size based on signal strength
    fn calculate_position_size(&self, strength: f64) -> Qty {
        // Base size with scaling by strength
        let base_size = 10000; // 1 unit
        let scaled_size = (base_size as f64 * strength.min(2.0)) as i64;
        Qty::from_i64(scaled_size)
    }
}