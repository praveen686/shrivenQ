//! Unit tests for API rate limiting

use super::test_utils::*;
use anyhow::{anyhow, Result};
use auth_service::{AuthConfig, AuthService, AuthServiceImpl, Permission};
use rustc_hash::FxHashMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;

/// Rate limiter implementation for testing
#[derive(Debug)]
pub struct RateLimiter {
    /// Requests per user per time window
    limits: FxHashMap<String, u32>,
    /// Time window in seconds
    window_seconds: u64,
    /// Current request counts per user
    request_counts: Arc<RwLock<FxHashMap<String, (u32, Instant)>>>,
}

impl RateLimiter {
    pub fn new(limits: FxHashMap<String, u32>, window_seconds: u64) -> Self {
        Self {
            limits,
            window_seconds,
            request_counts: Arc::new(RwLock::new(FxHashMap::default())),
        }
    }

    pub async fn check_rate_limit(&self, user_id: &str) -> Result<()> {
        let user_limit = self.limits.get("default").copied().unwrap_or(100);
        let now = Instant::now();
        
        let mut counts = self.request_counts.write().await;
        
        // Clean up old entries
        counts.retain(|_, (_, timestamp)| {
            now.duration_since(*timestamp).as_secs() < self.window_seconds
        });
        
        // Check current user's count
        if let Some((count, first_request)) = counts.get_mut(user_id) {
            if now.duration_since(*first_request).as_secs() < self.window_seconds {
                if *count >= user_limit {
                    return Err(anyhow!("Rate limit exceeded: {} requests per {} seconds", 
                                     user_limit, self.window_seconds));
                }
                *count += 1;
            } else {
                // Reset window
                *count = 1;
                *first_request = now;
            }
        } else {
            // First request from this user
            counts.insert(user_id.to_string(), (1, now));
        }
        
        Ok(())
    }

    pub async fn get_current_count(&self, user_id: &str) -> u32 {
        let counts = self.request_counts.read().await;
        if let Some((count, timestamp)) = counts.get(user_id) {
            let now = Instant::now();
            if now.duration_since(*timestamp).as_secs() < self.window_seconds {
                *count
            } else {
                0 // Window expired
            }
        } else {
            0
        }
    }

    pub async fn reset_user_limit(&self, user_id: &str) {
        self.request_counts.write().await.remove(user_id);
    }
}

/// Auth service with rate limiting
pub struct RateLimitedAuthService {
    inner: AuthServiceImpl,
    rate_limiter: RateLimiter,
}

impl RateLimitedAuthService {
    pub fn new(config: AuthConfig) -> Self {
        let rate_limiter = RateLimiter::new(config.rate_limits.clone(), 60); // 1 minute window
        Self {
            inner: AuthServiceImpl::new(config),
            rate_limiter,
        }
    }

    pub async fn get_current_rate_count(&self, user_id: &str) -> u32 {
        self.rate_limiter.get_current_count(user_id).await
    }

    pub async fn reset_rate_limit(&self, user_id: &str) {
        self.rate_limiter.reset_user_limit(user_id).await;
    }
}

#[tonic::async_trait]
impl AuthService for RateLimitedAuthService {
    async fn authenticate(&self, username: &str, password: &str) -> Result<auth_service::AuthContext> {
        self.rate_limiter.check_rate_limit(username).await?;
        self.inner.authenticate(username, password).await
    }

    async fn validate_token(&self, token: &str) -> Result<auth_service::AuthContext> {
        // Extract user from token for rate limiting
        if let Ok(context) = self.inner.validate_token(token).await {
            self.rate_limiter.check_rate_limit(&context.user_id).await?;
            Ok(context)
        } else {
            // Still apply rate limiting for invalid tokens (prevent abuse)
            self.rate_limiter.check_rate_limit("anonymous").await?;
            self.inner.validate_token(token).await
        }
    }

    async fn generate_token(&self, context: &auth_service::AuthContext) -> Result<String> {
        self.rate_limiter.check_rate_limit(&context.user_id).await?;
        self.inner.generate_token(context).await
    }

    async fn check_permission(&self, context: &auth_service::AuthContext, permission: Permission) -> bool {
        // Permission checks are fast, no rate limiting needed
        self.inner.check_permission(context, permission).await
    }

    async fn revoke_token(&self, token: &str) -> Result<()> {
        // Extract user from token for rate limiting
        if let Ok(context) = self.inner.validate_token(token).await {
            self.rate_limiter.check_rate_limit(&context.user_id).await?;
        } else {
            self.rate_limiter.check_rate_limit("anonymous").await?;
        }
        self.inner.revoke_token(token).await
    }
}

