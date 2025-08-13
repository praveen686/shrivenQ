# ADR-0004: Display Utilities for Safe Type Conversions

## Status
Accepted

## Context
The codebase needs to convert internal integer types to floating-point for display purposes (logging, metrics, UI). These conversions can lose precision but are acceptable ONLY for human-readable output, never for business logic.

## Decision
We will centralize all display-related type conversions in a `display_utils` module with clear documentation about when to use each function.

## Implementation

### Module Location
`crates/market-data/feeds/src/display_utils.rs`

### Usage Guidelines

#### WHEN TO USE
- Logging and debug output
- Metrics reporting  
- Performance statistics
- Human-readable displays
- Test assertions that compare outputs

#### WHEN NOT TO USE
- Business logic calculations
- Financial computations
- Risk calculations
- Order matching
- Position tracking
- Any logic that affects trading decisions

### Production Functions

```rust
use feeds::display_utils::*;

// Memory formatting
fmt_bytes_gib(bytes: u64) -> f64  // Format bytes as GiB
fmt_bytes_mib(bytes: u64) -> f64  // Format bytes as MiB  
fmt_kb_to_mb(kb: u64) -> f64      // Format KB as MB

// Time formatting
fmt_nanos_to_secs(nanos: u64) -> f64  // Format nanoseconds as seconds

// Performance metrics
calc_events_per_sec(events: u64, duration_secs: f64) -> f64
calc_percentage(part: u64, total: u64) -> f64
```

### Test Utilities

Test utilities are gated behind `#[cfg(test)]` or the `test-utils` feature:

```rust
#[cfg(test)]
use feeds::display_utils::test_utils::*;

// Index conversions with safety checks
index_to_u32(index: usize) -> u32        // Panics if > u32::MAX
index_to_u64(index: usize) -> u64        // Lossless on all platforms
index_to_f64(index: usize) -> f64        // Exact up to 2^53

// Wraparound for level generation
index_to_u8_wrapped(index: usize) -> u8  // Wraps at 256 (index % 256)

// Float conversion for assertions
i64_to_f64_for_assert(value: i64) -> f64 // Exact for |value| < 2^53

// Pointer utilities (logging only, no arithmetic!)
addr_for_log<T>(ptr: *const T) -> usize  // Format address for logs
alignment_of<T>(ptr: *const T) -> usize  // Check alignment
```

### Safety Rules

1. **Test utilities are TEST-ONLY**: Never use in production paths
2. **Prefer `From`/`TryFrom`**: Use standard conversions where possible
3. **Document precision loss**: Always note when precision is lost
4. **No pointer arithmetic**: Use `ptr::offset_from` for distance calculations
5. **Check boundaries**: Use debug_assert! for range validation

### Example Usage

```rust
// CORRECT: Display-only usage
use feeds::display_utils::*;

info!("Memory usage: {:.2} GiB", fmt_bytes_gib(memory_bytes));
info!("Processing rate: {:.0} events/sec", calc_events_per_sec(count, elapsed));

// INCORRECT: Business logic usage  
let risk = calc_percentage(exposure, limit); // WRONG! Use fixed-point
let price = test_index_to_f64(i) * 100.0;    // WRONG! Use Px type
```

## Consequences

### Positive
- Centralized, documented conversion functions
- Clear separation between display and business logic
- Reduced unsafe casts throughout codebase
- Consistent formatting across all modules

### Negative  
- Must import display_utils for formatting
- Developers must understand when to use vs not use
- Extra function call overhead (negligible for display)

## Compliance
All modules MUST use display_utils for display conversions. Direct casts like `as f64` are prohibited except within the display_utils module itself.