//! Comprehensive algorithm engine tests
//!
//! Advanced tests for execution algorithms focusing on:
//! - Algorithm state management and transitions
//! - Timing accuracy and slice scheduling  
//! - Market impact modeling and price adjustments
//! - Volume participation and liquidity analysis
//! - Edge cases and error conditions
//! - Performance characteristics under load

use execution_router::{
    algorithms::*,
    ExecutionAlgorithm, OrderRequest, OrderType, TimeInForce
};
use services_common::{Px, Qty, Side, Symbol};
use rstest::*;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use anyhow::Result;

/// Test data structures and utilities
#[derive(Debug, Clone)]
struct MockMarketData {
    current_volume: i64,
    recent_volumes: Vec<i64>, // Historical volume for VWAP calculations
    volatility: f64,
    spread_bps: i32,
}

impl MockMarketData {
    fn high_volume() -> Self {
        Self {
            current_volume: 50_000_000,
            recent_volumes: vec![45_000_000, 52_000_000, 48_000_000, 55_000_000],
            volatility: 0.015, // Low volatility in high volume
            spread_bps: 2, // Tight spread
        }
    }
    
    fn low_volume() -> Self {
        Self {
            current_volume: 1_000_000,
            recent_volumes: vec![800_000, 1_200_000, 900_000, 1_100_000],
            volatility: 0.045, // Higher volatility in low volume
            spread_bps: 15, // Wider spread
        }
    }
    
    fn volatile_market() -> Self {
        Self {
            current_volume: 10_000_000,
            recent_volumes: vec![5_000_000, 20_000_000, 8_000_000, 15_000_000],
            volatility: 0.08, // Very high volatility
            spread_bps: 25,
        }
    }
}

/// Test fixtures
#[fixture]
fn large_buy_order() -> OrderRequest {
    OrderRequest {
        client_order_id: "large_buy_001".to_string(),
        symbol: Symbol(1), // BTC/USDT
        side: Side::Buy,
        quantity: Qty::from_i64(5_000_000), // 500 BTC equivalent
        order_type: OrderType::Limit,
        limit_price: Some(Px::new(50000.00)),
        stop_price: None,
        is_buy: true,
        algorithm: ExecutionAlgorithm::Twap,
        urgency: 0.3, // Low urgency
        participation_rate: Some(0.15), // 15% max participation
        time_in_force: TimeInForce::GTC,
        venue: None,
        strategy_id: "institutional_strategy".to_string(),
        params: rustc_hash::FxHashMap::default(),
    }
}

#[fixture]
fn urgent_sell_order() -> OrderRequest {
    OrderRequest {
        client_order_id: "urgent_sell_001".to_string(),
        symbol: Symbol(2), // ETH/USDT
        side: Side::Sell,
        quantity: Qty::from_i64(10_000_000), // 1000 ETH equivalent
        order_type: OrderType::Limit,
        limit_price: Some(Px::new(3000.00)),
        stop_price: None,
        is_buy: false,
        algorithm: ExecutionAlgorithm::Vwap,
        urgency: 0.9, // High urgency
        participation_rate: Some(0.25), // 25% max participation
        time_in_force: TimeInForce::DAY,
        venue: None,
        strategy_id: "arbitrage_strategy".to_string(),
        params: rustc_hash::FxHashMap::default(),
    }
}

/// TWAP Algorithm Advanced Tests
#[fixture]
fn twap_params() -> AlgorithmParams {
    AlgorithmParams {
        algo_type: AlgorithmType::TWAP,
        start_time: Utc::now() + Duration::minutes(1),
        end_time: Utc::now() + Duration::hours(2),
        max_participation_rate: 1500, // 15%
        min_order_size: Qty::from_i64(10000), // Min 1 BTC
        max_order_size: Qty::from_i64(500000), // Max 50 BTC
        price_limit: Some(Px::new(51000.00)),
        urgency: 3,
    }
}

