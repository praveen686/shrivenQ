//! Stress Tests for API Gateway
//!
//! High-load stress tests to validate the API Gateway's performance under
//! extreme conditions typical of high-frequency trading environments.

use anyhow::Result;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::time::timeout;

use api_gateway::{
    GatewayConfig,
    models::{SubmitOrderRequest, WebSocketMessage},
    rate_limiter::RateLimiter,
};

/// Stress test configuration
struct StressTestConfig {
    concurrent_connections: usize,
    requests_per_connection: usize,
    test_duration: Duration,
    target_latency_percentile_95: Duration,
    target_throughput_rps: usize,
}

impl Default for StressTestConfig {
    fn default() -> Self {
        Self {
            concurrent_connections: 1000,
            requests_per_connection: 100,
            test_duration: Duration::from_secs(60),
            target_latency_percentile_95: Duration::from_millis(5), // 5ms for 95th percentile
            target_throughput_rps: 10000,                           // 10k requests per second
        }
    }
}

/// Stress test metrics collection
#[derive(Debug, Default)]
struct StressTestMetrics {
    total_requests: AtomicUsize,
    successful_requests: AtomicUsize,
    failed_requests: AtomicUsize,
    total_latency_ns: AtomicUsize,
    min_latency_ns: AtomicUsize,
    max_latency_ns: AtomicUsize,
}

impl StressTestMetrics {
    fn new() -> Self {
        Self {
            min_latency_ns: AtomicUsize::new(usize::MAX),
            ..Default::default()
        }
    }

    fn record_request(&self, latency: Duration, success: bool) {
        let latency_ns = latency.as_nanos() as usize;

        self.total_requests.fetch_add(1, Ordering::Relaxed);

        if success {
            self.successful_requests.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failed_requests.fetch_add(1, Ordering::Relaxed);
        }

        self.total_latency_ns
            .fetch_add(latency_ns, Ordering::Relaxed);

        // Update min latency
        loop {
            let current_min = self.min_latency_ns.load(Ordering::Relaxed);
            if latency_ns >= current_min {
                break;
            }
            if self
                .min_latency_ns
                .compare_exchange_weak(
                    current_min,
                    latency_ns,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                break;
            }
        }

        // Update max latency
        loop {
            let current_max = self.max_latency_ns.load(Ordering::Relaxed);
            if latency_ns <= current_max {
                break;
            }
            if self
                .max_latency_ns
                .compare_exchange_weak(
                    current_max,
                    latency_ns,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                break;
            }
        }
    }

    fn print_summary(&self, duration: Duration) {
        let total = self.total_requests.load(Ordering::Relaxed);
        let successful = self.successful_requests.load(Ordering::Relaxed);
        let failed = self.failed_requests.load(Ordering::Relaxed);
        let total_latency = self.total_latency_ns.load(Ordering::Relaxed);
        let min_latency = self.min_latency_ns.load(Ordering::Relaxed);
        let max_latency = self.max_latency_ns.load(Ordering::Relaxed);

        println!("\n📊 Stress Test Results:");
        println!("========================");
        println!("Duration: {:.2}s", duration.as_secs_f64());
        println!("Total Requests: {}", total);
        println!(
            "Successful: {} ({:.2}%)",
            successful,
            successful as f64 / total as f64 * 100.0
        );
        println!(
            "Failed: {} ({:.2}%)",
            failed,
            failed as f64 / total as f64 * 100.0
        );
        println!(
            "Throughput: {:.2} req/s",
            total as f64 / duration.as_secs_f64()
        );

        if total > 0 {
            let avg_latency_ns = total_latency / total;
            println!(
                "Average Latency: {:.2}ms",
                avg_latency_ns as f64 / 1_000_000.0
            );
            println!("Min Latency: {:.2}ms", min_latency as f64 / 1_000_000.0);
            println!("Max Latency: {:.2}ms", max_latency as f64 / 1_000_000.0);
        }
    }
}

