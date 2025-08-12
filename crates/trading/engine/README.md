# Trading Engine

## Zero-Allocation, Ultra-Low-Latency Execution Engine

The trading engine is the heart of ShrivenQuant's execution system, designed for institutional-grade performance with zero allocations in hot paths and sub-microsecond decision latency.

## Architecture

### Core Design Principles

1. **Zero-Allocation Hot Path**: All critical paths use pre-allocated memory pools
2. **Lock-Free Operations**: DashMap for concurrent access, atomic operations for state
3. **Branch-Free Risk Checks**: Bitwise operations eliminate branches in risk validation
4. **SIMD Optimizations**: AVX2 intrinsics for performance-critical calculations
5. **Cache-Aligned Structures**: All hot data structures are 64-byte aligned

### Components

#### Memory Management (`memory.rs`)
- **ObjectPool**: Pre-allocated pool for orders with lock-free acquire/release
- **Arena Allocator**: Chunk-based allocation for temporary objects
- Pre-allocates first chunk to guarantee availability
- Uses `MaybeUninit` for uninitialized memory safety

#### Core Engine (`core.rs`)
- Central orchestrator for all trading operations
- Manages venue adapters (Zerodha/Binance)
- Coordinates execution, risk, position, and metrics
- Supports Paper, Live, and Backtest modes

#### Risk Management (`risk.rs`)
- **Branch-free checks** using bitwise operations
- Per-symbol and global position limits
- Order size and value validation
- Rate limiting with ring buffer
- Circuit breakers and emergency stop
- Fixed-point arithmetic for deterministic calculations

#### Position Tracking (`position.rs`)
- **Incremental PnL updates** on hot path
- Periodic reconciliation for accuracy
- Atomic operations for thread-safe updates
- Handles both long and short positions
- Tracks realized and unrealized PnL

#### Execution Layer (`execution.rs`)
- Order lifecycle management
- Paper trading simulation
- Backtest replay with historical timestamps
- Fill processing and order matching

#### Venue Adapters (`venue.rs`)
- Abstraction for multiple exchanges
- Zerodha adapter for NSE/BSE
- Binance adapter for crypto
- Latency tracking and order status

#### Metrics Engine (`metrics.rs`)
- Lock-free performance counters
- SIMD-accelerated statistics
- Latency histograms with HdrHistogram
- Real-time performance monitoring

## Performance Characteristics

### Latency Targets
- **Risk Check**: < 50ns
- **Order Send**: < 100ns
- **Position Update**: < 75ns
- **PnL Calculation**: < 100ns (incremental)

### Throughput
- **Orders/sec**: > 1M
- **Risk Checks/sec**: > 10M
- **Position Updates/sec**: > 5M

### Memory Usage
- **Zero allocations** in steady state
- **Pre-allocated pools** for 10K orders
- **Fixed memory footprint** regardless of load

## Testing

### Test Framework
All tests use **rstest** for parametrized testing, reducing code duplication and improving coverage.

```rust
#[rstest]
#[case(Side::Bid, 100.0, 100.0, true)]  // Small order should pass
#[case(Side::Bid, 100000.0, 100.0, false)]  // Huge order should fail
fn test_risk_order_size_check(
    #[case] side: Side,
    #[case] qty: f64,
    #[case] price: f64,
    #[case] should_pass: bool,
) {
    // Test implementation
}
```

### Test Coverage
- **Unit Tests**: All components individually tested
- **Integration Tests**: End-to-end scenarios
- **Property Tests**: Randomized testing with proptest
- **Concurrency Tests**: Multi-threaded stress tests

### Running Tests
```bash
# Run all tests
cargo test -p engine

# Run with release optimizations
cargo test -p engine --release

# Run specific test
cargo test -p engine test_risk_order_size_check

# Run benchmarks
cargo bench -p engine
```

## Usage Example

```rust
use engine::core::{Engine, EngineConfig, ExecutionMode};
use engine::venue::{VenueConfig, create_zerodha_adapter};
use bus::EventBus;
use common::{Symbol, Side, Qty, Px};
use std::sync::Arc;

// Configure engine
let mut config = EngineConfig::default();
config.mode = ExecutionMode::Paper;
config.risk_check_enabled = true;

// Setup venue
let venue_config = VenueConfig {
    api_key: "your_key".to_string(),
    api_secret: "your_secret".to_string(),
    testnet: false,
};
let venue = create_zerodha_adapter(venue_config);

// Create event bus
let bus = Arc::new(EventBus::new(1024));

// Initialize engine
let engine = Engine::new(config, venue, bus);

// Send order
let symbol = Symbol(256100); // NIFTY
let result = engine.send_order(
    symbol,
    Side::Bid,
    Qty::new(50.0),
    Some(Px::new(19500.0))
);

match result {
    Ok(order_id) => println!("Order sent: {:?}", order_id),
    Err(e) => println!("Order rejected: {}", e),
}

// Get performance metrics
let perf = engine.get_performance();
println!("Orders sent: {}", perf.orders_sent);
println!("Avg latency: {}ns", perf.avg_tick_to_decision_ns);
```

## Configuration

```rust
pub struct EngineConfig {
    // Execution mode
    pub mode: ExecutionMode,
    pub venue: VenueType,

    // Risk settings
    pub risk_check_enabled: bool,
    pub max_order_value: u64,
    pub max_position_value: u64,
    pub max_daily_loss: i64,

    // Performance
    pub metrics_enabled: bool,
    pub order_pool_size: usize,
    pub position_cache_size: usize,
}
```

## Risk Limits

Default risk limits (configurable):
- **Max Order Size**: 1,000 units
- **Max Order Value**: ₹10 lakhs
- **Max Position Size**: 10,000 units
- **Max Position Value**: ₹1 crore
- **Max Total Exposure**: ₹5 crore
- **Max Daily Loss**: ₹5 lakhs
- **Max Drawdown**: ₹10 lakhs
- **Orders per Minute**: 100

## Authentication

The engine integrates with venue-specific authentication:

### Zerodha
- **Fully automated** login with TOTP
- No manual intervention required
- Session persistence in `/tmp/zerodha_token.json`

### Binance
- API key/secret authentication
- Supports testnet and production
- HMAC-SHA256 request signing

## Monitoring

Real-time metrics available via `get_performance()`:
- Orders sent/filled/rejected
- Average latencies (tick-to-decision, decision-to-send)
- Position count and exposure
- PnL (realized/unrealized)
- Risk metrics and breaker status

## Production Deployment

1. **Memory**: Pre-allocate all pools at startup
2. **CPU Affinity**: Pin to isolated cores
3. **Network**: Kernel bypass with DPDK (future)
4. **Monitoring**: Connect to observability stack
5. **Logging**: Async, off-critical-path logging

## Future Enhancements

- [ ] GPU acceleration for risk calculations
- [ ] Distributed order routing
- [ ] FIX protocol support
- [ ] Market making algorithms
- [ ] Smart order routing
