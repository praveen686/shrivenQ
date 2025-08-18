//! Order lifecycle management

use crate::order::{Order, OrderStatus, OrderType, TimeInForce};
use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use tracing::debug;

/// Order lifecycle manager
pub struct OrderLifecycleManager {
    /// Valid state transitions
    valid_transitions: HashMap<OrderStatus, Vec<OrderStatus>>,
}

impl Default for OrderLifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderLifecycleManager {
    /// Create new lifecycle manager
    #[must_use] pub fn new() -> Self {
        let mut valid_transitions = HashMap::new();
        
        // Define valid state transitions
        valid_transitions.insert(
            OrderStatus::New,
            vec![OrderStatus::Pending, OrderStatus::Cancelled, OrderStatus::Rejected],
        );
        
        valid_transitions.insert(
            OrderStatus::Pending,
            vec![OrderStatus::Submitted, OrderStatus::Cancelled, OrderStatus::Rejected],
        );
        
        valid_transitions.insert(
            OrderStatus::Submitted,
            vec![OrderStatus::Accepted, OrderStatus::Cancelled, OrderStatus::Rejected],
        );
        
        valid_transitions.insert(
            OrderStatus::Accepted,
            vec![
                OrderStatus::PartiallyFilled,
                OrderStatus::Filled,
                OrderStatus::Cancelled,
                OrderStatus::Expired,
            ],
        );
        
        valid_transitions.insert(
            OrderStatus::PartiallyFilled,
            vec![OrderStatus::Filled, OrderStatus::Cancelled, OrderStatus::Expired],
        );
        
        // Terminal states have no transitions
        valid_transitions.insert(OrderStatus::Filled, vec![]);
        valid_transitions.insert(OrderStatus::Cancelled, vec![]);
        valid_transitions.insert(OrderStatus::Rejected, vec![]);
        valid_transitions.insert(OrderStatus::Expired, vec![]);
        
