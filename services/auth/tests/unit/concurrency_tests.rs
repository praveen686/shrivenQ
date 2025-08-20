//! Unit tests for concurrent request handling and thread safety

use super::test_utils::*;
use auth_service::{AuthConfig, AuthService, AuthServiceImpl, Permission};
use rustc_hash::FxHashMap;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::task;
use tokio::sync::{RwLock, Semaphore};

/// Thread-safe statistics collector for concurrency tests
#[derive(Debug, Default)]
struct ConcurrencyStats {
    operations_completed: Arc<Mutex<u64>>,
    successful_operations: Arc<Mutex<u64>>,
    failed_operations: Arc<Mutex<u64>>,
    total_duration: Arc<Mutex<Duration>>,
    operation_times: Arc<Mutex<Vec<Duration>>>,
}

impl ConcurrencyStats {
    fn new() -> Self {
        Self::default()
    }

    fn record_success(&self, duration: Duration) {
        let mut ops = self.operations_completed.lock().unwrap();
        let mut success = self.successful_operations.lock().unwrap();
        let mut total = self.total_duration.lock().unwrap();
        let mut times = self.operation_times.lock().unwrap();

        *ops += 1;
        *success += 1;
        *total += duration;
        times.push(duration);
    }

    fn record_failure(&self, duration: Duration) {
        let mut ops = self.operations_completed.lock().unwrap();
        let mut failed = self.failed_operations.lock().unwrap();
        let mut total = self.total_duration.lock().unwrap();

        *ops += 1;
        *failed += 1;
        *total += duration;
    }

    fn get_stats(&self) -> (u64, u64, u64, Duration, Option<Duration>) {
        let ops = *self.operations_completed.lock().unwrap();
        let success = *self.successful_operations.lock().unwrap();
        let failed = *self.failed_operations.lock().unwrap();
        let total = *self.total_duration.lock().unwrap();
        
        let avg_duration = if ops > 0 {
            Some(total / ops as u32)
        } else {
            None
        };

        (ops, success, failed, total, avg_duration)
    }

    fn get_percentiles(&self) -> (Option<Duration>, Option<Duration>, Option<Duration>) {
        let times = self.operation_times.lock().unwrap();
        if times.is_empty() {
            return (None, None, None);
        }

        let mut sorted_times = times.clone();
        sorted_times.sort();

        let len = sorted_times.len();
        let p50 = sorted_times[len / 2];
        let p95 = sorted_times[(len * 95) / 100];
        let p99 = sorted_times[(len * 99) / 100];

        (Some(p50), Some(p95), Some(p99))
    }
}

