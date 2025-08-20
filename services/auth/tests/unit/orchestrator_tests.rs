//! Integration tests for multi-exchange authentication orchestrator

use super::test_utils::*;
use anyhow::{anyhow, Result};
use auth_service::{AuthService, Permission};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Mock multi-exchange orchestrator for testing
pub struct MockAuthOrchestrator {
    pub binance_service: Option<Arc<MockAuthService>>,
    pub zerodha_service: Option<Arc<MockAuthService>>,
    pub demo_service: Option<Arc<MockAuthService>>,
    pub active_connections: Arc<RwLock<Vec<String>>>,
    pub metrics: Arc<RwLock<OrchestratorMetrics>>,
    pub fallback_enabled: bool,
}

#[derive(Debug, Default)]
pub struct OrchestratorMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub binance_requests: u64,
    pub zerodha_requests: u64,
    pub demo_requests: u64,
    pub fallback_activations: u64,
    pub average_latency_ms: f64,
}

impl MockAuthOrchestrator {
    pub fn new() -> Self {        
        Self {
            binance_service: None,
            zerodha_service: None,
            demo_service: Some(Arc::new(MockAuthService::new())),
            active_connections: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(RwLock::new(OrchestratorMetrics::default())),
            fallback_enabled: true,
        }
    }

    pub fn with_binance(mut self, service: Arc<MockAuthService>) -> Self {
        self.binance_service = Some(service);
        self
    }

    pub fn with_zerodha(mut self, service: Arc<MockAuthService>) -> Self {
        self.zerodha_service = Some(service);
        self
    }

    pub fn with_fallback(mut self, enabled: bool) -> Self {
        self.fallback_enabled = enabled;
        self
    }

    /// Route authentication to appropriate exchange
    pub async fn authenticate(&self, username: &str, password: &str, exchange: &str) -> Result<auth_service::AuthContext> {
        let start = Instant::now();
        let mut metrics = self.metrics.write().await;
        metrics.total_requests += 1;

        let result = match exchange.to_lowercase().as_str() {
            "binance" => {
                metrics.binance_requests += 1;
                if let Some(service) = &self.binance_service {
                    service.authenticate(username, password).await
                } else if self.fallback_enabled {
                    metrics.fallback_activations += 1;
                    self.demo_service.as_ref().unwrap().authenticate(username, password).await
                } else {
                    Err(anyhow!("Binance service not available"))
                }
            },
            "zerodha" | "kite" => {
                metrics.zerodha_requests += 1;
                if let Some(service) = &self.zerodha_service {
                    service.authenticate(username, password).await
                } else if self.fallback_enabled {
                    metrics.fallback_activations += 1;
                    self.demo_service.as_ref().unwrap().authenticate(username, password).await
                } else {
                    Err(anyhow!("Zerodha service not available"))
                }
            },
            "demo" => {
                metrics.demo_requests += 1;
                self.demo_service.as_ref().unwrap().authenticate(username, password).await
            },
            _ => Err(anyhow!("Unsupported exchange: {}", exchange)),
        };

        // Update metrics
        let latency = start.elapsed();
        match &result {
            Ok(_) => {
                metrics.successful_requests += 1;
                self.active_connections.write().await.push(format!("{}@{}", username, exchange));
            },
            Err(_) => metrics.failed_requests += 1,
        }

        // Update average latency
        let total_completed = metrics.successful_requests + metrics.failed_requests;
        if total_completed > 0 {
            metrics.average_latency_ms = 
                (metrics.average_latency_ms * (total_completed - 1) as f64 + latency.as_millis() as f64) / total_completed as f64;
        }

        result
    }

    /// Test connectivity to all exchanges
    pub async fn test_connectivity(&self, exchange: &str) -> Result<Vec<(String, bool, Duration)>> {
        let exchanges = if exchange == "all" {
            vec!["binance", "zerodha", "demo"]
        } else {
            vec![exchange]
        };

        let mut results = Vec::new();

        for ex in exchanges {
            let start = Instant::now();
            let test_result = match ex {
                "binance" => {
                    if let Some(service) = &self.binance_service {
                        service.authenticate("test", "test").await.is_ok()
                    } else {
                        false
                    }
                },
                "zerodha" => {
                    if let Some(service) = &self.zerodha_service {
                        service.authenticate("test", "test").await.is_ok()
                    } else {
                        false
                    }
                },
                "demo" => {
                    self.demo_service.as_ref().unwrap().authenticate("test", "test").await.is_ok()
                },
                _ => false,
            };
            let duration = start.elapsed();

            results.push((ex.to_string(), test_result, duration));
        }

        Ok(results)
    }

