# Authentication Service Test Suite

This directory contains comprehensive tests for the ShrivenQuant authentication service, covering unit tests, integration tests, performance benchmarks, and security validations.

## Test Structure

```
tests/
├── unit/                          # Unit tests
│   ├── mod.rs                     # Module organization
│   ├── test_utils.rs              # Common test utilities and mocks
│   ├── auth_service_tests.rs      # Core AuthService trait tests
│   ├── binance_service_tests.rs   # Binance authentication tests
│   ├── zerodha_service_tests.rs   # Zerodha authentication tests
│   ├── grpc_service_tests.rs      # gRPC interface tests
│   ├── token_management_tests.rs  # JWT lifecycle tests
│   ├── error_handling_tests.rs    # Error scenarios and recovery
│   ├── concurrency_tests.rs       # Thread safety and concurrent ops
│   ├── security_tests.rs          # Security vulnerability tests
│   ├── rate_limiting_tests.rs     # API rate limiting tests
│   └── orchestrator_tests.rs      # Multi-exchange coordination
├── binance_integration_test.rs    # Binance end-to-end tests
├── zerodha_integration_test.rs    # Zerodha end-to-end tests
├── test_runner.rs                 # Comprehensive test runner
├── mod.rs                         # Test module organization
└── README.md                      # This file
```

## Running Tests

### All Tests
```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test file
cargo test --test auth_service_tests
```

### Unit Tests
```bash
# Run all unit tests
cargo test unit

# Run specific unit test modules
cargo test unit::auth_service_tests
cargo test unit::binance_service_tests
cargo test unit::zerodha_service_tests
cargo test unit::grpc_service_tests
cargo test unit::token_management_tests
cargo test unit::error_handling_tests
cargo test unit::concurrency_tests
cargo test unit::security_tests
cargo test unit::rate_limiting_tests
cargo test unit::orchestrator_tests
```

### Integration Tests
```bash
# Run integration tests (requires credentials)
cargo test --ignored binance_integration
cargo test --ignored zerodha_integration

# Run with output to see detailed results
cargo test --ignored -- --nocapture
```

### Performance Tests
```bash
# Run performance benchmarks
cargo test performance_tests -- --nocapture

# Run specific performance tests
cargo test test_auth_service_performance
cargo test test_concurrent_token_performance
cargo test test_rate_limiter_performance
```

### Security Tests
```bash
# Run security-focused tests
cargo test security_tests -- --nocapture

# Run specific security test categories
cargo test test_sql_injection_prevention
cargo test test_jwt_token_tampering_prevention
cargo test test_timing_attack_prevention
```

### Custom Test Runner
```bash
# Run the comprehensive test runner
cargo run --bin test_runner

# Or if configured as a test
cargo test test_runner
```

## Test Categories

### 1. Unit Tests

#### AuthService Core Tests (`auth_service_tests.rs`)
- Basic authentication functionality
- Token generation and validation
- Permission checking
- Concurrent operations
- Edge cases and error handling

#### Binance Service Tests (`binance_service_tests.rs`)
- Binance-specific authentication flow
- API key management
- Account information retrieval
- Listen key lifecycle
- Error handling and retries
- Network timeout simulation

#### Zerodha Service Tests (`zerodha_service_tests.rs`)
- Zerodha authentication workflow
- TOTP generation and validation
- Profile and margin data retrieval
- Token caching mechanisms
- Session management
- Error recovery patterns

#### gRPC Service Tests (`grpc_service_tests.rs`)
- Protocol buffer message handling
- Service method implementations
- Error code mapping
- Permission conversion
- Concurrent request handling
- Token lifecycle integration

#### Token Management Tests (`token_management_tests.rs`)
- JWT token generation and validation
- Token expiry handling
- Refresh token mechanisms
- Token revocation
- Concurrent token operations
- Security and tampering prevention

#### Error Handling Tests (`error_handling_tests.rs`)
- Various failure modes
- Retry mechanisms with exponential backoff
- Graceful degradation
- Circuit breaker patterns
- Error propagation
- Recovery scenarios

#### Concurrency Tests (`concurrency_tests.rs`)
- Thread safety validation
- High-concurrency stress testing
- Deadlock prevention
- Memory consistency
- Performance under load
- Race condition detection

