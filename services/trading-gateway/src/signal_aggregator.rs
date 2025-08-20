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
/// 
/// The `SignalAggregator` consolidates signals from multiple trading strategies
/// and determines whether to generate trading orders based on weighted signal consensus.
/// It maintains a time-windowed history of signals and applies configurable weights
/// to different signal types to compute aggregate trading decisions.
/// 
/// # Features
/// - Time-based signal expiry (default 5 seconds)
/// - Weighted signal aggregation by signal type
/// - Confidence threshold filtering
/// - Position sizing based on signal strength
/// - Thread-safe signal storage with RwLock
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

impl std::fmt::Debug for SignalAggregator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let signal_count = self.recent_signals.read().len();
        f.debug_struct("SignalAggregator")
            .field("active_symbols", &signal_count)
            .field("signal_weights", &self.signal_weights)
            .field("min_confidence", &self.min_confidence)
            .field("signal_expiry", &self.signal_expiry)
            .finish()
    }
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
    /// Creates a new signal aggregator with default configuration
    /// 
    /// # Returns
    /// A new `SignalAggregator` instance with:
    /// - Pre-configured signal weights for different signal types
    /// - Minimum confidence threshold of 0.6
    /// - Signal expiry duration of 5 seconds
    /// - Empty signal storage
    /// 
    /// # Signal Weights
    /// - Momentum: 0.3
    /// - MeanReversion: 0.2  
    /// - MarketMaking: 0.15
    /// - Arbitrage: 0.5
    /// - ToxicFlow: -0.3 (negative weight)
    /// - Microstructure: 0.25
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
    
    /// Aggregates a new signal with existing signals for the same symbol
    /// 
    /// This method processes incoming trading signals, stores them in the symbol's
    /// signal history, removes expired signals, and calculates whether the aggregated
    /// signal strength meets the confidence threshold for generating a trading order.
    /// 
    /// # Arguments
    /// * `signal` - The trading signal event to aggregate
    /// 
    /// # Returns
    /// * `Ok(Some(TradingEvent::OrderRequest))` - If aggregated signals meet confidence threshold
    /// * `Ok(None)` - If signals don't meet threshold or no trading action needed
    /// * `Err(anyhow::Error)` - If aggregation calculation fails
    /// 
    /// # Behavior
    /// 1. Extracts signal details from the trading event
    /// 2. Adds signal to symbol's history and removes expired signals
    /// 3. Calculates weighted aggregate score for buy/sell sides
    /// 4. Generates market order if aggregate confidence exceeds minimum threshold
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
    
    /// Calculates aggregate signal strength and direction for a symbol
    /// 
    /// Computes weighted buy/sell scores from all active signals for the given symbol,
    /// normalizes the scores, and determines if the net signal meets the confidence threshold.
    /// 
    /// # Arguments
    /// * `symbol` - The trading symbol to calculate aggregation for
    /// 
    /// # Returns
    /// A tuple containing:
    /// * `bool` - Whether to execute a trade based on signal strength
    /// * `Side` - The recommended trading side (Buy/Sell)
    /// * `f64` - The normalized signal strength (0.0 to 1.0+)
    /// 
    /// # Algorithm
    /// 1. Retrieves all active signals for the symbol
    /// 2. Applies signal type weights to compute buy/sell scores
    /// 3. Normalizes scores by total weight
    /// 4. Compares net score against minimum confidence threshold
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
    
    /// Calculates position size based on aggregated signal strength
    /// 
    /// Determines the quantity to trade by scaling a base position size
    /// according to the strength of the aggregated signal, with a maximum
    /// scaling factor to prevent excessive position sizes.
    /// 
    /// # Arguments
    /// * `strength` - The normalized signal strength (typically 0.0 to 2.0)
    /// 
    /// # Returns
    /// The calculated position size as a `Qty`
    /// 
    /// # Implementation
    /// - Base size: 10,000 units (1.0 in decimal representation)
    /// - Maximum strength multiplier: 2.0
    /// - Formula: `base_size * min(strength, 2.0)`
    fn calculate_position_size(&self, strength: f64) -> Qty {
        // Base size with scaling by strength
        let base_size = 10000; // 1 unit
        let scaled_size = (base_size as f64 * strength.min(2.0)) as i64;
        Qty::from_i64(scaled_size)
    }
}