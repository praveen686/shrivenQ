//! Unit tests for token management and JWT lifecycle

use super::test_utils::*;
use auth_service::{AuthConfig, AuthService, AuthServiceImpl, Permission};
use chrono::{DateTime, Utc, Duration as ChronoDuration};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation, TokenData, Algorithm};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Extended auth context for testing token expiry
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestTokenClaims {
    pub user_id: String,
    pub permissions: Vec<Permission>,
    pub exp: i64, // Expiration timestamp
    pub iat: i64, // Issued at timestamp
    pub nbf: i64, // Not before timestamp
}

#[tokio::test]
async fn test_jwt_token_generation() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "test_secret_for_jwt_generation".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = AuthServiceImpl::new(config);
    let context = create_test_auth_context("jwt_user");
    
    // Generate token
    let token = auth_service.generate_token(&context).await.unwrap();
    assert!(!token.is_empty());
    
    // Token should be a valid JWT (3 parts separated by dots)
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3);
    
    // Decode and verify token contents
    let decoded_context = auth_service.validate_token(&token).await.unwrap();
    assert_eq!(decoded_context.user_id, context.user_id);
    assert_eq!(decoded_context.permissions, context.permissions);
    assert_eq!(decoded_context.api_keys, context.api_keys);
}

#[tokio::test]
async fn test_jwt_token_validation_with_different_secrets() {
    let secret1 = "first_secret_key";
    let secret2 = "second_secret_key";
    
    let context = create_test_auth_context("secret_test_user");
    
    // Generate token with first secret
    let token = generate_test_jwt(&context, secret1).unwrap();
    
    // Should validate with correct secret
    let validation_result = validate_test_jwt(&token, secret1);
    assert!(validation_result.is_ok());
    
    let decoded_context = validation_result.unwrap();
    assert_eq!(decoded_context.user_id, context.user_id);
    
    // Should fail with wrong secret
    let wrong_validation = validate_test_jwt(&token, secret2);
    assert!(wrong_validation.is_err());
}