#[tokio::test]
async fn test_basic_rate_limiting() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 5); // 5 requests per minute
    
    let config = AuthConfig {
        jwt_secret: "rate_limit_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = RateLimitedAuthService::new(config);
    let user_id = "rate_limit_user";
    
    // First 5 requests should succeed
    for i in 1..=5 {
        let result = auth_service.authenticate(user_id, "password").await;
        assert!(result.is_ok(), "Request {} should succeed", i);
        
        let count = auth_service.get_current_rate_count(user_id).await;
        assert_eq!(count, i as u32, "Request count should be {}", i);
    }
    
    // 6th request should fail due to rate limiting
    let result = auth_service.authenticate(user_id, "password").await;
    assert!(result.is_err(), "Request should be rate limited");
    
    let error_msg = result.err().unwrap().to_string();
    assert!(error_msg.contains("Rate limit exceeded"), "Should indicate rate limiting: {}", error_msg);
    assert!(error_msg.contains("5 requests"), "Should mention the limit");
}

#[tokio::test]
async fn test_rate_limit_window_reset() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 3); // 3 requests per minute
    
    let config = AuthConfig {
        jwt_secret: "window_reset_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    // Create rate limiter with short window for testing
    let rate_limiter = RateLimiter::new(
        config.rate_limits.clone(),
        2 // 2 second window for fast testing
    );
    
    let user_id = "window_reset_user";
    
    // Use up the limit
    for i in 1..=3 {
        let result = rate_limiter.check_rate_limit(user_id).await;
        assert!(result.is_ok(), "Request {} should succeed", i);
    }
    
    // 4th request should fail
    let result = rate_limiter.check_rate_limit(user_id).await;
    assert!(result.is_err(), "Should be rate limited");
    
    // Wait for window to reset
    sleep(Duration::from_secs(3)).await;
    
    // Should be able to make requests again
    let result = rate_limiter.check_rate_limit(user_id).await;
    assert!(result.is_ok(), "Should work after window reset");
    
    let count = rate_limiter.get_current_count(user_id).await;
    assert_eq!(count, 1, "Count should reset to 1 after window expiry");
}

#[tokio::test]
async fn test_per_user_rate_limiting() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 3);
    
    let config = AuthConfig {
        jwt_secret: "per_user_rate_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = RateLimitedAuthService::new(config);
    
    let user1 = "user1";
    let user2 = "user2";
    
    // Each user should have their own rate limit
    for i in 1..=3 {
        // User 1 requests
        let result1 = auth_service.authenticate(user1, "password").await;
        assert!(result1.is_ok(), "User 1 request {} should succeed", i);
        
        // User 2 requests
        let result2 = auth_service.authenticate(user2, "password").await;
        assert!(result2.is_ok(), "User 2 request {} should succeed", i);
    }
    
    // Both users should now be rate limited independently
    let result1 = auth_service.authenticate(user1, "password").await;
    assert!(result1.is_err(), "User 1 should be rate limited");
    
    let result2 = auth_service.authenticate(user2, "password").await;
    assert!(result2.is_err(), "User 2 should be rate limited");
    
    // Check individual counts
    assert_eq!(auth_service.get_current_rate_count(user1).await, 3);
    assert_eq!(auth_service.get_current_rate_count(user2).await, 3);
}

#[tokio::test]
async fn test_rate_limiting_across_different_operations() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 5);
    
    let config = AuthConfig {
        jwt_secret: "cross_ops_rate_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = RateLimitedAuthService::new(config);
    let user_id = "cross_ops_user";
    
    // Mix different operations to consume the rate limit
    
    // 1. Authentication
    let auth_result = auth_service.authenticate(user_id, "password").await;
    assert!(auth_result.is_ok());
    let context = auth_result.unwrap();
    
    // 2. Token generation
    let token_result = auth_service.generate_token(&context).await;
    assert!(token_result.is_ok());
    let token = token_result.unwrap();
    
    // 3. Token validation (uses 3rd request)
    let validate_result = auth_service.validate_token(&token).await;
    assert!(validate_result.is_ok());
    
    // 4. Another authentication (4th request)
    let auth_result2 = auth_service.authenticate(user_id, "password").await;
    assert!(auth_result2.is_ok());
    
    // 5. Another token generation (5th request - should still work)
    let token_result2 = auth_service.generate_token(&context).await;
    assert!(token_result2.is_ok());
    let token2 = token_result2.unwrap();
    
    // 6. Token revocation (6th request - should be rate limited)
    let revoke_result = auth_service.revoke_token(&token).await;
    assert!(revoke_result.is_err(), "Should be rate limited");
    
    // Verify rate limit count
    assert_eq!(auth_service.get_current_rate_count(user_id).await, 5);
}

