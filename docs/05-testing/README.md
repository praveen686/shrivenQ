# 📊 Testing Infrastructure Documentation

**Last Updated**: August 20, 2025 | **Status**: Production-Ready Framework

## Overview

ShrivenQuant uses a comprehensive testing framework combining unit tests, integration tests, property-based tests, and performance benchmarks. The infrastructure is built on industry-standard Rust testing tools.

## Current Status

### Test Metrics
- **Total Tests**: 153
- **Passing Tests**: 110 (71.9% success rate)
- **Services with Tests**: 7/20 (35%)
- **Target Coverage**: 80%
- **Current Coverage**: ~40%

### Service Test Status

| Service | Unit Tests | Integration Tests | Total | Status |
|---------|------------|------------------|-------|--------|
| Auth Service | 26 | 17 | 43 | ✅ All passing |
| OMS | 13 | 4 | 17 | ✅ All passing |
| Portfolio Manager | 10 | 4 | 14 | ✅ All passing |
| Market Connector | 8 | 4 | 12 | ✅ All passing |
| Data Aggregator | 6 | 2 | 8 | ✅ All passing |
| Reporting | 0 | 6 | 6 | ✅ All passing |
| Trading Gateway | 2 | 2 | 4 | ✅ All passing |
| Risk Manager | 2 | 1 | 3 | ✅ All passing |
| Execution Router | 2 | 0 | 2 | ⚠️ 1 failing |
| Orderbook | - | - | - | ❌ Compilation errors |
| Others | - | - | - | ❌ No tests |

## Testing Framework

### Core Dependencies

```toml
[dev-dependencies]
# Test framework
rstest = "0.23"           # Parameterized tests & fixtures
proptest = "1.6"          # Property-based testing
quickcheck = "1.0"        # QuickCheck-style testing
criterion = "0.6"         # Performance benchmarking

# Async testing
tokio-test = "0.4"        # Tokio test utilities
futures-test = "0.3"      # Futures test utilities

# Mocking
mockall = "0.13"          # Mock generation
wiremock = "0.7"          # HTTP mocking

# Assertions
assert_matches = "1.5"    # Pattern matching assertions
approx = "0.5"            # Floating point comparisons
pretty_assertions = "1.4" # Better assertion output
```

### Test Organization

```
services/
├── service-name/
│   ├── src/
│   │   └── lib.rs
│   ├── tests/
│   │   ├── unit/
│   │   │   ├── mod.rs
│   │   │   ├── feature1_tests.rs
│   │   │   └── feature2_tests.rs
│   │   ├── integration/
│   │   │   ├── mod.rs
│   │   │   └── api_tests.rs
│   │   └── performance/
│   │       └── benchmarks.rs
│   └── benches/
│       └── criterion_bench.rs
```

## Test Types

### 1. Unit Tests

Fast, isolated tests for individual functions and methods.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;
    
    #[rstest]
    #[case(2, 2, 4)]
    #[case(3, 3, 6)]
    #[case(0, 5, 5)]
    fn test_addition(
        #[case] a: i32,
        #[case] b: i32,
        #[case] expected: i32
    ) {
        assert_eq!(add(a, b), expected);
    }
}
```

### 2. Integration Tests

Tests that verify interactions between components.

```rust
// tests/integration/auth_tests.rs
use auth_service::AuthService;
use services_common::proto::auth::v1::*;

#[tokio::test]
async fn test_login_flow() {
    let service = AuthService::new().await.unwrap();
    
    let request = LoginRequest {
        username: "test_user".to_string(),
        password: "password123".to_string(),
        exchange: "zerodha".to_string(),
    };
    
    let response = service.login(request).await.unwrap();
    assert!(!response.token.is_empty());
    assert_eq!(response.expires_in, 86400);
}
```

### 3. Property-Based Tests

Tests with randomly generated inputs to find edge cases.

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_order_book_invariants(
        orders in prop::collection::vec(any::<Order>(), 0..100)
    ) {
        let mut book = OrderBook::new();
        
        for order in orders {
            book.add_order(order);
        }
        
        // Invariant: bid prices < ask prices
        if let (Some(bid), Some(ask)) = (book.best_bid(), book.best_ask()) {
            prop_assert!(bid.price <= ask.price);
        }
    }
}
```

