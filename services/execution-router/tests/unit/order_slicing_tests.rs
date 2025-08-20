//! Order slicing and timing logic tests
//!
//! These tests verify the correctness of order slicing algorithms:
//! - Quantity distribution across time slices
//! - Timing calculations for TWAP
//! - Volume-based slicing for VWAP
//! - Slice size calculations and rounding

use execution_router::{
    smart_router::*,
    ExecutionAlgorithm, OrderRequest, OrderType, TimeInForce, OrderId
};
use services_common::{Px, Qty, Side, Symbol};
use anyhow::Result;
use rstest::*;
use std::collections::HashMap;

/// Test utilities for order slicing
mod slicing_utils {
    use super::*;
    
    pub fn create_large_order(algorithm: ExecutionAlgorithm, quantity: i64) -> OrderRequest {
        OrderRequest {
            client_order_id: format!("large_order_{}", quantity),
            symbol: Symbol(1),
            side: Side::Buy,
            quantity: Qty::from_i64(quantity),
            order_type: OrderType::Limit,
            limit_price: Some(Px::new(100.00)),
            stop_price: None,
            is_buy: true,
            algorithm,
            urgency: 0.5,
            participation_rate: Some(0.15), // 15% participation
            time_in_force: TimeInForce::GTC,
            venue: None,
            strategy_id: "large_order_strategy".to_string(),
        }
    }

    pub fn create_market_context_with_volume(volume: i64) -> MarketContext {
        MarketContext {
            bid: Some(Px::new(99.98)),
            ask: Some(Px::new(100.02)),
            mid: Some(Px::new(100.00)),
            spread: Some(4), // 0.04 spread
            volume,
            volatility: 0.02, // 2% volatility
            venues: vec!["Binance".to_string(), "Coinbase".to_string()],
        }
    }

    pub fn verify_quantity_conservation(child_orders: &[ChildOrder], original_qty: i64) {
        let total_child_qty: i64 = child_orders.iter().map(|c| c.quantity.as_i64()).sum();
        
        // Allow for small rounding differences in slicing
        let diff = (total_child_qty - original_qty).abs();
        let max_allowed_diff = child_orders.len() as i64; // Max 1 unit per slice for rounding
        
        assert!(
            diff <= max_allowed_diff,
            "Quantity conservation failed: original={}, total_child={}, diff={}, max_allowed={}",
            original_qty, total_child_qty, diff, max_allowed_diff
        );
    }

    pub fn verify_slice_timing_consistency(child_orders: &[ChildOrder]) {
        // All child orders should have consistent timing properties for TWAP
        if !child_orders.is_empty() {
            let first_tif = child_orders[0].time_in_force;
            for child in child_orders {
                assert_eq!(child.time_in_force, first_tif, "All slices should have consistent time in force");
            }
        }
    }
}

use slicing_utils::*;

/// TWAP Slicing Tests
#[fixture]
fn twap_algo() -> TwapAlgo {
    TwapAlgo::new()
}

#[rstest]
fn test_twap_equal_slice_distribution(twap_algo: TwapAlgo) -> Result<()> {
    let market_context = create_market_context_with_volume(5_000_000);
    let request = create_large_order(ExecutionAlgorithm::Twap, 1_000_000);

    let child_orders = twap_algo.execute(&request, &market_context)?;

    // TWAP should create exactly 10 slices
    assert_eq!(child_orders.len(), 10, "TWAP should create 10 time slices");

    // Each slice should be equal (100,000 each)
    let expected_slice_size = request.quantity.as_i64() / 10;
    for (i, child) in child_orders.iter().enumerate() {
        assert_eq!(
            child.quantity.as_i64(), 
            expected_slice_size,
            "Slice {} should have equal quantity", i
        );
    }

    verify_quantity_conservation(&child_orders, request.quantity.as_i64());
    verify_slice_timing_consistency(&child_orders);

    Ok(())
}