#[tokio::test]
async fn test_jwt_token_expiry() {
    let secret = "expiry_test_secret";
    
    // Create claims with short expiry (1 second)
    let now = Utc::now().timestamp();
    let claims = TestTokenClaims {
        user_id: "expiry_user".to_string(),
        permissions: vec![Permission::ReadMarketData],
        exp: now + 1, // Expires in 1 second
        iat: now,
        nbf: now,
    };
    
    // Generate token
    let key = EncodingKey::from_secret(secret.as_bytes());
    let header = Header::default();
    let token = encode(&header, &claims, &key).unwrap();
    
    // Should be valid immediately
    let decode_key = DecodingKey::from_secret(secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    let immediate_result = decode::<TestTokenClaims>(&token, &decode_key, &validation);
    assert!(immediate_result.is_ok());
    
    // Wait for expiry
    tokio::time::sleep(Duration::from_millis(1100)).await;
    
    // Should be expired now
    let expired_result = decode::<TestTokenClaims>(&token, &decode_key, &validation);
    assert!(expired_result.is_err());
    
    let error = expired_result.err().unwrap();
    assert!(error.to_string().contains("Expired"));
}

#[tokio::test]
async fn test_jwt_token_not_before() {
    let secret = "nbf_test_secret";
    
    // Create claims with future not-before time
    let now = Utc::now().timestamp();
    let claims = TestTokenClaims {
        user_id: "nbf_user".to_string(),
        permissions: vec![Permission::ReadMarketData],
        exp: now + 3600, // Valid for 1 hour
        iat: now,
        nbf: now + 2, // Not valid until 2 seconds from now
    };
    
    // Generate token
    let key = EncodingKey::from_secret(secret.as_bytes());
    let header = Header::default();
    let token = encode(&header, &claims, &key).unwrap();
    
    // Should not be valid yet
    let decode_key = DecodingKey::from_secret(secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_nbf = true;
    validation.validate_exp = true;
    let premature_result = decode::<TestTokenClaims>(&token, &decode_key, &validation);
    assert!(premature_result.is_err());
    
    // Wait for not-before time
    tokio::time::sleep(Duration::from_millis(2100)).await;
    
    // Should be valid now
    let valid_result = decode::<TestTokenClaims>(&token, &decode_key, &validation);
    assert!(valid_result.is_ok());
}

#[tokio::test]
async fn test_token_cache_behavior() {
    use std::sync::Arc;
    use tokio::sync::RwLock;
    
    // Simulate a caching auth service
    struct CachingAuthService {
        inner: AuthServiceImpl,
        cache: Arc<RwLock<FxHashMap<String, String>>>,
    }
    
    impl CachingAuthService {
        fn new(config: AuthConfig) -> Self {
            Self {
                inner: AuthServiceImpl::new(config),
                cache: Arc::new(RwLock::new(FxHashMap::default())),
            }
        }
    }
    
    #[tonic::async_trait]
    impl AuthService for CachingAuthService {
        async fn authenticate(&self, username: &str, password: &str) -> anyhow::Result<auth_service::AuthContext> {
            self.inner.authenticate(username, password).await
        }
        
        async fn validate_token(&self, token: &str) -> anyhow::Result<auth_service::AuthContext> {
            // Check cache first
            if let Some(cached_user) = self.cache.read().await.get(token) {
                return Ok(create_test_auth_context(cached_user));
            }
            
            // Validate and cache
            let context = self.inner.validate_token(token).await?;
            self.cache.write().await.insert(token.to_string(), context.user_id.clone());
            Ok(context)
        }
        
        async fn generate_token(&self, context: &auth_service::AuthContext) -> anyhow::Result<String> {
            let token = self.inner.generate_token(context).await?;
            self.cache.write().await.insert(token.clone(), context.user_id.clone());
            Ok(token)
        }
        
        async fn check_permission(&self, context: &auth_service::AuthContext, permission: Permission) -> bool {
            self.inner.check_permission(context, permission).await
        }
        
        async fn revoke_token(&self, token: &str) -> anyhow::Result<()> {
            self.cache.write().await.remove(token);
            self.inner.revoke_token(token).await
        }
    }
    
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "cache_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = CachingAuthService::new(config);
    let context = create_test_auth_context("cache_user");
    
    // Generate token (should be cached)
    let token = auth_service.generate_token(&context).await.unwrap();
    
    // First validation (from cache)
    let start = std::time::Instant::now();
    let validation1 = auth_service.validate_token(&token).await.unwrap();
    let cache_time = start.elapsed();
    
    // Second validation (should also be from cache and fast)
    let start = std::time::Instant::now();
    let validation2 = auth_service.validate_token(&token).await.unwrap();
    let cache_time2 = start.elapsed();
    
    assert_eq!(validation1.user_id, validation2.user_id);
    assert!(cache_time2 <= cache_time + Duration::from_millis(10)); // Should be similar times
    
    // Revoke token (should clear cache)
    auth_service.revoke_token(&token).await.unwrap();
    
    // Validation after revocation should still work (new context created)
    let validation3 = auth_service.validate_token(&token).await.unwrap();
    assert_eq!(validation3.user_id, "cache_user");
}

#[tokio::test]
async fn test_token_refresh_mechanism() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "refresh_test_secret".to_string(),
        token_expiry: 1, // 1 second for quick expiry
        rate_limits,
    };
    
    let auth_service = AuthServiceImpl::new(config);
    let context = create_test_auth_context("refresh_user");
    
    // Generate initial token
    let token1 = auth_service.generate_token(&context).await.unwrap();
    
    // Validate initial token
    let validation1 = auth_service.validate_token(&token1).await;
    assert!(validation1.is_ok());
    
    // Wait for expiry
    tokio::time::sleep(Duration::from_millis(1100)).await;
    
    // Generate new token (refresh)
    let token2 = auth_service.generate_token(&context).await.unwrap();
    
    // New token should be different and valid
    assert_ne!(token1, token2);
    let validation2 = auth_service.validate_token(&token2).await;
    assert!(validation2.is_ok());
}

