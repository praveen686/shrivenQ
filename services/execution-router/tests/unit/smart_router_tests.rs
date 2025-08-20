//! Unit tests for smart order router algorithms
//!
//! These tests verify the correctness of each execution algorithm:
//! - TWAP (Time-Weighted Average Price)
//! - VWAP (Volume-Weighted Average Price) 
//! - Iceberg orders
//! - POV (Percentage of Volume)
//! - Smart routing
//! - Order slicing and timing logic

use execution_router::{
    smart_router::*,
    ExecutionAlgorithm, OrderRequest, OrderType, TimeInForce, OrderId
};
use services_common::{Px, Qty, Side, Symbol, Ts};
use anyhow::Result;
use std::time::Instant;
use rstest::*;

/// Test utilities and fixtures
mod test_utils {
    use super::*;
    
    pub fn create_test_market_context(venues: Vec<String>) -> MarketContext {
        MarketContext {
            bid: Some(Px::new(99.95)),
            ask: Some(Px::new(100.05)),
            mid: Some(Px::new(100.00)),
            spread: Some(10), // 0.10 spread
            volume: 1_000_000,
            volatility: 0.025, // 2.5% volatility
            venues,
        }
    }

    pub fn create_test_order_request(
        algorithm: ExecutionAlgorithm,
        quantity: i64,
        side: Side,
    ) -> OrderRequest {
        OrderRequest {
            client_order_id: "test_order_001".to_string(),
            symbol: Symbol(1), // BTC/USDT
            side,
            quantity: Qty::from_i64(quantity),
            order_type: OrderType::Limit,
            limit_price: Some(Px::new(100.00)),
            stop_price: None,
            is_buy: matches!(side, Side::Buy),
            algorithm,
            urgency: 0.5,
            participation_rate: Some(0.10), // 10% participation
            time_in_force: TimeInForce::GTC,
            venue: None,
            strategy_id: "test_strategy".to_string(),
        }
    }

    pub fn assert_child_orders_valid(child_orders: &[ChildOrder], parent_qty: i64, venues: &[String]) {
        // Basic validations
        assert!(!child_orders.is_empty(), "Should generate child orders");
        
        // Verify total quantity matches parent (allowing for rounding)
        let total_qty: i64 = child_orders.iter().map(|c| c.quantity.as_i64()).sum();
        let diff = (total_qty - parent_qty).abs();
        assert!(diff <= venues.len() as i64, "Total quantity should match parent order within rounding error");

        // Verify all venues are valid
        for child in child_orders {
            assert!(venues.contains(&child.venue), "Child order venue should be valid");
            assert!(child.quantity.as_i64() > 0, "Child order quantity should be positive");
        }
    }
}

use test_utils::*;

/// TWAP Algorithm Tests
#[fixture]
fn twap_algo() -> TwapAlgo {
    TwapAlgo::new()
}

#[rstest]
fn test_twap_algorithm_basic(twap_algo: TwapAlgo) -> Result<()> {
    let venues = vec!["Binance".to_string()];
    let market_context = create_test_market_context(venues.clone());
    let request = create_test_order_request(ExecutionAlgorithm::Twap, 100_000, Side::Buy);

    let child_orders = twap_algo.execute(&request, &market_context)?;

    // TWAP should create multiple time slices
    assert_eq!(child_orders.len(), 10, "TWAP should create 10 time slices");
    
    // Each slice should have equal quantity (10,000 each)
    let expected_qty_per_slice = request.quantity.as_i64() / 10;
    for child in &child_orders {
        assert_eq!(child.quantity.as_i64(), expected_qty_per_slice);
        assert_eq!(child.order_type, OrderType::Limit);
        assert_eq!(child.time_in_force, TimeInForce::IOC); // TWAP uses IOC for time slices
        assert_eq!(child.limit_price, market_context.mid);
    }

    // Verify child order IDs are unique
    let mut ids: Vec<u64> = child_orders.iter().map(|c| c.child_id.0).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), child_orders.len(), "All child order IDs should be unique");

    Ok(())
}

