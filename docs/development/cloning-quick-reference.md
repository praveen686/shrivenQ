# Clone vs Copy vs Arc - Quick Reference

## Decision Tree

```
Is the type < 64 bytes AND Plain Old Data?
├─ YES → Make it Copy
│        Example: EngineConfig, Price, Symbol
└─ NO → Is it shared across threads/components?
    ├─ YES → Is it large (>1KB) AND rarely accessed?
    │   ├─ YES → Use Arc
    │   │        Example: AuthenticationState, SymbolMap
    │   └─ NO → Use references (&T)
    │            Example: Passing config to functions
    └─ NO → Use move semantics
             Example: Transferring orders to pool
```

## Examples

### ✅ GOOD: Copy for Small POD
```rust
#[derive(Clone, Copy)]
pub struct Price {
    value: i64,  // 8 bytes
}

// Usage - no allocation
let p1 = Price::new(100);
let p2 = p1; // Stack copy, ~2ns
```

### ❌ BAD: Arc for Small POD
```rust
// DON'T DO THIS
let config = Arc::new(EngineConfig::default()); // 64 bytes wrapped in Arc!
```

### ✅ GOOD: Arc for Large Shared State
```rust
pub struct MarketDataCache {
    symbols: HashMap<Symbol, MarketData>, // Can be megabytes
    historical: Vec<TickData>,           // Can be gigabytes
}

let cache = Arc::new(MarketDataCache::load()); // Shared across threads
```

### ✅ GOOD: References for Function Parameters
```rust
// Pass by reference, not clone
fn process_order(config: &EngineConfig, order: &Order) {
    // Use config and order without cloning
}
```

## Performance Impact

| Operation | Copy (64B) | Arc::clone | Clone (1KB) | Borrow |
|-----------|------------|------------|-------------|--------|
| Time      | 2ns        | 15ns       | 200ns       | 0ns    |
| Memory    | Stack      | +16B heap  | +1KB heap   | 0      |
| Cache     | Hot        | Cold       | Cold        | Hot    |

## Hot Path Rules

1. **NEVER** use Arc in hot paths
2. **NEVER** clone large structures per-tick
3. **ALWAYS** use Copy for types < 64 bytes
4. **ALWAYS** prefer borrows for read-only access
5. **ALWAYS** use object pools for frequent allocations

## Common Mistakes

```rust
// ❌ Arc for config that's only 64 bytes
Arc<EngineConfig>

// ❌ Cloning on every tick
let data = market_data.clone(); // NO!

// ❌ String in errors
Err("Market closed".to_string()) // Allocates!

// ✅ Static error enum
Err(TradingError::MarketClosed) // No allocation
```

## Measurement Commands

```bash
# Check for allocations in hot paths
cargo bench --bench hot_path -- --profile-time=10

# Memory profile
valgrind --tool=massif target/release/shriven-quant

# Check struct sizes
cargo test --features size_check
```

## References
- [ADR-0007: Zero-Copy Philosophy](../architecture/decisions/0007-zero-copy-philosophy.md)
- [Performance Guidelines](../performance/guidelines.md)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)