#[tokio::test]
async fn test_concurrent_authentication_requests() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 1000);
    
    let config = AuthConfig {
        jwt_secret: "concurrent_auth_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = Arc::new(AuthServiceImpl::new(config));
    let stats = Arc::new(ConcurrencyStats::new());
    let mut handles = Vec::new();
    
    let concurrent_users = 100;
    
    // Spawn concurrent authentication requests
    for i in 0..concurrent_users {
        let service = Arc::clone(&auth_service);
        let stats_clone = Arc::clone(&stats);
        
        let handle = task::spawn(async move {
            let username = format!("concurrent_user_{}", i);
            let start = Instant::now();
            
            match service.authenticate(&username, "password").await {
                Ok(context) => {
                    let duration = start.elapsed();
                    stats_clone.record_success(duration);
                    assert_eq!(context.user_id, username);
                    Ok(context)
                }
                Err(e) => {
                    let duration = start.elapsed();
                    stats_clone.record_failure(duration);
                    Err(e)
                }
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all authentication requests to complete
    let results = futures::future::join_all(handles).await;
    
    // Analyze results
    let successful_auths = results.iter().filter(|r| r.as_ref().unwrap().is_ok()).count();
    let failed_auths = results.iter().filter(|r| r.as_ref().unwrap().is_err()).count();
    
    let (ops, success, failed, total_time, avg_time) = stats.get_stats();
    let (p50, p95, p99) = stats.get_percentiles();
    
    println!("Concurrent Authentication Stats:");
    println!("  Total operations: {}", ops);
    println!("  Successful: {}", success);
    println!("  Failed: {}", failed);
    println!("  Total time: {:?}", total_time);
    println!("  Average time: {:?}", avg_time.unwrap_or(Duration::ZERO));
    println!("  P50: {:?}", p50.unwrap_or(Duration::ZERO));
    println!("  P95: {:?}", p95.unwrap_or(Duration::ZERO));
    println!("  P99: {:?}", p99.unwrap_or(Duration::ZERO));
    
    // Assertions
    assert_eq!(successful_auths, concurrent_users);
    assert_eq!(failed_auths, 0);
    assert_eq!(ops, concurrent_users as u64);
    assert_eq!(success, concurrent_users as u64);
    assert_eq!(failed, 0);
    
    // Performance assertions
    assert!(avg_time.unwrap() < Duration::from_millis(100));
}

#[tokio::test]
async fn test_concurrent_token_operations() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 1000);
    
    let config = AuthConfig {
        jwt_secret: "concurrent_token_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = Arc::new(AuthServiceImpl::new(config));
    let num_operations = 50;
    
    // Pre-authenticate users and generate tokens
    let mut initial_tokens = Vec::new();
    for i in 0..num_operations {
        let username = format!("token_user_{}", i);
        let context = auth_service.authenticate(&username, "password").await.unwrap();
        let token = auth_service.generate_token(&context).await.unwrap();
        initial_tokens.push((token, context));
    }
    
    let mut handles = Vec::new();
    
    // Spawn concurrent token operations (validation, generation, revocation)
    for (i, (token, context)) in initial_tokens.into_iter().enumerate() {
        let service = Arc::clone(&auth_service);
        
        let handle = task::spawn(async move {
            let mut results = Vec::new();
            
            // Validate existing token
            let validation_result = service.validate_token(&token).await;
            results.push(("validate", validation_result.is_ok()));
            
            // Generate new token
            let new_token_result = service.generate_token(&context).await;
            results.push(("generate", new_token_result.is_ok()));
            
            // Revoke original token
            let revoke_result = service.revoke_token(&token).await;
            results.push(("revoke", revoke_result.is_ok()));
            
            // If generation succeeded, validate the new token
            if let Ok(new_token) = new_token_result {
                let new_validation = service.validate_token(&new_token).await;
                results.push(("validate_new", new_validation.is_ok()));
            }
            
            (i, results)
        });
        
        handles.push(handle);
    }
    
    // Wait for all operations to complete
    let results = futures::future::try_join_all(handles).await.unwrap();
    
    // Analyze results
    let mut operation_stats: HashMap<&str, (u32, u32)> = HashMap::new();
    
    for (_, ops) in results {
        for (op_type, success) in ops {
            let entry = operation_stats.entry(op_type).or_insert((0, 0));
            if success {
                entry.0 += 1; // success count
            } else {
                entry.1 += 1; // failure count
            }
        }
    }
    
    println!("Concurrent Token Operations Stats:");
    for (op_type, (success, failed)) in operation_stats {
        println!("  {}: {} success, {} failed", op_type, success, failed);
        assert!(success > 0, "At least some {} operations should succeed", op_type);
    }
}

#[tokio::test]
async fn test_high_concurrency_stress() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 10000);
    
    let config = AuthConfig {
        jwt_secret: "stress_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = Arc::new(AuthServiceImpl::new(config));
    let stats = Arc::new(ConcurrencyStats::new());
    
    let high_concurrency = 500;
    let semaphore = Arc::new(Semaphore::new(100)); // Limit to 100 concurrent operations
    
    let mut handles = Vec::new();
    let start_time = Instant::now();
    
    // Spawn high-concurrency stress test
    for i in 0..high_concurrency {
        let service = Arc::clone(&auth_service);
        let stats_clone = Arc::clone(&stats);
        let sem = Arc::clone(&semaphore);
        
        let handle = task::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            
            let username = format!("stress_user_{}", i);
            let op_start = Instant::now();
            
            // Perform multiple operations per task
            let auth_result = service.authenticate(&username, "password").await;
            if let Ok(context) = auth_result {
                let token_result = service.generate_token(&context).await;
                if let Ok(token) = token_result {
                    let validate_result = service.validate_token(&token).await;
                    let revoke_result = service.revoke_token(&token).await;
                    
                    if validate_result.is_ok() && revoke_result.is_ok() {
                        let duration = op_start.elapsed();
                        stats_clone.record_success(duration);
                        return Ok(());
                    }
                }
            }
            
            let duration = op_start.elapsed();
            stats_clone.record_failure(duration);
            Err(anyhow::anyhow!("Operation failed"))
        });
        
        handles.push(handle);
    }
    
    // Wait for all stress operations to complete
    let results = futures::future::join_all(handles).await;
    let total_time = start_time.elapsed();
    
    let successful_ops = results.iter().filter(|r| r.as_ref().unwrap().is_ok()).count();
    let failed_ops = results.iter().filter(|r| r.as_ref().unwrap().is_err()).count();
    
    let (ops, success, failed, _, avg_time) = stats.get_stats();
    let (p50, p95, p99) = stats.get_percentiles();
    
    println!("High Concurrency Stress Test Stats:");
    println!("  Total operations: {}", ops);
    println!("  Successful: {} ({}%)", success, (success * 100) / ops);
    println!("  Failed: {} ({}%)", failed, (failed * 100) / ops);
    println!("  Total wall time: {:?}", total_time);
    println!("  Average operation time: {:?}", avg_time.unwrap_or(Duration::ZERO));
    println!("  Throughput: {:.2} ops/sec", ops as f64 / total_time.as_secs_f64());
    println!("  P50: {:?}", p50.unwrap_or(Duration::ZERO));
    println!("  P95: {:?}", p95.unwrap_or(Duration::ZERO));
    println!("  P99: {:?}", p99.unwrap_or(Duration::ZERO));
    
    // Assertions for stress test
    assert_eq!(successful_ops, high_concurrency);
    assert_eq!(failed_ops, 0);
    assert!(success > 0);
    
    // Performance assertions
    let throughput = ops as f64 / total_time.as_secs_f64();
    assert!(throughput > 100.0, "Throughput should be > 100 ops/sec, got {:.2}", throughput);
    assert!(p99.unwrap() < Duration::from_millis(500), "P99 latency should be < 500ms");
}

