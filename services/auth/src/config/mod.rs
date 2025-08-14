//! Configuration management for authentication service

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// Service configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceConfig {
    /// Service port
    pub port: u16,
    /// Database URL
    pub database_url: String,
    /// Redis URL for session management
    pub redis_url: String,
    /// JWT configuration
    pub jwt: JwtConfig,
    /// Exchange configurations
    pub exchanges: FxHashMap<String, ExchangeConfig>,
}

/// JWT configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JwtConfig {
    /// Secret key for signing
    pub secret: String,
    /// Token expiry in seconds
    pub expiry_seconds: u64,
    /// Issuer
    pub issuer: String,
}

/// Exchange-specific configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExchangeConfig {
    /// API endpoint
    pub api_url: String,
    /// WebSocket endpoint
    pub ws_url: Option<String>,
    /// Rate limit (requests per second)
    pub rate_limit: u32,
    /// Enabled features
    pub features: Vec<String>,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            port: 50051,
            database_url: "postgresql://localhost/auth".to_string(),
            redis_url: "redis://localhost:6379".to_string(),
            jwt: JwtConfig {
                secret: "change-me-in-production".to_string(),
                expiry_seconds: 3600,
                issuer: "shrivenquant-auth".to_string(),
            },
            exchanges: FxHashMap::default(),
        }
    }
}
