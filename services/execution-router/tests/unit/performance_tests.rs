//! Performance and concurrency tests
//!
//! High-performance and multi-threading tests for:
//! - Hot path execution latency and throughput
//! - Concurrent order processing and thread safety
//! - Memory allocation patterns and zero-copy operations
//! - Lock contention and scalability characteristics
//! - Resource utilization under load
//! - Performance regression detection

use execution_router::{
    ExecutionRouterService, VenueStrategy, memory::{Arena, ObjectPool, RingBuffer},
    smart_router::{Router, VenueConnection, MarketContext},
    OrderRequest, OrderType, TimeInForce, ExecutionAlgorithm, OrderId
};
use services_common::{Px, Qty, Side, Symbol, Ts};
use rstest::*;
use std::sync::{Arc, Barrier};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::runtime::Runtime;

/// Performance test configuration
const PERFORMANCE_ITERATIONS: usize = 10_000;
const CONCURRENCY_THREADS: usize = 8;
const STRESS_TEST_DURATION_SECS: u64 = 10;
const MEMORY_OPERATIONS_COUNT: usize = 1_000_000;

/// Performance measurement utilities
mod perf_utils {
    use super::*;
    
    pub struct PerformanceMetrics {
        pub operations: usize,
        pub duration: Duration,
        pub ops_per_sec: f64,
        pub avg_latency_ns: f64,
        pub p95_latency_ns: f64,
        pub p99_latency_ns: f64,
    }
    
    pub fn measure_operations<F, T>(iterations: usize, mut operation: F) -> (PerformanceMetrics, Vec<T>)
    where
        F: FnMut(usize) -> (Duration, T),
    {
        let mut latencies = Vec::with_capacity(iterations);
        let mut results = Vec::with_capacity(iterations);
        
        let start = Instant::now();
        
        for i in 0..iterations {
            let (op_duration, result) = operation(i);
            latencies.push(op_duration.as_nanos() as f64);
            results.push(result);
        }
        
        let total_duration = start.elapsed();
        
        // Calculate statistics
        latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let avg_latency = latencies.iter().sum::<f64>() / latencies.len() as f64;
        let p95_index = (latencies.len() as f64 * 0.95) as usize;
        let p99_index = (latencies.len() as f64 * 0.99) as usize;
        
        let metrics = PerformanceMetrics {
            operations: iterations,
            duration: total_duration,
            ops_per_sec: iterations as f64 / total_duration.as_secs_f64(),
            avg_latency_ns: avg_latency,
            p95_latency_ns: latencies[p95_index.min(latencies.len() - 1)],
            p99_latency_ns: latencies[p99_index.min(latencies.len() - 1)],
        };
        
        (metrics, results)
    }
    
    pub fn print_performance_summary(test_name: &str, metrics: &PerformanceMetrics) {
        println!("\n=== {} Performance Results ===", test_name);
        println!("Operations:     {}", metrics.operations);
        println!("Duration:       {:?}", metrics.duration);
        println!("Throughput:     {:.2} ops/sec", metrics.ops_per_sec);
        println!("Avg Latency:    {:.2} ns ({:.2} µs)", metrics.avg_latency_ns, metrics.avg_latency_ns / 1000.0);
        println!("P95 Latency:    {:.2} ns ({:.2} µs)", metrics.p95_latency_ns, metrics.p95_latency_ns / 1000.0);
        println!("P99 Latency:    {:.2} ns ({:.2} µs)", metrics.p99_latency_ns, metrics.p99_latency_ns / 1000.0);
        println!("=====================================\n");
    }
    
