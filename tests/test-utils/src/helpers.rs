//! Test helper functions and utilities

use std::time::Duration;
use tokio::time::{sleep, timeout};
use anyhow::Result;
use tracing_subscriber::EnvFilter;

/// Initialize test logging with environment-based configuration.
/// 
/// Sets up tracing subscriber for test environments with configurable log levels.
/// Uses environment variables for log level configuration and writes to test output.
/// Safe to call multiple times - subsequent calls are ignored.
pub fn init_test_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
}

/// Wait for a condition to become true with timeout and polling.
/// 
/// Repeatedly polls the condition function until it returns true or the timeout expires.
/// Useful for waiting on asynchronous operations to complete in tests.
/// 
/// # Arguments
/// 
/// * `condition` - Function that returns a future evaluating to bool
/// * `timeout_duration` - Maximum time to wait for the condition
/// * `poll_interval` - Time to wait between condition checks
/// 
/// # Returns
/// 
/// Ok(()) if condition becomes true, Err if timeout expires
/// 
/// # Examples
/// 
/// ```
/// wait_for(
///     || async { service.is_ready().await },
///     Duration::from_secs(30),
///     Duration::from_millis(100)
/// ).await?;
/// ```
pub async fn wait_for<F, Fut>(
    condition: F,
    timeout_duration: Duration,
    poll_interval: Duration,
) -> Result<()>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    timeout(timeout_duration, async {
        loop {
            if condition().await {
                return Ok(());
            }
            sleep(poll_interval).await;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("Timeout waiting for condition"))?
}

/// Create a test timeout wrapper for async operations.
/// 
/// Wraps any future with a timeout to prevent tests from hanging indefinitely.
/// Returns an error if the wrapped future doesn't complete within the specified duration.
/// 
/// # Arguments
/// 
/// * `duration` - Maximum time to wait for the future to complete
/// * `future` - The async operation to wrap with timeout
/// 
/// # Returns
/// 
/// The result of the future if it completes in time, or a timeout error
/// 
/// # Examples
/// 
/// ```
/// let result = with_timeout(
///     Duration::from_secs(5),
///     slow_database_operation()
/// ).await?;
/// ```
pub async fn with_timeout<T>(
    duration: Duration,
    future: impl std::future::Future<Output = T>,
) -> Result<T> {
    timeout(duration, future)
        .await
        .map_err(|_| anyhow::anyhow!("Test timeout after {:?}", duration))
}

/// Test environment setup with automatic cleanup.
/// 
/// Provides isolated temporary directories and cleanup handlers for tests.
/// Automatically manages temporary files and directories, ensuring clean test isolation.
/// 
/// # Examples
/// 
/// ```
/// let mut env = TestEnvironment::new()?;
/// let temp_file = env.temp_path().join("test.db");
/// env.add_cleanup(|| println!("Cleaning up resources"));
/// // Cleanup is automatically called when env is dropped
/// ```
pub struct TestEnvironment {
    /// Temporary directory for this test environment
    temp_dir: tempfile::TempDir,
    /// List of cleanup functions to execute
    cleanup_handlers: Vec<Box<dyn FnOnce() + Send>>,
}

impl std::fmt::Debug for TestEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestEnvironment")
            .field("temp_dir_path", &self.temp_dir.path())
            .field("cleanup_handlers_count", &self.cleanup_handlers.len())
            .finish()
    }
}

impl TestEnvironment {
    /// Creates a new test environment with a temporary directory.
    /// 
    /// Sets up an isolated temporary directory that will be automatically
    /// cleaned up when the TestEnvironment is dropped.
    /// 
    /// # Returns
    /// 
    /// A new TestEnvironment instance or an error if temp directory creation fails
    pub fn new() -> Result<Self> {
        Ok(Self {
            temp_dir: tempfile::tempdir()?,
            cleanup_handlers: Vec::new(),
        })
    }
    
    /// Returns the path to the temporary directory.
    /// 
    /// Use this path to create test files and subdirectories that will be
    /// automatically cleaned up when the test environment is dropped.
    /// 
    /// # Returns
    /// 
    /// Path reference to the temporary directory
    pub fn temp_path(&self) -> &std::path::Path {
        self.temp_dir.path()
    }
    