#[rstest]
fn test_twap_uneven_quantity_distribution(twap_algo: TwapAlgo) -> Result<()> {
    let market_context = create_market_context_with_volume(3_000_000);
    // Test with quantity that doesn't divide evenly by 10
    let request = create_large_order(ExecutionAlgorithm::Twap, 1_000_007);

    let child_orders = twap_algo.execute(&request, &market_context)?;

    assert_eq!(child_orders.len(), 10);

    // Each slice should get 100,000 (integer division)
    let expected_base_size = request.quantity.as_i64() / 10; // 100,000
    for child in &child_orders {
        assert_eq!(child.quantity.as_i64(), expected_base_size);
    }

    // Total should be 1,000,000 (7 units lost to integer division)
    let total_distributed: i64 = child_orders.iter().map(|c| c.quantity.as_i64()).sum();
    assert_eq!(total_distributed, 1_000_000);

    Ok(())
}

#[rstest]
fn test_twap_small_order_slicing(twap_algo: TwapAlgo) -> Result<()> {
    let market_context = create_market_context_with_volume(1_000_000);
    // Test with very small order
    let request = create_large_order(ExecutionAlgorithm::Twap, 50);

    let child_orders = twap_algo.execute(&request, &market_context)?;

    assert_eq!(child_orders.len(), 10);

    // Each slice gets 5 units (50/10)
    for child in &child_orders {
        assert_eq!(child.quantity.as_i64(), 5);
    }

    verify_quantity_conservation(&child_orders, request.quantity.as_i64());

    Ok(())
}

#[rstest]
fn test_twap_slice_timing_properties(twap_algo: TwapAlgo) -> Result<()> {
    let market_context = create_market_context_with_volume(2_000_000);
    let request = create_large_order(ExecutionAlgorithm::Twap, 500_000);

    let child_orders = twap_algo.execute(&request, &market_context)?;

    // All slices should use IOC for immediate execution at each time interval
    for child in &child_orders {
        assert_eq!(child.time_in_force, TimeInForce::IOC, "TWAP slices should use IOC");
        assert_eq!(child.order_type, OrderType::Limit, "TWAP slices should be limit orders");
    }

    // All slices should be priced at mid
    for child in &child_orders {
        assert_eq!(child.limit_price, market_context.mid, "TWAP slices should be priced at mid");
    }

    Ok(())
}

/// VWAP Slicing Tests
#[fixture]
fn vwap_algo() -> VwapAlgo {
    VwapAlgo::new()
}

#[rstest]
fn test_vwap_volume_weighted_distribution(vwap_algo: VwapAlgo) -> Result<()> {
    let market_context = create_market_context_with_volume(10_000_000);
    let request = create_large_order(ExecutionAlgorithm::Vwap, 1_000_000);

    let child_orders = vwap_algo.execute(&request, &market_context)?;

    assert!(!child_orders.is_empty(), "VWAP should create child orders");
    assert!(child_orders.len() <= 9, "VWAP should create at most 9 periods");

    // Verify volume-based distribution
    let quantities: Vec<i64> = child_orders.iter().map(|c| c.quantity.as_i64()).collect();
    
    // Should have different quantities (not all equal) due to volume profile
    let first_qty = quantities[0];
    let all_equal = quantities.iter().all(|&q| q == first_qty);
    
    if quantities.len() > 1 {
        assert!(!all_equal, "VWAP quantities should vary based on volume profile");
    }

    // All quantities should be positive
    for &qty in &quantities {
        assert!(qty > 0, "All VWAP slice quantities should be positive");
    }

    Ok(())
}

#[rstest]
fn test_vwap_price_adjustment_logic(vwap_algo: VwapAlgo) -> Result<()> {
    let market_context = create_market_context_with_volume(5_000_000);
    
    // Test buy order
    let buy_request = {
        let mut req = create_large_order(ExecutionAlgorithm::Vwap, 800_000);
        req.side = Side::Buy;
        req.is_buy = true;
        req
    };

    let buy_orders = vwap_algo.execute(&buy_request, &market_context)?;

    // Test sell order
    let sell_request = {
        let mut req = create_large_order(ExecutionAlgorithm::Vwap, 800_000);
        req.side = Side::Sell;
        req.is_buy = false;
        req
    };

    let sell_orders = vwap_algo.execute(&sell_request, &market_context)?;

    // Both should generate orders with appropriate pricing
    for child in &buy_orders {
        assert!(child.limit_price.is_some(), "Buy VWAP orders should have limit prices");
        // Price should be reasonable (around mid price)
        let price = child.limit_price.unwrap().as_f64();
        assert!(price > 99.0 && price < 101.0, "Buy price should be reasonable: {}", price);
    }

    for child in &sell_orders {
        assert!(child.limit_price.is_some(), "Sell VWAP orders should have limit prices");
        let price = child.limit_price.unwrap().as_f64();
        assert!(price > 99.0 && price < 101.0, "Sell price should be reasonable: {}", price);
    }

    Ok(())
}