#[tokio::test]
async fn test_concurrent_mixed_operations() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 1000);
    
    let config = AuthConfig {
        jwt_secret: "mixed_ops_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = Arc::new(AuthServiceImpl::new(config));
    let operation_counts = Arc::new(RwLock::new(HashMap::<String, u32>::new()));
    
    let num_workers = 20;
    let operations_per_worker = 10;
    
    let mut handles = Vec::new();
    
    // Spawn workers performing mixed operations
    for worker_id in 0..num_workers {
        let service = Arc::clone(&auth_service);
        let counts = Arc::clone(&operation_counts);
        
        let handle = task::spawn(async move {
            let mut local_results = Vec::new();
            
            for op_id in 0..operations_per_worker {
                let username = format!("mixed_user_{}_{}", worker_id, op_id);
                
                // Authenticate user
                let auth_result = service.authenticate(&username, "password").await;
                local_results.push(("auth", auth_result.is_ok()));
                
                if let Ok(context) = auth_result {
                    // Generate token
                    let token_result = service.generate_token(&context).await;
                    local_results.push(("generate", token_result.is_ok()));
                    
                    if let Ok(token) = token_result {
                        // Validate token
                        let validate_result = service.validate_token(&token).await;
                        local_results.push(("validate", validate_result.is_ok()));
                        
                        // Check permissions
                        if let Ok(validated_context) = validate_result {
                            let read_perm = service.check_permission(&validated_context, Permission::ReadMarketData).await;
                            let trade_perm = service.check_permission(&validated_context, Permission::PlaceOrders).await;
                            local_results.push(("check_read_perm", read_perm));
                            local_results.push(("check_trade_perm", trade_perm));
                        }
                        
                        // Revoke token
                        let revoke_result = service.revoke_token(&token).await;
                        local_results.push(("revoke", revoke_result.is_ok()));
                    }
                }
            }
            
            // Update shared counts
            let mut counts_guard = counts.write().await;
            for (op_type, success) in local_results {
                let key = format!("{}_{}", op_type, if success { "success" } else { "failure" });
                *counts_guard.entry(key).or_insert(0) += 1;
            }
            
            worker_id
        });
        
        handles.push(handle);
    }
    
    // Wait for all workers to complete
    let worker_ids = futures::future::try_join_all(handles).await.unwrap();
    
    // Analyze mixed operation results
    let final_counts = operation_counts.read().await;
    
    println!("Mixed Operations Concurrency Test Results:");
    for (operation, count) in final_counts.iter() {
        println!("  {}: {}", operation, count);
    }
    
    // Assertions
    assert_eq!(worker_ids.len(), num_workers);
    
    // Check that we have expected number of successful operations
    let expected_auths = num_workers * operations_per_worker;
    let successful_auths = final_counts.get("auth_success").unwrap_or(&0);
    assert_eq!(*successful_auths, expected_auths as u32);
    
    // Permissions should always succeed for authenticated users
    let read_perms = final_counts.get("check_read_perm_true").unwrap_or(&0);
    let trade_perms = final_counts.get("check_trade_perm_true").unwrap_or(&0);
    assert_eq!(*read_perms, expected_auths as u32);
    assert_eq!(*trade_perms, expected_auths as u32);
}

