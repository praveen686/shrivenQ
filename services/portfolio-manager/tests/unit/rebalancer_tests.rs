//! Rebalancing logic tests
//! Tests order generation, execution, and management

use portfolio_manager::rebalancer::{Rebalancer, RebalanceOrder};
use portfolio_manager::position::PositionTracker;
use portfolio_manager::{RebalanceChange};
use rstest::*;
use services_common::{Qty, Side, Symbol};

// Test fixtures
#[fixture]
fn rebalancer() -> Rebalancer {
    Rebalancer::new()
}

#[fixture]
fn tracker() -> PositionTracker {
    PositionTracker::new(100)
}

#[fixture]
fn sample_rebalance_changes() -> Vec<RebalanceChange> {
    vec![
        RebalanceChange {
            symbol: Symbol::new(1),
            old_weight: 2000,  // 20%
            new_weight: 3000,  // 30%  
            quantity_change: 100000, // Need to buy
        },
        RebalanceChange {
            symbol: Symbol::new(2),
            old_weight: 4000,  // 40%
            new_weight: 2500,  // 25%
            quantity_change: -150000, // Need to sell
        },
        RebalanceChange {
            symbol: Symbol::new(3),
            old_weight: 1500,  // 15%
            new_weight: 2500,  // 25%
            quantity_change: 80000, // Need to buy
        },
    ]
}

#[fixture]
fn large_rebalance_changes() -> Vec<RebalanceChange> {
    vec![
        RebalanceChange {
            symbol: Symbol::new(1),
            old_weight: 1000,
            new_weight: 5000,
            quantity_change: 2000000, // Very large buy
        },
        RebalanceChange {
            symbol: Symbol::new(2), 
            old_weight: 6000,
            new_weight: 1000,
            quantity_change: -1500000, // Large sell
        },
        RebalanceChange {
            symbol: Symbol::new(3),
            old_weight: 2000,
            new_weight: 3000,
            quantity_change: 500000, // Medium buy
        },
        RebalanceChange {
            symbol: Symbol::new(4),
            old_weight: 1000,
            new_weight: 1000,
            quantity_change: 0, // No change (should be filtered)
        },
    ]
}

mod rebalancer_initialization_tests {
    use super::*;

    #[rstest]
    fn test_rebalancer_new(rebalancer: Rebalancer) {
        assert!(!rebalancer.is_rebalancing());
        assert!(rebalancer.pending_orders().is_empty());
    }

    #[rstest]
    fn test_initial_state(mut rebalancer: Rebalancer) {
        let pending = rebalancer.pending_orders();
        assert!(pending.is_empty());
        
        assert!(!rebalancer.is_rebalancing());
        
        // Cancel on empty should work
        rebalancer.cancel_pending();
        assert!(!rebalancer.is_rebalancing());
    }
}

mod order_generation_tests {
    use super::*;

    #[rstest]
    fn test_generate_buy_order(mut rebalancer: Rebalancer, tracker: PositionTracker) {
        let changes = vec![RebalanceChange {
            symbol: Symbol::new(1),
            old_weight: 1000,
            new_weight: 2000,
            quantity_change: 50000, // Positive = buy
        }];

        let orders = rebalancer.generate_orders(changes, &tracker);

        assert_eq!(orders.len(), 1);
        let order = &orders[0];
        
        assert_eq!(order.symbol, Symbol::new(1));
        assert_eq!(order.side, Side::Bid); // Buy order
        assert_eq!(order.quantity.as_i64(), 50000);
        assert_eq!(order.target_weight, 2000);
        assert!(order.order_id > 0);
    }

    #[rstest]
    fn test_generate_sell_order(mut rebalancer: Rebalancer, tracker: PositionTracker) {
        let changes = vec![RebalanceChange {
            symbol: Symbol::new(2),
            old_weight: 4000,
            new_weight: 2000,
            quantity_change: -75000, // Negative = sell
        }];

        let orders = rebalancer.generate_orders(changes, &tracker);

        assert_eq!(orders.len(), 1);
        let order = &orders[0];
        
        assert_eq!(order.symbol, Symbol::new(2));
        assert_eq!(order.side, Side::Ask); // Sell order
        assert_eq!(order.quantity.as_i64(), 75000); // Absolute value
        assert_eq!(order.target_weight, 2000);
    }

