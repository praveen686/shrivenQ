//! Complete Zerodha authentication implementation with automatic token generation
//! Based on proven working implementation

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use reqwest::{Client, cookie::Jar};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use totp_rs::{Algorithm, Secret, TOTP};
use tracing::{debug, error, info, warn};

/// Zerodha configuration
#[derive(Debug, Clone)]
pub struct ZerodhaConfig {
    /// User ID (trading code)
    pub user_id: String,
    /// Login password
    pub password: String,
    /// TOTP secret for 2FA
    pub totp_secret: String,
    /// API key from Kite Connect app
    pub api_key: String,
    /// API secret from Kite Connect app
    pub api_secret: String,
    /// Cache directory for storing tokens
    pub cache_dir: String,
    /// Optional pre-existing access token
    pub access_token: Option<String>,
}

impl ZerodhaConfig {
    /// Create new configuration
    #[must_use]
    pub fn new(
        user_id: String,
        password: String,
        totp_secret: String,
        api_key: String,
        api_secret: String,
    ) -> Self {
        // Use project-relative cache directory
        let cache_dir =
            std::env::var("SHRIVEN_CACHE_DIR").unwrap_or_else(|_| "./cache/zerodha".to_string());

        Self {
            user_id,
            password,
            totp_secret,
            api_key,
            api_secret,
            cache_dir,
            access_token: None,
        }
    }

    /// Set cache directory
    #[must_use]
    pub fn with_cache_dir(mut self, dir: String) -> Self {
        self.cache_dir = dir;
        self
    }

    /// Set existing access token
    #[must_use]
    pub fn with_access_token(mut self, token: String) -> Self {
        self.access_token = Some(token);
        self
    }
}

/// Session cache for storing authentication tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCache {
    /// The access token for API authentication
    pub access_token: String,
    /// Timestamp when the token was generated
    pub generated_at: DateTime<Utc>,
    /// Timestamp when the token expires
    pub expires_at: DateTime<Utc>,
    /// The user ID associated with this session
    pub user_id: String,
    /// Partial API key for identification
    pub api_key_partial: String,
}

impl SessionCache {
    /// Check if cache is still valid (not expired)
    #[must_use]
    pub fn is_valid(&self) -> bool {
        Utc::now() < self.expires_at
    }

    /// Get age of cache in hours
    #[must_use]
    pub fn age_hours(&self) -> f64 {
        let age = Utc::now().signed_duration_since(self.generated_at);
        let minutes = i32::try_from(age.num_minutes()).unwrap_or(i32::MAX);
        f64::from(minutes) / 60.0
    }

    /// Get time until expiration in hours
    #[must_use]
    pub fn expires_in_hours(&self) -> f64 {
        let remaining = self.expires_at.signed_duration_since(Utc::now());
        let minutes = i32::try_from(remaining.num_minutes()).unwrap_or(i32::MAX);
        f64::from(minutes) / 60.0
    }
}

/// Zerodha authentication handler
pub struct ZerodhaAuth {
    config: ZerodhaConfig,
    http_client: Client,
    access_token: Arc<RwLock<Option<String>>>,
}

impl ZerodhaAuth {
    /// Create new authentication handler
    ///
    /// # Panics
    /// Panics if HTTP client cannot be created
    #[must_use]
    pub fn new(config: ZerodhaConfig) -> Self {
        // Create HTTP client with cookie jar for session management
        let jar = Arc::new(Jar::default());
        let http_client = Client::builder()
            .cookie_provider(jar)
            .timeout(Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            config,
            http_client,
            access_token: Arc::new(RwLock::new(None)),
        }
    }

    /// Authenticate and get access token
    ///
    /// # Errors
    /// Returns an error if authentication fails, network issues occur, or tokens are invalid
    pub async fn authenticate(&self) -> Result<String> {
        // Priority 1: Check for cached session first (most recent and likely valid)
        if let Some(cached_token) = self.load_cached_session() {
            info!("Found cached Zerodha session, validating...");
            if self.validate_access_token(&cached_token).await {
                info!("✓ Cached access token is valid");
                *self.access_token.write().await = Some(cached_token.clone());
                return Ok(cached_token);
            }
            warn!("Cached access token is invalid/expired, removing cache");
            let _ = self.remove_cached_session();
        }

        // Priority 2: Check if access token was provided in config
        if let Some(ref token) = self.config.access_token
            && !token.is_empty()
        {
            info!("Found access token in config, validating...");
            if self.validate_access_token(token).await {
                info!("✓ Config access token is valid");
                *self.access_token.write().await = Some(token.clone());
                self.save_session_cache(token)?;
                return Ok(token.clone());
            }
            warn!("Config access token is invalid/expired");
        }

        // Priority 3: Perform fresh authentication only if no valid token found
        info!("No valid access token found, performing fresh authentication");

        // Step 1: Get request token through automated login
        let request_token = self.get_request_token().await?;

        // Step 2: Generate access token
        let access_token = self.generate_access_token(&request_token).await?;

        // Step 3: Validate token
        if !self.validate_access_token(&access_token).await {
            return Err(anyhow!("Generated access token failed validation"));
        }

        // Step 4: Cache token
        self.save_session_cache(&access_token)?;

        // Store the token
        *self.access_token.write().await = Some(access_token.clone());

        info!("✅ Zerodha authentication completed successfully");
        Ok(access_token)
    }

