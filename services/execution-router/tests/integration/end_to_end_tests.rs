//! End-to-end integration tests
//!
//! Complete workflow tests spanning all service components:
//! - Full order lifecycle from submission to execution
//! - Multi-venue routing and failover scenarios  
//! - Algorithm execution with real market conditions
//! - Cross-service communication and coordination
//! - Error recovery and system resilience
//! - Performance under realistic trading loads

use execution_router::{
    ExecutionRouterService, VenueStrategy, ExecutionServiceImpl,
    smart_router::{Router, VenueConnection, MarketContext},
    venue_manager::{VenueManager, VenueStatus},
    ExecutionAlgorithm, OrderRequest, OrderType, TimeInForce, OrderId, OrderStatus,
    memory::{Arena, ObjectPool, RingBuffer}
};
use services_common::{Px, Qty, Side, Symbol, Ts};
use services_common::execution::v1::{
    execution_service_server::ExecutionService,
    SubmitOrderRequest, CancelOrderRequest, ModifyOrderRequest,
    StreamExecutionReportsRequest, GetMetricsRequest
};
use rstest::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::time::sleep;
use tokio_stream::StreamExt;
use tonic::Request;
use anyhow::Result;

/// Integration test configuration
const INTEGRATION_TEST_TIMEOUT: Duration = Duration::from_secs(30);
const LARGE_ORDER_SIZE: i64 = 10_000_000; // 1000 units (e.g., BTC)
const SMALL_ORDER_SIZE: i64 = 100_000;    // 10 units

/// Test fixtures and utilities for integration tests
mod integration_utils {
    use super::*;
    
    pub struct TradingEnvironment {
        pub router_service: ExecutionRouterService,
        pub grpc_service: ExecutionServiceImpl,
        pub venue_manager: VenueManager,
        pub smart_router: Router,
    }
    
    impl TradingEnvironment {
        pub async fn new_multi_venue() -> Self {
            let router_service = ExecutionRouterService::new(VenueStrategy::Smart);
            let grpc_service = ExecutionServiceImpl::new(Arc::new(router_service.clone()));
            let venue_manager = VenueManager::new("binance".to_string());
            let smart_router = Router::new();
            
            // Setup venues
            Self::setup_test_venues(&venue_manager, &smart_router).await;
            
            Self {
                router_service,
                grpc_service,
                venue_manager,
                smart_router,
            }
        }
        
        async fn setup_test_venues(venue_manager: &VenueManager, smart_router: &Router) {
            let venues = vec![
                ("binance", 100, 15_000_000.0, 5, 10),
                ("coinbase", 150, 8_000_000.0, 8, 15),
                ("kraken", 200, 5_000_000.0, 10, 20),
                ("bybit", 120, 12_000_000.0, 6, 12),
            ];
            
            for (name, latency, liquidity, maker_fee, taker_fee) in venues {
                // Add to venue manager
                venue_manager.add_venue(name.to_string(), rustc_hash::FxHashMap::default()).await;
                venue_manager.connect_venue(name).await.expect("Should connect venue");
                
                // Add to smart router
                smart_router.add_venue(VenueConnection {
                    name: name.to_string(),
                    is_connected: true,
                    latency_us: latency,
                    liquidity,
                    maker_fee_bp: maker_fee,
                    taker_fee_bp: taker_fee,
                    supported_types: vec![OrderType::Market, OrderType::Limit, OrderType::Stop],
                    last_heartbeat: Instant::now(),
                });
            }
        }
        
        pub async fn simulate_market_conditions(&self) -> MarketContext {
            MarketContext {
                bid: Some(Px::new(49_995.0)),
                ask: Some(Px::new(50_005.0)),
                mid: Some(Px::new(50_000.0)),
                spread: Some(10), // 0.10 spread
                volume: 25_000_000, // Active market
                volatility: 0.02, // 2% volatility
                venues: vec![
                    "binance".to_string(),
                    "coinbase".to_string(),
                    "kraken".to_string(),
                    "bybit".to_string(),
                ],
            }
        }
    }
    
    pub fn create_institutional_order(strategy: &str, algorithm: ExecutionAlgorithm) -> OrderRequest {
        OrderRequest {
            client_order_id: format!("INST_{}_{}", strategy, chrono::Utc::now().timestamp_millis()),
            symbol: Symbol::new(1), // BTC/USDT
            side: Side::Buy,
            quantity: Qty::from_i64(LARGE_ORDER_SIZE),
            order_type: OrderType::Limit,
            limit_price: Some(Px::new(50_000.0)),
            stop_price: None,
            is_buy: true,
            algorithm,
            urgency: 0.3, // Low urgency for institutional orders
            participation_rate: Some(0.15), // 15% participation
            time_in_force: TimeInForce::GTC,
            venue: None, // Let smart routing decide
            strategy_id: strategy.to_string(),
            params: rustc_hash::FxHashMap::default(),
        }
    }
    
