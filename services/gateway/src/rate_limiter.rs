//! Rate limiting implementation using token bucket algorithm

use governor::{
    Quota, RateLimiter as GovernorRateLimiter,
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
};
use rustc_hash::FxHashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::config::{EndpointRateLimit, RateLimitConfig};

// Safe constants for fallback values
const DEFAULT_REQUESTS_PER_MINUTE: NonZeroU32 = NonZeroU32::new(60).unwrap();
const DEFAULT_BURST_SIZE: NonZeroU32 = NonZeroU32::new(10).unwrap();

/// Rate limiter for the API Gateway
pub struct RateLimiter {
    /// Global rate limiter
    global_limiter: GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock>,
    /// Per-endpoint rate limiters
    endpoint_limiters:
        Arc<RwLock<FxHashMap<String, GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock>>>>,
    /// Per-IP rate limiters
    ip_limiters:
        Arc<RwLock<FxHashMap<String, GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock>>>>,
    /// Configuration
    config: RateLimitConfig,
}

impl RateLimiter {
    /// Create a new rate limiter
    #[must_use] pub fn new(config: RateLimitConfig) -> Self {
        // Create global rate limiter
        let global_quota = Quota::per_minute(
            NonZeroU32::new(config.requests_per_minute).unwrap_or(DEFAULT_REQUESTS_PER_MINUTE),
        )
        .allow_burst(NonZeroU32::new(config.burst_size).unwrap_or(DEFAULT_BURST_SIZE));
        let global_limiter = GovernorRateLimiter::direct(global_quota);

        // Pre-populate endpoint limiters
        let mut endpoint_limiters = FxHashMap::default();
        for (endpoint, endpoint_config) in &config.endpoint_limits {
            let quota = Quota::per_minute(
                NonZeroU32::new(endpoint_config.requests_per_minute)
                    .unwrap_or(DEFAULT_REQUESTS_PER_MINUTE),
            )
            .allow_burst(NonZeroU32::new(endpoint_config.burst_size).unwrap_or(DEFAULT_BURST_SIZE));
            let limiter = GovernorRateLimiter::direct(quota);
            endpoint_limiters.insert(endpoint.clone(), limiter);
        }

        Self {
            global_limiter,
            endpoint_limiters: Arc::new(RwLock::new(endpoint_limiters)),
            ip_limiters: Arc::new(RwLock::new(FxHashMap::default())),
            config,
        }
    }

    /// Check if a request should be rate limited
    pub async fn check_rate_limit(&self, client_ip: &str, path: &str) -> bool {
        if !self.config.enabled {
            return true;
        }

        // Check global rate limit
        if self.global_limiter.check().is_err() {
            warn!("Global rate limit exceeded");
            return false;
        }

        // Check per-IP rate limit
        if !self.check_ip_rate_limit(client_ip).await {
            return false;
        }

        // Check per-endpoint rate limit
        if !self.check_endpoint_rate_limit(path).await {
            return false;
        }

        debug!(
            "Rate limit check passed for IP: {} on path: {}",
            client_ip, path
        );
        true
    }

    /// Check per-IP rate limit
    async fn check_ip_rate_limit(&self, client_ip: &str) -> bool {
        let mut ip_limiters = self.ip_limiters.write().await;

        // Get or create limiter for this IP
        let limiter = ip_limiters.entry(client_ip.to_string()).or_insert_with(|| {
            let quota = Quota::per_minute(
                NonZeroU32::new(self.config.requests_per_minute)
                    .unwrap_or(DEFAULT_REQUESTS_PER_MINUTE),
            )
            .allow_burst(NonZeroU32::new(self.config.burst_size).unwrap_or(DEFAULT_BURST_SIZE));
            GovernorRateLimiter::direct(quota)
        });

        match limiter.check() {
            Ok(()) => true,
            Err(rate_limit_error) => {
                warn!(
                    "IP rate limit exceeded for {}: {:?}",
                    client_ip, rate_limit_error
                );
                false
            }
        }
    }

    /// Check per-endpoint rate limit
    async fn check_endpoint_rate_limit(&self, path: &str) -> bool {
        // Find matching endpoint configuration
        let endpoint_config = match self.find_endpoint_config(path) {
            Some(config) => config,
            None => return true, // No specific limit for this endpoint
        };
        let mut endpoint_limiters = self.endpoint_limiters.write().await;

        // Get or create limiter for this endpoint
        let limiter = endpoint_limiters
            .entry(path.to_string())
            .or_insert_with(|| {
                let quota = Quota::per_minute(
                    NonZeroU32::new(endpoint_config.requests_per_minute)
                        .unwrap_or(DEFAULT_REQUESTS_PER_MINUTE),
                )
                .allow_burst(
                    NonZeroU32::new(endpoint_config.burst_size).unwrap_or(DEFAULT_BURST_SIZE),
                );
                GovernorRateLimiter::direct(quota)
            });

        match limiter.check() {
            Ok(()) => true,
            Err(rate_limit_error) => {
                warn!(
                    "Endpoint rate limit exceeded for {}: {:?}",
                    path, rate_limit_error
                );
                false
            }
        }
    }

    /// Find endpoint configuration for a given path
    fn find_endpoint_config(&self, path: &str) -> Option<&EndpointRateLimit> {
        // Try exact match first
        if let Some(config) = self.config.endpoint_limits.get(path) {
            return Some(config);
        }

        // Try pattern matching (simple prefix matching)
        for (pattern, config) in &self.config.endpoint_limits {
            if path.starts_with(pattern) {
                return Some(config);
            }
        }

        None
    }

    /// Get current rate limiting statistics
    pub async fn get_stats(&self) -> RateLimitStats {
        let ip_limiters = self.ip_limiters.read().await;
        let endpoint_limiters = self.endpoint_limiters.read().await;

        RateLimitStats {
            total_ips: ip_limiters.len(),
            total_endpoints: endpoint_limiters.len(),
            global_limit_per_minute: self.config.requests_per_minute,
            global_burst_size: self.config.burst_size,
        }
    }

    /// Clear old rate limiters to prevent memory leaks
    pub async fn cleanup_old_limiters(&self) {
        const MAX_LIMITERS: usize = 10000;

        // Clean up IP limiters if too many
        let mut ip_limiters = self.ip_limiters.write().await;
        if ip_limiters.len() > MAX_LIMITERS {
            let keys_to_remove: Vec<_> = ip_limiters
                .keys()
                .take(ip_limiters.len() - MAX_LIMITERS)
                .cloned()
                .collect();
            for key in keys_to_remove {
                ip_limiters.remove(&key);
            }
        }
    }
}

/// Rate limiting statistics
#[derive(Debug, serde::Serialize)]
pub struct RateLimitStats {
    pub total_ips: usize,
    pub total_endpoints: usize,
    pub global_limit_per_minute: u32,
    pub global_burst_size: u32,
}
