//! Unit tests for Zerodha authentication service

use super::test_utils::*;
use anyhow::Result;
use auth_service::providers::zerodha::{ZerodhaConfig, UserProfile, MarginData};
use auth_service::{AuthService, Permission};
use rustc_hash::FxHashMap;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Mock Zerodha authentication for testing
pub struct MockZerodhaAuth {
    pub config: ZerodhaConfig,
    pub should_fail: Arc<RwLock<bool>>,
    pub cached_token: Arc<RwLock<Option<String>>>,
    pub network_delay: Arc<RwLock<Duration>>,
    pub profile_data: Arc<RwLock<UserProfile>>,
    pub margins_data: Arc<RwLock<FxHashMap<String, MarginData>>>,
    pub totp_code: Arc<RwLock<String>>,
}

impl MockZerodhaAuth {
    pub fn new() -> Self {
        // Create test margins
        let mut margins = FxHashMap::default();
        
        let mut equity_available = FxHashMap::default();
        equity_available.insert("cash".to_string(), 50000.0);
        equity_available.insert("intraday_payin".to_string(), 0.0);
        
        let mut equity_used = FxHashMap::default();
        equity_used.insert("debits".to_string(), 0.0);
        equity_used.insert("exposure".to_string(), 0.0);
        
        let mut equity_total = FxHashMap::default();
        equity_total.insert("cash".to_string(), 50000.0);
        equity_total.insert("collateral".to_string(), 0.0);
        
        margins.insert("equity".to_string(), MarginData {
            available: equity_available,
            used: equity_used,
            total: equity_total,
        });

        Self {
            config: ZerodhaConfig::new(
                "TEST123".to_string(),
                "test_password".to_string(),
                "JBSWY3DPEHPK3PXP".to_string(), // Test TOTP secret
                "test_api_key".to_string(),
                "test_api_secret".to_string(),
            ),
            should_fail: Arc::new(RwLock::new(false)),
            cached_token: Arc::new(RwLock::new(None)),
            network_delay: Arc::new(RwLock::new(Duration::from_millis(100))),
            profile_data: Arc::new(RwLock::new(UserProfile {
                user_id: "TEST123".to_string(),
                email: "test@example.com".to_string(),
                exchanges: vec!["NSE".to_string(), "BSE".to_string()],
                products: vec!["CNC".to_string(), "MIS".to_string(), "NRML".to_string()],
            })),
            margins_data: Arc::new(RwLock::new(margins)),
            totp_code: Arc::new(RwLock::new("123456".to_string())),
        }
    }

    pub async fn set_should_fail(&self, fail: bool) {
        *self.should_fail.write().await = fail;
    }

    pub async fn set_network_delay(&self, delay: Duration) {
        *self.network_delay.write().await = delay;
    }

    pub async fn set_cached_token(&self, token: Option<String>) {
        *self.cached_token.write().await = token;
    }

    pub async fn set_totp_code(&self, code: String) {
        *self.totp_code.write().await = code;
    }

    pub async fn authenticate(&self) -> Result<String> {
        let delay = *self.network_delay.read().await;
        tokio::time::sleep(delay).await;

        if *self.should_fail.read().await {
            return Err(anyhow::anyhow!("Mock authentication failed"));
        }

        // Check for cached token first
        if let Some(token) = self.cached_token.read().await.as_ref() {
            return Ok(token.clone());
        }

        // Simulate authentication process
        let access_token = format!("mock_zerodha_token_{}", uuid::Uuid::new_v4());
        self.set_cached_token(Some(access_token.clone())).await;
        
        Ok(access_token)
    }

    pub async fn get_profile(&self) -> Result<UserProfile> {
        let delay = *self.network_delay.read().await;
        tokio::time::sleep(delay).await;

        if *self.should_fail.read().await {
            return Err(anyhow::anyhow!("Failed to fetch profile"));
        }

        Ok(self.profile_data.read().await.clone())
    }

    pub async fn get_margins(&self) -> Result<FxHashMap<String, MarginData>> {
        let delay = *self.network_delay.read().await;
        tokio::time::sleep(delay).await;

        if *self.should_fail.read().await {
            return Err(anyhow::anyhow!("Failed to fetch margins"));
        }

        Ok(self.margins_data.read().await.clone())
    }

    pub fn get_api_key(&self) -> String {
        self.config.api_key.clone()
    }

    pub async fn generate_totp(&self) -> Result<String> {
        Ok(self.totp_code.read().await.clone())
    }

    pub async fn invalidate_cache(&self) -> Result<()> {
        self.set_cached_token(None).await;
        Ok(())
    }

    pub async fn has_valid_token(&self) -> bool {
        self.cached_token.read().await.is_some()
    }

