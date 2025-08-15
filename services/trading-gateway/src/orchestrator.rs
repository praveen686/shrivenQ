//! Orchestrator - Core coordination logic

use crate::TradingEvent;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info};

/// Core orchestrator for event coordination
pub struct Orchestrator {
    /// Event bus
    event_bus: Arc<broadcast::Sender<TradingEvent>>,
    /// Event counter
    events_processed: std::sync::atomic::AtomicU64,
}

impl Orchestrator {
    /// Create new orchestrator
    pub fn new(event_bus: Arc<broadcast::Sender<TradingEvent>>) -> Self {
        Self {
            event_bus,
            events_processed: std::sync::atomic::AtomicU64::new(0),
        }
    }
    
    /// Process event through orchestration logic
    pub async fn process_event(&self, event: TradingEvent) -> Result<()> {
        self.events_processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        match &event {
            TradingEvent::MarketUpdate { symbol, .. } => {
                debug!("Processing market update for {}", symbol);
            }
            TradingEvent::Signal { symbol, signal_type, .. } => {
                info!("Processing signal for {} type {:?}", symbol, signal_type);
            }
            TradingEvent::OrderRequest { symbol, .. } => {
                info!("Processing order request for {}", symbol);
            }
            TradingEvent::ExecutionReport { order_id, status, .. } => {
                info!("Execution report for order {} status {:?}", order_id, status);
            }
            TradingEvent::RiskAlert { severity, message, .. } => {
                error!("Risk alert [{:?}]: {}", severity, message);
            }
        }
        
        // Broadcast event to all listeners
        let _ = self.event_bus.send(event);
        
        Ok(())
    }
    
    /// Get events processed count
    pub fn get_events_processed(&self) -> u64 {
        self.events_processed.load(std::sync::atomic::Ordering::Relaxed)
    }
}