    /// Get current access token
    pub async fn get_access_token(&self) -> Option<String> {
        self.access_token.read().await.clone()
    }

    /// Get API key
    #[must_use]
    pub fn get_api_key(&self) -> String {
        self.config.api_key.clone()
    }

    /// Load cached session if available and valid
    fn load_cached_session(&self) -> Option<String> {
        let cache_path = self.get_cache_file_path();

        if !Path::new(&cache_path).exists() {
            debug!("No session cache file found at: {}", cache_path);
            return None;
        }

        match fs::read_to_string(&cache_path) {
            Ok(content) => match serde_json::from_str::<SessionCache>(&content) {
                Ok(cache) => {
                    info!(
                        "Found cached session generated {:.1} hours ago",
                        cache.age_hours()
                    );

                    if cache.is_valid() {
                        info!(
                            "Cached session found (expires in {:.1} hours)",
                            cache.expires_in_hours()
                        );
                        return Some(cache.access_token);
                    }
                    info!("Cached session has expired");
                    let _ = self.remove_cached_session();
                }
                Err(e) => {
                    warn!("Failed to parse cached session: {}", e);
                    let _ = self.remove_cached_session();
                }
            },
            Err(e) => {
                warn!("Failed to read cached session: {}", e);
            }
        }

        None
    }

    /// Get cache file path
    fn get_cache_file_path(&self) -> String {
        format!(
            "{}/zerodha_token_{}.json",
            self.config.cache_dir, self.config.user_id
        )
    }

    /// Save session to cache
    ///
    /// # Errors
    /// Returns an error if cache directory creation or file writing fails
    fn save_session_cache(&self, access_token: &str) -> Result<()> {
        let cache = SessionCache {
            access_token: access_token.to_string(),
            generated_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(12), // Zerodha tokens valid for ~12 hours
            user_id: self.config.user_id.clone(),
            api_key_partial: if self.config.api_key.len() > 8 {
                format!("{}...", &self.config.api_key[..8])
            } else {
                "partial".to_string()
            },
        };

        // Ensure cache directory exists
        fs::create_dir_all(&self.config.cache_dir)?;

        let cache_path = self.get_cache_file_path();
        let content = serde_json::to_string_pretty(&cache)?;

        fs::write(&cache_path, content)?;

        info!("Session cached at: {}", cache_path);
        Ok(())
    }

    /// Remove cached session file
    ///
    /// # Errors
    /// Returns an error if file deletion fails
    fn remove_cached_session(&self) -> Result<()> {
        let cache_path = self.get_cache_file_path();
        if Path::new(&cache_path).exists() {
            fs::remove_file(&cache_path)?;
            info!("Removed expired cached session");
        }
        Ok(())
    }

    /// Generate TOTP code
    ///
    /// # Errors
    /// Returns an error if TOTP generation fails or secret is invalid
    fn generate_totp(&self) -> Result<String> {
        let secret = self
            .config
            .totp_secret
            .trim()
            .replace(' ', "")
            .to_uppercase();

        info!(
            "🔢 Generating TOTP from secret: {}...",
            &secret[..std::cmp::min(8, secret.len())]
        );

        // Create secret from base32 string
        let secret_bytes = Secret::Encoded(secret).to_bytes()?;

        let totp = TOTP::new(
            Algorithm::SHA1,
            6,  // 6 digits
            1,  // 1 step
            30, // 30 second period
            secret_bytes,
        )?;

        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| anyhow!("Failed to get system time: {e}"))?
            .as_secs();

        let totp_code = totp.generate(current_time);
        info!("🔐 Generated TOTP code: {}", totp_code);

