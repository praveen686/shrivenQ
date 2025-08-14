//! Zerodha configuration

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerodhaConfig {
    pub api_key: String,
    pub api_secret: String,
    pub user_id: String,
    pub password: String,
    pub totp_secret: String,
    pub cache_dir: Option<String>,
}
