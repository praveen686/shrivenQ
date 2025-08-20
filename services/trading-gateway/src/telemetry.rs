//! Telemetry Collection for Trading Gateway

use anyhow::Result;
use metrics::{counter, gauge, histogram};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::info;

/// Telemetry collector for trading gateway metrics
/// 
/// The `TelemetryCollector` tracks and records various performance metrics
/// for the trading gateway, including order processing, market data updates,
/// risk checks, and signal generation. It integrates with the metrics crate
/// to provide real-time monitoring capabilities.
/// 
/// # Metrics Tracked
/// - Orderbook update counts and latencies
/// - Order submission and fill counts and latencies  
/// - Risk check counts and latencies
/// - Signal generation counts
/// - Active position and P&L gauges
/// 
/// All metrics are thread-safe using atomic operations.
pub struct TelemetryCollector {
    /// Orderbook updates
    orderbook_updates: AtomicU64,
    /// Orders submitted
    orders_submitted: AtomicU64,
    /// Orders filled
    orders_filled: AtomicU64,
    /// Risk checks performed
    risk_checks: AtomicU64,
    /// Signals generated
    signals_generated: AtomicU64,
}

impl std::fmt::Debug for TelemetryCollector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelemetryCollector")
            .field("orderbook_updates", &self.orderbook_updates.load(std::sync::atomic::Ordering::Relaxed))
            .field("orders_submitted", &self.orders_submitted.load(std::sync::atomic::Ordering::Relaxed))
            .field("orders_filled", &self.orders_filled.load(std::sync::atomic::Ordering::Relaxed))
            .field("risk_checks", &self.risk_checks.load(std::sync::atomic::Ordering::Relaxed))
            .field("signals_generated", &self.signals_generated.load(std::sync::atomic::Ordering::Relaxed))
            .finish()
    }
}

impl TelemetryCollector {
    /// Creates a new telemetry collector with initialized counters
    /// 
    /// # Returns
    /// A new `TelemetryCollector` instance with all metric counters
    /// initialized to zero. The collector is ready to start recording
    /// metrics immediately after creation.
    pub fn new() -> Self {
        Self {
            orderbook_updates: AtomicU64::new(0),
            orders_submitted: AtomicU64::new(0),
            orders_filled: AtomicU64::new(0),
            risk_checks: AtomicU64::new(0),
            signals_generated: AtomicU64::new(0),
        }
    }
    
    /// Initializes and registers all metrics with the metrics system
    /// 
    /// This method registers all counter, histogram, and gauge metrics
    /// that will be used by the telemetry collector. It should be called
    /// once during system startup before any metrics recording begins.
    /// 
    /// # Returns
    /// * `Ok(())` - If all metrics were successfully registered
    /// * `Err(anyhow::Error)` - If metric registration fails
    /// 
    /// # Metrics Registered
    /// - Counters: orderbook_updates, orders_submitted, orders_filled, risk_checks, signals_generated
    /// - Histograms: orderbook_latency, risk_check_latency, order_fill_latency
    /// - Gauges: active_positions, total_pnl
    pub async fn start(&self) -> Result<()> {
        info!("Starting telemetry collection");
        
        // Register metrics - store returned values to satisfy the must_use requirement
        let _orderbook_counter = counter!("gateway_orderbook_updates_total");
        let _orders_submitted_counter = counter!("gateway_orders_submitted_total");
        let _orders_filled_counter = counter!("gateway_orders_filled_total");
        let _risk_checks_counter = counter!("gateway_risk_checks_total");
        let _signals_counter = counter!("gateway_signals_generated_total");
        
        let _orderbook_histogram = histogram!("gateway_orderbook_latency_us");
        let _risk_check_histogram = histogram!("gateway_risk_check_latency_ns");
        let _order_fill_histogram = histogram!("gateway_order_fill_latency_us");
        
        let _positions_gauge = gauge!("gateway_active_positions");
        let _pnl_gauge = gauge!("gateway_total_pnl");
        
        Ok(())
    }
    
    /// Records an orderbook update event with latency measurement
    /// 
    /// # Arguments
    /// * `latency_us` - The latency of the orderbook update in microseconds
    /// 
    /// # Effects
    /// - Increments the orderbook update counter
    /// - Records the latency in the orderbook latency histogram
    /// - Updates both internal atomic counters and external metrics
    pub fn record_orderbook_update(&self, latency_us: u64) {
        self.orderbook_updates.fetch_add(1, Ordering::Relaxed);
        counter!("gateway_orderbook_updates_total").increment(1);
        histogram!("gateway_orderbook_latency_us").record(latency_us as f64);
    }
    