        Self { valid_transitions }
    }
    
    /// Validate order
    pub fn validate_order(&self, order: &Order) -> Result<()> {
        // Check required fields
        if order.quantity.as_i64() <= 0 {
            return Err(anyhow::anyhow!("Order quantity must be positive"));
        }
        
        // Validate order type specific requirements
        match order.order_type {
            OrderType::Limit | OrderType::StopLimit => {
                if order.price.is_none() {
                    return Err(anyhow::anyhow!("Limit order requires price"));
                }
            }
            OrderType::Stop | OrderType::StopLimit => {
                if order.stop_price.is_none() {
                    return Err(anyhow::anyhow!("Stop order requires stop price"));
                }
            }
            _ => {}
        }
        
        // Validate time in force
        if let TimeInForce::Gtt(expiry) = order.time_in_force
            && expiry <= Utc::now() {
                return Err(anyhow::anyhow!("GTT expiry must be in the future"));
            }
        
        // Check account and exchange
        if order.account.is_empty() {
            return Err(anyhow::anyhow!("Account is required"));
        }
        
        if order.exchange.is_empty() {
            return Err(anyhow::anyhow!("Exchange is required"));
        }
        
        debug!("Order {} validated successfully", order.id);
        Ok(())
    }
    
    /// Validate state transition
    pub fn validate_transition(&self, order: &Order, new_status: OrderStatus) -> Result<()> {
        let current_status = order.status;
        
        // Check if transition is valid
        if let Some(valid_next_states) = self.valid_transitions.get(&current_status)
            && valid_next_states.contains(&new_status) {
                debug!("Valid transition: {:?} -> {:?}", current_status, new_status);
                return Ok(());
            }
        
        Err(anyhow::anyhow!(
            "Invalid state transition: {:?} -> {:?}",
            current_status,
            new_status
        ))
    }
    
    /// Check if order can be cancelled
    #[must_use] pub const fn can_cancel(&self, order: &Order) -> bool {
        !order.is_terminal()
    }
    
    /// Check if order can be amended
    #[must_use] pub const fn can_amend(&self, order: &Order) -> bool {
        matches!(
            order.status,
            OrderStatus::New | OrderStatus::Pending | OrderStatus::Submitted | OrderStatus::Accepted
        )
    }
    
    /// Check if order should expire
    #[must_use] pub fn should_expire(&self, order: &Order) -> bool {
        if order.is_terminal() {
            return false;
        }
        
        match order.time_in_force {
            TimeInForce::Day => {
                // Check if order is from previous day
                let now = Utc::now();
                let order_date = order.created_at.date_naive();
                let today = now.date_naive();
                order_date < today
            }
            TimeInForce::Gtt(expiry) => {
                Utc::now() >= expiry
            }
            _ => false,
        }
    }
    
    /// Process order expiry
    pub fn expire_order(&self, order: &mut Order) -> Result<()> {
        if !self.should_expire(order) {
            return Err(anyhow::anyhow!("Order should not expire"));
        }
        
        self.validate_transition(order, OrderStatus::Expired)?;
        
        order.status = OrderStatus::Expired;
        order.updated_at = Utc::now();
        
        debug!("Order {} expired", order.id);
        Ok(())
    }
    
    /// Get next valid states
    #[must_use] pub fn get_valid_transitions(&self, status: OrderStatus) -> Vec<OrderStatus> {
        self.valid_transitions
            .get(&status)
            .cloned()
            .unwrap_or_default()
    }
    
    /// Validate parent-child relationship
    pub fn validate_parent_child(&self, parent: &Order, child: &Order) -> Result<()> {
        // Child order must reference parent
        if child.parent_order_id != Some(parent.id) {
            return Err(anyhow::anyhow!("Invalid parent-child relationship"));
        }
        
        // Parent must be an algo order
        if !matches!(parent.order_type, OrderType::Twap | OrderType::Vwap | OrderType::Pov) {
            return Err(anyhow::anyhow!("Parent must be an algorithmic order"));
        }
        
        // Child quantity cannot exceed parent remaining quantity
        if child.quantity > parent.remaining_quantity {
            return Err(anyhow::anyhow!("Child quantity exceeds parent remaining quantity"));
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use services_common::{Px, Qty, Symbol};
    use uuid::Uuid;
    
    fn create_test_order() -> Order {
        Order {
            id: Uuid::new_v4(),
            client_order_id: None,
            parent_order_id: None,
            symbol: Symbol(1),
            side: crate::order::OrderSide::Buy,
            order_type: OrderType::Limit,
            time_in_force: TimeInForce::Day,
            quantity: Qty::from_i64(10000),
            executed_quantity: Qty::ZERO,
            remaining_quantity: Qty::from_i64(10000),
            price: Some(Px::from_i64(1000000)),
            stop_price: None,
            status: OrderStatus::New,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            account: "test".to_string(),
            exchange: "binance".to_string(),
            strategy_id: None,
            tags: vec![],
            fills: vec![],
            amendments: vec![],
            version: 1,
            sequence_number: 1,
        }
    }
    
    #[test]
    fn test_valid_transitions() {
        let manager = OrderLifecycleManager::new();
        let order = create_test_order();
        
        // Valid transition
        assert!(manager.validate_transition(&order, OrderStatus::Pending).is_ok());
        
        // Invalid transition
        assert!(manager.validate_transition(&order, OrderStatus::Filled).is_err());
    }
    
    #[test]
    fn test_order_validation() {
        let manager = OrderLifecycleManager::new();
        let mut order = create_test_order();
        
        // Valid order
        assert!(manager.validate_order(&order).is_ok());
        
        // Invalid quantity
        order.quantity = Qty::ZERO;
        assert!(manager.validate_order(&order).is_err());
        
        // Missing price for limit order
        order.quantity = Qty::from_i64(10000);
        order.price = None;
        assert!(manager.validate_order(&order).is_err());
    }
    
    #[test]
    fn test_can_cancel() {
        let manager = OrderLifecycleManager::new();
        let mut order = create_test_order();
        
        // Can cancel new order
        assert!(manager.can_cancel(&order));
        
        // Cannot cancel filled order
        order.status = OrderStatus::Filled;
        assert!(!manager.can_cancel(&order));
    }
    
    #[test]
    fn test_can_amend() {
        let manager = OrderLifecycleManager::new();
        let mut order = create_test_order();
        
        // Can amend new order
        assert!(manager.can_amend(&order));
        
        // Cannot amend filled order
        order.status = OrderStatus::Filled;
        assert!(!manager.can_amend(&order));
        
        // Cannot amend partially filled order
        order.status = OrderStatus::PartiallyFilled;
        assert!(!manager.can_amend(&order));
    }
}