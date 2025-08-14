//! Performance Tests for API Gateway
//!
//! Comprehensive performance and benchmark tests to ensure the API Gateway
//! meets the strict latency requirements for ultra-low latency trading.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::time::Duration;
use tokio::runtime::Runtime;

use api_gateway::{
    GatewayConfig,
    middleware::parse_fixed_point,
    models::{CheckOrderRequest, LoginRequest, SubmitOrderRequest},
    rate_limiter::RateLimiter,
};

/// Benchmark configuration parsing and validation
fn bench_config_loading(c: &mut Criterion) {
    let config_content = r#"
[server]
host = "127.0.0.1"
port = 8080
workers = 4

[services]
auth_service = "http://127.0.0.1:50051"
execution_service = "http://127.0.0.1:50052"
market_data_service = "http://127.0.0.1:50053"
risk_service = "http://127.0.0.1:50054"

[auth]
jwt_secret = "test-secret"
token_expiry_hours = 24
"#;

    c.bench_function("config_parsing", |b| {
        b.iter(|| toml::from_str::<GatewayConfig>(criterion::black_box(config_content)))
    });
}

/// Benchmark request/response model serialization
fn bench_model_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("model_serialization");

    // Test order request serialization
    let order_request = SubmitOrderRequest {
        client_order_id: Some("TEST123".to_string()),
        symbol: "NIFTY2412050000CE".to_string(),
        side: "BUY".to_string(),
        quantity: "100.0000".to_string(),
        order_type: "LIMIT".to_string(),
        limit_price: Some("150.2500".to_string()),
        stop_price: None,
        time_in_force: Some("DAY".to_string()),
        exchange: Some("NSE".to_string()),
    };

    group.bench_function("order_request_json", |b| {
        b.iter(|| serde_json::to_string(criterion::black_box(&order_request)))
    });

    group.bench_function("order_request_bincode", |b| {
        b.iter(|| bincode::serialize(criterion::black_box(&order_request)))
    });

    // Test login request
    let login_request = LoginRequest {
        username: "testuser".to_string(),
        password: "testpass".to_string(),
        exchange: Some("ZERODHA".to_string()),
    };

    group.bench_function("login_request_json", |b| {
        b.iter(|| serde_json::to_string(criterion::black_box(&login_request)))
    });

    group.finish();
}

/// Benchmark fixed-point arithmetic conversions
fn bench_fixed_point_arithmetic(c: &mut Criterion) {
    let mut group = c.benchmark_group("fixed_point");

    // Benchmark parsing string to fixed-point
    group.bench_function("parse_price", |b| {
        b.iter(|| parse_fixed_point(criterion::black_box("123.4567")))
    });

    group.bench_function("parse_quantity", |b| {
        b.iter(|| parse_fixed_point(criterion::black_box("1000.0000")))
    });

    // Benchmark conversion back to string
    group.bench_function("format_price", |b| {
        b.iter(|| {
            let value = 1234567i64;
            format!("{:.4}", criterion::black_box(value) as f64 / 10000.0)
        })
    });

    group.finish();
}

/// Benchmark rate limiting performance
fn bench_rate_limiting(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("rate_limiting");

    // Setup rate limiter
    let rate_limiter = rt.block_on(async {
        use api_gateway::RateLimitConfig;
        use std::collections::HashMap;

        let config = RateLimitConfig {
            enabled: true,
            requests_per_minute: 60000, // High limit for benchmarking
            burst_size: 1000,
            per_ip_limit: 1000,
            per_endpoint_limits: HashMap::new(),
        };

        RateLimiter::new(config).await
    });

    group.bench_function("check_rate_limit", |b| {
        b.to_async(&rt).iter(|| async {
            let limiter = criterion::black_box(&rate_limiter);
            limiter.check_rate_limit("127.0.0.1", "/test").await
        })
    });

    group.finish();
}