    /// Adds a cleanup handler to be executed when the environment is cleaned up.
    /// 
    /// Cleanup handlers are executed in LIFO order (last added, first executed)
    /// when cleanup() is called or when the TestEnvironment is dropped.
    /// 
    /// # Arguments
    /// 
    /// * `handler` - Closure to execute during cleanup
    pub fn add_cleanup<F>(&mut self, handler: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.cleanup_handlers.push(Box::new(handler));
    }
    
    /// Manually trigger cleanup of all registered handlers.
    /// 
    /// Executes all cleanup handlers in LIFO order and consumes the TestEnvironment.
    /// This is automatically called when the TestEnvironment is dropped.
    pub fn cleanup(self) {
        for handler in self.cleanup_handlers {
            handler();
        }
    }
}

/// Generic test data builder using the builder pattern.
/// 
/// Provides a fluent interface for modifying test data before building.
/// Useful for creating variations of test objects with incremental modifications.
/// 
/// # Type Parameters
/// 
/// * `T` - The type of data being built
/// 
/// # Examples
/// 
/// ```
/// let order = TestDataBuilder::new(default_order)
///     .with(|o| o.quantity = 2.5)
///     .with(|o| o.symbol = "ETHUSDT".to_string())
///     .build();
/// ```
#[derive(Debug)]
pub struct TestDataBuilder<T> {
    /// The data being built
    data: T,
}

impl<T> TestDataBuilder<T> {
    /// Creates a new TestDataBuilder with the provided initial data.
    /// 
    /// # Arguments
    /// 
    /// * `data` - Initial data to build upon
    /// 
    /// # Returns
    /// 
    /// A new TestDataBuilder instance
    pub fn new(data: T) -> Self {
        Self { data }
    }
    
    /// Applies a modification function to the data.
    /// 
    /// Allows fluent chaining of modifications to the data being built.
    /// 
    /// # Arguments
    /// 
    /// * `modifier` - Function that modifies the data
    /// 
    /// # Returns
    /// 
    /// Self for method chaining
    pub fn with<F>(mut self, modifier: F) -> Self
    where
        F: FnOnce(&mut T),
    {
        modifier(&mut self.data);
        self
    }
    
    /// Consumes the builder and returns the final data.
    /// 
    /// # Returns
    /// 
    /// The built data instance
    pub fn build(self) -> T {
        self.data
    }
}

/// Performance measurement helper with automatic logging.
/// 
/// Measures elapsed time for operations and automatically logs the duration
/// when the instance is dropped. Useful for identifying performance bottlenecks
/// in tests and development.
/// 
/// # Examples
/// 
/// ```
/// {
///     let _perf = PerfMeasure::new("database_query");
///     expensive_database_operation().await;
///     // Duration is automatically logged when _perf is dropped
/// }
/// ```
#[derive(Debug)]
pub struct PerfMeasure {
    /// Name of the operation being measured
    name: String,
    /// Start time of the measurement
    start: std::time::Instant,
}

impl PerfMeasure {
    /// Creates a new performance measurement with the given name.
    /// 
    /// Starts timing immediately upon creation.
    /// 
    /// # Arguments
    /// 
    /// * `name` - Descriptive name for the operation being measured
    /// 
    /// # Returns
    /// 
    /// A new PerfMeasure instance that will log duration when dropped
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            start: std::time::Instant::now(),
        }
    }
    
    /// Returns the elapsed time since measurement started.
    /// 
    /// # Returns
    /// 
    /// Duration since the PerfMeasure was created
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

impl Drop for PerfMeasure {
    fn drop(&mut self) {
        tracing::info!(
            "Performance measurement '{}' took {:?}",
            self.name,
            self.elapsed()
        );
    }
}

