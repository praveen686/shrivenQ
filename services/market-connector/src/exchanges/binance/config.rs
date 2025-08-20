//! Binance configuration

use serde::{Deserialize, Serialize};

/// Configuration settings for Binance exchange connection
///
/// This struct contains all the necessary configuration parameters for connecting
/// to Binance's APIs, including both the live trading environment and testnet.
/// The API credentials are optional since some operations (like public market data)
/// don't require authentication.
///
/// # Security Considerations
/// - API credentials should be stored securely and never logged
/// - Use environment variables or secure configuration management for credentials
/// - API secret should have minimal required permissions
///
/// # Examples
/// ```
/// use binance::config::BinanceConfig;
///
/// // Configuration for public data access (no authentication)
/// let public_config = BinanceConfig {
///     api_key: None,
///     api_secret: None,
///     testnet: false,
/// };
///
/// // Configuration for authenticated access
/// let auth_config = BinanceConfig {
///     api_key: Some("your_api_key".to_string()),
///     api_secret: Some("your_api_secret".to_string()),
///     testnet: true, // Use testnet for development
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinanceConfig {
    /// Binance API key for authenticated requests
    ///
    /// Required for private endpoints like account information, trading operations,
    /// and private user data streams. Can be None for public market data access.
    /// 
    /// # Security Note
    /// This should be kept confidential and never exposed in logs or client-side code.
    pub api_key: Option<String>,
    
    /// Binance API secret for request signing
    ///
    /// Used to cryptographically sign requests for authentication. Required in
    /// conjunction with api_key for private endpoints. Can be None for public data access.
    ///
    /// # Security Note
    /// This is highly sensitive and should be stored securely. It's used to generate
    /// HMAC signatures for request authentication.
    pub api_secret: Option<String>,
    
    /// Whether to use Binance testnet environment
    ///
    /// When true, connects to Binance's testnet environment which provides a safe
    /// testing environment with fake funds. When false, connects to the live production
    /// environment with real trading.
    ///
    /// # Testnet vs Production
    /// - Testnet: Safe for development and testing, uses different base URLs
    /// - Production: Live trading environment with real funds and market data
    ///
    /// # Default
    /// Should typically default to true for development and false for production deployments.
    pub testnet: bool,
}
