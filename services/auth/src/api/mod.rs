//! API endpoints for authentication service

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Login request
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// Username for authentication
    pub username: String,
    /// Password for authentication
    pub password: String,
    /// Optional two-factor authentication code
    pub two_fa_code: Option<String>,
}

/// Login response
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    /// JWT authentication token
    pub token: String,
    /// Token expiration timestamp (Unix timestamp)
    pub expires_at: u64,
    /// List of user permissions
    pub permissions: Vec<String>,
}

/// Token validation request
#[derive(Debug, Deserialize)]
pub struct ValidateTokenRequest {
    /// JWT token to validate
    pub token: String,
}

/// Token validation response
#[derive(Debug, Serialize)]
pub struct ValidateTokenResponse {
    /// Whether the token is valid
    pub valid: bool,
    /// User ID if token is valid
    pub user_id: Option<String>,
    /// List of user permissions if token is valid
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
