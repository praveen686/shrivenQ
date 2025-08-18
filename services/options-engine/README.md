# Options Engine Service

## Overview
Options pricing engine implementing Black-Scholes model with full Greeks calculations.

## Status: ✅ Fully Implemented, ⚠️ Not Integrated

### What's Implemented
- Black-Scholes pricing model
- Full Greeks (Delta, Gamma, Theta, Vega, Rho)
- Implied volatility calculation
- American/European options
- gRPC service
- Proto definitions

### What's Missing
- Market data integration
- Real-time pricing
- Volatility surface
- Exotic options
- Monte Carlo pricing
- Binomial trees
- Historical volatility

## Pricing Models

### Black-Scholes
```rust
pub fn black_scholes(
    spot: f64,
    strike: f64,
    rate: f64,
    time: f64,
    volatility: f64,
    option_type: OptionType,
    dividend: f64,
) -> f64
```

### Greeks
- **Delta** - Price sensitivity to underlying
- **Gamma** - Delta sensitivity to underlying
- **Theta** - Time decay
- **Vega** - Volatility sensitivity
- **Rho** - Interest rate sensitivity

## API

### gRPC Endpoints

#### `CalculatePrice(PriceRequest) → PriceResponse`
Calculate option price using Black-Scholes.

```proto
message PriceRequest {
    double spot = 1;
    double strike = 2;
    double rate = 3;
    double time = 4;
    double volatility = 5;
    OptionType option_type = 6;
    double dividend = 7;
}
```

#### `CalculateGreeks(GreeksRequest) → GreeksResponse`
Calculate all Greeks for an option.

#### `CalculateImpliedVolatility(IVRequest) → IVResponse`
Calculate implied volatility from market price.

## Accuracy

### Black-Scholes Assumptions
1. No transaction costs
2. Constant risk-free rate
3. Log-normal distribution
4. No early exercise (European)
5. Constant volatility

### Limitations
- Not suitable for American options (early exercise)
- Assumes constant volatility
- No jump risk modeling
- No smile/skew handling

## Performance

```
Pricing calculation: ~100ns
Greeks calculation: ~500ns
IV calculation: ~10μs (iterative)
```

## Integration Status

| Component | Status | Notes |
|-----------|--------|-------|
| Pricing Engine | ✅ | Full implementation |
| Market Data | ❌ | Not connected |
| Risk Manager | ❌ | Not integrated |
| Trading Gateway | ❌ | Not integrated |

## Example Usage

```rust
use options_engine::{OptionsEngine, OptionType};

let engine = OptionsEngine::new();

// Price a call option
let price = engine.calculate_price(
    100.0,  // spot
    105.0,  // strike
    0.05,   // rate
    0.25,   // time (3 months)
    0.20,   // volatility
    OptionType::Call,
    0.0,    // dividend
);

// Calculate Greeks
let greeks = engine.calculate_greeks(
    100.0, 105.0, 0.05, 0.25, 0.20, OptionType::Call, 0.0
);
```

## Running

```bash
cargo run --release -p options-engine
```

Service listens on port `50055`.

## Testing

```bash
cargo test -p options-engine
```

Limited test coverage - needs expansion.

## Known Issues

1. No market data feed
2. No volatility surface
3. American options not properly handled
4. No exotic options
5. No portfolio Greeks
6. No risk scenarios
7. Not integrated with trading

## TODO

- [ ] Connect to market data
- [ ] Implement volatility surface
- [ ] Add Monte Carlo methods
- [ ] Support exotic options
- [ ] Portfolio-level Greeks
- [ ] Risk scenarios
- [ ] Performance optimization
- [ ] Integration tests