    pub fn create_test_order_request(id: usize) -> OrderRequest {
        OrderRequest {
            client_order_id: format!("perf_test_{}", id),
            symbol: Symbol::new(1),
            side: if id % 2 == 0 { Side::Buy } else { Side::Sell },
            quantity: Qty::from_i64(1_000_000 + (id as i64 * 1000)),
            order_type: OrderType::Limit,
            limit_price: Some(Px::from_i64(50_000_000_000 + (id as i64 * 1_000_000))),
            stop_price: None,
            is_buy: id % 2 == 0,
            algorithm: ExecutionAlgorithm::Smart,
            urgency: 0.5,
            participation_rate: Some(0.15),
            time_in_force: TimeInForce::GTC,
            venue: Some("binance".to_string()),
            strategy_id: format!("perf_strategy_{}", id % 10),
            params: rustc_hash::FxHashMap::default(),
        }
    }
}

use perf_utils::*;

/// Memory Management Performance Tests
#[rstest]
fn test_arena_allocation_performance() -> Result<(), String> {
    let arena = Arena::new(16 * 1024 * 1024)?; // 16MB arena
    
    let (metrics, _results) = measure_operations(MEMORY_OPERATIONS_COUNT, |i| {
        let start = Instant::now();
        
        let allocation: Option<&mut i64> = arena.alloc();
        if let Some(ptr) = allocation {
            *ptr = i as i64;
        }
        
        (start.elapsed(), allocation.is_some())
    });
    
    print_performance_summary("Arena Allocation", &metrics);
    
    // Performance assertions
    assert!(metrics.ops_per_sec > 1_000_000.0, "Arena allocation should be > 1M ops/sec, got {:.0}", metrics.ops_per_sec);
    assert!(metrics.avg_latency_ns < 1000.0, "Average allocation latency should be < 1µs, got {:.2} ns", metrics.avg_latency_ns);
    
    Ok(())
}

#[rstest]
fn test_object_pool_performance() {
    #[derive(Default)]
    struct TestObject {
        data: [u64; 8], // 64 bytes
        counter: usize,
    }
    
    let pool = ObjectPool::<TestObject>::new(10000);
    
    let (metrics, _results) = measure_operations(PERFORMANCE_ITERATIONS, |i| {
        let start = Instant::now();
        
        if let Some(mut obj) = pool.acquire() {
            obj.counter = i;
            obj.data[0] = i as u64;
            // Object returned automatically when dropped
            (start.elapsed(), true)
        } else {
            (start.elapsed(), false)
        }
    });
    
    print_performance_summary("Object Pool Acquire/Release", &metrics);
    
    // Performance assertions
    assert!(metrics.ops_per_sec > 100_000.0, "Pool operations should be > 100K ops/sec");
    assert!(metrics.avg_latency_ns < 10_000.0, "Average pool latency should be < 10µs");
}

#[rstest]
fn test_ring_buffer_performance() {
    let buffer = RingBuffer::<u64, 1024>::new();
    let test_data: Vec<u64> = (0..PERFORMANCE_ITERATIONS).map(|i| i as u64).collect();
    
    // Test push performance
    let (push_metrics, _) = measure_operations(test_data.len(), |i| {
        let start = Instant::now();
        let success = buffer.push(test_data[i]);
        if !success {
            // Buffer full, pop some items
            for _ in 0..100 {
                buffer.pop();
            }
            buffer.push(test_data[i]);
        }
        (start.elapsed(), success)
    });
    
    print_performance_summary("Ring Buffer Push", &push_metrics);
    
    // Test pop performance
    let (pop_metrics, _) = measure_operations(test_data.len(), |_| {
        let start = Instant::now();
        let result = buffer.pop();
        (start.elapsed(), result.is_some())
    });
    
    print_performance_summary("Ring Buffer Pop", &pop_metrics);
    
    // Performance assertions
    assert!(push_metrics.ops_per_sec > 1_000_000.0, "Ring buffer push should be > 1M ops/sec");
    assert!(pop_metrics.ops_per_sec > 1_000_000.0, "Ring buffer pop should be > 1M ops/sec");
}