#[tokio::test]
async fn test_concurrent_rate_limiting() {
    use tokio::task;
    
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 10);
    
    let config = AuthConfig {
        jwt_secret: "concurrent_rate_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = Arc::new(RateLimitedAuthService::new(config));
    let user_id = "concurrent_rate_user";
    let concurrent_requests = 20;
    
    let mut handles = Vec::new();
    
    // Spawn concurrent requests
    for i in 0..concurrent_requests {
        let service = Arc::clone(&auth_service);
        let user = user_id.to_string();
        
        let handle = task::spawn(async move {
            let result = service.authenticate(&user, "password").await;
            (i, result.is_ok())
        });
        
        handles.push(handle);
    }
    
    // Wait for all requests
    let results = futures::future::try_join_all(handles).await.unwrap();
    
    // Count successes and failures
    let successes = results.iter().filter(|(_, success)| *success).count();
    let failures = results.iter().filter(|(_, success)| !*success).count();
    
    println!("Concurrent rate limiting: {} successes, {} failures", successes, failures);
    
    // Should have exactly 10 successes (the rate limit)
    assert_eq!(successes, 10, "Should have exactly 10 successful requests");
    assert_eq!(failures, 10, "Should have exactly 10 rate-limited requests");
    assert_eq!(successes + failures, concurrent_requests);
    
    // Final count should be 10
    let final_count = auth_service.get_current_rate_count(user_id).await;
    assert_eq!(final_count, 10, "Final count should equal the rate limit");
}

#[tokio::test]
async fn test_rate_limit_burst_handling() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 5);
    
    let config = AuthConfig {
        jwt_secret: "burst_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = RateLimitedAuthService::new(config);
    let user_id = "burst_user";
    
    // Send requests in quick burst
    let start = Instant::now();
    let mut results = Vec::new();
    
    for i in 0..10 {
        let result = auth_service.authenticate(user_id, "password").await;
        results.push((i, result.is_ok(), start.elapsed()));
    }
    
    let burst_duration = start.elapsed();
    
    // Analyze burst results
    let successes = results.iter().filter(|(_, success, _)| *success).count();
    let failures = results.iter().filter(|(_, success, _)| !*success).count();
    
    println!("Burst test completed in {:?}", burst_duration);
    println!("Results: {} successes, {} failures", successes, failures);
    
    // Should respect rate limit even in burst
    assert_eq!(successes, 5, "Should allow exactly 5 requests in burst");
    assert_eq!(failures, 5, "Should rate limit 5 requests in burst");
    
    // Burst should be handled quickly
    assert!(burst_duration < Duration::from_millis(100), "Burst should be handled quickly");
}

#[tokio::test]
async fn test_rate_limit_recovery() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 3);
    
    let config = AuthConfig {
        jwt_secret: "recovery_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    // Use short window for faster testing
    let rate_limiter = RateLimiter::new(
        config.rate_limits.clone(),
        1 // 1 second window
    );
    
    let user_id = "recovery_user";
    
    // Hit rate limit
    for _ in 0..3 {
        rate_limiter.check_rate_limit(user_id).await.unwrap();
    }
    
    // Should be rate limited now
    let result = rate_limiter.check_rate_limit(user_id).await;
    assert!(result.is_err());
    
    // Wait for partial recovery (window reset)
    sleep(Duration::from_millis(1100)).await;
    
    // Should be able to make requests again
    for i in 1..=3 {
        let result = rate_limiter.check_rate_limit(user_id).await;
        assert!(result.is_ok(), "Request {} should succeed after recovery", i);
    }
    
    // Should be rate limited again
    let result = rate_limiter.check_rate_limit(user_id).await;
    assert!(result.is_err(), "Should be rate limited again after using up quota");
}

#[tokio::test]
async fn test_rate_limiting_with_invalid_tokens() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 3);
    
    let config = AuthConfig {
        jwt_secret: "invalid_token_rate_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = RateLimitedAuthService::new(config);
    
    // Try to validate invalid tokens (should still be rate limited)
    let invalid_tokens = vec![
        "invalid_token_1",
        "invalid_token_2", 
        "invalid_token_3",
        "invalid_token_4", // This should be rate limited
    ];
    
    let mut results = Vec::new();
    for (i, token) in invalid_tokens.iter().enumerate() {
        let result = auth_service.validate_token(token).await;
        results.push((i, result.is_err()));
        
        // First 3 should fail due to invalid token
        // 4th should fail due to rate limiting
        assert!(result.is_err(), "Invalid token {} should fail", i);
    }
    
    // Check that anonymous user (for invalid tokens) is rate limited
    let anon_count = auth_service.get_current_rate_count("anonymous").await;
    assert_eq!(anon_count, 3, "Anonymous rate limit should be applied to invalid tokens");
    
    // Another invalid token should be rate limited
    let result = auth_service.validate_token("another_invalid_token").await;
    assert!(result.is_err());
    
    let error_msg = result.err().unwrap().to_string();
    assert!(error_msg.contains("Rate limit exceeded") || error_msg.contains("Invalid token"), 
            "Should be rate limited or invalid token: {}", error_msg);
}

