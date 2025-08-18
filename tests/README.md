# ShrivenQuant Integration Tests

## Status: ⚠️ Minimal Coverage

This directory contains integration tests for the ShrivenQuant trading system.

## Current State

- **1 stub test** - Basic gRPC connectivity (not functional)
- **0% coverage** - No actual integration testing
- **Not automated** - No CI/CD integration

## Test Structure (Planned)

```
tests/
├── integration/
│   ├── order_flow_test.rs       # End-to-end order flow
│   ├── market_data_test.rs      # Market data pipeline
│   ├── risk_checks_test.rs      # Risk management flow
│   └── auth_flow_test.rs        # Authentication flow
├── performance/
│   ├── latency_test.rs          # Latency benchmarks
│   └── throughput_test.rs       # Throughput tests
└── fixtures/
    ├── sample_orders.json        # Test data
    └── mock_market_data.json    # Mock market data
```

## Running Tests

```bash
# Run all tests (currently just one stub)
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_grpc_connectivity
```

## Required Test Coverage

### Critical Paths (Must Have)
1. Order submission → Risk check → Execution
2. Market data → Order book update → Strategy signal
3. Authentication → Token validation → Service access
4. Position update → P&L calculation → Risk metrics

### Integration Scenarios
1. Multi-service communication
2. Service failure recovery
3. Message ordering guarantees
4. Concurrent request handling

### Performance Tests
1. Order latency < 5ms
2. Market data throughput > 10k msgs/sec
3. Risk checks < 1ms
4. Memory usage under load

## Current Issues

1. **No actual tests** - Just one stub
2. **Services not testable** - Missing test infrastructure
3. **No test data** - Need fixtures and mocks
4. **No CI integration** - Tests not automated
5. **No coverage metrics** - Unknown coverage

## TODO

- [ ] Create test infrastructure
- [ ] Add order flow integration test
- [ ] Add market data pipeline test
- [ ] Add risk management test
- [ ] Create test fixtures
- [ ] Add performance benchmarks
- [ ] Integrate with CI/CD
- [ ] Add coverage reporting

## Note

The current `integration_test.rs` is just a placeholder. Proper integration testing requires:
1. Service orchestration
2. Test data management
3. Mock exchange connections
4. Automated test execution

This is a critical gap that must be addressed before any production use.