### 4. Performance Benchmarks

Benchmarks to track performance regressions.

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_order_matching(c: &mut Criterion) {
    let mut book = create_test_order_book();
    
    c.bench_function("match_order", |b| {
        b.iter(|| {
            book.match_order(black_box(create_test_order()))
        })
    });
}

criterion_group!(benches, benchmark_order_matching);
criterion_main!(benches);
```

## Test Utilities

### Fixtures

Pre-configured test data and environments.

```rust
// tests/fixtures/mod.rs
use rstest::*;

#[fixture]
pub fn test_config() -> Config {
    Config {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        testnet: true,
        ..Default::default()
    }
}

#[fixture]
pub async fn test_client(test_config: Config) -> Client {
    Client::new(test_config).await.unwrap()
}
```

### Factories

Functions to create test objects with default or custom values.

```rust
// tests/factories/mod.rs
use fake::{Fake, Faker};

pub struct OrderFactory;

impl OrderFactory {
    pub fn market_buy() -> Order {
        Order {
            id: Faker.fake(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            quantity: (100..1000).fake(),
            ..Default::default()
        }
    }
    
    pub fn limit_sell(price: Decimal) -> Order {
        Order {
            id: Faker.fake(),
            side: OrderSide::Sell,
            order_type: OrderType::Limit,
            price: Some(price),
            quantity: (100..1000).fake(),
            ..Default::default()
        }
    }
}
```

### Mocks

Mock implementations for testing.

```rust
use mockall::*;

#[automock]
pub trait ExchangeClient {
    async fn place_order(&self, order: Order) -> Result<OrderResponse>;
    async fn cancel_order(&self, id: &str) -> Result<()>;
}

#[test]
async fn test_order_placement() {
    let mut mock = MockExchangeClient::new();
    
    mock.expect_place_order()
        .times(1)
        .returning(|_| Ok(OrderResponse::default()));
    
    let result = mock.place_order(Order::default()).await;
    assert!(result.is_ok());
}
```

## Running Tests

### Basic Commands

```bash
# Run all tests
cargo test --workspace

# Run tests for specific service
cargo test -p auth-service

# Run only unit tests
cargo test --lib

# Run only integration tests
cargo test --test '*'

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_login_flow

# Run ignored tests
cargo test -- --ignored

# Run benchmarks
cargo bench
```

### Test Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --workspace --out Html --output-dir coverage

# With specific features
cargo tarpaulin --workspace --features "feature1,feature2"

# Exclude certain files
cargo tarpaulin --workspace --exclude-files "*/examples/*" "*/tests/*"
```

### Continuous Testing

```bash
# Install cargo-watch
cargo install cargo-watch

# Run tests on file change
cargo watch -x test

# Run specific tests on change
cargo watch -x "test -p auth-service"

# Clear screen and run tests
cargo watch -c -x test
```

## Writing Tests

### Best Practices

1. **Test Naming**
   ```rust
   #[test]
   fn test_should_calculate_pnl_correctly_for_long_position() {
       // Clear, descriptive name
   }
   ```

2. **Arrange-Act-Assert**
   ```rust
   #[test]
   fn test_order_cancellation() {
       // Arrange
       let mut book = OrderBook::new();
       let order = create_test_order();
       book.add_order(order.clone());
       
       // Act
       let result = book.cancel_order(&order.id);
       
       // Assert
       assert!(result.is_ok());
       assert!(book.get_order(&order.id).is_none());
   }
   ```

3. **Use Fixtures**
   ```rust
   #[rstest]
   async fn test_with_fixture(
       test_client: Client,
       test_config: Config
   ) {
       // Fixtures provide consistent setup
   }
   ```

4. **Test Error Cases**
   ```rust
   #[test]
   fn test_invalid_input_returns_error() {
       let result = parse_quantity("-100");
       assert!(matches!(result, Err(Error::InvalidQuantity(_))));
   }
   ```

5. **Async Testing**
   ```rust
   #[tokio::test]
   async fn test_async_operation() {
       let future = async_function();
       tokio::time::timeout(Duration::from_secs(5), future)
           .await
           .expect("Operation timed out");
   }
   ```

### Common Patterns

#### Testing gRPC Services

```rust
#[tokio::test]
async fn test_grpc_service() {
    let service = MyServiceImpl::new();
    let request = tonic::Request::new(MyRequest {
        field: "value".to_string(),
    });
    
    let response = service.my_method(request).await.unwrap();
    let inner = response.into_inner();
    
    assert_eq!(inner.result, "expected");
}
```

#### Testing with Time

```rust
#[tokio::test]
async fn test_with_time() {
    tokio::time::pause();
    
    let start = Instant::now();
    let future = tokio::time::sleep(Duration::from_secs(10));
    
    tokio::time::advance(Duration::from_secs(10)).await;
    future.await;
    
    assert_eq!(start.elapsed().as_secs(), 10);
}
```

#### Testing Panics

```rust
#[test]
#[should_panic(expected = "division by zero")]
fn test_panic() {
    divide(10, 0);
}
```

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          
      - name: Run tests
        run: cargo test --workspace --all-features
        
