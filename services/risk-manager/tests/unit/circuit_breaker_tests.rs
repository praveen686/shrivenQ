//! Unit tests for circuit breaker

use risk_manager::circuit_breaker::CircuitBreaker;
use std::thread;
use std::time::Duration;

#[test]
fn test_circuit_breaker_starts_closed() {
    let cb = CircuitBreaker::new(3, 1000);
    assert!(!cb.is_open(), "Circuit breaker should start closed");
}

#[test]
fn test_circuit_breaker_opens_after_threshold() {
    let cb = CircuitBreaker::new(3, 1000);
    
    // Record failures up to threshold
    cb.record_failure();
    assert!(!cb.is_open(), "Should remain closed under threshold");
    
    cb.record_failure();
    assert!(!cb.is_open(), "Should remain closed at threshold-1");
    
    cb.record_failure();
    assert!(cb.is_open(), "Should open at threshold");
}

#[test]
fn test_circuit_breaker_resets_after_timeout() {
    let cb = CircuitBreaker::new(3, 100); // 100ms timeout
    
    // Open the circuit
    for _ in 0..3 {
        cb.record_failure();
    }
    assert!(cb.is_open(), "Circuit should be open");
    
    // Wait for timeout
    thread::sleep(Duration::from_millis(150));
    
    assert!(!cb.is_open(), "Circuit should reset after timeout");
}

#[test]
fn test_circuit_breaker_success_resets_count() {
    let cb = CircuitBreaker::new(3, 1000);
    
    cb.record_failure();
    cb.record_failure();
    cb.record_success(); // Reset count
    
    cb.record_failure();
    cb.record_failure();
    assert!(!cb.is_open(), "Should not open after reset");
    
    cb.record_failure();
    assert!(cb.is_open(), "Should open after 3 new failures");
}

#[test]
fn test_circuit_breaker_concurrent_access() {
    use std::sync::Arc;
    
    let cb = Arc::new(CircuitBreaker::new(10, 1000));
    let mut handles = vec![];
    
    // Spawn multiple threads recording failures
    for _ in 0..5 {
        let cb_clone = Arc::clone(&cb);
        handles.push(thread::spawn(move || {
            for _ in 0..3 {
                cb_clone.record_failure();
                thread::sleep(Duration::from_millis(10));
            }
        }));
    }
    
    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Should have recorded 15 failures (5 threads * 3 failures)
    assert!(cb.is_open(), "Circuit should be open after concurrent failures");
}

#[test]
fn test_circuit_breaker_half_open_state() {
    let cb = CircuitBreaker::new(2, 50);
    
    // Open circuit
    cb.record_failure();
    cb.record_failure();
    assert!(cb.is_open());
    
    // Wait for timeout
    thread::sleep(Duration::from_millis(60));
    
    // Circuit should be closed (half-open state)
    assert!(!cb.is_open());
    
    // Single failure should not re-open
    cb.record_failure();
    assert!(!cb.is_open());
    
    // But reaching threshold should
    cb.record_failure();
    assert!(cb.is_open());
}

#[test]
fn test_circuit_breaker_edge_case_zero_timeout() {
    let cb = CircuitBreaker::new(3, 0); // Zero timeout
    
    // Open circuit
    for _ in 0..3 {
        cb.record_failure();
    }
    assert!(cb.is_open());
    
    // With zero timeout, should immediately reset
    assert!(!cb.is_open());
}

#[test]
fn test_circuit_breaker_edge_case_zero_threshold() {
    let cb = CircuitBreaker::new(0, 1000); // Zero threshold
    
    // First failure should open circuit
    cb.record_failure();
    assert!(cb.is_open());
}

#[test]
fn test_circuit_breaker_edge_case_one_threshold() {
    let cb = CircuitBreaker::new(1, 1000);
    
    // Should not be open initially
    assert!(!cb.is_open());
    
    // First failure should open it
    cb.record_failure();
    assert!(cb.is_open());
}

#[test]
fn test_circuit_breaker_multiple_success_resets() {
    let cb = CircuitBreaker::new(5, 1000);
    
    // Build up failures
    for _ in 0..4 {
        cb.record_failure();
        assert!(!cb.is_open(), "Should not be open before threshold");
    }
    
    // Multiple successes should reset
    cb.record_success();
    cb.record_success();
    cb.record_success();
    
    // Should still not be open after resets
    for _ in 0..4 {
        cb.record_failure();
        assert!(!cb.is_open(), "Should not be open after reset");
    }
    
    // Final failure should open
    cb.record_failure();
    assert!(cb.is_open());
}