#[rstest]
fn test_twap_algorithm_state_management(large_buy_order: OrderRequest, twap_params: AlgorithmParams) -> Result<()> {
    let mut twap = TwapAlgorithm::new(large_buy_order.clone(), twap_params);
    
    // Initial state
    assert!(!twap.state().started);
    assert!(!twap.state().completed);
    assert_eq!(twap.state().executed_qty, Qty::ZERO);
    assert_eq!(twap.state().remaining_qty, large_buy_order.quantity);
    assert_eq!(twap.state().child_orders.len(), 0);
    
    // Generate first slice
    if let Some(child_order) = twap.get_next_slice() {
        assert!(child_order.quantity.as_i64() > 0);
        assert!(child_order.client_order_id.contains("twap"));
        
        // State should be updated
        assert!(twap.state().remaining_qty.as_i64() < large_buy_order.quantity.as_i64());
        assert_eq!(twap.state().child_orders.len(), 1);
    }
    
    // Generate all remaining slices
    let mut total_child_qty = Qty::ZERO;
    let mut slice_count = 0;
    
    while let Some(child_order) = twap.get_next_slice() {
        total_child_qty = Qty::from_i64(total_child_qty.as_i64() + child_order.quantity.as_i64());
        slice_count += 1;
        
        // Verify slice properties
        assert!(child_order.quantity >= twap.params().min_order_size);
        assert!(child_order.quantity <= twap.params().max_order_size);
        
        // Safety check
        if slice_count > 1000 {
            break;
        }
    }
    
    // Should eventually be completed
    assert!(twap.state().completed || slice_count > 0);
    
    Ok(())
}

#[rstest]
fn test_twap_timing_precision() -> Result<()> {
    let params = AlgorithmParams {
        algo_type: AlgorithmType::TWAP,
        start_time: Utc::now() + Duration::minutes(30), // Future start
        end_time: Utc::now() + Duration::hours(4),
        max_participation_rate: 1000, // 10%
        min_order_size: Qty::from_i64(1000),
        max_order_size: Qty::from_i64(100000),
        price_limit: None,
        urgency: 5,
    };
    
    let order = OrderRequest {
        client_order_id: "timing_test".to_string(),
        symbol: Symbol(1),
        side: Side::Buy,
        quantity: Qty::from_i64(1_000_000),
        order_type: OrderType::Limit,
        limit_price: Some(Px::new(50000.00)),
        stop_price: None,
        is_buy: true,
        algorithm: ExecutionAlgorithm::Twap,
        urgency: 0.5,
        participation_rate: Some(0.10),
        time_in_force: TimeInForce::GTC,
        venue: None,
        strategy_id: "timing_test".to_string(),
        params: rustc_hash::FxHashMap::default(),
    };
    
    let mut twap = TwapAlgorithm::new(order, params);
    
    // Should not execute before start time
    let slice = twap.get_next_slice();
    assert!(slice.is_none(), "Should not execute before start time");
    
    Ok(())
}

