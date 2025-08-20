//! ML Models for Trading Predictions

use anyhow::Result;
use ndarray::Array1;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Base trait for all ML models
pub trait TradingModel: Send + Sync {
    /// Make a prediction given features
    fn predict(&self, features: &Array1<f64>) -> Result<ModelOutput>;
    
    /// Get model metadata
    fn metadata(&self) -> ModelMetadata;
    
    /// Update model with new data (for online learning)
    fn update(&mut self, features: &Array1<f64>, target: f64) -> Result<()>;
}

/// Output from a machine learning model prediction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelOutput {
    /// Named predictions (e.g., "price_change": 0.05)
    pub predictions: HashMap<String, f64>,
    /// Confidence score for the prediction (0.0 to 1.0)
    pub confidence: f64,
    /// Optional feature importance scores
    pub feature_importance: Option<Vec<f64>>,
}

/// Metadata describing a machine learning model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    /// Human-readable name of the model
    pub name: String,
    /// Version string for this model
    pub version: String,
    /// Type of machine learning algorithm
    pub model_type: ModelType,
    /// Names of input features expected by the model
    pub input_features: Vec<String>,
    /// Type of output produced by the model
    pub output_type: OutputType,
    /// Number of samples used to train this model
    pub training_samples: u64,
    /// When this model was last updated
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// Types of machine learning algorithms supported
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelType {
    /// Linear regression model
    LinearRegression,
    /// Random forest ensemble model
    RandomForest,
    /// Feed-forward neural network
    NeuralNetwork,
    /// Long Short-Term Memory network
    LSTM,
    /// Extreme Gradient Boosting
    XGBoost,
    /// Ensemble of multiple models
    Ensemble,
}

/// Types of outputs that models can produce
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputType {
    /// Continuous value prediction
    Regression,
    /// Discrete class prediction (Up/Down/Neutral)
    Classification,
    /// Probability distribution over outcomes
    Probability,
    /// Multiple simultaneous predictions
    MultiOutput,
}

/// Simple linear model for demonstration
#[derive(Debug)]
pub struct LinearModel {
    weights: Array1<f64>,
    bias: f64,
    metadata: ModelMetadata,
}

impl LinearModel {
    /// Create a new linear model with the specified input dimension
    pub fn new(input_dim: usize) -> Self {
        Self {
            weights: Array1::zeros(input_dim),
            bias: 0.0,
            metadata: ModelMetadata {
                name: "LinearRegression".to_string(),
                version: "1.0.0".to_string(),
                model_type: ModelType::LinearRegression,
                input_features: (0..input_dim).map(|i| format!("feature_{}", i)).collect(),
                output_type: OutputType::Regression,
                training_samples: 0,
                last_updated: chrono::Utc::now(),
            },
        }
    }
    
    /// Initialize with pretrained weights
    pub fn from_weights(weights: Array1<f64>, bias: f64) -> Self {
        let input_dim = weights.len();
        Self {
            weights,
            bias,
            metadata: ModelMetadata {
                name: "LinearRegression".to_string(),
                version: "1.0.0".to_string(),
                model_type: ModelType::LinearRegression,
                input_features: (0..input_dim).map(|i| format!("feature_{}", i)).collect(),
                output_type: OutputType::Regression,
                training_samples: 0,
                last_updated: chrono::Utc::now(),
            },
        }
    }
}

impl TradingModel for LinearModel {
    fn predict(&self, features: &Array1<f64>) -> Result<ModelOutput> {
        if features.len() != self.weights.len() {
            anyhow::bail!("Feature dimension mismatch");
        }
        
        let prediction = features.dot(&self.weights) + self.bias;
        
        // Simple confidence based on prediction magnitude
        let confidence = 1.0 / (1.0 + (-prediction.abs()).exp());
        
        let mut predictions = HashMap::new();
        predictions.insert("price_change".to_string(), prediction);
        
        Ok(ModelOutput {
            predictions,
            confidence,
            feature_importance: Some(self.weights.to_vec()),
        })
    }
    
    fn metadata(&self) -> ModelMetadata {
        self.metadata.clone()
    }
    
    fn update(&mut self, features: &Array1<f64>, target: f64) -> Result<()> {
        // Simple gradient descent update
        let prediction = features.dot(&self.weights) + self.bias;
        let error = prediction - target;
        let learning_rate = 0.001;
        
        // Update weights
        self.weights = &self.weights - learning_rate * error * features;
        self.bias -= learning_rate * error;
        
        self.metadata.training_samples += 1;
        self.metadata.last_updated = chrono::Utc::now();
        
        Ok(())
    }
}

/// Ensemble model combining multiple models
pub struct EnsembleModel {
    models: Vec<Box<dyn TradingModel>>,
    weights: Vec<f64>,
    metadata: ModelMetadata,
}

impl std::fmt::Debug for EnsembleModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnsembleModel")
            .field("models", &format!("[{} models]", self.models.len()))
            .field("weights", &self.weights)
            .field("metadata", &self.metadata)
            .finish()
    }
}

impl EnsembleModel {
    /// Create a new ensemble model from multiple models and their weights
    pub fn new(models: Vec<Box<dyn TradingModel>>, weights: Vec<f64>) -> Self {
        Self {
            metadata: ModelMetadata {
                name: "Ensemble".to_string(),
                version: "1.0.0".to_string(),
                model_type: ModelType::Ensemble,
                input_features: vec![],
                output_type: OutputType::MultiOutput,
                training_samples: 0,
                last_updated: chrono::Utc::now(),
            },
            models,
            weights,
        }
    }
}

impl TradingModel for EnsembleModel {
    fn predict(&self, features: &Array1<f64>) -> Result<ModelOutput> {
        let mut ensemble_predictions = HashMap::new();
        let mut total_confidence = 0.0;
        
        for (model, weight) in self.models.iter().zip(&self.weights) {
            let output = model.predict(features)?;
            
            for (key, value) in output.predictions {
                *ensemble_predictions.entry(key).or_insert(0.0) += value * weight;
            }
            
            total_confidence += output.confidence * weight;
        }
        
        Ok(ModelOutput {
            predictions: ensemble_predictions,
            confidence: total_confidence / self.weights.iter().sum::<f64>(),
            feature_importance: None,
        })
    }
    
    fn metadata(&self) -> ModelMetadata {
        self.metadata.clone()
    }
    
    fn update(&mut self, features: &Array1<f64>, target: f64) -> Result<()> {
        // Update all models in ensemble
        for model in &mut self.models {
            model.update(features, target)?;
        }
        
        self.metadata.training_samples += 1;
        self.metadata.last_updated = chrono::Utc::now();
        
        Ok(())
    }
}