#### Security Tests (`security_tests.rs`)
- SQL injection prevention
- JWT token tampering protection
- Timing attack mitigation
- Input sanitization
- Session security
- Authorization bypass prevention
- Information disclosure protection

#### Rate Limiting Tests (`rate_limiting_tests.rs`)
- Request throttling
- Per-user rate limits
- Window-based limiting
- Burst handling
- Concurrent rate limiting
- Cleanup and recovery

#### Orchestrator Tests (`orchestrator_tests.rs`)
- Multi-exchange routing
- Fallback mechanisms
- Load balancing
- Circuit breaker integration
- Metrics collection
- Service health monitoring

### 2. Integration Tests

#### Binance Integration (`binance_integration_test.rs`)
- End-to-end authentication flow
- Real API connectivity testing
- Account information retrieval
- WebSocket authentication
- Order placement authentication
- Testnet integration

#### Zerodha Integration (`zerodha_integration_test.rs`)
- Complete authentication workflow
- TOTP-based 2FA integration
- Session caching validation
- Profile and margin data
- WebSocket connectivity
- Order authentication flow

### 3. Performance Tests

Located within each test module as `performance_tests` submodules:
- Authentication throughput benchmarks
- Token generation performance
- Concurrent operation scalability
- Memory usage optimization
- Latency percentile analysis

## Test Utilities

### Mock Services (`test_utils.rs`)
- `MockAuthService`: Configurable authentication service mock
- `MockHttpClient`: HTTP client simulation
- `TestAuthContext` helpers: Context creation utilities
- JWT token helpers: Generation and validation utilities

### Common Patterns
- Async test execution
- Error simulation and recovery
- Performance measurement
- Concurrent operation testing
- Security vulnerability probing

## Environment Setup

### For Unit Tests
No special setup required - all dependencies are mocked.

### For Integration Tests
Set environment variables in `.env` file:
```env
# Binance credentials (optional - tests are ignored without them)
BINANCE_SPOT_API_KEY=your_binance_spot_api_key
BINANCE_SPOT_API_SECRET=your_binance_spot_secret
BINANCE_FUTURES_API_KEY=your_binance_futures_key
BINANCE_FUTURES_API_SECRET=your_binance_futures_secret

# Zerodha credentials (optional - tests are ignored without them)
ZERODHA_API_KEY=your_zerodha_api_key
ZERODHA_API_SECRET=your_zerodha_secret
ZERODHA_USER_ID=your_trading_id
ZERODHA_PASSWORD=your_password
ZERODHA_TOTP_SECRET=your_totp_secret

# JWT configuration
JWT_SECRET=your_jwt_secret_key
TOKEN_EXPIRY=3600
```

## Test Coverage Goals

- **Core Authentication**: >95% coverage
- **Exchange Integrations**: >90% coverage
- **Security Features**: >95% coverage
- **Error Handling**: >90% coverage
- **Concurrent Operations**: >85% coverage

## Continuous Integration

### GitHub Actions Integration
```yaml
name: Auth Service Tests
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run unit tests
        run: cargo test unit
      - name: Run security tests
        run: cargo test security_tests
      - name: Run performance tests
        run: cargo test performance_tests
```

## Contributing

When adding new features:
1. Write unit tests first (TDD approach)
2. Add integration tests for new exchange integrations
3. Include security tests for authentication-related features
4. Add performance tests for critical paths
5. Update this README with new test categories

## Troubleshooting

### Common Issues

1. **Integration tests failing**: Check that credentials are properly set in `.env`
2. **Performance tests flaky**: May need adjustment for CI environments
3. **Security tests too strict**: Review implementation for timing-related issues
4. **Concurrency tests failing**: Check for race conditions in test setup

### Test Environment

- Tests run in isolated environments with mocked external dependencies
- Real network calls only occur in integration tests
- All tests should be deterministic and repeatable
- Performance tests include reasonable tolerances for CI environments

### Debugging Tests

```bash
# Run with debug output
RUST_LOG=debug cargo test -- --nocapture

# Run specific test with backtrace
RUST_BACKTRACE=1 cargo test specific_test_name -- --nocapture

# Run tests in single thread for easier debugging
cargo test -- --test-threads=1 --nocapture
```

## Metrics and Reporting

The test suite provides detailed metrics:
- Test execution times
- Success/failure rates
- Performance benchmarks
- Coverage reports
- Security vulnerability assessments

Use the custom test runner for comprehensive reporting and analysis.