#[tokio::test]
async fn test_deadlock_prevention() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 1000);
    
    let config = AuthConfig {
        jwt_secret: "deadlock_test_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = Arc::new(AuthServiceImpl::new(config));
    
    // Create a scenario that could potentially cause deadlocks
    // by having circular dependencies in operations
    let mut handles = Vec::new();
    let shared_tokens = Arc::new(RwLock::new(Vec::<String>::new()));
    
    // First phase: Generate tokens concurrently
    for i in 0..10 {
        let service = Arc::clone(&auth_service);
        let tokens = Arc::clone(&shared_tokens);
        
        let handle = task::spawn(async move {
            let username = format!("deadlock_user_{}", i);
            let context = service.authenticate(&username, "password").await.unwrap();
            let token = service.generate_token(&context).await.unwrap();
            
            // Add token to shared collection
            tokens.write().await.push(token);
        });
        
        handles.push(handle);
    }
    
    // Wait for token generation
    futures::future::try_join_all(handles).await.unwrap();
    
    // Second phase: Cross-validate tokens concurrently
    let mut handles = Vec::new();
    
    for i in 0..10 {
        let service = Arc::clone(&auth_service);
        let tokens = Arc::clone(&shared_tokens);
        
        let handle = task::spawn(async move {
            let tokens_guard = tokens.read().await;
            let mut validation_results = Vec::new();
            
            // Each worker validates all tokens (potential for contention)
            for (j, token) in tokens_guard.iter().enumerate() {
                if i != j { // Don't validate own token
                    let result = service.validate_token(token).await;
                    validation_results.push(result.is_ok());
                }
            }
            
            validation_results
        });
        
        handles.push(handle);
    }
    
    // This should complete without deadlocking
    let validation_results = futures::future::try_join_all(handles).await.unwrap();
    
    // All validations should succeed
    for results in validation_results {
        for is_valid in results {
            assert!(is_valid, "Token validation should succeed");
        }
    }
    
    println!("Deadlock prevention test completed successfully");
}

