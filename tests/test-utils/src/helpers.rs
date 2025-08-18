//! Test helper functions and utilities

use std::time::Duration;
use tokio::time::{sleep, timeout};
use anyhow::Result;
use tracing_subscriber::EnvFilter;

/// Initialize test logging
pub fn init_test_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
}

/// Wait for a condition to become true with timeout
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

/// Create a test timeout wrapper
pub async fn with_timeout<T>(
    duration: Duration,
    future: impl std::future::Future<Output = T>,
) -> Result<T> {
    timeout(duration, future)
        .await
        .map_err(|_| anyhow::anyhow!("Test timeout after {:?}", duration))
}

/// Test environment setup
pub struct TestEnvironment {
    temp_dir: tempfile::TempDir,
    cleanup_handlers: Vec<Box<dyn FnOnce() + Send>>,
}

impl TestEnvironment {
    pub fn new() -> Result<Self> {
        Ok(Self {
            temp_dir: tempfile::tempdir()?,
            cleanup_handlers: Vec::new(),
        })
    }
    
    pub fn temp_path(&self) -> &std::path::Path {
        self.temp_dir.path()
    }
    
    pub fn add_cleanup<F>(&mut self, handler: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.cleanup_handlers.push(Box::new(handler));
    }
    
    pub fn cleanup(self) {
        for handler in self.cleanup_handlers {
            handler();
        }
    }
}

/// Test data builder pattern
pub struct TestDataBuilder<T> {
    data: T,
}

impl<T> TestDataBuilder<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }
    
    pub fn with<F>(mut self, modifier: F) -> Self
    where
        F: FnOnce(&mut T),
    {
        modifier(&mut self.data);
        self
    }
    
    pub fn build(self) -> T {
        self.data
    }
}

/// Performance measurement helper
pub struct PerfMeasure {
    name: String,
    start: std::time::Instant,
}

impl PerfMeasure {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            start: std::time::Instant::now(),
        }
    }
    
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

/// Retry helper for flaky operations
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

/// Test snapshot helper
pub struct TestSnapshot {
    data: serde_json::Value,
}

impl TestSnapshot {
    pub fn capture<T: serde::Serialize>(value: &T) -> Result<Self> {
        Ok(Self {
            data: serde_json::to_value(value)?,
        })
    }
    
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

/// Generate deterministic test data
pub mod deterministic {
    use uuid::Uuid;
    
    /// Generate deterministic UUID from seed
    pub fn uuid_from_seed(seed: u64) -> Uuid {
        let mut bytes = [0u8; 16];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = ((seed + i as u64) % 256) as u8;
        }
        Uuid::from_bytes(bytes)
    }
    
    /// Generate deterministic price from seed
    pub fn price_from_seed(seed: u64, base: f64) -> f64 {
        base * (1.0 + ((seed % 100) as f64 - 50.0) / 1000.0)
    }
    
    /// Generate deterministic quantity from seed
    pub fn quantity_from_seed(seed: u64) -> f64 {
        ((seed % 100) + 1) as f64 / 10.0
    }
}

/// Test server builder
pub struct TestServerBuilder {
    port: u16,
    routes: Vec<(String, String)>,
}

impl TestServerBuilder {
    pub fn new() -> Self {
        Self {
            port: 0, // Random port
            routes: Vec::new(),
        }
    }
    
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }
    
    pub fn with_route(mut self, path: String, response: String) -> Self {
        self.routes.push((path, response));
        self
    }
    
    pub async fn start(self) -> Result<TestServer> {
        // This would start an actual test server
        // For now, return a mock implementation
        Ok(TestServer {
            port: self.port,
            url: format!("http://localhost:{}", self.port),
        })
    }
}

pub struct TestServer {
    pub port: u16,
    pub url: String,
}

/// Concurrent test helper
pub async fn run_concurrent<T, F, Fut>(count: usize, operation: F) -> Vec<Result<T>>
where
    F: Fn(usize) -> Fut + Clone,
    Fut: std::future::Future<Output = Result<T>>,
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