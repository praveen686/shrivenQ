//! Execution algorithms (TWAP, VWAP, Iceberg, etc.)

use crate::OrderRequest;
use chrono::{DateTime, Duration, Utc};
use services_common::{Px, Qty};
use serde::{Deserialize, Serialize};

/// Algorithm type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlgorithmType {
    /// Time-weighted average price
    TWAP,
    /// Volume-weighted average price
    VWAP,
    /// Iceberg (show only partial quantity)
    Iceberg,
    /// Percentage of volume
    POV,
    /// Implementation shortfall
    IS,
    /// Arrival price
    ArrivalPrice,
}

/// Algorithm parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgorithmParams {
    /// Algorithm type
    pub algo_type: AlgorithmType,
    /// Start time
    pub start_time: DateTime<Utc>,
    /// End time
    pub end_time: DateTime<Utc>,
    /// Maximum participation rate (fixed-point: 1000 = 10%)
    pub max_participation_rate: i32,
    /// Minimum order size
    pub min_order_size: Qty,
    /// Maximum order size
    pub max_order_size: Qty,
    /// Price limit
    pub price_limit: Option<Px>,
    /// Urgency level (1-10)
    pub urgency: u8,
}

/// Algorithm state
#[derive(Debug, Clone)]
pub struct AlgorithmState {
    /// Parent order
    pub parent_order: OrderRequest,
    /// Algorithm parameters
    pub params: AlgorithmParams,
    /// Executed quantity
    pub executed_qty: Qty,
    /// Remaining quantity
    pub remaining_qty: Qty,
    /// Child orders
    pub child_orders: Vec<OrderRequest>,
    /// Started flag
    pub started: bool,
    /// Completed flag
    pub completed: bool,
}

/// TWAP algorithm - splits order evenly over time
#[derive(Debug)]
pub struct TwapAlgorithm {
    state: AlgorithmState,
    _slice_interval: Duration, // Reserved for next slice timing
    slices_sent: u32,
    total_slices: u32,
}

impl TwapAlgorithm {
    /// Create new TWAP algorithm
    #[must_use] pub fn new(parent_order: OrderRequest, params: AlgorithmParams) -> Self {
        let duration = params.end_time - params.start_time;
        let total_slices = u32::try_from((duration.num_seconds() / 60).max(1)).unwrap_or(u32::MAX);
        let slice_interval = duration / i32::try_from(total_slices).unwrap_or(i32::MAX);

        let state = AlgorithmState {
            remaining_qty: parent_order.quantity,
            executed_qty: Qty::ZERO,
            parent_order,
            params,
            child_orders: Vec::new(),
            started: false,
            completed: false,
        };

        Self {
            state,
            _slice_interval: slice_interval,
            slices_sent: 0,
            total_slices,
        }
    }

    /// Get next slice
    pub fn get_next_slice(&mut self) -> Option<OrderRequest> {
        if self.state.completed || self.slices_sent >= self.total_slices {
            return None;
        }

        let now = Utc::now();
        if now < self.state.params.start_time {
            return None;
        }

        // Calculate slice size
        let remaining_slices = self.total_slices - self.slices_sent;
        let slice_qty = if remaining_slices > 0 {
            Qty::from_i64(self.state.remaining_qty.as_i64() / i64::from(remaining_slices))
        } else {
            self.state.remaining_qty
        };

        // Ensure within min/max bounds
        let slice_qty = slice_qty
            .max(self.state.params.min_order_size)
            .min(self.state.params.max_order_size);

        if slice_qty == Qty::ZERO {
            self.state.completed = true;
            return None;
        }

        // Create child order
        let mut child = self.state.parent_order.clone();
        child.quantity = slice_qty;
        child.client_order_id = format!("{}_twap_{}", child.client_order_id, self.slices_sent);

        self.slices_sent += 1;
        self.state.remaining_qty =
            Qty::from_i64(self.state.remaining_qty.as_i64() - slice_qty.as_i64());
        self.state.child_orders.push(child.clone());

        Some(child)
    }
}