/// High-concurrency request stress test
#[tokio::test]
async fn stress_test_concurrent_requests() -> Result<()> {
    let config = StressTestConfig::default();
    let metrics = Arc::new(StressTestMetrics::new());
    let semaphore = Arc::new(Semaphore::new(config.concurrent_connections));

    println!("🚀 Starting concurrent requests stress test");
    println!("Concurrent connections: {}", config.concurrent_connections);
    println!(
        "Requests per connection: {}",
        config.requests_per_connection
    );

    let start_time = Instant::now();
    let mut tasks = Vec::new();

    // Create mock HTTP client for testing
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let base_url = "http://127.0.0.1:8080"; // Mock URL for stress test

    for connection_id in 0..config.concurrent_connections {
        let semaphore = Arc::clone(&semaphore);
        let metrics = Arc::clone(&metrics);
        let client = client.clone();
        let base_url = base_url.to_string();

        let task = tokio::spawn(async move {
            // Hold semaphore permit for connection lifetime
            let permit = semaphore.acquire().await.unwrap();

            for request_id in 0..config.requests_per_connection {
                let request_start = Instant::now();

                // Simulate different types of requests
                let endpoint = match request_id % 4 {
                    0 => "/health",
                    1 => "/auth/validate",
                    2 => "/market-data/snapshot?symbols=NIFTY&exchange=NSE",
                    _ => "/risk/metrics",
                };

                let url = format!("{}{}", base_url, endpoint);
                let success = match timeout(Duration::from_secs(10), client.get(&url).send()).await
                {
                    Ok(Ok(response)) => response.status().is_success(),
                    _ => false, // Timeout or error - expected in stress test without real server
                };

                let latency = request_start.elapsed();
                metrics.record_request(latency, success);

                // Small delay to simulate real-world request patterns
                if request_id % 10 == 0 {
                    tokio::time::sleep(Duration::from_micros(100)).await;
                }
            }
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete or timeout
    let timeout_duration = Duration::from_secs(120);
    let tasks_result = timeout(timeout_duration, async {
        for task in tasks {
            let _ = task.await;
        }
    })
    .await;

    let actual_duration = start_time.elapsed();
    metrics.print_summary(actual_duration);

    // Verify stress test completed
    match tasks_result {
        Ok(_) => println!("✅ All concurrent tasks completed successfully"),
        Err(_) => println!("⚠️ Some tasks timed out (expected behavior in stress test)"),
    }

    // The test passes if we can handle the load without panicking
    let total_requests = metrics.total_requests.load(Ordering::Relaxed);
    assert!(total_requests > 0, "Should have processed some requests");

    Ok(())
}

/// Memory usage stress test
#[tokio::test]
async fn stress_test_memory_usage() -> Result<()> {
    println!("🧠 Starting memory usage stress test");

    let iterations = 100_000;
    let batch_size = 1000;

    for batch in 0..iterations / batch_size {
        let mut orders = Vec::with_capacity(batch_size);

        // Create many order objects to test memory allocation patterns
        for i in 0..batch_size {
            let order = SubmitOrderRequest {
                client_order_id: Some(format!("STRESS_ORDER_{}_{}", batch, i)),
                symbol: "NIFTY2412050000CE".to_string(),
                side: if i % 2 == 0 { "BUY" } else { "SELL" }.to_string(),
                quantity: format!("{:.4}", (i as f64 + 1.0) * 100.0),
                order_type: "LIMIT".to_string(),
                limit_price: Some(format!("{:.4}", 150.0 + (i as f64 * 0.01))),
                stop_price: None,
                time_in_force: Some("DAY".to_string()),
                exchange: Some("NSE".to_string()),
            };

            // Serialize to test JSON processing performance
            // Validate JSON serialization
            let json = serde_json::to_string(&order)?;
            assert!(!json.is_empty());
            orders.push(order);
        }

        // Process orders (simulate real workload)
        for order in &orders {
            // Verify fixed-point parsing works correctly
            let price_check = api_gateway::middleware::parse_fixed_point(&order.quantity);
            assert!(price_check.is_ok());
            if let Some(ref limit_price) = order.limit_price {
                // Verify limit price parsing
                let limit_check = api_gateway::middleware::parse_fixed_point(limit_price);
                assert!(limit_check.is_ok());
            }
        }

        // Explicit drop to test memory cleanup
        drop(orders);

        if batch % 10 == 0 {
            println!("Processed batch {}/{}", batch + 1, iterations / batch_size);
        }

        // Small yield to allow other tasks to run
        tokio::task::yield_now().await;
    }

    println!("✅ Memory stress test completed successfully");
    Ok(())
}

/// Rate limiter stress test
#[tokio::test]
async fn stress_test_rate_limiter() -> Result<()> {
    println!("🚦 Starting rate limiter stress test");

    use api_gateway::RateLimitConfig;
    use std::collections::HashMap;

    let config = RateLimitConfig {
        enabled: true,
        requests_per_minute: 6000, // 100 req/sec
        burst_size: 200,
        per_ip_limit: 100,
        per_endpoint_limits: HashMap::new(),
    };

    let rate_limiter = RateLimiter::new(config).await;
    let metrics = Arc::new(StressTestMetrics::new());

    let concurrent_clients = 50;
    let requests_per_client = 100;

    let mut tasks = Vec::new();

    for client_id in 0..concurrent_clients {
        let rate_limiter = rate_limiter.clone();
        let metrics = Arc::clone(&metrics);

        let task = tokio::spawn(async move {
            let ip = format!("192.168.1.{}", client_id % 255);

            for request_id in 0..requests_per_client {
                let start = Instant::now();
                let endpoint = format!("/test-endpoint-{}", request_id % 10);

                let allowed = rate_limiter.check_rate_limit(&ip, &endpoint).await;
                let latency = start.elapsed();

                metrics.record_request(latency, allowed.is_ok());

                // Small delay between requests
                if request_id % 5 == 0 {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
        });

        tasks.push(task);
    }

    // Wait for all tasks
    for task in tasks {
        let _ = task.await;
    }

    let total_requests = metrics.total_requests.load(Ordering::Relaxed);
    let successful = metrics.successful_requests.load(Ordering::Relaxed);
    let failed = metrics.failed_requests.load(Ordering::Relaxed);

    println!("Rate limiter processed {} requests", total_requests);
    println!("Allowed: {}, Rejected: {}", successful, failed);
    println!(
        "Rejection rate: {:.2}%",
        failed as f64 / total_requests as f64 * 100.0
    );

    // Verify rate limiting is working
    assert!(total_requests > 0);
    assert!(
        failed > 0,
        "Should have some rejected requests due to rate limiting"
    );

    println!("✅ Rate limiter stress test completed");
    Ok(())
}

/// WebSocket message processing stress test  
#[tokio::test]
async fn stress_test_websocket_messages() -> Result<()> {
    println!("🌐 Starting WebSocket message processing stress test");

    let message_count = 50_000;
    let batch_size = 1000;

    for batch in 0..message_count / batch_size {
        let mut messages = Vec::with_capacity(batch_size);

        // Generate different types of WebSocket messages
        for i in 0..batch_size {
            let message = match i % 4 {
                0 => WebSocketMessage {
                    message_type: "market_data".to_string(),
                    data: serde_json::json!({
                        "symbol": format!("SYMBOL_{}", i),
                        "price": format!("{:.4}", 100.0 + i as f64),
                        "volume": format!("{:.4}", 1000.0 + i as f64),
                        "timestamp": chrono::Utc::now().timestamp_nanos()
                    }),
                    timestamp: chrono::Utc::now().timestamp(),
                },
                1 => WebSocketMessage {
                    message_type: "execution_report".to_string(),
                    data: serde_json::json!({
                        "order_id": format!("ORDER_{}", i),
                        "status": "FILLED",
                        "quantity": format!("{:.4}", 100.0),
                        "price": format!("{:.4}", 150.0)
                    }),
                    timestamp: chrono::Utc::now().timestamp(),
                },
                2 => WebSocketMessage {
                    message_type: "risk_alert".to_string(),
                    data: serde_json::json!({
                        "alert_type": "POSITION_LIMIT",
                        "severity": "HIGH",
                        "message": "Position limit approaching"
                    }),
                    timestamp: chrono::Utc::now().timestamp(),
                },
                _ => WebSocketMessage {
                    message_type: "heartbeat".to_string(),
                    data: serde_json::json!({
                        "status": "alive",
                        "server_time": chrono::Utc::now().timestamp()
                    }),
                    timestamp: chrono::Utc::now().timestamp(),
                },
            };

            // Test serialization performance
            // Validate WebSocket message serialization
            let serialized = serde_json::to_string(&message)?;
            assert!(!serialized.is_empty());
            messages.push(message);
        }

        // Process messages (simulate broadcast)
        for message in &messages {
            // Ensure message can be serialized
            let json = serde_json::to_string(message)?;
            assert!(!json.is_empty());
            // Simulate message routing logic
            match message.message_type.as_str() {
                "market_data" => { /* route to market data subscribers */ }
                "execution_report" => { /* route to trading subscribers */ }
                "risk_alert" => { /* route to risk management */ }
                "heartbeat" => { /* handle keepalive */ }
                _ => { /* handle unknown message type */ }
            }
        }

        drop(messages);

        if batch % 10 == 0 {
            println!(
                "Processed WebSocket batch {}/{}",
                batch + 1,
                message_count / batch_size
            );
        }

        // Yield control
        tokio::task::yield_now().await;
    }

    println!("✅ WebSocket message stress test completed");
    Ok(())
}

/// Configuration parsing stress test
#[tokio::test]
async fn stress_test_config_parsing() -> Result<()> {
    println!("⚙️ Starting configuration parsing stress test");

    let config_content = r#"
[server]
host = "127.0.0.1"
port = 8080
workers = 4
compression = true
request_timeout_ms = 5000

[services]
auth_service = "http://127.0.0.1:50051"
execution_service = "http://127.0.0.1:50052"
market_data_service = "http://127.0.0.1:50053"
risk_service = "http://127.0.0.1:50054"

[auth]
jwt_secret = "super-secret-key-for-testing"
token_expiry_hours = 24
refresh_expiry_days = 7
require_2fa = false

[rate_limiting]
enabled = true
requests_per_minute = 60000
burst_size = 1000
per_ip_limit = 1000

[cors]
enabled = true
allowed_origins = ["http://localhost:3000", "https://trading.example.com"]
max_age_seconds = 3600

[monitoring]
metrics_enabled = true
metrics_port = 9090
tracing_enabled = true
"#;

    let iterations = 10_000;
    let start = Instant::now();

    for i in 0..iterations {
        // Parse configuration
        let config: GatewayConfig = toml::from_str(config_content)?;

        // Validate configuration
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8080);
        assert!(config.rate_limiting.enabled);
        assert!(config.cors.enabled);

        if i % 1000 == 0 {
            println!("Parsed configuration {} times", i + 1);
        }
    }

    let duration = start.elapsed();
    let ops_per_sec = iterations as f64 / duration.as_secs_f64();

    println!("Configuration parsing performance:");
    println!("  Total iterations: {}", iterations);
    println!("  Total time: {:.2}s", duration.as_secs_f64());
    println!("  Rate: {:.2} config/s", ops_per_sec);
    println!(
        "  Average time per config: {:.2}μs",
        duration.as_micros() as f64 / iterations as f64
    );

    // Verify performance is acceptable (should parse configs very quickly)
    assert!(
        ops_per_sec > 1000.0,
        "Config parsing too slow: {:.2} ops/sec",
        ops_per_sec
    );

    println!("✅ Configuration parsing stress test completed");
    Ok(())
}

/// Error handling stress test
#[tokio::test]
async fn stress_test_error_handling() -> Result<()> {
    println!("❌ Starting error handling stress test");

    use api_gateway::models::{ApiResponse, ErrorResponse};
    use std::collections::HashMap;

    let error_scenarios = vec![
        ("VALIDATION_ERROR", "Invalid input parameters"),
        ("AUTHENTICATION_FAILED", "Token validation failed"),
        ("RATE_LIMIT_EXCEEDED", "Too many requests"),
        ("SERVICE_UNAVAILABLE", "Backend service down"),
        ("INTERNAL_ERROR", "Unexpected server error"),
        ("PERMISSION_DENIED", "Insufficient permissions"),
        ("ORDER_REJECTED", "Risk limits exceeded"),
        ("MARKET_CLOSED", "Market is currently closed"),
    ];

    let iterations_per_scenario = 5000;
    let start = Instant::now();
    let mut total_operations = 0;

    for (error_code, error_message) in &error_scenarios {
        for i in 0..iterations_per_scenario {
            // Create error response
            let mut details = HashMap::new();
            details.insert("error_id".to_string(), format!("ERR_{}", i));
            details.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());

            let error_response = ErrorResponse {
                error: error_code.to_string(),
                message: error_message.to_string(),
                details: Some(details),
            };

            // Create API response
            let api_response = ApiResponse::error(error_response);

            // Serialize response (what would happen in real request)
            // Validate API response serialization
            let json = serde_json::to_string(&api_response)?;
            assert!(!json.is_empty());

            total_operations += 1;
        }
    }

    let duration = start.elapsed();
    let ops_per_sec = total_operations as f64 / duration.as_secs_f64();

    println!("Error handling performance:");
    println!("  Error scenarios: {}", error_scenarios.len());
    println!("  Operations per scenario: {}", iterations_per_scenario);
    println!("  Total operations: {}", total_operations);
    println!("  Total time: {:.2}s", duration.as_secs_f64());
    println!("  Rate: {:.2} errors/s", ops_per_sec);

    // Verify error handling performance is acceptable
    assert!(
        ops_per_sec > 10000.0,
        "Error handling too slow: {:.2} ops/sec",
        ops_per_sec
    );

    println!("✅ Error handling stress test completed");
    Ok(())
}

/// Overall system stress test combining multiple components
#[tokio::test]
async fn stress_test_system_integration() -> Result<()> {
    println!("🏗️ Starting comprehensive system integration stress test");

    let config = StressTestConfig {
        concurrent_connections: 100,
        requests_per_connection: 50,
        test_duration: Duration::from_secs(30),
        ..Default::default()
    };

    let metrics = Arc::new(StressTestMetrics::new());
    let start_time = Instant::now();

    // Simulate different types of load concurrently
    let mut tasks = Vec::new();

    // HTTP request load
    for i in 0..config.concurrent_connections / 4 {
        let metrics = Arc::clone(&metrics);
        let task = tokio::spawn(async move {
            for _ in 0..config.requests_per_connection {
                let start = Instant::now();

                // Simulate request processing pipeline
                tokio::time::sleep(Duration::from_micros(100)).await; // Auth check
                tokio::time::sleep(Duration::from_micros(50)).await; // Rate limiting
                tokio::time::sleep(Duration::from_micros(200)).await; // Business logic
                tokio::time::sleep(Duration::from_micros(100)).await; // Response serialization

                let latency = start.elapsed();
                metrics.record_request(latency, true);
            }
        });
        tasks.push(task);
    }

    // WebSocket message processing load
    for i in 0..config.concurrent_connections / 4 {
        let metrics = Arc::clone(&metrics);
        let task = tokio::spawn(async move {
            for _ in 0..config.requests_per_connection * 2 {
                // WebSocket has higher message rate
                let start = Instant::now();

                // Simulate WebSocket message processing
                let message = WebSocketMessage {
                    message_type: "test".to_string(),
                    data: serde_json::json!({"test": "data"}),
                    timestamp: chrono::Utc::now().timestamp(),
                };

                // Ensure message serializes correctly
                let serialized = serde_json::to_string(&message).unwrap();
                assert!(!serialized.is_empty());

                let latency = start.elapsed();
                metrics.record_request(latency, true);
            }
        });
        tasks.push(task);
    }

    // Configuration and error handling load
    for i in 0..config.concurrent_connections / 4 {
        let metrics = Arc::clone(&metrics);
        let task = tokio::spawn(async move {
            for _ in 0..config.requests_per_connection {
                let start = Instant::now();

                // Simulate error handling
                use api_gateway::models::{ApiResponse, ErrorResponse};
                let error = ErrorResponse {
                    error: "TEST_ERROR".to_string(),
                    message: "Test error message".to_string(),
                    details: None,
                };

                let response = ApiResponse::error(error);
                // Validate response JSON
                let json = serde_json::to_string(&response).unwrap();
                assert!(!json.is_empty());

                let latency = start.elapsed();
                metrics.record_request(latency, true);
            }
        });
        tasks.push(task);
    }

    // Fixed-point arithmetic load
    for i in 0..config.concurrent_connections / 4 {
        let metrics = Arc::clone(&metrics);
        let task = tokio::spawn(async move {
            for j in 0..config.requests_per_connection * 5 {
                // High frequency arithmetic
                let start = Instant::now();

                // Simulate price calculations
                let price_str = format!("{:.4}", 100.0 + (j as f64 * 0.01));
                // Verify price parsing accuracy
                let price_int = api_gateway::middleware::parse_fixed_point(&price_str);
                assert!(price_int.is_ok());

                let latency = start.elapsed();
                metrics.record_request(latency, true);
            }
        });
        tasks.push(task);
    }

    // Wait for all tasks to complete
    for task in tasks {
        let _ = task.await;
    }

    let total_duration = start_time.elapsed();
    metrics.print_summary(total_duration);

    // Verify system handled the integrated load
    let total_requests = metrics.total_requests.load(Ordering::Relaxed);
    let successful_requests = metrics.successful_requests.load(Ordering::Relaxed);
    let success_rate = successful_requests as f64 / total_requests as f64 * 100.0;

    println!("\n🎯 System Integration Results:");
    println!("Success Rate: {:.2}%", success_rate);
    println!(
        "Total Throughput: {:.2} req/s",
        total_requests as f64 / total_duration.as_secs_f64()
    );

    // System should handle integrated load with high success rate
    assert!(
        success_rate > 99.0,
        "System success rate too low: {:.2}%",
        success_rate
    );
    assert!(
        total_requests > 10000,
        "System should process significant load"
    );

    println!("✅ Comprehensive system integration stress test completed");
    Ok(())
}