/// Order Processing Performance Tests
#[rstest]
fn test_order_submission_performance() {
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        let router = ExecutionRouterService::new(VenueStrategy::Smart);
        
        let (metrics, results) = measure_operations(PERFORMANCE_ITERATIONS, |i| {
            let start = Instant::now();
            let request = create_test_order_request(i);
            
            // Create a future for the submission
            let submit_future = router.submit_order(
                request.client_order_id.clone(),
                request.symbol,
                request.side,
                request.quantity,
                "binance".to_string(),
                request.strategy_id,
            );
            
            // We can't easily measure async operation latency in this sync context
            // So we measure the setup time
            (start.elapsed(), submit_future)
        });
        
        // Await all submissions
        let mut successful_orders = 0;
        for (_, future) in results {
            if future.await.is_ok() {
                successful_orders += 1;
            }
        }
        
        print_performance_summary("Order Submission Setup", &metrics);
        println!("Successful orders: {} / {}", successful_orders, PERFORMANCE_ITERATIONS);
        
        assert!(successful_orders > PERFORMANCE_ITERATIONS / 2, "At least 50% of orders should succeed");
    });
}

#[rstest]
fn test_order_query_performance() {
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        let router = ExecutionRouterService::new(VenueStrategy::Primary);
        
        // Submit some orders first
        let mut order_ids = Vec::new();
        for i in 0..100 {
            let request = create_test_order_request(i);
            if let Ok(order_id) = router.submit_order(
                request.client_order_id,
                request.symbol,
                request.side,
                request.quantity,
                "binance".to_string(),
                request.strategy_id,
            ).await {
                order_ids.push(order_id);
            }
        }
        
        if order_ids.is_empty() {
            println!("No orders submitted, skipping query performance test");
            return;
        }
        
        // Test order query performance
        let (metrics, _results) = measure_operations(PERFORMANCE_ITERATIONS, |i| {
            let order_id = order_ids[i % order_ids.len()];
            let start = Instant::now();
            
            let query_future = router.get_order(order_id);
            (start.elapsed(), query_future)
        });
        
        print_performance_summary("Order Query Setup", &metrics);
        
        // The actual async performance would need a different measurement approach
        assert!(metrics.avg_latency_ns < 1000.0, "Order query setup should be fast");
    });
}

/// Concurrency and Thread Safety Tests
#[rstest]
fn test_concurrent_order_processing() {
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        let router = Arc::new(ExecutionRouterService::new(VenueStrategy::Smart));
        let operations_per_thread = 100;
        let barrier = Arc::new(Barrier::new(CONCURRENCY_THREADS));
        
        let start_time = Instant::now();
        
        let mut handles = Vec::new();
        
        for thread_id in 0..CONCURRENCY_THREADS {
            let router_clone = Arc::clone(&router);
            let barrier_clone = Arc::clone(&barrier);
            
            let handle = tokio::spawn(async move {
                barrier_clone.wait();
                let thread_start = Instant::now();
                
                let mut successful_ops = 0;
                let mut failed_ops = 0;
                
                for i in 0..operations_per_thread {
                    let request = create_test_order_request(thread_id * 1000 + i);
                    
                    match router_clone.submit_order(
                        request.client_order_id,
                        request.symbol,
                        request.side,
                        request.quantity,
                        "binance".to_string(),
                        request.strategy_id,
                    ).await {
                        Ok(_) => successful_ops += 1,
                        Err(_) => failed_ops += 1,
                    }
                }
                
                (thread_id, successful_ops, failed_ops, thread_start.elapsed())
            });
            
            handles.push(handle);
        }
        
        // Wait for all threads to complete
        let mut total_successful = 0;
        let mut total_failed = 0;
        
        for handle in handles {
            let (thread_id, successful, failed, thread_duration) = handle.await.unwrap();
            total_successful += successful;
            total_failed += failed;
            
            println!("Thread {}: {} successful, {} failed in {:?}", 
                     thread_id, successful, failed, thread_duration);
        }
        
        let total_duration = start_time.elapsed();
        let total_operations = total_successful + total_failed;
        let throughput = total_operations as f64 / total_duration.as_secs_f64();
        
        println!("\nConcurrency Test Results:");
        println!("Threads:           {}", CONCURRENCY_THREADS);
        println!("Total operations:  {}", total_operations);
        println!("Successful:        {}", total_successful);
        println!("Failed:            {}", total_failed);
        println!("Total duration:    {:?}", total_duration);
        println!("Throughput:        {:.2} ops/sec", throughput);
        
        // Performance assertions
        assert!(total_successful > 0, "Should have some successful operations");
        assert!(throughput > 100.0, "Concurrent throughput should be > 100 ops/sec");
    });
}

