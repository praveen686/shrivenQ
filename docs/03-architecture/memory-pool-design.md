# ADR-0006: Lock-Free Memory Pool Design

## Status
Accepted

## Context
High-frequency trading systems require zero-allocation in hot paths to maintain ultra-low latency. Dynamic memory allocation causes:
- Unpredictable latency spikes (malloc can take microseconds)
- Cache misses and fragmentation
- Potential page faults
- GC pressure in managed languages

## Decision

We implement a lock-free, ABA-safe object pool with RAII semantics for automatic resource management.

### Design Principles

1. **Lock-Free Operation**: Uses atomic compare-and-swap for thread-safe access without locks
2. **ABA Prevention**: Tagged pointers with generation counters prevent ABA problems
3. **RAII Wrapper**: Objects automatically returned via Drop trait
4. **Safe API**: No unsafe code exposed to users
5. **Cache-Friendly**: Pre-allocated, contiguous memory layout

### Architecture

```rust
pub struct ObjectPool<T> {
    storage: Box<[UnsafeCell<MaybeUninit<T>>]>,  // Pre-allocated objects
    free_list: AtomicUsize,                       // Tagged head pointer
    nodes: Box<[FreeNode]>,                       // Free list metadata
    allocated: AtomicUsize,                       // Usage counter
}

pub struct PoolRef<'a, T> {
    obj: &'a mut T,
    pool: &'a ObjectPool<T>,
    index: usize,
}
```

### Tagged Pointer Format
```
63                    32 31                    0
+----------------------+----------------------+
|    Generation (32)   |     Index (32)       |
+----------------------+----------------------+
```

### Key Implementation Details

1. **Pointer Arithmetic**: Uses `offset_from()` instead of raw pointer subtraction
2. **Debug Assertions**: Double-free detection without runtime overhead
3. **Atomic Operations**: Relaxed ordering where possible, Acquire-Release for synchronization
4. **Capacity Limits**: 2^32 objects max (4 billion) due to index size

## Implementation

```rust
// Acquire object - lock-free with retry loop
pub fn acquire(&self) -> Option<PoolRef<'_, T>> {
    loop {
        let head_tagged = self.free_list.load(Ordering::Acquire);
        let head_index = (head_tagged & 0xFFFFFFFF) as usize;
        
        if head_index == usize::MAX {
            return None; // Pool exhausted
        }
        
        // Load next and create new tagged value
        let next = self.nodes[head_index].next.load(Ordering::Acquire);
        let new_gen = ((head_tagged >> 32) + 1) & 0xFFFFFFFF;
        let new_tagged = (new_gen << 32) | (next & 0xFFFFFFFF);
        
        // CAS to update head
        if self.free_list.compare_exchange_weak(
            head_tagged,
            new_tagged,
            Ordering::Release,
            Ordering::Acquire,
        ).is_ok() {
            // Success - return wrapped object
            return Some(PoolRef { ... });
        }
        // Retry on CAS failure
    }
}
```

## Performance Characteristics

| Operation | Complexity | Typical Latency |
|-----------|------------|-----------------|
| Acquire   | O(1) amortized | ~20ns |
| Release   | O(1) amortized | ~15ns |
| Memory    | O(n) pre-allocated | 0 runtime |

### Benchmarks (Intel i7-12700K)
```
ObjectPool::acquire    20.3 ns/iter (+/- 0.8)
ObjectPool::release    15.7 ns/iter (+/- 0.5)
Heap allocation       180.0 ns/iter (+/- 25.0)
```

## Trade-offs

### Advantages
- Predictable, ultra-low latency
- No memory fragmentation
- Thread-safe without locks
- Automatic cleanup via RAII
- Cache-friendly layout

### Disadvantages
- Fixed capacity (must size appropriately)
- Memory overhead if underutilized
- Complex implementation
- Limited to single type per pool

## Alternatives Considered

1. **Arena Allocator**: Simple but not thread-safe, no individual deallocation
2. **Slab Allocator**: More complex, higher overhead for small objects
3. **Thread-Local Pools**: No cross-thread sharing, complex ownership
4. **Hazard Pointers**: More complex, higher overhead for our use case

## Usage Example

```rust
// Create pool with 1000 pre-allocated orders
let pool = OrderPool::new(1000);

// In hot path - zero allocation
if let Some(mut order) = pool.acquire() {
    *order = Order::new(id, symbol, side, qty, price);
    order.status.store(OrderStatus::Pending, Ordering::Release);
    
    // Send to venue...
    venue.send_order(&order)?;
    
    // Automatically returned to pool when dropped
}
```

## Monitoring

The pool provides metrics for monitoring:
- `allocated()`: Current objects in use
- `capacity()`: Total pool size
- `is_exhausted()`: Pool exhaustion check

## Future Enhancements

1. **Dynamic Resizing**: Grow pool when approaching exhaustion
2. **NUMA Awareness**: Per-NUMA-node pools for better locality
3. **Statistics**: Track high-water marks, allocation patterns
4. **Sharding**: Multiple pools to reduce contention

## References
- [Lock-Free Programming](https://en.wikipedia.org/wiki/Non-blocking_algorithm)
- [ABA Problem](https://en.wikipedia.org/wiki/ABA_problem)
- [Rust Memory Model](https://doc.rust-lang.org/nomicon/atomics.html)
- [High Performance Trading Systems](https://web.archive.org/web/20110219163448/http://www.kx.com/papers/)