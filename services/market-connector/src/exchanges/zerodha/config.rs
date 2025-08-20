//! Zerodha configuration

use serde::{Deserialize, Serialize};

/// Configuration settings for Zerodha KiteConnect API access
///
/// This struct contains all the necessary authentication and configuration
/// parameters for connecting to Zerodha's KiteConnect API. All fields except
/// cache_dir are required for proper authentication.
///
/// # Security Considerations
/// - All authentication fields contain sensitive information and should be stored securely
/// - Never log or expose these credentials in client-side code
/// - Use environment variables or secure configuration management
/// - TOTP secret is particularly sensitive as it's used for 2FA bypassing
///
/// # Authentication Flow
/// Zerodha uses a multi-step authentication process:
/// 1. API key/secret for initial app identification
/// 2. User ID and password for user authentication
/// 3. TOTP secret for automatic 2FA handling
///
/// # Examples
/// ```
/// use zerodha::config::ZerodhaConfig;
///
/// let config = ZerodhaConfig {
///     api_key: "your_api_key".to_string(),
///     api_secret: "your_api_secret".to_string(),
///     user_id: "your_zerodha_client_id".to_string(),
///     password: "your_password".to_string(),
///     totp_secret: "your_totp_secret".to_string(),
///     cache_dir: Some("./cache".to_string()),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZerodhaConfig {
    /// Zerodha API key obtained from KiteConnect app registration
    ///
    /// This identifies your application to Zerodha's API. You can obtain this
    /// by creating a KiteConnect app on the Zerodha developer console.
    ///
    /// # Security Note
    /// While this is less sensitive than other credentials, it should still be
    /// kept confidential to prevent unauthorized use of your API quota.
    pub api_key: String,
    
    /// Zerodha API secret for request authentication
    ///
    /// Used in conjunction with the API key to authenticate your application.
    /// This is obtained when you create your KiteConnect app.
    ///
    /// # Security Note
    /// This is sensitive information and should be stored securely.
    pub api_secret: String,
    
    /// Zerodha client ID (user ID)
    ///
    /// Your Zerodha trading account client ID, typically in the format "AB1234".
    /// This identifies the specific trading account that will be accessed.
    ///
    /// # Format
    /// Usually a 6-character alphanumeric string starting with letters.
    pub user_id: String,
    
    /// Zerodha account password
    ///
    /// The password for your Zerodha trading account. This is used during
    /// the authentication process to verify account access.
    ///
    /// # Security Note
    /// This is highly sensitive information. Consider using encrypted storage
    /// or environment variables rather than hard-coding in configuration files.
    pub password: String,
    
    /// TOTP secret for automatic 2FA handling
    ///
    /// The TOTP (Time-based One-Time Password) secret used to automatically
    /// generate 2FA codes during authentication. This allows automated login
    /// without manual intervention for 2FA.
    ///
    /// # Obtaining TOTP Secret
    /// You can get this by setting up 2FA in your Zerodha account and extracting
    /// the secret from the QR code or setup process.
    ///
    /// # Security Note
    /// This is extremely sensitive as it bypasses 2FA protection. Store securely
    /// and limit access to this credential.
    pub totp_secret: String,
    
    /// Optional directory for caching authentication tokens
    ///
    /// When specified, authentication tokens will be cached in this directory
    /// to avoid repeated authentication calls. This improves performance and
    /// reduces API usage.
    ///
    /// # Usage
    /// - Set to Some("/path/to/cache") to enable caching
    /// - Set to None to disable caching (tokens will be requested fresh each time)
    ///
    /// # Security Note
    /// Cached tokens should be stored in a secure location with appropriate
    /// file permissions to prevent unauthorized access.
    pub cache_dir: Option<String>,
}