    /// Get current orchestrator metrics
    pub async fn get_metrics(&self) -> OrchestratorMetrics {
        let guard = self.metrics.read().await;
        OrchestratorMetrics {
            total_requests: guard.total_requests,
            successful_requests: guard.successful_requests,
            failed_requests: guard.failed_requests,
            binance_requests: guard.binance_requests,
            zerodha_requests: guard.zerodha_requests,
            demo_requests: guard.demo_requests,
            fallback_activations: guard.fallback_activations,
            average_latency_ms: guard.average_latency_ms,
        }
    }

    /// Get active connections
    pub async fn get_active_connections(&self) -> Vec<String> {
        self.active_connections.read().await.clone()
    }

    /// Reset metrics
    pub async fn reset_metrics(&self) {
        let mut metrics = self.metrics.write().await;
        *metrics = OrchestratorMetrics::default();
        self.active_connections.write().await.clear();
    }
}

#[tokio::test]
async fn test_orchestrator_basic_routing() {
    let binance_service = Arc::new(MockAuthService::new());
    let zerodha_service = Arc::new(MockAuthService::new());

    // Setup mock users
    binance_service.add_user("binance_user".to_string(), create_test_auth_context("binance_user")).await;
    zerodha_service.add_user("zerodha_user".to_string(), create_test_auth_context("zerodha_user")).await;

    let orchestrator = MockAuthOrchestrator::new()
        .with_binance(binance_service)
        .with_zerodha(zerodha_service);

    // Test Binance routing
    let result = orchestrator.authenticate("binance_user", "password", "binance").await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().user_id, "binance_user");

    // Test Zerodha routing
    let result = orchestrator.authenticate("zerodha_user", "password", "zerodha").await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().user_id, "zerodha_user");

    // Test demo routing
    let result = orchestrator.authenticate("demo_user", "password", "demo").await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().user_id, "demo_user");

    // Test invalid exchange
    let result = orchestrator.authenticate("user", "password", "invalid_exchange").await;
    assert!(result.is_err());
    assert!(result.err().unwrap().to_string().contains("Unsupported exchange"));
}

#[tokio::test]
async fn test_orchestrator_fallback_mechanism() {
    let orchestrator = MockAuthOrchestrator::new()
        .with_fallback(true); // Only demo service available

    // Request for unavailable service should fallback to demo
    let result = orchestrator.authenticate("fallback_user", "password", "binance").await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().user_id, "fallback_user");

    // Check metrics
    let metrics = orchestrator.get_metrics().await;
    assert_eq!(metrics.binance_requests, 1);
    assert_eq!(metrics.fallback_activations, 1);
    assert_eq!(metrics.successful_requests, 1);
}

#[tokio::test]
async fn test_orchestrator_without_fallback() {
    let orchestrator = MockAuthOrchestrator::new()
        .with_fallback(false); // Fallback disabled

    // Request for unavailable service should fail
    let result = orchestrator.authenticate("no_fallback_user", "password", "binance").await;
    assert!(result.is_err());
    assert!(result.err().unwrap().to_string().contains("Binance service not available"));

    // Check metrics
    let metrics = orchestrator.get_metrics().await;
    assert_eq!(metrics.binance_requests, 1);
    assert_eq!(metrics.fallback_activations, 0);
    assert_eq!(metrics.failed_requests, 1);
}

