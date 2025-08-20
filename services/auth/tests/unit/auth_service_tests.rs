//! Unit tests for the core AuthService trait implementations

use super::test_utils::*;
use auth_service::{AuthConfig, AuthService, AuthServiceImpl, Permission};
use rustc_hash::FxHashMap;

#[tokio::test]
async fn test_auth_service_impl_authenticate() {
    // Create config
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = AuthServiceImpl::new(config);
    
    // Test authentication with any username
    let result = auth_service.authenticate("test_user", "password").await;
    assert!(result.is_ok());
    
    let context = result.unwrap();
    assert_eq!(context.user_id, "test_user");
    assert!(context.permissions.contains(&Permission::ReadMarketData));
    assert!(context.permissions.contains(&Permission::PlaceOrders));
    assert!(context.api_keys.contains_key("demo"));
    assert!(context.metadata.contains_key("login_time"));
}

#[tokio::test]
async fn test_auth_service_impl_token_lifecycle() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "test_secret_key_for_jwt".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = AuthServiceImpl::new(config);
    
    // Authenticate user
    let context = auth_service.authenticate("token_test_user", "password").await.unwrap();
    
    // Generate token
    let token = auth_service.generate_token(&context).await.unwrap();
    assert!(!token.is_empty());
    
    // Validate token
    let validated_context = auth_service.validate_token(&token).await.unwrap();
    assert_eq!(validated_context.user_id, context.user_id);
    assert_eq!(validated_context.permissions, context.permissions);
    
    // Test permission checking
    assert!(auth_service.check_permission(&validated_context, Permission::ReadMarketData).await);
    assert!(auth_service.check_permission(&validated_context, Permission::PlaceOrders).await);
    assert!(!auth_service.check_permission(&validated_context, Permission::Admin).await);
    
    // Revoke token
    let revoke_result = auth_service.revoke_token(&token).await;
    assert!(revoke_result.is_ok());
}

#[tokio::test]
async fn test_auth_service_impl_invalid_token() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = AuthServiceImpl::new(config);
    
    // Test with invalid token
    let result = auth_service.validate_token("invalid_token").await;
    assert!(result.is_err());
    
    // Test with empty token
    let result = auth_service.validate_token("").await;
    assert!(result.is_err());
    
    // Test with malformed JWT
    let result = auth_service.validate_token("malformed.jwt.token").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_auth_service_impl_different_secrets() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    // Service with one secret
    let config1 = AuthConfig {
        jwt_secret: "secret_one".to_string(),
        token_expiry: 3600,
        rate_limits: rate_limits.clone(),
    };
    let auth_service1 = AuthServiceImpl::new(config1);
    
    // Service with different secret
    let config2 = AuthConfig {
        jwt_secret: "secret_two".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    let auth_service2 = AuthServiceImpl::new(config2);
    
    // Generate token with first service
    let context = auth_service1.authenticate("cross_secret_user", "password").await.unwrap();
    let token = auth_service1.generate_token(&context).await.unwrap();
    
    // Should validate with first service
    let result1 = auth_service1.validate_token(&token).await;
    assert!(result1.is_ok());
    
    // Should fail with second service (different secret)
    let result2 = auth_service2.validate_token(&token).await;
    assert!(result2.is_err());
}

#[tokio::test]
async fn test_auth_service_permission_hierarchy() {
    let context_admin = create_admin_auth_context("admin_user");
    let context_limited = create_limited_auth_context("limited_user");
    
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = AuthServiceImpl::new(config);
    
    // Admin should have all permissions
    assert!(auth_service.check_permission(&context_admin, Permission::ReadMarketData).await);
    assert!(auth_service.check_permission(&context_admin, Permission::PlaceOrders).await);
    assert!(auth_service.check_permission(&context_admin, Permission::CancelOrders).await);
    assert!(auth_service.check_permission(&context_admin, Permission::ViewPositions).await);
    assert!(auth_service.check_permission(&context_admin, Permission::ModifyRiskLimits).await);
    assert!(auth_service.check_permission(&context_admin, Permission::Admin).await);
    
    // Limited user should only have read permission
    assert!(auth_service.check_permission(&context_limited, Permission::ReadMarketData).await);
    assert!(!auth_service.check_permission(&context_limited, Permission::PlaceOrders).await);
    assert!(!auth_service.check_permission(&context_limited, Permission::CancelOrders).await);
    assert!(!auth_service.check_permission(&context_limited, Permission::ViewPositions).await);
    assert!(!auth_service.check_permission(&context_limited, Permission::ModifyRiskLimits).await);
    assert!(!auth_service.check_permission(&context_limited, Permission::Admin).await);
}