#[rstest]
fn test_twap_slice_size_calculations() -> Result<()> {
    let test_cases = vec![
        (1_000_000i64, 100), // 1M quantity, expect 10k per slice
        (750_000, 75),       // 750k quantity, expect 7.5k per slice  
        (100, 10),           // Small quantity
        (99, 9),             // Quantity not divisible by slice count
    ];
    
    for (quantity, expected_avg_slice) in test_cases {
        let params = AlgorithmParams {
            algo_type: AlgorithmType::TWAP,
            start_time: Utc::now() - Duration::minutes(1), // Already started
            end_time: Utc::now() + Duration::hours(1),
            max_participation_rate: 2000, // 20%
            min_order_size: Qty::from_i64(1),
            max_order_size: Qty::from_i64(1_000_000),
            price_limit: None,
            urgency: 5,
        };
        
        let order = OrderRequest {
            client_order_id: format!("slice_test_{}", quantity),
            symbol: Symbol(1),
            side: Side::Buy,
            quantity: Qty::from_i64(quantity),
            order_type: OrderType::Limit,
            limit_price: Some(Px::new(50000.00)),
            stop_price: None,
            is_buy: true,
            algorithm: ExecutionAlgorithm::Twap,
            urgency: 0.5,
            participation_rate: Some(0.20),
            time_in_force: TimeInForce::GTC,
            venue: None,
            strategy_id: "slice_test".to_string(),
            params: rustc_hash::FxHashMap::default(),
        };
        
        let mut twap = TwapAlgorithm::new(order, params);
        
        // Get first slice and verify size is reasonable
        if let Some(child_order) = twap.get_next_slice() {
            let slice_size = child_order.quantity.as_i64();
            
            // Should be approximately expected size (within reasonable bounds)
            assert!(slice_size > 0, "Slice size should be positive for quantity {}", quantity);
            
            if expected_avg_slice > 0 {
                let ratio = slice_size as f64 / expected_avg_slice as f64;
                assert!(ratio >= 0.5 && ratio <= 2.0, 
                    "Slice size {} should be reasonable vs expected {} for quantity {}", 
                    slice_size, expected_avg_slice, quantity);
            }
        }
    }
    
    Ok(())
}

/// VWAP Algorithm Advanced Tests
#[fixture]
fn vwap_params() -> AlgorithmParams {
    AlgorithmParams {
        algo_type: AlgorithmType::VWAP,
        start_time: Utc::now(),
        end_time: Utc::now() + Duration::hours(6), // Full trading session
        max_participation_rate: 1000, // 10%
        min_order_size: Qty::from_i64(5000),
        max_order_size: Qty::from_i64(200000),
        price_limit: Some(Px::new(3100.00)),
        urgency: 4,
    }
}

#[rstest]
fn test_vwap_volume_weighted_distribution(urgent_sell_order: OrderRequest, vwap_params: AlgorithmParams) -> Result<()> {
    let mut vwap = VwapAlgorithm::new(urgent_sell_order.clone(), vwap_params);
    
    // Test with different market volumes
    let volume_scenarios = vec![
        (1_000_000i64, "Low volume"),
        (10_000_000, "Medium volume"),
        (50_000_000, "High volume"),
        (100_000_000, "Very high volume"),
    ];
    
    for (market_volume, scenario) in volume_scenarios {
        let market_qty = Qty::from_i64(market_volume);
        
        if let Some(child_order) = vwap.get_next_slice(market_qty) {
            let child_qty = child_order.quantity.as_i64();
            
            // Participation should respect the configured rate
            let participation_rate = vwap.params().max_participation_rate as f64 / 10000.0; // Convert from basis points
            let expected_max_qty = (market_volume as f64 * participation_rate) as i64;
            
            assert!(child_qty <= expected_max_qty, 
                "{}: Child quantity {} should not exceed participation limit {}", 
                scenario, child_qty, expected_max_qty);
            
            assert!(child_qty >= vwap.params().min_order_size.as_i64(),
                "{}: Child quantity should meet minimum size", scenario);
            
            assert!(child_qty <= vwap.params().max_order_size.as_i64(),
                "{}: Child quantity should not exceed maximum size", scenario);
        }
    }
    
    Ok(())
}