    #[rstest]
    fn test_generate_multiple_orders(mut rebalancer: Rebalancer, tracker: PositionTracker, sample_rebalance_changes: Vec<RebalanceChange>) {
        let orders = rebalancer.generate_orders(sample_rebalance_changes.clone(), &tracker);

        assert_eq!(orders.len(), 3); // Should match number of changes
        
        // Check each order corresponds to a change
        for (i, order) in orders.iter().enumerate() {
            let change = &sample_rebalance_changes[i];
            assert_eq!(order.symbol, change.symbol);
            assert_eq!(order.target_weight, change.new_weight);
            
            // Check side based on quantity change
            let expected_side = if change.quantity_change > 0 { Side::Bid } else { Side::Ask };
            assert_eq!(order.side, expected_side);
            
            // Check quantity is absolute value
            assert_eq!(order.quantity.as_i64(), change.quantity_change.abs());
        }
    }

    #[rstest]
    fn test_filter_zero_quantity_changes(mut rebalancer: Rebalancer, tracker: PositionTracker, large_rebalance_changes: Vec<RebalanceChange>) {
        let orders = rebalancer.generate_orders(large_rebalance_changes, &tracker);

        // Should exclude the zero quantity change
        assert_eq!(orders.len(), 3);
        
        // Verify no order has zero quantity
        for order in &orders {
            assert!(order.quantity.as_i64() > 0);
        }
        
        // Verify the zero-change symbol is not included
        let symbols: std::collections::HashSet<_> = orders.iter().map(|o| o.symbol).collect();
        assert!(!symbols.contains(&Symbol::new(4))); // The zero-change symbol
    }

    #[rstest]
    fn test_order_id_assignment(mut rebalancer: Rebalancer, tracker: PositionTracker) {
        let changes = vec![
            RebalanceChange {
                symbol: Symbol::new(1),
                old_weight: 1000,
                new_weight: 2000,
                quantity_change: 50000,
            },
            RebalanceChange {
                symbol: Symbol::new(2), 
                old_weight: 3000,
                new_weight: 1500,
                quantity_change: -30000,
            },
        ];

        let orders = rebalancer.generate_orders(changes, &tracker);

        assert_eq!(orders.len(), 2);
        
        // Order IDs should be unique and sequential
        assert_ne!(orders[0].order_id, orders[1].order_id);
        assert!(orders[0].order_id > 0);
        assert!(orders[1].order_id > 0);
        
        // Should typically be sequential (starting from internal counter)
        assert_eq!(orders[1].order_id, orders[0].order_id + 1);
    }

    #[rstest]
    fn test_empty_changes_list(mut rebalancer: Rebalancer, tracker: PositionTracker) {
        let empty_changes = vec![];
        let orders = rebalancer.generate_orders(empty_changes, &tracker);
        
        assert!(orders.is_empty());
    }

    #[rstest]
    fn test_large_quantity_handling(mut rebalancer: Rebalancer, tracker: PositionTracker) {
        let changes = vec![RebalanceChange {
            symbol: Symbol::new(1),
            old_weight: 1000,
            new_weight: 8000,
            quantity_change: 10_000_000_000, // Very large quantity
        }];

        let orders = rebalancer.generate_orders(changes, &tracker);

        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].quantity.as_i64(), 10_000_000_000);
        assert_eq!(orders[0].side, Side::Bid);
    }
}

mod rebalance_execution_tests {
    use super::*;

    #[rstest]
    async fn test_execute_rebalance(mut rebalancer: Rebalancer, tracker: PositionTracker, sample_rebalance_changes: Vec<RebalanceChange>) {
        let result = rebalancer.execute(sample_rebalance_changes.clone(), &tracker).await;

        assert!(result.is_ok());
        assert!(rebalancer.is_rebalancing());
        
        let pending_orders = rebalancer.pending_orders();
        assert_eq!(pending_orders.len(), 3);
        
        // Check that orders were created for each change
        for change in &sample_rebalance_changes {
            let order = pending_orders.iter().find(|o| o.symbol == change.symbol);
            assert!(order.is_some(), "No order found for symbol {:?}", change.symbol);
            
            let order = order.unwrap();
            assert_eq!(order.target_weight, change.new_weight);
            assert_eq!(order.quantity.as_i64(), change.quantity_change.abs());
        }
    }