#[tokio::test]
async fn test_orchestrator_connectivity_testing() {
    let binance_service = Arc::new(MockAuthService::new());
    let zerodha_service = Arc::new(MockAuthService::new());
    
    binance_service.add_user("test".to_string(), create_test_auth_context("test")).await;
    zerodha_service.add_user("test".to_string(), create_test_auth_context("test")).await;

    let orchestrator = MockAuthOrchestrator::new()
        .with_binance(binance_service)
        .with_zerodha(zerodha_service);

    // Test all exchanges
    let results = orchestrator.test_connectivity("all").await.unwrap();
    assert_eq!(results.len(), 3); // binance, zerodha, demo

    for (exchange, connected, latency) in results {
        assert!(connected, "{} should be connected", exchange);
        assert!(latency < Duration::from_secs(1), "{} latency should be reasonable", exchange);
    }

    // Test specific exchange
    let results = orchestrator.test_connectivity("binance").await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "binance");
    assert!(results[0].1); // Should be connected
}

#[tokio::test]
async fn test_orchestrator_service_failure_handling() {
    let failing_binance = Arc::new(MockAuthService::new());
    failing_binance.set_should_fail(true).await;

    let working_zerodha = Arc::new(MockAuthService::new());
    working_zerodha.add_user("zerodha_user".to_string(), create_test_auth_context("zerodha_user")).await;

    let orchestrator = MockAuthOrchestrator::new()
        .with_binance(failing_binance)
        .with_zerodha(working_zerodha)
        .with_fallback(true);

    // Binance request should fail, fallback to demo
    let result = orchestrator.authenticate("binance_user", "password", "binance").await;
    assert!(result.is_ok()); // Should succeed via fallback
    assert_eq!(result.unwrap().user_id, "binance_user");

    // Zerodha should work normally
    let result = orchestrator.authenticate("zerodha_user", "password", "zerodha").await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().user_id, "zerodha_user");

    // Check metrics
    let metrics = orchestrator.get_metrics().await;
    assert_eq!(metrics.successful_requests, 2);
    assert_eq!(metrics.fallback_activations, 1);
}

#[tokio::test]
async fn test_orchestrator_concurrent_requests() {
    use tokio::task;

    let binance_service = Arc::new(MockAuthService::new());
    let zerodha_service = Arc::new(MockAuthService::new());

    // Setup users for both services
    for i in 0..50 {
        let username = format!("user_{}", i);
        binance_service.add_user(username.clone(), create_test_auth_context(&username)).await;
        zerodha_service.add_user(username, create_test_auth_context(&format!("zrd_user_{}", i))).await;
    }

    let orchestrator = Arc::new(
        MockAuthOrchestrator::new()
            .with_binance(binance_service)
            .with_zerodha(zerodha_service)
    );

    let mut handles = Vec::new();
    let concurrent_requests = 100;

    // Spawn concurrent requests to different exchanges
    for i in 0..concurrent_requests {
        let orch = Arc::clone(&orchestrator);
        let handle = task::spawn(async move {
            let username = format!("user_{}", i % 50);
            let exchange = if i % 2 == 0 { "binance" } else { "zerodha" };
            
            let result = orch.authenticate(&username, "password", exchange).await;
            (i, exchange, result.is_ok())
        });
        handles.push(handle);
    }

    // Wait for all requests
    let results = futures::future::try_join_all(handles).await.unwrap();

    // Analyze results
    let successful = results.iter().filter(|(_, _, success)| *success).count();
    let failed = results.iter().filter(|(_, _, success)| !*success).count();
    let binance_requests = results.iter().filter(|(_, exchange, _)| *exchange == "binance").count();
    let zerodha_requests = results.iter().filter(|(_, exchange, _)| *exchange == "zerodha").count();

    println!("Concurrent orchestrator test: {} successful, {} failed", successful, failed);
    println!("Binance requests: {}, Zerodha requests: {}", binance_requests, zerodha_requests);

    // All requests should succeed
    assert_eq!(successful, concurrent_requests);
    assert_eq!(failed, 0);
    assert_eq!(binance_requests, 50);
    assert_eq!(zerodha_requests, 50);

    // Check final metrics
    let metrics = orchestrator.get_metrics().await;
    assert_eq!(metrics.total_requests, concurrent_requests as u64);
    assert_eq!(metrics.successful_requests, concurrent_requests as u64);
    assert_eq!(metrics.binance_requests, 50);
    assert_eq!(metrics.zerodha_requests, 50);
    assert!(metrics.average_latency_ms > 0.0);
}

