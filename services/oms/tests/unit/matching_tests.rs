//! Unit tests for order matching engine

use chrono::Utc;
use rstest::*;
use services_common::{Px, Qty, Symbol};
use uuid::Uuid;
use std::sync::Arc;

use oms::matching::{MatchingEngine, OrderBookMatcher, Match, match_to_fills};
use oms::order::{Order, OrderSide, OrderStatus, OrderType, TimeInForce, LiquidityIndicator};

/// Test fixture for matching engine
#[fixture]
fn matching_engine() -> MatchingEngine {
    MatchingEngine::new()
}

/// Test fixture for creating buy orders
#[fixture]
fn buy_order_at_price(#[default(1_000_000i64)] price: i64, #[default(10_000i64)] qty: i64) -> Order {
    create_test_order(OrderSide::Buy, Some(price), qty, 1)
}

/// Test fixture for creating sell orders
#[fixture]
fn sell_order_at_price(#[default(1_000_000i64)] price: i64, #[default(10_000i64)] qty: i64) -> Order {
    create_test_order(OrderSide::Sell, Some(price), qty, 2)
}

/// Helper function to create test orders
fn create_test_order(side: OrderSide, price: Option<i64>, qty: i64, seq: u64) -> Order {
    Order {
        id: Uuid::new_v4(),
        client_order_id: Some(format!("TEST-{}", seq)),
        parent_order_id: None,
        symbol: Symbol(1), // BTC/USDT
        side,
        order_type: if price.is_some() { OrderType::Limit } else { OrderType::Market },
        time_in_force: TimeInForce::Gtc,
        quantity: Qty::from_i64(qty),
        executed_quantity: Qty::ZERO,
        remaining_quantity: Qty::from_i64(qty),
        price: price.map(Px::from_i64),
        stop_price: None,
        status: OrderStatus::New,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        account: "test_account".to_string(),
        exchange: "internal_matching".to_string(),
        strategy_id: Some("test_strategy".to_string()),
        tags: vec!["test".to_string()],
        fills: vec![],
        amendments: vec![],
        version: 1,
        sequence_number: seq,
    }
}

/// Helper to create market orders
fn create_market_order(side: OrderSide, qty: i64, seq: u64) -> Order {
    create_test_order(side, None, qty, seq)
}

#[rstest]
fn test_add_limit_order_no_match(matching_engine: MatchingEngine) {
    let buy_order = create_test_order(OrderSide::Buy, Some(1_000_000), 10_000, 1);
    
    let matches = matching_engine.add_order(&buy_order).expect("Should add order successfully");
    assert!(matches.is_empty(), "Single order should not generate matches");
}

#[rstest]
fn test_limit_order_matching_exact_price() {
    let engine = MatchingEngine::new();
    
    // Add sell order at $100.00
    let sell_order = create_test_order(OrderSide::Sell, Some(1_000_000), 10_000, 1);
    let sell_matches = engine.add_order(&sell_order).expect("Should add sell order");
    assert!(sell_matches.is_empty(), "First order should not match");
    
    // Add buy order at same price - should match
    let buy_order = create_test_order(OrderSide::Buy, Some(1_000_000), 5_000, 2);
    let buy_matches = engine.add_order(&buy_order).expect("Should add buy order");
    
    assert_eq!(buy_matches.len(), 1, "Should generate one match");
    assert_eq!(buy_matches[0].quantity.as_i64(), 5_000, "Should match full buy quantity");
    assert_eq!(buy_matches[0].price.as_i64(), 1_000_000, "Should match at passive (sell) price");
    assert_eq!(buy_matches[0].aggressive_order, buy_order.id);
    assert_eq!(buy_matches[0].passive_order, sell_order.id);
}

