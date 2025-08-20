//! Unit tests for error handling and retry mechanisms

use super::test_utils::*;
use anyhow::{anyhow, Result};
use auth_service::{AuthConfig, AuthService, AuthServiceImpl, Permission};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Mock service that can simulate various failure modes
pub struct FailingAuthService {
    pub inner: AuthServiceImpl,
    pub failure_mode: Arc<RwLock<FailureMode>>,
    pub failure_count: Arc<RwLock<u32>>,
    pub max_failures: Arc<RwLock<u32>>,
}

#[derive(Debug, Clone)]
pub enum FailureMode {
    None,
    AuthenticationTimeout,
    DatabaseError,
    NetworkError,
    InvalidCredentials,
    TokenGenerationError,
    ValidationError,
    PermissionDenied,
    RateLimitExceeded,
    ServiceUnavailable,
    Intermittent, // Fails every other request
}

impl FailingAuthService {
    pub fn new(config: AuthConfig) -> Self {
        Self {
            inner: AuthServiceImpl::new(config),
            failure_mode: Arc::new(RwLock::new(FailureMode::None)),
            failure_count: Arc::new(RwLock::new(0)),
            max_failures: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn set_failure_mode(&self, mode: FailureMode, max_failures: u32) {
        *self.failure_mode.write().await = mode;
        *self.max_failures.write().await = max_failures;
        *self.failure_count.write().await = 0;
    }

    async fn should_fail(&self) -> Result<()> {
        let mut count = self.failure_count.write().await;
        let max_failures = *self.max_failures.read().await;
        let mode = self.failure_mode.read().await.clone();

        if *count < max_failures {
            *count += 1;
            
            match mode {
                FailureMode::None => Ok(()),
                FailureMode::AuthenticationTimeout => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Err(anyhow!("Authentication timeout after 30 seconds"))
                },
                FailureMode::DatabaseError => {
                    Err(anyhow!("Database connection failed: Connection refused"))
                },
                FailureMode::NetworkError => {
                    Err(anyhow!("Network error: DNS resolution failed"))
                },
                FailureMode::InvalidCredentials => {
                    Err(anyhow!("Invalid credentials provided"))
                },
                FailureMode::TokenGenerationError => {
                    Err(anyhow!("Failed to generate JWT token: Key error"))
                },
                FailureMode::ValidationError => {
                    Err(anyhow!("Token validation failed: Signature invalid"))
                },
                FailureMode::PermissionDenied => {
                    Err(anyhow!("Permission denied: Insufficient privileges"))
                },
                FailureMode::RateLimitExceeded => {
                    Err(anyhow!("Rate limit exceeded: Try again in 60 seconds"))
                },
                FailureMode::ServiceUnavailable => {
                    Err(anyhow!("Service temporarily unavailable"))
                },
                FailureMode::Intermittent => {
                    if *count % 2 == 1 {
                        Err(anyhow!("Intermittent failure: Network timeout"))
                    } else {
                        Ok(())
                    }
                },
            }
        } else {
            Ok(())
        }
    }
}

#[tonic::async_trait]
impl AuthService for FailingAuthService {
    async fn authenticate(&self, username: &str, password: &str) -> Result<auth_service::AuthContext> {
        self.should_fail().await?;
        self.inner.authenticate(username, password).await
    }

    async fn validate_token(&self, token: &str) -> Result<auth_service::AuthContext> {
        self.should_fail().await?;
        self.inner.validate_token(token).await
    }

    async fn generate_token(&self, context: &auth_service::AuthContext) -> Result<String> {
        self.should_fail().await?;
        self.inner.generate_token(context).await
    }

    async fn check_permission(&self, context: &auth_service::AuthContext, permission: Permission) -> bool {
        // Permission checks don't fail for simplicity
        self.inner.check_permission(context, permission).await
    }