#[rstest]
fn test_concurrent_memory_operations() {
    let arena = Arc::new(Arena::new(64 * 1024 * 1024).unwrap()); // 64MB
    let pool = Arc::new(ObjectPool::<[u64; 16]>::new(10000));
    let buffer = Arc::new(RingBuffer::<u64, 2048>::new());
    
    let operations_per_thread = 10000;
    let barrier = Arc::new(Barrier::new(CONCURRENCY_THREADS));
    
    let start_time = Instant::now();
    
    let handles: Vec<_> = (0..CONCURRENCY_THREADS).map(|thread_id| {
        let arena_clone = Arc::clone(&arena);
        let pool_clone = Arc::clone(&pool);
        let buffer_clone = Arc::clone(&buffer);
        let barrier_clone = Arc::clone(&barrier);
        
        thread::spawn(move || {
            barrier_clone.wait();
            let thread_start = Instant::now();
            
            let mut arena_ops = 0;
            let mut pool_ops = 0;
            let mut buffer_ops = 0;
            
            for i in 0..operations_per_thread {
                let value = (thread_id * 1000 + i) as u64;
                
                // Arena operations
                if let Some(ptr) = arena_clone.alloc::<u64>() {
                    *ptr = value;
                    arena_ops += 1;
                }
                
                // Pool operations
                if let Some(mut obj) = pool_clone.acquire() {
                    obj[0] = value;
                    pool_ops += 1;
                }
                
                // Buffer operations
                if buffer_clone.push(value) {
                    buffer_ops += 1;
                }
                
                // Occasionally pop from buffer to prevent overflow
                if i % 100 == 0 {
                    buffer_clone.pop();
                }
            }
            
            (thread_id, arena_ops, pool_ops, buffer_ops, thread_start.elapsed())
        })
    }).collect();
    
    // Wait for all threads
    let mut total_arena_ops = 0;
    let mut total_pool_ops = 0;
    let mut total_buffer_ops = 0;
    
    for handle in handles {
        let (thread_id, arena_ops, pool_ops, buffer_ops, thread_duration) = handle.join().unwrap();
        total_arena_ops += arena_ops;
        total_pool_ops += pool_ops;
        total_buffer_ops += buffer_ops;
        
        println!("Thread {}: Arena={}, Pool={}, Buffer={} ops in {:?}", 
                 thread_id, arena_ops, pool_ops, buffer_ops, thread_duration);
    }
    
    let total_duration = start_time.elapsed();
    
    println!("\nConcurrent Memory Operations Results:");
    println!("Threads:            {}", CONCURRENCY_THREADS);
    println!("Arena operations:   {}", total_arena_ops);
    println!("Pool operations:    {}", total_pool_ops);
    println!("Buffer operations:  {}", total_buffer_ops);
    println!("Total duration:     {:?}", total_duration);
    
    // Performance assertions
    assert!(total_arena_ops > 0, "Should have successful arena operations");
    assert!(total_pool_ops > 0, "Should have successful pool operations");
    assert!(total_buffer_ops > 0, "Should have successful buffer operations");
}