#[rstest]
fn test_twap_algorithm_sell_side(twap_algo: TwapAlgo) -> Result<()> {
    let venues = vec!["Binance".to_string()];
    let market_context = create_test_market_context(venues);
    let request = create_test_order_request(ExecutionAlgorithm::Twap, 50_000, Side::Sell);

    let child_orders = twap_algo.execute(&request, &market_context)?;

    assert_eq!(child_orders.len(), 10);
    let expected_qty_per_slice = 5_000;
    for child in &child_orders {
        assert_eq!(child.quantity.as_i64(), expected_qty_per_slice);
    }

    Ok(())
}

#[rstest]
fn test_twap_no_venues_error(twap_algo: TwapAlgo) -> Result<()> {
    let market_context = MarketContext {
        bid: Some(Px::new(99.95)),
        ask: Some(Px::new(100.05)),
        mid: Some(Px::new(100.00)),
        spread: Some(10),
        volume: 1_000_000,
        volatility: 0.025,
        venues: vec![], // No venues available
    };
    let request = create_test_order_request(ExecutionAlgorithm::Twap, 100_000, Side::Buy);

    let result = twap_algo.execute(&request, &market_context);
    assert!(result.is_err(), "TWAP should fail with no venues");

    Ok(())
}

/// VWAP Algorithm Tests
#[fixture]
fn vwap_algo() -> VwapAlgo {
    VwapAlgo::new()
}

#[rstest]
fn test_vwap_algorithm_basic(vwap_algo: VwapAlgo) -> Result<()> {
    let venues = vec!["Binance".to_string()];
    let market_context = create_test_market_context(venues);
    let request = create_test_order_request(ExecutionAlgorithm::Vwap, 100_000, Side::Buy);

    let child_orders = vwap_algo.execute(&request, &market_context)?;

    // VWAP should create orders based on volume profile (9 periods in current implementation)
    assert!(child_orders.len() <= 9, "VWAP should create at most 9 volume periods");
    assert!(!child_orders.is_empty(), "VWAP should create at least one order");

    // Verify volume-weighted distribution
    let total_qty: i64 = child_orders.iter().map(|c| c.quantity.as_i64()).sum();
    assert!(total_qty <= request.quantity.as_i64(), "Total quantity should not exceed parent");
    assert!(total_qty > 0, "Should allocate some quantity");

    // Verify all orders use GTC (VWAP is longer-term strategy)
    for child in &child_orders {
        assert_eq!(child.time_in_force, TimeInForce::GTC);
        assert_eq!(child.order_type, OrderType::Limit);
        assert!(child.quantity.as_i64() > 0);
    }

    Ok(())
}

#[rstest]
fn test_vwap_volume_weighting(vwap_algo: VwapAlgo) -> Result<()> {
    let venues = vec!["Binance".to_string()];
    let market_context = create_test_market_context(venues);
    let request = create_test_order_request(ExecutionAlgorithm::Vwap, 100_000, Side::Buy);

    let child_orders = vwap_algo.execute(&request, &market_context)?;

    // Should have different quantities based on volume profile
    let quantities: Vec<i64> = child_orders.iter().map(|c| c.quantity.as_i64()).collect();
    
    // Check that we have variation in quantities (not all equal)
    let first_qty = quantities[0];
    let all_same = quantities.iter().all(|&q| q == first_qty);
    assert!(!all_same || quantities.len() == 1, "VWAP quantities should vary based on volume profile");

    Ok(())
}

#[rstest]
fn test_vwap_price_adjustment(vwap_algo: VwapAlgo) -> Result<()> {
    let venues = vec!["Binance".to_string()];
    let market_context = create_test_market_context(venues);
    
    // Test buy side
    let buy_request = create_test_order_request(ExecutionAlgorithm::Vwap, 100_000, Side::Buy);
    let buy_orders = vwap_algo.execute(&buy_request, &market_context)?;
    
    // Test sell side  
    let sell_request = create_test_order_request(ExecutionAlgorithm::Vwap, 100_000, Side::Sell);
    let sell_orders = vwap_algo.execute(&sell_request, &market_context)?;

    // Both should have valid prices
    for child in &buy_orders {
        assert!(child.limit_price.is_some(), "Buy orders should have limit prices");
    }
    
    for child in &sell_orders {
        assert!(child.limit_price.is_some(), "Sell orders should have limit prices");
    }

    Ok(())
}