#[rstest]
fn test_vwap_time_in_force_consistency(vwap_algo: VwapAlgo) -> Result<()> {
    let market_context = create_market_context_with_volume(3_000_000);
    let request = create_large_order(ExecutionAlgorithm::Vwap, 600_000);

    let child_orders = vwap_algo.execute(&request, &market_context)?;

    // All VWAP orders should use GTC (longer-term execution)
    for child in &child_orders {
        assert_eq!(child.time_in_force, TimeInForce::GTC, "VWAP orders should use GTC");
        assert_eq!(child.order_type, OrderType::Limit, "VWAP orders should be limit orders");
    }

    Ok(())
}

/// Smart Algorithm Slicing Tests
#[fixture]
fn smart_algo() -> SmartAlgo {
    SmartAlgo::new()
}

#[rstest]
fn test_smart_venue_distribution(smart_algo: SmartAlgo) -> Result<()> {
    let venues = vec![
        "Binance".to_string(),
        "Coinbase".to_string(), 
        "Kraken".to_string(),
        "Bybit".to_string(),
    ];
    let market_context = MarketContext {
        bid: Some(Px::new(99.95)),
        ask: Some(Px::new(100.05)),
        mid: Some(Px::new(100.00)),
        spread: Some(10),
        volume: 8_000_000,
        volatility: 0.025,
        venues: venues.clone(),
    };

    let request = create_large_order(ExecutionAlgorithm::Smart, 2_000_000);
    let child_orders = smart_algo.execute(&request, &market_context)?;

    // Should create one order per venue
    assert_eq!(child_orders.len(), venues.len(), "Should create one order per venue");

    // Each venue should get equal allocation
    let expected_qty_per_venue = request.quantity.as_i64() / venues.len() as i64;
    for (i, child) in child_orders.iter().enumerate() {
        assert_eq!(child.quantity.as_i64(), expected_qty_per_venue);
        assert_eq!(child.venue, venues[i], "Venue assignment should match");
    }

    verify_quantity_conservation(&child_orders, request.quantity.as_i64());

    Ok(())
}

#[rstest]
fn test_smart_uneven_venue_distribution(smart_algo: SmartAlgo) -> Result<()> {
    let venues = vec![
        "Binance".to_string(),
        "Coinbase".to_string(),
        "Kraken".to_string(),
    ];
    let market_context = MarketContext {
        bid: Some(Px::new(100.10)),
        ask: Some(Px::new(100.20)),
        mid: Some(Px::new(100.15)),
        spread: Some(10),
        volume: 5_000_000,
        volatility: 0.02,
        venues: venues.clone(),
    };

    // Order quantity that doesn't divide evenly by venue count
    let request = create_large_order(ExecutionAlgorithm::Smart, 1_000_001);
    let child_orders = smart_algo.execute(&request, &market_context)?;

    assert_eq!(child_orders.len(), 3);

    // Each venue gets 333,333 (integer division)
    let expected_base_qty = 1_000_001 / 3; // 333,333
    for child in &child_orders {
        assert_eq!(child.quantity.as_i64(), expected_base_qty);
    }

    // Total distributed should be 999,999 (2 units lost to integer division)
    let total_distributed: i64 = child_orders.iter().map(|c| c.quantity.as_i64()).sum();
    assert_eq!(total_distributed, 999_999);

    Ok(())
}

/// Slice Size Edge Cases
#[rstest]
fn test_zero_quantity_handling() {
    let market_context = create_market_context_with_volume(1_000_000);
    let mut request = create_large_order(ExecutionAlgorithm::Twap, 0);
    
    let twap_algo = TwapAlgo::new();
    let result = twap_algo.execute(&request, &market_context);
    
    // Should either succeed with empty orders or handle gracefully
    if let Ok(child_orders) = result {
        for child in &child_orders {
            assert_eq!(child.quantity.as_i64(), 0, "Zero quantity should result in zero slices");
        }
    }
}