    pub async fn validate_token(&self, token: &str) -> bool {
        let delay = *self.network_delay.read().await;
        tokio::time::sleep(delay).await;

        if *self.should_fail.read().await {
            return false;
        }

        // Simple validation - check if token exists and is not empty
        !token.is_empty() && token.starts_with("mock_zerodha_token_")
    }
}

#[tokio::test]
async fn test_zerodha_config_creation() {
    let config = ZerodhaConfig::new(
        "TEST123".to_string(),
        "password".to_string(),
        "TOTP_SECRET".to_string(),
        "api_key".to_string(),
        "api_secret".to_string(),
    );

    assert_eq!(config.user_id, "TEST123");
    assert_eq!(config.password, "password");
    assert_eq!(config.totp_secret, "TOTP_SECRET");
    assert_eq!(config.api_key, "api_key");
    assert_eq!(config.api_secret, "api_secret");
    assert!(config.cache_dir.contains("cache/zerodha"));
    assert!(config.access_token.is_none());
}

#[tokio::test]
async fn test_zerodha_config_with_modifications() {
    let config = ZerodhaConfig::new(
        "TEST123".to_string(),
        "password".to_string(),
        "TOTP_SECRET".to_string(),
        "api_key".to_string(),
        "api_secret".to_string(),
    )
    .with_cache_dir("/custom/cache".to_string())
    .with_access_token("existing_token".to_string());

    assert_eq!(config.cache_dir, "/custom/cache");
    assert_eq!(config.access_token, Some("existing_token".to_string()));
}

#[tokio::test]
async fn test_mock_zerodha_authentication_flow() {
    let mock_auth = MockZerodhaAuth::new();

    // Test initial authentication
    let token = mock_auth.authenticate().await.unwrap();
    assert!(token.starts_with("mock_zerodha_token_"));
    assert!(mock_auth.has_valid_token().await);

    // Test cached token reuse
    let token2 = mock_auth.authenticate().await.unwrap();
    assert_eq!(token, token2); // Should return same cached token

    // Test cache invalidation
    mock_auth.invalidate_cache().await.unwrap();
    assert!(!mock_auth.has_valid_token().await);

    // Test fresh authentication after cache invalidation
    let token3 = mock_auth.authenticate().await.unwrap();
    assert_ne!(token, token3); // Should be different token
}

#[tokio::test]
async fn test_mock_zerodha_profile_retrieval() {
    let mock_auth = MockZerodhaAuth::new();

    let profile = mock_auth.get_profile().await.unwrap();
    assert_eq!(profile.user_id, "TEST123");
    assert_eq!(profile.email, "test@example.com");
    assert_eq!(profile.exchanges, vec!["NSE", "BSE"]);
    assert_eq!(profile.products, vec!["CNC", "MIS", "NRML"]);
}

#[tokio::test]
async fn test_mock_zerodha_margins_retrieval() {
    let mock_auth = MockZerodhaAuth::new();

    let margins = mock_auth.get_margins().await.unwrap();
    assert!(margins.contains_key("equity"));

    let equity_margins = margins.get("equity").unwrap();
    assert_eq!(equity_margins.available.get("cash"), Some(&50000.0));
    assert_eq!(equity_margins.used.get("debits"), Some(&0.0));
    assert_eq!(equity_margins.total.get("cash"), Some(&50000.0));
}

#[tokio::test]
async fn test_zerodha_totp_generation() {
    let mock_auth = MockZerodhaAuth::new();

    // Test default TOTP code
    let totp = mock_auth.generate_totp().await.unwrap();
    assert_eq!(totp, "123456");

    // Test custom TOTP code
    mock_auth.set_totp_code("654321".to_string()).await;
    let totp2 = mock_auth.generate_totp().await.unwrap();
    assert_eq!(totp2, "654321");
}

#[tokio::test]
async fn test_zerodha_token_validation() {
    let mock_auth = MockZerodhaAuth::new();

    // Test valid token
    let valid_token = "mock_zerodha_token_12345";
    assert!(mock_auth.validate_token(valid_token).await);

    // Test invalid token format
    let invalid_token = "invalid_token_format";
    assert!(!mock_auth.validate_token(invalid_token).await);

    // Test empty token
    assert!(!mock_auth.validate_token("").await);
}

#[tokio::test]
async fn test_zerodha_failure_scenarios() {
    let mock_auth = MockZerodhaAuth::new();

    // Set failure mode
    mock_auth.set_should_fail(true).await;

    // All operations should fail
    let auth_result = mock_auth.authenticate().await;
    assert!(auth_result.is_err());
    assert!(auth_result.err().unwrap().to_string().contains("Mock authentication failed"));

    let profile_result = mock_auth.get_profile().await;
    assert!(profile_result.is_err());
    assert!(profile_result.err().unwrap().to_string().contains("Failed to fetch profile"));

    let margins_result = mock_auth.get_margins().await;
    assert!(margins_result.is_err());
    assert!(margins_result.err().unwrap().to_string().contains("Failed to fetch margins"));

    // Token validation should also fail
    assert!(!mock_auth.validate_token("any_token").await);

    // Reset failure mode
    mock_auth.set_should_fail(false).await;

    // Operations should work again
    let auth_result = mock_auth.authenticate().await;
    assert!(auth_result.is_ok());
}

