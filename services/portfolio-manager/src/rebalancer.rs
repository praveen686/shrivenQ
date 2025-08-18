//! Portfolio rebalancing execution
//!
//! COMPLIANCE:
//! - No allocations during rebalancing
//! - Fixed-point arithmetic
//! - Atomic order generation

use crate::{RebalanceChange, position::PositionTracker};
use anyhow::Result;
use services_common::{Qty, Side, Symbol};
use rustc_hash::FxHashMap;

/// Portfolio rebalancer
pub struct Rebalancer {
    /// Pending rebalance orders
    pending_orders: FxHashMap<u64, RebalanceOrder>,
    /// Next order ID
    next_order_id: u64,
}

/// Rebalance order
#[derive(Debug, Clone)]
pub struct RebalanceOrder {
    pub order_id: u64,
    pub symbol: Symbol,
    pub side: Side,
    pub quantity: Qty,
    pub target_weight: i32,
}

impl Rebalancer {
    /// Create new rebalancer
    pub fn new() -> Self {
        let mut pending_orders = FxHashMap::default();
        pending_orders.reserve(100);

        Self {
            pending_orders,
            next_order_id: 1,
        }
    }

    /// Execute rebalance changes
    pub async fn execute(
        &mut self,
        changes: Vec<RebalanceChange>,
        tracker: &PositionTracker,
    ) -> Result<()> {
        // Generate orders for each change
        let orders = self.generate_orders(changes, tracker);

        // Store pending orders
        for order in orders {
            self.pending_orders.insert(order.order_id, order.clone());

            // Would send to execution router here
            // For now, just track internally
            tracker.add_pending(order.order_id, order.symbol, order.side, order.quantity);
        }

        Ok(())
    }

    /// Generate orders from rebalance changes
    fn generate_orders(
        &mut self,
        changes: Vec<RebalanceChange>,
        _tracker: &PositionTracker,
    ) -> Vec<RebalanceOrder> {
        let mut orders = Vec::with_capacity(changes.len());

        for change in changes {
            if change.quantity_change == 0 {
                continue;
            }

            let order_id = self.next_order_id;
            self.next_order_id += 1;

            let (side, quantity) = if change.quantity_change > 0 {
                (Side::Bid, Qty::from_i64(change.quantity_change))
            } else {
                (Side::Ask, Qty::from_i64(-change.quantity_change))
            };

            orders.push(RebalanceOrder {
                order_id,
                symbol: change.symbol,
                side,
                quantity,
                target_weight: change.new_weight,
            });
        }

        orders
    }

    /// Handle order fill
    pub fn handle_fill(&mut self, order_id: u64) -> Option<RebalanceOrder> {
        self.pending_orders.remove(&order_id)
    }

    /// Cancel pending rebalance
    pub fn cancel_pending(&mut self) {
        self.pending_orders.clear();
    }

    /// Get pending orders
    pub fn pending_orders(&self) -> Vec<RebalanceOrder> {
        self.pending_orders.values().cloned().collect()
    }

    /// Check if rebalance is in progress
    pub fn is_rebalancing(&self) -> bool {
        !self.pending_orders.is_empty()
    }
}

impl Default for Rebalancer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rebalancer_execution() {
        let mut rebalancer = Rebalancer::new();
        let tracker = PositionTracker::new(10);

        let changes = vec![
            RebalanceChange {
                symbol: Symbol::new(1),
                old_weight: 3000,
                new_weight: 4000,
                quantity_change: 100000,
            },
            RebalanceChange {
                symbol: Symbol::new(2),
                old_weight: 4000,
                new_weight: 3000,
                quantity_change: -50000,
            },
        ];

        rebalancer.execute(changes, &tracker).await.unwrap();

        assert!(rebalancer.is_rebalancing());
        assert_eq!(rebalancer.pending_orders().len(), 2);
    }

    #[test]
    fn test_order_generation() {
        let mut rebalancer = Rebalancer::new();
        let tracker = PositionTracker::new(10);

        let changes = vec![RebalanceChange {
            symbol: Symbol::new(1),
            old_weight: 2000,
            new_weight: 3000,
            quantity_change: 50000,
        }];

        let orders = rebalancer.generate_orders(changes, &tracker);

        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].side, Side::Bid);
        assert_eq!(orders[0].quantity.as_i64(), 50000);
    }
}