#[tokio::test]
async fn test_rate_limit_cleanup() {
    let rate_limiter = RateLimiter::new(
        {
            let mut limits = FxHashMap::default();
            limits.insert("default".to_string(), 100);
            limits
        },
        2 // 2 second window
    );
    
    // Create requests from multiple users
    let users = vec!["user1", "user2", "user3", "user4", "user5"];
    
    for user in &users {
        rate_limiter.check_rate_limit(user).await.unwrap();
    }
    
    // Check all users are tracked
    let mut active_users = 0;
    for user in &users {
        if rate_limiter.get_current_count(user).await > 0 {
            active_users += 1;
        }
    }
    assert_eq!(active_users, 5, "All users should be tracked initially");
    
    // Wait for window to expire
    sleep(Duration::from_millis(2500)).await;
    
    // Make a request to trigger cleanup
    rate_limiter.check_rate_limit("cleanup_trigger").await.unwrap();
    
    // Old entries should be cleaned up
    active_users = 0;
    for user in &users {
        if rate_limiter.get_current_count(user).await > 0 {
            active_users += 1;
        }
    }
    assert_eq!(active_users, 0, "Old user entries should be cleaned up");
    
    // New user should be tracked
    assert_eq!(rate_limiter.get_current_count("cleanup_trigger").await, 1);
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_rate_limiter_performance() {
        let mut rate_limits = FxHashMap::default();
        rate_limits.insert("default".to_string(), 1000);
        
        let rate_limiter = RateLimiter::new(rate_limits, 60);
        
        // Benchmark rate limit checking
        let start = Instant::now();
        let iterations = 10000;
        
        for i in 0..iterations {
            let user_id = format!("perf_user_{}", i % 100); // 100 different users
            let _ = rate_limiter.check_rate_limit(&user_id).await.unwrap();
        }
        
        let duration = start.elapsed();
        let ops_per_second = iterations as f64 / duration.as_secs_f64();
        
        println!("Rate limiter performance: {} ops in {:?} ({:.2} ops/sec)", 
                 iterations, duration, ops_per_second);
        
        // Should handle reasonable throughput
        assert!(ops_per_second > 1000.0, "Rate limiter should handle >1000 ops/sec, got {:.2}", ops_per_second);
        assert!(duration < Duration::from_secs(5), "Should complete in reasonable time");
    }

    #[tokio::test]
    async fn test_concurrent_rate_limiter_performance() {
        use tokio::task;
        
        let mut rate_limits = FxHashMap::default();
        rate_limits.insert("default".to_string(), 10000);
        
        let rate_limiter = Arc::new(RateLimiter::new(rate_limits, 60));
        
        let start = Instant::now();
        let mut handles = Vec::new();
        let concurrent_tasks = 100;
        let ops_per_task = 100;
        
        // Spawn concurrent tasks
        for task_id in 0..concurrent_tasks {
            let limiter = Arc::clone(&rate_limiter);
            
            let handle = task::spawn(async move {
                for i in 0..ops_per_task {
                    let user_id = format!("perf_user_{}_{}", task_id, i);
                    let _ = limiter.check_rate_limit(&user_id).await.unwrap();
                }
                task_id
            });
            
            handles.push(handle);
        }
        
        // Wait for all tasks
        let _results = futures::future::try_join_all(handles).await.unwrap();
        let duration = start.elapsed();
        
        let total_ops = concurrent_tasks * ops_per_task;
        let ops_per_second = total_ops as f64 / duration.as_secs_f64();
        
        println!("Concurrent rate limiter: {} ops in {:?} ({:.2} ops/sec)", 
                 total_ops, duration, ops_per_second);
        
        // Should handle concurrent load efficiently
        assert!(ops_per_second > 2000.0, "Should handle >2000 concurrent ops/sec, got {:.2}", ops_per_second);
        assert!(duration < Duration::from_secs(3), "Should complete concurrent work quickly");
    }
}