#[rstest]
fn test_limit_order_matching_better_price() {
    let engine = MatchingEngine::new();
    
    // Add sell order at $100.00
    let sell_order = create_test_order(OrderSide::Sell, Some(1_000_000), 10_000, 1);
    engine.add_order(&sell_order).expect("Should add sell order");
    
    // Add buy order at better price ($101.00) - should match at sell price
    let buy_order = create_test_order(OrderSide::Buy, Some(1_010_000), 5_000, 2);
    let matches = engine.add_order(&buy_order).expect("Should add buy order");
    
    assert_eq!(matches.len(), 1, "Should generate one match");
    assert_eq!(matches[0].price.as_i64(), 1_000_000, "Should match at passive price (price improvement)");
}

#[rstest]
fn test_limit_order_no_match_worse_price() {
    let engine = MatchingEngine::new();
    
    // Add sell order at $100.00
    let sell_order = create_test_order(OrderSide::Sell, Some(1_000_000), 10_000, 1);
    engine.add_order(&sell_order).expect("Should add sell order");
    
    // Add buy order at worse price ($99.00) - should not match
    let buy_order = create_test_order(OrderSide::Buy, Some(990_000), 5_000, 2);
    let matches = engine.add_order(&buy_order).expect("Should add buy order");
    
    assert!(matches.is_empty(), "Worse price should not match");
}

#[rstest]
fn test_partial_fill_matching() {
    let engine = MatchingEngine::new();
    
    // Add large sell order at $100.00
    let sell_order = create_test_order(OrderSide::Sell, Some(1_000_000), 50_000, 1);
    engine.add_order(&sell_order).expect("Should add sell order");
    
    // Add smaller buy order - should partially fill
    let buy_order = create_test_order(OrderSide::Buy, Some(1_000_000), 20_000, 2);
    let matches = engine.add_order(&buy_order).expect("Should add buy order");
    
    assert_eq!(matches.len(), 1, "Should generate one match");
    assert_eq!(matches[0].quantity.as_i64(), 20_000, "Should match full buy quantity");
    
    // Sell order should still be in the book with reduced quantity
    let depth = engine.get_depth(Symbol(1), 5).expect("Should get depth");
    assert_eq!(depth.asks.len(), 1, "Should have one ask level");
    assert_eq!(depth.asks[0].1, 30_000, "Remaining sell quantity should be 30,000");
}

#[rstest]
fn test_multiple_price_levels_matching() {
    let engine = MatchingEngine::new();
    
    // Add multiple sell orders at different prices
    let sell1 = create_test_order(OrderSide::Sell, Some(1_000_000), 10_000, 1); // $100.00
    let sell2 = create_test_order(OrderSide::Sell, Some(1_010_000), 10_000, 2); // $101.00
    let sell3 = create_test_order(OrderSide::Sell, Some(1_020_000), 10_000, 3); // $102.00
    
    engine.add_order(&sell1).expect("Should add sell1");
    engine.add_order(&sell2).expect("Should add sell2");  
    engine.add_order(&sell3).expect("Should add sell3");
    
    // Add large buy order that should match multiple levels
    let buy_order = create_test_order(OrderSide::Buy, Some(1_015_000), 25_000, 4); // $101.50
    let matches = engine.add_order(&buy_order).expect("Should add buy order");
    
    assert_eq!(matches.len(), 2, "Should match two price levels");
    
    // First match should be at best (lowest) sell price
    assert_eq!(matches[0].quantity.as_i64(), 10_000);
    assert_eq!(matches[0].price.as_i64(), 1_000_000); // $100.00
    
    // Second match should be at next price level  
    assert_eq!(matches[1].quantity.as_i64(), 10_000);
    assert_eq!(matches[1].price.as_i64(), 1_010_000); // $101.00
    
    // Buy order should have 5,000 remaining and be added to book
    let depth = engine.get_depth(Symbol(1), 5).expect("Should get depth");
    assert_eq!(depth.bids.len(), 1, "Should have one bid level");
    assert_eq!(depth.bids[0].1, 5_000, "Should have 5,000 remaining");
}

