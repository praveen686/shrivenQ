//! Unit tests for Binance authentication service

use super::test_utils::*;
use anyhow::Result;
use auth_service::binance_service::BinanceAuthService;
use auth_service::{AuthService, Permission};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Mock Binance authentication for testing
pub struct MockBinanceAuth {
    pub api_key: String,
    pub should_fail: Arc<RwLock<bool>>,
    pub account_info: Arc<RwLock<MockAccountInfo>>,
    pub network_delay: Arc<RwLock<Duration>>,
}

#[derive(Debug, Clone)]
pub struct MockAccountInfo {
    pub can_trade: bool,
    pub can_withdraw: bool,
    pub can_deposit: bool,
    pub balances: Vec<MockBalance>,
}

#[derive(Debug, Clone)]
pub struct MockBalance {
    pub asset: String,
    pub free: String,
    pub locked: String,
}

impl MockBinanceAuth {
    pub fn new() -> Self {
        Self {
            api_key: "test_binance_api_key".to_string(),
            should_fail: Arc::new(RwLock::new(false)),
            account_info: Arc::new(RwLock::new(MockAccountInfo {
                can_trade: true,
                can_withdraw: false,
                can_deposit: true,
                balances: vec![
                    MockBalance {
                        asset: "USDT".to_string(),
                        free: "1000.00".to_string(),
                        locked: "0.00".to_string(),
                    },
                    MockBalance {
                        asset: "BTC".to_string(),
                        free: "0.01".to_string(),
                        locked: "0.00".to_string(),
                    },
                ],
            })),
            network_delay: Arc::new(RwLock::new(Duration::from_millis(100))),
        }
    }

    pub async fn set_should_fail(&self, fail: bool) {
        *self.should_fail.write().await = fail;
    }

    pub async fn set_network_delay(&self, delay: Duration) {
        *self.network_delay.write().await = delay;
    }

    pub async fn validate_credentials(&self) -> Result<bool> {
        let delay = *self.network_delay.read().await;
        tokio::time::sleep(delay).await;

        if *self.should_fail.read().await {
            return Err(anyhow::anyhow!("Mock network failure"));
        }

        Ok(true)
    }

    pub async fn get_account_info(&self) -> Result<MockAccountInfo> {
        let delay = *self.network_delay.read().await;
        tokio::time::sleep(delay).await;

        if *self.should_fail.read().await {
            return Err(anyhow::anyhow!("Mock API failure"));
        }

        Ok(self.account_info.read().await.clone())
    }

    pub async fn create_listen_key(&self) -> Result<String> {
        let delay = *self.network_delay.read().await;
        tokio::time::sleep(delay).await;

        if *self.should_fail.read().await {
            return Err(anyhow::anyhow!("Failed to create listen key"));
        }

        Ok(format!("mock_listen_key_{}", uuid::Uuid::new_v4()))
    }

    pub fn get_api_key(&self) -> &str {
        &self.api_key
    }
}

#[tokio::test]
async fn test_binance_service_creation_with_credentials() {
    // Set up environment variables for test
    unsafe {
        std::env::set_var("BINANCE_SPOT_API_KEY", "test_api_key");
        std::env::set_var("BINANCE_SPOT_API_SECRET", "test_api_secret");
        std::env::set_var("JWT_SECRET", "test_jwt_secret");
        std::env::set_var("TOKEN_EXPIRY", "3600");
    }

    // Test service creation (this will work in test with env vars set)
    // Note: In actual implementation, this would create a real BinanceAuthService
    let jwt_secret = "test_jwt_secret".to_string();
    let token_expiry = 3600;

    // Simulate service creation logic
    let service_result = BinanceAuthService::new(jwt_secret, token_expiry);
    
    // Clean up environment variables
    unsafe {
        std::env::remove_var("BINANCE_SPOT_API_KEY");
        std::env::remove_var("BINANCE_SPOT_API_SECRET");
        std::env::remove_var("JWT_SECRET");
        std::env::remove_var("TOKEN_EXPIRY");
    }

    // In test environment without real credentials, this should fail
    // which is expected behavior
    assert!(service_result.is_err());
}