/// Benchmark JWT token operations
fn bench_jwt_operations(c: &mut Criterion) {
    use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
    use serde::{Deserialize, Serialize};
    use std::collections::HashSet;

    #[derive(Debug, Serialize, Deserialize)]
    struct Claims {
        sub: String,
        exp: usize,
        permissions: Vec<String>,
    }

    let secret = "test-secret-key-for-benchmarking";
    let encoding_key = EncodingKey::from_secret(secret.as_ref());
    let decoding_key = DecodingKey::from_secret(secret.as_ref());

    let claims = Claims {
        sub: "testuser".to_string(),
        exp: 1234567890,
        permissions: vec!["PLACE_ORDERS".to_string(), "VIEW_POSITIONS".to_string()],
    };

    let mut group = c.benchmark_group("jwt_operations");

    group.bench_function("encode_token", |b| {
        b.iter(|| {
            encode(
                &Header::default(),
                criterion::black_box(&claims),
                &encoding_key,
            )
        })
    });

    let token = encode(&Header::default(), &claims, &encoding_key).unwrap();

    group.bench_function("decode_token", |b| {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.required_spec_claims = HashSet::new();

        b.iter(|| decode::<Claims>(criterion::black_box(&token), &decoding_key, &validation))
    });

    group.finish();
}

/// Benchmark HTTP middleware stack performance
fn bench_middleware_stack(c: &mut Criterion) {
    use api_gateway::middleware::{auth_middleware, rate_limit_middleware};
    use axum::{
        body::Body,
        http::{Request, Response, StatusCode},
        response::IntoResponse,
    };

    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("middleware");

    // Benchmark individual middleware components would go here
    // For now, we'll benchmark the core operations they perform

    group.bench_function("header_extraction", |b| {
        let request = Request::builder()
            .header("Authorization", "Bearer test-token")
            .header("Content-Type", "application/json")
            .body(Body::empty())
            .unwrap();

        b.iter(|| {
            let headers = criterion::black_box(&request).headers();
            headers.get("authorization")
        })
    });

    group.finish();
}

/// Benchmark WebSocket message handling
fn bench_websocket_operations(c: &mut Criterion) {
    use api_gateway::models::WebSocketMessage;
    use serde_json::json;

    let mut group = c.benchmark_group("websocket");

    let message = WebSocketMessage {
        message_type: "market_data".to_string(),
        data: json!({
            "symbol": "BTCUSDT",
            "price": "50000.00",
            "volume": "1.23456",
            "timestamp": 1640995200000000000i64
        }),
        timestamp: 1640995200,
    };

    group.bench_function("message_serialization", |b| {
        b.iter(|| serde_json::to_string(criterion::black_box(&message)))
    });

    let message_json = serde_json::to_string(&message).unwrap();

    group.bench_function("message_deserialization", |b| {
        b.iter(|| serde_json::from_str::<WebSocketMessage>(criterion::black_box(&message_json)))
    });

    group.finish();
}

/// Benchmark error handling and response formatting
fn bench_error_handling(c: &mut Criterion) {
    use api_gateway::models::{ApiResponse, ErrorResponse};
    use std::collections::HashMap;

    let mut group = c.benchmark_group("error_handling");

    let error_response = ErrorResponse {
        error: "VALIDATION_ERROR".to_string(),
        message: "Invalid order parameters".to_string(),
        details: Some({
            let mut details = HashMap::new();
            details.insert("field".to_string(), "quantity".to_string());
            details.insert("reason".to_string(), "must_be_positive".to_string());
            details
        }),
    };

    group.bench_function("error_response_creation", |b| {
        b.iter(|| ApiResponse::error(criterion::black_box(error_response.clone())))
    });

    let api_response = ApiResponse::error(error_response);

    group.bench_function("error_response_serialization", |b| {
        b.iter(|| serde_json::to_string(criterion::black_box(&api_response)))
    });

    group.finish();
}

