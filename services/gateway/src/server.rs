//! API Gateway server implementation

use anyhow::Result;
use axum::{
    Router,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    middleware,
    response::Json,
    routing::{delete, get, post},
};
use std::{net::SocketAddr, sync::Arc, time::Instant};
use tower_http::{compression::CompressionLayer, timeout::TimeoutLayer, trace::TraceLayer};
use tracing::{error, info};

use crate::{
    config::GatewayConfig,
    grpc_clients::GrpcClients,
    handlers::{AuthHandlers, ExecutionHandlers, HealthHandlers, execution::OrderQuery},
    middleware::{
        AuthState, RateLimitState, auth_middleware, create_cors_layer, logging_middleware,
        rate_limit_middleware,
    },
    rate_limiter::RateLimiter,
};

/// Unified application state containing all handlers
#[derive(Clone)]
pub struct AppState {
    pub auth_handlers: AuthHandlers,
    pub execution_handlers: ExecutionHandlers,
    pub health_handlers: HealthHandlers,
    pub grpc_clients: Arc<GrpcClients>,
}

/// API Gateway server
pub struct ApiGatewayServer {
    config: GatewayConfig,
    grpc_clients: Arc<GrpcClients>,
    start_time: Instant,
}

impl ApiGatewayServer {
    /// Create a new API Gateway server
    pub async fn new(config: GatewayConfig) -> Result<Self> {
        info!("Initializing API Gateway server");

        // Connect to gRPC services
        let grpc_clients = match GrpcClients::new(
            &config.services.auth_service,
            &config.services.execution_service,
            &config.services.market_data_service,
            &config.services.risk_service,
        )
        .await
        {
            Ok(clients) => {
                info!("Successfully connected to all gRPC services");
                Arc::new(clients)
            }
            Err(e) => {
                error!("Failed to connect to gRPC services: {}", e);
                error!("Please verify that all microservices are running and accessible");
                return Err(e);
            }
        };

        info!("API Gateway server initialized successfully");

        Ok(Self {
            config,
            grpc_clients,
            start_time: Instant::now(),
        })
    }

    /// Start the server
    pub async fn start(self) -> Result<()> {
        let addr: SocketAddr = match self.config.server_address().parse() {
            Ok(addr) => {
                info!("Parsed server address: {}", addr);
                addr
            }
            Err(e) => {
                error!(
                    "Invalid server address '{}': {}",
                    self.config.server_address(),
                    e
                );
                return Err(anyhow::anyhow!("Invalid server address: {}", e));
            }
        };

        let app = match self.create_app().await {
            Ok(app) => {
                info!("Application routes and middleware configured successfully");
                app
            }
            Err(e) => {
                error!("Failed to create application: {}", e);
                return Err(e);
            }
        };

        info!("Starting API Gateway server on {}", addr);

        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                info!("TCP listener bound successfully to {}", addr);
                listener
            }
            Err(e) => {
                error!("Failed to bind TCP listener to {}: {}", addr, e);
                error!(
                    "Please check if the port is already in use or if you have sufficient permissions"
                );
                return Err(anyhow::anyhow!("Failed to bind to address {}: {}", addr, e));
            }
        };

        if let Err(e) = axum::serve(listener, app).await {
            error!("Server encountered a fatal error: {}", e);
            error!("Server shutting down due to unrecoverable error");
            return Err(anyhow::anyhow!("Server error: {}", e));
        }

        Ok(())
    }

    /// Create the Axum application with all routes and middleware
    async fn create_app(self) -> Result<Router> {
        // Create handlers
        let auth_handlers = AuthHandlers::new(Arc::clone(&self.grpc_clients));
        let execution_handlers = ExecutionHandlers::new(Arc::clone(&self.grpc_clients));
        let health_handlers = HealthHandlers::new(Arc::clone(&self.grpc_clients), self.start_time);

        // Create unified app state
        let app_state = AppState {
            auth_handlers,
            execution_handlers,
            health_handlers,
            grpc_clients: self.grpc_clients,
        };

        // Create middleware states
        let auth_state = AuthState {
            config: Arc::new(self.config.clone()),
        };

        let rate_limiter = Arc::new(RateLimiter::new(self.config.rate_limiting.clone()));
        let rate_limit_state = RateLimitState {
            limiter: rate_limiter,
        };

        // Build the router with all routes
        let app = Router::new()
            // Health and monitoring endpoints (no auth required)
            .route(&self.config.monitoring.health_path, get(health_check))
            .route(&self.config.monitoring.metrics_path, get(metrics))
            // Authentication routes (no auth required)
            .route("/api/v1/auth/login", post(login))
            .route("/api/v1/auth/refresh", post(refresh_token))
            // Execution routes (auth required)
            .route("/api/v1/orders", post(submit_order))
            .route("/api/v1/orders", delete(cancel_order))
            .route("/api/v1/orders/:order_id", get(get_order_status))
            .route("/api/v1/execution/metrics", get(get_execution_metrics))
            // Set unified app state
            .with_state(app_state)
            // Add middleware layers separately to avoid trait bound issues
            .layer(DefaultBodyLimit::max(self.config.server.max_body_size))
            .layer(TimeoutLayer::new(std::time::Duration::from_secs(
                self.config.server.timeout_seconds,
            )))
            .layer(middleware::from_fn_with_state(auth_state, auth_middleware))
            .layer(middleware::from_fn_with_state(
                rate_limit_state,
                rate_limit_middleware,
            ))
            .layer(middleware::from_fn(logging_middleware))
            .layer(TraceLayer::new_for_http())
            .layer(CompressionLayer::new())
            .layer(create_cors_layer(&self.config));

        info!("API Gateway routes configured successfully");
        Ok(app)
    }
}