/// Iceberg Algorithm Tests
#[fixture]
fn iceberg_algo() -> IcebergAlgo {
    IcebergAlgo::new()
}

#[rstest]
fn test_iceberg_algorithm_basic(iceberg_algo: IcebergAlgo) -> Result<()> {
    let venues = vec!["Binance".to_string()];
    let market_context = create_test_market_context(venues);
    let request = create_test_order_request(ExecutionAlgorithm::Iceberg, 100_000, Side::Buy);

    let child_orders = iceberg_algo.execute(&request, &market_context)?;

    // Iceberg should create exactly one visible order
    assert_eq!(child_orders.len(), 1, "Iceberg should create exactly one child order");

    let child = &child_orders[0];
    // Should show only 10% of total quantity (visible portion)
    let expected_visible_qty = request.quantity.as_i64() / 10;
    assert_eq!(child.quantity.as_i64(), expected_visible_qty, "Should show 10% of total quantity");
    
    assert_eq!(child.order_type, OrderType::Limit);
    assert_eq!(child.time_in_force, TimeInForce::GTC);
    assert_eq!(child.limit_price, request.limit_price);
    assert_eq!(child.venue, "Binance");

    Ok(())
}

#[rstest]
fn test_iceberg_supports_limit_only(iceberg_algo: IcebergAlgo) {
    assert!(iceberg_algo.supports(OrderType::Limit));
    assert!(!iceberg_algo.supports(OrderType::Market));
    assert!(!iceberg_algo.supports(OrderType::Stop));
    assert!(!iceberg_algo.supports(OrderType::StopLimit));
}

#[rstest]
fn test_iceberg_no_venues_error(iceberg_algo: IcebergAlgo) -> Result<()> {
    let market_context = MarketContext {
        bid: Some(Px::new(99.95)),
        ask: Some(Px::new(100.05)),
        mid: Some(Px::new(100.00)),
        spread: Some(10),
        volume: 1_000_000,
        volatility: 0.025,
        venues: vec![],
    };
    let request = create_test_order_request(ExecutionAlgorithm::Iceberg, 100_000, Side::Buy);

    let result = iceberg_algo.execute(&request, &market_context);
    assert!(result.is_err(), "Iceberg should fail with no venues");

    Ok(())
}

/// POV Algorithm Tests
#[fixture] 
fn pov_algo() -> PovAlgo {
    PovAlgo::new()
}

#[rstest]
fn test_pov_algorithm_basic(pov_algo: PovAlgo) -> Result<()> {
    let venues = vec!["Binance".to_string()];
    let market_context = create_test_market_context(venues);
    let request = create_test_order_request(ExecutionAlgorithm::Pov, 100_000, Side::Buy);

    let child_orders = pov_algo.execute(&request, &market_context)?;

    // Currently POV returns empty (to be implemented)
    assert_eq!(child_orders.len(), 0, "POV algorithm is not yet implemented");

    Ok(())
}

#[rstest]
fn test_pov_name(pov_algo: PovAlgo) {
    assert_eq!(pov_algo.name(), "POV");
}

/// Smart Algorithm Tests
#[fixture]
fn smart_algo() -> SmartAlgo {
    SmartAlgo::new()
}

#[rstest]
fn test_smart_algorithm_basic(smart_algo: SmartAlgo) -> Result<()> {
    let venues = vec!["Binance".to_string(), "Coinbase".to_string(), "Kraken".to_string()];
    let market_context = create_test_market_context(venues.clone());
    let request = create_test_order_request(ExecutionAlgorithm::Smart, 100_000, Side::Buy);

    let child_orders = smart_algo.execute(&request, &market_context)?;

    // Smart routing should split across all venues
    assert_eq!(child_orders.len(), venues.len(), "Should create one order per venue");

    let expected_qty_per_venue = request.quantity.as_i64() / venues.len() as i64;
    for (i, child) in child_orders.iter().enumerate() {
        assert_eq!(child.quantity.as_i64(), expected_qty_per_venue);
        assert_eq!(child.venue, venues[i]);
        assert_eq!(child.order_type, request.order_type);
        assert_eq!(child.limit_price, request.limit_price);
        assert_eq!(child.time_in_force, request.time_in_force);
    }

    assert_child_orders_valid(&child_orders, request.quantity.as_i64(), &venues);

    Ok(())
}

