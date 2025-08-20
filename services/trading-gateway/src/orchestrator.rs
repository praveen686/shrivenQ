//! Orchestrator - Core coordination logic

use crate::TradingEvent;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info};

/// Core orchestrator for event coordination and distribution
/// 
/// The `Orchestrator` serves as the central coordination hub for all trading events
/// within the gateway. It receives events from various sources (market data, strategies,
/// execution reports) and distributes them to all registered listeners via a broadcast
/// channel. This enables loose coupling between system components while maintaining
/// event ordering and delivery guarantees.
/// 
/// # Architecture
/// - Uses tokio broadcast channels for event distribution
/// - Maintains event processing statistics
/// - Provides centralized logging for all event types
/// - Thread-safe with atomic counters for performance metrics
pub struct Orchestrator {
    /// Event bus
    event_bus: Arc<broadcast::Sender<TradingEvent>>,
    /// Event counter
    events_processed: std::sync::atomic::AtomicU64,
}

impl std::fmt::Debug for Orchestrator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Orchestrator")
            .field("event_bus_receiver_count", &self.event_bus.receiver_count())
            .field("events_processed", &self.events_processed.load(std::sync::atomic::Ordering::Relaxed))
            .finish()
    }
}

impl Orchestrator {
    /// Creates a new orchestrator with the specified event bus
    /// 
    /// # Arguments
    /// * `event_bus` - Shared broadcast sender for distributing trading events
    /// 
    /// # Returns
    /// A new `Orchestrator` instance ready to process and distribute events
    /// 
    /// The orchestrator will use the provided event bus to broadcast all
    /// processed events to subscribed listeners throughout the system.
    pub fn new(event_bus: Arc<broadcast::Sender<TradingEvent>>) -> Self {
        Self {
            event_bus,
            events_processed: std::sync::atomic::AtomicU64::new(0),
        }
    }
    
    /// Processes a trading event through the orchestration pipeline
    /// 
    /// This method handles all types of trading events, performs appropriate
    /// logging based on event type and severity, and broadcasts the event
    /// to all registered listeners via the event bus.
    /// 
    /// # Arguments
    /// * `event` - The trading event to process and distribute
    /// 
    /// # Returns
    /// * `Ok(())` - If the event was successfully processed and broadcast
    /// * `Err(anyhow::Error)` - If event processing fails
    /// 
    /// # Event Processing
    /// - Market updates: Debug level logging
    /// - Signals: Info level logging with signal type
    /// - Order requests: Info level logging
    /// - Execution reports: Info level logging with status
    /// - Risk alerts: Error level logging with severity
    /// 
    /// All events are broadcast regardless of processing result,
    /// but broadcast failures are silently ignored to prevent blocking.
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
    
    /// Returns the total number of events processed by this orchestrator
    /// 
    /// # Returns
    /// The cumulative count of all events that have been processed
    /// since the orchestrator was created. This includes all event
    /// types (market updates, signals, orders, executions, alerts).
    /// 
    /// # Thread Safety
    /// Uses relaxed atomic ordering for optimal performance in
    /// high-frequency trading scenarios.
    pub fn get_events_processed(&self) -> u64 {
        self.events_processed.load(std::sync::atomic::Ordering::Relaxed)
    }
}