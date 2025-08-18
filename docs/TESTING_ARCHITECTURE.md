# 🧪 ShrivenQuant Testing Architecture

**Last Updated**: January 18, 2025  
**Version**: 1.0  
**Status**: Production-Grade Testing Framework

---

## 📋 Overview

ShrivenQuant uses a comprehensive, production-grade testing architecture designed for financial systems reliability. Our testing framework emphasizes:

- **Zero unwrap() in production code** - All error handling uses Result<T, E>
- **Complete test isolation** - Tests never affect production code metrics
- **Property-based testing** - Ensure correctness across all input ranges
- **Performance validation** - Sub-millisecond latency requirements
- **Deterministic testing** - Reproducible test results

---

## 🏗️ Testing Structure

```
/ShrivenQuant/
├── tests/
│   ├── test-utils/           # Shared testing utilities
│   │   ├── src/
│   │   │   ├── fixtures.rs   # Test data fixtures
│   │   │   ├── factories.rs  # Test data factories
│   │   │   ├── mocks.rs      # Mock services
│   │   │   ├── helpers.rs    # Test helpers
│   │   │   └── assertions.rs # Custom assertions
│   │   └── Cargo.toml
│   ├── integration/          # Integration tests
│   ├── unit/                 # Shared unit tests
│   ├── performance/          # Performance tests
│   └── property/             # Property-based tests
└── services/
    └── {service-name}/
        └── tests/
            ├── unit/         # Service-specific unit tests
            └── integration/  # Service-specific integration tests
```

---

## 🛠️ Testing Stack

### Core Testing Frameworks

- **rstest** (v0.23) - Fixture-based testing with parameterization
- **proptest** (v1.6) - Property-based testing
- **criterion** (v0.5) - Benchmarking and performance testing
- **mockall** (v0.13) - Automatic mock generation
- **wiremock** (v0.6) - HTTP mocking
- **insta** (v1.41) - Snapshot testing

### Testing Utilities

- **fake** (v2.10) - Test data generation
- **test-case** (v3.1) - Test case generation
- **pretty_assertions** (v1.4) - Better assertion output
- **arbitrary** (v1.4) - Arbitrary data generation
- **quickcheck** (v1.0) - QuickCheck-style testing

---

## 📝 Testing Guidelines

### 1. NO unwrap() in Production Code

```rust
// ❌ NEVER DO THIS in production code
let value = some_result.unwrap();

// ✅ DO THIS instead
let value = some_result?;
// or
let value = some_result.map_err(|e| CustomError::from(e))?;
```

### 2. Use Fixtures for Test Data

```rust
use rstest::*;
use test_utils::*;

#[rstest]
fn test_order_processing(
    #[from(market_data)] market: MarketDataFixture,
    #[from(order_data)] order: OrderFixture,
) {
    // Test implementation
}
```

### 3. Property-Based Testing for Critical Logic

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_order_validation(
        quantity in 0.0001f64..10000.0,
        price in 0.01f64..1000000.0,
    ) {
        let order = create_order(quantity, price);
        prop_assert!(validate_order(&order).is_ok());
    }
}
```

### 4. Performance Testing with Criterion

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_orderbook_update(c: &mut Criterion) {
    c.bench_function("orderbook_update", |b| {
        b.iter(|| {
            update_orderbook(black_box(&update))
        });
    });
}

criterion_group!(benches, benchmark_orderbook_update);
criterion_main!(benches);
```

### 5. Mock External Dependencies

```rust
use mockall::*;

#[automock]
trait ExchangeConnector {
    async fn place_order(&self, order: Order) -> Result<OrderId>;
}

#[tokio::test]
async fn test_with_mock() {
    let mut mock = MockExchangeConnector::new();
    mock.expect_place_order()
        .returning(|_| Ok(OrderId::new()));
    
    // Use mock in test
}
```

---

## 🎯 Test Categories

### Unit Tests
- Test individual functions and methods
- No external dependencies
- Fast execution (<1ms per test)
- Located in `tests/unit/` directories

### Integration Tests
- Test service interactions
- May use mock external services
- Medium execution time (<100ms per test)
- Located in `tests/integration/` directories

### Performance Tests
- Validate latency requirements
- Measure throughput capabilities
- Use criterion for benchmarking
- Located in `tests/performance/` directories

### Property Tests
- Verify invariants across input ranges
- Find edge cases automatically
- Use proptest/quickcheck
- Located in `tests/property/` directories

---

## 🔧 Test Utilities