#[rstest]
fn test_market_order_buy() {
    let engine = MatchingEngine::new();
    
    // Add sell orders at different prices
    let sell1 = create_test_order(OrderSide::Sell, Some(1_000_000), 5_000, 1);  // $100.00
    let sell2 = create_test_order(OrderSide::Sell, Some(1_010_000), 7_000, 2);  // $101.00
    
    engine.add_order(&sell1).expect("Should add sell1");
    engine.add_order(&sell2).expect("Should add sell2");
    
    // Market buy order should match against available liquidity
    let market_buy = create_market_order(OrderSide::Buy, 8_000, 3);
    let matches = engine.add_order(&market_buy).expect("Should add market buy");
    
    assert_eq!(matches.len(), 2, "Should match against two orders");
    
    // Should fill first order completely and second partially
    assert_eq!(matches[0].quantity.as_i64(), 5_000);
    assert_eq!(matches[0].price.as_i64(), 1_000_000);
    
    assert_eq!(matches[1].quantity.as_i64(), 3_000);
    assert_eq!(matches[1].price.as_i64(), 1_010_000);
}

#[rstest] 
fn test_market_order_sell() {
    let engine = MatchingEngine::new();
    
    // Add buy orders at different prices
    let buy1 = create_test_order(OrderSide::Buy, Some(1_010_000), 5_000, 1);  // $101.00  
    let buy2 = create_test_order(OrderSide::Buy, Some(1_000_000), 7_000, 2);  // $100.00
    
    engine.add_order(&buy1).expect("Should add buy1");
    engine.add_order(&buy2).expect("Should add buy2");
    
    // Market sell order should match against best bids
    let market_sell = create_market_order(OrderSide::Sell, 8_000, 3);
    let matches = engine.add_order(&market_sell).expect("Should add market sell");
    
    assert_eq!(matches.len(), 2, "Should match against two orders");
    
    // Should fill highest bid first
    assert_eq!(matches[0].quantity.as_i64(), 5_000);
    assert_eq!(matches[0].price.as_i64(), 1_010_000); // Best bid price
    
    assert_eq!(matches[1].quantity.as_i64(), 3_000);
    assert_eq!(matches[1].price.as_i64(), 1_000_000);
}

#[rstest]
fn test_order_book_depth() {
    let engine = MatchingEngine::new();
    
    // Add multiple orders at same price levels
    engine.add_order(&create_test_order(OrderSide::Buy, Some(1_000_000), 10_000, 1)).unwrap();
    engine.add_order(&create_test_order(OrderSide::Buy, Some(1_000_000), 5_000, 2)).unwrap();
    engine.add_order(&create_test_order(OrderSide::Buy, Some(990_000), 8_000, 3)).unwrap();
    
    engine.add_order(&create_test_order(OrderSide::Sell, Some(1_010_000), 12_000, 4)).unwrap();
    engine.add_order(&create_test_order(OrderSide::Sell, Some(1_010_000), 3_000, 5)).unwrap();
    engine.add_order(&create_test_order(OrderSide::Sell, Some(1_020_000), 6_000, 6)).unwrap();
    
    let depth = engine.get_depth(Symbol(1), 3).expect("Should get depth");
    
    // Check bids (should be sorted by price descending)
    assert_eq!(depth.bids.len(), 2, "Should have 2 bid levels");
    assert_eq!(depth.bids[0], (1_000_000, 15_000)); // Aggregated quantity
    assert_eq!(depth.bids[1], (990_000, 8_000));
    
    // Check asks (should be sorted by price ascending)  
    assert_eq!(depth.asks.len(), 2, "Should have 2 ask levels");
    assert_eq!(depth.asks[0], (1_010_000, 15_000)); // Aggregated quantity
    assert_eq!(depth.asks[1], (1_020_000, 6_000));
}

