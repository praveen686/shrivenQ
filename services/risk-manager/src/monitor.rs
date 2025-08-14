//! Risk monitoring and alerting

use serde::{Deserialize, Serialize};

/// Risk alert level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
    Emergency,
}

/// Risk alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAlert {
    pub level: AlertLevel,
    pub message: String,
    pub timestamp: i64,
    pub source: String,
}