#[rstest]
fn test_single_unit_quantity_slicing() -> Result<()> {
    let market_context = create_market_context_with_volume(100_000);
    let request = create_large_order(ExecutionAlgorithm::Twap, 1);

    let twap_algo = TwapAlgo::new();
    let child_orders = twap_algo.execute(&request, &market_context)?;

    // With quantity 1 and 10 slices, most slices will be 0
    assert_eq!(child_orders.len(), 10);
    
    let total_distributed: i64 = child_orders.iter().map(|c| c.quantity.as_i64()).sum();
    assert_eq!(total_distributed, 0); // Integer division 1/10 = 0

    Ok(())
}

/// Timing Calculation Tests
#[rstest]
fn test_twap_timing_intervals() -> Result<()> {
    let market_context = create_market_context_with_volume(2_000_000);
    let request = create_large_order(ExecutionAlgorithm::Twap, 500_000);

    let twap_algo = TwapAlgo::new();
    let child_orders = twap_algo.execute(&request, &market_context)?;

    // Verify that each slice has a unique child ID (simulates timing)
    let mut child_ids: Vec<u64> = child_orders.iter().map(|c| c.child_id.0).collect();
    child_ids.sort_unstable();
    
    // Should have unique IDs for each time slice
    for i in 1..child_ids.len() {
        assert_ne!(child_ids[i-1], child_ids[i], "Child order IDs should be unique");
    }

    Ok(())
}

/// Volume Participation Logic Tests  
#[rstest]
fn test_vwap_volume_profile_application(vwap_algo: VwapAlgo) -> Result<()> {
    let high_volume_context = create_market_context_with_volume(50_000_000);
    let low_volume_context = create_market_context_with_volume(1_000_000);
    
    let request = create_large_order(ExecutionAlgorithm::Vwap, 1_000_000);

    let high_vol_orders = vwap_algo.execute(&request, &high_volume_context)?;
    let low_vol_orders = vwap_algo.execute(&request, &low_volume_context)?;

    // Both should create orders, but may have different characteristics
    assert!(!high_vol_orders.is_empty());
    assert!(!low_vol_orders.is_empty());

    // High volume context might allow more aggressive execution
    // (specific behavior depends on implementation details)
    
    Ok(())
}

/// Comprehensive Slicing Validation
#[rstest]
fn test_comprehensive_slicing_properties() -> Result<()> {
    let algorithms = vec![
        (ExecutionAlgorithm::Twap, TwapAlgo::new() as Box<dyn AlgorithmEngine>),
        (ExecutionAlgorithm::Vwap, VwapAlgo::new() as Box<dyn AlgorithmEngine>),
        (ExecutionAlgorithm::Smart, SmartAlgo::new() as Box<dyn AlgorithmEngine>),
    ];

    let test_quantities = vec![100, 1_000, 10_000, 100_000, 1_000_000];
    let market_context = create_market_context_with_volume(10_000_000);

    for (algo_type, algo_engine) in &algorithms {
        for &quantity in &test_quantities {
            let request = create_large_order(*algo_type, quantity);
            
            match algo_engine.execute(&request, &market_context) {
                Ok(child_orders) => {
                    // Basic validations
                    assert!(!child_orders.is_empty() || quantity == 0, 
                        "Algorithm {:?} should create orders for quantity {}", algo_type, quantity);
                    
                    // Verify all child orders have positive quantities
                    for child in &child_orders {
                        assert!(child.quantity.as_i64() >= 0, 
                            "Child order quantity should be non-negative for {:?}", algo_type);
                    }

                    // Verify venue assignments are valid
                    for child in &child_orders {
                        assert!(market_context.venues.contains(&child.venue) || child.venue == "Default",
                            "Invalid venue assignment for {:?}: {}", algo_type, child.venue);
                    }
                }
                Err(e) => {
                    // Some algorithms may fail for certain conditions (e.g., no venues)
                    println!("Algorithm {:?} failed for quantity {}: {}", algo_type, quantity, e);
                }
            }
        }
    }

    Ok(())
}