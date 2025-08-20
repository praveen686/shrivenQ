//! Memory management tests
//!
//! Comprehensive tests for zero-allocation memory components:
//! - Arena allocator correctness and performance
//! - ObjectPool lock-free operations and ABA prevention  
//! - RingBuffer SPSC communication and memory safety
//! - Memory statistics and monitoring
//! - Concurrent access patterns and thread safety

use execution_router::memory::{Arena, ObjectPool, PoolRef, RingBuffer, MemoryStats};
use rstest::*;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Test data structures for memory tests
#[derive(Debug, Clone, PartialEq, Default)]
struct TestData {
    value: i64,
    flag: bool,
}

#[derive(Debug, Default)]
struct LargeTestData {
    data: [u64; 64], // 512 bytes
    counter: usize,
}

/// Arena Allocator Tests
#[fixture]
fn test_arena() -> Arena {
    Arena::new(4096).expect("Failed to create test arena")
}

#[rstest]
fn test_arena_creation() -> Result<(), String> {
    // Test valid creation
    let arena = Arena::new(1024)?;
    let stats = arena.stats();
    assert_eq!(stats.chunks, 1);
    assert_eq!(stats.chunk_size, 1024);
    assert_eq!(stats.total_used, 0);
    
    // Test zero size
    let empty_arena = Arena::new(0)?;
    let empty_stats = empty_arena.stats();
    assert_eq!(empty_stats.chunks, 0);
    
    Ok(())
}

#[rstest]
fn test_arena_basic_allocation(test_arena: Arena) {
    // Allocate different types
    let int_ref: &mut i32 = test_arena.alloc().expect("Failed to allocate i32");
    *int_ref = 42;
    assert_eq!(*int_ref, 42);
    
    let data_ref: &mut TestData = test_arena.alloc().expect("Failed to allocate TestData");
    data_ref.value = 100;
    data_ref.flag = true;
    assert_eq!(data_ref.value, 100);
    assert!(data_ref.flag);
    
    // Verify statistics
    let stats = test_arena.stats();
    assert!(stats.total_used > 0);
    assert!(stats.total_used <= stats.total_size);
}

#[rstest]
fn test_arena_alignment() -> Result<(), String> {
    let arena = Arena::new(2048)?;
    
    // Allocate byte to potentially misalign subsequent allocations
    let byte: &mut u8 = arena.alloc().expect("Failed to allocate u8");
    *byte = 0x42;
    
    // Allocate aligned types
    let aligned_u64: &mut u64 = arena.alloc().expect("Failed to allocate u64");
    *aligned_u64 = 0x123456789ABCDEF0;
    
    let aligned_data: &mut TestData = arena.alloc().expect("Failed to allocate TestData");
    aligned_data.value = 999;
    
    // Verify alignment
    let addr_u64 = aligned_u64 as *mut u64 as usize;
    assert_eq!(addr_u64 % std::mem::align_of::<u64>(), 0, "u64 should be properly aligned");
    
    let addr_data = aligned_data as *mut TestData as usize;
    assert_eq!(addr_data % std::mem::align_of::<TestData>(), 0, "TestData should be properly aligned");
    
    // Verify values are correct
    assert_eq!(*byte, 0x42);
    assert_eq!(*aligned_u64, 0x123456789ABCDEF0);
    assert_eq!(aligned_data.value, 999);
    
    Ok(())
}

#[rstest]
fn test_arena_exhaustion() -> Result<(), String> {
    let small_arena = Arena::new(64)?; // Very small arena
    
    // Allocate until exhausted
    let mut allocations = 0;
    loop {
        match small_arena.alloc::<TestData>() {
            Some(data) => {
                data.value = allocations;
                allocations += 1;
            }
            None => break,
        }
        
        // Safety check to prevent infinite loop
        if allocations > 100 {
            panic!("Arena should have been exhausted by now");
        }
    }
    
    assert!(allocations > 0, "Should have succeeded some allocations");
    assert!(allocations < 10, "Small arena should be exhausted quickly");
    
    Ok(())
}