#[tokio::test]
async fn test_orchestrator_metrics_tracking() {
    let orchestrator = MockAuthOrchestrator::new();

    // Initial metrics should be empty
    let initial_metrics = orchestrator.get_metrics().await;
    assert_eq!(initial_metrics.total_requests, 0);
    assert_eq!(initial_metrics.successful_requests, 0);
    assert_eq!(initial_metrics.failed_requests, 0);

    // Make some requests
    let _ = orchestrator.authenticate("user1", "password", "demo").await;
    let _ = orchestrator.authenticate("user2", "password", "demo").await;
    let _ = orchestrator.authenticate("user3", "password", "invalid_exchange").await; // This will fail

    // Check updated metrics
    let metrics = orchestrator.get_metrics().await;
    assert_eq!(metrics.total_requests, 3);
    assert_eq!(metrics.successful_requests, 2);
    assert_eq!(metrics.failed_requests, 1);
    assert_eq!(metrics.demo_requests, 2);
    assert!(metrics.average_latency_ms > 0.0);

    // Check active connections
    let connections = orchestrator.get_active_connections().await;
    assert_eq!(connections.len(), 2);
    assert!(connections.contains(&"user1@demo".to_string()));
    assert!(connections.contains(&"user2@demo".to_string()));

    // Reset metrics
    orchestrator.reset_metrics().await;
    let reset_metrics = orchestrator.get_metrics().await;
    assert_eq!(reset_metrics.total_requests, 0);
    assert!(orchestrator.get_active_connections().await.is_empty());
}

#[tokio::test]
async fn test_orchestrator_exchange_preference() {
    let binance_service = Arc::new(MockAuthService::new());
    let zerodha_service = Arc::new(MockAuthService::new());

    // Setup same user on both services
    let context_binance = create_test_auth_context("multi_exchange_user");
    let mut context_zerodha = create_test_auth_context("multi_exchange_user");
    context_zerodha.metadata.insert("exchange".to_string(), "zerodha".to_string());

    binance_service.add_user("multi_exchange_user".to_string(), context_binance).await;
    zerodha_service.add_user("multi_exchange_user".to_string(), context_zerodha).await;

    let orchestrator = MockAuthOrchestrator::new()
        .with_binance(binance_service)
        .with_zerodha(zerodha_service);

    // Same user should get different contexts based on exchange choice
    let binance_result = orchestrator.authenticate("multi_exchange_user", "password", "binance").await.unwrap();
    let zerodha_result = orchestrator.authenticate("multi_exchange_user", "password", "zerodha").await.unwrap();

    assert_eq!(binance_result.user_id, "multi_exchange_user");
    assert_eq!(zerodha_result.user_id, "multi_exchange_user");

    // Metadata should differ
    assert!(!binance_result.metadata.contains_key("exchange") || 
            binance_result.metadata.get("exchange") != Some(&"zerodha".to_string()));
    assert_eq!(zerodha_result.metadata.get("exchange"), Some(&"zerodha".to_string()));
}