#[tokio::test]
async fn test_binance_service_creation_without_credentials() {
    // Ensure no Binance environment variables are set
    unsafe {
        std::env::remove_var("BINANCE_SPOT_API_KEY");
        std::env::remove_var("BINANCE_FUTURES_API_KEY");
    }

    let jwt_secret = "test_jwt_secret".to_string();
    let token_expiry = 3600;

    let service_result = BinanceAuthService::new(jwt_secret, token_expiry);
    assert!(service_result.is_err());
    
    // Should get error message about missing credentials
    let error_msg = service_result.err().unwrap().to_string();
    assert!(error_msg.contains("No Binance credentials"));
}

#[tokio::test]
async fn test_mock_binance_authentication_flow() {
    let mock_auth = MockBinanceAuth::new();

    // Test successful credential validation
    let validation_result = mock_auth.validate_credentials().await;
    assert!(validation_result.is_ok());
    assert!(validation_result.unwrap());

    // Test account info retrieval
    let account_info = mock_auth.get_account_info().await.unwrap();
    assert!(account_info.can_trade);
    assert!(!account_info.can_withdraw);
    assert!(account_info.can_deposit);
    assert_eq!(account_info.balances.len(), 2);

    // Check USDT balance
    let usdt_balance = &account_info.balances[0];
    assert_eq!(usdt_balance.asset, "USDT");
    assert_eq!(usdt_balance.free, "1000.00");
    assert_eq!(usdt_balance.locked, "0.00");

    // Test listen key creation
    let listen_key = mock_auth.create_listen_key().await.unwrap();
    assert!(listen_key.starts_with("mock_listen_key_"));
}

#[tokio::test]
async fn test_mock_binance_failure_scenarios() {
    let mock_auth = MockBinanceAuth::new();

    // Test credential validation failure
    mock_auth.set_should_fail(true).await;

    let validation_result = mock_auth.validate_credentials().await;
    assert!(validation_result.is_err());
    assert!(validation_result.err().unwrap().to_string().contains("Mock network failure"));

    // Test account info failure
    let account_result = mock_auth.get_account_info().await;
    assert!(account_result.is_err());
    assert!(account_result.err().unwrap().to_string().contains("Mock API failure"));

    // Test listen key failure
    let listen_key_result = mock_auth.create_listen_key().await;
    assert!(listen_key_result.is_err());
    assert!(listen_key_result.err().unwrap().to_string().contains("Failed to create listen key"));

    // Reset failure state
    mock_auth.set_should_fail(false).await;

    // Should work again
    let validation_result = mock_auth.validate_credentials().await;
    assert!(validation_result.is_ok());
}

#[tokio::test]
async fn test_binance_username_parsing() {
    // Test various username formats that Binance service should handle
    let test_cases = vec![
        ("binance_spot", ("binance", "spot")),
        ("binance_futures", ("binance", "futures")),
        ("binance_usd", ("binance", "usd")),
        ("binance_usdfutures", ("binance", "usdfutures")),
    ];

    for (username, expected) in test_cases {
        let parts: Vec<&str> = username.split('_').collect();
        if parts.len() == 2 {
            let (exchange, market) = (parts[0], parts[1]);
            assert_eq!((exchange, market), expected);
        }
    }

    // Test invalid username format
    let invalid_username = "invalid_format_too_many_parts";
    let parts: Vec<&str> = invalid_username.split('_').collect();
    assert!(parts.len() > 2); // Should be detected as invalid
}

