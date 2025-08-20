//! State Management for Trading Gateway

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tracing::info;

/// Gateway state for persistence and recovery
/// 
/// The `GatewayState` maintains critical runtime information that needs to
/// persist across gateway restarts. This includes active strategies, circuit
/// breaker status, trading statistics, and session metadata. The state can
/// be serialized to disk and restored during startup for continuity.
/// 
/// # Persistence Features
/// - JSON serialization for human-readable storage
/// - Automatic state restoration on startup
/// - Graceful handling of missing state files
/// - Session tracking with timestamps
/// 
/// # Use Cases
/// - Recovery after planned or unplanned restarts
/// - Audit trail for trading session statistics
/// - Circuit breaker state persistence
/// - Strategy configuration preservation
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
    /// Saves the current gateway state to a file
    /// 
    /// # Arguments
    /// * `path` - The file path where the state should be saved
    /// 
    /// # Returns
    /// * `Ok(())` - If the state was successfully saved
    /// * `Err(anyhow::Error)` - If file writing or serialization fails
    /// 
    /// # Behavior
    /// - Serializes the state to pretty-printed JSON format
    /// - Writes atomically to the specified file path
    /// - Logs successful save operations
    /// - Overwrites existing files at the target path
    /// 
    /// # File Format
    /// The state is saved as human-readable JSON with proper indentation
    /// for debugging and manual inspection if needed.
    pub async fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json).await?;
        info!("Gateway state saved to {:?}", path);
        Ok(())
    }
    
    /// Loads gateway state from a file or creates a new default state
    /// 
    /// # Arguments
    /// * `path` - The file path to load the state from
    /// 
    /// # Returns
    /// * `Ok(GatewayState)` - The loaded or newly created state
    /// * `Err(anyhow::Error)` - If file reading or deserialization fails
    /// 
    /// # Behavior
    /// - If the file exists: Deserializes and returns the saved state
    /// - If the file doesn't exist: Creates a new default state
    /// - Logs the operation result for monitoring
    /// 
    /// # Default State
    /// When no existing state file is found, creates a new state with:
    /// - Empty active strategies list
    /// - Circuit breaker not tripped
    /// - Zero order and volume counters
    /// - Current timestamp as session start time
    /// 
    /// This ensures the gateway can start cleanly even without prior state.
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