#[tokio::test]
async fn test_orchestrator_load_balancing() {
    // Create multiple instances of the same service for load balancing simulation
    let service1 = Arc::new(MockAuthService::new());
    let service2 = Arc::new(MockAuthService::new());

    // Setup users
    for i in 0..20 {
        let username = format!("lb_user_{}", i);
        service1.add_user(username.clone(), create_test_auth_context(&username)).await;
        service2.add_user(username, create_test_auth_context(&format!("srv2_{}", i))).await;
    }

    // Simple round-robin load balancer simulation
    struct LoadBalancedOrchestrator {
        services: Vec<Arc<MockAuthService>>,
        current_index: Arc<tokio::sync::Mutex<usize>>,
        metrics: Arc<RwLock<FxHashMap<usize, u64>>>,
    }

    impl LoadBalancedOrchestrator {
        fn new(services: Vec<Arc<MockAuthService>>) -> Self {
            let metrics = Arc::new(RwLock::new(FxHashMap::default()));
            for i in 0..services.len() {
                metrics.blocking_write().insert(i, 0);
            }
            
            Self {
                services,
                current_index: Arc::new(tokio::sync::Mutex::new(0)),
                metrics,
            }
        }

        async fn authenticate(&self, username: &str, password: &str) -> Result<auth_service::AuthContext> {
            let mut index = self.current_index.lock().await;
            let service_index = *index;
            *index = (*index + 1) % self.services.len();
            drop(index);

            let result = self.services[service_index].authenticate(username, password).await;
            
            if result.is_ok() {
                let mut metrics = self.metrics.write().await;
                *metrics.entry(service_index).or_insert(0) += 1;
            }

            result
        }

        async fn get_load_distribution(&self) -> Vec<(usize, u64)> {
            let metrics = self.metrics.read().await;
            metrics.iter().map(|(&k, &v)| (k, v)).collect()
        }
    }

    let lb_orchestrator = Arc::new(LoadBalancedOrchestrator::new(vec![service1, service2]));

    // Make requests that should be load balanced
    let mut handles = Vec::new();
    
    for i in 0..20 {
        let orch = Arc::clone(&lb_orchestrator);
        let username = format!("lb_user_{}", i);
        
        let handle = tokio::spawn(async move {
            orch.authenticate(&username, "password").await.is_ok()
        });
        
        handles.push(handle);
    }

    let results = futures::future::try_join_all(handles).await.unwrap();
    let successful = results.iter().filter(|&&success| success).count();

    assert_eq!(successful, 20, "All load balanced requests should succeed");

    // Check load distribution
    let distribution = lb_orchestrator.get_load_distribution().await;
    println!("Load distribution: {:?}", distribution);
    
    // Should be roughly evenly distributed
    for (service_id, count) in distribution {
        assert!(count > 0, "Service {} should have handled some requests", service_id);
        assert!(count <= 12, "Service {} should not be overloaded (max 12/20)", service_id);
    }
}