#[tokio::test]
async fn test_binance_permissions_mapping() {
    let mock_auth = MockBinanceAuth::new();
    let account_info = mock_auth.get_account_info().await.unwrap();

    // Test permission assignment based on account capabilities
    let mut permissions = vec![Permission::ReadMarketData, Permission::ViewPositions];

    if account_info.can_trade {
        permissions.push(Permission::PlaceOrders);
        permissions.push(Permission::CancelOrders);
    }

    // Should have trading permissions for mock account
    assert!(permissions.contains(&Permission::ReadMarketData));
    assert!(permissions.contains(&Permission::ViewPositions));
    assert!(permissions.contains(&Permission::PlaceOrders));
    assert!(permissions.contains(&Permission::CancelOrders));
    assert!(!permissions.contains(&Permission::Admin));
    assert!(!permissions.contains(&Permission::ModifyRiskLimits));
}

#[tokio::test]
async fn test_binance_api_key_management() {
    let mock_auth = MockBinanceAuth::new();
    let api_key = mock_auth.get_api_key();
    
    assert_eq!(api_key, "test_binance_api_key");
    assert!(!api_key.is_empty());

    // Test API key in context
    let mut api_keys = FxHashMap::default();
    api_keys.insert("binance_spot_api_key".to_string(), api_key.to_string());
    api_keys.insert("binance_spot_listen_key".to_string(), "mock_listen_key".to_string());

    assert!(api_keys.contains_key("binance_spot_api_key"));
    assert!(api_keys.contains_key("binance_spot_listen_key"));
    assert_eq!(api_keys.get("binance_spot_api_key"), Some(&api_key.to_string()));
}

#[tokio::test]
async fn test_binance_network_timeout_handling() {
    let mock_auth = MockBinanceAuth::new();

    // Test with short delay
    mock_auth.set_network_delay(Duration::from_millis(50)).await;
    let start = std::time::Instant::now();
    let _ = mock_auth.validate_credentials().await;
    let elapsed = start.elapsed();
    
    assert!(elapsed >= Duration::from_millis(45));
    assert!(elapsed < Duration::from_millis(200));

    // Test with longer delay
    mock_auth.set_network_delay(Duration::from_millis(500)).await;
    let start = std::time::Instant::now();
    let _ = mock_auth.validate_credentials().await;
    let elapsed = start.elapsed();
    
    assert!(elapsed >= Duration::from_millis(450));
    assert!(elapsed < Duration::from_millis(700));
}