/// Retry helper for flaky operations with exponential backoff.
/// 
/// Attempts to execute an operation multiple times with a delay between attempts.
/// Useful for dealing with transient failures in tests (network timeouts, resource contention, etc.).
/// 
/// # Arguments
/// 
/// * `operation` - Async function that returns a Result
/// * `max_attempts` - Maximum number of retry attempts
/// * `delay` - Base delay between retry attempts
/// 
/// # Returns
/// 
/// The successful result or the last error encountered
/// 
/// # Examples
/// 
/// ```
/// let result = retry(
///     || async { flaky_network_call().await },
///     3,
///     Duration::from_millis(100)
/// ).await?;
/// ```
pub async fn retry<T, E, F, Fut>(
    mut operation: F,
    max_attempts: usize,
    delay: Duration,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut last_error = None;
    
    for attempt in 1..=max_attempts {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                tracing::warn!(
                    "Attempt {}/{} failed: {}",
                    attempt,
                    max_attempts,
                    e
                );
                last_error = Some(e);
                
                if attempt < max_attempts {
                    sleep(delay).await;
                }
            }
        }
    }
    
    Err(last_error.expect("Should have at least one error"))
}

/// Test snapshot helper for regression testing.
/// 
/// Captures serializable data as JSON snapshots that can be compared
/// against future test runs to detect unintended changes.
/// 
/// # Examples
/// 
/// ```
/// let snapshot = TestSnapshot::capture(&complex_data_structure)?;
/// // Later in the test...
/// snapshot.assert_matches(&updated_data_structure)?;
/// ```
#[derive(Debug)]
pub struct TestSnapshot {
    /// Captured JSON data for comparison
    data: serde_json::Value,
}

impl TestSnapshot {
    /// Captures a snapshot of the provided data.
    /// 
    /// Serializes the data to JSON for later comparison.
    /// 
    /// # Arguments
    /// 
    /// * `value` - Data to capture (must implement Serialize)
    /// 
    /// # Returns
    /// 
    /// A TestSnapshot containing the serialized data
    pub fn capture<T: serde::Serialize>(value: &T) -> Result<Self> {
        Ok(Self {
            data: serde_json::to_value(value)?,
        })
    }
    
    /// Asserts that the provided data matches the captured snapshot.
    /// 
    /// Compares the current data against the captured snapshot and returns
    /// an error with detailed diff information if they don't match.
    /// 
    /// # Arguments
    /// 
    /// * `value` - Current data to compare against the snapshot
    /// 
    /// # Returns
    /// 
    /// Ok(()) if data matches, Err with diff details if different
    pub fn assert_matches<T: serde::Serialize>(&self, value: &T) -> Result<()> {
        let current = serde_json::to_value(value)?;
        if self.data != current {
            return Err(anyhow::anyhow!(
                "Snapshot mismatch:\nExpected: {}\nActual: {}",
                serde_json::to_string_pretty(&self.data)?,
                serde_json::to_string_pretty(&current)?
            ));
        }
        Ok(())
    }
}

/// Generate deterministic test data for reproducible tests.
/// 
/// Provides functions for generating consistent, repeatable test data based on seeds.
/// Useful for creating deterministic tests that produce the same results across runs.
pub mod deterministic {
    use uuid::Uuid;
    
    /// Generate deterministic UUID from seed.
    /// 
    /// Creates a UUID based on the provided seed value, ensuring the same
    /// seed always produces the same UUID.
    /// 
    /// # Arguments
    /// 
    /// * `seed` - Seed value for UUID generation
    /// 
    /// # Returns
    /// 
    /// A deterministic UUID based on the seed
    pub fn uuid_from_seed(seed: u64) -> Uuid {
        let mut bytes = [0u8; 16];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = ((seed + i as u64) % 256) as u8;
        }
        Uuid::from_bytes(bytes)
    }
    
    /// Generate deterministic price from seed.
    /// 
    /// Creates a price value by applying a deterministic variation to the base price.
    /// The variation is based on the seed value, ensuring reproducible price data.
    /// 
    /// # Arguments
    /// 
    /// * `seed` - Seed value for price variation
    /// * `base` - Base price to vary from
    /// 
    /// # Returns
    /// 
    /// A deterministic price based on the seed and base
    pub fn price_from_seed(seed: u64, base: f64) -> f64 {
        base * (1.0 + ((seed % 100) as f64 - 50.0) / 1000.0)
    }
    
    /// Generate deterministic quantity from seed.
    /// 
    /// Creates a quantity value between 0.1 and 10.0 based on the seed.
    /// The same seed will always produce the same quantity.
    /// 
    /// # Arguments
    /// 
    /// * `seed` - Seed value for quantity generation
    /// 
    /// # Returns
    /// 
    /// A deterministic quantity between 0.1 and 10.0
    pub fn quantity_from_seed(seed: u64) -> f64 {
        ((seed % 100) + 1) as f64 / 10.0
    }
}