#[tokio::test]
async fn test_token_revocation_list() {
    use std::sync::Arc;
    use tokio::sync::RwLock;
    
    // Simulate revocation list
    struct RevocationAuthService {
        inner: AuthServiceImpl,
        revoked_tokens: Arc<RwLock<std::collections::HashSet<String>>>,
    }
    
    impl RevocationAuthService {
        fn new(config: AuthConfig) -> Self {
            Self {
                inner: AuthServiceImpl::new(config),
                revoked_tokens: Arc::new(RwLock::new(std::collections::HashSet::new())),
            }
        }
    }
    
    #[tonic::async_trait]
    impl AuthService for RevocationAuthService {
        async fn authenticate(&self, username: &str, password: &str) -> anyhow::Result<auth_service::AuthContext> {
            self.inner.authenticate(username, password).await
        }
        
        async fn validate_token(&self, token: &str) -> anyhow::Result<auth_service::AuthContext> {
            // Check revocation list first
            if self.revoked_tokens.read().await.contains(token) {
                return Err(anyhow::anyhow!("Token has been revoked"));
            }
            
            self.inner.validate_token(token).await
        }
        
        async fn generate_token(&self, context: &auth_service::AuthContext) -> anyhow::Result<String> {
            self.inner.generate_token(context).await
        }
        
        async fn check_permission(&self, context: &auth_service::AuthContext, permission: Permission) -> bool {
            self.inner.check_permission(context, permission).await
        }
        
        async fn revoke_token(&self, token: &str) -> anyhow::Result<()> {
            self.revoked_tokens.write().await.insert(token.to_string());
            Ok(())
        }
    }
    
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "revocation_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = RevocationAuthService::new(config);
    let context = create_test_auth_context("revocation_user");
    
    // Generate token
    let token = auth_service.generate_token(&context).await.unwrap();
    
    // Should be valid initially
    let validation1 = auth_service.validate_token(&token).await;
    assert!(validation1.is_ok());
    
    // Revoke token
    auth_service.revoke_token(&token).await.unwrap();
    
    // Should be invalid after revocation
    let validation2 = auth_service.validate_token(&token).await;
    assert!(validation2.is_err());
    assert!(validation2.err().unwrap().to_string().contains("revoked"));
}

#[tokio::test]
async fn test_concurrent_token_operations() {
    use std::sync::Arc;
    use tokio::task;
    
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 1000);
    
    let config = AuthConfig {
        jwt_secret: "concurrent_token_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = Arc::new(AuthServiceImpl::new(config));
    let mut handles = Vec::new();
    
    // Spawn multiple concurrent token operations
    for i in 0..50 {
        let service = Arc::clone(&auth_service);
        let handle = task::spawn(async move {
            let context = create_test_auth_context(&format!("concurrent_user_{}", i));
            
            // Generate token
            let token = service.generate_token(&context).await.unwrap();
            
            // Validate token
            let validated_context = service.validate_token(&token).await.unwrap();
            
            // Check permissions
            let can_read = service.check_permission(&validated_context, Permission::ReadMarketData).await;
            let can_trade = service.check_permission(&validated_context, Permission::PlaceOrders).await;
            
            // Revoke token
            service.revoke_token(&token).await.unwrap();
            
            (i, token, can_read, can_trade)
        });
        handles.push(handle);
    }
    
    // Wait for all operations to complete
    let results = futures::future::try_join_all(handles).await.unwrap();
    
    // Verify all operations succeeded
    let mut unique_tokens = std::collections::HashSet::new();
    for (i, token, can_read, can_trade) in results {
        assert!(!token.is_empty(), "Empty token for user {}", i);
        assert!(unique_tokens.insert(token.clone()), "Duplicate token: {}", token);
        assert!(can_read, "User {} should have read permission", i);
        assert!(can_trade, "User {} should have trade permission", i);
    }
    
    assert_eq!(unique_tokens.len(), 50);
}

#[tokio::test]
async fn test_token_metadata_preservation() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "metadata_preservation_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = AuthServiceImpl::new(config);
    let mut context = create_test_auth_context("metadata_user");
    
    // Add custom metadata
    context.metadata.insert("role".to_string(), "trader".to_string());
    context.metadata.insert("exchange_preference".to_string(), "binance".to_string());
    context.metadata.insert("risk_level".to_string(), "moderate".to_string());
    
    // Generate token
    let token = auth_service.generate_token(&context).await.unwrap();
    
    // Validate and check metadata preservation
    let validated_context = auth_service.validate_token(&token).await.unwrap();
    
    assert_eq!(validated_context.user_id, context.user_id);
    assert_eq!(validated_context.permissions, context.permissions);
    assert_eq!(validated_context.api_keys, context.api_keys);
    
    // Custom metadata should be preserved
    assert_eq!(validated_context.metadata.get("role"), Some(&"trader".to_string()));
    assert_eq!(validated_context.metadata.get("exchange_preference"), Some(&"binance".to_string()));
    assert_eq!(validated_context.metadata.get("risk_level"), Some(&"moderate".to_string()));
    
    // Login time should be preserved
    assert!(validated_context.metadata.contains_key("login_time"));
}