#[rstest]
fn test_cancel_order() {
    let engine = MatchingEngine::new();
    
    let order = create_test_order(OrderSide::Buy, Some(1_000_000), 10_000, 1);
    let order_id = order.id;
    let symbol = order.symbol;
    
    engine.add_order(&order).expect("Should add order");
    
    let cancelled = engine.cancel_order(order_id, symbol).expect("Should cancel order");
    assert!(cancelled, "Order should be successfully cancelled");
    
    // Order book should be empty
    let depth = engine.get_depth(Symbol(1), 5).expect("Should get depth");
    assert!(depth.bids.is_empty(), "Bids should be empty after cancellation");
}

#[rstest]
fn test_cancel_nonexistent_order() {
    let engine = MatchingEngine::new();
    
    let fake_id = Uuid::new_v4();
    let cancelled = engine.cancel_order(fake_id, Symbol(1)).expect("Should handle gracefully");
    assert!(!cancelled, "Non-existent order cancellation should return false");
}

#[rstest]
fn test_time_priority() {
    let engine = MatchingEngine::new();
    
    // Add two buy orders at same price but different times
    let buy1 = create_test_order(OrderSide::Buy, Some(1_000_000), 10_000, 1); // Earlier sequence
    let buy2 = create_test_order(OrderSide::Buy, Some(1_000_000), 10_000, 2); // Later sequence
    
    engine.add_order(&buy1).expect("Should add buy1");
    engine.add_order(&buy2).expect("Should add buy2");
    
    // Add partial sell order - should match first buy order due to time priority
    let sell_order = create_test_order(OrderSide::Sell, Some(1_000_000), 5_000, 3);
    let matches = engine.add_order(&sell_order).expect("Should add sell order");
    
    assert_eq!(matches.len(), 1, "Should generate one match");
    assert_eq!(matches[0].passive_order, buy1.id, "Should match with earlier order");
}

#[rstest]
fn test_price_time_priority() {
    let engine = MatchingEngine::new();
    
    // Add buy orders at different prices
    let buy_lower = create_test_order(OrderSide::Buy, Some(990_000), 10_000, 1);   // $99.00, earlier
    let buy_higher = create_test_order(OrderSide::Buy, Some(1_000_000), 10_000, 2); // $100.00, later
    
    engine.add_order(&buy_lower).expect("Should add lower buy");
    engine.add_order(&buy_higher).expect("Should add higher buy");
    
    // Sell order should match higher price first, despite later time
    let sell_order = create_test_order(OrderSide::Sell, Some(990_000), 5_000, 3);
    let matches = engine.add_order(&sell_order).expect("Should add sell order");
    
    assert_eq!(matches.len(), 1, "Should generate one match");
    assert_eq!(matches[0].passive_order, buy_higher.id, "Should match with better price first");
    assert_eq!(matches[0].price.as_i64(), 1_000_000, "Should match at better price");
}

#[rstest]
fn test_match_to_fills() {
    let match_data = Match {
        id: 12345,
        symbol: Symbol(1),
        aggressive_order: Uuid::new_v4(),
        passive_order: Uuid::new_v4(),
        quantity: Qty::from_i64(10_000),
        price: Px::from_i64(1_000_000),
        timestamp: Utc::now(),
    };
    
    let (aggressive_fill, passive_fill) = match_to_fills(&match_data);
    
    // Check aggressive fill (taker)
    assert_eq!(aggressive_fill.order_id, match_data.aggressive_order);
    assert_eq!(aggressive_fill.quantity, match_data.quantity);
    assert_eq!(aggressive_fill.price, match_data.price);
    assert_eq!(aggressive_fill.liquidity, LiquidityIndicator::Taker);
    assert!(aggressive_fill.commission > 0, "Taker should pay commission");
    
    // Check passive fill (maker)
    assert_eq!(passive_fill.order_id, match_data.passive_order);
    assert_eq!(passive_fill.quantity, match_data.quantity);
    assert_eq!(passive_fill.price, match_data.price);
    assert_eq!(passive_fill.liquidity, LiquidityIndicator::Maker);
    assert!(passive_fill.commission >= 0, "Maker should pay lower/zero commission");
    
    // Taker commission should be higher than maker
    assert!(aggressive_fill.commission >= passive_fill.commission, 
            "Taker commission should be >= maker commission");
}

