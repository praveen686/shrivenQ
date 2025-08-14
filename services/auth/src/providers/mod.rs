//! Exchange-specific authentication providers

pub mod binance;
pub mod binance_enhanced;
pub mod zerodha;

use anyhow::Result;

/// Common trait for exchange authentication
pub trait ExchangeAuth: Send + Sync {
    /// Get API key for the exchange
    fn get_api_key(&self) -> &str;

    /// Get API secret
    fn get_api_secret(&self) -> &str;

    /// Sign a request
    fn sign_request(&self, payload: &str) -> Result<String>;

    /// Validate credentials
    fn validate_credentials(&self) -> Result<bool>;
}