      - name: Generate coverage
        run: |
          cargo install cargo-tarpaulin
          cargo tarpaulin --workspace --out Xml
          
      - name: Upload coverage
        uses: codecov/codecov-action@v3
```

## Troubleshooting

### Common Issues

1. **Tests hang indefinitely**
   ```bash
   # Add timeout
   cargo test -- --test-threads=1 --nocapture
   
   # Or use tokio timeout
   #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
   async fn test_with_timeout() {
       tokio::time::timeout(Duration::from_secs(5), async_op()).await.unwrap();
   }
   ```

2. **Flaky tests**
   ```rust
   // Add retry logic
   #[test]
   #[retry(3)]  // Using test-retry crate
   fn potentially_flaky_test() {
       // Test that might fail intermittently
   }
   ```

3. **Resource conflicts**
   ```rust
   // Use unique resources per test
   #[test]
   fn test_with_unique_port() {
       let port = get_random_port();
       let addr = format!("127.0.0.1:{}", port);
       // Use addr for test
   }
   ```

4. **Compilation errors in tests**
   - Check feature flags: `cargo test --all-features`
   - Update dependencies: `cargo update`
   - Clean build: `cargo clean && cargo test`

## Test Improvements Roadmap

### Completed ✅
- [x] Test framework setup
- [x] Test utilities (fixtures, factories, mocks)
- [x] Auth service tests (43 passing)
- [x] OMS tests (17 passing)
- [x] Integration test structure

### In Progress 🚧
- [ ] Increase coverage to 80%
- [ ] Fix orderbook compilation
- [ ] Add performance benchmarks
- [ ] Property-based testing expansion

### Planned 📋
- [ ] Mutation testing
- [ ] Fuzz testing
- [ ] Load testing framework
- [ ] Chaos engineering tests
- [ ] Contract testing
- [ ] End-to-end tests

## Resources

### Documentation
- [Rust Book - Testing](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [rstest Documentation](https://docs.rs/rstest)
- [Proptest Book](https://proptest-rs.github.io/proptest/)
- [Criterion User Guide](https://bheisler.github.io/criterion.rs/book/)

### Tools
- [cargo-nextest](https://nexte.st/) - Faster test runner
- [cargo-mutants](https://mutants.rs/) - Mutation testing
- [cargo-fuzz](https://rust-fuzz.github.io/book/) - Fuzz testing
- [insta](https://insta.rs/) - Snapshot testing

---

*Last Updated: August 20, 2025*