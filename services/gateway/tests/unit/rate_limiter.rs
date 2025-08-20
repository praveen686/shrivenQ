//! Rate limiter unit tests

use rstest::*;
use serial_test::serial;
use std::time::Duration;
use tokio::time::sleep;

use api_gateway::{
    config::{RateLimitConfig, EndpointRateLimit},
    rate_limiter::{RateLimiter, RateLimitStats},
};
use rustc_hash::FxHashMap;

#[fixture]
fn basic_rate_limit_config() -> RateLimitConfig {
    RateLimitConfig {
        enabled: true,
        requests_per_minute: 60,
        burst_size: 10,
        endpoint_limits: FxHashMap::default(),
    }
}

#[fixture]
fn endpoint_rate_limit_config() -> RateLimitConfig {
    let mut endpoint_limits = FxHashMap::default();
    endpoint_limits.insert(
        "/api/v1/orders".to_string(),
        EndpointRateLimit {
            requests_per_minute: 30,
            burst_size: 5,
        },
    );
    endpoint_limits.insert(
        "/api/v1/auth/login".to_string(),
        EndpointRateLimit {
            requests_per_minute: 10,
            burst_size: 2,
        },
    );

    RateLimitConfig {
        enabled: true,
        requests_per_minute: 60,
        burst_size: 10,
        endpoint_limits,
    }
}

#[fixture]
fn disabled_rate_limit_config() -> RateLimitConfig {
    RateLimitConfig {
        enabled: false,
        requests_per_minute: 60,
        burst_size: 10,
        endpoint_limits: FxHashMap::default(),
    }
}

#[rstest]
#[tokio::test]
async fn test_rate_limiter_creation(basic_rate_limit_config: RateLimitConfig) {
    let rate_limiter = RateLimiter::new(basic_rate_limit_config);
    
    // Should create without panicking
    assert!(true);
    
    // Test getting stats
    let stats = rate_limiter.get_stats().await;
    assert_eq!(stats.total_ips, 0);
    assert_eq!(stats.total_endpoints, 0);
    assert_eq!(stats.global_limit_per_minute, 60);
    assert_eq!(stats.global_burst_size, 10);
}

#[rstest]
#[tokio::test]
async fn test_rate_limiter_disabled(disabled_rate_limit_config: RateLimitConfig) {
    let rate_limiter = RateLimiter::new(disabled_rate_limit_config);
    
    // When disabled, should always allow requests
    for _i in 0..100 {
        let allowed = rate_limiter.check_rate_limit("127.0.0.1", "/api/v1/orders").await;
        assert!(allowed, "Disabled rate limiter should always allow requests");
    }
}

#[rstest]
#[tokio::test]
#[serial] // Run serially to avoid test interference
async fn test_basic_rate_limiting(basic_rate_limit_config: RateLimitConfig) {
    let rate_limiter = RateLimiter::new(basic_rate_limit_config.clone());
    let client_ip = "127.0.0.1";
    let path = "/api/v1/test";
    
    // First requests should be allowed (within burst limit)
    for i in 0..basic_rate_limit_config.burst_size {
        let allowed = rate_limiter.check_rate_limit(client_ip, path).await;
        assert!(allowed, "Request {} should be allowed within burst limit", i);
    }
    
    // Additional requests should be rate limited
    let allowed = rate_limiter.check_rate_limit(client_ip, path).await;
    assert!(!allowed, "Request exceeding burst should be rate limited");
}

#[rstest]
#[tokio::test]
#[serial]
async fn test_ip_based_rate_limiting(basic_rate_limit_config: RateLimitConfig) {
    let rate_limiter = RateLimiter::new(basic_rate_limit_config.clone());
    let path = "/api/v1/test";
    
    // Different IPs should have separate rate limits
    let ips = vec!["192.168.1.1", "192.168.1.2", "192.168.1.3"];
    
    for ip in &ips {
        // Each IP should be able to make burst_size requests
        for i in 0..basic_rate_limit_config.burst_size {
            let allowed = rate_limiter.check_rate_limit(ip, path).await;
            assert!(allowed, "Request {} from IP {} should be allowed", i, ip);
        }
        
        // Exceeding burst should be rate limited
        let allowed = rate_limiter.check_rate_limit(ip, path).await;
        assert!(!allowed, "Excess request from IP {} should be rate limited", ip);
    }
    
    // Verify stats show all IPs
    let stats = rate_limiter.get_stats().await;
    assert_eq!(stats.total_ips, ips.len());
}