#[tokio::test]
async fn test_orchestrator_circuit_breaker_pattern() {
    // Simulate circuit breaker pattern for failing services
    struct CircuitBreakerOrchestrator {
        service: Arc<MockAuthService>,
        failure_count: Arc<tokio::sync::Mutex<u32>>,
        circuit_open: Arc<tokio::sync::Mutex<bool>>,
        last_failure_time: Arc<tokio::sync::Mutex<Option<Instant>>>,
        failure_threshold: u32,
        recovery_time: Duration,
    }

    impl CircuitBreakerOrchestrator {
        fn new(service: Arc<MockAuthService>) -> Self {
            Self {
                service,
                failure_count: Arc::new(tokio::sync::Mutex::new(0)),
                circuit_open: Arc::new(tokio::sync::Mutex::new(false)),
                last_failure_time: Arc::new(tokio::sync::Mutex::new(None)),
                failure_threshold: 3,
                recovery_time: Duration::from_millis(100),
            }
        }

        async fn authenticate(&self, username: &str, password: &str) -> Result<auth_service::AuthContext> {
            // Check if circuit is open
            let mut circuit_open = self.circuit_open.lock().await;
            if *circuit_open {
                let last_failure = self.last_failure_time.lock().await;
                if let Some(last_time) = *last_failure {
                    if last_time.elapsed() > self.recovery_time {
                        // Try to close circuit
                        *circuit_open = false;
                        *self.failure_count.lock().await = 0;
                        drop(last_failure);
                        drop(circuit_open);
                    } else {
                        drop(last_failure);
                        drop(circuit_open);
                        return Err(anyhow!("Circuit breaker open - service unavailable"));
                    }
                } else {
                    drop(last_failure);
                    drop(circuit_open);
                    return Err(anyhow!("Circuit breaker open - service unavailable"));
                }
            } else {
                drop(circuit_open);
            }

            // Try the actual request
            let result = self.service.authenticate(username, password).await;
            
            match result {
                Ok(context) => {
                    // Reset failure count on success
                    *self.failure_count.lock().await = 0;
                    Ok(context)
                }
                Err(e) => {
                    // Increment failure count
                    let mut failure_count = self.failure_count.lock().await;
                    *failure_count += 1;
                    
                    if *failure_count >= self.failure_threshold {
                        // Open circuit
                        *self.circuit_open.lock().await = true;
                        *self.last_failure_time.lock().await = Some(Instant::now());
                    }
                    
                    Err(e)
                }
            }
        }

        async fn is_circuit_open(&self) -> bool {
            *self.circuit_open.lock().await
        }

        async fn get_failure_count(&self) -> u32 {
            *self.failure_count.lock().await
        }
    }

    let failing_service = Arc::new(MockAuthService::new());
    failing_service.set_should_fail(true).await;
    
    let circuit_breaker = CircuitBreakerOrchestrator::new(failing_service.clone());

    // First few requests should fail and open circuit
    for i in 1..=3 {
        let result = circuit_breaker.authenticate("user", "password").await;
        assert!(result.is_err(), "Request {} should fail", i);
    }

    // Circuit should now be open
    assert!(circuit_breaker.is_circuit_open().await);
    assert_eq!(circuit_breaker.get_failure_count().await, 3);

    // Subsequent requests should be immediately rejected
    let result = circuit_breaker.authenticate("user", "password").await;
    assert!(result.is_err());
    assert!(result.err().unwrap().to_string().contains("Circuit breaker open"));

    // Wait for recovery time
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Fix the service
    failing_service.set_should_fail(false).await;

    // Next request should work and close circuit
    let result = circuit_breaker.authenticate("user", "password").await;
    assert!(result.is_ok(), "Request should succeed after circuit recovery");
    
    assert!(!circuit_breaker.is_circuit_open().await);
    assert_eq!(circuit_breaker.get_failure_count().await, 0);
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_orchestrator_throughput() {
        let binance_service = Arc::new(MockAuthService::new());
        let zerodha_service = Arc::new(MockAuthService::new());

        // Setup many users
        for i in 0..1000 {
            let username = format!("perf_user_{}", i);
            binance_service.add_user(username.clone(), create_test_auth_context(&username)).await;
            zerodha_service.add_user(username, create_test_auth_context(&format!("zrd_{}", i))).await;
        }

        let orchestrator = Arc::new(
            MockAuthOrchestrator::new()
                .with_binance(binance_service)
                .with_zerodha(zerodha_service)
        );

        let start = Instant::now();
        let mut handles = Vec::new();
        let total_requests = 1000;

        // Spawn high-throughput requests
        for i in 0..total_requests {
            let orch = Arc::clone(&orchestrator);
            let handle = tokio::spawn(async move {
                let username = format!("perf_user_{}", i % 1000);
                let exchange = if i % 2 == 0 { "binance" } else { "zerodha" };
                
                let start = Instant::now();
                let result = orch.authenticate(&username, "password", exchange).await;
                let latency = start.elapsed();
                
                (result.is_ok(), latency)
            });
            handles.push(handle);
        }

        let results = futures::future::try_join_all(handles).await.unwrap();
        let total_time = start.elapsed();

        let successful = results.iter().filter(|(success, _)| *success).count();
        let failed = results.iter().filter(|(success, _)| !*success).count();
        let avg_latency: Duration = results.iter().map(|(_, latency)| *latency).sum::<Duration>() / results.len() as u32;
        let throughput = total_requests as f64 / total_time.as_secs_f64();

        println!("Orchestrator throughput test:");
        println!("  Total requests: {}", total_requests);
        println!("  Successful: {} ({:.1}%)", successful, (successful as f64 / total_requests as f64) * 100.0);
        println!("  Failed: {} ({:.1}%)", failed, (failed as f64 / total_requests as f64) * 100.0);
        println!("  Total time: {:?}", total_time);
        println!("  Average latency: {:?}", avg_latency);
        println!("  Throughput: {:.2} req/sec", throughput);

        // Performance assertions
        assert_eq!(successful, total_requests, "All requests should succeed");
        assert_eq!(failed, 0, "No requests should fail");
        assert!(throughput > 1000.0, "Throughput should exceed 1000 req/sec, got {:.2}", throughput);
        assert!(avg_latency < Duration::from_millis(10), "Average latency should be < 10ms");

        // Check final metrics
        let metrics = orchestrator.get_metrics().await;
        assert_eq!(metrics.total_requests, total_requests as u64);
        assert_eq!(metrics.successful_requests, total_requests as u64);
        assert!(metrics.average_latency_ms > 0.0);
        assert!(metrics.average_latency_ms < 10.0);
    }
}