    #[rstest]
    async fn test_execute_empty_rebalance(mut rebalancer: Rebalancer, tracker: PositionTracker) {
        let empty_changes = vec![];
        let result = rebalancer.execute(empty_changes, &tracker).await;

        assert!(result.is_ok());
        assert!(!rebalancer.is_rebalancing()); // No pending orders
        assert!(rebalancer.pending_orders().is_empty());
    }

    #[rstest]
    async fn test_execute_with_zero_quantities(mut rebalancer: Rebalancer, tracker: PositionTracker, large_rebalance_changes: Vec<RebalanceChange>) {
        let result = rebalancer.execute(large_rebalance_changes, &tracker).await;

        assert!(result.is_ok());
        assert!(rebalancer.is_rebalancing());
        
        // Should filter out zero-quantity changes
        let pending_orders = rebalancer.pending_orders();
        assert_eq!(pending_orders.len(), 3); // 4 changes - 1 zero quantity = 3
    }

    #[rstest]
    async fn test_multiple_executions(mut rebalancer: Rebalancer, tracker: PositionTracker) {
        let changes1 = vec![RebalanceChange {
            symbol: Symbol::new(1),
            old_weight: 2000,
            new_weight: 3000,
            quantity_change: 50000,
        }];

        let changes2 = vec![RebalanceChange {
            symbol: Symbol::new(2),
            old_weight: 4000,
            new_weight: 2000,
            quantity_change: -30000,
        }];

        // Execute first rebalance
        rebalancer.execute(changes1, &tracker).await.unwrap();
        assert_eq!(rebalancer.pending_orders().len(), 1);

        // Execute second rebalance (should add to pending)
        rebalancer.execute(changes2, &tracker).await.unwrap();
        assert_eq!(rebalancer.pending_orders().len(), 2);
    }
}

mod order_management_tests {
    use super::*;

    #[rstest]
    async fn test_handle_order_fill(mut rebalancer: Rebalancer, tracker: PositionTracker) {
        let changes = vec![RebalanceChange {
            symbol: Symbol::new(1),
            old_weight: 2000,
            new_weight: 3000,
            quantity_change: 50000,
        }];

        // Execute rebalance
        rebalancer.execute(changes, &tracker).await.unwrap();
        let pending_before = rebalancer.pending_orders();
        assert_eq!(pending_before.len(), 1);
        
        let order_id = pending_before[0].order_id;

        // Handle fill
        let filled_order = rebalancer.handle_fill(order_id);
        
        assert!(filled_order.is_some());
        let filled = filled_order.unwrap();
        assert_eq!(filled.order_id, order_id);
        assert_eq!(filled.symbol, Symbol::new(1));
        
        // Should no longer be pending
        assert!(!rebalancer.is_rebalancing());
        assert!(rebalancer.pending_orders().is_empty());
    }

    #[rstest]
    async fn test_handle_nonexistent_order_fill(mut rebalancer: Rebalancer, tracker: PositionTracker) {
        let changes = vec![RebalanceChange {
            symbol: Symbol::new(1),
            old_weight: 2000,
            new_weight: 3000,
            quantity_change: 50000,
        }];

        rebalancer.execute(changes, &tracker).await.unwrap();
        
        // Try to fill non-existent order
        let result = rebalancer.handle_fill(99999);
        assert!(result.is_none());
        
        // Pending orders should remain unchanged
        assert!(rebalancer.is_rebalancing());
        assert_eq!(rebalancer.pending_orders().len(), 1);
    }

