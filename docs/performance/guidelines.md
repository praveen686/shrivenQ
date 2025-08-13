# Performance Guidelines

## Overview
This document outlines performance best practices for ShrivenQuant's high-frequency trading system.

## Core Principles

### 1. Zero Allocation in Hot Paths
**Never allocate memory in performance-critical code paths.**

```rust
// BAD: Allocates on every tick
fn process_tick(data: TickData) {
    let order = Box::new(Order::new()); // ALLOCATION!
    let message = format!("Price: {}", data.price); // ALLOCATION!
}

// GOOD: Use pre-allocated pools
fn process_tick(data: TickData) {
    let order = self.order_pool.acquire(); // Pre-allocated
    // Use static errors instead of String
}
```

### 2. Clone Elimination Strategy

#### Use Copy for Small POD Types (<64 bytes)
```rust
// GOOD: Small struct, make it Copy
#[derive(Clone, Copy)]
pub struct Price {
    value: i64,
    decimals: u8,
}

// Usage - no heap allocation
let price1 = Price::new(100);
let price2 = price1; // Stack copy, ~2ns
```

#### Avoid Arc for Small Types
```rust
// BAD: Arc for 64-byte struct
let config = Arc::new(EngineConfig::default()); // 45ns overhead!

// GOOD: Make it Copy
let config = EngineConfig::default(); // 2ns copy
```

#### When to Use Arc
Only use `Arc` when ALL conditions are met:
- Data size > 1KB
- Multiple long-lived owners required
- Not accessed in hot paths
- Cloning would be expensive

```rust
// GOOD: Large, shared, infrequently accessed
struct AuthenticationState {
    credentials: Vec<u8>,        // Can be large
    certificates: Vec<Certificate>, // Expensive to clone
    session_tokens: HashMap<String, Token>,
}
let auth = Arc::new(AuthenticationState::load());
```

### 3. Memory Layout Optimization

#### Cache Line Alignment
```rust
// Align hot data structures to cache lines (64 bytes)
#[repr(C, align(64))]
pub struct MarketData {
    bid: i64,           // 8 bytes
    ask: i64,           // 8 bytes
    bid_size: u64,      // 8 bytes
    ask_size: u64,      // 8 bytes
    timestamp: u64,     // 8 bytes
    _padding: [u8; 24], // Total: 64 bytes
}
```

#### Avoid False Sharing
```rust
// BAD: Atomic counters in same cache line
struct Counters {
    orders: AtomicU64,    // Thread 1 writes
    fills: AtomicU64,     // Thread 2 writes - FALSE SHARING!
}

// GOOD: Separate cache lines
#[repr(C)]
struct Counters {
    orders: CacheAligned<AtomicU64>,
    fills: CacheAligned<AtomicU64>,
}

#[repr(C, align(64))]
struct CacheAligned<T>(T);
```

### 4. String and Error Handling

#### Use Static Errors
```rust
// BAD: String allocation on every error
fn process() -> Result<(), String> {
    Err("Market closed".to_string()) // ALLOCATION!
}

// GOOD: Static error enum
#[derive(Debug, Copy, Clone)]
enum TradingError {
    MarketClosed,
    InvalidOrder,
}

fn process() -> Result<(), TradingError> {
    Err(TradingError::MarketClosed) // No allocation
}
```

### 5. Pre-allocation Strategies

#### Object Pools
```rust
// Pre-allocate frequently used objects
let order_pool = ObjectPool::<Order>::new(10000);

// Hot path - no allocation
fn send_order(&self) {
    let mut order = self.order_pool.acquire()?;
    order.configure(...);
    // Automatically returned when dropped
}
```

#### Pre-sized Collections
```rust
// BAD: Growing vectors allocate
let mut fills = Vec::new();
fills.push(fill); // May allocate!

// GOOD: Pre-allocate capacity
let mut fills = Vec::with_capacity(100);
fills.push(fill); // No allocation until capacity exceeded
```

### 6. Benchmarking Guidelines

#### Measure Everything
```rust
#[bench]
fn bench_order_processing(b: &mut Bencher) {
    let engine = setup_engine();
    b.iter(|| {
        engine.process_order(black_box(order))
    });
}
```

#### Key Metrics
- **Tick-to-Trade Latency**: <100μs target
- **Order Processing**: <10μs per order
- **Risk Check**: <5μs per check
- **Memory Allocation**: 0 in hot paths

### 7. Lock-Free Programming

#### Use Atomic Operations
```rust
// Atomic counters for metrics
struct Metrics {
    orders: AtomicU64,
    fills: AtomicU64,
}

impl Metrics {
    fn record_order(&self) {
        self.orders.fetch_add(1, Ordering::Relaxed);
    }
}
```

#### Lock-Free Data Structures
- SPSC queues for order flow
- Lock-free pools for object reuse
- Atomic slots for market data

## Performance Checklist

Before deploying any code to production:

- [ ] Zero allocations in hot paths verified with benchmarks
- [ ] All small POD types (<64 bytes) implement Copy
- [ ] No unnecessary Arc usage
- [ ] Cache-line aligned hot data structures
- [ ] Pre-allocated all pools and buffers
- [ ] String errors replaced with enums
- [ ] Lock-free data structures where applicable
- [ ] Benchmarks show <10% variance
- [ ] Memory usage is constant under load
- [ ] No system calls in critical paths

## Profiling Tools

### Linux perf
```bash
# Record performance data
perf record -g ./target/release/shriven-quant

# Analyze hot spots
perf report
```

### Flamegraphs
```bash
# Generate flamegraph
cargo flamegraph --bin shriven-quant
```

### Memory Profiling
```bash
# Check allocations with Valgrind
valgrind --tool=massif ./target/release/shriven-quant
```

## Anti-Patterns to Avoid

1. **String formatting in hot paths**
2. **Dynamic dispatch (trait objects) in critical sections**
3. **Unnecessary cloning of large structures**
4. **Mutex/RwLock in performance-critical code**
5. **HashMap with String keys in hot paths**
6. **Unbounded growth of collections**
7. **System calls (time, random) in tight loops**
8. **Logging in hot paths**

## References
- [ADR-0007: Zero-Copy Philosophy](../architecture/decisions/0007-zero-copy-philosophy.md)
- [ADR-0006: Memory Pool Design](../architecture/decisions/0006-memory-pool-design.md)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Computer Architecture: A Quantitative Approach](https://www.elsevier.com/books/computer-architecture/hennessy/978-0-12-811905-1)