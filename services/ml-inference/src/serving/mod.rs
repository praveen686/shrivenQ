//! Model serving infrastructure

use anyhow::Result;
use dashmap::DashMap;
use std::sync::Arc;
use crate::models::{TradingModel, ModelMetadata};

/// Model registry for managing multiple models
pub struct ModelRegistry {
    models: Arc<DashMap<String, Arc<Box<dyn TradingModel>>>>,
    active_model: Arc<parking_lot::RwLock<String>>,
}

impl std::fmt::Debug for ModelRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let active = self.active_model.read();
        f.debug_struct("ModelRegistry")
            .field("models", &format!("[{} models]", self.models.len()))
            .field("active_model", &*active)
            .finish()
    }
}

impl ModelRegistry {
    /// Create a new empty model registry
    pub fn new() -> Self {
        Self {
            models: Arc::new(DashMap::new()),
            active_model: Arc::new(parking_lot::RwLock::new("default".to_string())),
        }
    }
    
    /// Register a model
    pub fn register_model(&self, name: String, model: Box<dyn TradingModel>) {
        self.models.insert(name.clone(), Arc::new(model));
        
        // Set as active if first model
        if self.models.len() == 1 {
            *self.active_model.write() = name;
        }
    }
    
    /// Get active model
    pub fn get_active_model(&self) -> Option<Arc<Box<dyn TradingModel>>> {
        let active = self.active_model.read();
        self.models.get(active.as_str()).map(|m| m.clone())
    }
    
    /// Set active model
    pub fn set_active_model(&self, name: &str) -> Result<()> {
        if self.models.contains_key(name) {
            *self.active_model.write() = name.to_string();
            Ok(())
        } else {
            anyhow::bail!("Model {} not found", name)
        }
    }
    
    /// List all models
    pub fn list_models(&self) -> Vec<(String, ModelMetadata)> {
        self.models.iter()
            .map(|entry| (entry.key().clone(), entry.value().metadata()))
            .collect()
    }
}