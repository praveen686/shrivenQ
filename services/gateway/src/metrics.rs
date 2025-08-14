//! Prometheus metrics for API Gateway
//!
//! Comprehensive metrics collection covering:
//! - HTTP request metrics (latency, status codes, throughput)
//! - WebSocket connection metrics
//! - gRPC client metrics
//! - Rate limiting metrics
//! - Authentication metrics
//! - Business metrics (orders, trades, risk alerts)

use chrono::Utc;
use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};
use parking_lot::RwLock;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Gateway metrics collector
#[derive(Debug)]
pub struct GatewayMetrics {
    start_time: AtomicU64,
    active_connections: Arc<RwLock<u64>>,
    active_websocket_connections: Arc<RwLock<u64>>,
}

impl GatewayMetrics {
    /// Create new metrics instance
    pub fn new() -> Self {
        // Register all metrics with descriptions
        Self::register_metrics();

        Self {
            // SAFETY: Unix timestamp fits in u64 for reasonable dates
            start_time: AtomicU64::new(Utc::now().timestamp() as u64),
            active_connections: Arc::new(RwLock::new(0)),
            active_websocket_connections: Arc::new(RwLock::new(0)),
        }
    }

    /// Register all metrics with Prometheus
    fn register_metrics() {
        // HTTP Request Metrics
        describe_counter!(
            "api_gateway_http_requests_total",
            "Total number of HTTP requests"
        );
        describe_histogram!(
            "api_gateway_http_request_duration_seconds",
            "HTTP request duration in seconds"
        );
        describe_counter!(
            "api_gateway_http_responses_total",
            "Total number of HTTP responses by status code"
        );

        // WebSocket Metrics
        describe_gauge!(
            "api_gateway_websocket_connections_active",
            "Number of active WebSocket connections"
        );
        describe_counter!(
            "api_gateway_websocket_messages_sent_total",
            "Total WebSocket messages sent"
        );
        describe_counter!(
            "api_gateway_websocket_messages_received_total",
            "Total WebSocket messages received"
        );

        // gRPC Client Metrics
        describe_counter!(
            "api_gateway_grpc_requests_total",
            "Total gRPC requests to backend services"
        );
        describe_histogram!(
            "api_gateway_grpc_request_duration_seconds",
            "gRPC request duration in seconds"
        );
        describe_counter!("api_gateway_grpc_errors_total", "Total gRPC errors");

        // Authentication Metrics
        describe_counter!(
            "api_gateway_auth_attempts_total",
            "Total authentication attempts"
        );
        describe_counter!(
            "api_gateway_auth_failures_total",
            "Total authentication failures"
        );
        describe_counter!(
            "api_gateway_jwt_tokens_issued_total",
            "Total JWT tokens issued"
        );
        describe_counter!(
            "api_gateway_jwt_tokens_expired_total",
            "Total JWT tokens expired"
        );

        // Rate Limiting Metrics
        describe_counter!(
            "api_gateway_rate_limit_exceeded_total",
            "Total rate limit violations"
        );
        describe_histogram!(
            "api_gateway_rate_limit_wait_seconds",
            "Time waiting due to rate limits"
        );

        // Business Metrics
        describe_counter!(
            "api_gateway_orders_submitted_total",
            "Total orders submitted"
        );
        describe_counter!(
            "api_gateway_orders_cancelled_total",
            "Total orders cancelled"
        );
        describe_counter!("api_gateway_orders_modified_total", "Total orders modified");
        describe_counter!(
            "api_gateway_market_data_subscriptions_total",
            "Total market data subscriptions"
        );
        describe_counter!(
            "api_gateway_risk_alerts_total",
            "Total risk alerts processed"
        );

        // System Metrics
        describe_gauge!("api_gateway_uptime_seconds", "Gateway uptime in seconds");
        describe_gauge!("api_gateway_memory_usage_bytes", "Memory usage in bytes");
        describe_gauge!("api_gateway_cpu_usage_percent", "CPU usage percentage");
        describe_gauge!(
            "api_gateway_active_connections",
            "Number of active HTTP connections"
        );
    }

    /// Record HTTP request
    pub fn record_http_request(&self, method: &str, path: &str, status: u16, duration: f64) {
        counter!("api_gateway_http_requests_total",
            "method" => method.to_string(),
            "path" => path.to_string()
        )
        .increment(1);

        histogram!("api_gateway_http_request_duration_seconds",
            "method" => method.to_string(),
            "path" => path.to_string()
        )
        .record(duration);

        counter!("api_gateway_http_responses_total",
            "status_code" => status.to_string()
        )
        .increment(1);
    }

    /// Record WebSocket connection
    pub fn record_websocket_connection(&self) {
        let mut active = self.active_websocket_connections.write();
        *active += 1;
        // SAFETY: Connection count safely converts to f64 for metrics
        #[allow(clippy::cast_precision_loss)]
        gauge!("api_gateway_websocket_connections_active").set(*active as f64);
    }

    /// Record WebSocket disconnection
    pub fn record_websocket_disconnection(&self) {
        let mut active = self.active_websocket_connections.write();
        *active = active.saturating_sub(1);
        // SAFETY: Connection count safely converts to f64 for metrics
        #[allow(clippy::cast_precision_loss)]
        gauge!("api_gateway_websocket_connections_active").set(*active as f64);
    }

    /// Record WebSocket message sent
    pub fn record_websocket_message_sent(&self, message_type: &str) {
        counter!("api_gateway_websocket_messages_sent_total",
            "type" => message_type.to_string()
        )
        .increment(1);
    }

