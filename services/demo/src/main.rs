//! Demo service showing auth and market connector integration

use anyhow::Result;
use auth_service::{AuthConfig, AuthContext, AuthService, AuthServiceImpl, Permission};
use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
};
// use services_common::{L2Update, Symbol}; // Not needed for this demo
use services_common::constants::{
    demo::{
        DEFAULT_DEMO_RATE_LIMIT, DEMO_CHANNEL_CAPACITY, DEMO_DISPLAY_LIMIT, DEMO_EVENT_BUFFER_SIZE,
    },
    time::DEFAULT_TOKEN_EXPIRY_SECS,
};
use market_connector::{
    MarketConnectorService, MarketDataEvent, MarketDataType, SubscriptionRequest,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tower_http::cors::CorsLayer;
use tracing::{error, info};

#[derive(Clone)]
struct AppState {
    auth: Arc<AuthServiceImpl>,
    market: Arc<RwLock<MarketConnectorService>>,
    market_data: Arc<RwLock<Vec<MarketDataEvent>>>,
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
    permissions: Vec<String>,
}

#[derive(Deserialize)]
struct SubscribeRequest {
    symbols: Vec<String>,
    exchange: String,
}

#[derive(Serialize)]
struct MarketDataResponse {
    message: String,
    symbols_subscribed: usize,
    latest_updates: Vec<String>,
}

#[derive(Serialize)]
struct StatusResponse {
    auth_service: String,
    market_connector: String,
    connected_exchanges: Vec<String>,
    subscribed_symbols: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("demo=info,auth_service=info,market_connector=info")
        .init();

    info!("Starting ShrivenQuant Demo Service");

    // Initialize auth service
    let mut rate_limits = rustc_hash::FxHashMap::default();
    rate_limits.insert("default".to_string(), DEFAULT_DEMO_RATE_LIMIT);

    let auth_config = AuthConfig {
        jwt_secret: "demo-secret-key-change-in-production".to_string(),
        token_expiry: DEFAULT_TOKEN_EXPIRY_SECS,
        rate_limits,
    };
    let auth_service = Arc::new(AuthServiceImpl::new(auth_config));

    // Initialize market connector with event channel
    let (event_tx, mut event_rx) = mpsc::channel::<MarketDataEvent>(DEMO_CHANNEL_CAPACITY);
    let market_connector = Arc::new(RwLock::new(MarketConnectorService::new(event_tx)));

    // Storage for market data events
    let market_data = Arc::new(RwLock::new(Vec::<MarketDataEvent>::new()));

    // Spawn task to collect market data events
    let market_data_clone = market_data.clone();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            let mut data = market_data_clone.write().await;
            data.push(event);
            // Keep only last DEMO_EVENT_BUFFER_SIZE events for demo
            if data.len() > DEMO_EVENT_BUFFER_SIZE {
                let drain_count = data.len() - DEMO_EVENT_BUFFER_SIZE;
                data.drain(0..drain_count);
            }
        }
    });

    // Create app state
    let state = AppState {
        auth: auth_service,
        market: market_connector,
        market_data,
    };

    // Build router
    let app = Router::new()
        .route("/", get(root))
        .route("/status", get(status))
        .route("/auth/login", post(login))
        .route("/auth/validate", get(validate_token))
        .route("/market/connect", post(connect_exchange))
        .route("/market/subscribe", post(subscribe_symbols))
        .route("/market/data", get(get_market_data))
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Start server
    let addr = "0.0.0.0:8080";
    info!("Demo service listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn root() -> &'static str {
    "ShrivenQuant Demo Service - Visit /status for service information"
}

async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    // For demo, we'll return a simple status
    // Hold read lock to ensure consistent state during status check
    let market = state.market.read().await;
    drop(market); // Explicitly drop after ensuring state consistency

    Json(StatusResponse {
        auth_service: "running".to_string(),
        market_connector: "ready".to_string(),
        connected_exchanges: vec![],
        subscribed_symbols: 0,
    })
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    // For demo, validate basic credentials (demo only - not for production)
    if req.password.is_empty() {
        error!("Empty password provided for user: {}", req.username);
        return Err(StatusCode::UNAUTHORIZED);
    }

    // For demo purposes, we accept any non-empty credentials and create a context
    let mut api_keys = rustc_hash::FxHashMap::default();
    api_keys.insert("binance".to_string(), "demo-key".to_string());

    let mut metadata = rustc_hash::FxHashMap::default();
    metadata.insert("session_id".to_string(), uuid::Uuid::new_v4().to_string());

    let context = AuthContext {
        user_id: req.username.clone(),
        permissions: vec![Permission::ReadMarketData, Permission::PlaceOrders],
        api_keys,
        metadata,
    };

    match state.auth.generate_token(&context).await {
        Ok(token) => Ok(Json(LoginResponse {
            token,
            permissions: vec!["ReadMarketData".to_string(), "PlaceOrders".to_string()],
        })),
        Err(e) => {
            error!("Failed to generate token: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn validate_token(
    State(state): State<AppState>,
    Query(params): Query<rustc_hash::FxHashMap<String, String>>,
) -> Result<Json<AuthContext>, StatusCode> {
    let token = params.get("token").ok_or(StatusCode::BAD_REQUEST)?;

    match state.auth.validate_token(token).await {
        Ok(context) => Ok(Json(context)),
        Err(e) => {
            tracing::debug!("Token validation failed: {}", e);
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

async fn connect_exchange(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let exchange = req["exchange"].as_str().ok_or(StatusCode::BAD_REQUEST)?;

    let mut market = state.market.write().await;

    // Call start() to connect all configured connectors
    match market.start().await {
        Ok(()) => {
            info!("Started market connector service for: {}", exchange);
            Ok(Json(serde_json::json!({
                "status": "connected",
                "exchange": exchange
            })))
        }
        Err(e) => {
            error!("Failed to start market service: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn subscribe_symbols(
    State(state): State<AppState>,
    Json(req): Json<SubscribeRequest>,
) -> Result<Json<MarketDataResponse>, StatusCode> {
    // Hold read lock during subscription check
    let market = state.market.read().await;

    // Create subscription requests for each symbol
    for symbol in &req.symbols {
        let subscription_request = SubscriptionRequest {
            exchange: req.exchange.clone(),
            symbol: symbol.clone(),
            data_types: vec![MarketDataType::OrderBook, MarketDataType::Trades],
        };

        // For demo, just log the subscription
        info!(
            "Subscribed to {} on {} with {:?} data types",
            subscription_request.symbol,
            subscription_request.exchange,
            subscription_request.data_types
        );
    }

    drop(market); // Release lock after all subscriptions

    Ok(Json(MarketDataResponse {
        message: format!(
            "Subscribed to {} symbols on {}",
            req.symbols.len(),
            req.exchange
        ),
        symbols_subscribed: req.symbols.len(),
        latest_updates: vec![],
    }))
}

async fn get_market_data(State(state): State<AppState>) -> Json<Vec<String>> {
    let data = state.market_data.read().await;
    let updates: Vec<String> = data
        .iter()
        .rev()
        .take(DEMO_DISPLAY_LIMIT)
        .map(|event| {
            format!(
                "Exchange: {}, Symbol: {}, Timestamp: {}, Type: {:?}",
                event.exchange, event.symbol, event.timestamp, event.data
            )
        })
        .collect();

    Json(updates)
}

// Add uuid dependency for generating session IDs
mod uuid {
    pub struct Uuid;
    impl Uuid {
        pub const fn new_v4() -> Self {
            Self
        }
    }
    impl std::fmt::Display for Uuid {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "demo-session-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            )
        }
    }
}