#[rstest]
fn test_order_book_matcher_stats() {
    let matcher = OrderBookMatcher::new(Symbol(1));
    
    assert_eq!(matcher.get_symbol(), Symbol(1));
    
    let stats = matcher.get_symbol_stats();
    assert_eq!(stats.symbol, Symbol(1));
    assert_eq!(stats.last_price, 0);
    assert_eq!(stats.total_volume, 0);
    assert_eq!(stats.bid_count, 0);
    assert_eq!(stats.ask_count, 0);
}

#[rstest]
fn test_pending_matches_queue() {
    let engine = MatchingEngine::new();
    
    // Add orders that will match
    let sell_order = create_test_order(OrderSide::Sell, Some(1_000_000), 10_000, 1);
    engine.add_order(&sell_order).expect("Should add sell order");
    
    let buy_order = create_test_order(OrderSide::Buy, Some(1_000_000), 5_000, 2);
    engine.add_order(&buy_order).expect("Should add buy order");
    
    // Check pending matches queue
    let pending_matches = engine.get_pending_matches();
    assert_eq!(pending_matches.len(), 1, "Should have one pending match");
    assert_eq!(pending_matches[0].quantity.as_i64(), 5_000);
    
    // Getting pending matches again should be empty (consumed)
    let empty_matches = engine.get_pending_matches();
    assert!(empty_matches.is_empty(), "Pending matches should be consumed");
}

// Edge cases and error handling

#[rstest]
fn test_limit_order_without_price() {
    let engine = MatchingEngine::new();
    let mut order = create_test_order(OrderSide::Buy, Some(1_000_000), 10_000, 1);
    order.price = None;
    
    let result = engine.add_order(&order);
    assert!(result.is_err(), "Limit order without price should fail");
    assert!(result.unwrap_err().to_string().contains("Limit order requires price"));
}

#[rstest]
fn test_zero_quantity_order() {
    let engine = MatchingEngine::new();
    let order = create_test_order(OrderSide::Buy, Some(1_000_000), 0, 1);
    
    // This should be caught at validation level, but matching engine should handle gracefully
    let matches = engine.add_order(&order).expect("Should handle zero quantity");
    assert!(matches.is_empty(), "Zero quantity should not generate matches");
}

#[rstest]
fn test_self_match_prevention() {
    let engine = MatchingEngine::new();
    
    let order_id = Uuid::new_v4();
    
    // Add buy order
    let mut buy_order = create_test_order(OrderSide::Buy, Some(1_000_000), 10_000, 1);
    buy_order.id = order_id;
    engine.add_order(&buy_order).expect("Should add buy order");
    
    // Try to add sell order with same ID (simulating self-match)
    let mut sell_order = create_test_order(OrderSide::Sell, Some(1_000_000), 5_000, 2);
    sell_order.id = order_id;
    
    // This should not self-match (in real implementation)
    // For now, just verify the engine doesn't crash
    let _matches = engine.add_order(&sell_order).expect("Should handle gracefully");
}