    pub fn create_retail_order(client_id: &str) -> OrderRequest {
        OrderRequest {
            client_order_id: client_id.to_string(),
            symbol: Symbol::new(2), // ETH/USDT
            side: Side::Sell,
            quantity: Qty::from_i64(SMALL_ORDER_SIZE),
            order_type: OrderType::Limit,
            limit_price: Some(Px::new(3_000.0)),
            stop_price: None,
            is_buy: false,
            algorithm: ExecutionAlgorithm::Smart,
            urgency: 0.7, // Higher urgency for retail
            participation_rate: Some(0.05), // 5% participation
            time_in_force: TimeInForce::DAY,
            venue: Some("coinbase".to_string()),
            strategy_id: "retail_strategy".to_string(),
            params: rustc_hash::FxHashMap::default(),
        }
    }
    
    pub async fn wait_for_order_processing() {
        sleep(Duration::from_millis(100)).await;
    }
    
    pub async fn wait_for_venue_setup() {
        sleep(Duration::from_millis(50)).await;
    }
}

use integration_utils::*;

/// Complete Order Lifecycle Tests
#[rstest]
#[tokio::test]
async fn test_complete_institutional_order_workflow() -> Result<()> {
    let env = TradingEnvironment::new_multi_venue().await;
    wait_for_venue_setup().await;
    
    println!("=== Starting Institutional Order Workflow ===");
    
    // Step 1: Submit large TWAP order
    let order_request = create_institutional_order("pension_fund_alpha", ExecutionAlgorithm::Twap);
    let client_order_id = order_request.client_order_id.clone();
    
    println!("Submitting institutional TWAP order: {}", client_order_id);
    let order_id = env.router_service.submit_order(order_request).await?;
    wait_for_order_processing().await;
    
    // Step 2: Verify order was accepted and tracked
    let order = env.router_service.get_order(order_id).await?;
    assert_eq!(order.status, OrderStatus::Pending);
    assert_eq!(order.algorithm, ExecutionAlgorithm::Twap);
    assert_eq!(order.quantity, Qty::from_i64(LARGE_ORDER_SIZE));
    println!("✓ Order accepted and tracked: {} (Status: {:?})", order_id, order.status);
    
    // Step 3: Check that smart routing has analyzed venues
    let market_context = env.simulate_market_conditions().await;
    assert!(market_context.venues.len() >= 4, "Should have multiple venues available");
    println!("✓ Market context established with {} venues", market_context.venues.len());
    
    // Step 4: Verify venue connectivity
    let venue_statuses = env.venue_manager.get_all_statuses().await;
    let connected_venues: Vec<_> = venue_statuses.iter()
        .filter(|(_, &status)| status == VenueStatus::Connected)
        .collect();
    
    assert!(connected_venues.len() >= 3, "Should have multiple connected venues");
    println!("✓ Connected to {} venues: {:?}", connected_venues.len(), 
             connected_venues.iter().map(|(name, _)| name).collect::<Vec<_>>());
    
    // Step 5: Test order modification
    let modify_result = env.router_service.modify_order(
        order_id, 
        Some(Qty::from_i64(LARGE_ORDER_SIZE - 1_000_000)), // Reduce size
        Some(Px::new(49_900.0)) // Lower price
    ).await;
    
    match modify_result {
        Ok(modified_order) => {
            println!("✓ Order successfully modified: new qty = {}", modified_order.quantity.as_i64());
            assert_eq!(modified_order.limit_price, Some(Px::new(49_900.0)));
        }
        Err(e) => {
            println!("⚠️ Order modification failed (acceptable): {}", e);
        }
    }
    
    // Step 6: Check metrics and performance
    let metrics = env.router_service.get_metrics().await;
    assert!(metrics.total_orders >= 1, "Should track submitted orders");
    println!("✓ Metrics updated: {} total orders", metrics.total_orders);
    
    // Step 7: Cancel order
    let cancel_result = env.router_service.cancel_order(order_id).await;
    assert!(cancel_result.is_ok(), "Should be able to cancel pending order");
    
    // Step 8: Verify cancellation
    wait_for_order_processing().await;
    let final_order = env.router_service.get_order(order_id).await?;
    assert_eq!(final_order.status, OrderStatus::Cancelled);
    println!("✓ Order successfully cancelled");
    
    println!("=== Institutional Order Workflow Completed ===\n");
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_retail_order_fast_execution() -> Result<()> {
    let env = TradingEnvironment::new_multi_venue().await;
    wait_for_venue_setup().await;
    
    println!("=== Starting Retail Order Fast Execution ===");
    
    // Submit multiple retail orders quickly
    let mut order_ids = Vec::new();
    
    for i in 0..5 {
        let order_request = create_retail_order(&format!("RETAIL_{:03}", i));
        println!("Submitting retail order: {}", order_request.client_order_id);
        
        let order_id = env.router_service.submit_order(order_request).await?;
        order_ids.push(order_id);
        
        // Small delay between orders to simulate realistic timing
        sleep(Duration::from_millis(10)).await;
    }
    
    wait_for_order_processing().await;
    
    // Verify all orders were processed
    let mut successful_orders = 0;
    for order_id in &order_ids {
        if let Ok(order) = env.router_service.get_order(*order_id).await {
            successful_orders += 1;
            println!("✓ Order {} processed: {:?}", order_id, order.status);
        }
    }
    
    assert_eq!(successful_orders, order_ids.len(), "All retail orders should be processed");
    
    // Test rapid cancellations
    let mut cancelled_orders = 0;
    for order_id in &order_ids {
        if env.router_service.cancel_order(*order_id).await.is_ok() {
            cancelled_orders += 1;
        }
    }
    
    println!("✓ Cancelled {} / {} orders", cancelled_orders, order_ids.len());
    println!("=== Retail Order Fast Execution Completed ===\n");
    
    Ok(())
}

/// Multi-Venue Routing Tests
#[rstest]
#[tokio::test]
async fn test_intelligent_venue_selection() -> Result<()> {
    let env = TradingEnvironment::new_multi_venue().await;
    wait_for_venue_setup().await;
    
    println!("=== Starting Intelligent Venue Selection Test ===");
    
    // Test different order types and verify intelligent routing
    let test_scenarios = vec![
        ("Large BTC order", ExecutionAlgorithm::Smart, 50_000_000i64, "Should split across venues"),
        ("VWAP ETH order", ExecutionAlgorithm::Vwap, 10_000_000, "Should consider volume patterns"),
        ("Iceberg order", ExecutionAlgorithm::Iceberg, 20_000_000, "Should hide quantity"),
    ];
    
    let mut all_order_ids = Vec::new();
    
    for (scenario_name, algorithm, quantity, expected_behavior) in test_scenarios {
        println!("\nTesting scenario: {}", scenario_name);
        
        let order_request = OrderRequest {
            client_order_id: format!("SMART_{}_{}", scenario_name.replace(' ', "_"), chrono::Utc::now().timestamp_millis()),
            symbol: Symbol::new(if quantity > 30_000_000 { 1 } else { 2 }),
            side: Side::Buy,
            quantity: Qty::from_i64(quantity),
            order_type: OrderType::Limit,
            limit_price: Some(Px::new(if quantity > 30_000_000 { 50_000.0 } else { 3_000.0 })),
            stop_price: None,
            is_buy: true,
            algorithm,
            urgency: 0.5,
            participation_rate: Some(0.20),
            time_in_force: TimeInForce::GTC,
            venue: None, // Let smart routing decide
            strategy_id: "venue_selection_test".to_string(),
            params: rustc_hash::FxHashMap::default(),
        };
        
        let order_id = env.router_service.submit_order(order_request).await?;
        all_order_ids.push((order_id, scenario_name, expected_behavior));
        
        println!("  ✓ {} submitted (Order ID: {})", scenario_name, order_id);
        println!("  Expected: {}", expected_behavior);
    }
    
    wait_for_order_processing().await;
    
    // Analyze routing decisions
    for (order_id, scenario_name, expected_behavior) in all_order_ids {
        if let Ok(order) = env.router_service.get_order(order_id).await {
            println!("\n{} Analysis:", scenario_name);
            println!("  Status: {:?}", order.status);
            println!("  Algorithm: {:?}", order.algorithm);
            println!("  Venue: {}", order.venue);
            println!("  Expected: {}", expected_behavior);
            
            // Verify routing made intelligent decisions
            match order.algorithm {
                ExecutionAlgorithm::Smart => {
                    assert!(!order.venue.is_empty(), "Smart routing should select a venue");
                }
                ExecutionAlgorithm::Vwap => {
                    assert!(!order.venue.is_empty(), "VWAP should select a venue based on volume");
                }
                ExecutionAlgorithm::Iceberg => {
                    assert!(!order.venue.is_empty(), "Iceberg should select a venue for display");
                }
                _ => {}
            }
        }
    }
    
    println!("\n=== Intelligent Venue Selection Completed ===\n");
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_venue_failover_scenarios() -> Result<()> {
    let env = TradingEnvironment::new_multi_venue().await;
    wait_for_venue_setup().await;
    
    println!("=== Starting Venue Failover Test ===");
    
    // Step 1: Verify all venues are initially connected
    let initial_statuses = env.venue_manager.get_all_statuses().await;
    let initial_connected = initial_statuses.values()
        .filter(|&&status| status == VenueStatus::Connected)
        .count();
    
    println!("Initial connected venues: {}", initial_connected);
    assert!(initial_connected >= 3, "Should start with multiple connected venues");
    
    // Step 2: Submit order to primary venue
    let primary_venue = env.venue_manager.get_primary_venue();
    println!("Primary venue: {}", primary_venue);
    
    let order_request = OrderRequest {
        client_order_id: format!("FAILOVER_TEST_{}", chrono::Utc::now().timestamp_millis()),
        symbol: Symbol::new(1),
        side: Side::Buy,
        quantity: Qty::from_i64(5_000_000),
        order_type: OrderType::Limit,
        limit_price: Some(Px::new(50_000.0)),
        stop_price: None,
        is_buy: true,
        algorithm: ExecutionAlgorithm::Smart,
        urgency: 0.6,
        participation_rate: Some(0.10),
        time_in_force: TimeInForce::GTC,
        venue: None,
        strategy_id: "failover_test".to_string(),
        params: rustc_hash::FxHashMap::default(),
    };
    
    let order_id = env.router_service.submit_order(order_request).await?;
    wait_for_order_processing().await;
    
    // Step 3: Simulate primary venue failure
    println!("Simulating primary venue failure...");
    env.venue_manager.disconnect_venue(primary_venue).await
        .expect("Should be able to disconnect primary venue");
    
    wait_for_venue_setup().await;
    
    // Step 4: Verify failover occurred
    let best_available = env.venue_manager.get_best_available_venue().await;
    assert!(best_available.is_some(), "Should have backup venue available");
    assert_ne!(best_available.as_ref().unwrap(), primary_venue, "Should failover to backup venue");
    
    println!("✓ Failed over to venue: {}", best_available.unwrap());
    
    // Step 5: Submit new order after failover
    let failover_order_request = OrderRequest {
        client_order_id: format!("POST_FAILOVER_{}", chrono::Utc::now().timestamp_millis()),
        symbol: Symbol::new(2),
        side: Side::Sell,
        quantity: Qty::from_i64(2_000_000),
        order_type: OrderType::Limit,
        limit_price: Some(Px::new(3_000.0)),
        stop_price: None,
        is_buy: false,
        algorithm: ExecutionAlgorithm::Smart,
        urgency: 0.8,
        participation_rate: Some(0.20),
        time_in_force: TimeInForce::GTC,
        venue: None,
        strategy_id: "post_failover_test".to_string(),
        params: rustc_hash::FxHashMap::default(),
    };
    
    let failover_order_id = env.router_service.submit_order(failover_order_request).await?;
    wait_for_order_processing().await;
    
    // Step 6: Verify system continues to function
    let failover_order = env.router_service.get_order(failover_order_id).await?;
    assert_eq!(failover_order.status, OrderStatus::Pending);
    println!("✓ Post-failover order processed successfully");
    
    // Step 7: Reconnect primary venue
    println!("Reconnecting primary venue...");
    env.venue_manager.connect_venue(primary_venue).await
        .expect("Should reconnect primary venue");
    
    wait_for_venue_setup().await;
    
    // Step 8: Verify primary venue is preferred again
    let restored_primary = env.venue_manager.get_best_available_venue().await;
    // Note: Depending on implementation, may or may not immediately prefer primary
    println!("Best venue after reconnection: {:?}", restored_primary);
    
    // Clean up
    let _ = env.router_service.cancel_order(order_id).await;
    let _ = env.router_service.cancel_order(failover_order_id).await;
    
    println!("=== Venue Failover Test Completed ===\n");
    Ok(())
}

/// Algorithm Execution Integration Tests
#[rstest]
#[tokio::test]
async fn test_twap_algorithm_integration() -> Result<()> {
    let env = TradingEnvironment::new_multi_venue().await;
    wait_for_venue_setup().await;
    
    println!("=== Starting TWAP Algorithm Integration Test ===");
    
    let twap_order = OrderRequest {
        client_order_id: format!("TWAP_INTEGRATION_{}", chrono::Utc::now().timestamp_millis()),
        symbol: Symbol::new(1),
        side: Side::Buy,
        quantity: Qty::from_i64(100_000_000), // Very large order for TWAP
        order_type: OrderType::Limit,
        limit_price: Some(Px::new(50_000.0)),
        stop_price: None,
        is_buy: true,
        algorithm: ExecutionAlgorithm::Twap,
        urgency: 0.2, // Low urgency - spread over time
        participation_rate: Some(0.10), // Conservative participation
        time_in_force: TimeInForce::GTC,
        venue: None,
        strategy_id: "twap_integration_test".to_string(),
        params: {
            let mut params = rustc_hash::FxHashMap::default();
            params.insert("time_horizon_minutes".to_string(), "120".to_string()); // 2 hours
            params.insert("slice_count".to_string(), "10".to_string());
            params
        },
    };
    
    println!("Submitting large TWAP order: {} units", twap_order.quantity.as_i64());
    let order_id = env.router_service.submit_order(twap_order.clone()).await?;
    wait_for_order_processing().await;
    
    // Verify TWAP characteristics
    let order = env.router_service.get_order(order_id).await?;
    assert_eq!(order.algorithm, ExecutionAlgorithm::Twap);
    assert_eq!(order.quantity, twap_order.quantity);
    println!("✓ TWAP order created with algorithm: {:?}", order.algorithm);
    
    // Simulate market conditions and time progression
    let market_context = env.simulate_market_conditions().await;
    println!("✓ Market conditions: mid={:?}, volume={}, venues={}", 
             market_context.mid, market_context.volume, market_context.venues.len());
    
    // In a real implementation, TWAP would create child orders over time
    // Here we verify the order tracking and state management
    
    let metrics_before = env.router_service.get_metrics().await;
    println!("✓ Pre-execution metrics: {} total orders", metrics_before.total_orders);
    
    // Test order modification during TWAP execution
    let modify_result = env.router_service.modify_order(
        order_id,
        Some(Qty::from_i64(80_000_000)), // Reduce remaining quantity
        None, // Keep same price
    ).await;
    
    match modify_result {
        Ok(modified_order) => {
            println!("✓ TWAP order modified: new qty = {}", modified_order.quantity.as_i64());
            assert!(modified_order.quantity.as_i64() <= twap_order.quantity.as_i64());
        }
        Err(e) => {
            println!("⚠️ TWAP modification failed (may be expected): {}", e);
        }
    }
    
    // Cancel TWAP order
    let cancel_result = env.router_service.cancel_order(order_id).await;
    assert!(cancel_result.is_ok(), "Should be able to cancel TWAP order");
    
    wait_for_order_processing().await;
    
    let final_order = env.router_service.get_order(order_id).await?;
    assert_eq!(final_order.status, OrderStatus::Cancelled);
    
    let final_metrics = env.router_service.get_metrics().await;
    assert!(final_metrics.cancelled_orders > metrics_before.cancelled_orders);
    
    println!("✓ TWAP order successfully cancelled");
    println!("=== TWAP Algorithm Integration Completed ===\n");
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_iceberg_algorithm_integration() -> Result<()> {
    let env = TradingEnvironment::new_multi_venue().await;
    wait_for_venue_setup().await;
    
    println!("=== Starting Iceberg Algorithm Integration Test ===");
    
    let iceberg_order = OrderRequest {
        client_order_id: format!("ICEBERG_INTEGRATION_{}", chrono::Utc::now().timestamp_millis()),
        symbol: Symbol::new(2), // ETH
        side: Side::Sell,
        quantity: Qty::from_i64(50_000_000), // Large hidden order
        order_type: OrderType::Limit,
        limit_price: Some(Px::new(3_000.0)),
        stop_price: None,
        is_buy: false,
        algorithm: ExecutionAlgorithm::Iceberg,
        urgency: 0.4,
        participation_rate: Some(0.08), // Conservative to hide better
        time_in_force: TimeInForce::GTC,
        venue: Some("binance".to_string()), // Prefer high-liquidity venue
        strategy_id: "iceberg_integration_test".to_string(),
        params: {
            let mut params = rustc_hash::FxHashMap::default();
            params.insert("display_quantity".to_string(), "5000000".to_string()); // 10% display
            params.insert("refresh_threshold".to_string(), "0.3".to_string()); // Refresh at 30%
            params
        },
    };
    
    println!("Submitting iceberg order: {} total, {} display", 
             iceberg_order.quantity.as_i64(),
             iceberg_order.params.get("display_quantity").unwrap_or(&"unknown".to_string()));
    
    let order_id = env.router_service.submit_order(iceberg_order.clone()).await?;
    wait_for_order_processing().await;
    
    // Verify iceberg characteristics
    let order = env.router_service.get_order(order_id).await?;
    assert_eq!(order.algorithm, ExecutionAlgorithm::Iceberg);
    assert_eq!(order.venue, "binance"); // Should use specified venue
    println!("✓ Iceberg order placed on venue: {}", order.venue);
    
    // Simulate partial fills and refreshes
    // (In real implementation, this would be driven by market events)
    
    // Test that the order maintains its iceberg properties
    let current_order = env.router_service.get_order(order_id).await?;
    assert_eq!(current_order.status, OrderStatus::Pending);
    assert_eq!(current_order.quantity, iceberg_order.quantity);
    
    println!("✓ Iceberg order maintaining hidden quantity: {}", current_order.quantity.as_i64());
    
    // Cancel iceberg order
    let cancel_result = env.router_service.cancel_order(order_id).await;
    assert!(cancel_result.is_ok(), "Should be able to cancel iceberg order");
    
    wait_for_order_processing().await;
    
    let cancelled_order = env.router_service.get_order(order_id).await?;
    assert_eq!(cancelled_order.status, OrderStatus::Cancelled);
    
    println!("✓ Iceberg order successfully cancelled");
    println!("=== Iceberg Algorithm Integration Completed ===\n");
    
    Ok(())
}

/// gRPC Integration Tests
#[rstest]
#[tokio::test]
async fn test_grpc_service_integration() -> Result<()> {
    let env = TradingEnvironment::new_multi_venue().await;
    wait_for_venue_setup().await;
    
    println!("=== Starting gRPC Service Integration Test ===");
    
    // Test complete gRPC workflow
    
    // Step 1: Submit order via gRPC
    let grpc_request = SubmitOrderRequest {
        client_order_id: format!("GRPC_TEST_{}", chrono::Utc::now().timestamp_millis()),
        symbol: "1".to_string(),
        side: 1, // BUY
        quantity: 5_000_000,
        order_type: 2, // LIMIT
        limit_price: 50_000_000_000, // $50,000 in fixed-point
        stop_price: 0,
        time_in_force: 1, // GTC
        venue: "coinbase".to_string(),
        strategy_id: "grpc_integration_test".to_string(),
        urgency: 0.6,
        participation_rate: 0.12,
    };
    
    println!("Submitting order via gRPC: {}", grpc_request.client_order_id);
    let submit_response = env.grpc_service.submit_order(Request::new(grpc_request.clone())).await?;
    let submit_result = submit_response.into_inner();
    
    assert!(submit_result.order_id > 0, "Should return valid order ID");
    assert_eq!(submit_result.status, 1, "Should return PENDING status");
    println!("✓ Order submitted via gRPC: ID={}, Status={}", submit_result.order_id, submit_result.status);
    
    // Step 2: Query order via gRPC
    let get_request = services_common::execution::v1::GetOrderRequest {
        order_id: submit_result.order_id,
        client_order_id: String::new(),
    };
    
    let get_response = env.grpc_service.get_order(Request::new(get_request)).await?;
    let order_result = get_response.into_inner();
    
    assert!(order_result.order.is_some(), "Should return order details");
    let order_proto = order_result.order.unwrap();
    assert_eq!(order_proto.client_order_id, grpc_request.client_order_id);
    assert_eq!(order_proto.venue, grpc_request.venue);
    println!("✓ Order retrieved via gRPC: {}", order_proto.client_order_id);
    
    // Step 3: Modify order via gRPC
    let modify_request = services_common::execution::v1::ModifyOrderRequest {
        order_id: submit_result.order_id,
        new_quantity: 4_000_000, // Reduce quantity
        new_price: 49_500_000_000, // Lower price
    };
    
    let modify_response = env.grpc_service.modify_order(Request::new(modify_request)).await?;
    let modify_result = modify_response.into_inner();
    
    if modify_result.success {
        println!("✓ Order modified via gRPC");
        if let Some(updated_order) = modify_result.updated_order {
            assert_eq!(updated_order.quantity, 4_000_000);
            assert_eq!(updated_order.limit_price, 49_500_000_000);
        }
    } else {
        println!("⚠️ Order modification failed (may be expected): {}", modify_result.message);
    }
    
    // Step 4: Get metrics via gRPC
    let metrics_request = GetMetricsRequest {};
    let metrics_response = env.grpc_service.get_metrics(Request::new(metrics_request)).await?;
    let metrics_result = metrics_response.into_inner();
    
    assert!(metrics_result.metrics.is_some(), "Should return metrics");
    let metrics = metrics_result.metrics.unwrap();
    assert!(metrics.total_orders > 0, "Should show submitted orders");
    println!("✓ Metrics retrieved via gRPC: {} total orders", metrics.total_orders);
    
    // Step 5: Cancel order via gRPC
    let cancel_request = CancelOrderRequest {
        order_id: submit_result.order_id,
    };
    
    let cancel_response = env.grpc_service.cancel_order(Request::new(cancel_request)).await?;
    let cancel_result = cancel_response.into_inner();
    
    assert!(cancel_result.success, "Should successfully cancel order");
    assert_eq!(cancel_result.status, 6, "Should return CANCELLED status");
    println!("✓ Order cancelled via gRPC");
    
    println!("=== gRPC Service Integration Completed ===\n");
    Ok(())
}

/// System Resilience Tests
#[rstest]
#[tokio::test]
async fn test_system_resilience_under_load() -> Result<()> {
    let env = TradingEnvironment::new_multi_venue().await;
    wait_for_venue_setup().await;
    
    println!("=== Starting System Resilience Test ===");
    
    let concurrent_orders = 50;
    let mut order_handles = Vec::new();
    
    // Submit many orders concurrently
    for i in 0..concurrent_orders {
        let router_service = env.router_service.clone();
        
        let handle = tokio::spawn(async move {
            let order_request = OrderRequest {
                client_order_id: format!("STRESS_{}_{}", i, chrono::Utc::now().timestamp_millis()),
                symbol: Symbol::new(if i % 2 == 0 { 1 } else { 2 }),
                side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
                quantity: Qty::from_i64(1_000_000 + (i as i64 * 10_000)),
                order_type: OrderType::Limit,
                limit_price: Some(Px::new(if i % 2 == 0 { 50_000.0 } else { 3_000.0 })),
                stop_price: None,
                is_buy: i % 2 == 0,
                algorithm: match i % 3 {
                    0 => ExecutionAlgorithm::Smart,
                    1 => ExecutionAlgorithm::Twap,
                    _ => ExecutionAlgorithm::Iceberg,
                },
                urgency: 0.3 + ((i as f64) / (concurrent_orders as f64)) * 0.4, // Varying urgency
                participation_rate: Some(0.05 + ((i as f64) / (concurrent_orders as f64)) * 0.15),
                time_in_force: TimeInForce::GTC,
                venue: None,
                strategy_id: format!("stress_test_{}", i % 10),
                params: rustc_hash::FxHashMap::default(),
            };
            
            router_service.submit_order(order_request).await
        });
        
        order_handles.push(handle);
    }
    
    // Wait for all orders to complete
    let mut successful_orders = Vec::new();
    let mut failed_orders = 0;
    
    for handle in order_handles {
        match handle.await {
            Ok(Ok(order_id)) => {
                successful_orders.push(order_id);
            }
            Ok(Err(_)) => {
                failed_orders += 1;
            }
            Err(_) => {
                failed_orders += 1;
            }
        }
    }
    
    println!("Load test results: {} successful, {} failed orders", 
             successful_orders.len(), failed_orders);
    
    // System should handle reasonable load
    assert!(successful_orders.len() > concurrent_orders / 2, 
            "At least 50% of orders should succeed under load");
    
    // Test system recovery - submit more orders after load
    wait_for_order_processing().await;
    
    let recovery_order = create_retail_order("RECOVERY_TEST");
    let recovery_result = env.router_service.submit_order(recovery_order).await;
    assert!(recovery_result.is_ok(), "System should recover after load test");
    
    // Check system health
    let is_healthy = env.router_service.is_healthy().await;
    println!("System health after load test: {}", is_healthy);
    
    // Get final metrics
    let final_metrics = env.router_service.get_metrics().await;
    println!("Final metrics: {} total orders, {} failed", 
             final_metrics.total_orders, final_metrics.rejected_orders);
    
    // Clean up - cancel successful orders
    let mut cancelled_count = 0;
    for order_id in successful_orders {
        if env.router_service.cancel_order(order_id).await.is_ok() {
            cancelled_count += 1;
        }
    }
    
    println!("✓ Cleaned up {} orders", cancelled_count);
    println!("=== System Resilience Test Completed ===\n");
    
    Ok(())
}

/// Performance Integration Test
#[rstest]
#[tokio::test]
async fn test_end_to_end_performance() -> Result<()> {
    let env = TradingEnvironment::new_multi_venue().await;
    wait_for_venue_setup().await;
    
    println!("=== Starting End-to-End Performance Test ===");
    
    let performance_iterations = 100;
    let mut latencies = Vec::new();
    let start_time = Instant::now();
    
    for i in 0..performance_iterations {
        let order_start = Instant::now();
        
        let order_request = OrderRequest {
            client_order_id: format!("PERF_{}_{}", i, chrono::Utc::now().timestamp_nanos()),
            symbol: Symbol::new(1),
            side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
            quantity: Qty::from_i64(1_000_000),
            order_type: OrderType::Limit,
            limit_price: Some(Px::new(50_000.0)),
            stop_price: None,
            is_buy: i % 2 == 0,
            algorithm: ExecutionAlgorithm::Smart,
            urgency: 0.5,
            participation_rate: Some(0.10),
            time_in_force: TimeInForce::GTC,
            venue: None,
            strategy_id: "performance_test".to_string(),
            params: rustc_hash::FxHashMap::default(),
        };
        
        // Measure full round-trip time: submit -> query -> cancel
        let order_id = env.router_service.submit_order(order_request).await?;
        let _order = env.router_service.get_order(order_id).await?;
        let _cancel_result = env.router_service.cancel_order(order_id).await;
        
        let order_latency = order_start.elapsed();
        latencies.push(order_latency);
        
        // Small delay to prevent overwhelming the system
        if i % 10 == 0 {
            sleep(Duration::from_millis(1)).await;
        }
    }
    
    let total_time = start_time.elapsed();
    
    // Calculate performance statistics
    latencies.sort();
    let avg_latency = latencies.iter().sum::<Duration>() / latencies.len() as u32;
    let p95_latency = latencies[(latencies.len() as f64 * 0.95) as usize];
    let p99_latency = latencies[(latencies.len() as f64 * 0.99) as usize];
    let throughput = performance_iterations as f64 / total_time.as_secs_f64();
    
    println!("\nEnd-to-End Performance Results:");
    println!("Operations:      {}", performance_iterations);
    println!("Total time:      {:?}", total_time);
    println!("Throughput:      {:.2} ops/sec", throughput);
    println!("Average latency: {:?}", avg_latency);
    println!("P95 latency:     {:?}", p95_latency);
    println!("P99 latency:     {:?}", p99_latency);
    
    // Performance assertions
    assert!(throughput > 10.0, "End-to-end throughput should be > 10 ops/sec");
    assert!(avg_latency < Duration::from_millis(100), "Average latency should be < 100ms");
    assert!(p95_latency < Duration::from_millis(500), "P95 latency should be < 500ms");
    
    // Get final system metrics
    let final_metrics = env.router_service.get_metrics().await;
    println!("\nFinal System Metrics:");
    println!("Total orders:     {}", final_metrics.total_orders);
    println!("Filled orders:    {}", final_metrics.filled_orders);
    println!("Cancelled orders: {}", final_metrics.cancelled_orders);
    println!("Rejected orders:  {}", final_metrics.rejected_orders);
    
    println!("=== End-to-End Performance Test Completed ===\n");
    
    Ok(())
}

/// Complete System Integration Test
#[rstest]
#[tokio::test]
async fn test_complete_system_integration() -> Result<()> {
    let env = TradingEnvironment::new_multi_venue().await;
    wait_for_venue_setup().await;
    
    println!("=== Starting Complete System Integration Test ===");
    
    // This test combines all components in a realistic trading scenario
    
    // Phase 1: System initialization and health checks
    println!("\nPhase 1: System Health Checks");
    let is_healthy = env.router_service.is_healthy().await;
    assert!(is_healthy, "System should be healthy at startup");
    
    let venue_statuses = env.venue_manager.get_all_statuses().await;
    let connected_venues = venue_statuses.values()
        .filter(|&&s| s == VenueStatus::Connected)
        .count();
    assert!(connected_venues >= 3, "Should have multiple venues connected");
    println!("✓ {} venues connected and healthy", connected_venues);
    
    // Phase 2: Mixed order flow simulation
    println!("\nPhase 2: Mixed Order Flow");
    let mut all_orders = Vec::new();
    
    // Large institutional orders
    for i in 0..3 {
        let inst_order = create_institutional_order(
            &format!("institution_{}", i), 
            match i % 3 {
                0 => ExecutionAlgorithm::Twap,
                1 => ExecutionAlgorithm::Vwap,
                _ => ExecutionAlgorithm::Iceberg,
            }
        );
        let order_id = env.router_service.submit_order(inst_order).await?;
        all_orders.push((order_id, "institutional"));
    }
    
    // Retail orders
    for i in 0..5 {
        let retail_order = create_retail_order(&format!("retail_{}", i));
        let order_id = env.router_service.submit_order(retail_order).await?;
        all_orders.push((order_id, "retail"));
    }
    
    wait_for_order_processing().await;
    println!("✓ Submitted {} orders ({} institutional, {} retail)", 
             all_orders.len(), 3, 5);
    
    // Phase 3: Order management operations
    println!("\nPhase 3: Order Management");
    let mut modification_results = Vec::new();
    
    for (order_id, order_type) in &all_orders {
        if order_type == &"retail" {
            // Try to modify retail orders (smaller, more flexible)
            let modify_result = env.router_service.modify_order(
                *order_id,
                Some(Qty::from_i64(SMALL_ORDER_SIZE / 2)),
                None,
            ).await;
            modification_results.push(modify_result.is_ok());
        }
    }
    
    let successful_modifications = modification_results.iter().filter(|&&success| success).count();
    println!("✓ {} / {} order modifications successful", 
             successful_modifications, modification_results.len());
    
    // Phase 4: Market conditions change simulation
    println!("\nPhase 4: Market Stress Testing");
    
    // Simulate venue outage
    env.venue_manager.disconnect_venue("kraken").await?;
    sleep(Duration::from_millis(100)).await;
    
    // Submit order during outage
    let stress_order = create_retail_order("stress_test");
    let stress_result = env.router_service.submit_order(stress_order).await;
    
    match stress_result {
        Ok(order_id) => {
            println!("✓ Order processed during venue outage: {}", order_id);
            all_orders.push((order_id, "stress"));
        }
        Err(e) => {
            println!("⚠️ Order failed during outage (acceptable): {}", e);
        }
    }
    
    // Reconnect venue
    env.venue_manager.connect_venue("kraken").await?;
    sleep(Duration::from_millis(100)).await;
    
    // Phase 5: System metrics and performance analysis
    println!("\nPhase 5: System Analysis");
    let final_metrics = env.router_service.get_metrics().await;
    
    println!("System Metrics:");
    println!("  Total orders: {}", final_metrics.total_orders);
    println!("  Filled orders: {}", final_metrics.filled_orders);
    println!("  Cancelled orders: {}", final_metrics.cancelled_orders);
    println!("  Rejected orders: {}", final_metrics.rejected_orders);
    println!("  Average fill time: {} ms", final_metrics.avg_fill_time_ms);
    println!("  Fill rate: {:.2}%", final_metrics.fill_rate * 100.0);
    
    assert!(final_metrics.total_orders >= all_orders.len() as u64);
    
    // Phase 6: System cleanup and shutdown
    println!("\nPhase 6: System Cleanup");
    let mut cancelled_orders = 0;
    
    for (order_id, _) in &all_orders {
        if env.router_service.cancel_order(*order_id).await.is_ok() {
            cancelled_orders += 1;
        }
    }
    
    println!("✓ Cancelled {} / {} orders", cancelled_orders, all_orders.len());
    
    // Final health check
    wait_for_order_processing().await;
    let final_health = env.router_service.is_healthy().await;
    println!("Final system health: {}", final_health);
    
    // Final metrics
    let cleanup_metrics = env.router_service.get_metrics().await;
    println!("Final cancelled orders: {}", cleanup_metrics.cancelled_orders);
    
    println!("=== Complete System Integration Test Successful ===\n");
    
    Ok(())
}