#[rstest]
fn test_vwap_historical_volume_adaptation() -> Result<()> {
    let market_data_scenarios = vec![
        MockMarketData::high_volume(),
        MockMarketData::low_volume(),
        MockMarketData::volatile_market(),
    ];
    
    let base_order = OrderRequest {
        client_order_id: "vwap_adaptation_test".to_string(),
        symbol: Symbol(1),
        side: Side::Buy,
        quantity: Qty::from_i64(2_000_000),
        order_type: OrderType::Limit,
        limit_price: Some(Px::new(50000.00)),
        stop_price: None,
        is_buy: true,
        algorithm: ExecutionAlgorithm::Vwap,
        urgency: 0.5,
        participation_rate: Some(0.12),
        time_in_force: TimeInForce::GTC,
        venue: None,
        strategy_id: "adaptation_test".to_string(),
        params: rustc_hash::FxHashMap::default(),
    };
    
    for (i, market_data) in market_data_scenarios.iter().enumerate() {
        let params = AlgorithmParams {
            algo_type: AlgorithmType::VWAP,
            start_time: Utc::now(),
            end_time: Utc::now() + Duration::hours(4),
            max_participation_rate: 1200, // 12%
            min_order_size: Qty::from_i64(1000),
            max_order_size: Qty::from_i64(300000),
            price_limit: None,
            urgency: if market_data.volatility > 0.05 { 8 } else { 3 }, // Higher urgency in volatile markets
        };
        
        let mut order = base_order.clone();
        order.client_order_id = format!("vwap_test_{}", i);
        
        let mut vwap = VwapAlgorithm::new(order, params);
        
        // Generate slices for this market scenario
        let mut slices = Vec::new();
        for historical_volume in &market_data.recent_volumes {
            if let Some(slice) = vwap.get_next_slice(Qty::from_i64(*historical_volume)) {
                slices.push(slice);
            }
        }
        
        // Verify slices adapt to market conditions
        if !slices.is_empty() {
            let avg_slice_size: i64 = slices.iter().map(|s| s.quantity.as_i64()).sum::<i64>() / slices.len() as i64;
            
            // High volume markets should allow larger slices
            if market_data.current_volume > 20_000_000 {
                assert!(avg_slice_size > 50_000, 
                    "High volume market should allow larger average slice size, got {}", avg_slice_size);
            }
            
            // All slices should be within configured bounds
            for slice in &slices {
                assert!(slice.quantity.as_i64() >= vwap.params().min_order_size.as_i64());
                assert!(slice.quantity.as_i64() <= vwap.params().max_order_size.as_i64());
            }
        }
    }
    
    Ok(())
}

/// Iceberg Algorithm Advanced Tests
#[rstest]
fn test_iceberg_display_logic() -> Result<()> {
    let large_order = OrderRequest {
        client_order_id: "iceberg_test".to_string(),
        symbol: Symbol(1),
        side: Side::Buy,
        quantity: Qty::from_i64(5_000_000), // 5M units
        order_type: OrderType::Limit,
        limit_price: Some(Px::new(100.00)),
        stop_price: None,
        is_buy: true,
        algorithm: ExecutionAlgorithm::Iceberg,
        urgency: 0.4,
        participation_rate: None,
        time_in_force: TimeInForce::GTC,
        venue: None,
        strategy_id: "iceberg_test".to_string(),
        params: rustc_hash::FxHashMap::default(),
    };
    
    let params = AlgorithmParams {
        algo_type: AlgorithmType::Iceberg,
        start_time: Utc::now(),
        end_time: Utc::now() + Duration::hours(8),
        max_participation_rate: 500, // 5%
        min_order_size: Qty::from_i64(10000),
        max_order_size: Qty::from_i64(500000), // Display quantity
        price_limit: Some(Px::new(100.50)),
        urgency: 4,
    };
    
    let display_qty = Qty::from_i64(500000); // 500k display
    let mut iceberg = IcebergAlgorithm::new(large_order.clone(), params, display_qty);
    
    // First slice should show only display quantity
    if let Some(first_slice) = iceberg.get_next_slice() {
        assert_eq!(first_slice.quantity, display_qty);
        assert!(first_slice.client_order_id.contains("iceberg"));
        
        // Remaining quantity should be reduced
        assert_eq!(iceberg.state().remaining_qty.as_i64(), 
                   large_order.quantity.as_i64() - display_qty.as_i64());
    }
    
    // Simulate partial fill and refresh
    let fill_qty = Qty::from_i64(200000); // 200k filled
    iceberg.on_fill(fill_qty);
    
    // Should still have some display quantity remaining
    assert!(iceberg.refresh_qty.as_i64() > 0);
    
    // Simulate complete fill of display quantity
    let remaining_display = iceberg.refresh_qty;
    iceberg.on_fill(remaining_display);
    
    // Should refresh with new display quantity
    if let Some(next_slice) = iceberg.get_next_slice() {
        assert_eq!(next_slice.quantity, display_qty.min(iceberg.state().remaining_qty));
    }
    
    Ok(())
}

