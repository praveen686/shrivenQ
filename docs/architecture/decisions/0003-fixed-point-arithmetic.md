# ADR-0003: Fixed-Point Arithmetic for Financial Calculations

## Status
Accepted

## Context
In quantitative trading systems, numerical precision is critical. Floating-point arithmetic (f32/f64) has inherent precision issues:
- Cannot exactly represent many decimal values (e.g., 0.1)
- Rounding errors accumulate over many operations
- Non-deterministic behavior across different platforms
- Silent precision loss can lead to financial losses

## Decision
We will use fixed-point arithmetic (i64 with 4 decimal places) for ALL internal financial calculations:
- Prices are stored as `Px` type (i64 ticks, 10000 ticks = 1 unit)
- Quantities are stored as `Qty` type (i64 units, 10000 units = 1 unit)
- All internal calculations use fixed-point arithmetic
- Floating-point is ONLY used at system boundaries for external API compatibility

## Consequences

### Positive
- Exact decimal representation (no rounding errors)
- Deterministic calculations across all platforms
- Fast integer arithmetic (better performance)
- No silent precision loss in calculations
- Compliant with financial system requirements

### Negative
- Conversion overhead at API boundaries
- Limited range compared to floating-point
- More complex implementation for some mathematical operations

## Implementation Details

### Internal Representation
```rust
pub struct Px(i64);  // Price in ticks (1 unit = 10000 ticks)
pub struct Qty(i64); // Quantity in units (1 unit = 10000 units)
```

### API Boundary Conversions
At system boundaries (external APIs, display), we must convert to f64:
```rust
#[allow(clippy::cast_precision_loss)]
pub fn as_f64(&self) -> f64 {
    self.0 as f64 / 10000.0
}
```

**WARNING**: This conversion may lose precision for values > 2^53 / 10000. This is acceptable ONLY at system boundaries where external systems require floating-point.

### Prohibited Patterns
- NO floating-point arithmetic for money calculations
- NO unsafe numeric casts in business logic
- NO f64/f32 types in core domain models

### Allowed Exceptions
1. External API compatibility (with explicit `#[allow]` and documentation)
2. Display/formatting for human readability
3. Scientific calculations that don't involve money (e.g., volatility forecasting)

## References
- [Fixed-Point Arithmetic in Finance](https://www.quantstart.com/articles/Fixed-Point-Arithmetic-in-Quantitative-Finance/)
- [IEEE 754 Floating-Point Issues](https://docs.oracle.com/cd/E19957-01/806-3568/ncg_goldberg.html)
- [Rust Numeric Types](https://doc.rust-lang.org/book/ch03-02-data-types.html)

## Review Date
2024-01-13

## Authors
- ShrivenQuant Team