#[rstest]
fn test_arena_reset() -> Result<(), String> {
    let mut arena = Arena::new(1024)?;
    
    // Allocate some objects
    for i in 0..10 {
        let data: &mut TestData = arena.alloc().expect("Failed to allocate");
        data.value = i;
    }
    
    let stats_before = arena.stats();
    assert!(stats_before.total_used > 0);
    
    // Reset arena
    arena.reset();
    
    let stats_after = arena.stats();
    assert_eq!(stats_after.total_used, 0);
    assert_eq!(stats_after.chunks, stats_before.chunks); // No deallocation
    assert_eq!(stats_after.chunk_size, stats_before.chunk_size);
    
    // Should be able to allocate again after reset
    let data: &mut TestData = arena.alloc().expect("Failed to allocate after reset");
    data.value = 42;
    assert_eq!(data.value, 42);
    
    Ok(())
}

#[rstest]
fn test_arena_concurrent_allocation() -> Result<(), String> {
    let arena = Arc::new(Arena::new(8192)?);
    let num_threads = 4;
    let allocations_per_thread = 100;
    let barrier = Arc::new(Barrier::new(num_threads));
    
    let handles: Vec<_> = (0..num_threads).map(|thread_id| {
        let arena = Arc::clone(&arena);
        let barrier = Arc::clone(&barrier);
        
        thread::spawn(move || {
            barrier.wait(); // Synchronize start
            
            let mut successful_allocations = 0;
            for i in 0..allocations_per_thread {
                if let Some(data) = arena.alloc::<TestData>() {
                    data.value = (thread_id * 1000 + i) as i64;
                    data.flag = thread_id % 2 == 0;
                    successful_allocations += 1;
                }
            }
            successful_allocations
        })
    }).collect();
    
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let total_allocations: usize = results.iter().sum();
    
    assert!(total_allocations > 0, "Should have some successful allocations");
    println!("Concurrent allocations: {total_allocations} / {}", num_threads * allocations_per_thread);
    
    Ok(())
}

/// ObjectPool Tests
#[fixture]
fn test_pool() -> ObjectPool<TestData> {
    ObjectPool::new(10)
}

#[rstest]
fn test_pool_creation(test_pool: ObjectPool<TestData>) {
    assert_eq!(test_pool.capacity(), 10);
    assert_eq!(test_pool.allocated(), 0);
    assert!(!test_pool.is_exhausted());
}

#[rstest]
fn test_pool_basic_operations(test_pool: ObjectPool<TestData>) {
    // Acquire object
    {
        let mut obj = test_pool.acquire().expect("Failed to acquire object");
        obj.value = 42;
        obj.flag = true;
        
        assert_eq!(test_pool.allocated(), 1);
        assert_eq!(obj.value, 42);
        assert!(obj.flag);
    } // Object returned to pool here
    
    assert_eq!(test_pool.allocated(), 0);
}

#[rstest]
fn test_pool_exhaustion(test_pool: ObjectPool<TestData>) {
    let mut objects = Vec::new();
    
    // Exhaust pool
    for i in 0..test_pool.capacity() {
        let mut obj = test_pool.acquire().expect("Pool should not be exhausted yet");
        obj.value = i as i64;
        objects.push(obj);
    }
    
    assert!(test_pool.is_exhausted());
    assert!(test_pool.acquire().is_none());
    
    // Return one object
    objects.pop();
    assert!(!test_pool.is_exhausted());
    
    // Should be able to acquire again
    let obj = test_pool.acquire().expect("Should be able to acquire after return");
    assert!(obj.value >= 0); // Value from previous use should be preserved
}