    /// Records an order submission event
    /// 
    /// # Effects
    /// - Increments the order submission counter
    /// - Updates both internal atomic counters and external metrics
    /// 
    /// This should be called every time an order is submitted to an exchange
    /// or execution venue, regardless of whether it's accepted or rejected.
    pub fn record_order_submission(&self) {
        self.orders_submitted.fetch_add(1, Ordering::Relaxed);
        counter!("gateway_orders_submitted_total").increment(1);
    }
    
    /// Records an order fill (execution) event with latency measurement
    /// 
    /// # Arguments
    /// * `latency_us` - The time from order submission to fill in microseconds
    /// 
    /// # Effects
    /// - Increments the order fill counter
    /// - Records the fill latency in the order fill latency histogram
    /// - Updates both internal atomic counters and external metrics
    pub fn record_order_fill(&self, latency_us: u64) {
        self.orders_filled.fetch_add(1, Ordering::Relaxed);
        counter!("gateway_orders_filled_total").increment(1);
        histogram!("gateway_order_fill_latency_us").record(latency_us as f64);
    }
    
    /// Records a risk check event with latency measurement
    /// 
    /// # Arguments
    /// * `latency_ns` - The duration of the risk check in nanoseconds
    /// 
    /// # Effects
    /// - Increments the risk check counter
    /// - Records the check latency in the risk check latency histogram
    /// - Updates both internal atomic counters and external metrics
    /// 
    /// This should be called for every risk validation performed,
    /// whether it passes or fails the risk criteria.
    pub fn record_risk_check(&self, latency_ns: u64) {
        self.risk_checks.fetch_add(1, Ordering::Relaxed);
        counter!("gateway_risk_checks_total").increment(1);
        histogram!("gateway_risk_check_latency_ns").record(latency_ns as f64);
    }
    
    /// Records a trading signal generation event
    /// 
    /// # Effects
    /// - Increments the signal generation counter
    /// - Updates both internal atomic counters and external metrics
    /// 
    /// This should be called every time a trading strategy generates
    /// a signal, regardless of whether it results in an order.
    pub fn record_signal(&self) {
        self.signals_generated.fetch_add(1, Ordering::Relaxed);
        counter!("gateway_signals_generated_total").increment(1);
    }
    
    /// Updates the active positions gauge metric
    /// 
    /// # Arguments
    /// * `count` - The current number of active positions
    /// 
    /// # Effects
    /// Updates the active positions gauge to reflect the current
    /// number of open positions across all symbols and strategies.
    pub fn update_positions(&self, count: usize) {
        gauge!("gateway_active_positions").set(count as f64);
    }
    
    /// Updates the total profit and loss gauge metric
    /// 
    /// # Arguments
    /// * `pnl` - The total P&L in the base currency's smallest unit (e.g., cents)
    /// 
    /// # Effects
    /// Updates the total P&L gauge, converting from integer representation
    /// to decimal format (dividing by 10,000 for 4-decimal precision).
    pub fn update_pnl(&self, pnl: i64) {
        gauge!("gateway_total_pnl").set(pnl as f64 / 10000.0);
    }
    
    /// Retrieves a snapshot of current telemetry statistics
    /// 
    /// # Returns
    /// A `TelemetryStats` struct containing current values of all
    /// tracked counters. The values represent cumulative counts
    /// since the collector was initialized.
    /// 
    /// # Thread Safety
    /// This method uses relaxed atomic ordering for performance,
    /// as exact consistency between counters is not critical for
    /// monitoring purposes.
    pub async fn get_stats(&self) -> TelemetryStats {
        TelemetryStats {
            orderbook_updates: self.orderbook_updates.load(Ordering::Relaxed),
            orders_submitted: self.orders_submitted.load(Ordering::Relaxed),
            orders_filled: self.orders_filled.load(Ordering::Relaxed),
            risk_checks: self.risk_checks.load(Ordering::Relaxed),
            signals_generated: self.signals_generated.load(Ordering::Relaxed),
        }
    }
}

/// Telemetry statistics snapshot
/// 
/// Contains a point-in-time snapshot of all telemetry counters tracked
/// by the `TelemetryCollector`. This structure provides a consistent
/// view of system performance metrics that can be used for monitoring,
/// reporting, and debugging purposes.
/// 
/// # Fields
/// All fields represent cumulative counts since the collector was initialized.
#[derive(Debug, Clone)]
pub struct TelemetryStats {
    /// Orderbook updates
    pub orderbook_updates: u64,
    /// Orders submitted
    pub orders_submitted: u64,
    /// Orders filled
    pub orders_filled: u64,
    /// Risk checks performed
    pub risk_checks: u64,
    /// Signals generated
    pub signals_generated: u64,
}