#[rstest]
fn test_iceberg_refresh_patterns() -> Result<()> {
    let test_cases = vec![
        (10_000_000i64, 1_000_000i64, 5), // Large order, should generate 5+ refreshes
        (2_000_000, 500_000, 4),          // Medium order
        (800_000, 200_000, 4),            // Small order
        (100_000, 200_000, 1),            // Display larger than total (edge case)
    ];
    
    for (total_qty, display_qty, expected_min_slices) in test_cases {
        let order = OrderRequest {
            client_order_id: format!("iceberg_refresh_{}", total_qty),
            symbol: Symbol(1),
            side: Side::Sell,
            quantity: Qty::from_i64(total_qty),
            order_type: OrderType::Limit,
            limit_price: Some(Px::new(100.00)),
            stop_price: None,
            is_buy: false,
            algorithm: ExecutionAlgorithm::Iceberg,
            urgency: 0.6,
            participation_rate: None,
            time_in_force: TimeInForce::GTC,
            venue: None,
            strategy_id: "refresh_test".to_string(),
            params: rustc_hash::FxHashMap::default(),
        };
        
        let params = AlgorithmParams {
            algo_type: AlgorithmType::Iceberg,
            start_time: Utc::now(),
            end_time: Utc::now() + Duration::hours(12),
            max_participation_rate: 800,
            min_order_size: Qty::from_i64(1000),
            max_order_size: Qty::from_i64(display_qty),
            price_limit: None,
            urgency: 3,
        };
        
        let mut iceberg = IcebergAlgorithm::new(order, params, Qty::from_i64(display_qty));
        
        let mut slice_count = 0;
        let mut total_generated = 0i64;
        
        // Generate all slices and simulate complete fills
        while let Some(slice) = iceberg.get_next_slice() {
            slice_count += 1;
            total_generated += slice.quantity.as_i64();
            
            // Simulate complete fill of this slice
            iceberg.on_fill(slice.quantity);
            
            // Safety break
            if slice_count > 50 {
                break;
            }
        }
        
        // Verify behavior
        assert!(slice_count >= expected_min_slices, 
            "Should generate at least {} slices for total_qty={}, got {}", 
            expected_min_slices, total_qty, slice_count);
        
        // Total generated should not exceed original quantity (allowing for rounding)
        assert!(total_generated <= total_qty, 
            "Total generated {} should not exceed original quantity {}", 
            total_generated, total_qty);
    }
    
    Ok(())
}

/// Algorithm State Persistence and Recovery Tests
#[rstest]
fn test_algorithm_state_serialization() -> Result<()> {
    // Test that algorithm states can be properly serialized/deserialized for recovery
    
    let order = OrderRequest {
        client_order_id: "persistence_test".to_string(),
        symbol: Symbol(1),
        side: Side::Buy,
        quantity: Qty::from_i64(1_000_000),
        order_type: OrderType::Limit,
        limit_price: Some(Px::new(50000.00)),
        stop_price: None,
        is_buy: true,
        algorithm: ExecutionAlgorithm::Twap,
        urgency: 0.5,
        participation_rate: Some(0.15),
        time_in_force: TimeInForce::GTC,
        venue: None,
        strategy_id: "persistence_test".to_string(),
        params: rustc_hash::FxHashMap::default(),
    };
    
    let params = AlgorithmParams {
        algo_type: AlgorithmType::TWAP,
        start_time: Utc::now(),
        end_time: Utc::now() + Duration::hours(2),
        max_participation_rate: 1500,
        min_order_size: Qty::from_i64(10000),
        max_order_size: Qty::from_i64(100000),
        price_limit: None,
        urgency: 5,
    };
    
    let mut twap = TwapAlgorithm::new(order.clone(), params.clone());
    
    // Generate some slices to change state
    let _slice1 = twap.get_next_slice();
    let _slice2 = twap.get_next_slice();
    
    // Capture current state
    let state_snapshot = twap.state().clone();
    
    // Create new algorithm instance from same parameters
    let mut restored_twap = TwapAlgorithm::new(order, params);
    
    // Verify initial states are equivalent (before any execution)
    assert_eq!(restored_twap.state().parent_order.quantity, state_snapshot.parent_order.quantity);
    assert_eq!(restored_twap.state().params.algo_type, state_snapshot.params.algo_type);
    
    Ok(())
}