#[rstest]
fn test_smart_algorithm_single_venue(smart_algo: SmartAlgo) -> Result<()> {
    let venues = vec!["Binance".to_string()];
    let market_context = create_test_market_context(venues.clone());
    let request = create_test_order_request(ExecutionAlgorithm::Smart, 50_000, Side::Sell);

    let child_orders = smart_algo.execute(&request, &market_context)?;

    assert_eq!(child_orders.len(), 1);
    assert_eq!(child_orders[0].quantity, request.quantity);
    assert_eq!(child_orders[0].venue, venues[0]);

    Ok(())
}

#[rstest]
fn test_smart_algorithm_no_venues_error(smart_algo: SmartAlgo) -> Result<()> {
    let market_context = MarketContext {
        bid: Some(Px::new(99.95)),
        ask: Some(Px::new(100.05)),
        mid: Some(Px::new(100.00)),
        spread: Some(10),
        volume: 1_000_000,
        volatility: 0.025,
        venues: vec![],
    };
    let request = create_test_order_request(ExecutionAlgorithm::Smart, 100_000, Side::Buy);

    let result = smart_algo.execute(&request, &market_context);
    assert!(result.is_err(), "Smart routing should fail with no venues");

    Ok(())
}

/// Peg Algorithm Tests
#[fixture]
fn peg_algo() -> PegAlgo {
    PegAlgo::new()
}

#[rstest]
fn test_peg_algorithm_basic(peg_algo: PegAlgo) -> Result<()> {
    let venues = vec!["Binance".to_string()];
    let market_context = create_test_market_context(venues);
    let request = create_test_order_request(ExecutionAlgorithm::Peg, 100_000, Side::Buy);

    let child_orders = peg_algo.execute(&request, &market_context)?;

    // Peg should create exactly one order
    assert_eq!(child_orders.len(), 1);

    let child = &child_orders[0];
    assert_eq!(child.quantity, request.quantity);
    assert_eq!(child.order_type, OrderType::Limit);
    assert_eq!(child.time_in_force, TimeInForce::GTC);
    assert_eq!(child.limit_price, market_context.mid); // Pegged to mid price
    assert_eq!(child.venue, "Binance");

    Ok(())
}

/// Router Integration Tests
#[fixture]
fn test_router() -> Router {
    Router::new()
}

#[rstest]  
#[tokio::test]
async fn test_router_creation(test_router: Router) {
    // Router should be created with all algorithm engines
    assert_eq!(test_router.active_orders.len(), 0, "New router should have no active orders");
    
    // Test that metrics are initialized
    assert_eq!(test_router.metrics.orders_routed.load(std::sync::atomic::Ordering::Relaxed), 0);
}

#[rstest]
#[tokio::test] 
async fn test_router_add_venue(test_router: Router) {
    let venue = VenueConnection {
        name: "TestVenue".to_string(),
        is_connected: true,
        latency_us: 500,
        liquidity: 1_000_000.0,
        maker_fee_bp: 5,
        taker_fee_bp: 10,
        supported_types: vec![OrderType::Market, OrderType::Limit],
        last_heartbeat: Instant::now(),
    };

    test_router.add_venue(venue.clone());

    // Verify venue was added
    let venues = test_router.venues.read();
    assert!(venues.contains_key("TestVenue"));
    let stored_venue = &venues["TestVenue"];
    assert_eq!(stored_venue.name, venue.name);
    assert_eq!(stored_venue.is_connected, venue.is_connected);
    assert_eq!(stored_venue.latency_us, venue.latency_us);
}