#[tokio::test]
async fn test_zerodha_network_timing() {
    let mock_auth = MockZerodhaAuth::new();

    // Test with short delay
    mock_auth.set_network_delay(Duration::from_millis(50)).await;
    let start = std::time::Instant::now();
    let _ = mock_auth.authenticate().await.unwrap();
    let elapsed = start.elapsed();
    
    assert!(elapsed >= Duration::from_millis(45));
    assert!(elapsed < Duration::from_millis(150));

    // Test with longer delay
    mock_auth.set_network_delay(Duration::from_millis(300)).await;
    let start = std::time::Instant::now();
    let _ = mock_auth.get_profile().await.unwrap();
    let elapsed = start.elapsed();
    
    assert!(elapsed >= Duration::from_millis(250));
    assert!(elapsed < Duration::from_millis(400));
}

#[tokio::test]
async fn test_zerodha_permissions_mapping() {
    let mock_auth = MockZerodhaAuth::new();
    let profile = mock_auth.get_profile().await.unwrap();
    let margins = mock_auth.get_margins().await.unwrap();

    // Test permission assignment logic
    let mut permissions = vec![Permission::ReadMarketData, Permission::ViewPositions];

    // Check if user has trading permissions based on products
    if profile.products.contains(&"MIS".to_string()) 
        || profile.products.contains(&"CNC".to_string()) 
    {
        permissions.push(Permission::PlaceOrders);
        permissions.push(Permission::CancelOrders);
    }

    // Should have trading permissions
    assert!(permissions.contains(&Permission::ReadMarketData));
    assert!(permissions.contains(&Permission::ViewPositions));
    assert!(permissions.contains(&Permission::PlaceOrders));
    assert!(permissions.contains(&Permission::CancelOrders));

    // Should not have admin permissions
    assert!(!permissions.contains(&Permission::Admin));
    assert!(!permissions.contains(&Permission::ModifyRiskLimits));

    // Check margin availability
    if let Some(equity) = margins.get("equity") {
        if let Some(available) = equity.available.get("cash") {
            assert_eq!(*available, 50000.0);
        }
    }
}

#[tokio::test]
async fn test_zerodha_api_key_management() {
    let mock_auth = MockZerodhaAuth::new();
    let api_key = mock_auth.get_api_key();
    
    assert_eq!(api_key, "test_api_key");
    assert!(!api_key.is_empty());

    // Test API key in auth context
    let mut api_keys = FxHashMap::default();
    api_keys.insert("zerodha".to_string(), "mock_access_token".to_string());
    api_keys.insert("zerodha_api_key".to_string(), api_key.clone());

    assert!(api_keys.contains_key("zerodha"));
    assert!(api_keys.contains_key("zerodha_api_key"));
    assert_eq!(api_keys.get("zerodha_api_key"), Some(&api_key));
}

