//! API endpoints for authentication service

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Login request
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub two_fa_code: Option<String>,
}

/// Login response
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_at: u64,
    pub permissions: Vec<String>,
}

/// Token validation request
#[derive(Debug, Deserialize)]
pub struct ValidateTokenRequest {
    pub token: String,
}

/// Token validation response
#[derive(Debug, Serialize)]
pub struct ValidateTokenResponse {
    pub valid: bool,
    pub user_id: Option<String>,
    pub permissions: Vec<String>,
}

/// API handler trait
pub trait AuthApi: Send + Sync {
    /// Handle login request
    fn login(&self, request: LoginRequest) -> Result<LoginResponse>;

    /// Handle token validation
    fn validate(&self, request: ValidateTokenRequest) -> Result<ValidateTokenResponse>;

    /// Handle logout
    fn logout(&self, token: &str) -> Result<()>;

    /// Refresh token
    fn refresh(&self, token: &str) -> Result<String>;
}