// Handler wrapper functions to work with unified state
async fn health_check(
    State(state): State<AppState>,
) -> Result<Json<crate::models::ApiResponse<crate::models::HealthCheckResponse>>, StatusCode> {
    match HealthHandlers::health_check(State(state.health_handlers)).await {
        Ok(response) => Ok(response),
        Err(status) => {
            error!("Health check failed with status: {}", status);
            Err(status)
        }
    }
}

async fn metrics(State(_state): State<AppState>) -> Result<String, StatusCode> {
    HealthHandlers::metrics().await
}

async fn login(
    State(state): State<AppState>,
    Json(request): Json<crate::models::LoginRequest>,
) -> Result<Json<crate::models::ApiResponse<crate::models::LoginResponse>>, StatusCode> {
    let username = request.username.clone();
    match AuthHandlers::login(State(state.auth_handlers), Json(request)).await {
        Ok(response) => Ok(response),
        Err(status) => {
            error!(
                "Login failed for user '{}' with status: {}",
                username, status
            );
            Err(status)
        }
    }
}

async fn refresh_token(
    State(state): State<AppState>,
    Json(request): Json<crate::models::RefreshTokenRequest>,
) -> Result<Json<crate::models::ApiResponse<crate::models::LoginResponse>>, StatusCode> {
    AuthHandlers::refresh_token(State(state.auth_handlers), Json(request)).await
}

async fn submit_order(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(request): Json<crate::models::SubmitOrderRequest>,
) -> Result<Json<crate::models::ApiResponse<crate::models::SubmitOrderResponse>>, StatusCode> {
    let symbol = request.symbol.clone();
    let side = request.side.clone();
    let quantity = request.quantity.clone();
    match ExecutionHandlers::submit_order(State(state.execution_handlers), headers, Json(request))
        .await
    {
        Ok(response) => Ok(response),
        Err(status) => {
            error!(
                "Order submission failed for symbol '{}' side '{}' qty '{}' with status: {}",
                symbol, side, quantity, status
            );
            Err(status)
        }
    }
}

async fn cancel_order(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(request): Json<crate::models::CancelOrderRequest>,
) -> Result<Json<crate::models::ApiResponse<bool>>, StatusCode> {
    ExecutionHandlers::cancel_order(State(state.execution_handlers), headers, Json(request)).await
}

async fn get_order_status(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(order_id): axum::extract::Path<i64>,
    axum::extract::Query(query): axum::extract::Query<OrderQuery>,
) -> Result<Json<crate::models::ApiResponse<crate::models::OrderStatusResponse>>, StatusCode> {
    ExecutionHandlers::get_order_status(
        State(state.execution_handlers),
        headers,
        axum::extract::Path(order_id),
        axum::extract::Query(query),
    )
    .await
}

async fn get_execution_metrics(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<crate::models::ApiResponse<serde_json::Value>>, StatusCode> {
    ExecutionHandlers::get_metrics(State(state.execution_handlers), headers).await
}

/// API route documentation
pub fn print_routes() {
    println!("API Gateway Routes:");
    println!("===================");
    println!();
    println!("Health & Monitoring:");
    println!("  GET  /health                 - Health check");
    println!("  GET  /metrics                - Prometheus metrics");
    println!();
    println!("Authentication:");
    println!("  POST /api/v1/auth/login      - User login");
    println!("  POST /api/v1/auth/refresh    - Refresh token");
    println!();
    println!("Order Management:");
    println!("  POST   /api/v1/orders        - Submit order");
    println!("  DELETE /api/v1/orders        - Cancel order");
    println!("  GET    /api/v1/orders/:id    - Get order status");
    println!();
    println!("Execution:");
    println!("  GET  /api/v1/execution/metrics - Execution metrics");
    println!();
    println!("All endpoints support:");
    println!("- JSON request/response bodies");
    println!("- JWT authentication (except auth endpoints)");
    println!("- Rate limiting");
    println!("- CORS");
    println!("- Compression");
    println!("- Request tracing");
}