    /// Record WebSocket message received
    pub fn record_websocket_message_received(&self, message_type: &str) {
        counter!("api_gateway_websocket_messages_received_total",
            "type" => message_type.to_string()
        )
        .increment(1);
    }

    /// Record gRPC request
    pub fn record_grpc_request(&self, service: &str, method: &str, duration: f64, success: bool) {
        counter!("api_gateway_grpc_requests_total",
            "service" => service.to_string(),
            "method" => method.to_string()
        )
        .increment(1);

        histogram!("api_gateway_grpc_request_duration_seconds",
            "service" => service.to_string(),
            "method" => method.to_string()
        )
        .record(duration);

        if !success {
            counter!("api_gateway_grpc_errors_total",
                "service" => service.to_string(),
                "method" => method.to_string()
            )
            .increment(1);
        }
    }

    /// Record authentication attempt
    pub fn record_auth_attempt(&self, success: bool, method: &str) {
        counter!("api_gateway_auth_attempts_total",
            "method" => method.to_string()
        )
        .increment(1);

        if !success {
            counter!("api_gateway_auth_failures_total",
                "method" => method.to_string()
            )
            .increment(1);
        }
    }

    /// Record JWT token issued
    pub fn record_jwt_token_issued(&self) {
        counter!("api_gateway_jwt_tokens_issued_total").increment(1);
    }

    /// Record JWT token expired
    pub fn record_jwt_token_expired(&self) {
        counter!("api_gateway_jwt_tokens_expired_total").increment(1);
    }

    /// Record rate limit exceeded
    pub fn record_rate_limit_exceeded(&self, endpoint: &str, client_ip: &str) {
        counter!("api_gateway_rate_limit_exceeded_total",
            "endpoint" => endpoint.to_string(),
            "client_ip" => client_ip.to_string()
        )
        .increment(1);
    }

    /// Record rate limit wait time
    pub fn record_rate_limit_wait(&self, wait_time: f64) {
        histogram!("api_gateway_rate_limit_wait_seconds").record(wait_time);
    }

    /// Record order submitted
    pub fn record_order_submitted(&self, symbol: &str, venue: &str) {
        counter!("api_gateway_orders_submitted_total",
            "symbol" => symbol.to_string(),
            "venue" => venue.to_string()
        )
        .increment(1);
    }

    /// Record order cancelled
    pub fn record_order_cancelled(&self, symbol: &str, venue: &str) {
        counter!("api_gateway_orders_cancelled_total",
            "symbol" => symbol.to_string(),
            "venue" => venue.to_string()
        )
        .increment(1);
    }

    /// Record order modified
    pub fn record_order_modified(&self, symbol: &str, venue: &str) {
        counter!("api_gateway_orders_modified_total",
            "symbol" => symbol.to_string(),
            "venue" => venue.to_string()
        )
        .increment(1);
    }

    /// Record market data subscription
    pub fn record_market_data_subscription(&self, symbols: &[String], exchange: &str) {
        counter!("api_gateway_market_data_subscriptions_total",
            "exchange" => exchange.to_string(),
            "symbol_count" => symbols.len().to_string()
        )
        .increment(1);
    }

    /// Record risk alert
    pub fn record_risk_alert(&self, level: &str, source: &str) {
        counter!("api_gateway_risk_alerts_total",
            "level" => level.to_string(),
            "source" => source.to_string()
        )
        .increment(1);
    }

    /// Update system metrics
    pub fn update_system_metrics(&self) {
        // Update uptime
        // SAFETY: Unix timestamp fits in u64
        let uptime = Utc::now().timestamp() as u64 - self.start_time.load(Ordering::Relaxed);
        // SAFETY: Uptime seconds safely converts to f64 for metrics
        #[allow(clippy::cast_precision_loss)]
        gauge!("api_gateway_uptime_seconds").set(uptime as f64);

        // Update memory usage (simplified)
        #[cfg(target_os = "linux")]
        if let Ok(memory_info) = procfs::process::Process::myself().and_then(|p| p.stat()) {
            let memory_kb = memory_info.rss * 4; // RSS is in pages, typically 4KB per page
            // SAFETY: Memory size in bytes safely converts to f64 for metrics
            #[allow(clippy::cast_precision_loss)]
            gauge!("api_gateway_memory_usage_bytes").set((memory_kb * 1024) as f64);
        }

        // Update connection count
        let active_connections = self.active_connections.read();
        // SAFETY: Connection count safely converts to f64 for metrics
        #[allow(clippy::cast_precision_loss)]
        gauge!("api_gateway_active_connections").set(*active_connections as f64);
    }

    /// Increment active connections
    pub fn increment_active_connections(&self) {
        let mut active = self.active_connections.write();
        *active += 1;
    }

    /// Decrement active connections
    pub fn decrement_active_connections(&self) {
        let mut active = self.active_connections.write();
        *active = active.saturating_sub(1);
    }
}

impl Default for GatewayMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Global metrics instance
static METRICS: std::sync::OnceLock<GatewayMetrics> = std::sync::OnceLock::new();

/// Get global metrics instance
pub fn get_metrics() -> &'static GatewayMetrics {
    METRICS.get_or_init(GatewayMetrics::new)
}

/// Initialize metrics system
pub fn init_metrics() -> &'static GatewayMetrics {
    get_metrics()
}

/// Start metrics updater task
pub fn start_metrics_updater() {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));

        loop {
            interval.tick().await;
            get_metrics().update_system_metrics();
        }
    });
}