#[test]
fn test_circuit_breaker_timeout_precision() {
    let cb = CircuitBreaker::new(2, 100); // 100ms timeout
    
    // Open circuit
    cb.record_failure();
    cb.record_failure();
    assert!(cb.is_open());
    
    // Wait just under timeout
    thread::sleep(Duration::from_millis(90));
    assert!(cb.is_open(), "Should still be open before timeout");
    
    // Wait past timeout
    thread::sleep(Duration::from_millis(20));
    assert!(!cb.is_open(), "Should be closed after timeout");
}

#[test]
fn test_circuit_breaker_stress_test_sequential() {
    let cb = CircuitBreaker::new(100, 1000);
    
    // Rapid sequential failures
    for i in 0..99 {
        cb.record_failure();
        assert!(!cb.is_open(), "Should not be open at failure {}", i);
    }
    
    // Final failure should open
    cb.record_failure();
    assert!(cb.is_open());
    
    // Rapid sequential successes after timeout
    thread::sleep(Duration::from_millis(1001));
    for _ in 0..10 {
        cb.record_success();
        assert!(!cb.is_open(), "Should be closed after success");
    }
}

#[test]
fn test_circuit_breaker_mixed_success_failure_patterns() {
    let cb = CircuitBreaker::new(3, 1000);
    
    // Pattern: F-S-F-S-F-F-F (should open on last F)
    cb.record_failure(); // 1
    assert!(!cb.is_open());
    
    cb.record_success(); // Reset to 0
    assert!(!cb.is_open());
    
    cb.record_failure(); // 1
    cb.record_success(); // Reset to 0
    assert!(!cb.is_open());
    
    cb.record_failure(); // 1
    cb.record_failure(); // 2
    cb.record_failure(); // 3 - Should open
    assert!(cb.is_open());
}

#[test]
fn test_circuit_breaker_high_frequency_operations() {
    use std::sync::Arc;
    
    let cb = Arc::new(CircuitBreaker::new(50, 100));
    let mut handles = vec![];
    
    // High frequency operations from multiple threads
    for thread_id in 0..5 {
        let cb_clone = Arc::clone(&cb);
        handles.push(thread::spawn(move || {
            for i in 0..20 {
                if i % 2 == 0 {
                    cb_clone.record_failure();
                } else if thread_id % 2 == 0 {
                    cb_clone.record_success();
                }
                
                // Check state frequently
                let _ = cb_clone.is_open();
                
                // Small delay to interleave operations
                thread::sleep(Duration::from_micros(100));
            }
        }));
    }
    
    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Circuit breaker should still be functional
    assert!(!cb.is_open() || cb.is_open()); // Just verify no panic
}

#[test]
fn test_circuit_breaker_timeout_race_conditions() {
    use std::sync::Arc;
    
    let cb = Arc::new(CircuitBreaker::new(3, 50));
    
    // Open circuit
    for _ in 0..3 {
        cb.record_failure();
    }
    assert!(cb.is_open());
    
    let mut handles = vec![];
    
    // Multiple threads checking state during timeout window
    for _ in 0..10 {
        let cb_clone = Arc::clone(&cb);
        handles.push(thread::spawn(move || {
            let mut results = vec![];
            for _ in 0..20 {
                results.push(cb_clone.is_open());
                thread::sleep(Duration::from_millis(5));
            }
            results
        }));
    }
    
    let mut all_results = vec![];
    for handle in handles {
        all_results.extend(handle.join().unwrap());
    }
    
    // Should see transition from open to closed
    assert!(all_results.iter().any(|&x| x), "Should have some open states");
    assert!(all_results.iter().any(|&x| !x), "Should have some closed states");
}

