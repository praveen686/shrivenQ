//! Example of inter-service communication using gRPC clients

use anyhow::Result;
use services_common::{AuthClient, MarketDataClient, RiskClient, ExecutionClient, ServiceEndpoints};
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();
    
    info!("Starting inter-service communication example");
    
    // Load service endpoints (in production, this would come from config or service discovery)
    let endpoints = ServiceEndpoints::default();
    
    // Example 1: Authenticate with auth service
    info!("Connecting to auth service...");
    let mut auth_client = AuthClient::new(&endpoints.auth_service).await?;
    
    let login_response = auth_client.login("demo_user", "demo_password").await?;
    info!("Login successful, token: {}", login_response.token);
    
    // Validate the token
    let validation = auth_client.validate_token(&login_response.token).await?;
    info!("Token valid: {}, user_id: {}", validation.valid, validation.user_id);
    
    // Example 2: Subscribe to market data
    info!("Connecting to market data service...");
    let mut market_client = MarketDataClient::new(&endpoints.market_data_service).await?;
    
    let subscribe_response = market_client.subscribe(
        vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()],
        vec!["trades".to_string(), "orderbook".to_string()],
    ).await?;
    info!("Subscribed with ID: {}", subscribe_response.subscription_id);
    
    // Get a market snapshot
    let snapshot = market_client.get_snapshot("BTCUSDT").await?;
    info!("Got snapshot for BTCUSDT: bid={}, ask={}", 
        snapshot.best_bid_price, snapshot.best_ask_price);
    
    // Example 3: Check risk before placing order
    info!("Connecting to risk service...");
    let mut risk_client = RiskClient::new(&endpoints.risk_service).await?;
    
    let risk_check = shrivenquant_proto::risk::v1::CheckRiskRequest {
        symbol: "BTCUSDT".to_string(),
        side: "BUY".to_string(),
        quantity: 100_0000, // 100 units
        price: 50000_0000,  // $50,000
    };
    
    let risk_response = risk_client.check_risk(risk_check).await?;
    info!("Risk check passed: {}, reason: {}", 
        risk_response.approved, risk_response.reason);
    
    // Example 4: Submit order through execution service
    if risk_response.approved {
        info!("Connecting to execution service...");
        let mut exec_client = ExecutionClient::new(&endpoints.execution_service).await?;
        
        let order_request = shrivenquant_proto::execution::v1::SubmitOrderRequest {
            client_order_id: "demo_order_001".to_string(),
            symbol: "BTCUSDT".to_string(),
            side: "BUY".to_string(),
            quantity: 100_0000,
            order_type: "LIMIT".to_string(),
            limit_price: Some(50000_0000),
            stop_price: None,
            time_in_force: "GTC".to_string(),
        };
        
        let order_response = exec_client.submit_order(order_request).await?;
        info!("Order submitted, ID: {}", order_response.order_id);
        
        // Check order status
        let order_status = exec_client.get_order(order_response.order_id.clone()).await?;
        info!("Order status: {}", order_status.status);
    }
    
    // Cleanup: Unsubscribe from market data
    market_client.unsubscribe(&subscribe_response.subscription_id).await?;
    info!("Unsubscribed from market data");
    
    info!("Inter-service communication example completed successfully");
    Ok(())
}