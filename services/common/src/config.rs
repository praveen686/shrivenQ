//! Service configuration

use serde::{Deserialize, Serialize};

/// Service endpoints configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEndpoints {
    pub auth_service: String,
    pub market_data_service: String,
    pub risk_service: String,
    pub execution_service: String,
    pub data_aggregator_service: String,
}

impl Default for ServiceEndpoints {
    fn default() -> Self {
        Self {
            auth_service: "http://localhost:50051".to_string(),
            market_data_service: "http://localhost:50052".to_string(),
            risk_service: "http://localhost:50053".to_string(),
            execution_service: "http://localhost:50054".to_string(),
            data_aggregator_service: "http://localhost:50055".to_string(),
        }
    }
}

/// Service discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDiscoveryConfig {
    pub enabled: bool,
    pub consul_endpoint: Option<String>,
    pub etcd_endpoint: Option<String>,
    pub refresh_interval_secs: u64,
}

impl Default for ServiceDiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            consul_endpoint: None,
            etcd_endpoint: None,
            refresh_interval_secs: 30,
        }
    }
}