#[rstest]
fn test_pool_concurrent_access() {
    let pool = Arc::new(ObjectPool::<TestData>::new(50));
    let num_threads = 8;
    let iterations_per_thread = 100;
    let barrier = Arc::new(Barrier::new(num_threads));
    
    let handles: Vec<_> = (0..num_threads).map(|thread_id| {
        let pool = Arc::clone(&pool);
        let barrier = Arc::clone(&barrier);
        
        thread::spawn(move || {
            barrier.wait();
            
            let mut acquisitions = 0;
            let mut failures = 0;
            
            for i in 0..iterations_per_thread {
                match pool.acquire() {
                    Some(mut obj) => {
                        obj.value = (thread_id * 1000 + i) as i64;
                        obj.flag = thread_id % 2 == 0;
                        acquisitions += 1;
                        
                        // Simulate work
                        thread::sleep(Duration::from_micros(1));
                    }
                    None => {
                        failures += 1;
                    }
                }
            }
            
            (acquisitions, failures)
        })
    }).collect();
    
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let (total_acquisitions, total_failures): (usize, usize) = results.iter().cloned().fold((0, 0), |(acc_acq, acc_fail), (acq, fail)| (acc_acq + acq, acc_fail + fail));
    
    assert!(total_acquisitions > 0, "Should have some successful acquisitions");
    assert_eq!(pool.allocated(), 0, "All objects should be returned");
    
    println!("Concurrent pool operations: {total_acquisitions} successes, {total_failures} failures");
}

#[rstest]
fn test_pool_aba_prevention() {
    // Test for ABA problem in lock-free operations
    let pool = Arc::new(ObjectPool::<TestData>::new(2));
    let iterations = 1000;
    
    // Rapidly acquire and release objects from multiple threads
    let handles: Vec<_> = (0..4).map(|thread_id| {
        let pool = Arc::clone(&pool);
        
        thread::spawn(move || {
            for i in 0..iterations {
                if let Some(mut obj) = pool.acquire() {
                    obj.value = (thread_id * iterations + i) as i64;
                    // Drop immediately to trigger rapid recycle
                }
                
                // Occasionally yield to create contention
                if i % 10 == 0 {
                    thread::yield_now();
                }
            }
        })
    }).collect();
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    assert_eq!(pool.allocated(), 0, "All objects should be returned after ABA test");
}

/// RingBuffer Tests
#[fixture]
fn test_ring_buffer() -> RingBuffer<i32, 16> {
    RingBuffer::new()
}

#[rstest]
fn test_ring_buffer_creation(test_ring_buffer: RingBuffer<i32, 16>) {
    assert!(test_ring_buffer.is_empty());
    assert!(!test_ring_buffer.is_full());
    assert_eq!(test_ring_buffer.len(), 0);
    assert_eq!(test_ring_buffer.capacity(), 15); // N-1 for ring buffer
}

#[rstest]
fn test_ring_buffer_basic_operations(test_ring_buffer: RingBuffer<i32, 16>) {
    // Push some values
    assert!(test_ring_buffer.push(1));
    assert!(test_ring_buffer.push(2));
    assert!(test_ring_buffer.push(3));
    
    assert_eq!(test_ring_buffer.len(), 3);
    assert!(!test_ring_buffer.is_empty());
    assert!(!test_ring_buffer.is_full());
    
    // Pop values
    assert_eq!(test_ring_buffer.pop(), Some(1));
    assert_eq!(test_ring_buffer.pop(), Some(2));
    assert_eq!(test_ring_buffer.pop(), Some(3));
    assert_eq!(test_ring_buffer.pop(), None);
    
    assert!(test_ring_buffer.is_empty());
    assert_eq!(test_ring_buffer.len(), 0);
}

#[rstest]
fn test_ring_buffer_wraparound(test_ring_buffer: RingBuffer<i32, 16>) {
    let capacity = test_ring_buffer.capacity();
    
    // Fill to capacity
    for i in 0..capacity {
        assert!(test_ring_buffer.push(i as i32), "Should be able to push value {i}");
    }
    
    assert!(test_ring_buffer.is_full());
    assert!(!test_ring_buffer.push(999)); // Should fail when full
    
    // Empty half the buffer
    for i in 0..capacity/2 {
        assert_eq!(test_ring_buffer.pop(), Some(i as i32));
    }
    
    // Fill the freed space (testing wraparound)
    for i in 0..capacity/2 {
        let value = 1000 + i as i32;
        assert!(test_ring_buffer.push(value), "Should be able to push wrapped value {value}");
    }
    
    // Verify remaining original values
    for i in capacity/2..capacity {
        assert_eq!(test_ring_buffer.pop(), Some(i as i32));
    }
    
    // Verify wrapped values
    for i in 0..capacity/2 {
        let expected = 1000 + i as i32;
        assert_eq!(test_ring_buffer.pop(), Some(expected));
    }
    
    assert!(test_ring_buffer.is_empty());
}