#[rstest]
#[tokio::test]
async fn test_router_route_order_success() -> Result<()> {
    let router = Router::new();
    
    // Add a test venue
    router.add_venue(VenueConnection {
        name: "Binance".to_string(),
        is_connected: true,
        latency_us: 100,
        liquidity: 2_000_000.0,
        maker_fee_bp: 10,
        taker_fee_bp: 20,
        supported_types: vec![OrderType::Market, OrderType::Limit],
        last_heartbeat: Instant::now(),
    });

    let request = create_test_order_request(ExecutionAlgorithm::Smart, 100_000, Side::Buy);
    let order_id = router.route_order(request).await?;

    // Verify order was created
    let order = router.get_order(order_id);
    assert!(order.is_some(), "Order should exist after routing");

    let order_state = order.unwrap();
    assert_eq!(order_state.order_id, order_id);
    assert_eq!(order_state.executed_qty, Qty::ZERO);
    assert!(order_state.child_orders.is_empty()); // Child orders routed asynchronously

    // Verify metrics updated
    assert_eq!(router.metrics.orders_routed.load(std::sync::atomic::Ordering::Relaxed), 1);

    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_router_cancel_order() -> Result<()> {
    let router = Router::new();
    
    router.add_venue(VenueConnection {
        name: "Binance".to_string(),
        is_connected: true,
        latency_us: 100,
        liquidity: 2_000_000.0,
        maker_fee_bp: 10,
        taker_fee_bp: 20,
        supported_types: vec![OrderType::Market, OrderType::Limit],
        last_heartbeat: Instant::now(),
    });

    let request = create_test_order_request(ExecutionAlgorithm::Smart, 100_000, Side::Buy);
    let order_id = router.route_order(request).await?;

    // Cancel the order
    router.cancel_order(order_id).await?;

    // Verify order was cancelled
    let order = router.get_order(order_id).unwrap();
    assert_eq!(order.status, execution_router::OrderStatus::Cancelled);

    Ok(())
}

/// Market Context Tests
#[rstest]
fn test_market_context_creation() {
    let venues = vec!["Binance".to_string(), "Coinbase".to_string()];
    let context = create_test_market_context(venues.clone());

    assert_eq!(context.venues, venues);
    assert!(context.bid.is_some());
    assert!(context.ask.is_some());
    assert!(context.mid.is_some());
    assert!(context.spread.is_some());
    assert!(context.volume > 0);
    assert!(context.volatility > 0.0);

    // Verify bid < mid < ask
    let bid = context.bid.unwrap().as_f64();
    let ask = context.ask.unwrap().as_f64();
    let mid = context.mid.unwrap().as_f64();

    assert!(bid < mid, "Bid should be less than mid");
    assert!(mid < ask, "Mid should be less than ask");
}

/// Algorithm Name Tests
#[rstest]
fn test_algorithm_names() {
    assert_eq!(TwapAlgo::new().name(), "TWAP");
    assert_eq!(VwapAlgo::new().name(), "VWAP");
    assert_eq!(PovAlgo::new().name(), "POV");
    assert_eq!(IcebergAlgo::new().name(), "Iceberg");
    assert_eq!(SmartAlgo::new().name(), "Smart");
    assert_eq!(PegAlgo::new().name(), "Peg");
}

/// Algorithm Support Tests
#[rstest]
fn test_algorithm_supports() {
    let twap = TwapAlgo::new();
    let vwap = VwapAlgo::new();
    let pov = PovAlgo::new();
    let iceberg = IcebergAlgo::new();
    let smart = SmartAlgo::new();
    let peg = PegAlgo::new();

    // Most algorithms should support all order types
    for order_type in [OrderType::Market, OrderType::Limit, OrderType::Stop] {
        assert!(twap.supports(order_type), "TWAP should support {order_type:?}");
        assert!(vwap.supports(order_type), "VWAP should support {order_type:?}");
        assert!(pov.supports(order_type), "POV should support {order_type:?}");
        assert!(smart.supports(order_type), "Smart should support {order_type:?}");
        assert!(peg.supports(order_type), "Peg should support {order_type:?}");
    }

    // Iceberg only supports limit orders
    assert!(iceberg.supports(OrderType::Limit), "Iceberg should support Limit");
    assert!(!iceberg.supports(OrderType::Market), "Iceberg should not support Market");
}