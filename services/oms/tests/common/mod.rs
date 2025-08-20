//! Common test utilities and fixtures

use chrono::Utc;
use services_common::{Px, Qty, Symbol};
use uuid::Uuid;

use oms::order::{Order, OrderRequest, OrderSide, OrderStatus, OrderType, TimeInForce, Fill, Amendment, LiquidityIndicator};
use oms::OmsConfig;

/// Create a standard test configuration
pub fn create_test_config() -> OmsConfig {
    OmsConfig {
        database_url: "postgresql://test:test@localhost/test_oms".to_string(),
        max_orders_memory: 10000,
        retention_days: 7,
        enable_audit: true,
        enable_matching: true,
        persist_batch_size: 100,
    }
}

/// Create a test configuration without external dependencies
pub fn create_isolated_config() -> OmsConfig {
    OmsConfig {
        database_url: "postgresql://test:test@localhost/test_oms_isolated".to_string(),
        max_orders_memory: 1000,
        retention_days: 1,
        enable_audit: false, // Disable for isolated testing
        enable_matching: false, // Disable for isolated testing
        persist_batch_size: 50,
    }
}

/// Create a standard test order request
pub fn create_test_order_request(id: usize) -> OrderRequest {
    OrderRequest {
        client_order_id: Some(format!("TEST-{:06}", id)),
        parent_order_id: None,
        symbol: Symbol((id % 5) as u32 + 1),
        side: if id % 2 == 0 { OrderSide::Buy } else { OrderSide::Sell },
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Day,
        quantity: Qty::from_i64(1000 + (id % 10000) as i64),
        price: Some(Px::from_i64(1_000_000 + (id as i64 * 1000))),
        stop_price: None,
        account: format!("test_account_{}", id % 10),
        exchange: "test_exchange".to_string(),
        strategy_id: Some("test_strategy".to_string()),
        tags: vec!["test".to_string(), "generated".to_string()],
    }
}

/// Create a market order request
pub fn create_market_order_request(id: usize) -> OrderRequest {
    OrderRequest {
        client_order_id: Some(format!("MARKET-{:06}", id)),
        parent_order_id: None,
        symbol: Symbol(1),
        side: if id % 2 == 0 { OrderSide::Buy } else { OrderSide::Sell },
        order_type: OrderType::Market,
        time_in_force: TimeInForce::Ioc,
        quantity: Qty::from_i64(1000 + (id % 5000) as i64),
        price: None,
        stop_price: None,
        account: format!("market_account_{}", id % 5),
        exchange: "test_exchange".to_string(),
        strategy_id: Some("market_strategy".to_string()),
        tags: vec!["market".to_string(), "test".to_string()],
    }
}

/// Create a test order with specific parameters
pub fn create_test_order(
    side: OrderSide,
    order_type: OrderType,
    quantity: i64,
    price: Option<i64>,
    sequence: u64,
) -> Order {
    Order {
        id: Uuid::new_v4(),
        client_order_id: Some(format!("CUSTOM-{}", sequence)),
        parent_order_id: None,
        symbol: Symbol(1),
        side,
        order_type,
        time_in_force: TimeInForce::Day,
        quantity: Qty::from_i64(quantity),
        executed_quantity: Qty::ZERO,
        remaining_quantity: Qty::from_i64(quantity),
        price: price.map(Px::from_i64),
        stop_price: None,
        status: OrderStatus::New,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        account: "test_account".to_string(),
        exchange: "test_exchange".to_string(),
        strategy_id: Some("test_strategy".to_string()),
        tags: vec!["custom".to_string(), "test".to_string()],
        fills: vec![],
        amendments: vec![],
        version: 1,
        sequence_number: sequence,
    }
}

/// Create a test fill for an order
pub fn create_test_fill(order: &Order, quantity: i64, price: i64) -> Fill {
    Fill {
        id: Uuid::new_v4(),
        order_id: order.id,
        execution_id: format!("EXEC-{}", chrono::Utc::now().timestamp_nanos()),
        quantity: Qty::from_i64(quantity),
        price: Px::from_i64(price),
        commission: (quantity * price) / 10000, // 0.01% commission
        commission_currency: "USDT".to_string(),
        timestamp: Utc::now(),
        liquidity: LiquidityIndicator::Maker,
    }
}

/// Create a test amendment for an order
pub fn create_test_amendment(order: &Order, new_quantity: Option<i64>, new_price: Option<i64>) -> Amendment {
    Amendment {
        id: Uuid::new_v4(),
        order_id: order.id,
        new_quantity: new_quantity.map(Qty::from_i64),
        new_price: new_price.map(Px::from_i64),
        reason: "Test amendment".to_string(),
        timestamp: Utc::now(),
    }
}

/// Test data generators for property-based testing
pub mod generators {
    use super::*;
    use proptest::prelude::*;
    
    /// Generate valid quantities
    pub fn valid_quantity() -> impl Strategy<Value = i64> {
        1i64..=i64::MAX / 1000 // Reasonable range to avoid overflow
    }
    
    /// Generate valid prices
    pub fn valid_price() -> impl Strategy<Value = i64> {
        1i64..=i64::MAX / 1000
    }
    
    /// Generate order sides
    pub fn order_side() -> impl Strategy<Value = OrderSide> {
        prop::sample::select(vec![OrderSide::Buy, OrderSide::Sell])
    }
    
    /// Generate order types
    pub fn order_type() -> impl Strategy<Value = OrderType> {
        prop::sample::select(vec![
            OrderType::Market,
            OrderType::Limit,
            OrderType::Stop,
            OrderType::StopLimit,
            OrderType::Twap,
            OrderType::Vwap,
            OrderType::Pov,
        ])
    }
    