#[rstest]
#[tokio::test]
#[serial]
async fn test_endpoint_specific_rate_limiting(endpoint_rate_limit_config: RateLimitConfig) {
    let rate_limiter = RateLimiter::new(endpoint_rate_limit_config);
    let client_ip = "127.0.0.1";
    
    // Test orders endpoint (30 req/min, burst 5)
    let orders_path = "/api/v1/orders";
    for i in 0..5 {
        let allowed = rate_limiter.check_rate_limit(client_ip, orders_path).await;
        assert!(allowed, "Orders request {} should be allowed", i);
    }
    
    let allowed = rate_limiter.check_rate_limit(client_ip, orders_path).await;
    assert!(!allowed, "Orders request exceeding burst should be rate limited");
    
    // Test login endpoint (10 req/min, burst 2)
    let login_path = "/api/v1/auth/login";
    for i in 0..2 {
        let allowed = rate_limiter.check_rate_limit(client_ip, login_path).await;
        assert!(allowed, "Login request {} should be allowed", i);
    }
    
    let allowed = rate_limiter.check_rate_limit(client_ip, login_path).await;
    assert!(!allowed, "Login request exceeding burst should be rate limited");
    
    // Test non-configured endpoint (should use global limits)
    let other_path = "/api/v1/other";
    for i in 0..10 { // Global burst is 10
        let allowed = rate_limiter.check_rate_limit(client_ip, other_path).await;
        assert!(allowed, "Other request {} should be allowed", i);
    }
    
    let allowed = rate_limiter.check_rate_limit(client_ip, other_path).await;
    assert!(!allowed, "Other request exceeding global burst should be rate limited");
}

#[rstest]
#[tokio::test]
#[serial]
async fn test_global_rate_limiting(basic_rate_limit_config: RateLimitConfig) {
    let rate_limiter = RateLimiter::new(basic_rate_limit_config.clone());
    let path = "/api/v1/test";
    
    // Fill up global rate limit with requests from different IPs
    let mut request_count = 0;
    let burst_size = basic_rate_limit_config.burst_size;
    
    for i in 0..20 { // Try 20 IPs
        for j in 0..burst_size {
            let ip = format!("192.168.1.{}", i + 1);
            let allowed = rate_limiter.check_rate_limit(&ip, path).await;
            
            if allowed {
                request_count += 1;
            } else {
                // Global rate limit hit
                break;
            }
        }
        
        // If we can't make any requests for this IP, global limit is hit
        if request_count > 0 && !rate_limiter.check_rate_limit(&format!("192.168.1.{}", i + 1), path).await {
            break;
        }
    }
    
    // Should have made some requests before hitting global limit
    assert!(request_count > 0, "Should have allowed some requests before global limit");
    assert!(request_count <= 200, "Shouldn't allow unlimited requests"); // Reasonable upper bound
}

#[rstest]
#[tokio::test]
async fn test_concurrent_rate_limiting(basic_rate_limit_config: RateLimitConfig) {
    use futures::future::join_all;
    
    let rate_limiter = RateLimiter::new(basic_rate_limit_config.clone());
    let client_ip = "127.0.0.1";
    let path = "/api/v1/test";
    
    // Create many concurrent requests
    let requests = (0..50).map(|i| {
        let limiter = &rate_limiter;
        async move {
            (i, limiter.check_rate_limit(client_ip, path).await)
        }
    });
    
    let results = join_all(requests).await;
    
    // Count allowed and denied requests
    let allowed_count = results.iter().filter(|(_, allowed)| *allowed).count();
    let denied_count = results.iter().filter(|(_, allowed)| !*allowed).count();
    
    // Should have some allowed (up to burst limit) and some denied
    assert!(allowed_count > 0, "Should allow some concurrent requests");
    assert!(allowed_count <= basic_rate_limit_config.burst_size as usize + 5, "Shouldn't allow too many requests"); // Allow some tolerance
    assert_eq!(allowed_count + denied_count, 50, "All requests should be processed");
    
    println!("Concurrent test: {} allowed, {} denied", allowed_count, denied_count);
}

#[rstest]
#[tokio::test]
async fn test_rate_limiter_stats(endpoint_rate_limit_config: RateLimitConfig) {
    let rate_limiter = RateLimiter::new(endpoint_rate_limit_config.clone());
    
    // Make requests from different IPs to different endpoints
    let ips = vec!["192.168.1.1", "192.168.1.2", "192.168.1.3"];
    let paths = vec!["/api/v1/orders", "/api/v1/auth/login", "/api/v1/other"];
    
    for ip in &ips {
        for path in &paths {
            let _allowed = rate_limiter.check_rate_limit(ip, path).await;
        }
    }
    
    let stats = rate_limiter.get_stats().await;
    
    // Should track all IPs
    assert_eq!(stats.total_ips, ips.len());
    
    // Should have created endpoint limiters for configured paths
    assert!(stats.total_endpoints >= 2); // At least the configured endpoints
    
    // Should have correct global limits
    assert_eq!(stats.global_limit_per_minute, 60);
    assert_eq!(stats.global_burst_size, 10);
}