#[tokio::test]
async fn test_token_size_and_structure() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "structure_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = AuthServiceImpl::new(config);
    
    // Test with minimal context
    let minimal_context = create_limited_auth_context("minimal_user");
    let minimal_token = auth_service.generate_token(&minimal_context).await.unwrap();
    
    // Test with rich context
    let mut rich_context = create_test_auth_context("rich_user");
    for i in 0..10 {
        rich_context.metadata.insert(format!("key_{}", i), format!("value_{}", i));
        rich_context.api_keys.insert(format!("exchange_{}", i), format!("api_key_{}", i));
    }
    let rich_token = auth_service.generate_token(&rich_context).await.unwrap();
    
    // Verify token structure (JWT format)
    assert_eq!(minimal_token.split('.').count(), 3);
    assert_eq!(rich_token.split('.').count(), 3);
    
    // Rich token should be longer than minimal token
    assert!(rich_token.len() > minimal_token.len());
    
    // Both should be valid
    assert!(auth_service.validate_token(&minimal_token).await.is_ok());
    assert!(auth_service.validate_token(&rich_token).await.is_ok());
    
    // Verify rich context data is preserved
    let validated_rich = auth_service.validate_token(&rich_token).await.unwrap();
    assert_eq!(validated_rich.metadata.get("key_5"), Some(&"value_5".to_string()));
    assert_eq!(validated_rich.api_keys.get("exchange_3"), Some(&"api_key_3".to_string()));
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_token_generation_performance() {
        let mut rate_limits = FxHashMap::default();
        rate_limits.insert("default".to_string(), 10000);
        
        let config = AuthConfig {
            jwt_secret: "performance_test_secret".to_string(),
            token_expiry: 3600,
            rate_limits,
        };
        
        let auth_service = AuthServiceImpl::new(config);
        let context = create_test_auth_context("perf_user");
        
        // Benchmark token generation
        let start = Instant::now();
        let mut tokens = Vec::new();
        
        for _ in 0..1000 {
            let token = auth_service.generate_token(&context).await.unwrap();
            tokens.push(token);
        }
        
        let generation_time = start.elapsed();
        println!("1000 token generations: {:?}", generation_time);
        
        // Benchmark token validation
        let start = Instant::now();
        
        for token in &tokens {
            let _ = auth_service.validate_token(token).await.unwrap();
        }
        
        let validation_time = start.elapsed();
        println!("1000 token validations: {:?}", validation_time);
        
        // Performance assertions
        assert!(generation_time < Duration::from_secs(5));
        assert!(validation_time < Duration::from_secs(3));
        
        // Average times
        let avg_generation = generation_time.as_micros() / 1000;
        let avg_validation = validation_time.as_micros() / 1000;
        
        println!("Average token generation: {}μs", avg_generation);
        println!("Average token validation: {}μs", avg_validation);
        
        // Reasonable performance expectations
        assert!(avg_generation < 2000); // Less than 2ms per token
        assert!(avg_validation < 1000);  // Less than 1ms per validation
    }

    #[tokio::test]
    async fn test_concurrent_token_performance() {
        use std::sync::Arc;
        use tokio::task;

        let mut rate_limits = FxHashMap::default();
        rate_limits.insert("default".to_string(), 10000);
        
        let config = AuthConfig {
            jwt_secret: "concurrent_perf_secret".to_string(),
            token_expiry: 3600,
            rate_limits,
        };
        
        let auth_service = Arc::new(AuthServiceImpl::new(config));
        
        let start = Instant::now();
        let mut handles = Vec::new();
        
        // Launch 100 concurrent token operations
        for i in 0..100 {
            let service = Arc::clone(&auth_service);
            let handle = task::spawn(async move {
                let context = create_test_auth_context(&format!("perf_user_{}", i));
                
                // Generate and validate token
                let token = service.generate_token(&context).await.unwrap();
                let _ = service.validate_token(&token).await.unwrap();
                
                token.len() // Return token length for verification
            });
            handles.push(handle);
        }
        
        // Wait for all operations to complete
        let results = futures::future::try_join_all(handles).await.unwrap();
        let concurrent_time = start.elapsed();
        
        println!("100 concurrent token operations: {:?}", concurrent_time);
        
        // All operations should complete
        assert_eq!(results.len(), 100);
        
        // Should be faster than sequential operations
        assert!(concurrent_time < Duration::from_secs(2));
        
        // Verify all tokens were generated (non-zero length)
        for token_len in results {
            assert!(token_len > 0);
        }
    }
}