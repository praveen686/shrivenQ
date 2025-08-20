# Market Connector Service - Testing Guide

This document outlines the comprehensive testing strategy and implementation for the market-connector service.

## Test Structure

The test suite is organized into several categories to ensure thorough coverage:

### Unit Tests (`tests/unit/`)

**Binance WebSocket Tests** (`binance_websocket_tests.rs`)
- WebSocket connection handling and lifecycle management
- Message parsing for depth updates, trades, and ticker data
- Order book management and reconstruction
- Binary data processing and validation
- Error handling and malformed data scenarios
- Performance under high-frequency updates

**Zerodha WebSocket Tests** (`zerodha_websocket_tests.rs`)
- WebSocket connection with Kite API authentication
- Binary tick data parsing (LTP, Quote, Full modes)
- JSON message processing for orders and quotes
- Market data conversion and validation
- Authentication flow and error recovery
- Binary protocol compliance testing

**gRPC Service Tests** (`grpc_service_tests.rs`)
- gRPC endpoint functionality (subscribe, unsubscribe, snapshots)
- Event streaming and client connection management
- Data type conversion between internal and protobuf formats
- Subscription filtering and routing logic
- Error handling and status code validation
- WebSocket connection coordination

**Instrument Service Tests** (`instrument_service_tests.rs`)
- CSV parsing of Zerodha instrument data
- WAL (Write-Ahead Log) storage and persistence
- Option chain calculations and ATM strike determination
- Instrument querying and filtering
- Subscription token management
- Memory efficiency with large datasets

**Connector Adapter Tests** (`connector_adapter_tests.rs`)
- Feed adapter trait implementations
- Configuration management and validation
- Adapter lifecycle (connect, subscribe, run, disconnect)
- Multiple market type support (Spot, Futures)
- Testnet vs mainnet configuration
- Error resilience and reconnection logic

### Integration Tests (`tests/integration/`)

**End-to-End Tests** (`end_to_end_tests.rs`)
- Complete market data flow from WebSocket to gRPC clients
- Multi-symbol and multi-exchange subscription handling
- Event broadcasting and filtering validation
- Service coordination and state management
- High-frequency event processing
- Memory usage under sustained load

**WebSocket Resilience Tests** (`websocket_resilience_tests.rs`)
- Connection timeout and retry mechanisms
- Network interruption simulation and recovery
- Authentication failure handling
- WebSocket protocol compliance
- Reconnection logic validation
- Connection pool management

**gRPC Integration Tests** (`grpc_integration_tests.rs`)
- Multi-client subscription scenarios
- Concurrent stream processing
- Cross-exchange data routing
- Service discovery and health checking
- Load balancing and failover
- Client connection lifecycle management

### Performance Tests (`tests/performance/`)

**High-Frequency Tests** (`high_frequency_tests.rs`)
- Throughput measurement under extreme load
- Latency distribution analysis
- Order book processing speed benchmarks
- Binary data parsing performance
- Memory allocation patterns
- System stability under stress

**Concurrent Connection Tests** (`concurrent_connection_tests.rs`)
- Multiple simultaneous WebSocket connections
- Concurrent gRPC stream processing
- Connection pool exhaustion and recovery
- Resource contention handling
- Scaling behavior validation
- Thread safety verification

**Memory Performance Tests** (`memory_performance_tests.rs`)
- Memory usage profiling under load
- Memory leak detection across service restarts
- Allocation efficiency measurement
- Garbage collection impact analysis
- Memory fragmentation resistance
- Long-running stability testing

## Key Test Scenarios

### WebSocket Connection Testing
- **Connection Establishment**: Verify successful connection to exchange endpoints
- **Authentication**: Validate API key and token-based authentication
- **Message Processing**: Test parsing of various message formats (JSON, binary)
- **Error Recovery**: Simulate network issues and validate reconnection logic
- **Rate Limiting**: Test behavior under exchange rate limits

### Market Data Processing
- **Order Book Reconstruction**: Validate accurate order book building from incremental updates
- **Data Normalization**: Ensure consistent data format across different exchanges
- **Sequence Validation**: Verify proper handling of message sequence numbers
- **Gap Detection**: Test recovery from missed messages
- **Cross-Validation**: Compare data consistency between different feeds

### gRPC Service Functionality
- **Subscription Management**: Test client subscription lifecycle
- **Event Filtering**: Validate routing of events to appropriate subscribers
- **Stream Management**: Test concurrent stream handling
- **Error Propagation**: Verify proper error handling and status codes
- **Protocol Compliance**: Ensure adherence to gRPC specifications

### Performance Validation
- **Throughput Benchmarks**: Measure events processed per second
- **Latency Analysis**: Profile end-to-end message processing time
- **Resource Utilization**: Monitor CPU, memory, and network usage
- **Scalability Testing**: Validate behavior with increasing load
- **Stability Testing**: Long-running tests for memory leaks and crashes

## Running Tests

### Unit Tests
```bash
# Run all unit tests
cargo test --lib

# Run specific test module
cargo test --test binance_websocket_tests

# Run with logging output
RUST_LOG=debug cargo test --test binance_websocket_tests -- --nocapture
```

### Integration Tests
```bash
# Run all integration tests
cargo test --test integration

# Run specific integration test
cargo test --test end_to_end_tests

# Run with network access (for real WebSocket testing)
cargo test --test websocket_resilience_tests -- --ignored
```

### Performance Tests
```bash
# Run performance benchmarks
cargo test --test performance --release

# Run specific performance test
cargo test --test high_frequency_tests --release

# Generate performance report
cargo test --test performance --release -- --nocapture > performance_report.txt
```

## Test Configuration

### Environment Variables
- `RUST_LOG`: Set logging level (debug, info, warn, error)
- `TEST_TIMEOUT`: Override default test timeout in seconds
- `ENABLE_NETWORK_TESTS`: Enable tests requiring network connectivity
- `ZERODHA_API_KEY`: API key for Zerodha integration tests
- `BINANCE_API_KEY`: API key for Binance integration tests

### Test Data
- Mock market data is generated programmatically
- Real market data can be used with appropriate API credentials
- Test symbols follow consistent naming patterns (TEST_SYMBOL_*)
- Performance tests use configurable load parameters

## Continuous Integration

The test suite is designed to run in CI/CD environments with:
- Parallel test execution for faster feedback
- Proper resource cleanup to prevent test interference
- Configurable timeouts for different test categories
- Comprehensive error reporting and logging
- Performance regression detection

## Coverage Goals

- **Unit Test Coverage**: >90% line coverage for core modules
- **Integration Coverage**: All major user workflows tested
- **Performance Coverage**: All critical paths benchmarked
- **Error Coverage**: All error conditions tested
- **Edge Case Coverage**: Boundary conditions and unusual scenarios

## Maintenance

- Tests are updated alongside code changes
- Performance benchmarks are tracked over time
- Test data is refreshed periodically
- Mock services are kept synchronized with real APIs
- Documentation is updated with new test scenarios

This comprehensive testing strategy ensures the market-connector service is robust, performant, and reliable in production environments.