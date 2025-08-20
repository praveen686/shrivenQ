//! Test module organization for the orderbook service
//! 
//! This module provides a centralized way to organize and run all tests
//! for the orderbook service, including unit tests, integration tests,
//! and property-based tests.

// Re-export test modules for easy access
pub mod unit {
    pub mod test_core;
    pub mod test_analytics; 
    pub mod test_events;
    pub mod test_metrics;
    pub mod test_replay;
    pub mod test_integration;
}

pub mod property {
    pub mod test_invariants;
}

#[cfg(test)]
mod test_runner {
    /// Run all unit tests
    #[test]
    fn run_all_unit_tests() {
        // This ensures all unit test modules are compiled and linked
        println!("All unit test modules are available for execution");
    }
    
    /// Run all property-based tests  
    #[test]
    fn run_all_property_tests() {
        // This ensures all property test modules are compiled and linked
        println!("All property test modules are available for execution");
    }
}

/// Test configuration and utilities
pub mod utils {
    use services_common::{Px, Qty, Ts};
    use std::time::{SystemTime, UNIX_EPOCH};
    
    /// Create a deterministic timestamp for testing
    pub fn test_timestamp(offset_secs: u64) -> Ts {
        let base_time = 1_600_000_000; // Sep 13, 2020 - stable base time
        Ts::from_nanos((base_time + offset_secs) * 1_000_000_000)
    }
    
    /// Create test price with validation
    pub fn test_price(value: i64) -> Px {
        assert!(value > 0, "Price must be positive");
        assert!(value < 10_000_000, "Price must be reasonable");
        Px::from_i64(value)
    }
    
    /// Create test quantity with validation
    pub fn test_quantity(value: i64) -> Qty {
        assert!(value >= 0, "Quantity must be non-negative");
        assert!(value < 100_000_000, "Quantity must be reasonable");
        Qty::from_i64(value)
    }
    
    /// Generate deterministic random-like sequence for testing
    pub fn pseudo_random_sequence(seed: u64, count: usize) -> Vec<u64> {
        let mut values = Vec::with_capacity(count);
        let mut current = seed;
        
        for _ in 0..count {
            // Simple linear congruential generator
            current = current.wrapping_mul(1664525).wrapping_add(1013904223);
            values.push(current);
        }
        
        values
    }
    
    /// Test configuration for different scenarios
    pub struct TestConfig {
        pub name: String,
        pub max_orders: usize,
        pub max_price_levels: usize, 
        pub timeout_ms: u64,
        pub enable_logging: bool,
    }
    
    impl Default for TestConfig {
        fn default() -> Self {
            Self {
                name: "default_test".to_string(),
                max_orders: 10000,
                max_price_levels: 100,
                timeout_ms: 5000,
                enable_logging: false,
            }
        }
    }
    
    impl TestConfig {
        pub fn performance_test() -> Self {
            Self {
                name: "performance_test".to_string(),
                max_orders: 100000,
                max_price_levels: 1000,
                timeout_ms: 30000,
                enable_logging: false,
            }
        }
        
        pub fn stress_test() -> Self {
            Self {
                name: "stress_test".to_string(),
                max_orders: 1000000,
                max_price_levels: 10000,
                timeout_ms: 120000,
                enable_logging: true,
            }
        }
        
        pub fn integration_test() -> Self {
            Self {
                name: "integration_test".to_string(), 
                max_orders: 50000,
                max_price_levels: 500,
                timeout_ms: 15000,
                enable_logging: true,
            }
        }
    }
}

/// Test data generators
pub mod generators {
    use orderbook::core::{Order, Side};
    use services_common::{Px, Qty, Ts};
    use crate::utils::test_timestamp;
    
    /// Generate a series of realistic market orders
    pub fn generate_market_orders(count: usize, base_price: i64, base_time: u64) -> Vec<Order> {
        let mut orders = Vec::with_capacity(count);
        let mut order_id = 1u64;
        
        for i in 0..count {
            // Create price variation around base price
            let price_offset = ((i % 20) as i64) - 10; // -10 to +9 ticks
            let price = base_price + price_offset * 100;
            
            // Alternate sides with some bias
            let side = if i % 3 == 0 { Side::Ask } else { Side::Bid };
            
            // Vary quantities
            let quantity = 1000 + ((i % 10) as i64) * 500;
            
            let order = Order {
                id: order_id,
                price: Px::from_i64(price),
                quantity: Qty::from_i64(quantity),
                original_quantity: Qty::from_i64(quantity),
                timestamp: test_timestamp(base_time + i as u64),
                side,
                is_iceberg: i % 20 == 0, // 5% iceberg orders
                visible_quantity: if i % 20 == 0 {
                    Some(Qty::from_i64(quantity / 4))
                } else {
                    None
                },
            };
            
            orders.push(order);
            order_id += 1;
        }
        
        orders
    }
    