#[tokio::test]
async fn test_auth_service_concurrent_operations() {
    use std::sync::Arc;
    use tokio::task;
    
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "concurrent_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = Arc::new(AuthServiceImpl::new(config));
    let mut handles = Vec::new();
    
    // Spawn multiple concurrent authentication tasks
    for i in 0..10 {
        let service = Arc::clone(&auth_service);
        let handle = task::spawn(async move {
            let username = format!("concurrent_user_{}", i);
            let context = service.authenticate(&username, "password").await.unwrap();
            let token = service.generate_token(&context).await.unwrap();
            let validated_context = service.validate_token(&token).await.unwrap();
            
            assert_eq!(validated_context.user_id, username);
            token
        });
        handles.push(handle);
    }
    
    // Wait for all tasks to complete
    let tokens: Vec<String> = futures::future::try_join_all(handles).await.unwrap();
    
    // Verify all tokens are unique
    let mut unique_tokens = std::collections::HashSet::new();
    for token in &tokens {
        assert!(unique_tokens.insert(token.clone()), "Duplicate token found: {}", token);
    }
    
    assert_eq!(tokens.len(), 10);
}

#[tokio::test]
async fn test_auth_service_metadata_preservation() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "metadata_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = AuthServiceImpl::new(config);
    
    // Authenticate and check metadata
    let context = auth_service.authenticate("metadata_user", "password").await.unwrap();
    
    // Should have login_time metadata
    assert!(context.metadata.contains_key("login_time"));
    
    // Generate token and validate
    let token = auth_service.generate_token(&context).await.unwrap();
    let validated_context = auth_service.validate_token(&token).await.unwrap();
    
    // Metadata should be preserved in token
    assert_eq!(context.user_id, validated_context.user_id);
    assert_eq!(context.permissions, validated_context.permissions);
    assert_eq!(context.api_keys.get("demo"), validated_context.api_keys.get("demo"));
    
    // Login time should be preserved
    assert!(validated_context.metadata.contains_key("login_time"));
}

#[tokio::test]
async fn test_auth_service_edge_cases() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "edge_case_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = AuthServiceImpl::new(config);
    
    // Test empty username
    let result = auth_service.authenticate("", "password").await;
    assert!(result.is_ok()); // Demo service accepts any username
    
    // Test very long username
    let long_username = "a".repeat(1000);
    let result = auth_service.authenticate(&long_username, "password").await;
    assert!(result.is_ok());
    let context = result.unwrap();
    assert_eq!(context.user_id, long_username);
    
    // Test special characters in username
    let special_username = "test@user.com";
    let result = auth_service.authenticate(special_username, "password").await;
    assert!(result.is_ok());
    let context = result.unwrap();
    assert_eq!(context.user_id, special_username);
    
    // Test Unicode username
    let unicode_username = "测试用户";
    let result = auth_service.authenticate(unicode_username, "password").await;
    assert!(result.is_ok());
    let context = result.unwrap();
    assert_eq!(context.user_id, unicode_username);
}

#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn benchmark_auth_service_performance() {
        let mut rate_limits = FxHashMap::default();
        rate_limits.insert("default".to_string(), 1000);
        
        let config = AuthConfig {
            jwt_secret: "benchmark_secret".to_string(),
            token_expiry: 3600,
            rate_limits,
        };
        
        let auth_service = AuthServiceImpl::new(config);
        
        // Benchmark authentication
        let start = Instant::now();
        let mut tokens = Vec::new();
        
        for i in 0..100 {
            let username = format!("bench_user_{}", i);
            let context = auth_service.authenticate(&username, "password").await.unwrap();
            let token = auth_service.generate_token(&context).await.unwrap();
            tokens.push(token);
        }
        
        let auth_duration = start.elapsed();
        println!("Authentication + token generation for 100 users: {:?}", auth_duration);
        
        // Benchmark token validation
        let start = Instant::now();
        
        for token in &tokens {
            let _ = auth_service.validate_token(token).await.unwrap();
        }
        
        let validation_duration = start.elapsed();
        println!("Token validation for 100 tokens: {:?}", validation_duration);
        
        // Performance assertions
        assert!(auth_duration.as_millis() < 1000, "Auth too slow: {:?}", auth_duration);
        assert!(validation_duration.as_millis() < 500, "Validation too slow: {:?}", validation_duration);
    }
}