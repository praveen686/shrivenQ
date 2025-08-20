# Common Services Test Suite

This comprehensive test suite provides extensive coverage for all components in the `services-common` library, ensuring reliability, performance, and correctness of the core utilities used across ShrivenQuant services.

## Test Coverage

### 📡 Event Bus Tests (`event_bus_tests.rs`)
- **Core Functionality**: Message publishing, subscribing, and routing
- **Message Handlers**: Handler registration, processing, and error handling
- **Routing Systems**: Topic-based, priority-based, load balancing, and content-based routing
- **Metrics Collection**: Performance monitoring and statistics
- **TTL and Expiration**: Message time-to-live and cleanup
- **Retry Logic**: Handler failure and recovery mechanisms
- **Concurrency**: Multi-threaded publishing and subscribing

### 🌐 Client Tests (`client_tests.rs`)
- **Connection Management**: Client creation, connection lifecycle, timeouts
- **Retry Logic**: Exponential backoff, reconnection attempts
- **Configuration**: Endpoint validation, timeout settings, buffer sizes
- **Error Handling**: gRPC status code conversion, error propagation
- **Streaming**: Real-time data streaming and subscription management
- **Resource Cleanup**: Proper disconnection and resource management

### ⚙️ Configuration Tests (`config_tests.rs`)
- **Service Endpoints**: Default values, customization, validation
- **Service Discovery**: Consul/etcd integration configuration
- **Serialization**: JSON parsing, partial configuration handling
- **Validation**: URL format validation, edge case handling
- **Builder Pattern**: Incremental configuration building

### ❌ Error Tests (`error_tests.rs`)
- **Error Conversion**: Tonic gRPC status to ServiceError mapping
- **Error Propagation**: Async context error handling
- **Error Categorization**: Retry logic, severity classification
- **Error Statistics**: Metrics collection and monitoring
- **Edge Cases**: Unicode messages, long messages, empty messages

### 💾 Storage Tests (`storage_tests.rs`)
- **WAL Operations**: Write-ahead log creation, appending, reading
- **Data Serialization**: Event type serialization/deserialization
- **Iterator Patterns**: Streaming data access with filtering
- **Segment Management**: File rotation and segment handling
- **Performance**: High-volume writes and reads
- **Error Handling**: Corruption recovery, invalid data handling
- **Concurrency**: Multi-threaded WAL access

### 🔗 Integration Tests (`integration_tests.rs`)
- **Multi-threaded Operations**: Concurrent event bus usage
- **Resource Contention**: Shared resource access patterns
- **Performance Under Load**: System behavior with high throughput
- **Memory Pressure**: Large message handling
- **Cross-component Integration**: Service interaction scenarios
- **Race Condition Detection**: Concurrent access safety

## Test Categories

### ✅ Unit Tests
Individual component functionality testing with mocked dependencies.

### ⚡ Performance Tests  
Throughput and latency testing under various load conditions.

### 🔄 Concurrency Tests
Multi-threaded access patterns and race condition detection.

### 🧪 Integration Tests
Cross-component interaction and end-to-end scenarios.

### 💥 Stress Tests
System behavior under extreme conditions and resource pressure.

## Running Tests

### Run All Tests
```bash
cd services/common
cargo test
```

### Run Specific Test Modules
```bash
# Event bus functionality
cargo test event_bus_tests

# Client implementations
cargo test client_tests

# Configuration management
cargo test config_tests

# Error handling
cargo test error_tests

# Storage utilities
cargo test storage_tests

# Integration scenarios
cargo test integration_tests
```

### Run Performance Benchmarks
```bash
# Run with release optimizations for accurate performance measurement
cargo test benchmark --release -- --nocapture

# Specific benchmarks
cargo test benchmark_event_bus_throughput --release -- --nocapture
cargo test benchmark_wal_operations --release -- --nocapture
cargo test benchmark_error_handling_overhead --release -- --nocapture
```