    async fn revoke_token(&self, token: &str) -> Result<()> {
        self.should_fail().await?;
        self.inner.revoke_token(token).await
    }
}

/// Simple retry mechanism for testing
async fn retry_with_backoff<T, F, Fut>(
    mut operation: F,
    max_attempts: u32,
    base_delay: Duration,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut attempts = 0;
    let mut last_error = None;

    while attempts < max_attempts {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);
                attempts += 1;
                
                if attempts < max_attempts {
                    let delay = base_delay * 2_u32.pow(attempts - 1); // Exponential backoff
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("Max retry attempts reached")))
}

#[tokio::test]
async fn test_authentication_timeout_error() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "timeout_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let service = FailingAuthService::new(config);
    service.set_failure_mode(FailureMode::AuthenticationTimeout, 1).await;
    
    let start = Instant::now();
    let result = service.authenticate("test_user", "password").await;
    let duration = start.elapsed();
    
    assert!(result.is_err());
    assert!(duration >= Duration::from_millis(90)); // Should include the delay
    
    let error_msg = result.err().unwrap().to_string();
    assert!(error_msg.contains("Authentication timeout"));
    assert!(error_msg.contains("30 seconds"));
}

#[tokio::test]
async fn test_database_connection_error() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "db_error_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let service = FailingAuthService::new(config);
    service.set_failure_mode(FailureMode::DatabaseError, 1).await;
    
    let result = service.authenticate("test_user", "password").await;
    assert!(result.is_err());
    
    let error_msg = result.err().unwrap().to_string();
    assert!(error_msg.contains("Database connection failed"));
    assert!(error_msg.contains("Connection refused"));
}

#[tokio::test]
async fn test_network_error_handling() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "network_error_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let service = FailingAuthService::new(config);
    service.set_failure_mode(FailureMode::NetworkError, 1).await;
    
    let result = service.authenticate("test_user", "password").await;
    assert!(result.is_err());
    
    let error_msg = result.err().unwrap().to_string();
    assert!(error_msg.contains("Network error"));
    assert!(error_msg.contains("DNS resolution failed"));
}

#[tokio::test]
async fn test_token_generation_error() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "token_gen_error_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let service = FailingAuthService::new(config);
    let context = create_test_auth_context("token_gen_user");
    
    service.set_failure_mode(FailureMode::TokenGenerationError, 1).await;
    
    let result = service.generate_token(&context).await;
    assert!(result.is_err());
    
    let error_msg = result.err().unwrap().to_string();
    assert!(error_msg.contains("Failed to generate JWT token"));
    assert!(error_msg.contains("Key error"));
}

#[tokio::test]
async fn test_token_validation_error() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "validation_error_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let service = FailingAuthService::new(config);
    service.set_failure_mode(FailureMode::ValidationError, 1).await;
    
    let result = service.validate_token("any_token").await;
    assert!(result.is_err());
    
    let error_msg = result.err().unwrap().to_string();
    assert!(error_msg.contains("Token validation failed"));
    assert!(error_msg.contains("Signature invalid"));
}

#[tokio::test]
async fn test_rate_limit_exceeded_error() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "rate_limit_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let service = FailingAuthService::new(config);
    service.set_failure_mode(FailureMode::RateLimitExceeded, 1).await;
    
    let result = service.authenticate("test_user", "password").await;
    assert!(result.is_err());
    
    let error_msg = result.err().unwrap().to_string();
    assert!(error_msg.contains("Rate limit exceeded"));
    assert!(error_msg.contains("60 seconds"));
}

#[tokio::test]
async fn test_retry_mechanism_success_after_failures() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "retry_success_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let service = Arc::new(FailingAuthService::new(config));
    service.set_failure_mode(FailureMode::NetworkError, 2).await; // Fail twice, then succeed
    
    let service_clone = Arc::clone(&service);
    let start = Instant::now();
    
    let result = retry_with_backoff(
        || {
            let s = Arc::clone(&service_clone);
            async move { s.authenticate("retry_user", "password").await }
        },
        5, // Max 5 attempts
        Duration::from_millis(10), // Base delay
    ).await;
    
    let duration = start.elapsed();
    
    assert!(result.is_ok());
    let context = result.unwrap();
    assert_eq!(context.user_id, "retry_user");
    
    // Should have taken some time due to retries and backoff
    assert!(duration >= Duration::from_millis(20)); // At least 2 retry delays
}