### Fixtures

```rust
// Standard market data fixture
#[fixture]
pub fn market_data() -> MarketDataFixture {
    MarketDataFixture {
        symbol: "BTCUSDT".to_string(),
        bid_price: 45000.0,
        ask_price: 45010.0,
        // ...
    }
}
```

### Factories

```rust
// Order factory for generating test data
let factory = OrderFactory::new()
    .with_symbol("ETHUSDT")
    .with_quantity(10.0);

let orders = factory.build_batch(100, (2800.0, 2900.0));
```

### Custom Assertions

```rust
// Floating point comparison
assert_approx_eq(calculated, expected, 0.0001);

// Range assertions
assert_in_range(value, min, max);

// Performance assertions
let _perf = PerformanceAssertion::new("critical_path", Duration::from_millis(10));
```

---

## 📊 Coverage Requirements

### Minimum Coverage Targets
- **Unit Tests**: 80% line coverage
- **Integration Tests**: 60% scenario coverage
- **Critical Paths**: 100% coverage required
- **Error Paths**: 90% coverage required

### Coverage Reporting

```bash
# Generate coverage report
cargo tarpaulin --out Html --output-dir ./coverage

# View coverage
open coverage/index.html
```

---

## 🚀 Running Tests

### Run All Tests
```bash
cargo test
```

### Run Specific Test Categories
```bash
# Unit tests only
cargo test --lib

# Integration tests only
cargo test --test '*'

# Service-specific tests
cargo test -p market-connector

# With output
cargo test -- --nocapture
```

### Run Performance Tests
```bash
cargo bench

# Specific benchmark
cargo bench --bench orderbook_bench
```

### Run Property Tests
```bash
# Default proptest runs
cargo test --features proptest

# With more iterations
PROPTEST_CASES=10000 cargo test
```

---

## 🔍 Test Organization Best Practices

### 1. Separate Test Code from Production
- Never use `#[cfg(test)]` modules in production files
- Keep all tests in dedicated `tests/` directories
- This ensures unwrap() calls in tests don't affect metrics

### 2. Use Descriptive Test Names
```rust
#[test]
fn test_order_rejection_when_exceeds_position_limit() { }
// Not: test_order_1()
```

### 3. Test One Thing Per Test
```rust
// ✅ Good - Single responsibility
#[test]
fn test_order_validation_rejects_negative_quantity() { }

// ❌ Bad - Multiple assertions
#[test]
fn test_order_validation() { 
    // Tests quantity, price, symbol, etc.
}
```

### 4. Use Test Builders for Complex Data
```rust
let portfolio = PortfolioBuilder::new()
    .with_cash(100000.0)
    .with_position("BTCUSDT", 2.5, 44000.0)
    .with_position("ETHUSDT", 10.0, 2800.0)
    .build();
```

---

## 🐛 Debugging Tests

### Enable Test Logging
```rust
#[test]
fn test_with_logging() {
    init_test_logging();
    // Test code
}
```

### Run Single Test
```bash
cargo test test_order_flow -- --exact
```

### Debug Output
```bash
RUST_LOG=debug cargo test -- --nocapture
```

---

## 📈 Continuous Integration

### GitHub Actions Workflow
```yaml
test:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v2
    - uses: actions-rs/toolchain@v1
    - run: cargo test --all-features
    - run: cargo bench --no-run
    - run: cargo tarpaulin --out Xml
```

---

## 🔒 Security Testing

### Fuzz Testing
```rust
#[cfg(fuzzing)]
fuzz_target!(|data: &[u8]| {
    // Fuzz test implementation
});
```

### Penetration Testing
- Regular security audits
- Dependency vulnerability scanning
- Input validation testing

---

## 📚 Testing Documentation

Each service should maintain:
1. `tests/README.md` - Test documentation
2. Test scenarios document
3. Performance baseline document
4. Known issues and limitations

---

## ⚠️ Important Notes

1. **NEVER use unwrap() in production code** - Use proper error handling
2. **Isolate test dependencies** - Use test-utils crate
3. **Mock external services** - Never call real exchanges in tests
4. **Use deterministic data** - Reproducible test results
5. **Performance matters** - Test execution should be fast

---

## 🎓 Examples

See `/tests/integration/test_order_flow.rs` for comprehensive examples of:
- Fixture usage
- Mock services
- Concurrent testing
- Performance assertions
- Property-based testing

---

*Testing Architecture Version: 1.0*  
*Maintained by: CTO*