    #[rstest]
    async fn test_partial_order_fills(mut rebalancer: Rebalancer, tracker: PositionTracker, sample_rebalance_changes: Vec<RebalanceChange>) {
        rebalancer.execute(sample_rebalance_changes, &tracker).await.unwrap();
        let initial_orders = rebalancer.pending_orders();
        assert_eq!(initial_orders.len(), 3);

        // Fill orders one by one
        let order_id_1 = initial_orders[0].order_id;
        let order_id_2 = initial_orders[1].order_id;

        // Fill first order
        let filled_1 = rebalancer.handle_fill(order_id_1);
        assert!(filled_1.is_some());
        assert_eq!(rebalancer.pending_orders().len(), 2);
        assert!(rebalancer.is_rebalancing());

        // Fill second order  
        let filled_2 = rebalancer.handle_fill(order_id_2);
        assert!(filled_2.is_some());
        assert_eq!(rebalancer.pending_orders().len(), 1);
        assert!(rebalancer.is_rebalancing());

        // Fill last order
        let order_id_3 = rebalancer.pending_orders()[0].order_id;
        let filled_3 = rebalancer.handle_fill(order_id_3);
        assert!(filled_3.is_some());
        assert!(!rebalancer.is_rebalancing());
        assert!(rebalancer.pending_orders().is_empty());
    }

    #[rstest]
    async fn test_cancel_pending_orders(mut rebalancer: Rebalancer, tracker: PositionTracker, sample_rebalance_changes: Vec<RebalanceChange>) {
        rebalancer.execute(sample_rebalance_changes, &tracker).await.unwrap();
        assert!(rebalancer.is_rebalancing());
        assert_eq!(rebalancer.pending_orders().len(), 3);

        // Cancel all pending
        rebalancer.cancel_pending();
        
        assert!(!rebalancer.is_rebalancing());
        assert!(rebalancer.pending_orders().is_empty());
    }

    #[rstest]
    fn test_cancel_when_no_pending_orders(mut rebalancer: Rebalancer) {
        assert!(!rebalancer.is_rebalancing());
        
        // Should handle gracefully
        rebalancer.cancel_pending();
        
        assert!(!rebalancer.is_rebalancing());
        assert!(rebalancer.pending_orders().is_empty());
    }
}

mod rebalance_order_tests {
    use super::*;

    #[rstest]
    fn test_rebalance_order_creation() {
        let order = RebalanceOrder {
            order_id: 123,
            symbol: Symbol::new(5),
            side: Side::Bid,
            quantity: Qty::from_i64(50000),
            target_weight: 2500,
        };

        assert_eq!(order.order_id, 123);
        assert_eq!(order.symbol, Symbol::new(5));
        assert_eq!(order.side, Side::Bid);
        assert_eq!(order.quantity.as_i64(), 50000);
        assert_eq!(order.target_weight, 2500);
    }

    #[rstest]
    fn test_rebalance_order_clone() {
        let original = RebalanceOrder {
            order_id: 456,
            symbol: Symbol::new(7),
            side: Side::Ask,
            quantity: Qty::from_i64(75000),
            target_weight: 1500,
        };

        let cloned = original.clone();

        assert_eq!(cloned.order_id, original.order_id);
        assert_eq!(cloned.symbol, original.symbol);
        assert_eq!(cloned.side, original.side);
        assert_eq!(cloned.quantity.as_i64(), original.quantity.as_i64());
        assert_eq!(cloned.target_weight, original.target_weight);
    }
}

mod edge_case_tests {
    use super::*;

