# Quantitative Development Best Practices
## DO's and DON'Ts for Low Latency Trading Systems

> **Critical Warning**: This document contains industry-standard practices for ultra-low latency quantitative trading systems. Violations can result in significant financial losses, regulatory issues, and system failures.

---

## Table of Contents
1. [Performance & Latency](#performance--latency)
2. [Memory Management](#memory-management)
3. [Code Quality & Safety](#code-quality--safety)
4. [Concurrency & Threading](#concurrency--threading)
5. [Data Handling & Serialization](#data-handling--serialization)
6. [Testing & Validation](#testing--validation)
7. [Configuration & Deployment](#configuration--deployment)
8. [Pre-commit Optimization](#pre-commit-optimization)
9. [AI Agent Development](#ai-agent-development)
10. [Compilation Time Optimization](#compilation-time-optimization)

---

## Performance & Latency

### ✅ DO's - Performance

```rust
// ✅ DO: Use stack allocation for hot path structures
#[repr(C)]
struct OrderBookLevel {
    price: i64,    // Fixed-point arithmetic
    quantity: u64,
    count: u32,
}

// ✅ DO: Pre-allocate collections with known capacity
let mut orders = Vec::with_capacity(1000);
let mut prices = HashMap::with_capacity_and_hasher(500, FxBuildHasher);

// ✅ DO: Use branch prediction hints
if likely!(price > 0) {
    process_order();
}

// ✅ DO: Minimize system calls in hot paths
static mut BUFFER: [u8; 4096] = [0; 4096];
unsafe {
    let bytes_written = write_to_buffer(&mut BUFFER, data);
}

// ✅ DO: Use const generics for compile-time optimization
fn process_levels<const N: usize>(levels: &[PriceLevel; N]) {
    // Compiler knows N at compile time
}

// ✅ DO: Inline critical functions
#[inline(always)]
fn calculate_mid_price(bid: Price, ask: Price) -> Price {
    (bid + ask) >> 1  // Fast division by 2
}
```

### ❌ DON'T's - Performance

```rust
// ❌ DON'T: Allocate in hot paths
fn process_tick() {
    let temp_vec = Vec::new();  // NEVER do this in hot paths
    let temp_string = String::new();  // Heap allocation
}

// ❌ DON'T: Use floating-point arithmetic for money
let price = 123.45_f64;  // Precision errors!
let quantity = price * 1000.0;  // Rounding errors!

// ❌ DON'T: Use std::collections::HashMap in hot paths
use std::collections::HashMap;  // Too slow
let mut map = HashMap::new();

// ❌ DON'T: Create temporary objects unnecessarily
fn get_symbol_name(symbol: &Symbol) -> String {
    symbol.name.clone()  // Unnecessary allocation
}

// ❌ DON'T: Use println! or format! in production hot paths
println!("Processing order: {}", order_id);  // I/O in hot path
let msg = format!("Price: {}", price);  // String allocation
```

---

## Memory Management

### ✅ DO's - Memory

```rust
// ✅ DO: Use object pools for frequent allocations
struct OrderPool {
    orders: Vec<Order>,
    free_indices: Vec<usize>,
}

impl OrderPool {
    fn acquire(&mut self) -> Option<&mut Order> {
        self.free_indices.pop().map(|idx| &mut self.orders[idx])
    }
}

// ✅ DO: Use arena allocation for related objects
use typed_arena::Arena;
let arena = Arena::new();
let nodes: Vec<&Node> = (0..1000)
    .map(|_| arena.alloc(Node::new()))
    .collect();

// ✅ DO: Use zero-copy deserialization
#[derive(serde::Deserialize)]
struct MarketData<'a> {
    #[serde(borrow)]
    symbol: &'a str,
    price: u64,
}

// ✅ DO: Implement custom Drop for cleanup
impl Drop for OrderBook {
    fn drop(&mut self) {
        // Critical cleanup logic
        self.flush_pending_orders();
    }
}
```

### ❌ DON'T's - Memory

```rust
// ❌ DON'T: Use Box/Rc/Arc in hot paths unnecessarily
let order = Box::new(Order::new());  // Heap allocation
let shared = Rc::new(data);  // Reference counting overhead

// ❌ DON'T: Clone large structures
let copied_book = order_book.clone();  // Expensive copy

// ❌ DON'T: Use String when &str suffices
fn process_symbol(symbol: String) {  // Takes ownership
    // ...
}
// Better:
fn process_symbol(symbol: &str) {
    // ...
}

// ❌ DON'T: Ignore memory leaks
fn create_orders() {
    let orders = Vec::new();
    // Forgot to store/cleanup orders - leak!
}
```

---

## Code Quality & Safety

### ✅ DO's - Quality

```rust
// ✅ DO: Use type safety for domain concepts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Price(i64);  // Fixed-point price

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Quantity(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OrderId(u64);

// ✅ DO: Use const assertions for critical invariants
const _: () = assert!(std::mem::size_of::<Order>() <= 64);

// ✅ DO: Document performance characteristics
/// O(1) insertion into order book level
/// # Safety
/// Caller must ensure price is valid
#[inline]
pub unsafe fn insert_order_unchecked(&mut self, price: Price, qty: Quantity) {
    // ...
}

// ✅ DO: Use error types, not panics
#[derive(Debug, thiserror::Error)]
enum TradingError {
    #[error("Invalid price: {price}")]
    InvalidPrice { price: i64 },
    #[error("Insufficient liquidity")]
    InsufficientLiquidity,
}

// ✅ DO: Validate all external inputs
fn process_order(raw_order: &[u8]) -> Result<Order, TradingError> {
    if raw_order.len() < MIN_ORDER_SIZE {
        return Err(TradingError::InvalidOrderSize);
    }
    // Validate all fields...
}
```

### ❌ DON'T's - Quality

```rust
// ❌ DON'T: Use unwrap() in production code
let price = order.price().unwrap();  // Will panic!

// ❌ DON'T: Use panic! for error handling
if price < 0 {
    panic!("Negative price!");  // Crashes entire system
}

// ❌ DON'T: Ignore compiler warnings
#[allow(unused_variables)]  // Don't suppress warnings
fn process_data(data: MarketData) {
    // Unused variable indicates possible bug
}

// ❌ DON'T: Use magic numbers
let adjusted_price = price * 10000;  // What is 10000?
// Better:
const PRICE_MULTIPLIER: i64 = 10000;
let adjusted_price = price * PRICE_MULTIPLIER;

// ❌ DON'T: Mix business logic with I/O
fn calculate_risk_and_log(position: Position) -> Risk {
    let risk = calculate_risk(position);
    println!("Risk calculated: {}", risk);  // I/O mixed with calculation
    risk
}
```

---

## Concurrency & Threading

### ✅ DO's - Concurrency

```rust
// ✅ DO: Use lock-free data structures when possible
use crossbeam::queue::ArrayQueue;
let queue = ArrayQueue::<Order>::new(1000);

// ✅ DO: Use thread-local storage for hot data
thread_local! {
    static ORDER_BUFFER: RefCell<Vec<Order>> = RefCell::new(Vec::with_capacity(1000));
}

// ✅ DO: Pin threads to CPU cores
use core_affinity;
let core_ids = core_affinity::get_core_ids().unwrap();
core_affinity::set_for_current(core_ids[0]);

// ✅ DO: Use parking_lot for faster mutexes
use parking_lot::Mutex;
let data = Mutex::new(OrderBook::new());

// ✅ DO: Minimize critical sections
{
    let mut book = order_book.lock();
    book.add_order(order);  // Minimal work in lock
}
// Heavy computation outside lock
let risk = calculate_risk(&order);
```

### ❌ DON'T's - Concurrency

```rust
// ❌ DON'T: Hold locks while doing I/O
let mut book = order_book.lock();
book.add_order(order);
log_to_file("Order added");  // I/O while holding lock!

// ❌ DON'T: Use std::sync::Mutex in hot paths
use std::sync::Mutex;  // Slower than parking_lot
let data = Mutex::new(data);

// ❌ DON'T: Create threads in hot paths
fn process_order(order: Order) {
    std::thread::spawn(|| {  // Thread creation overhead
        validate_order(order);
    });
}

// ❌ DON'T: Use channels for single producer/consumer
use std::sync::mpsc;
let (tx, rx) = mpsc::channel();  // Unnecessary overhead
```

---

## Data Handling & Serialization

### ✅ DO's - Data

```rust
// ✅ DO: Use binary serialization for performance
use bincode;
let serialized = bincode::serialize(&order)?;
let deserialized: Order = bincode::deserialize(&data)?;

// ✅ DO: Use zero-copy parsing
use zerocopy::{AsBytes, FromBytes};
#[derive(FromBytes, AsBytes)]
#[repr(C)]
struct MarketTick {
    timestamp: u64,
    symbol: [u8; 8],
    price: u64,
    quantity: u64,
}

// ✅ DO: Version your data structures
#[derive(Serialize, Deserialize)]
struct OrderV1 {
    #[serde(default)]
    version: u32,  // Always include version
    id: OrderId,
    price: Price,
    quantity: Quantity,
}

// ✅ DO: Use fixed-size arrays when possible
struct PriceLevels {
    levels: [PriceLevel; 10],  // Fixed size, stack allocated
}

// ✅ DO: Validate deserialized data
fn deserialize_order(data: &[u8]) -> Result<Order, Error> {
    let order: Order = bincode::deserialize(data)?;
    if !order.is_valid() {
        return Err(Error::InvalidOrder);
    }
    Ok(order)
}
```

### ❌ DON'T's - Data

```rust
// ❌ DON'T: Use JSON for high-frequency data
let json = serde_json::to_string(&order)?;  // Slow text parsing

// ❌ DON'T: Use HashMap<String, _> for known keys
let mut data = HashMap::<String, f64>::new();  // String keys are slow

// ❌ DON'T: Ignore endianness for network protocols
let price = u64::from_be_bytes(bytes);  // Specify byte order

// ❌ DON'T: Use Vec<T> when [T; N] suffices
struct OrderBook {
    levels: Vec<PriceLevel>,  // If size is known, use array
}
```

---

## Testing & Validation

### ✅ DO's - Testing

```rust
// ✅ DO: Write property-based tests for critical functions
use proptest::prelude::*;

proptest! {
    #[test]
    fn price_calculation_never_overflows(
        bid in 0i64..i64::MAX/2,
        ask in 0i64..i64::MAX/2
    ) {
        let mid = calculate_mid_price(Price(bid), Price(ask));
        assert!(mid.0 >= bid && mid.0 <= ask);
    }
}

// ✅ DO: Test with realistic market data
#[test]
fn test_with_real_market_conditions() {
    let market_data = load_historical_data("BTCUSDT_2024_01_01.bin");
    let mut book = OrderBook::new();

    for tick in market_data {
        book.apply_update(tick);
        assert!(book.is_consistent());
    }
}

// ✅ DO: Benchmark critical paths
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_order_insertion(c: &mut Criterion) {
    let mut book = OrderBook::new();
    c.bench_function("insert_order", |b| {
        b.iter(|| {
            book.insert_order(black_box(create_test_order()));
        });
    });
}

// ✅ DO: Test error conditions
#[test]
fn test_invalid_order_rejection() {
    let mut book = OrderBook::new();
    let invalid_order = Order {
        price: Price(-1),  // Invalid negative price
        ..Default::default()
    };

    assert!(matches!(
        book.add_order(invalid_order),
        Err(TradingError::InvalidPrice { .. })
    ));
}
```

### ❌ DON'T's - Testing

```rust
// ❌ DON'T: Only test happy paths
#[test]
fn test_only_valid_orders() {
    // Missing: invalid orders, edge cases, error conditions
}

// ❌ DON'T: Use floating point for test assertions
assert_eq!(calculated_price, 123.45);  // Floating point comparison

// ❌ DON'T: Ignore timing in tests
#[test]
fn test_performance() {
    let start = std::time::Instant::now();
    process_orders();
    // No assertion on timing!
}

// ❌ DON'T: Test implementation details
#[test]
fn test_internal_hash_map() {
    assert_eq!(book.internal_map.len(), 5);  // Testing internals
}
```

---

## Configuration & Deployment

### ✅ DO's - Config

```toml
# ✅ DO: Use environment-specific configs
[dev]
latency_target_ns = 100_000  # 100μs for development
log_level = "debug"

[prod]
latency_target_ns = 10_000   # 10μs for production
log_level = "error"

[risk_limits]
max_position_size = 1_000_000
max_daily_loss = 50_000
```

```rust
// ✅ DO: Validate configuration at startup
#[derive(serde::Deserialize)]
struct TradingConfig {
    max_position: u64,
    risk_limit: u64,
}

impl TradingConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.max_position == 0 {
            return Err(ConfigError::InvalidMaxPosition);
        }
        if self.risk_limit < self.max_position {
            return Err(ConfigError::RiskLimitTooLow);
        }
        Ok(())
    }
}

// ✅ DO: Use const for compile-time constants
const MAX_ORDER_SIZE: usize = 1024;
const TICK_SIZE: Price = Price(1);
```

### ❌ DON'T's - Config

```rust
// ❌ DON'T: Hard-code configuration values
fn calculate_risk() -> f64 {
    position_size * 0.05  // Magic number!
}

// ❌ DON'T: Change config during runtime
static mut RISK_MULTIPLIER: f64 = 1.0;  // Mutable global state

// ❌ DON'T: Ignore configuration validation
let config: Config = toml::from_str(&config_str).unwrap();  // No validation
```

---

## Pre-commit Optimization

### ✅ DO's - Pre-commit Speed

```bash
# ✅ DO: Use incremental compilation
export CARGO_INCREMENTAL=1
export RUSTC_WRAPPER=sccache  # Cache compilation

# ✅ DO: Optimize clippy for speed
cargo clippy --all-targets --all-features -- \
    -D warnings \
    -A clippy::multiple_crate_versions  # Skip slow checks

# ✅ DO: Use parallel test execution
cargo test --jobs $(nproc)

# ✅ DO: Cache dependencies
# Use sccache or equivalent for Rust compilation caching
```

```yaml
# ✅ DO: Optimize pre-commit hook order (fastest first)
repos:
  - repo: https://github.com/pre-commit/pre-commit-hooks
    rev: v4.5.0
    hooks:
      - id: check-yaml        # Fast
      - id: check-toml        # Fast
      - id: trailing-whitespace  # Fast

  - repo: local
    hooks:
      - id: rust-fmt          # Medium speed
      - id: cargo-check       # Slower
      - id: clippy-strict     # Slowest
```

### ❌ DON'T's - Pre-commit Speed

```bash
# ❌ DON'T: Run full clean builds
cargo clean && cargo build  # Throws away incremental compilation

# ❌ DON'T: Run unnecessary checks in sequence
cargo fmt --check
cargo clippy
cargo test
cargo doc  # Could be parallel

# ❌ DON'T: Check entire codebase for small changes
cargo clippy --all-targets --all-features  # On single file change
```

---

## AI Agent Development

### ⚠️ Agent Anti-Patterns - Critical Violations

**These are INSTANT REJECTION patterns commonly used by AI agents:**

```rust
// ❌ LAZY UNDERSCORE ABUSE - Don't silence compiler with underscore prefixes
let _unused_data = expensive_calculation();  // Compiler warning ignored
let _result = process_order(order);  // Value completely ignored

// ✅ CORRECT - Handle properly or use explicit allow
#[allow(unused_variables)]  // With clear justification
let calculation_result = expensive_calculation();
// OR
let _guard = mutex.lock();  // Legitimate underscore for guard/phantom types

// ❌ SHORTCUT COMMENTS - Don't leave unfinished work
fn calculate_risk() -> f64 {
    // TODO: implement proper risk calculation
    0.0  // Placeholder return
}

// ✅ CORRECT - Complete implementation or create issue
fn calculate_risk() -> Result<RiskScore, RiskError> {
    // Issue #123: Implement VaR calculation
    unimplemented!("Risk calculation pending - see issue #123")
}

// ❌ CLONE() SHORTCUTS - Don't clone to avoid borrow checker
fn process_orders(orders: &[Order]) {
    for order in orders.clone() {  // Expensive clone to avoid borrowing
        process_order(order);
    }
}

// ✅ CORRECT - Use proper borrowing
fn process_orders(orders: &[Order]) {
    for order in orders.iter() {  // Zero-cost iteration
        process_order(order);
    }
}

// ❌ GENERIC SHORTCUTS - Don't over-generify simple functions
fn add_numbers<T: Add<Output = T> + Copy>(a: T, b: T) -> T {
    a + b  // Unnecessarily generic
}

// ✅ CORRECT - Use specific types for domain logic
fn add_prices(a: Price, b: Price) -> Price {
    Price(a.0 + b.0)  // Domain-specific, type-safe
}

// ❌ STRING ALLOCATION SHORTCUTS - Don't allocate strings unnecessarily
fn get_symbol_name(symbol: &Symbol) -> String {
    symbol.name.to_string()  // Unnecessary allocation
}

// ✅ CORRECT - Return borrowed string when possible
fn get_symbol_name(symbol: &Symbol) -> &str {
    &symbol.name  // Zero-copy access
}

// ❌ COLLECTION SHORTCUTS - Don't collect unnecessarily
let prices: Vec<Price> = orders.iter()
    .map(|o| o.price)
    .collect();  // Collect just to iterate again
for price in prices {
    process_price(price);
}

// ✅ CORRECT - Use iterators directly
orders.iter()
    .map(|o| o.price)
    .for_each(process_price);  // No intermediate collection

// ❌ DEFAULT SHORTCUTS - Don't use Default for complex initialization
let config = Config::default();  // Unclear what defaults are set

// ✅ CORRECT - Explicit initialization
let config = Config {
    max_position_size: Quantity(1_000_000),
    risk_limit_percent: RiskPercent::from_basis_points(500),
    latency_budget: Duration::from_micros(10),
};

// ❌ ANYHOW SHORTCUTS - Don't use anyhow for specific errors
use anyhow::Result;
fn validate_order(order: &Order) -> Result<()> {  // Generic error
    // ...
}

// ✅ CORRECT - Specific error types
#[derive(Debug, thiserror::Error)]
enum OrderValidationError {
    #[error("Price {price} below minimum {min}")]
    PriceTooLow { price: Price, min: Price },
}

fn validate_order(order: &Order) -> Result<(), OrderValidationError> {
    // ...
}

// ❌ PATTERN MATCHING SHORTCUTS - Don't ignore error details
match result {
    Ok(value) => value,
    Err(_) => return Default::default(),  // Ignores error completely
}

// ✅ CORRECT - Handle errors appropriately
match result {
    Ok(value) => value,
    Err(OrderError::InsufficientFunds) => {
        log::warn!("Insufficient funds for order");
        return Err(ProcessingError::InsufficientFunds);
    }
    Err(e) => {
        log::error!("Order processing failed: {}", e);
        return Err(ProcessingError::OrderValidation(e));
    }
}
```

### ✅ DO's - AI Agent Guidelines

```rust
// ✅ DO: Create AI-friendly documentation
/// # AI Agent Instructions
/// This function calculates mid-price with these constraints:
/// - Input: bid and ask prices as fixed-point integers
/// - Output: mid-price, guaranteed to be between bid and ask
/// - Performance: Must complete in <10ns
/// - Safety: Never panics, always returns valid price
#[inline(always)]
pub fn calculate_mid_price(bid: Price, ask: Price) -> Price {
    debug_assert!(bid.0 > 0);
    debug_assert!(ask.0 >= bid.0);
    Price((bid.0 + ask.0) / 2)
}

// ✅ DO: Use explicit error types for AI understanding
#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    #[error("Price {price} is below minimum tick size {min_tick}")]
    PriceBelowTickSize { price: Price, min_tick: Price },

    #[error("Quantity {qty} exceeds maximum {max_qty}")]
    QuantityTooLarge { qty: Quantity, max_qty: Quantity },
}

// ✅ DO: Provide clear examples in docs
/// # Examples for AI Agents
/// ```rust
/// let order = Order::new(OrderId(1), Price(10000), Quantity(100))?;
/// let result = order_book.add_order(order);
/// match result {
///     Ok(trades) => process_trades(trades),
///     Err(OrderError::PriceBelowTickSize { .. }) => reject_order(),
///     Err(e) => log_error(e),
/// }
/// ```
```

### ❌ DON'T's - AI Agent Guidelines

```rust
// ❌ DON'T: Write unclear or ambiguous code
fn proc(d: &[u8]) -> i32 {  // Unclear function name and return type
    // AI agents can't understand this
}

// ❌ DON'T: Use complex macros without documentation
macro_rules! complex_trading_logic {
    // 50 lines of undocumented macro magic
}

// ❌ DON'T: Hide critical business logic in unsafe blocks
unsafe {
    // Critical trading logic here without explanation
    std::ptr::write(ptr, value);
}
```

---

## Compilation Time Optimization

### ✅ DO's - Compilation Speed

```rust
// ✅ DO: Use feature flags to reduce compilation
[features]
default = ["basic"]
basic = []
advanced = ["serde", "tokio"]
development = ["advanced", "debug-tools"]

// ✅ DO: Split large modules
mod order_book;      // Separate compilation unit
mod risk_engine;     // Separate compilation unit
mod market_data;     // Separate compilation unit

// ✅ DO: Use type aliases to reduce monomorphization
type FastHashMap<K, V> = std::collections::HashMap<K, V, FxBuildHasher>;
type OrderMap = FastHashMap<OrderId, Order>;

// ✅ DO: Prefer &str over String in function signatures
fn process_symbol(symbol: &str) {  // No String allocation needed
    // ...
}

// ✅ DO: Use workspace dependencies
[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.0", features = ["rt-multi-thread"] }
```

### ❌ DON'T's - Compilation Speed

```rust
// ❌ DON'T: Use generic functions unnecessarily
fn process<T: Serialize + DeserializeOwned + Clone + Debug>(data: T) {
    // Creates many monomorphized versions
}

// ❌ DON'T: Import entire crates
use serde::*;  // Imports everything
use std::collections::*;  // Slows compilation

// ❌ DON'T: Use deeply nested generic types
type ComplexType = HashMap<String, Vec<Option<Result<Order, Box<dyn Error>>>>>;

// ❌ DON'T: Put everything in lib.rs
// lib.rs with 10,000 lines - slows incremental compilation
```

---

## Performance Monitoring & Profiling

### ✅ DO's - Monitoring

```rust
// ✅ DO: Use compile-time performance budgets
const _: () = {
    assert!(std::mem::size_of::<Order>() <= 64);  // Memory budget
    assert!(std::mem::align_of::<Order>() == 8);  // Alignment requirement
};

// ✅ DO: Instrument critical paths
use tracing::{instrument, info_span};

#[instrument(skip(self))]
fn process_order(&mut self, order: Order) -> Result<Vec<Trade>, OrderError> {
    let _span = info_span!("process_order", order_id = order.id.0).entered();
    // Implementation
}

// ✅ DO: Use static assertions for performance invariants
use static_assertions::*;
assert_eq_size!(Order, [u8; 64]);  // Ensure Order is exactly 64 bytes
assert_eq_align!(Order, u64);      // Ensure proper alignment

// ✅ DO: Profile in production-like conditions
#[cfg(feature = "profiling")]
fn profile_order_processing() {
    use pprof::ProfilerGuard;
    let guard = ProfilerGuard::new(100).unwrap();

    // Run realistic workload
    for _ in 0..1_000_000 {
        process_order(generate_realistic_order());
    }

    // Generate flamegraph
    if let Ok(report) = guard.report().build() {
        let file = File::create("flamegraph.svg").unwrap();
        report.flamegraph(file).unwrap();
    }
}
```

### ❌ DON'T's - Monitoring

```rust
// ❌ DON'T: Profile with debug builds
// Always use --release for performance testing

// ❌ DON'T: Ignore memory allocations in profiles
fn hot_function() {
    let temp = Vec::new();  // Allocation in hot path - will show in profiler
    // ...
}

// ❌ DON'T: Use println! for performance logging
println!("Processing took: {:?}", elapsed);  // Too slow for hot paths
```

---

## Risk Management & Safety

### ✅ DO's - Risk Management

```rust
// ✅ DO: Implement circuit breakers
struct CircuitBreaker {
    failure_count: AtomicU32,
    last_failure: AtomicU64,
    threshold: u32,
    timeout: Duration,
}

impl CircuitBreaker {
    fn call<F, R>(&self, f: F) -> Result<R, CircuitBreakerError>
    where
        F: FnOnce() -> Result<R, Box<dyn Error>>,
    {
        if self.is_open() {
            return Err(CircuitBreakerError::Open);
        }
        // Execute function...
    }
}

// ✅ DO: Validate all risk parameters
#[derive(Debug)]
struct RiskLimits {
    max_position_size: Quantity,
    max_order_size: Quantity,
    max_daily_loss: Money,
}

impl RiskLimits {
    fn validate_order(&self, order: &Order, current_position: Position) -> Result<(), RiskError> {
        if order.quantity > self.max_order_size {
            return Err(RiskError::OrderSizeExceeded);
        }

        let new_position = current_position.apply_order(order);
        if new_position.size() > self.max_position_size {
            return Err(RiskError::PositionLimitExceeded);
        }

        Ok(())
    }
}

// ✅ DO: Implement graceful degradation
fn process_market_data(data: MarketData) -> Result<(), ProcessingError> {
    match try_fast_path(data) {
        Ok(result) => Ok(result),
        Err(FastPathError::Overloaded) => {
            // Fall back to slower but reliable method
            warn!("Fast path overloaded, using fallback");
            fallback_processing(data)
        }
        Err(e) => Err(ProcessingError::from(e)),
    }
}
```

### ❌ DON'T's - Risk Management

```rust
// ❌ DON'T: Ignore position limits
fn execute_order(order: Order) {
    // Execute without checking risk limits - DANGEROUS!
    send_to_exchange(order);
}

// ❌ DON'T: Use floating point for money calculations
let profit_loss = buy_price * quantity - sell_price * quantity;  // Precision errors

// ❌ DON'T: Process orders without validation
fn process_external_order(raw_data: &[u8]) {
    let order = unsafe { std::mem::transmute(raw_data) };  // NEVER do this
    execute_order(order);
}
```

---

## Summary: Quick Reference

### 🔥 Critical Performance Rules
1. **No allocations in hot paths**
2. **Use fixed-point arithmetic for money**
3. **Pin threads to CPU cores**
4. **Pre-allocate all collections**
5. **Use lock-free data structures**

### 🛡️ Critical Safety Rules
1. **Validate all external inputs**
2. **Never panic in production code**
3. **Always check risk limits**
4. **Use type-safe wrappers for domain types**
5. **Test error conditions extensively**

### ⚡ Pre-commit Speed Rules
1. **Use incremental compilation (sccache)**
2. **Run checks in parallel**
3. **Order hooks by speed (fast first)**
4. **Cache compilation artifacts**
5. **Only check changed files when possible**

### 🤖 AI Agent Rules
1. **Document all performance requirements**
2. **Use explicit error types**
3. **Provide clear code examples**
4. **Avoid complex macros**
5. **Keep business logic visible**

---

## Enforcement via Pre-commit Hooks

The ShrivenQuant pre-commit configuration automatically enforces these rules:

- **Rust formatting**: Ensures consistent style
- **Ultra-strict Clippy**: Catches performance and safety issues
- **Dead code detection**: Prevents unused code
- **Security audit**: Checks for vulnerabilities
- **Documentation coverage**: Ensures all APIs are documented
- **Performance regression**: Prevents performance degradation
- **Hot path allocation check**: Prevents allocations in critical paths
- **API compatibility**: Ensures backward compatibility
- **Risk limits validation**: Validates trading parameters

**Violation of these rules will result in commit rejection.**

---

*Last updated: 2024-12-30*  
*Version: 1.0*  
*Status: Production Ready*