#[tokio::test]
async fn test_retry_mechanism_max_attempts_reached() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "retry_failure_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let service = Arc::new(FailingAuthService::new(config));
    service.set_failure_mode(FailureMode::ServiceUnavailable, 10).await; // Fail 10 times
    
    let service_clone = Arc::clone(&service);
    let start = Instant::now();
    
    let result = retry_with_backoff(
        || {
            let s = Arc::clone(&service_clone);
            async move { s.authenticate("retry_fail_user", "password").await }
        },
        3, // Max 3 attempts
        Duration::from_millis(10), // Base delay
    ).await;
    
    let duration = start.elapsed();
    
    assert!(result.is_err());
    let error_msg = result.err().unwrap().to_string();
    assert!(error_msg.contains("Service temporarily unavailable"));
    
    // Should have tried multiple times with exponential backoff
    // 10ms + 20ms = 30ms minimum for 3 attempts
    assert!(duration >= Duration::from_millis(25));
}

#[tokio::test]
async fn test_intermittent_failure_pattern() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "intermittent_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let service = FailingAuthService::new(config);
    service.set_failure_mode(FailureMode::Intermittent, 10).await;
    
    let mut results = Vec::new();
    
    // Test 10 consecutive calls
    for _ in 0..10 {
        let result = service.authenticate("intermittent_user", "password").await;
        results.push(result.is_ok());
    }
    
    // Should have alternating success/failure pattern
    // Failures on odd attempts (1st, 3rd, 5th, etc.)
    // Successes on even attempts (2nd, 4th, 6th, etc.)
    assert!(!results[0]); // 1st call fails
    assert!(results[1]);  // 2nd call succeeds
    assert!(!results[2]); // 3rd call fails
    assert!(results[3]);  // 4th call succeeds
    assert!(!results[4]); // 5th call fails
}

#[tokio::test]
async fn test_error_propagation_in_token_lifecycle() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "error_propagation_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let service = FailingAuthService::new(config);
    
    // Test authentication error
    service.set_failure_mode(FailureMode::InvalidCredentials, 1).await;
    let auth_result = service.authenticate("error_user", "password").await;
    assert!(auth_result.is_err());
    assert!(auth_result.err().unwrap().to_string().contains("Invalid credentials"));
    
    // Reset for successful authentication
    service.set_failure_mode(FailureMode::None, 0).await;
    let context = service.authenticate("error_user", "password").await.unwrap();
    
    // Test token generation error
    service.set_failure_mode(FailureMode::TokenGenerationError, 1).await;
    let token_result = service.generate_token(&context).await;
    assert!(token_result.is_err());
    assert!(token_result.err().unwrap().to_string().contains("Failed to generate JWT token"));
    
    // Reset and generate valid token
    service.set_failure_mode(FailureMode::None, 0).await;
    let token = service.generate_token(&context).await.unwrap();
    
    // Test validation error
    service.set_failure_mode(FailureMode::ValidationError, 1).await;
    let validation_result = service.validate_token(&token).await;
    assert!(validation_result.is_err());
    assert!(validation_result.err().unwrap().to_string().contains("Token validation failed"));
    
    // Test revocation error
    service.set_failure_mode(FailureMode::ServiceUnavailable, 1).await;
    let revoke_result = service.revoke_token(&token).await;
    assert!(revoke_result.is_err());
    assert!(revoke_result.err().unwrap().to_string().contains("Service temporarily unavailable"));
}

