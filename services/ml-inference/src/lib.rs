//! ML Inference Service for Real-time Trading Predictions
//! 
//! Provides:
//! - Real-time feature engineering
//! - Model serving for multiple strategies
//! - Signal generation from ML predictions
//! - Online learning capabilities

pub mod features;
pub mod models;
pub mod serving;

use anyhow::{Result, Context};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use ndarray::{Array1, Array2};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tracing::{info, warn, error};

/// ML prediction signal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MLSignal {
    pub symbol: String,
    pub prediction: PredictionType,
    pub confidence: f64,
    pub features: Vec<f64>,
    pub model_version: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PredictionType {
    PriceDirection { probability_up: f64 },
    PriceTarget { target: f64, horizon_minutes: u32 },
    Volatility { predicted_vol: f64 },
    Regime { market_regime: MarketRegime },
    Anomaly { is_anomalous: bool, score: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketRegime {
    Trending,
    RangeBound,
    VolatilityExpansion,
    VolatilityContraction,
}

/// Feature store for real-time feature computation
pub struct FeatureStore {
    price_buffers: Arc<DashMap<String, VecDeque<f64>>>,
    volume_buffers: Arc<DashMap<String, VecDeque<f64>>>,
    computed_features: Arc<DashMap<String, Features>>,
    config: FeatureConfig,
}

#[derive(Debug, Clone)]
pub struct FeatureConfig {
    pub lookback_periods: Vec<usize>,  // [5, 10, 20, 50, 100]
    pub buffer_size: usize,            // Max history to keep
    pub update_frequency_ms: u64,      // Feature update frequency
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            lookback_periods: vec![5, 10, 20, 50, 100, 200],
            buffer_size: 500,
            update_frequency_ms: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Features {
    // Price features
    pub returns: Vec<f64>,
    pub log_returns: Vec<f64>,
    pub moving_averages: Vec<f64>,
    pub volatility: Vec<f64>,
    
    // Technical indicators
    pub rsi: f64,
    pub macd: f64,
    pub bollinger_position: f64,
    
    // Microstructure features
    pub bid_ask_spread: f64,
    pub volume_imbalance: f64,
    pub trade_intensity: f64,
    
    // Market features
    pub market_beta: f64,
    pub correlation_index: f64,
    
    // Computed timestamp
    pub timestamp: DateTime<Utc>,
}

impl FeatureStore {
    pub fn new(config: FeatureConfig) -> Self {
        Self {
            price_buffers: Arc::new(DashMap::new()),
            volume_buffers: Arc::new(DashMap::new()),
            computed_features: Arc::new(DashMap::new()),
            config,
        }
    }
    
    /// Update price data for a symbol
    pub fn update_price(&self, symbol: &str, price: f64, volume: f64) {
        // Update price buffer
        let mut price_buffer = self.price_buffers.entry(symbol.to_string())
            .or_insert_with(|| VecDeque::with_capacity(self.config.buffer_size));
        
        price_buffer.push_back(price);
        if price_buffer.len() > self.config.buffer_size {
            price_buffer.pop_front();
        }
        
        // Update volume buffer
        let mut volume_buffer = self.volume_buffers.entry(symbol.to_string())
            .or_insert_with(|| VecDeque::with_capacity(self.config.buffer_size));
        
        volume_buffer.push_back(volume);
        if volume_buffer.len() > self.config.buffer_size {
            volume_buffer.pop_front();
        }
        
        // Trigger feature computation
        if let Some(features) = self.compute_features(symbol) {
            self.computed_features.insert(symbol.to_string(), features);
        }
    }
    
    /// Compute features for a symbol
    fn compute_features(&self, symbol: &str) -> Option<Features> {
        let price_buffer = self.price_buffers.get(symbol)?;
        let volume_buffer = self.volume_buffers.get(symbol)?;
        
        if price_buffer.len() < 200 {
            return None; // Need minimum history
        }
        
        let prices: Vec<f64> = price_buffer.iter().copied().collect();
        let volumes: Vec<f64> = volume_buffer.iter().copied().collect();
        
        // Compute returns
        let returns = self.calculate_returns(&prices);
        let log_returns = self.calculate_log_returns(&prices);
        
        // Compute moving averages
        let moving_averages = self.config.lookback_periods.iter()
            .map(|&period| self.calculate_sma(&prices, period))
            .collect();
        
        // Compute volatility
        let volatility = self.config.lookback_periods.iter()
            .map(|&period| self.calculate_volatility(&returns, period))
            .collect();
        
        // Technical indicators
        let rsi = self.calculate_rsi(&prices, 14);
        let macd = self.calculate_macd(&prices);
        let bollinger_position = self.calculate_bollinger_position(&prices, 20);
        
        // Microstructure (simplified)
        let bid_ask_spread = 0.0001; // Placeholder
        let volume_imbalance = self.calculate_volume_imbalance(&volumes);
        let trade_intensity = volumes.last().copied().unwrap_or(0.0) / 
            volumes.iter().sum::<f64>().max(1.0);
        
        Some(Features {
            returns,
            log_returns,
            moving_averages,
            volatility,
            rsi,
            macd,
            bollinger_position,
            bid_ask_spread,
            volume_imbalance,
            trade_intensity,
            market_beta: 1.0,  // Placeholder
            correlation_index: 0.5,  // Placeholder
            timestamp: Utc::now(),
        })
    }
    
    /// Calculate simple returns
    fn calculate_returns(&self, prices: &[f64]) -> Vec<f64> {
        self.config.lookback_periods.iter().map(|&period| {
            if prices.len() > period {
                let old_price = prices[prices.len() - period - 1];
                let new_price = prices[prices.len() - 1];
                (new_price - old_price) / old_price
            } else {
                0.0
            }
        }).collect()
    }
    
    /// Calculate log returns
    fn calculate_log_returns(&self, prices: &[f64]) -> Vec<f64> {
        self.config.lookback_periods.iter().map(|&period| {
            if prices.len() > period {
                let old_price = prices[prices.len() - period - 1];
                let new_price = prices[prices.len() - 1];
                (new_price / old_price).ln()
            } else {
                0.0
            }
        }).collect()
    }
    
    /// Calculate simple moving average
    fn calculate_sma(&self, prices: &[f64], period: usize) -> f64 {
        if prices.len() >= period {
            let start = prices.len() - period;
            prices[start..].iter().sum::<f64>() / period as f64
        } else {
            prices.iter().sum::<f64>() / prices.len().max(1) as f64
        }
    }
    
    /// Calculate volatility (standard deviation of returns)
    fn calculate_volatility(&self, returns: &[f64], period: usize) -> f64 {
        if returns.len() >= period && period > 0 {
            let mean = returns.iter().take(period).sum::<f64>() / period as f64;
            let variance = returns.iter()
                .take(period)
                .map(|r| (r - mean).powi(2))
                .sum::<f64>() / period as f64;
            variance.sqrt()
        } else {
            0.0
        }
    }
    
    /// Calculate RSI
    fn calculate_rsi(&self, prices: &[f64], period: usize) -> f64 {
        if prices.len() < period + 1 {
            return 50.0;
        }
        
        let mut gains = 0.0;
        let mut losses = 0.0;
        
        for i in (prices.len() - period)..prices.len() {
            let change = prices[i] - prices[i - 1];
            if change > 0.0 {
                gains += change;
            } else {
                losses += change.abs();
            }
        }
        
        let avg_gain = gains / period as f64;
        let avg_loss = losses / period as f64;
        
        if avg_loss == 0.0 {
            100.0
        } else {
            let rs = avg_gain / avg_loss;
            100.0 - (100.0 / (1.0 + rs))
        }
    }
    
    /// Calculate MACD
    fn calculate_macd(&self, prices: &[f64]) -> f64 {
        let ema_12 = self.calculate_ema(prices, 12);
        let ema_26 = self.calculate_ema(prices, 26);
        ema_12 - ema_26
    }
    
    /// Calculate EMA
    fn calculate_ema(&self, prices: &[f64], period: usize) -> f64 {
        if prices.is_empty() {
            return 0.0;
        }
        
        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = prices[0];
        
        for price in prices.iter().skip(1) {
            ema = (price - ema) * multiplier + ema;
        }
        
        ema
    }
    
    /// Calculate Bollinger Band position
    fn calculate_bollinger_position(&self, prices: &[f64], period: usize) -> f64 {
        let sma = self.calculate_sma(prices, period);
        let std_dev = self.calculate_std_dev(prices, period);
        
        if std_dev == 0.0 {
            return 0.0;
        }
        
        let current_price = prices.last().copied().unwrap_or(sma);
        (current_price - sma) / (2.0 * std_dev)
    }
    
    /// Calculate standard deviation
    fn calculate_std_dev(&self, prices: &[f64], period: usize) -> f64 {
        if prices.len() < period {
            return 0.0;
        }
        
        let start = prices.len() - period;
        let slice = &prices[start..];
        let mean = slice.iter().sum::<f64>() / period as f64;
        let variance = slice.iter()
            .map(|p| (p - mean).powi(2))
            .sum::<f64>() / period as f64;
        
        variance.sqrt()
    }
    
    /// Calculate volume imbalance
    fn calculate_volume_imbalance(&self, volumes: &[f64]) -> f64 {
        if volumes.len() < 20 {
            return 0.0;
        }
        
        let recent_avg = volumes.iter().rev().take(5).sum::<f64>() / 5.0;
        let historical_avg = volumes.iter().rev().take(20).sum::<f64>() / 20.0;
        
        if historical_avg == 0.0 {
            0.0
        } else {
            (recent_avg - historical_avg) / historical_avg
        }
    }
    
    /// Get features for a symbol
    pub fn get_features(&self, symbol: &str) -> Option<Features> {
        self.computed_features.get(symbol).map(|f| f.clone())
    }
}