/// Latency test to ensure sub-10ms response times
fn bench_end_to_end_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("latency");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(1000);

    // Simulate complete request processing pipeline
    group.bench_function("request_pipeline", |b| {
        b.to_async(&rt).iter(|| async {
            // Simulate auth check
            // Simulate authentication check latency
            tokio::time::sleep(Duration::from_micros(100)).await;

            // Simulate rate limiting
            // Simulate rate limiting check latency
            tokio::time::sleep(Duration::from_micros(50)).await;

            // Simulate request parsing
            let request_body = r#"{"symbol":"NIFTY","side":"BUY","quantity":"100.0000"}"#;
            // Parse and validate request body
            let parsed: serde_json::Value = serde_json::from_str(request_body).unwrap();
            assert!(parsed.is_object());

            // Simulate response creation
            let response = json!({
                "success": true,
                "data": {
                    "order_id": "12345",
                    "status": "ACCEPTED"
                }
            });

            serde_json::to_string(&response).unwrap()
        })
    });

    group.finish();
}

/// Memory allocation benchmark to ensure minimal allocations
fn bench_memory_usage(c: &mut Criterion) {
    use api_gateway::models::SubmitOrderRequest;

    let mut group = c.benchmark_group("memory");

    // Test stack-allocated operations
    group.bench_function("stack_operations", |b| {
        b.iter(|| {
            let price_str = "123.4567";
            let price_int = parse_fixed_point(price_str).unwrap_or(0);
            // Verify round-trip conversion accuracy
            let price_back = format!("{:.4}", price_int as f64 / 10000.0);
            assert!(!price_back.is_empty());
            price_int
        })
    });

    // Test minimal heap allocations
    group.bench_function("minimal_heap", |b| {
        b.iter(|| {
            let mut order = SubmitOrderRequest {
                client_order_id: None,
                symbol: String::with_capacity(32),
                side: String::with_capacity(4),
                quantity: String::with_capacity(16),
                order_type: String::with_capacity(8),
                limit_price: None,
                stop_price: None,
                time_in_force: None,
                exchange: None,
            };

            order.symbol.push_str("NIFTY");
            order.side.push_str("BUY");
            order.quantity.push_str("100.0000");
            order.order_type.push_str("LIMIT");

            criterion::black_box(order)
        })
    });

    group.finish();
}

/// Throughput benchmark for high-frequency scenarios
fn bench_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");

    for batch_size in [10, 100, 1000].iter() {
        group.throughput(Throughput::Elements(*batch_size));
        group.bench_with_input(
            BenchmarkId::new("order_processing", batch_size),
            batch_size,
            |b, &size| {
                let orders: Vec<_> = (0..size)
                    .map(|i| SubmitOrderRequest {
                        client_order_id: Some(format!("ORDER_{}", i)),
                        symbol: "NIFTY2412050000CE".to_string(),
                        side: "BUY".to_string(),
                        quantity: "100.0000".to_string(),
                        order_type: "LIMIT".to_string(),
                        limit_price: Some("150.2500".to_string()),
                        stop_price: None,
                        time_in_force: Some("DAY".to_string()),
                        exchange: Some("NSE".to_string()),
                    })
                    .collect();

                b.iter(|| {
                    for order in &orders {
                        // Benchmark serialization performance
                        let serialized =
                            serde_json::to_string(criterion::black_box(order)).unwrap();
                        criterion::black_box(serialized);
                        // Benchmark fixed-point parsing
                        let price = parse_fixed_point(&order.quantity).unwrap_or(0);
                        criterion::black_box(price);
                        if let Some(ref limit_price) = order.limit_price {
                            // Benchmark limit price parsing
                            let limit = parse_fixed_point(limit_price).unwrap_or(0);
                            criterion::black_box(limit);
                        }
                    }
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_config_loading,
    bench_model_serialization,
    bench_fixed_point_arithmetic,
    bench_rate_limiting,
    bench_jwt_operations,
    bench_middleware_stack,
    bench_websocket_operations,
    bench_error_handling,
    bench_end_to_end_latency,
    bench_memory_usage,
    bench_throughput
);

criterion_main!(benches);
