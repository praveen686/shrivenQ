//! State Management for Trading Gateway

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tracing::info;

/// Gateway state for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayState {
    /// Active strategies
    pub active_strategies: Vec<String>,
    /// Circuit breaker state
    pub circuit_breaker_tripped: bool,
    /// Total orders processed
    pub total_orders: u64,
    /// Total volume
    pub total_volume: u64,
    /// Session start time
    pub session_start: i64,
}

impl GatewayState {
    /// Save state to file
    pub async fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json).await?;
        info!("Gateway state saved to {:?}", path);
        Ok(())
    }
    
    /// Load state from file
    pub async fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let json = fs::read_to_string(path).await?;
            let state = serde_json::from_str(&json)?;
            info!("Gateway state loaded from {:?}", path);
            Ok(state)
        } else {
            info!("No existing state found, creating new");
            Ok(Self {
                active_strategies: Vec::new(),
                circuit_breaker_tripped: false,
                total_orders: 0,
                total_volume: 0,
                session_start: chrono::Utc::now().timestamp(),
            })
        }
    }
}