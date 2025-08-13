# ADR-0005: Numeric Policy for Feature Calculations

## Status
Accepted

## Context
Feature calculations in quantitative trading require complex mathematical operations:
- Volatility calculations (standard deviation, GARCH)
- Correlation matrices
- Regression coefficients
- Technical indicators (EMA, Bollinger Bands)
- Microstructure metrics (VPIN, Kyle's Lambda)

## Numeric Policy

All prices, quantities, notional, and P&L are stored and validated in fixed-point integers (ticks/lots/min currency units). For analytics/feature computation (e.g., volatility, correlations, regressions), we convert to f64 at the analysis boundary using dedicated helpers. Floating-point values are not used for order-book math, risk limits, or order placement. Any value that re-enters execution is re-quantized to tick/lot granularity with checked rounding and bounds.

## Guardrails

- **Scope**: Conversion helpers live in an analytics/features module; don't sprinkle casts across the codebase.

- **Determinism**: Same inputs → same outputs. Record the feature config and seed in the run manifest.

- **Range checks**: Before casting to f64, assert the fixed-point magnitude is below ~9e15 (the exact-integer limit of f64). If larger (e.g., big notionals), use a decimal type for that computation.

- **Rounding policy**: When converting analytics outputs back to trading numbers, document the rounding (floor to tick for prices, min of requested vs allowed size for qty, etc.).

- **NaN/Inf hygiene**: Clamp or reject; never let NaN/Inf propagate to decisions. Log with context.

- **Algorithm choice**: Use numerically stable methods (e.g., Welford or pairwise sums for variance/covariance); avoid naive mean(x^2)-mean(x)^2.

- **Tests**: Property tests that requantize(from_f64(to_f64(x))) == x for representable ranges; fuzz for boundary values and tick/lot edges.

- **Linting**: Keep #[allow(clippy::cast_precision_loss)] only on the conversion helpers (or the analytics module), not globally.

## Implementation

```rust
// Example: Calculate volatility
fn calculate_volatility_fixed(returns: &[i64]) -> i64 {
    // Convert fixed-point returns to f64 for std dev calculation
    let returns_f64: Vec<f64> = returns.iter()
        .map(|&r| r as f64 / 10000.0)
        .collect();
    
    // Calculate standard deviation in f64
    let mean = returns_f64.iter().sum::<f64>() / returns_f64.len() as f64;
    let variance = returns_f64.iter()
        .map(|r| (r - mean).powi(2))
        .sum::<f64>() / returns_f64.len() as f64;
    let std_dev = variance.sqrt();
    
    // Convert back to fixed-point
    (std_dev * 10000.0).round() as i64
}
```

## Trade-offs

### Why This Is Acceptable
1. **Feature calculations are read-only** - they don't modify order book state
2. **Features are indicators** - used for signals, not direct execution
3. **Time budget allows it** - feature calc has ~100μs budget vs ~1μs for order matching
4. **Mathematical necessity** - some calculations (sqrt, log) require floating-point

### Mitigations
1. All conversions are documented
2. Boundary is clearly defined
3. Critical paths remain fixed-point
4. Features are pre-calculated when possible

## Consequences

### Positive
- Complex features remain implementable
- Statistical calculations are accurate
- Performance targets still met
- Code remains maintainable

### Negative
- Type conversions at boundaries
- Potential precision loss (acceptable for features)
- Must carefully track conversion points

## Alternatives Considered
1. **Pure fixed-point**: Would require custom math library, too complex
2. **Pure floating-point**: Unacceptable for critical paths
3. **Decimal types**: Too slow for our latency requirements

## Compliance
All f64 conversions in feature calculations must:
1. Be documented with rationale
2. Have fixed-point input/output
3. Not be in order matching path
4. Have bounded precision loss