#[tokio::test]
async fn test_concurrent_error_handling() {
    use std::sync::Arc;
    use tokio::task;
    
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 1000);
    
    let config = AuthConfig {
        jwt_secret: "concurrent_error_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let service = Arc::new(FailingAuthService::new(config));
    service.set_failure_mode(FailureMode::Intermittent, 50).await; // Intermittent failures
    
    let mut handles = Vec::new();
    
    // Spawn multiple concurrent operations
    for i in 0..20 {
        let s = Arc::clone(&service);
        let handle = task::spawn(async move {
            let username = format!("concurrent_error_user_{}", i);
            let result = s.authenticate(&username, "password").await;
            (i, result.is_ok(), result.is_err())
        });
        handles.push(handle);
    }
    
    // Wait for all operations
    let results = futures::future::try_join_all(handles).await.unwrap();
    
    // Some should succeed, some should fail due to intermittent pattern
    let successes = results.iter().filter(|(_, success, _)| *success).count();
    let failures = results.iter().filter(|(_, _, failure)| *failure).count();
    
    assert!(successes > 0, "At least some operations should succeed");
    assert!(failures > 0, "At least some operations should fail");
    assert_eq!(successes + failures, 20);
}

#[tokio::test]
async fn test_error_recovery_after_service_restart() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "recovery_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let service = FailingAuthService::new(config);
    
    // Simulate service being down
    service.set_failure_mode(FailureMode::ServiceUnavailable, 3).await;
    
    // First few requests should fail
    let result1 = service.authenticate("recovery_user", "password").await;
    let result2 = service.authenticate("recovery_user", "password").await;
    let result3 = service.authenticate("recovery_user", "password").await;
    
    assert!(result1.is_err());
    assert!(result2.is_err());
    assert!(result3.is_err());
    
    // Service "recovers" after 3 failures
    let result4 = service.authenticate("recovery_user", "password").await;
    assert!(result4.is_ok());
    
    let context = result4.unwrap();
    assert_eq!(context.user_id, "recovery_user");
    
    // Subsequent requests should continue to work
    let result5 = service.authenticate("recovery_user2", "password").await;
    assert!(result5.is_ok());
}

#[tokio::test]
async fn test_graceful_degradation() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 100);
    
    let config = AuthConfig {
        jwt_secret: "degradation_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let service = FailingAuthService::new(config);
    
    // Test that permission checking doesn't fail even during service issues
    service.set_failure_mode(FailureMode::ServiceUnavailable, 10).await;
    
    let context = create_test_auth_context("degradation_user");
    
    // Permission checks should still work
    assert!(service.check_permission(&context, Permission::ReadMarketData).await);
    assert!(service.check_permission(&context, Permission::PlaceOrders).await);
    assert!(!service.check_permission(&context, Permission::Admin).await);
    
    // But authentication should fail
    let auth_result = service.authenticate("degradation_user", "password").await;
    assert!(auth_result.is_err());
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_error_handling_performance_impact() {
        let mut rate_limits = FxHashMap::default();
        rate_limits.insert("default".to_string(), 1000);
        
        let config = AuthConfig {
            jwt_secret: "error_perf_test_secret".to_string(),
            token_expiry: 3600,
            rate_limits,
        };
        
        let failing_service = FailingAuthService::new(config.clone());
        let normal_service = AuthServiceImpl::new(config);
        
        // Benchmark normal operations
        let start = Instant::now();
        for i in 0..100 {
            let username = format!("normal_user_{}", i);
            let _ = normal_service.authenticate(&username, "password").await.unwrap();
        }
        let normal_time = start.elapsed();
        
        // Benchmark with intermittent failures
        failing_service.set_failure_mode(FailureMode::Intermittent, 200).await;
        
        let start = Instant::now();
        let mut success_count = 0;
        
        for i in 0..100 {
            let username = format!("failing_user_{}", i);
            if failing_service.authenticate(&username, "password").await.is_ok() {
                success_count += 1;
            }
        }
        let failing_time = start.elapsed();
        
        println!("Normal service: 100 auths in {:?}", normal_time);
        println!("Failing service: 100 attempts ({} successful) in {:?}", success_count, failing_time);
        
        // Failing service should be slower due to error handling
        // But not excessively so
        assert!(failing_time > normal_time);
        assert!(failing_time < normal_time * 3); // No more than 3x slower
        
        // Should have roughly 50% success rate due to intermittent pattern
        assert!(success_count >= 40 && success_count <= 60);
    }
}