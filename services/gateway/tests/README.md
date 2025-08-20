# API Gateway Test Suite

Comprehensive test suite for the ShrivenQuant API Gateway service, covering unit tests, integration tests, performance benchmarks, stress tests, and error handling scenarios.

## Test Structure

```
tests/
├── lib.rs                     # Test utilities and common fixtures
├── integration_tests.rs       # Integration tests for HTTP endpoints
├── performance_tests.rs       # Performance benchmarks with Criterion
├── stress_tests.rs            # High-load stress testing
├── error_handling_tests.rs    # Error scenarios and resilience
├── unit/                      # Unit tests organized by module
│   ├── mod.rs                 # Unit test module organization
│   ├── helpers.rs             # Test helpers and utilities
│   ├── auth_handlers.rs       # Authentication handler tests
│   ├── execution_handlers.rs  # Execution handler tests
│   ├── risk_handlers.rs       # Risk management handler tests
│   ├── middleware.rs          # Middleware tests
│   ├── rate_limiter.rs        # Rate limiter tests
│   ├── websocket.rs           # WebSocket handler tests
│   └── models.rs              # Data model tests
└── README.md                  # This file
```

## Test Categories

### Unit Tests (`tests/unit/`)

Isolated tests for individual components:

- **Authentication Handlers**: JWT validation, login/logout flows, permission checks
- **Execution Handlers**: Order submission, cancellation, status queries
- **Risk Handlers**: Risk checks, position queries, kill switch functionality
- **Middleware**: Authentication, rate limiting, CORS, request logging
- **Rate Limiter**: Concurrent rate limiting, IP-based limits, endpoint-specific limits
- **WebSocket**: Message handling, subscription management, broadcasting
- **Models**: Data serialization/deserialization, validation

### Integration Tests (`integration_tests.rs`)

End-to-end tests covering complete request flows:

- Authentication flows (login, token refresh, validation)
- Order management (submit, cancel, query status)
- Market data endpoints (snapshots, historical data)
- Risk management (position checks, metrics)
- Rate limiting behavior
- WebSocket connections and streaming
- Error handling and recovery
- CORS and preflight requests

### Performance Tests (`performance_tests.rs`)

Criterion-based benchmarks measuring:

- Request/response serialization performance
- JWT token operations (encode/decode)
- Fixed-point arithmetic conversions
- Rate limiter performance under load
- WebSocket message processing
- Memory allocation patterns
- End-to-end latency requirements (<10ms target)
- Throughput benchmarks (>10k RPS target)

### Stress Tests (`stress_tests.rs`)

High-load scenarios testing system limits:

- Concurrent connection handling (1000+ simultaneous connections)
- Memory usage under extreme load
- Rate limiter behavior at scale
- WebSocket message broadcasting performance
- Configuration parsing under stress
- Error handling at high volumes
- System integration under combined load

### Error Handling Tests (`error_handling_tests.rs`)

Fault tolerance and resilience testing:

- gRPC service unavailability scenarios
- Authentication failure handling
- Rate limiting error responses
- Invalid request data handling
- Timeout and circuit breaker simulation
- Concurrent error scenarios
- Resource exhaustion handling
- Graceful degradation testing
- Error propagation chains

## Running Tests

### Prerequisites

Ensure you have the required test dependencies:

```bash
cargo install cargo-nextest  # Faster test runner (optional)
cargo install cargo-criterion  # Criterion benchmark runner
```

### Unit Tests

Run all unit tests:

```bash
cargo test --lib
```

Run specific unit test modules:

```bash
cargo test unit::auth_handlers
cargo test unit::rate_limiter
cargo test unit::websocket
```

### Integration Tests

Run integration tests (requires mock services or will test error handling):

```bash
cargo test --test integration_tests
```

### Performance Benchmarks

Run performance benchmarks:

```bash
cargo bench
```

View benchmark results in browser:

```bash
open target/criterion/report/index.html
```

### Stress Tests

Run stress tests with detailed output:

```bash
RUST_LOG=info cargo test --test stress_tests -- --nocapture
```

### Error Handling Tests

Run error handling and resilience tests:

```bash
cargo test --test error_handling_tests
```

### All Tests

Run the complete test suite:

```bash
# Using standard test runner
cargo test --all

# Using nextest (faster, parallel execution)
cargo nextest run --all
```

## Test Configuration

### Environment Variables

- `RUST_LOG`: Set logging level (default: `info`)
- `RUST_BACKTRACE`: Enable backtraces on panics (set to `1`)
- `GATEWAY_TEST_TIMEOUT`: Override default test timeouts (seconds)
- `GATEWAY_TEST_PARALLEL`: Number of parallel test threads

### Test Profiles

Different test configurations for various scenarios:

```bash
# Fast tests (skip slow integration tests)
cargo test --exclude integration_tests

# Performance focus (only benchmarks)
cargo bench --bench gateway_benchmarks

# Stress testing profile
RUST_LOG=debug cargo test stress_test --release -- --nocapture
```

## Key Test Scenarios

### Authentication Testing

- ✅ Valid JWT token validation
- ✅ Expired token handling
- ✅ Malformed token rejection
- ✅ Permission-based access control
- ✅ Concurrent authentication requests

### Order Management Testing

- ✅ Order submission validation
- ✅ Order cancellation flows
- ✅ Order status queries
- ✅ Invalid order parameter handling
- ✅ Permission checks for order operations

### Rate Limiting Testing

- ✅ Per-IP rate limiting
- ✅ Per-endpoint rate limiting
- ✅ Burst capacity handling
- ✅ Rate limit recovery over time
- ✅ Concurrent client scenarios

### WebSocket Testing

- ✅ Connection establishment
- ✅ Message subscription handling
- ✅ Market data streaming
- ✅ Execution report broadcasting
- ✅ Connection cleanup on disconnect

### Performance Requirements

The test suite validates these performance targets:

- **Latency**: 95th percentile < 10ms for critical paths
- **Throughput**: > 10,000 requests/second sustained
- **Memory**: Stable memory usage under load
- **Concurrency**: Handle 1,000+ simultaneous connections
- **Rate Limiting**: < 1ms per rate limit check

### Error Scenarios

Comprehensive error testing covers:

- Network failures and timeouts
- Service unavailability
- Invalid input data
- Authentication failures
- Resource exhaustion
- Concurrent access errors

## Continuous Integration

### Test Pipeline

The CI/CD pipeline runs tests in this order:

1. **Unit Tests**: Fast, isolated component tests
2. **Integration Tests**: End-to-end functionality validation
3. **Performance Benchmarks**: Regression detection
4. **Stress Tests**: Load handling verification
5. **Error Handling**: Resilience validation

### Performance Regression Detection

Benchmarks are compared against baseline measurements:

- Fail CI if performance degrades > 10%
- Generate performance trend reports
- Alert on memory usage increases

### Test Coverage

Minimum coverage requirements:

- Unit tests: > 90% line coverage
- Integration tests: Cover all API endpoints
- Error scenarios: Test all error code paths
- Performance: Benchmark critical operations

## Writing New Tests

### Unit Test Example

```rust
use rstest::*;
use api_gateway::handlers::AuthHandlers;

#[rstest]
#[tokio::test]
async fn test_login_success() {
    // Arrange
    let handlers = create_test_auth_handlers();
    let request = create_login_request("testuser");

    // Act
    let result = AuthHandlers::login(State(handlers), Json(request)).await;

    // Assert
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_api_success(&response.0);
}
```

### Integration Test Example

```rust
#[tokio::test]
async fn test_order_submission_flow() {
    let fixture = TestFixture::setup().await?;
    let token = fixture.authenticate("testuser").await?;

    let order_request = create_order_request("NIFTY", "BUY");
    let response = fixture.submit_order(&token, order_request).await?;

    assert_eq!(response.status(), StatusCode::OK);
    let body: ApiResponse<SubmitOrderResponse> = response.json().await?;
    assert_api_success(&body);
}
```

### Performance Benchmark Example

```rust
fn bench_order_processing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("order_processing", |b| {
        b.to_async(&rt).iter(|| async {
            let order = create_order_request("TEST", "BUY");
            process_order(order).await
        })
    });
}
```

## Test Maintenance

### Regular Tasks

- Update test data and fixtures
- Review and update performance baselines
- Add tests for new features
- Remove obsolete test scenarios
- Update documentation

### Best Practices

- Use descriptive test names
- Follow AAA pattern (Arrange, Act, Assert)
- Keep tests independent and isolated
- Use fixtures for common setup
- Mock external dependencies
- Test both success and failure paths
- Include edge cases and boundary conditions

## Troubleshooting

### Common Issues

1. **Tests timing out**: Increase timeout values or check for deadlocks
2. **Flaky tests**: Add proper synchronization and retries
3. **Performance regression**: Review recent changes and optimize
4. **Memory leaks**: Use memory profiling tools
5. **Connection errors**: Verify mock services are running

### Debug Commands

```bash
# Run tests with debug logging
RUST_LOG=debug cargo test -- --nocapture

# Run single test with backtrace
RUST_BACKTRACE=full cargo test specific_test_name

# Profile memory usage
cargo test --features memory-profiling

# Generate test coverage report
cargo tarpaulin --out html --output-dir coverage/
```

## Contributing

When adding new features:

1. Write unit tests for new components
2. Add integration tests for new endpoints
3. Include performance benchmarks for critical paths
4. Test error conditions and edge cases
5. Update documentation and examples

For questions or issues with the test suite, please refer to the main project documentation or open an issue in the repository.