/// VWAP algorithm - matches market volume distribution
#[derive(Debug)]
pub struct VwapAlgorithm {
    state: AlgorithmState,
    _volume_curve: Vec<i32>, // Reserved for volume-weighted distribution
}

impl VwapAlgorithm {
    /// Create new VWAP algorithm
    #[must_use] pub fn new(parent_order: OrderRequest, params: AlgorithmParams) -> Self {
        // Default volume curve (U-shaped for typical trading day)
        let volume_curve = vec![
            150, 120, 100, 80, 70, 60, 50, 45, 40, 35, // Morning
            30, 28, 25, 25, 25, 25, 25, 28, 30, 35, // Midday
            40, 45, 50, 60, 70, 80, 100, 120, 150, 200, // Afternoon
        ];

        let state = AlgorithmState {
            remaining_qty: parent_order.quantity,
            executed_qty: Qty::ZERO,
            parent_order,
            params,
            child_orders: Vec::new(),
            started: false,
            completed: false,
        };

        Self {
            state,
            _volume_curve: volume_curve,
        }
    }

    /// Get next slice based on volume
    pub fn get_next_slice(&mut self, market_volume: Qty) -> Option<OrderRequest> {
        if self.state.completed {
            return None;
        }

        let now = Utc::now();
        if now < self.state.params.start_time || now > self.state.params.end_time {
            return None;
        }

        // Calculate participation
        let participation_qty = Qty::from_i64(
            market_volume
                .as_i64()
                .saturating_mul(i64::from(self.state.params.max_participation_rate))
                / 10000,
        );

        let slice_qty = participation_qty
            .min(self.state.remaining_qty)
            .max(self.state.params.min_order_size)
            .min(self.state.params.max_order_size);

        if slice_qty == Qty::ZERO {
            self.state.completed = true;
            return None;
        }

        // Create child order
        let mut child = self.state.parent_order.clone();
        child.quantity = slice_qty;
        child.client_order_id = format!(
            "{}_vwap_{}",
            child.client_order_id,
            self.state.child_orders.len()
        );

        self.state.remaining_qty =
            Qty::from_i64(self.state.remaining_qty.as_i64() - slice_qty.as_i64());
        self.state.child_orders.push(child.clone());

        Some(child)
    }
}

/// Iceberg algorithm - shows only partial quantity
#[derive(Debug)]
pub struct IcebergAlgorithm {
    state: AlgorithmState,
    display_qty: Qty,
    refresh_qty: Qty,
}

impl IcebergAlgorithm {
    /// Create new iceberg algorithm
    #[must_use] pub const fn new(parent_order: OrderRequest, params: AlgorithmParams, display_qty: Qty) -> Self {
        let state = AlgorithmState {
            remaining_qty: parent_order.quantity,
            executed_qty: Qty::ZERO,
            parent_order,
            params,
            child_orders: Vec::new(),
            started: false,
            completed: false,
        };

        Self {
            state,
            display_qty,
            refresh_qty: display_qty,
        }
    }

    /// Get next visible slice
    pub fn get_next_slice(&mut self) -> Option<OrderRequest> {
        if self.state.completed || self.state.remaining_qty == Qty::ZERO {
            self.state.completed = true;
            return None;
        }

        let slice_qty = self.display_qty.min(self.state.remaining_qty);

        // Create child order
        let mut child = self.state.parent_order.clone();
        child.quantity = slice_qty;
        child.client_order_id = format!(
            "{}_iceberg_{}",
            child.client_order_id,
            self.state.child_orders.len()
        );

        self.state.remaining_qty =
            Qty::from_i64(self.state.remaining_qty.as_i64() - slice_qty.as_i64());
        self.state.child_orders.push(child.clone());

        Some(child)
    }

    /// Handle fill and refresh
    pub fn on_fill(&mut self, filled_qty: Qty) {
        self.state.executed_qty =
            Qty::from_i64(self.state.executed_qty.as_i64() + filled_qty.as_i64());
        self.refresh_qty = Qty::from_i64(self.refresh_qty.as_i64() - filled_qty.as_i64());

        if self.refresh_qty == Qty::ZERO {
            self.refresh_qty = self.display_qty;
        }
    }
}