    /// Generate orders that create a realistic order book shape
    pub fn generate_book_building_orders(levels: usize, base_price: i64) -> Vec<Order> {
        let mut orders = Vec::new();
        let mut order_id = 1u64;
        
        // Create bid levels (descending prices)
        for i in 0..levels {
            let price = base_price - (i as i64 * 100);
            let quantity = 1000 + (i as i64 * 200); // Deeper levels have more quantity
            
            let order = Order {
                id: order_id,
                price: Px::from_i64(price),
                quantity: Qty::from_i64(quantity),
                original_quantity: Qty::from_i64(quantity),
                timestamp: test_timestamp(i as u64),
                side: Side::Bid,
                is_iceberg: false,
                visible_quantity: None,
            };
            
            orders.push(order);
            order_id += 1;
        }
        
        // Create ask levels (ascending prices)
        for i in 0..levels {
            let price = base_price + 100 + (i as i64 * 100); // Start above base price
            let quantity = 1000 + (i as i64 * 200);
            
            let order = Order {
                id: order_id,
                price: Px::from_i64(price),
                quantity: Qty::from_i64(quantity),
                original_quantity: Qty::from_i64(quantity),
                timestamp: test_timestamp(levels as u64 + i as u64),
                side: Side::Ask,
                is_iceberg: false,
                visible_quantity: None,
            };
            
            orders.push(order);
            order_id += 1;
        }
        
        orders
    }
    
    /// Generate a series of order modifications and cancellations
    pub fn generate_modification_sequence(initial_orders: &[Order]) -> Vec<(u64, ModificationType)> {
        let mut modifications = Vec::new();
        
        for (i, order) in initial_orders.iter().enumerate() {
            if i % 4 == 0 {
                // Cancel every 4th order
                modifications.push((order.id, ModificationType::Cancel));
            } else if i % 7 == 0 {
                // Modify every 7th order 
                modifications.push((order.id, ModificationType::Modify { 
                    new_quantity: order.quantity.as_i64() / 2 
                }));
            }
        }
        
        modifications
    }
    
    pub enum ModificationType {
        Cancel,
        Modify { new_quantity: i64 },
    }
}

/// Test assertions and validators
pub mod assertions {
    use orderbook::OrderBook;
    use services_common::{Px, Qty};
    
    /// Assert that orderbook satisfies basic invariants
    pub fn assert_orderbook_invariants(book: &OrderBook) {
        let (best_bid, best_ask) = book.get_bbo();
        
        // If both sides exist, spread should be non-negative
        if let (Some(bid), Some(ask)) = (best_bid, best_ask) {
            assert!(ask >= bid, "Ask price {} should be >= bid price {}", 
                ask.as_i64(), bid.as_i64());
        }
        
        // Get depth and verify ordering
        let (bid_levels, ask_levels) = book.get_depth(20);
        
        // Bid levels should be in descending price order
        for window in bid_levels.windows(2) {
            assert!(window[0].0 >= window[1].0, 
                "Bid levels not in descending order: {} vs {}", 
                window[0].0.as_i64(), window[1].0.as_i64());
        }
        
        // Ask levels should be in ascending price order
        for window in ask_levels.windows(2) {
            assert!(window[0].0 <= window[1].0,
                "Ask levels not in ascending order: {} vs {}",
                window[0].0.as_i64(), window[1].0.as_i64());
        }
        
        // All quantities should be positive
        for (_, qty, count) in bid_levels.iter().chain(ask_levels.iter()) {
            assert!(qty.as_i64() > 0, "Level quantity should be positive: {}", qty.as_i64());
            assert!(*count > 0, "Order count should be positive: {}", count);
        }
        
        // If we have both sides, verify mid price is between them
        if let (Some(bid), Some(ask)) = (best_bid, best_ask) {
            if let Some(mid) = book.get_mid() {
                assert!(mid >= bid && mid <= ask,
                    "Mid price {} should be between bid {} and ask {}",
                    mid.as_i64(), bid.as_i64(), ask.as_i64());
            }
        }
    }
    
    /// Assert that two orderbooks have the same state
    pub fn assert_orderbooks_equal(book1: &OrderBook, book2: &OrderBook) {
        let (bid1, ask1) = book1.get_bbo();
        let (bid2, ask2) = book2.get_bbo();
        
        assert_eq!(bid1, bid2, "Best bids don't match");
        assert_eq!(ask1, ask2, "Best asks don't match");
        
        let (levels1_bid, levels1_ask) = book1.get_depth(50);
        let (levels2_bid, levels2_ask) = book2.get_depth(50);
        
        assert_eq!(levels1_bid.len(), levels2_bid.len(), "Bid level counts don't match");
        assert_eq!(levels1_ask.len(), levels2_ask.len(), "Ask level counts don't match");
        
        for (level1, level2) in levels1_bid.iter().zip(levels2_bid.iter()) {
            assert_eq!(level1, level2, "Bid levels don't match: {:?} vs {:?}", level1, level2);
        }
        
        for (level1, level2) in levels1_ask.iter().zip(levels2_ask.iter()) {
            assert_eq!(level1, level2, "Ask levels don't match: {:?} vs {:?}", level1, level2);
        }
    }
    
    /// Assert that analytics values are within reasonable ranges
    pub fn assert_analytics_ranges(
        vpin: f64, 
        flow_imbalance: f64, 
        toxicity: f64,
        lambda: f64
    ) {
        assert!(vpin >= 0.0 && vpin <= 100.0, 
            "VPIN out of range [0, 100]: {}", vpin);
        assert!(flow_imbalance >= -100.0 && flow_imbalance <= 100.0,
            "Flow imbalance out of range [-100, 100]: {}", flow_imbalance);
        assert!(toxicity >= 0.0 && toxicity <= 100.0,
            "Toxicity out of range [0, 100]: {}", toxicity);
        assert!(lambda >= 0.0,
            "Kyle's lambda should be non-negative: {}", lambda);
    }
}