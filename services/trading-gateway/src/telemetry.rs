//! Telemetry Collection for Trading Gateway

use anyhow::Result;
use metrics::{counter, gauge, histogram};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::info;

/// Telemetry collector
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

impl TelemetryCollector {
    /// Create new telemetry collector
    pub fn new() -> Self {
        Self {
            orderbook_updates: AtomicU64::new(0),
            orders_submitted: AtomicU64::new(0),
            orders_filled: AtomicU64::new(0),
            risk_checks: AtomicU64::new(0),
            signals_generated: AtomicU64::new(0),
        }
    }
    
    /// Start telemetry collection
    pub async fn start(&self) -> Result<()> {
        info!("Starting telemetry collection");
        
        // Register metrics
        counter!("gateway_orderbook_updates_total");
        counter!("gateway_orders_submitted_total");
        counter!("gateway_orders_filled_total");
        counter!("gateway_risk_checks_total");
        counter!("gateway_signals_generated_total");
        
        histogram!("gateway_orderbook_latency_us");
        histogram!("gateway_risk_check_latency_ns");
        histogram!("gateway_order_fill_latency_us");
        
        gauge!("gateway_active_positions");
        gauge!("gateway_total_pnl");
        
        Ok(())
    }
    
    /// Record orderbook update
    pub fn record_orderbook_update(&self, latency_us: u64) {
        self.orderbook_updates.fetch_add(1, Ordering::Relaxed);
        counter!("gateway_orderbook_updates_total").increment(1);
        histogram!("gateway_orderbook_latency_us").record(latency_us as f64);
    }
    
    /// Record order submission
    pub fn record_order_submission(&self) {
        self.orders_submitted.fetch_add(1, Ordering::Relaxed);
        counter!("gateway_orders_submitted_total").increment(1);
    }
    
    /// Record order fill
    pub fn record_order_fill(&self, latency_us: u64) {
        self.orders_filled.fetch_add(1, Ordering::Relaxed);
        counter!("gateway_orders_filled_total").increment(1);
        histogram!("gateway_order_fill_latency_us").record(latency_us as f64);
    }
    
    /// Record risk check
    pub fn record_risk_check(&self, latency_ns: u64) {
        self.risk_checks.fetch_add(1, Ordering::Relaxed);
        counter!("gateway_risk_checks_total").increment(1);
        histogram!("gateway_risk_check_latency_ns").record(latency_ns as f64);
    }
    
    /// Record signal generation
    pub fn record_signal(&self) {
        self.signals_generated.fetch_add(1, Ordering::Relaxed);
        counter!("gateway_signals_generated_total").increment(1);
    }
    
    /// Update position gauge
    pub fn update_positions(&self, count: usize) {
        gauge!("gateway_active_positions").set(count as f64);
    }
    
    /// Update P&L gauge
    pub fn update_pnl(&self, pnl: i64) {
        gauge!("gateway_total_pnl").set(pnl as f64 / 10000.0);
    }
    
    /// Get telemetry statistics
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

/// Telemetry statistics
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