    #[rstest]
    async fn test_very_large_rebalance(mut rebalancer: Rebalancer, tracker: PositionTracker) {
        let large_changes = vec![
            RebalanceChange {
                symbol: Symbol::new(1),
                old_weight: 100,
                new_weight: 9000,
                quantity_change: 1_000_000_000_000, // 1 trillion
            },
        ];

        let result = rebalancer.execute(large_changes, &tracker).await;
        assert!(result.is_ok());
        
        let orders = rebalancer.pending_orders();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].quantity.as_i64(), 1_000_000_000_000);
    }

    #[rstest]
    async fn test_many_small_rebalances(mut rebalancer: Rebalancer, tracker: PositionTracker) {
        let many_changes: Vec<RebalanceChange> = (1..=100)
            .map(|i| RebalanceChange {
                symbol: Symbol::new(i),
                old_weight: 100,
                new_weight: 200,
                quantity_change: 1000,
            })
            .collect();

        let result = rebalancer.execute(many_changes, &tracker).await;
        assert!(result.is_ok());
        
        let orders = rebalancer.pending_orders();
        assert_eq!(orders.len(), 100);
        
        // All orders should be buy orders with same quantity
        for order in &orders {
            assert_eq!(order.side, Side::Bid);
            assert_eq!(order.quantity.as_i64(), 1000);
            assert_eq!(order.target_weight, 200);
        }
    }

    #[rstest]
    async fn test_mixed_positive_negative_changes(mut rebalancer: Rebalancer, tracker: PositionTracker) {
        let mixed_changes = vec![
            RebalanceChange {
                symbol: Symbol::new(1),
                old_weight: 1000,
                new_weight: 3000,
                quantity_change: 100000, // Buy
            },
            RebalanceChange {
                symbol: Symbol::new(2),
                old_weight: 3000,
                new_weight: 1000,
                quantity_change: -80000, // Sell
            },
            RebalanceChange {
                symbol: Symbol::new(3),
                old_weight: 2000,
                new_weight: 4000,
                quantity_change: 150000, // Buy
            },
            RebalanceChange {
                symbol: Symbol::new(4),
                old_weight: 4000,
                new_weight: 2000,
                quantity_change: -120000, // Sell
            },
        ];

        let result = rebalancer.execute(mixed_changes, &tracker).await;
        assert!(result.is_ok());
        
        let orders = rebalancer.pending_orders();
        assert_eq!(orders.len(), 4);
        
        // Check sides are correct
        let buy_orders: Vec<_> = orders.iter().filter(|o| o.side == Side::Bid).collect();
        let sell_orders: Vec<_> = orders.iter().filter(|o| o.side == Side::Ask).collect();
        
        assert_eq!(buy_orders.len(), 2);  // Symbols 1 and 3
        assert_eq!(sell_orders.len(), 2); // Symbols 2 and 4
    }

    #[rstest]
    async fn test_rebalance_with_tracker_integration(mut rebalancer: Rebalancer, tracker: PositionTracker) {
        let changes = vec![RebalanceChange {
            symbol: Symbol::new(1),
            old_weight: 2000,
            new_weight: 4000,
            quantity_change: 100000,
        }];

        // Execute rebalance
        rebalancer.execute(changes, &tracker).await.unwrap();
        
        // Check that pending order was added to tracker
        let orders = rebalancer.pending_orders();
        assert_eq!(orders.len(), 1);
        
        let order = &orders[0];
        
        // The rebalancer should have called tracker.add_pending()
        // This is internal behavior, so we mainly verify the order exists
        assert_eq!(order.symbol, Symbol::new(1));
        assert_eq!(order.side, Side::Bid);
        assert_eq!(order.quantity.as_i64(), 100000);
    }

    #[rstest]
    async fn test_concurrent_rebalance_operations() {
        // This would test concurrent access, but since we don't have
        // explicit concurrency in the rebalancer, we test sequential operations
        let mut rebalancer = Rebalancer::new();
        let tracker = PositionTracker::new(10);

        // Quick succession of operations
        let changes1 = vec![RebalanceChange {
            symbol: Symbol::new(1),
            old_weight: 1000,
            new_weight: 2000,
            quantity_change: 50000,
        }];

        let changes2 = vec![RebalanceChange {
            symbol: Symbol::new(2),
            old_weight: 3000,
            new_weight: 1500,
            quantity_change: -30000,
        }];

        // Execute both
        rebalancer.execute(changes1, &tracker).await.unwrap();
        rebalancer.execute(changes2, &tracker).await.unwrap();

        // Should have both orders pending
        assert_eq!(rebalancer.pending_orders().len(), 2);
        assert!(rebalancer.is_rebalancing());
    }

    #[rstest]
    fn test_rebalancer_state_consistency(mut rebalancer: Rebalancer) {
        // Test various state transitions
        assert!(!rebalancer.is_rebalancing());
        
        // Cancel when empty should work
        rebalancer.cancel_pending();
        assert!(!rebalancer.is_rebalancing());
        
        // Handle fill on empty should return None
        let result = rebalancer.handle_fill(123);
        assert!(result.is_none());
        assert!(!rebalancer.is_rebalancing());
    }
}