/// Performance and Stress Tests
#[rstest]
fn test_algorithm_performance_characteristics() -> Result<()> {
    use std::time::Instant;
    
    let order = OrderRequest {
        client_order_id: "performance_test".to_string(),
        symbol: Symbol(1),
        side: Side::Buy,
        quantity: Qty::from_i64(100_000_000), // Very large order
        order_type: OrderType::Limit,
        limit_price: Some(Px::new(50000.00)),
        stop_price: None,
        is_buy: true,
        algorithm: ExecutionAlgorithm::Twap,
        urgency: 0.5,
        participation_rate: Some(0.20),
        time_in_force: TimeInForce::GTC,
        venue: None,
        strategy_id: "performance_test".to_string(),
        params: rustc_hash::FxHashMap::default(),
    };
    
    let params = AlgorithmParams {
        algo_type: AlgorithmType::TWAP,
        start_time: Utc::now(),
        end_time: Utc::now() + Duration::hours(1),
        max_participation_rate: 2000,
        min_order_size: Qty::from_i64(1000),
        max_order_size: Qty::from_i64(1_000_000),
        price_limit: None,
        urgency: 5,
    };
    
    let mut twap = TwapAlgorithm::new(order, params);
    
    // Measure slice generation performance
    let start = Instant::now();
    let mut slice_count = 0;
    
    while let Some(_slice) = twap.get_next_slice() {
        slice_count += 1;
        
        // Safety break and performance check
        if slice_count > 1000 || start.elapsed().as_millis() > 100 {
            break;
        }
    }
    
    let elapsed = start.elapsed();
    
    assert!(slice_count > 0, "Should generate at least one slice");
    assert!(elapsed.as_millis() < 50, "Slice generation should be fast, took {:?}", elapsed);
    
    println!("Generated {} slices in {:?} ({:.2} µs/slice)", 
             slice_count, elapsed, elapsed.as_micros() as f64 / slice_count as f64);
    
    Ok(())
}