#[rstest]
fn test_ring_buffer_spsc_communication() {
    let buffer = Arc::new(RingBuffer::<TestData, 64>::new());
    let num_messages = 10000;
    let barrier = Arc::new(Barrier::new(2));
    
    let buffer_producer = Arc::clone(&buffer);
    let barrier_producer = Arc::clone(&barrier);
    
    // Producer thread
    let producer = thread::spawn(move || {
        barrier_producer.wait();
        let start = Instant::now();
        
        for i in 0..num_messages {
            let data = TestData {
                value: i,
                flag: i % 2 == 0,
            };
            
            while !buffer_producer.push(data.clone()) {
                thread::yield_now(); // Wait for consumer to catch up
            }
        }
        
        start.elapsed()
    });
    
    let buffer_consumer = Arc::clone(&buffer);
    let barrier_consumer = Arc::clone(&barrier);
    
    // Consumer thread
    let consumer = thread::spawn(move || {
        barrier_consumer.wait();
        let start = Instant::now();
        let mut received = Vec::with_capacity(num_messages);
        
        while received.len() < num_messages {
            if let Some(data) = buffer_consumer.pop() {
                received.push(data);
            } else {
                thread::yield_now();
            }
        }
        
        (received, start.elapsed())
    });
    
    let producer_time = producer.join().unwrap();
    let (received_data, consumer_time) = consumer.join().unwrap();
    
    // Verify all messages received in order
    assert_eq!(received_data.len(), num_messages);
    for (i, data) in received_data.iter().enumerate() {
        assert_eq!(data.value, i as i64, "Message {i} value mismatch");
        assert_eq!(data.flag, i % 2 == 0, "Message {i} flag mismatch");
    }
    
    assert!(buffer.is_empty(), "Buffer should be empty after test");
    
    println!("SPSC performance: Producer: {producer_time:?}, Consumer: {consumer_time:?}");
}

#[rstest]
fn test_ring_buffer_performance_characteristics() {
    let buffer = RingBuffer::<u64, 1024>::new();
    let iterations = 100_000;
    
    // Measure push performance
    let start = Instant::now();
    for i in 0..iterations {
        if !buffer.push(i) {
            // Handle full buffer
            while buffer.pop().is_some() {} // Empty buffer
            buffer.push(i); // Retry
        }
    }
    let push_time = start.elapsed();
    
    // Measure pop performance
    let start = Instant::now();
    let mut popped = 0;
    while let Some(_) = buffer.pop() {
        popped += 1;
    }
    let pop_time = start.elapsed();
    
    println!("Ring buffer performance:");
    println!("  Push: {:?} for {iterations} operations ({:.2} ns/op)", 
             push_time, push_time.as_nanos() as f64 / iterations as f64);
    println!("  Pop: {:?} for {popped} operations ({:.2} ns/op)", 
             pop_time, pop_time.as_nanos() as f64 / popped as f64);
    
    // Performance assertions (adjust based on platform)
    assert!(push_time.as_nanos() / iterations < 1000, "Push should be under 1µs per operation");
    assert!(pop_time.as_nanos() / popped < 1000, "Pop should be under 1µs per operation");
}

/// Memory Safety and Edge Case Tests
#[rstest]
fn test_arena_large_allocation_failure() -> Result<(), String> {
    let arena = Arena::new(1024)?;
    
    // Try to allocate object larger than chunk size
    let large_allocation = arena.alloc::<LargeTestData>();
    assert!(large_allocation.is_none(), "Large allocation should fail gracefully");
    
    Ok(())
}

#[rstest]
fn test_pool_zero_capacity() {
    // This should panic in debug mode due to assertion
    std::panic::catch_unwind(|| {
        ObjectPool::<TestData>::new(0)
    }).expect_err("Zero capacity pool should panic");
}