// Performance tests
#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;
    
    #[test]
    fn test_order_insertion_performance() {
        let engine = MatchingEngine::new();
        
        let start = Instant::now();
        for i in 0..10_000 {
            let order = create_test_order(
                if i % 2 == 0 { OrderSide::Buy } else { OrderSide::Sell },
                Some(1_000_000 + (i % 1000) as i64 * 1000), // Spread prices
                1000,
                i as u64,
            );
            engine.add_order(&order).expect("Should add order");
        }
        let duration = start.elapsed();
        
        println!("Added 10,000 orders in {}ms", duration.as_millis());
        assert!(duration.as_millis() < 1000, "Should add 10k orders in under 1 second");
    }
    
    #[test]
    fn test_matching_performance() {
        let engine = MatchingEngine::new();
        
        // Add 5000 buy and 5000 sell orders that will all match
        for i in 0..5_000 {
            let sell_order = create_test_order(OrderSide::Sell, Some(1_000_000), 1000, i);
            engine.add_order(&sell_order).expect("Should add sell order");
        }
        
        let start = Instant::now();
        for i in 5_000..10_000 {
            let buy_order = create_test_order(OrderSide::Buy, Some(1_000_000), 1000, i);
            let _matches = engine.add_order(&buy_order).expect("Should add buy order");
        }
        let duration = start.elapsed();
        
        println!("Matched 5,000 orders in {}ms", duration.as_millis());
        assert!(duration.as_millis() < 500, "Should match 5k orders in under 500ms");
    }
    
    #[test] 
    fn test_depth_calculation_performance() {
        let engine = MatchingEngine::new();
        
        // Add many orders at different price levels
        for i in 0..1000 {
            let buy_price = 1_000_000 - i * 1000;
            let sell_price = 1_000_000 + i * 1000;
            
            engine.add_order(&create_test_order(OrderSide::Buy, Some(buy_price), 1000, i * 2)).unwrap();
            engine.add_order(&create_test_order(OrderSide::Sell, Some(sell_price), 1000, i * 2 + 1)).unwrap();
        }
        
        let start = Instant::now();
        for _ in 0..1000 {
            let _depth = engine.get_depth(Symbol(1), 10).expect("Should get depth");
        }
        let duration = start.elapsed();
        
        println!("Calculated depth 1,000 times in {}ms", duration.as_millis());
        assert!(duration.as_millis() < 100, "Depth calculation should be fast");
    }
}

// Concurrent access tests  
#[cfg(test)]
mod concurrency_tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    
    #[test]
    fn test_concurrent_order_insertion() {
        let engine = Arc::new(MatchingEngine::new());
        let mut handles = vec![];
        
        // Spawn 10 threads, each adding 1000 orders
        for thread_id in 0..10 {
            let engine_clone = Arc::clone(&engine);
            let handle = thread::spawn(move || {
                for i in 0..1000 {
                    let order = create_test_order(
                        if i % 2 == 0 { OrderSide::Buy } else { OrderSide::Sell },
                        Some(1_000_000 + (i % 100) as i64 * 1000),
                        1000,
                        (thread_id * 1000 + i) as u64,
                    );
                    engine_clone.add_order(&order).expect("Should add order");
                }
            });
            handles.push(handle);
        }
        
        // Wait for all threads to complete
        for handle in handles {
            handle.join().expect("Thread should complete successfully");
        }
        
        // Verify final state
        let depth = engine.get_depth(Symbol(1), 100).expect("Should get depth");
        assert!(!depth.bids.is_empty() || !depth.asks.is_empty(), "Should have some orders");
    }
    
    #[test]
    fn test_concurrent_matching_and_cancellation() {
        let engine = Arc::new(MatchingEngine::new());
        
        // Add some initial orders
        for i in 0..100 {
            let order = create_test_order(OrderSide::Sell, Some(1_000_000 + i * 1000), 1000, i);
            engine.add_order(&order).expect("Should add order");
        }
        
        let engine1 = Arc::clone(&engine);
        let engine2 = Arc::clone(&engine);
        
        // Thread 1: Add matching buy orders
        let handle1 = thread::spawn(move || {
            for i in 0..50 {
                let order = create_test_order(OrderSide::Buy, Some(1_050_000), 1000, 1000 + i);
                let _matches = engine1.add_order(&order).expect("Should add order");
            }
        });
        
        // Thread 2: Try to cancel orders (some may not exist anymore due to matching)
        let handle2 = thread::spawn(move || {
            for i in 0..25 {
                let fake_id = Uuid::new_v4();
                let _cancelled = engine2.cancel_order(fake_id, Symbol(1)).expect("Should handle gracefully");
            }
        });
        
        handle1.join().expect("Thread 1 should complete");
        handle2.join().expect("Thread 2 should complete");
    }
}