        Ok(totp_code)
    }

    /// Perform automated login and get request token
    ///
    /// # Errors
    /// Returns an error if login fails, network issues occur, or request token is not found
    async fn get_request_token(&self) -> Result<String> {
        info!("Starting Zerodha login for user: {}", self.config.user_id);

        // Step 1: Get login page to establish session
        let login_page_response = self
            .http_client
            .get("https://kite.zerodha.com/")
            .send()
            .await?;

        // Ensure session is established
        if !login_page_response.status().is_success() {
            return Err(anyhow!(
                "Failed to establish session: HTTP {}",
                login_page_response.status()
            ));
        }

        // Step 2: Submit login credentials
        let login_params = [
            ("user_id", self.config.user_id.as_str()),
            ("password", self.config.password.as_str()),
        ];

        let login_response = self
            .http_client
            .post("https://kite.zerodha.com/api/login")
            .form(&login_params)
            .header("Referer", "https://kite.zerodha.com/")
            .header("Origin", "https://kite.zerodha.com")
            .send()
            .await?;

        let status = login_response.status();
        if !status.is_success() {
            let response_text = login_response.text().await.unwrap_or_default();
            return Err(anyhow!("Login failed: HTTP {} - {}", status, response_text));
        }

        let login_json: Value = login_response.json().await?;

        let request_id = login_json
            .get("data")
            .and_then(|d| d.get("request_id"))
            .and_then(|r| r.as_str())
            .ok_or_else(|| anyhow!("Request ID not found in login response"))?;

        debug!("Login successful, got request_id: {}", request_id);

        // Step 3: Submit TOTP
        let totp_code = self.generate_totp()?;

        let twofa_params = [
            ("user_id", self.config.user_id.as_str()),
            ("request_id", request_id),
            ("twofa_value", totp_code.as_str()),
        ];

        let twofa_response = self
            .http_client
            .post("https://kite.zerodha.com/api/twofa")
            .form(&twofa_params)
            .send()
            .await?;

        let status = twofa_response.status();
        if !status.is_success() {
            let response_text = twofa_response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "TOTP verification failed: HTTP {} - {}",
                status,
                response_text
            ));
        }

        debug!("✅ TOTP verification successful");

        // Small delay to ensure session is properly established
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Step 4: Follow redirect to get request_token
        debug!("🔗 Following redirect to extract request_token...");
        let redirect_url = format!(
            "https://kite.zerodha.com/connect/login?v=3&api_key={}",
            self.config.api_key
        );

        let redirect_response = self
            .http_client
            .get(&redirect_url)
            .header("Referer", "https://kite.zerodha.com/")
            .header("Origin", "https://kite.zerodha.com")
            .send()
            .await?;

        let final_url = redirect_response.url().to_string();
        debug!("Final redirect URL: {}", final_url);

        // Extract request_token from URL
        final_url.find("request_token=").map_or_else(
            || {
                error!("Request token not found in redirect URL: {}", final_url);
                Err(anyhow!(
                    "Request token not found in redirect URL: {}",
                    final_url
                ))
            },
            |start| {
                let token_start = start + "request_token=".len();
                let token_end = final_url[token_start..]
                    .find('&')
                    .map_or(final_url.len(), |i| token_start + i);

                let request_token = &final_url[token_start..token_end];
                info!("✅ Request token extracted: {}", request_token);
                Ok(request_token.to_string())
            },
        )
    }

    /// Generate access token from request token
    ///
    /// # Errors
    /// Returns an error if token exchange fails or network issues occur
    async fn generate_access_token(&self, request_token: &str) -> Result<String> {
        debug!("Generating access token from request token");

        let checksum_data = format!(
            "{}{}{}",
            self.config.api_key, request_token, self.config.api_secret
        );
        let mut hasher = Sha256::new();
        hasher.update(checksum_data.as_bytes());
        let checksum = format!("{:x}", hasher.finalize());

        let session_params = [
            ("api_key", self.config.api_key.as_str()),
            ("request_token", request_token),
            ("checksum", checksum.as_str()),
        ];

        let session_response = self
            .http_client
            .post("https://api.kite.trade/session/token")
            .form(&session_params)
            .send()
            .await?;

        let status = session_response.status();
        if !status.is_success() {
            let error_text = session_response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Session token generation failed: HTTP {} - {}",
                status,
                error_text
            ));
        }

        let session_json: Value = session_response.json().await?;

        let access_token = session_json
            .get("data")
            .and_then(|d| d.get("access_token"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| anyhow!("Access token not found in session response"))?;

        info!("Access token generated successfully");
        Ok(access_token.to_string())
    }

    /// Validate access token by making a test API call
    async fn validate_access_token(&self, access_token: &str) -> bool {
        let profile_url = "https://api.kite.trade/user/profile";

        let response = self
            .http_client
            .get(profile_url)
            .header("X-Kite-Version", "3")
            .header(
                "Authorization",
                format!("token {}:{}", self.config.api_key, access_token),
            )
            .send()
            .await;

        match response {
            Ok(resp) => {
                let is_valid = resp.status().is_success();
                if is_valid {
                    debug!("Access token validation successful");
                } else {
                    debug!(
                        "Access token validation failed with status: {}",
                        resp.status()
                    );
                }
                is_valid
            }
            Err(e) => {
                debug!("Access token validation request failed: {}", e);
                false
            }
        }
    }
}