#[tokio::test]
async fn test_memory_consistency_under_concurrency() {
    let mut rate_limits = FxHashMap::default();
    rate_limits.insert("default".to_string(), 1000);
    
    let config = AuthConfig {
        jwt_secret: "memory_consistency_secret".to_string(),
        token_expiry: 3600,
        rate_limits,
    };
    
    let auth_service = Arc::new(AuthServiceImpl::new(config));
    let shared_context = Arc::new(RwLock::new(None::<auth_service::AuthContext>));
    let shared_token = Arc::new(RwLock::new(None::<String>));
    
    // Writer task: Continuously updates context and token
    let writer_service = Arc::clone(&auth_service);
    let writer_context = Arc::clone(&shared_context);
    let writer_token = Arc::clone(&shared_token);
    
    let writer_handle = task::spawn(async move {
        for i in 0..100 {
            let username = format!("consistency_user_{}", i);
            let context = writer_service.authenticate(&username, "password").await.unwrap();
            let token = writer_service.generate_token(&context).await.unwrap();
            
            // Update shared state atomically
            {
                let mut context_guard = writer_context.write().await;
                let mut token_guard = writer_token.write().await;
                *context_guard = Some(context);
                *token_guard = Some(token);
            }
            
            // Small delay to allow readers to observe state
            tokio::time::sleep(Duration::from_micros(100)).await;
        }
    });
    
    // Reader tasks: Continuously validate consistency
    let mut reader_handles = Vec::new();
    
    for reader_id in 0..5 {
        let reader_service = Arc::clone(&auth_service);
        let reader_context = Arc::clone(&shared_context);
        let reader_token = Arc::clone(&shared_token);
        
        let handle = task::spawn(async move {
            let mut consistency_checks = 0;
            let mut consistency_failures = 0;
            
            for _ in 0..200 {
                // Read shared state atomically
                let (context_opt, token_opt) = {
                    let context_guard = reader_context.read().await;
                    let token_guard = reader_token.read().await;
                    (context_guard.clone(), token_guard.clone())
                };
                
                if let (Some(context), Some(token)) = (context_opt, token_opt) {
                    consistency_checks += 1;
                    
                    // Validate that token corresponds to context
                    if let Ok(validated_context) = reader_service.validate_token(&token).await {
                        if validated_context.user_id != context.user_id {
                            consistency_failures += 1;
                        }
                    } else {
                        consistency_failures += 1;
                    }
                }
                
                tokio::time::sleep(Duration::from_micros(50)).await;
            }
            
            (reader_id, consistency_checks, consistency_failures)
        });
        
        reader_handles.push(handle);
    }
    
    // Wait for all tasks to complete
    writer_handle.await.unwrap();
    let reader_results = futures::future::try_join_all(reader_handles).await.unwrap();
    
    // Analyze consistency results
    let mut total_checks = 0;
    let mut total_failures = 0;
    
    for (reader_id, checks, failures) in reader_results {
        total_checks += checks;
        total_failures += failures;
        println!("Reader {}: {} checks, {} failures", reader_id, checks, failures);
        
        // Each reader should have performed many consistency checks
        assert!(checks > 50, "Reader should perform many consistency checks");
    }
    
    println!("Memory consistency test: {} total checks, {} failures", total_checks, total_failures);
    
    // Memory consistency should be maintained
    assert_eq!(total_failures, 0, "No consistency failures should occur");
    assert!(total_checks > 250, "Should perform many consistency checks across all readers");
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_concurrency_scalability() {
        let mut rate_limits = FxHashMap::default();
        rate_limits.insert("default".to_string(), 10000);
        
        let config = AuthConfig {
            jwt_secret: "scalability_test_secret".to_string(),
            token_expiry: 3600,
            rate_limits,
        };
        
        let auth_service = Arc::new(AuthServiceImpl::new(config));
        
        // Test with increasing levels of concurrency
        let concurrency_levels = vec![10, 50, 100, 200];
        let operations_per_level = 100;
        
        for concurrency in concurrency_levels {
            let start = Instant::now();
            let mut handles = Vec::new();
            
            for i in 0..concurrency {
                let service = Arc::clone(&auth_service);
                
                let handle = task::spawn(async move {
                    let mut successful_ops = 0;
                    
                    for j in 0..operations_per_level / concurrency {
                        let username = format!("scale_user_{}_{}", i, j);
                        if let Ok(context) = service.authenticate(&username, "password").await {
                            if let Ok(token) = service.generate_token(&context).await {
                                if service.validate_token(&token).await.is_ok() {
                                    successful_ops += 1;
                                }
                            }
                        }
                    }
                    
                    successful_ops
                });
                
                handles.push(handle);
            }
            
            let results = futures::future::try_join_all(handles).await.unwrap();
            let duration = start.elapsed();
            
            let total_successful: u32 = results.iter().sum();
            let throughput = total_successful as f64 / duration.as_secs_f64();
            
            println!("Concurrency {}: {} ops in {:?} ({:.2} ops/sec)", 
                     concurrency, total_successful, duration, throughput);
            
            // Assertions
            assert!(total_successful > 0);
            assert!(throughput > 50.0); // Should maintain reasonable throughput
        }
    }
}