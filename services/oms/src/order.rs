//! Order definitions and structures

use chrono::{DateTime, Utc};
use services_common::{Px, Qty, Symbol};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Order structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    /// Unique order ID
    pub id: Uuid,
    /// Client order ID (optional)
    pub client_order_id: Option<String>,
    /// Parent order ID for child orders
    pub parent_order_id: Option<Uuid>,
    /// Symbol
    pub symbol: Symbol,
    /// Order side
    pub side: OrderSide,
    /// Order type
    pub order_type: OrderType,
    /// Time in force
    pub time_in_force: TimeInForce,
    /// Original quantity
    pub quantity: Qty,
    /// Executed quantity
    pub executed_quantity: Qty,
    /// Remaining quantity
    pub remaining_quantity: Qty,
    /// Limit price (for limit orders)
    pub price: Option<Px>,
    /// Stop price (for stop orders)
    pub stop_price: Option<Px>,
    /// Order status
    pub status: OrderStatus,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// Account
    pub account: String,
    /// Exchange
    pub exchange: String,
    /// Strategy ID
    pub strategy_id: Option<String>,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Fills
    pub fills: Vec<Fill>,
    /// Amendments
    pub amendments: Vec<Amendment>,
    /// Version number
    pub version: u32,
    /// Sequence number
    pub sequence_number: u64,
}

/// Order request for creating new orders
#[derive(Debug, Clone)]
pub struct OrderRequest {
    /// Client order ID
    pub client_order_id: Option<String>,
    /// Parent order ID
    pub parent_order_id: Option<Uuid>,
    /// Symbol
    pub symbol: Symbol,
    /// Side
    pub side: OrderSide,
    /// Order type
    pub order_type: OrderType,
    /// Time in force
    pub time_in_force: TimeInForce,
    /// Quantity
    pub quantity: Qty,
    /// Price
    pub price: Option<Px>,
    /// Stop price
    pub stop_price: Option<Px>,
    /// Account
    pub account: String,
    /// Exchange
    pub exchange: String,
    /// Strategy ID
    pub strategy_id: Option<String>,
    /// Tags
    pub tags: Vec<String>,
}

/// Order side
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    /// Buy order
    Buy,
    /// Sell order
    Sell,
}

/// Order type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    /// Market order
    Market,
    /// Limit order
    Limit,
    /// Stop order
    Stop,
    /// Stop limit order
    StopLimit,
    /// Iceberg order
    Iceberg,
    /// TWAP algorithm
    Twap,
    /// VWAP algorithm
    Vwap,
    /// POV algorithm
    Pov,
}

/// Time in force
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeInForce {
    /// Good till cancelled
    Gtc,
    /// Immediate or cancel
    Ioc,
    /// Fill or kill
    Fok,
    /// Good for day
    Day,
    /// Good till time
    Gtt(DateTime<Utc>),
}

/// Order status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderStatus {
    /// New order created
    New,
    /// Pending submission
    Pending,
    /// Submitted to exchange
    Submitted,
    /// Accepted by exchange
    Accepted,
    /// Partially filled
    PartiallyFilled,
    /// Fully filled
    Filled,
    /// Cancelled
    Cancelled,
    /// Rejected
    Rejected,
    /// Expired
    Expired,
}

/// Fill information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    /// Fill ID
    pub id: Uuid,
    /// Order ID
    pub order_id: Uuid,
    /// Execution ID from exchange
    pub execution_id: String,
    /// Fill quantity
    pub quantity: Qty,
    /// Fill price
    pub price: Px,
    /// Commission
    pub commission: i64,
    /// Commission currency
    pub commission_currency: String,
    /// Fill timestamp
    pub timestamp: DateTime<Utc>,
    /// Liquidity indicator (maker/taker)
    pub liquidity: LiquidityIndicator,
}

/// Liquidity indicator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiquidityIndicator {
    /// Added liquidity (maker)
    Maker,
    /// Removed liquidity (taker)
    Taker,
}

/// Order amendment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Amendment {
    /// Amendment ID
    pub id: Uuid,
    /// Order ID
    pub order_id: Uuid,
    /// New quantity
    pub new_quantity: Option<Qty>,
    /// New price
    pub new_price: Option<Px>,
    /// Amendment reason
    pub reason: String,
    /// Amendment timestamp
    pub timestamp: DateTime<Utc>,
}

impl Order {
    /// Calculate average fill price
    #[must_use] pub fn average_fill_price(&self) -> Option<Px> {
        if self.fills.is_empty() {
            return None;
        }
        
        let total_value: i64 = self.fills
            .iter()
            .map(|f| f.price.as_i64() * f.quantity.as_i64())
            .sum();
            
        let total_quantity: i64 = self.fills
            .iter()
            .map(|f| f.quantity.as_i64())
            .sum();
            
        if total_quantity > 0 {
            Some(Px::from_i64(total_value / total_quantity))
        } else {
            None
        }
    }
    
    /// Calculate total commission
    #[must_use] pub fn total_commission(&self) -> i64 {
        self.fills.iter().map(|f| f.commission).sum()
    }
    
    /// Check if order is active
    #[must_use] pub const fn is_active(&self) -> bool {
        matches!(self.status, 
            OrderStatus::New | 
            OrderStatus::Pending | 
            OrderStatus::Submitted | 
            OrderStatus::Accepted | 
            OrderStatus::PartiallyFilled
        )
    }
    
    /// Check if order is terminal
    #[must_use] pub const fn is_terminal(&self) -> bool {
        matches!(self.status,
            OrderStatus::Filled |
            OrderStatus::Cancelled |
            OrderStatus::Rejected |
            OrderStatus::Expired
        )
    }
    
    /// Get fill rate
    #[must_use] pub fn fill_rate(&self) -> f64 {
        if self.quantity.as_i64() == 0 {
            return 0.0;
        }
        (self.executed_quantity.as_i64() as f64 / self.quantity.as_i64() as f64) * 100.0
    }
}