    /// Generate time in force values
    pub fn time_in_force() -> impl Strategy<Value = TimeInForce> {
        prop::sample::select(vec![
            TimeInForce::Gtc,
            TimeInForce::Ioc,
            TimeInForce::Fok,
            TimeInForce::Day,
        ])
    }
    
    /// Generate order statuses
    pub fn order_status() -> impl Strategy<Value = OrderStatus> {
        prop::sample::select(vec![
            OrderStatus::New,
            OrderStatus::Pending,
            OrderStatus::Submitted,
            OrderStatus::Accepted,
            OrderStatus::PartiallyFilled,
            OrderStatus::Filled,
            OrderStatus::Cancelled,
            OrderStatus::Rejected,
            OrderStatus::Expired,
        ])
    }
}

/// Test assertions and validation helpers
pub mod assertions {
    use super::*;
    
    /// Assert that an order maintains quantity invariants
    pub fn assert_quantity_invariants(order: &Order) {
        assert_eq!(
            order.executed_quantity.as_i64() + order.remaining_quantity.as_i64(),
            order.quantity.as_i64(),
            "Quantity invariant violated: executed + remaining != total"
        );
        
        assert!(
            order.executed_quantity.as_i64() >= 0,
            "Executed quantity cannot be negative"
        );
        
        assert!(
            order.remaining_quantity.as_i64() >= 0,
            "Remaining quantity cannot be negative"
        );
        
        assert!(
            order.quantity.as_i64() > 0,
            "Total quantity must be positive"
        );
    }
    
    /// Assert that an order is in a valid state
    pub fn assert_valid_order_state(order: &Order) {
        assert_quantity_invariants(order);
        
        assert!(order.version >= 1, "Version must be at least 1");
        assert!(order.sequence_number >= 1, "Sequence number must be at least 1");
        assert!(order.created_at <= order.updated_at, "Created time must be <= updated time");
        assert!(!order.account.is_empty(), "Account cannot be empty");
        assert!(!order.exchange.is_empty(), "Exchange cannot be empty");
        
        // Status-specific validations
        match order.status {
            OrderStatus::New => {
                assert_eq!(order.executed_quantity, Qty::ZERO, "New order should have no execution");
                assert_eq!(order.fills.len(), 0, "New order should have no fills");
            }
            OrderStatus::Filled => {
                assert_eq!(order.executed_quantity, order.quantity, "Filled order should be fully executed");
                assert_eq!(order.remaining_quantity, Qty::ZERO, "Filled order should have no remaining quantity");
                assert!(!order.fills.is_empty(), "Filled order should have fills");
            }
            OrderStatus::PartiallyFilled => {
                assert!(order.executed_quantity.as_i64() > 0, "Partially filled order should have some execution");
                assert!(order.remaining_quantity.as_i64() > 0, "Partially filled order should have remaining quantity");
                assert!(!order.fills.is_empty(), "Partially filled order should have fills");
            }
            _ => {} // Other statuses have fewer constraints
        }
        
        // Order type specific validations
        match order.order_type {
            OrderType::Limit | OrderType::StopLimit => {
                assert!(order.price.is_some(), "Limit orders must have price");
            }
            OrderType::Stop | OrderType::StopLimit => {
                assert!(order.stop_price.is_some(), "Stop orders must have stop price");
            }
            _ => {}
        }
    }
    
    /// Assert that a fill is valid for an order
    pub fn assert_valid_fill(order: &Order, fill: &Fill) {
        assert_eq!(fill.order_id, order.id, "Fill order ID must match");
        assert!(fill.quantity.as_i64() > 0, "Fill quantity must be positive");
        assert!(fill.price.as_i64() > 0, "Fill price must be positive");
        assert!(!fill.execution_id.is_empty(), "Fill must have execution ID");
        assert!(!fill.commission_currency.is_empty(), "Fill must have commission currency");
    }
}

/// Mock implementations for testing
pub mod mocks {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::collections::HashMap;
    
    /// Mock order storage for testing without database
    pub struct MockOrderStorage {
        orders: Arc<Mutex<HashMap<Uuid, Order>>>,
        sequence: Arc<Mutex<u64>>,
    }
    
    impl MockOrderStorage {
        pub fn new() -> Self {
            Self {
                orders: Arc::new(Mutex::new(HashMap::new())),
                sequence: Arc::new(Mutex::new(1)),
            }
        }
        
        pub fn store_order(&self, mut order: Order) -> Order {
            let mut sequence = self.sequence.lock().unwrap();
            order.sequence_number = *sequence;
            *sequence += 1;
            
            self.orders.lock().unwrap().insert(order.id, order.clone());
            order
        }
        
        pub fn get_order(&self, id: &Uuid) -> Option<Order> {
            self.orders.lock().unwrap().get(id).cloned()
        }
        
        pub fn get_all_orders(&self) -> Vec<Order> {
            self.orders.lock().unwrap().values().cloned().collect()
        }
        
        pub fn update_order(&self, order: Order) {
            self.orders.lock().unwrap().insert(order.id, order);
        }
        
        pub fn remove_order(&self, id: &Uuid) -> Option<Order> {
            self.orders.lock().unwrap().remove(id)
        }
        
        pub fn clear(&self) {
            self.orders.lock().unwrap().clear();
        }
    }
    
    impl Default for MockOrderStorage {
        fn default() -> Self {
            Self::new()
        }
    }
}