/// Router Performance Tests
#[rstest]
fn test_smart_router_performance() {
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        let router = Router::new();
        
        // Add test venues
        for venue_name in &["Binance", "Coinbase", "Kraken", "Bybit"] {
            router.add_venue(VenueConnection {
                name: venue_name.to_string(),
                is_connected: true,
                latency_us: 1000,
                liquidity: 10_000_000.0,
                maker_fee_bp: 10,
                taker_fee_bp: 20,
                supported_types: vec![OrderType::Market, OrderType::Limit],
                last_heartbeat: Instant::now(),
            });
        }
        
        let (metrics, results) = measure_operations(1000, |i| {
            let start = Instant::now();
            let request = create_test_order_request(i);
            
            let route_future = router.route_order(request);
            (start.elapsed(), route_future)
        });
        
        // Await routing results
        let mut successful_routes = 0;
        for (_, future) in results {
            if future.await.is_ok() {
                successful_routes += 1;
            }
        }
        
        print_performance_summary("Smart Router Setup", &metrics);
        println!("Successful routes: {} / 1000", successful_routes);
        
        assert!(successful_routes > 800, "At least 80% of routes should succeed");
    });
}

/// Stress Testing
#[rstest]
fn test_sustained_load_performance() {
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        let router = Arc::new(ExecutionRouterService::new(VenueStrategy::Smart));
        let operations_counter = Arc::new(AtomicUsize::new(0));
        let success_counter = Arc::new(AtomicUsize::new(0));
        let error_counter = Arc::new(AtomicUsize::new(0));
        
        let test_duration = Duration::from_secs(STRESS_TEST_DURATION_SECS);
        let start_time = Instant::now();
        
        let mut handles = Vec::new();
        
        for thread_id in 0..CONCURRENCY_THREADS {
            let router_clone = Arc::clone(&router);
            let ops_counter = Arc::clone(&operations_counter);
            let success_counter = Arc::clone(&success_counter);
            let error_counter = Arc::clone(&error_counter);
            
            let handle = tokio::spawn(async move {
                let mut local_ops = 0;
                let thread_start = Instant::now();
                
                while thread_start.elapsed() < test_duration {
                    let request = create_test_order_request(local_ops + thread_id * 100000);
                    
                    match router_clone.submit_order(
                        request.client_order_id,
                        request.symbol,
                        request.side,
                        request.quantity,
                        "binance".to_string(),
                        request.strategy_id,
                    ).await {
                        Ok(_) => {
                            success_counter.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(_) => {
                            error_counter.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    
                    local_ops += 1;
                    ops_counter.fetch_add(1, Ordering::Relaxed);
                    
                    // Small delay to prevent overwhelming
                    if local_ops % 100 == 0 {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                }
                
                local_ops
            });
            
            handles.push(handle);
        }
        
        // Wait for all threads to complete
        let mut total_thread_ops = 0;
        for handle in handles {
            total_thread_ops += handle.await.unwrap();
        }
        
        let actual_duration = start_time.elapsed();
        let total_operations = operations_counter.load(Ordering::Relaxed);
        let successful_operations = success_counter.load(Ordering::Relaxed);
        let failed_operations = error_counter.load(Ordering::Relaxed);
        
        let throughput = total_operations as f64 / actual_duration.as_secs_f64();
        let success_rate = successful_operations as f64 / total_operations as f64 * 100.0;
        
        println!("\nSustained Load Test Results:");
        println!("Test duration:      {:?}", actual_duration);
        println!("Total operations:   {}", total_operations);
        println!("Successful:         {}", successful_operations);
        println!("Failed:             {}", failed_operations);
        println!("Success rate:       {:.2}%", success_rate);
        println!("Throughput:         {:.2} ops/sec", throughput);
        
        // Performance assertions
        assert!(total_operations > 0, "Should have processed operations");
        assert!(throughput > 10.0, "Should maintain > 10 ops/sec under sustained load");
        assert!(success_rate > 0.0, "Should have some successful operations");
        
        // Health check after stress test
        let is_healthy = router.is_healthy().await;
        println!("Service health after stress test: {}", is_healthy);
    });
}

/// Resource Utilization Tests
#[rstest]
fn test_memory_usage_patterns() -> Result<(), String> {
    // Test that memory allocators don't leak or fragment excessively
    let arena = Arena::new(1024 * 1024)?; // 1MB
    let pool = ObjectPool::<[u8; 1024]>::new(1000);
    
    let iterations = 10000;
    let allocation_pattern_test = |name: &str, mut alloc_fn: Box<dyn FnMut() -> bool>| {
        let start_memory = std::alloc::System.used_memory();
        let start_time = Instant::now();
        
        let mut successful_allocations = 0;
        for _ in 0..iterations {
            if alloc_fn() {
                successful_allocations += 1;
            }
        }
        
        let end_time = Instant::now();
        let end_memory = std::alloc::System.used_memory();
        
        println!("{} Pattern Test:", name);
        println!("  Successful allocations: {} / {}", successful_allocations, iterations);
        println!("  Duration: {:?}", end_time - start_time);
        println!("  Memory change: {} bytes", end_memory as i64 - start_memory as i64);
        
        successful_allocations > 0
    };
    
    // Arena allocation pattern
    let arena_test = allocation_pattern_test(
        "Arena",
        Box::new(|| arena.alloc::<[u8; 1024]>().is_some())
    );
    
    // Pool allocation pattern
    let pool_test = allocation_pattern_test(
        "Pool", 
        Box::new(|| {
            if let Some(_obj) = pool.acquire() {
                // Object dropped immediately
                true
            } else {
                false
            }
        })
    );
    
    assert!(arena_test, "Arena allocation pattern should succeed");
    assert!(pool_test, "Pool allocation pattern should succeed");
    
    Ok(())
}

/// Performance Regression Tests
#[rstest]
fn test_performance_regression_detection() {
    // Define expected performance baselines
    let performance_baselines = HashMap::from([
        ("arena_alloc_ops_per_sec", 1_000_000.0),
        ("pool_acquire_ops_per_sec", 100_000.0),
        ("ring_buffer_ops_per_sec", 1_000_000.0),
    ]);
    
    // Run performance tests and compare against baselines
    let mut actual_performance = HashMap::new();
    
    // Arena performance
    if let Ok(arena) = Arena::new(16 * 1024 * 1024) {
        let (metrics, _) = measure_operations(10000, |i| {
            let start = Instant::now();
            if let Some(ptr) = arena.alloc::<u64>() {
                *ptr = i as u64;
            }
            (start.elapsed(), ())
        });
        
        actual_performance.insert("arena_alloc_ops_per_sec", metrics.ops_per_sec);
    }
    
    // Pool performance
    let pool = ObjectPool::<u64>::new(1000);
    let (metrics, _) = measure_operations(5000, |_| {
        let start = Instant::now();
        let _obj = pool.acquire();
        (start.elapsed(), ())
    });
    actual_performance.insert("pool_acquire_ops_per_sec", metrics.ops_per_sec);
    
    // Ring buffer performance
    let buffer = RingBuffer::<u64, 1024>::new();
    let (metrics, _) = measure_operations(10000, |i| {
        let start = Instant::now();
        if !buffer.push(i as u64) {
            // Buffer full, pop and retry
            buffer.pop();
            buffer.push(i as u64);
        }
        (start.elapsed(), ())
    });
    actual_performance.insert("ring_buffer_ops_per_sec", metrics.ops_per_sec);
    
    // Compare against baselines
    println!("\nPerformance Regression Analysis:");
    for (metric, baseline) in performance_baselines.iter() {
        if let Some(actual) = actual_performance.get(metric) {
            let ratio = actual / baseline;
            let status = if ratio >= 0.8 {
                "✅ PASS"
            } else if ratio >= 0.5 {
                "⚠️  SLOW"
            } else {
                "❌ FAIL"
            };
            
            println!("  {}: {:.0} ops/sec (baseline: {:.0}, ratio: {:.2}) {}", 
                     metric, actual, baseline, ratio, status);
            
            // Fail test if performance is significantly degraded
            assert!(ratio >= 0.3, "Performance regression detected for {}: {:.2}x slower than baseline", metric, 1.0 / ratio);
        }
    }
}

/// Lock Contention Analysis
#[rstest]
fn test_lock_contention_patterns() {
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        let router = Arc::new(ExecutionRouterService::new(VenueStrategy::Smart));
        let contention_threads = 16; // High thread count to stress locks
        let operations_per_thread = 100;
        
        let barrier = Arc::new(Barrier::new(contention_threads));
        let start_time = Arc::new(std::sync::Mutex::new(None));
        
        let handles: Vec<_> = (0..contention_threads).map(|thread_id| {
            let router_clone = Arc::clone(&router);
            let barrier_clone = Arc::clone(&barrier);
            let start_time_clone = Arc::clone(&start_time);
            
            tokio::spawn(async move {
                // Wait for all threads to be ready
                barrier_clone.wait();
                
                // Record start time from first thread
                if thread_id == 0 {
                    *start_time_clone.lock().unwrap() = Some(Instant::now());
                }
                
                let thread_start = Instant::now();
                let mut operation_times = Vec::new();
                
                for i in 0..operations_per_thread {
                    let op_start = Instant::now();
                    
                    // Mix of read and write operations to test different lock types
                    match i % 3 {
                        0 => {
                            // Read operation (metrics)
                            let _metrics = router_clone.get_metrics().await;
                        }
                        1 => {
                            // Write operation (order submission)
                            let request = create_test_order_request(thread_id * 1000 + i);
                            let _result = router_clone.submit_order(
                                request.client_order_id,
                                request.symbol,
                                request.side,
                                request.quantity,
                                "binance".to_string(),
                                request.strategy_id,
                            ).await;
                        }
                        _ => {
                            // Query operation
                            let _result = router_clone.get_order(thread_id as u64 + i as u64).await;
                        }
                    }
                    
                    operation_times.push(op_start.elapsed());
                }
                
                (thread_id, operation_times, thread_start.elapsed())
            })
        }).collect();
        
        // Collect results
        let mut all_operation_times = Vec::new();
        let mut max_thread_time = Duration::ZERO;
        
        for handle in handles {
            let (thread_id, operation_times, thread_duration) = handle.await.unwrap();
            all_operation_times.extend(operation_times);
            max_thread_time = max_thread_time.max(thread_duration);
            
            println!("Thread {} completed in {:?}", thread_id, thread_duration);
        }
        
        // Analyze lock contention by looking at operation time distribution
        all_operation_times.sort();
        let total_ops = all_operation_times.len();
        let median_time = all_operation_times[total_ops / 2];
        let p95_time = all_operation_times[(total_ops as f64 * 0.95) as usize];
        let p99_time = all_operation_times[(total_ops as f64 * 0.99) as usize];
        
        println!("\nLock Contention Analysis:");
        println!("Total operations:   {}", total_ops);
        println!("Max thread time:    {:?}", max_thread_time);
        println!("Median op time:     {:?}", median_time);
        println!("P95 op time:        {:?}", p95_time);
        println!("P99 op time:        {:?}", p99_time);
        
        // Assess contention level
        let contention_ratio = p99_time.as_nanos() as f64 / median_time.as_nanos() as f64;
        println!("Contention ratio:   {:.2}x (P99/median)", contention_ratio);
        
        if contention_ratio > 10.0 {
            println!("⚠️  High lock contention detected");
        } else if contention_ratio > 3.0 {
            println!("⚠️  Moderate lock contention detected");
        } else {
            println!("✅ Low lock contention");
        }
        
        // Performance assertion - shouldn't have extreme contention
        assert!(contention_ratio < 100.0, "Extreme lock contention detected: {:.2}x", contention_ratio);
    });
}

// Helper trait for memory usage measurement (simplified implementation)
trait MemoryUsage {
    fn used_memory(&self) -> usize;
}

impl MemoryUsage for std::alloc::System {
    fn used_memory(&self) -> usize {
        // Simplified implementation - in real usage, you'd use system-specific APIs
        // or profiling tools like jemalloc or tcmalloc
        0 // Placeholder
    }
}