### Run Specific Test Patterns
```bash
# High-volume tests
cargo test high_volume --release -- --nocapture

# Concurrent access tests
cargo test concurrent -- --nocapture

# Memory pressure tests  
cargo test memory_pressure -- --nocapture

# Error handling tests
cargo test error -- --nocapture
```

### Test Output Options
```bash
# Show test output
cargo test -- --nocapture

# Run tests in single thread (for debugging)
cargo test -- --test-threads=1

# Show ignored tests
cargo test -- --ignored

# Run specific test
cargo test test_event_bus_basic -- --nocapture
```

## Test Structure

### Test Organization
- **Module-based**: Each test file covers a specific component
- **Scenario-based**: Tests grouped by functionality and use cases
- **Parameterized**: Using `rstest` for data-driven testing
- **Async-aware**: Full `tokio` integration for async testing

### Test Data
- **Realistic Scenarios**: Real-world trading system messages and events
- **Edge Cases**: Boundary conditions and error scenarios
- **Performance Data**: Large datasets for throughput testing
- **Concurrent Patterns**: Multi-threaded access scenarios

### Assertions
- **Functional Correctness**: Verify expected behavior
- **Performance Thresholds**: Ensure acceptable performance
- **Resource Management**: Verify proper cleanup
- **Error Handling**: Ensure graceful failure modes

## Performance Thresholds

### Event Bus
- **Throughput**: ≥1,000 messages/second
- **Latency**: ≤100ms for message processing
- **Memory**: Efficient memory usage under load

### WAL Operations
- **Write Throughput**: ≥1,000 entries/second  
- **Read Throughput**: ≥5,000 entries/second
- **Storage Efficiency**: Minimal overhead per entry

### Client Operations
- **Connection Time**: ≤10 seconds with retries
- **Request Timeout**: Configurable and respected
- **Resource Cleanup**: No memory leaks

### Error Handling
- **Conversion Speed**: ≥10,000 conversions/second
- **Memory Overhead**: Minimal per error instance

## Test Environment

### Dependencies
- **rstest**: Parameterized testing framework
- **tempfile**: Temporary directory management
- **tokio**: Async runtime for testing
- **anyhow**: Error handling in tests

### Mock Services
Tests use disconnected clients and mock data rather than requiring running services, making them suitable for CI/CD environments.

### Resource Management
All tests properly clean up resources (files, connections, memory) to prevent interference between test runs.

## Test Scenarios Covered

### Event Bus Scenarios
- Single publisher, multiple subscribers
- Multiple publishers, single subscriber  
- Handler failure and retry
- Message expiration and TTL
- High-throughput publishing
- Concurrent subscription management

### Client Scenarios
- Connection failure handling
- Timeout and retry logic
- Configuration validation
- Streaming data management
- Resource cleanup

### Storage Scenarios
- WAL segment rotation
- Concurrent read/write access
- Data corruption handling
- Large file management
- Iterator performance

### Integration Scenarios
- Cross-component communication
- Resource contention
- Memory pressure handling
- Performance under load
- Race condition detection

## Extending Tests

### Adding New Tests
1. Choose appropriate test module based on component
2. Use `rstest` for parameterized tests
3. Include both positive and negative test cases
4. Add performance assertions where appropriate
5. Ensure proper resource cleanup

### Test Naming Convention
- `test_[component]_[scenario]`: Basic functionality
- `test_[component]_[error_condition]`: Error scenarios  
- `test_concurrent_[operation]`: Concurrency tests
- `benchmark_[component]`: Performance tests

### Performance Test Guidelines
- Use `--release` flag for accurate measurements
- Include warm-up iterations for JIT optimization
- Test with realistic data sizes and patterns
- Assert minimum performance thresholds
- Monitor memory usage and cleanup

This comprehensive test suite ensures the reliability and performance of the common services library, providing confidence in the core infrastructure used throughout the ShrivenQuant trading system.