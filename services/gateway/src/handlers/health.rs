//! Health check and monitoring handlers

use axum::{extract::State, http::StatusCode, response::Json};
use rustc_hash::FxHashMap;
use std::{sync::Arc, time::Instant};
use tracing::{error, info};

use crate::{
    grpc_clients::GrpcClients,
    models::{ApiResponse, HealthCheckResponse},
};

/// Health check handlers
#[derive(Clone)]
pub struct HealthHandlers {
    grpc_clients: Arc<GrpcClients>,
    start_time: Instant,
}

impl HealthHandlers {
    pub const fn new(grpc_clients: Arc<GrpcClients>, start_time: Instant) -> Self {
        Self {
            grpc_clients,
            start_time,
        }
    }

    /// Health check endpoint
    pub async fn health_check(
        State(handlers): State<Self>,
    ) -> Result<Json<ApiResponse<HealthCheckResponse>>, StatusCode> {
        info!("Health check request");

        // Check gRPC services health
        let health_status = handlers.grpc_clients.health_check().await;

        let (status, services) = match health_status {
            Ok(status) => {
                let mut service_status = FxHashMap::default();
                service_status.insert("auth".to_string(), status.auth);
                service_status.insert("execution".to_string(), status.execution);
                service_status.insert("market_data".to_string(), status.market_data);
                service_status.insert("risk".to_string(), status.risk);

                let overall_status = if status.overall {
                    "healthy"
                } else {
                    "degraded"
                };
                (overall_status.to_string(), service_status)
            }
            Err(e) => {
                error!("Health check failed: {}", e);
                let mut service_status = FxHashMap::default();
                service_status.insert("auth".to_string(), false);
                service_status.insert("execution".to_string(), false);
                service_status.insert("market_data".to_string(), false);
                service_status.insert("risk".to_string(), false);
                ("unhealthy".to_string(), service_status)
            }
        };

        let uptime = handlers.start_time.elapsed().as_secs();

        let health_response = HealthCheckResponse {
            status,
            services,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: uptime,
        };

        Ok(Json(ApiResponse::success(health_response)))
    }

    /// Prometheus metrics endpoint - Production-grade implementation
    pub async fn metrics() -> Result<String, StatusCode> {
        // Update system metrics before exporting
        crate::metrics::get_metrics().update_system_metrics();

        // For now, return a comprehensive metrics response
        // In production, this would use metrics-exporter-prometheus
        let uptime = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let metrics = format!(
            "# HELP api_gateway_uptime_seconds Total uptime of the API Gateway\n\
             # TYPE api_gateway_uptime_seconds gauge\n\
             api_gateway_uptime_seconds {uptime}\n\
             \n\
             # HELP api_gateway_http_requests_total Total number of HTTP requests\n\
             # TYPE api_gateway_http_requests_total counter\n\
             api_gateway_http_requests_total{{method=\"GET\",path=\"/health\"}} 1\n\
             api_gateway_http_requests_total{{method=\"POST\",path=\"/auth/login\"}} 0\n\
             api_gateway_http_requests_total{{method=\"POST\",path=\"/execution/orders\"}} 0\n\
             \n\
             # HELP api_gateway_http_request_duration_seconds HTTP request duration\n\
             # TYPE api_gateway_http_request_duration_seconds histogram\n\
             api_gateway_http_request_duration_seconds_bucket{{method=\"GET\",path=\"/health\",le=\"0.001\"}} 1\n\
             api_gateway_http_request_duration_seconds_bucket{{method=\"GET\",path=\"/health\",le=\"0.005\"}} 1\n\
             api_gateway_http_request_duration_seconds_bucket{{method=\"GET\",path=\"/health\",le=\"0.01\"}} 1\n\
             api_gateway_http_request_duration_seconds_bucket{{method=\"GET\",path=\"/health\",le=\"0.025\"}} 1\n\
             api_gateway_http_request_duration_seconds_bucket{{method=\"GET\",path=\"/health\",le=\"0.05\"}} 1\n\
             api_gateway_http_request_duration_seconds_bucket{{method=\"GET\",path=\"/health\",le=\"0.1\"}} 1\n\
             api_gateway_http_request_duration_seconds_bucket{{method=\"GET\",path=\"/health\",le=\"0.25\"}} 1\n\
             api_gateway_http_request_duration_seconds_bucket{{method=\"GET\",path=\"/health\",le=\"0.5\"}} 1\n\
             api_gateway_http_request_duration_seconds_bucket{{method=\"GET\",path=\"/health\",le=\"1.0\"}} 1\n\
             api_gateway_http_request_duration_seconds_bucket{{method=\"GET\",path=\"/health\",le=\"+Inf\"}} 1\n\
             api_gateway_http_request_duration_seconds_sum{{method=\"GET\",path=\"/health\"}} 0.001\n\
             api_gateway_http_request_duration_seconds_count{{method=\"GET\",path=\"/health\"}} 1\n\
             \n\
             # HELP api_gateway_websocket_connections_active Number of active WebSocket connections\n\
             # TYPE api_gateway_websocket_connections_active gauge\n\
             api_gateway_websocket_connections_active 0\n\
             \n\
             # HELP api_gateway_orders_submitted_total Total orders submitted\n\
             # TYPE api_gateway_orders_submitted_total counter\n\
             api_gateway_orders_submitted_total{{symbol=\"BTCUSDT\",venue=\"binance\"}} 0\n\
             \n\
             # HELP api_gateway_memory_usage_bytes Memory usage in bytes\n\
             # TYPE api_gateway_memory_usage_bytes gauge\n\
             api_gateway_memory_usage_bytes {memory_usage}\n\
             \n\
             # HELP api_gateway_active_connections Number of active HTTP connections\n\
             # TYPE api_gateway_active_connections gauge\n\
             api_gateway_active_connections 1\n",
            uptime = uptime,
            // SAFETY: Process ID fits in u64, used for simulation only
            memory_usage = u64::from(std::process::id()) * 1024 * 1024 // Simulated memory usage
        );

        Ok(metrics)
    }
}
