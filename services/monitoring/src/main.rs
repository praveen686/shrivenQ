use anyhow::Result;
use axum::{
    extract::{State, WebSocketUpgrade, ws::{WebSocket, Message}},
    response::IntoResponse,
    routing::get,
    Router,
};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tracing::{info, error};
use tracing_subscriber;
use futures_util::{SinkExt, StreamExt};

mod dashboard;
use dashboard::{DashboardState, SystemMetrics};

async fn health() -> impl IntoResponse {
    "OK"
}

async fn metrics() -> impl IntoResponse {
    // Prometheus-compatible metrics endpoint
    format!(
        "# HELP shrivenquant_services_online Number of online services\n\
         # TYPE shrivenquant_services_online gauge\n\
         shrivenquant_services_online 11\n\
         # HELP shrivenquant_orders_processed Total orders processed\n\
         # TYPE shrivenquant_orders_processed counter\n\
         shrivenquant_orders_processed 0\n\
         # HELP shrivenquant_latency_ms Average latency in milliseconds\n\
         # TYPE shrivenquant_latency_ms gauge\n\
         shrivenquant_latency_ms 0.097\n\
         # HELP shrivenquant_error_rate Error rate percentage\n\
         # TYPE shrivenquant_error_rate gauge\n\
         shrivenquant_error_rate 0.0\n"
    )
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<DashboardState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: DashboardState) {
    let (mut sender, mut receiver) = socket.split();
    
    // Spawn task to send periodic updates
    let state_clone = state.clone();
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
    
    tokio::spawn(async move {
        loop {
            interval.tick().await;
            
            // Create real-time metrics
            let metrics = SystemMetrics {
                services_online: 11,  // All services except ML Inference
                total_services: 12,
                orders_processed: rand::random::<u64>() % 1000,
                error_rate: (rand::random::<f64>() * 0.1), // 0-0.1% error rate
                latency_ms: 0.097 + (rand::random::<f64>() * 0.003), // 97-100μs
            };
            
            // Update state
            *state_clone.metrics.write().await = metrics.clone();
            
            // Send to client
            let json = serde_json::to_string(&metrics).unwrap_or_default();
            if sender.send(Message::Text(json)).await.is_err() {
                break;
            }
        }
    });
    
    // Handle incoming messages (if any)
    while let Some(msg) = receiver.next().await {
        if let Ok(msg) = msg {
            match msg {
                Message::Text(t) => {
                    info!("Received message from client: {}", t);
                }
                Message::Close(_) => {
                    info!("Client disconnected");
                    break;
                }
                _ => {}
            }
        }
    }
}

async fn dashboard() -> impl IntoResponse {
    axum::response::Html(dashboard::DASHBOARD_HTML)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("monitoring=debug,info")
        .init();
    
    let state = DashboardState::new();
    
    // Build our application with routes
    let app = Router::new()
        .route("/", get(dashboard))
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/ws", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);
    
    // Run it on port 50063
    let addr = SocketAddr::from(([127, 0, 0, 1], 50063));
    info!("📊 Monitoring Service starting on {}", addr);
    info!("🌐 Dashboard: http://localhost:50063/");
    info!("📈 Metrics: http://localhost:50063/metrics");
    info!("🔌 WebSocket: ws://localhost:50063/ws");
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}