#[tokio::test]
async fn test_zerodha_concurrent_operations() {
    use std::sync::Arc;
    use tokio::task;

    let mock_auth = Arc::new(MockZerodhaAuth::new());
    let mut handles = Vec::new();

    // Spawn multiple concurrent authentication attempts
    for i in 0..10 {
        let auth = Arc::clone(&mock_auth);
        let handle = task::spawn(async move {
            let token_result = auth.authenticate().await;
            let profile_result = auth.get_profile().await;
            let margins_result = auth.get_margins().await;
            
            (i, token_result.is_ok(), profile_result.is_ok(), margins_result.is_ok())
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    let results = futures::future::try_join_all(handles).await.unwrap();

    // All requests should succeed
    for (i, token_ok, profile_ok, margins_ok) in results {
        assert!(token_ok, "Token auth failed for request {}", i);
        assert!(profile_ok, "Profile fetch failed for request {}", i);
        assert!(margins_ok, "Margins fetch failed for request {}", i);
    }
}

#[tokio::test]
async fn test_zerodha_metadata_construction() {
    let mock_auth = MockZerodhaAuth::new();
    let profile = mock_auth.get_profile().await.unwrap();

    // Test metadata construction as it would be done in the service
    let mut metadata = FxHashMap::default();
    metadata.insert("login_time".to_string(), chrono::Utc::now().to_rfc3339());
    metadata.insert("email".to_string(), profile.email.clone());
    metadata.insert("exchanges".to_string(), profile.exchanges.join(","));
    metadata.insert("products".to_string(), profile.products.join(","));

    // Verify metadata structure
    assert!(metadata.contains_key("login_time"));
    assert!(metadata.contains_key("email"));
    assert!(metadata.contains_key("exchanges"));
    assert!(metadata.contains_key("products"));

    assert_eq!(metadata.get("email"), Some(&profile.email));
    assert_eq!(metadata.get("exchanges"), Some(&"NSE,BSE".to_string()));
    assert_eq!(metadata.get("products"), Some(&"CNC,MIS,NRML".to_string()));

    // Verify login_time is valid RFC3339
    let login_time = metadata.get("login_time").unwrap();
    assert!(chrono::DateTime::parse_from_rfc3339(login_time).is_ok());
}

#[tokio::test]
async fn test_zerodha_cache_behavior() {
    let mock_auth = MockZerodhaAuth::new();

    // Initial state - no cached token
    assert!(!mock_auth.has_valid_token().await);

    // First authentication - should create token
    let token1 = mock_auth.authenticate().await.unwrap();
    assert!(mock_auth.has_valid_token().await);

    // Second authentication - should return cached token
    let token2 = mock_auth.authenticate().await.unwrap();
    assert_eq!(token1, token2);

    // Manual cache setting
    mock_auth.set_cached_token(Some("manual_token".to_string())).await;
    let token3 = mock_auth.authenticate().await.unwrap();
    assert_eq!(token3, "manual_token");

    // Cache invalidation
    mock_auth.invalidate_cache().await.unwrap();
    assert!(!mock_auth.has_valid_token().await);

    // Should create new token after invalidation
    let token4 = mock_auth.authenticate().await.unwrap();
    assert_ne!(token4, token3);
    assert_ne!(token4, "manual_token");
}

#[tokio::test]
async fn test_zerodha_error_recovery() {
    let mock_auth = MockZerodhaAuth::new();

    // Start with working state
    let token1 = mock_auth.authenticate().await.unwrap();
    assert!(token1.starts_with("mock_zerodha_token_"));

    // Introduce failure
    mock_auth.set_should_fail(true).await;

    // Operations should fail
    let auth_result = mock_auth.authenticate().await;
    assert!(auth_result.is_err());

    // Remove failure condition
    mock_auth.set_should_fail(false).await;

    // Should recover and work again
    let token2 = mock_auth.authenticate().await.unwrap();
    assert!(token2.starts_with("mock_zerodha_token_"));
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_zerodha_auth_performance() {
        let mock_auth = MockZerodhaAuth::new();
        mock_auth.set_network_delay(Duration::from_millis(10)).await;

        // Benchmark authentication (with caching)
        let start = Instant::now();
        let token1 = mock_auth.authenticate().await.unwrap();
        let first_auth_time = start.elapsed();

        // Benchmark cached authentication (should be much faster)
        let start = Instant::now();
        for _ in 0..100 {
            let _ = mock_auth.authenticate().await.unwrap();
        }
        let cached_auth_time = start.elapsed();

        println!("First auth: {:?}", first_auth_time);
        println!("100 cached auths: {:?}", cached_auth_time);

        // Cached operations should be much faster
        assert!(cached_auth_time < Duration::from_millis(100));
        assert!(first_auth_time > Duration::from_millis(5));

        // Benchmark profile and margins
        mock_auth.invalidate_cache().await.unwrap(); // Reset cache
        
        let start = Instant::now();
        for _ in 0..50 {
            let _ = mock_auth.get_profile().await.unwrap();
            let _ = mock_auth.get_margins().await.unwrap();
        }
        let data_fetch_time = start.elapsed();

        println!("50 profile + margins fetches: {:?}", data_fetch_time);
        assert!(data_fetch_time < Duration::from_secs(3));
    }

    #[tokio::test]
    async fn test_zerodha_concurrent_performance() {
        use std::sync::Arc;
        use tokio::task;

        let mock_auth = Arc::new(MockZerodhaAuth::new());
        mock_auth.set_network_delay(Duration::from_millis(50)).await;

        let start = Instant::now();
        let mut handles = Vec::new();

        // Launch 15 concurrent operations
        for _ in 0..15 {
            let auth = Arc::clone(&mock_auth);
            let handle = task::spawn(async move {
                let _ = auth.authenticate().await;
                let _ = auth.get_profile().await;
                let _ = auth.get_margins().await;
            });
            handles.push(handle);
        }

        // Wait for all to complete
        futures::future::try_join_all(handles).await.unwrap();
        let concurrent_time = start.elapsed();

        println!("15 concurrent Zerodha operations: {:?}", concurrent_time);
        
        // With caching, concurrent operations should be efficient
        assert!(concurrent_time < Duration::from_millis(300));
    }
}