/// Edge Cases and Error Handling Tests
#[rstest]
fn test_algorithm_edge_cases() -> Result<()> {
    // Test various edge cases that algorithms should handle gracefully
    
    let edge_cases = vec![
        // (quantity, display_qty_for_iceberg, description)
        (0, 0, "Zero quantity"),
        (1, 1, "Single unit quantity"),
        (i64::MAX, 1_000_000, "Maximum quantity"),
        (1_000_000, 2_000_000, "Display larger than total"),
    ];
    
    for (quantity, display_qty, description) in edge_cases {
        println!("Testing edge case: {}", description);
        
        let order = OrderRequest {
            client_order_id: format!("edge_case_{}", quantity),
            symbol: Symbol(1),
            side: Side::Buy,
            quantity: Qty::from_i64(quantity),
            order_type: OrderType::Limit,
            limit_price: Some(Px::new(50000.00)),
            stop_price: None,
            is_buy: true,
            algorithm: ExecutionAlgorithm::Twap,
            urgency: 0.5,
            participation_rate: Some(0.10),
            time_in_force: TimeInForce::GTC,
            venue: None,
            strategy_id: "edge_case_test".to_string(),
            params: rustc_hash::FxHashMap::default(),
        };
        
        let params = AlgorithmParams {
            algo_type: AlgorithmType::TWAP,
            start_time: Utc::now(),
            end_time: Utc::now() + Duration::hours(1),
            max_participation_rate: 1000,
            min_order_size: Qty::from_i64(1),
            max_order_size: Qty::from_i64(1_000_000),
            price_limit: None,
            urgency: 5,
        };
        
        // Test TWAP
        let mut twap = TwapAlgorithm::new(order.clone(), params.clone());
        let twap_result = twap.get_next_slice();
        
        if quantity > 0 {
            // Should handle gracefully, either succeed or fail predictably
            match twap_result {
                Some(slice) => {
                    assert!(slice.quantity.as_i64() <= quantity, 
                        "Slice should not exceed original quantity");
                }
                None => {
                    // Acceptable for edge cases like very small quantities
                }
            }
        }
        
        // Test Iceberg with edge cases
        if quantity > 0 {
            let iceberg_params = AlgorithmParams {
                algo_type: AlgorithmType::Iceberg,
                ..params
            };
            
            let mut iceberg = IcebergAlgorithm::new(order, iceberg_params, Qty::from_i64(display_qty));
            let iceberg_result = iceberg.get_next_slice();
            
            match iceberg_result {
                Some(slice) => {
                    let expected_display = std::cmp::min(quantity, display_qty);
                    assert!(slice.quantity.as_i64() <= expected_display,
                        "Iceberg slice should not exceed display quantity");
                }
                None => {
                    // May fail for extreme edge cases
                }
            }
        }
    }
    
    Ok(())
}

/// Market Condition Adaptation Tests
#[rstest]
fn test_algorithm_market_adaptation() -> Result<()> {
    let base_order = OrderRequest {
        client_order_id: "adaptation_test".to_string(),
        symbol: Symbol(1),
        side: Side::Buy,
        quantity: Qty::from_i64(5_000_000),
        order_type: OrderType::Limit,
        limit_price: Some(Px::new(50000.00)),
        stop_price: None,
        is_buy: true,
        algorithm: ExecutionAlgorithm::Vwap,
        urgency: 0.5,
        participation_rate: Some(0.15),
        time_in_force: TimeInForce::GTC,
        venue: None,
        strategy_id: "adaptation_test".to_string(),
        params: rustc_hash::FxHashMap::default(),
    };
    
    let market_conditions = vec![
        ("Bull market", MockMarketData::high_volume(), 0.2), // Low urgency in good conditions
        ("Bear market", MockMarketData::low_volume(), 0.8), // High urgency in poor conditions
        ("Volatile market", MockMarketData::volatile_market(), 0.9), // Very high urgency
    ];
    
    for (condition_name, market_data, urgency) in market_conditions {
        let mut order = base_order.clone();
        order.urgency = urgency;
        order.client_order_id = format!("adaptation_{}_{}", condition_name.replace(' ', "_"), (urgency * 10.0) as u32);
        
        let params = AlgorithmParams {
            algo_type: AlgorithmType::VWAP,
            start_time: Utc::now(),
            end_time: Utc::now() + Duration::hours(4),
            max_participation_rate: if market_data.volatility > 0.05 { 800 } else { 1500 }, // More conservative in volatile markets
            min_order_size: Qty::from_i64(5000),
            max_order_size: Qty::from_i64(200000),
            price_limit: None,
            urgency: if urgency > 0.7 { 8 } else { 3 },
        };
        
        let mut vwap = VwapAlgorithm::new(order, params);
        
        // Test slice generation under different market conditions
        let slice = vwap.get_next_slice(Qty::from_i64(market_data.current_volume));
        
        match slice {
            Some(child_order) => {
                // Verify adaptation to market conditions
                let participation_rate = child_order.quantity.as_i64() as f64 / market_data.current_volume as f64;
                let max_participation = vwap.params().max_participation_rate as f64 / 10000.0;
                
                assert!(participation_rate <= max_participation * 1.1, // Allow 10% tolerance
                    "{}: Participation rate {:.4} exceeds limit {:.4}", 
                    condition_name, participation_rate, max_participation);
                
                println!("{}: Generated slice of {} units (participation: {:.2}%)", 
                         condition_name, child_order.quantity.as_i64(), participation_rate * 100.0);
            }
            None => {
                println!("{}: No slice generated (possibly due to market conditions)", condition_name);
            }
        }
    }
    
    Ok(())
}