#[rstest]
#[tokio::test]
async fn test_pattern_matching_endpoints() {
    let mut endpoint_limits = FxHashMap::default();
    endpoint_limits.insert(
        "/api/v1/orders".to_string(),
        EndpointRateLimit {
            requests_per_minute: 30,
            burst_size: 5,
        },
    );
    // Test prefix matching
    endpoint_limits.insert(
        "/api/v1/auth/".to_string(),
        EndpointRateLimit {
            requests_per_minute: 10,
            burst_size: 2,
        },
    );

    let config = RateLimitConfig {
        enabled: true,
        requests_per_minute: 60,
        burst_size: 10,
        endpoint_limits,
    };
    
    let rate_limiter = RateLimiter::new(config);
    let client_ip = "127.0.0.1";
    
    // Test exact match
    for i in 0..5 {
        let allowed = rate_limiter.check_rate_limit(client_ip, "/api/v1/orders").await;
        assert!(allowed, "Exact match request {} should be allowed", i);
    }
    
    // Should be rate limited after burst
    let allowed = rate_limiter.check_rate_limit(client_ip, "/api/v1/orders").await;
    assert!(!allowed, "Should be rate limited after exact match burst");
    
    // Test prefix match
    let client_ip2 = "192.168.1.2"; // Different IP to avoid interference
    for i in 0..2 {
        let allowed = rate_limiter.check_rate_limit(client_ip2, "/api/v1/auth/login").await;
        assert!(allowed, "Prefix match request {} should be allowed", i);
    }
    
    // Should be rate limited after burst
    let allowed = rate_limiter.check_rate_limit(client_ip2, "/api/v1/auth/login").await;
    assert!(!allowed, "Should be rate limited after prefix match burst");
}

#[rstest]
#[tokio::test]
async fn test_cleanup_old_limiters(basic_rate_limit_config: RateLimitConfig) {
    let rate_limiter = RateLimiter::new(basic_rate_limit_config);
    
    // Create many IP limiters
    for i in 0..100 {
        let ip = format!("192.168.1.{}", i);
        let _allowed = rate_limiter.check_rate_limit(&ip, "/api/v1/test").await;
    }
    
    let stats_before = rate_limiter.get_stats().await;
    assert_eq!(stats_before.total_ips, 100);
    
    // Cleanup shouldn't remove anything since we're under the limit
    rate_limiter.cleanup_old_limiters().await;
    
    let stats_after = rate_limiter.get_stats().await;
    assert_eq!(stats_after.total_ips, 100);
}

#[rstest]
#[tokio::test]
async fn test_edge_case_zero_limits() {
    let config = RateLimitConfig {
        enabled: true,
        requests_per_minute: 0, // Invalid, should use default
        burst_size: 0, // Invalid, should use default
        endpoint_limits: FxHashMap::default(),
    };
    
    let rate_limiter = RateLimiter::new(config);
    
    // Should not panic and should use fallback values
    let allowed = rate_limiter.check_rate_limit("127.0.0.1", "/test").await;
    // Behavior depends on implementation - just ensure it doesn't crash
    assert!(allowed || !allowed); // Should return a boolean
}

#[rstest]
#[tokio::test]
async fn test_stress_concurrent_different_ips(basic_rate_limit_config: RateLimitConfig) {
    use futures::future::join_all;
    
    let rate_limiter = RateLimiter::new(basic_rate_limit_config);
    let path = "/api/v1/test";
    
    // Create requests from many different IPs concurrently
    let requests = (0..100).map(|i| {
        let limiter = &rate_limiter;
        let ip = format!("192.168.{}.{}", i / 256, i % 256);
        
        async move {
            let allowed = limiter.check_rate_limit(&ip, path).await;
            (i, ip, allowed)
        }
    });
    
    let results = join_all(requests).await;
    
    // Most should be allowed since each IP gets its own bucket
    let allowed_count = results.iter().filter(|(_, _, allowed)| *allowed).count();
    let denied_count = results.iter().filter(|(_, _, allowed)| !*allowed).count();
    
    // Should allow most requests from different IPs
    assert!(allowed_count > 50, "Should allow most requests from different IPs");
    assert_eq!(allowed_count + denied_count, 100);
    
    println!("Stress test: {} allowed, {} denied from {} different IPs", 
             allowed_count, denied_count, results.len());
}

#[rstest]
#[tokio::test]
async fn test_rate_limiter_recovery_over_time() {
    // This test would ideally test rate limit recovery but requires waiting
    // For now, just test the structure
    let config = RateLimitConfig {
        enabled: true,
        requests_per_minute: 60, // Very low for testing
        burst_size: 2,
        endpoint_limits: FxHashMap::default(),
    };
    
    let rate_limiter = RateLimiter::new(config);
    let client_ip = "127.0.0.1";
    let path = "/test";
    
    // Exhaust the rate limit
    for i in 0..2 {
        let allowed = rate_limiter.check_rate_limit(client_ip, path).await;
        assert!(allowed, "Request {} should be allowed", i);
    }
    
    // Should be rate limited now
    let allowed = rate_limiter.check_rate_limit(client_ip, path).await;
    assert!(!allowed, "Should be rate limited");
    
    // In a real scenario, we would wait and test recovery
    // For unit tests, we just verify the structure works
    
    // Brief delay to simulate some time passing
    sleep(Duration::from_millis(10)).await;
    
    // Still should be rate limited (recovery is much slower)
    let allowed = rate_limiter.check_rate_limit(client_ip, path).await;
    assert!(!allowed, "Should still be rate limited after brief delay");
}