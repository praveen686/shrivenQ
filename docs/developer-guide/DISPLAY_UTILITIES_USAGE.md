# Display Utilities Usage Guide

## Overview
The `display_utils` module provides safe, documented functions for converting internal types to display formats. This guide explains when and how to use these utilities.

## Core Principle
**Display utilities are ONLY for human-readable output, NEVER for business logic.**

## Import Patterns

### Production Code
```rust
use feeds::display_utils::*;
```

### Test Code
```rust
#[cfg(test)]
use feeds::display_utils::test_utils::*;
```

### Benchmarks
```rust
// In Cargo.toml
[dev-dependencies]
feeds = { path = "../feeds", features = ["test-utils"] }

// In benchmark code
use feeds::display_utils::test_utils::*;
```

## Production Functions

### Memory Formatting
```rust
// Convert bytes to human-readable units
let memory_bytes = 1_879_048_192;
info!("Memory: {:.2} GiB", fmt_bytes_gib(memory_bytes));  // "1.75 GiB"
info!("Memory: {:.0} MiB", fmt_bytes_mib(memory_bytes));  // "1792 MiB"

// Convert KB to MB
let kb = 2048;
info!("Size: {:.1} MB", fmt_kb_to_mb(kb));  // "2.0 MB"
```

### Time Formatting
```rust
// Convert nanoseconds to seconds
let elapsed_ns = 1_500_000_000;
info!("Elapsed: {:.2}s", fmt_nanos_to_secs(elapsed_ns));  // "1.50s"
```

### Performance Metrics
```rust
// Calculate events per second
let events = 1_000_000;
let duration_secs = 2.5;
info!("Rate: {:.0} events/sec", calc_events_per_sec(events, duration_secs));

// Calculate percentage
let completed = 750;
let total = 1000;
info!("Progress: {:.1}%", calc_percentage(completed, total));  // "75.0%"
```

## Test Utilities

### Safe Index Conversions
```rust
#[test]
fn test_generate_data() {
    for i in 0..100 {
        // Safe conversions with proper error handling
        let id = index_to_u32(i);  // Panics if i > u32::MAX
        let timestamp = index_to_u64(i);  // Always safe
        let price = index_to_f64(i) * 100.0;  // Exact for i < 2^53
        
        let order = Order {
            id,
            timestamp,
            price: Px::new(price),
        };
    }
}
```

### Level Generation with Wraparound
```rust
#[test]
fn test_orderbook_levels() {
    for i in 0..1000 {
        // Generates levels 0-255 with wraparound
        let level = index_to_u8_wrapped(i);
        assert!(level < 256);
        
        // i=0 → 0, i=255 → 255, i=256 → 0, i=257 → 1
    }
}
```

### Float Assertions
```rust
#[test]
fn test_price_calculations() {
    let expected_i64 = 100_000;  // Fixed-point representation
    let actual_f64 = 100.0;
    
    // Convert for comparison with epsilon
    let expected = i64_to_f64_for_assert(expected_i64) / 1000.0;
    assert!((actual_f64 - expected).abs() < 0.001);
}
```

### Pointer Logging (NO ARITHMETIC!)
```rust
#[test]
fn test_memory_alignment() {
    let value: u64 = 42;
    let ptr = &value as *const u64;
    
    // CORRECT: Logging only
    debug!("Allocated at: 0x{:x}", addr_for_log(ptr));
    
    // CORRECT: Alignment check
    assert_eq!(alignment_of(ptr), 0, "u64 should be aligned");
    
    // WRONG: Never do arithmetic!
    // let distance = addr_for_log(ptr2) - addr_for_log(ptr1);  // BAD!
    
    // RIGHT: Use offset_from for distance
    // let distance = unsafe { ptr2.offset_from(ptr1) };
}
```

## Common Patterns

### Pattern 1: Metrics Reporting
```rust
fn report_performance(events: u64, start_time: Instant) {
    let elapsed = start_time.elapsed();
    let duration_secs = elapsed.as_secs_f64();
    
    info!("Performance Report:");
    info!("  Events: {}", events);
    info!("  Duration: {:.2}s", duration_secs);
    info!("  Rate: {:.0} events/sec", calc_events_per_sec(events, duration_secs));
}
```

### Pattern 2: Progress Tracking
```rust
fn track_progress(completed: u64, total: u64) {
    let pct = calc_percentage(completed, total);
    info!("Progress: {} of {} ({:.1}%)", completed, total, pct);
}
```

### Pattern 3: Test Data Generation
```rust
#[test]
fn test_batch_processing() {
    let orders: Vec<_> = (0..100)
        .map(|i| Order {
            id: index_to_u64(i),
            symbol: Symbol(index_to_u32(i) + 100),
            price: Px::new(index_to_f64(i) * 0.1 + 100.0),
            level: index_to_u8_wrapped(i),
        })
        .collect();
}
```

## Anti-Patterns (DON'T DO THIS!)

### ❌ Using Display Utils in Business Logic
```rust
// WRONG: Business logic using display function
fn calculate_risk(exposure: u64, limit: u64) -> f64 {
    calc_percentage(exposure, limit)  // NO! Use fixed-point
}

// RIGHT: Use proper types
fn calculate_risk(exposure: Px, limit: Px) -> Px {
    exposure.multiply_ratio(10000, limit.raw())
}
```

### ❌ Direct Casts in Production Code
```rust
// WRONG: Direct cast
let rate = events as f64 / duration;

// RIGHT: Use display utility
let rate = calc_events_per_sec(events, duration);
```

### ❌ Pointer Arithmetic via Integers
```rust
// WRONG: Integer arithmetic on pointers
let distance = (ptr2 as usize) - (ptr1 as usize);

// RIGHT: Use offset_from
let distance = unsafe { ptr2.offset_from(ptr1) };
```

### ❌ Using Test Utils in Production
```rust
// WRONG: Test utility in production
fn process_order(index: usize) {
    let id = index_to_u64(index);  // NO! Test-only
}

// RIGHT: Proper conversion
fn process_order(index: usize) -> Result<()> {
    let id = u64::try_from(index)?;
}
```

## Compliance Checklist

- [ ] Display utilities used ONLY for display/logging
- [ ] No display utilities in business logic
- [ ] Test utilities properly gated with `#[cfg(test)]`
- [ ] All pointer operations use safe APIs
- [ ] Precision loss documented where applicable
- [ ] Using `From`/`TryFrom` where possible
- [ ] Debug assertions for range validation

## References

- [ADR-0004: Display Utilities](../architecture/decisions/0004-display-utilities.md)
- [Fixed-Point Arithmetic](../architecture/decisions/0003-fixed-point-arithmetic.md)
- [Quantitative Development Best Practices](./QUANTITATIVE_DEVELOPMENT_BEST_PRACTICES.md)