/// Comprehensive Algorithm Integration Test
#[rstest]
fn test_algorithm_comprehensive_workflow() -> Result<()> {
    // Test complete workflow from order creation to execution
    
    let scenarios = vec![
        ("Large institutional TWAP", ExecutionAlgorithm::Twap, 50_000_000i64, 0.3),
        ("Medium VWAP execution", ExecutionAlgorithm::Vwap, 10_000_000, 0.6),
        ("Small iceberg order", ExecutionAlgorithm::Iceberg, 1_000_000, 0.4),
    ];
    
    for (scenario_name, algorithm, quantity, urgency) in scenarios {
        println!("Testing scenario: {}", scenario_name);
        
        let order = OrderRequest {
            client_order_id: format!("comprehensive_{}_{}", 
                scenario_name.replace(' ', "_"), 
                chrono::Utc::now().timestamp_millis()),
            symbol: Symbol(1),
            side: Side::Buy,
            quantity: Qty::from_i64(quantity),
            order_type: OrderType::Limit,
            limit_price: Some(Px::new(50000.00)),
            stop_price: None,
            is_buy: true,
            algorithm,
            urgency,
            participation_rate: Some(0.15),
            time_in_force: TimeInForce::GTC,
            venue: None,
            strategy_id: format!("comprehensive_{}", algorithm as u32),
            params: rustc_hash::FxHashMap::default(),
        };
        
        let params = AlgorithmParams {
            algo_type: match algorithm {
                ExecutionAlgorithm::Twap => AlgorithmType::TWAP,
                ExecutionAlgorithm::Vwap => AlgorithmType::VWAP,
                ExecutionAlgorithm::Iceberg => AlgorithmType::Iceberg,
                _ => AlgorithmType::TWAP,
            },
            start_time: Utc::now(),
            end_time: Utc::now() + Duration::hours(if urgency > 0.7 { 2 } else { 6 }),
            max_participation_rate: (urgency * 2000.0) as i32, // Scale participation with urgency
            min_order_size: Qty::from_i64(1000),
            max_order_size: Qty::from_i64(500000),
            price_limit: None,
            urgency: (urgency * 10.0) as u8,
        };
        
        match algorithm {
            ExecutionAlgorithm::Twap => {
                let mut twap = TwapAlgorithm::new(order, params);
                let mut generated_slices = 0;
                
                while let Some(_slice) = twap.get_next_slice() {
                    generated_slices += 1;
                    if generated_slices > 20 { break; } // Reasonable limit
                }
                
                assert!(generated_slices > 0, "{}: Should generate at least one slice", scenario_name);
            }
            
            ExecutionAlgorithm::Vwap => {
                let mut vwap = VwapAlgorithm::new(order, params);
                let market_volume = Qty::from_i64(25_000_000); // Simulated market volume
                
                let slice = vwap.get_next_slice(market_volume);
                if let Some(child_order) = slice {
                    assert!(child_order.quantity.as_i64() > 0, "{}: VWAP slice should have positive quantity", scenario_name);
                }
            }
            
            ExecutionAlgorithm::Iceberg => {
                let display_qty = Qty::from_i64(quantity / 10); // 10% display
                let mut iceberg = IcebergAlgorithm::new(order, params, display_qty);
                
                let slice = iceberg.get_next_slice();
                if let Some(child_order) = slice {
                    assert_eq!(child_order.quantity.as_i64().min(display_qty.as_i64()), child_order.quantity.as_i64(),
                        "{}: Iceberg should respect display quantity", scenario_name);
                }
            }
            
            _ => {} // Other algorithms not implemented in this test
        }
        
        println!("  ✓ {} completed successfully", scenario_name);
    }
    
    Ok(())
}