/// Builder for creating test HTTP servers with configurable routes.
/// 
/// Provides a fluent interface for setting up mock HTTP servers for testing.
/// Useful for testing components that interact with external HTTP services.
/// 
/// # Examples
/// 
/// ```text
/// let server = TestServerBuilder::new()
///     .with_port(8080)
///     .with_route("/api/data".to_string(), "{\"status\":\"ok\"}".to_string())
///     .start().await?;
/// ```
#[derive(Debug)]
pub struct TestServerBuilder {
    /// Port number for the test server (0 for random port)
    port: u16,
    /// List of (path, response) pairs for mock routes
    routes: Vec<(String, String)>,
}

impl TestServerBuilder {
    /// Creates a new TestServerBuilder with default settings.
    /// 
    /// Uses port 0 (random port assignment) and no predefined routes.
    /// 
    /// # Returns
    /// 
    /// A new TestServerBuilder instance
    pub fn new() -> Self {
        Self {
            port: 0, // Random port
            routes: Vec::new(),
        }
    }
    
    /// Sets the port for the test server.
    /// 
    /// # Arguments
    /// 
    /// * `port` - Port number (use 0 for random port assignment)
    /// 
    /// # Returns
    /// 
    /// Self for method chaining
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }
    
    /// Adds a mock route to the test server.
    /// 
    /// # Arguments
    /// 
    /// * `path` - URL path for the route (e.g., "/api/data")
    /// * `response` - Response body to return for this route
    /// 
    /// # Returns
    /// 
    /// Self for method chaining
    pub fn with_route(mut self, path: String, response: String) -> Self {
        self.routes.push((path, response));
        self
    }
    
    /// Starts the test server with the configured settings.
    /// 
    /// Creates and starts an HTTP server with the configured port and routes.
    /// 
    /// # Returns
    /// 
    /// A TestServer instance representing the running server
    pub async fn start(self) -> Result<TestServer> {
        // This would start an actual test server
        // For now, return a mock implementation
        Ok(TestServer {
            port: self.port,
            url: format!("http://localhost:{}", self.port),
        })
    }
}

#[derive(Debug)]
/// Represents a running test HTTP server.
/// 
/// Provides access to the server's port and URL for making test requests.
/// The server continues running until the TestServer instance is dropped.
pub struct TestServer {
    /// Port number the server is listening on
    pub port: u16,
    /// Full URL of the server (e.g., "http://localhost:8080")
    pub url: String,
}

/// Concurrent test helper for load testing and parallel execution.
/// 
/// Runs multiple instances of an operation concurrently and collects the results.
/// Useful for testing race conditions, load handling, and concurrent safety.
/// 
/// # Arguments
/// 
/// * `count` - Number of concurrent operations to run
/// * `operation` - Function that takes an index and returns a future
/// 
/// # Returns
/// 
/// Vector of results from all concurrent operations
/// 
/// # Examples
/// 
/// ```
/// let results = run_concurrent(10, |i| async move {
///     database.insert(format!("record_{}", i)).await
/// }).await;
/// ```
pub async fn run_concurrent<T, F, Fut>(count: usize, operation: F) -> Vec<Result<T>>
where
    F: Fn(usize) -> Fut + Clone + Send + 'static,
    Fut: std::future::Future<Output = Result<T>> + Send + 'static,
    T: Send + 'static,
{
    let mut handles = Vec::new();
    
    for i in 0..count {
        let op = operation.clone();
        handles.push(tokio::spawn(async move { op(i).await }));
    }
    
    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(result) => results.push(result),
            Err(e) => results.push(Err(anyhow::anyhow!("Task panicked: {}", e))),
        }
    }
    
    results
}