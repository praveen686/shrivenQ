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
    /// The trading symbol (e.g., "AAPL", "BTCUSD")
    pub symbol: String,
    /// The type and details of the prediction
    pub prediction: PredictionType,
    /// Confidence score between 0.0 and 1.0
    pub confidence: f64,
    /// Input features used for this prediction
    pub features: Vec<f64>,
    /// Version identifier of the model that made this prediction
    pub model_version: String,
    /// When this prediction was generated
    pub timestamp: DateTime<Utc>,
}

/// Types of predictions that ML models can generate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PredictionType {
    /// Predicts whether price will go up or down
    PriceDirection { 
        /// Probability that price will increase (0.0 to 1.0)
        probability_up: f64 
    },
    /// Predicts a specific price target
    PriceTarget { 
        /// Target price level
        target: f64, 
        /// Time horizon for the prediction in minutes
        horizon_minutes: u32 
    },
    /// Predicts market volatility
    Volatility { 
        /// Predicted volatility value
        predicted_vol: f64 
    },
    /// Predicts market regime
    Regime { 
        /// The predicted market regime
        market_regime: MarketRegime 
    },
    /// Detects market anomalies
    Anomaly { 
        /// Whether an anomaly was detected
        is_anomalous: bool, 
        /// Anomaly score (higher = more anomalous)
        score: f64 
    },
}

/// Different market regimes that can be detected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketRegime {
    /// Market is in a clear trending state
    Trending,
    /// Market is trading within a range
    RangeBound,
    /// Volatility is increasing
    VolatilityExpansion,
    /// Volatility is decreasing
    VolatilityContraction,
}

/// Feature store for real-time feature computation
#[derive(Debug)]
pub struct FeatureStore {
    price_buffers: Arc<DashMap<String, VecDeque<f64>>>,
    volume_buffers: Arc<DashMap<String, VecDeque<f64>>>,
    computed_features: Arc<DashMap<String, Features>>,
    config: FeatureConfig,
}

/// Configuration for feature computation
#[derive(Debug, Clone)]
pub struct FeatureConfig {
    /// Lookback periods for moving averages and returns (e.g., [5, 10, 20, 50, 100])
    pub lookback_periods: Vec<usize>,
    /// Maximum number of data points to keep in history buffers
    pub buffer_size: usize,
    /// How often to update features in milliseconds
    pub update_frequency_ms: u64,
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

/// Computed features for ML model input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Features {
    /// Simple returns for different lookback periods
    pub returns: Vec<f64>,
    /// Logarithmic returns for different lookback periods
    pub log_returns: Vec<f64>,
    /// Moving averages for different periods
    pub moving_averages: Vec<f64>,
    /// Volatility estimates for different periods
    pub volatility: Vec<f64>,
    
    /// Relative Strength Index (RSI) indicator
    pub rsi: f64,
    /// Moving Average Convergence Divergence (MACD) indicator
    pub macd: f64,
    /// Position within Bollinger Bands (-1 to 1)
    pub bollinger_position: f64,
    
    /// Bid-ask spread as a fraction of mid price
    pub bid_ask_spread: f64,
    /// Volume imbalance indicator
    pub volume_imbalance: f64,
    /// Recent trading intensity
    pub trade_intensity: f64,
    
    /// Market beta (correlation with market)
    pub market_beta: f64,
    /// Correlation with market index
    pub correlation_index: f64,
    
    /// When these features were computed
    pub timestamp: DateTime<Utc>,
}

impl FeatureStore {
    /// Create a new feature store with the given configuration
    pub fn new(config: FeatureConfig) -> Self {
        info!("Initializing FeatureStore with config: {:?}", config);
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
            warn!("Insufficient price history for {}: {} samples", symbol, price_buffer.len());
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
    
    /// Convert features to ndarray for ML model input (uses Array1)
    pub fn features_to_array(&self, features: &Features) -> Result<Array1<f64>> {
        let mut feature_vec = Vec::new();
        
        // Add all features to vector
        feature_vec.extend_from_slice(&features.returns);
        feature_vec.extend_from_slice(&features.log_returns);
        feature_vec.extend_from_slice(&features.moving_averages);
        feature_vec.extend_from_slice(&features.volatility);
        feature_vec.push(features.rsi);
        feature_vec.push(features.macd);
        feature_vec.push(features.bollinger_position);
        feature_vec.push(features.bid_ask_spread);
        feature_vec.push(features.volume_imbalance);
        feature_vec.push(features.trade_intensity);
        feature_vec.push(features.market_beta);
        feature_vec.push(features.correlation_index);
        
        Ok(Array1::from_vec(feature_vec))
    }
    
    /// Create batch feature matrix for multiple symbols (uses Array2)
    pub fn create_feature_matrix(&self, symbols: &[String]) -> Result<Array2<f64>> {
        let mut feature_arrays = Vec::new();
        
        for symbol in symbols {
            match self.get_features(symbol) {
                Some(features) => {
                    let array = self.features_to_array(&features)?;
                    feature_arrays.push(array);
                }
                None => {
                    error!("No features available for symbol: {}", symbol);
                    return Err(anyhow::anyhow!("Missing features for {}", symbol))
                        .context("Feature matrix creation failed")?;
                }
            }
        }
        
        if feature_arrays.is_empty() {
            return Err(anyhow::anyhow!("No features available"))
                .context("Cannot create empty feature matrix")?;
        }
        
        let n_features = feature_arrays[0].len();
        let n_samples = feature_arrays.len();
        let flat_data: Vec<f64> = feature_arrays.into_iter().flatten().collect();
        
        Array2::from_shape_vec((n_samples, n_features), flat_data)
            .context("Failed to create 2D feature matrix")
    }
}

/// Model cache using RwLock for thread-safe access
#[derive(Debug)]
pub struct ModelCache {
    models: Arc<RwLock<DashMap<String, CachedModel>>>,
}

/// A cached model with metadata
#[derive(Debug, Clone)]
pub struct CachedModel {
    /// Unique identifier for this model
    pub model_id: String,
    /// Version string for this model
    pub version: String,
    /// Model weights/parameters
    pub weights: Vec<f64>,
    /// When this model was last updated
    pub last_updated: DateTime<Utc>,
}

impl ModelCache {
    /// Create a new empty model cache
    pub fn new() -> Self {
        Self {
            models: Arc::new(RwLock::new(DashMap::new())),
        }
    }
    
    /// Load model with error context
    pub fn load_model(&self, model_id: &str) -> Result<CachedModel> {
        let cache = self.models.read();
        cache.get(model_id)
            .map(|entry| entry.clone())
            .context(format!("Model {} not found in cache", model_id))
    }
    
    /// Update model in cache
    pub fn update_model(&self, model_id: String, model: CachedModel) -> Result<()> {
        let cache = self.models.write();
        cache.insert(model_id.clone(), model);
        info!("Updated model {} in cache", model_id);
        Ok(())
    }
}