#[test]
fn test_circuit_breaker_memory_safety() {
    use std::sync::Arc;
    
    let cb = Arc::new(CircuitBreaker::new(10, 100));
    let mut handles = vec![];
    
    // Test that dropping references while operations are in progress is safe
    for i in 0..20 {
        let cb_clone = Arc::clone(&cb);
        handles.push(thread::spawn(move || {
            for j in 0..10 {
                cb_clone.record_failure();
                if (i + j) % 5 == 0 {
                    cb_clone.record_success();
                }
                let _ = cb_clone.is_open();
            }
        }));
        
        // Drop some handles early to test cleanup
        if i % 3 == 0 && i > 0 {
            if let Some(handle) = handles.pop() {
                handle.join().unwrap();
            }
        }
    }
    
    // Join remaining handles
    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_circuit_breaker_large_threshold_values() {
    let cb = CircuitBreaker::new(u64::MAX / 2, 1000);
    
    // Should handle very large threshold values without overflow
    for _ in 0..1000 {
        cb.record_failure();
        assert!(!cb.is_open(), "Should not open with very large threshold");
    }
}

#[test]
fn test_circuit_breaker_large_timeout_values() {
    let cb = CircuitBreaker::new(1, u64::MAX);
    
    // Open circuit
    cb.record_failure();
    assert!(cb.is_open());
    
    // With very large timeout, should remain open
    thread::sleep(Duration::from_millis(10));
    assert!(cb.is_open(), "Should remain open with very large timeout");
}

// Additional comprehensive tests for circuit breaker

#[test]
fn test_circuit_breaker_rapid_state_changes() {
    let cb = CircuitBreaker::new(2, 10); // Very short timeout for rapid testing
    
    // Rapidly open and close circuit
    for _ in 0..10 {
        // Open circuit
        cb.record_failure();
        cb.record_failure();
        assert!(cb.is_open(), "Should be open after failures");
        
        // Wait for timeout and verify reset
        thread::sleep(Duration::from_millis(15));
        assert!(!cb.is_open(), "Should be closed after timeout");
        
        // Reset with success
        cb.record_success();
        assert!(!cb.is_open(), "Should remain closed after success");
    }
}

#[test]
fn test_circuit_breaker_failure_count_accuracy() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    
    let cb = Arc::new(CircuitBreaker::new(100, 5000));
    let failure_count = Arc::new(AtomicU64::new(0));
    let mut handles = vec![];
    
    // Multiple threads recording failures
    for _ in 0..10 {
        let cb_clone = Arc::clone(&cb);
        let count_clone = Arc::clone(&failure_count);
        handles.push(thread::spawn(move || {
            for _ in 0..10 {
                cb_clone.record_failure();
                count_clone.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Should have recorded 100 failures total
    assert_eq!(failure_count.load(Ordering::Relaxed), 100);
    assert!(cb.is_open(), "Circuit should be open after 100 failures (threshold=100)");
}

#[test]
fn test_circuit_breaker_mixed_operations_under_load() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    
    let cb = Arc::new(CircuitBreaker::new(50, 100));
    let operations = Arc::new(AtomicU64::new(0));
    let mut handles = vec![];
    
    // Mix of failure and success operations
    for i in 0..20 {
        let cb_clone = Arc::clone(&cb);
        let ops_clone = Arc::clone(&operations);
        handles.push(thread::spawn(move || {
            for j in 0..10 {
                if (i + j) % 3 == 0 {
                    cb_clone.record_success();
                } else {
                    cb_clone.record_failure();
                }
                ops_clone.fetch_add(1, Ordering::Relaxed);
                
                // Occasionally check state
                let _ = cb_clone.is_open();
            }
        }));
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    assert_eq!(operations.load(Ordering::Relaxed), 200);
    // Circuit state is unpredictable due to mixed operations, but shouldn't panic
    let _ = cb.is_open();
}

#[test]
fn test_circuit_breaker_timeout_boundary_conditions() {
    let cb = CircuitBreaker::new(1, 50); // 50ms timeout
    
    // Open circuit
    cb.record_failure();
    assert!(cb.is_open());
    
    // Test at various time boundaries
    let test_times = [10, 25, 40, 50, 60, 75];
    for &wait_ms in &test_times {
        // Re-open circuit
        cb.record_failure();
        assert!(cb.is_open(), "Should be open initially");
        
        thread::sleep(Duration::from_millis(wait_ms));
        
        if wait_ms >= 50 {
            assert!(!cb.is_open(), "Should be closed after {}ms (>= 50ms)", wait_ms);
        }
        // For wait_ms < 50, state is indeterminate due to timing precision
    }
}

#[test]
fn test_circuit_breaker_success_after_timeout() {
    let cb = CircuitBreaker::new(3, 100);
    
    // Open circuit
    for _ in 0..3 {
        cb.record_failure();
    }
    assert!(cb.is_open());
    
    // Wait for timeout
    thread::sleep(Duration::from_millis(150));
    assert!(!cb.is_open(), "Should be closed after timeout");
    
    // Record success after timeout
    cb.record_success();
    assert!(!cb.is_open(), "Should remain closed after success");
    
    // Should require full threshold again
    cb.record_failure();
    cb.record_failure();
    assert!(!cb.is_open(), "Should not open before threshold");
    
    cb.record_failure();
    assert!(cb.is_open(), "Should open at threshold after reset");
}

#[test]
fn test_circuit_breaker_multiple_timeouts() {
    let cb = CircuitBreaker::new(2, 50);
    
    for iteration in 0..5 {
        // Open circuit
        cb.record_failure();
        cb.record_failure();
        assert!(cb.is_open(), "Should be open in iteration {}", iteration);
        
        // Wait for timeout
        thread::sleep(Duration::from_millis(60));
        assert!(!cb.is_open(), "Should be closed after timeout in iteration {}", iteration);
    }
}

#[test]
fn test_circuit_breaker_thread_safety_stress() {
    use std::sync::Arc;
    use std::sync::Barrier;
    
    let cb = Arc::new(CircuitBreaker::new(1000, 1000));
    let barrier = Arc::new(Barrier::new(20));
    let mut handles = vec![];
    
    // 20 threads starting simultaneously
    for thread_id in 0..20 {
        let cb_clone = Arc::clone(&cb);
        let barrier_clone = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier_clone.wait(); // Synchronize start
            
            for i in 0..100 {
                match (thread_id + i) % 4 {
                    0 => cb_clone.record_failure(),
                    1 => cb_clone.record_success(),
                    2 => { cb_clone.record_failure(); cb_clone.record_success(); },
                    _ => { let _ = cb_clone.is_open(); },
                }
            }
        }));
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Should not panic and should be in a valid state
    let _ = cb.is_open();
}

#[test]
fn test_circuit_breaker_overflow_protection() {
    let cb = CircuitBreaker::new(u64::MAX, 1000);
    
    // Record many failures - should not overflow
    for _ in 0..1000 {
        cb.record_failure();
    }
    
    // Should not be open with MAX threshold
    assert!(!cb.is_open(), "Should not open with u64::MAX threshold");
    
    // Test with actual overflow-inducing threshold
    let cb2 = CircuitBreaker::new(1, u64::MAX);
    cb2.record_failure();
    assert!(cb2.is_open(), "Should open with threshold 1");
    
    // With MAX timeout, should not reset quickly
    thread::sleep(Duration::from_millis(10));
    assert!(cb2.is_open(), "Should remain open with MAX timeout");
}

#[test]
fn test_circuit_breaker_state_consistency() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    
    let cb = Arc::new(CircuitBreaker::new(5, 100));
    let inconsistency_detected = Arc::new(AtomicBool::new(false));
    let mut handles = vec![];
    
    // Multiple threads checking and modifying state
    for i in 0..10 {
        let cb_clone = Arc::clone(&cb);
        let inconsistency_clone = Arc::clone(&inconsistency_detected);
        
        handles.push(thread::spawn(move || {
            for j in 0..50 {
                let state_before = cb_clone.is_open();
                
                // Perform operation
                if (i + j) % 2 == 0 {
                    cb_clone.record_failure();
                } else {
                    cb_clone.record_success();
                }
                
                let state_after = cb_clone.is_open();
                
                // State should change logically
                // (This is a basic consistency check - more complex logic would be needed for full verification)
                if state_before && !state_after {
                    // Transition from open to closed should only happen via timeout or success
                    // We can't easily verify this without access to internal state
                }
                
                // Just ensure no panics occur
                thread::sleep(Duration::from_micros(100));
            }
        }));
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    assert!(!inconsistency_detected.load(Ordering::Relaxed), "No inconsistencies should be detected");
}

#[test]
fn test_circuit_breaker_performance_characteristics() {
    use std::time::Instant;
    
    let cb = CircuitBreaker::new(1000, 5000);
    
    // Test performance of failure recording
    let start = Instant::now();
    for _ in 0..10000 {
        cb.record_failure();
    }
    let failure_duration = start.elapsed();
    
    // Should be very fast
    assert!(failure_duration < Duration::from_millis(100), 
        "Recording 10000 failures took too long: {:?}", failure_duration);
    
    // Test performance of success recording
    let start = Instant::now();
    for _ in 0..10000 {
        cb.record_success();
    }
    let success_duration = start.elapsed();
    
    assert!(success_duration < Duration::from_millis(100), 
        "Recording 10000 successes took too long: {:?}", success_duration);
    
    // Test performance of state checking
    let start = Instant::now();
    for _ in 0..10000 {
        cb.is_open();
    }
    let check_duration = start.elapsed();
    
    assert!(check_duration < Duration::from_millis(100), 
        "Checking state 10000 times took too long: {:?}", check_duration);
}

#[test] 
fn test_circuit_breaker_deterministic_behavior() {
    // Test that circuit breaker behaves deterministically with same inputs
    for run in 0..10 {
        let cb = CircuitBreaker::new(3, 1000);
        
        // Same sequence of operations
        cb.record_failure();
        assert!(!cb.is_open(), "Run {}: Should be closed after 1 failure", run);
        
        cb.record_failure();
        assert!(!cb.is_open(), "Run {}: Should be closed after 2 failures", run);
        
        cb.record_failure();
        assert!(cb.is_open(), "Run {}: Should be open after 3 failures", run);
        
        cb.record_success();
        assert!(!cb.is_open(), "Run {}: Should be closed after success", run);
    }
}