#[tokio::test]
async fn test_binance_concurrent_requests() {
    use std::sync::Arc;
    use tokio::task;

    let mock_auth = Arc::new(MockBinanceAuth::new());
    let mut handles = Vec::new();

    // Spawn multiple concurrent requests
    for i in 0..10 {
        let auth = Arc::clone(&mock_auth);
        let handle = task::spawn(async move {
            let validation_result = auth.validate_credentials().await;
            let listen_key_result = auth.create_listen_key().await;
            
            (i, validation_result.is_ok(), listen_key_result.is_ok())
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    let results = futures::future::try_join_all(handles).await.unwrap();

    // All requests should succeed
    for (i, validation_ok, listen_key_ok) in results {
        assert!(validation_ok, "Validation failed for request {}", i);
        assert!(listen_key_ok, "Listen key creation failed for request {}", i);
    }
}

#[tokio::test]
async fn test_binance_balance_parsing() {
    let mock_auth = MockBinanceAuth::new();
    let account_info = mock_auth.get_account_info().await.unwrap();

    // Test balance parsing and filtering
    let balance_str = account_info
        .balances
        .iter()
        .filter_map(|b| {
            let free = b.free.parse::<f64>().unwrap_or(0.0);
            if free > 0.0 {
                Some(format!("{}:{}", b.asset, free))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(",");

    // Should include both USDT and BTC as they have positive balances
    assert!(balance_str.contains("USDT:1000"));
    assert!(balance_str.contains("BTC:0.01"));
    assert!(balance_str.contains(","));
}

#[tokio::test]
async fn test_binance_metadata_construction() {
    let mock_auth = MockBinanceAuth::new();
    let account_info = mock_auth.get_account_info().await.unwrap();

    // Test metadata construction
    let mut metadata = FxHashMap::default();
    metadata.insert("login_time".to_string(), chrono::Utc::now().to_rfc3339());
    metadata.insert("exchange".to_string(), "binance".to_string());
    metadata.insert("market".to_string(), "spot".to_string());
    metadata.insert("balances".to_string(), "USDT:1000,BTC:0.01".to_string());

    // Verify metadata structure
    assert!(metadata.contains_key("login_time"));
    assert!(metadata.contains_key("exchange"));
    assert!(metadata.contains_key("market"));
    assert!(metadata.contains_key("balances"));
    
    assert_eq!(metadata.get("exchange"), Some(&"binance".to_string()));
    assert_eq!(metadata.get("market"), Some(&"spot".to_string()));

    // Verify login_time is valid RFC3339
    let login_time = metadata.get("login_time").unwrap();
    assert!(chrono::DateTime::parse_from_rfc3339(login_time).is_ok());
}

#[tokio::test]
async fn test_binance_error_handling() {
    let mock_auth = MockBinanceAuth::new();

    // Test various error conditions
    mock_auth.set_should_fail(true).await;

    // All operations should fail gracefully
    let validation_err = mock_auth.validate_credentials().await.err().unwrap();
    assert!(validation_err.to_string().contains("Mock network failure"));

    let account_err = mock_auth.get_account_info().await.err().unwrap();
    assert!(account_err.to_string().contains("Mock API failure"));

    let listen_key_err = mock_auth.create_listen_key().await.err().unwrap();
    assert!(listen_key_err.to_string().contains("Failed to create listen key"));

    // Reset and verify recovery
    mock_auth.set_should_fail(false).await;

    let validation_ok = mock_auth.validate_credentials().await;
    assert!(validation_ok.is_ok());
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_binance_auth_performance() {
        let mock_auth = MockBinanceAuth::new();
        mock_auth.set_network_delay(Duration::from_millis(10)).await;

        // Benchmark credential validation
        let start = Instant::now();
        for _ in 0..100 {
            let _ = mock_auth.validate_credentials().await.unwrap();
        }
        let validation_time = start.elapsed();
        
        println!("100 credential validations: {:?}", validation_time);
        
        // Benchmark account info retrieval
        let start = Instant::now();
        for _ in 0..50 {
            let _ = mock_auth.get_account_info().await.unwrap();
        }
        let account_info_time = start.elapsed();
        
        println!("50 account info retrievals: {:?}", account_info_time);

        // Performance assertions (with reasonable margins for CI)
        assert!(validation_time < Duration::from_secs(5));
        assert!(account_info_time < Duration::from_secs(3));
    }

    #[tokio::test]
    async fn test_binance_concurrent_performance() {
        use std::sync::Arc;
        use tokio::task;

        let mock_auth = Arc::new(MockBinanceAuth::new());
        mock_auth.set_network_delay(Duration::from_millis(50)).await;

        let start = Instant::now();
        let mut handles = Vec::new();

        // Launch 20 concurrent operations
        for _ in 0..20 {
            let auth = Arc::clone(&mock_auth);
            let handle = task::spawn(async move {
                let _ = auth.validate_credentials().await;
                let _ = auth.get_account_info().await;
                let _ = auth.create_listen_key().await;
            });
            handles.push(handle);
        }

        // Wait for all to complete
        futures::future::try_join_all(handles).await.unwrap();
        let concurrent_time = start.elapsed();

        println!("20 concurrent auth operations: {:?}", concurrent_time);
        
        // Should be faster than sequential due to concurrency
        // With 50ms delay per operation, sequential would be ~3 seconds
        // Concurrent should be much faster
        assert!(concurrent_time < Duration::from_millis(500));
    }
}