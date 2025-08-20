# Portfolio Manager Test Suite

This directory contains comprehensive tests for the portfolio-manager service, covering all major functionality with extensive unit and integration testing.

## Test Structure

### Unit Tests (`/tests/unit/`)
Modular unit tests focusing on individual components:

1. **position_tests.rs** - Position tracking and atomic operations
   - Position initialization and state management
   - Long/short position opening and closing
   - Position averaging and partial fills
   - Position flipping (long to short and vice versa)
   - Market price updates and P&L calculations
   - Concurrent position operations and thread safety
   - Edge cases (zero quantities, large positions)

2. **portfolio_tests.rs** - Portfolio analytics and performance metrics
   - Portfolio statistics calculation (long/short exposure, net/gross exposure)
   - Risk metrics (VaR, CVaR, volatility, drawdown, downside deviation)
   - Performance metrics (returns, Sharpe ratio, Sortino ratio, Calmar ratio)
   - Win/loss statistics and profit factors
   - Correlation and beta calculations
   - Buffer management for historical data

3. **optimization_tests.rs** - Portfolio optimization algorithms
   - Equal weight allocation
   - Minimum variance optimization
   - Maximum Sharpe ratio optimization
   - Risk parity allocation
   - Weight calculation and constraint enforcement
   - Rebalance change generation and validation

4. **rebalancer_tests.rs** - Rebalancing logic and order generation
   - Order generation from rebalance changes
   - Buy/sell order creation and validation
   - Order management (pending orders, fills, cancellations)
   - Rebalance execution workflows
   - Edge cases (zero quantities, large orders)

5. **market_feed_tests.rs** - Market feed integration and price updates
   - Price snapshot atomic operations
   - Market feed manager functionality
   - Price update processing and validation
   - Returns buffer management and calculations
   - Beta and correlation calculations
   - Concurrent price updates and thread safety

### Integration Tests (`/tests/integration/`)
End-to-end testing of complete workflows:

1. **portfolio_manager_integration_tests.rs** - Service integration tests
   - Complete portfolio lifecycle management
   - Multi-position portfolio operations
   - Optimization and rebalancing workflows
   - Market data integration
   - Risk and performance metrics integration
   - Error handling and edge cases
   - Concurrent operations testing

2. **concurrency_stress_tests.rs** - Performance and scalability testing
   - High-frequency position updates (10,000+ operations)
   - Rapid market price updates (50,000+ updates)
   - Concurrent read/write operations
   - Large-scale portfolio testing (1000+ symbols)
   - Memory-intensive operations
   - Atomic operations stress testing
   - Position tracker concurrent access

## Test Coverage

### Core Functionality
- ✅ Position tracking (atomic operations, P&L calculations)
- ✅ Portfolio analytics (statistics, risk, performance)
- ✅ Optimization algorithms (all 4 strategies)
- ✅ Rebalancing logic (order generation, execution)
- ✅ Market feed integration (price updates, correlations)

### Edge Cases
- ✅ Empty portfolios and single positions
- ✅ Zero quantities and extreme values
- ✅ Large position sizes and high-frequency operations
- ✅ Concurrent access and thread safety
- ✅ Error conditions and recovery

### Performance Testing
- ✅ High-frequency operations (>10K ops/sec)
- ✅ Large portfolios (1000+ symbols)
- ✅ Memory intensive scenarios
- ✅ Concurrent access patterns
- ✅ Stress testing under load

## Key Test Scenarios

### Position Management
```rust
// Long position lifecycle
position.apply_fill(Side::Bid, qty, price, timestamp);
position.update_market(higher_bid, higher_ask, timestamp);
// Verify unrealized P&L > 0

// Position flipping
position.apply_fill(Side::Bid, qty, price, timestamp);    // Long
position.apply_fill(Side::Ask, larger_qty, price, timestamp); // Flip to short
// Verify realized P&L and new position direction
```

### Portfolio Analytics
```rust
// Risk metrics calculation
let risk = analyzer.calculate_risk(&returns);
assert!(risk.var_95 <= 0);  // VaR should be negative (loss)
assert!(risk.volatility > 0);
assert!(risk.sharpe_ratio != 0);
```

### Optimization
```rust
// Equal weight optimization
let changes = optimizer.optimize(
    OptimizationStrategy::EqualWeight,
    &positions,
    &constraints
).await.unwrap();
// Verify rebalance changes aim for equal weights
```

### Concurrency
```rust
// Concurrent position updates
let handles: Vec<_> = (0..num_threads)
    .map(|thread_id| {
        let tracker = Arc::clone(&tracker);
        tokio::spawn(async move {
            // Perform concurrent operations
        })
    }).collect();
```

## Running Tests

```bash
# Run all tests
cargo test

# Run specific test categories
cargo test --test working_integration_test
cargo test --lib  # Library unit tests

# Run with output
cargo test -- --nocapture

# Run stress tests (may take longer)
cargo test concurrency_stress --test concurrency_stress_tests

# Run specific test
cargo test test_portfolio_manager_basic_workflow
```

## Performance Benchmarks

Based on the stress tests, the portfolio manager achieves:

- **Position Updates**: >10,000 operations/sec
- **Market Updates**: >50,000 updates/sec  
- **Mixed Operations**: >20,000 ops/sec
- **Concurrent Access**: 10+ threads with consistent state
- **Large Portfolios**: 1000+ symbols with reasonable performance

## Test Dependencies

The tests use the following frameworks and tools:

- **rstest**: Parameterized testing and fixtures
- **tokio-test**: Async testing utilities
- **approx**: Floating-point comparisons
- **std::thread**: Concurrency testing
- **std::sync::Arc**: Thread-safe shared state

## Notes

- All tests use fixed-point arithmetic consistent with the production code
- Thread safety is extensively tested with atomic operations
- Performance tests include realistic trading scenarios
- Edge cases cover both normal and extreme market conditions
- Integration tests verify end-to-end workflows match expected behavior