#[rstest]
fn test_ring_buffer_single_element() {
    let buffer = RingBuffer::<i32, 2>::new(); // Capacity 1 (N-1)
    
    assert!(buffer.push(42));
    assert!(buffer.is_full());
    assert!(!buffer.push(43)); // Should fail
    
    assert_eq!(buffer.pop(), Some(42));
    assert!(buffer.is_empty());
    assert!(buffer.push(43)); // Should work now
}

/// Stress Tests
#[rstest]
fn test_memory_components_stress() -> Result<(), String> {
    // Arena stress test
    let arena = Arena::new(16384)?;
    for _ in 0..1000 {
        let _data: &mut TestData = arena.alloc().expect("Arena allocation should succeed");
    }
    
    // Pool stress test
    let pool = ObjectPool::<TestData>::new(100);
    let mut objects = Vec::new();
    
    for _ in 0..100 {
        if let Some(obj) = pool.acquire() {
            objects.push(obj);
        }
    }
    assert_eq!(objects.len(), 100);
    
    // Ring buffer stress test
    let buffer = RingBuffer::<i64, 256>::new();
    for i in 0..200 {
        buffer.push(i);
    }
    
    for i in 0..100 {
        assert_eq!(buffer.pop(), Some(i));
    }
    
    Ok(())
}

#[rstest]
fn test_memory_alignment_comprehensive() -> Result<(), String> {
    let arena = Arena::new(4096)?;
    
    // Test various alignment requirements
    let u8_ptr: &mut u8 = arena.alloc().unwrap();
    let u16_ptr: &mut u16 = arena.alloc().unwrap();
    let u32_ptr: &mut u32 = arena.alloc().unwrap();
    let u64_ptr: &mut u64 = arena.alloc().unwrap();
    let data_ptr: &mut TestData = arena.alloc().unwrap();
    
    // Verify alignments
    assert_eq!(u8_ptr as *mut u8 as usize % 1, 0);
    assert_eq!(u16_ptr as *mut u16 as usize % 2, 0);
    assert_eq!(u32_ptr as *mut u32 as usize % 4, 0);
    assert_eq!(u64_ptr as *mut u64 as usize % 8, 0);
    assert_eq!(data_ptr as *mut TestData as usize % std::mem::align_of::<TestData>(), 0);
    
    Ok(())
}

/// Benchmark-style Performance Tests
#[rstest]
fn test_hot_path_performance() -> Result<(), String> {
    const ITERATIONS: usize = 1_000_000;
    
    // Arena hot path
    let arena = Arena::new(64 * 1024)?; // 64KB
    let start = Instant::now();
    
    for _ in 0..ITERATIONS {
        if let Some(data) = arena.alloc::<i64>() {
            *data = 42;
        }
    }
    
    let arena_time = start.elapsed();
    
    // Pool hot path
    let pool = ObjectPool::<TestData>::new(1000);
    let start = Instant::now();
    
    for _ in 0..ITERATIONS / 100 { // Fewer iterations due to exhaustion
        if let Some(mut obj) = pool.acquire() {
            obj.value = 42;
            // Object returned when dropped
        }
    }
    
    let pool_time = start.elapsed();
    
    // Ring buffer hot path
    let buffer = RingBuffer::<i64, 1024>::new();
    let start = Instant::now();
    
    for i in 0..ITERATIONS / 10 {
        if !buffer.push(i as i64) {
            // Empty buffer and continue
            while buffer.pop().is_some() {}
            buffer.push(i as i64);
        }
    }
    
    let ring_time = start.elapsed();
    
    println!("Hot path performance:");
    println!("  Arena: {arena_time:?} for {ITERATIONS} allocations");
    println!("  Pool: {pool_time:?} for {} acquisitions", ITERATIONS / 100);
    println!("  Ring: {ring_time:?} for {} pushes", ITERATIONS / 10);
    
    // Basic performance assertions (adjust based on platform)
    assert!(arena_time.as_millis() < 100, "Arena allocations should be fast");
    assert!(pool_time.as_millis() < 100, "Pool operations should be fast");
    assert!(ring_time.as_millis() < 100, "Ring buffer operations should be fast");
    
    Ok(())
}