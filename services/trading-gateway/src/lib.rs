//! Trading Gateway - World-Class Trading Orchestrator
//! 
//! This is the nerve center of ShrivenQuant - a sophisticated orchestration layer
//! that coordinates all trading components with institutional-grade reliability.
//!
//! Architecture inspired by leading HFT firms:
//! - Jane Street's OCaml trading systems
//! - Jump Trading's C++ infrastructure
//! - Citadel's distributed architecture

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use anyhow::Result;
use async_trait::async_trait;
use common::{Px, Qty, Symbol, Ts};
use crossbeam::channel::{bounded, Receiver, Sender};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tracing::{debug, error, info, warn};

pub mod orchestrator;
pub mod state;
pub mod strategy;
pub mod risk_gate;
pub mod execution_engine;
pub mod market_maker;
pub mod signal_aggregator;
pub mod position_manager;
pub mod telemetry;

/// Core trading event types
#[derive(Debug, Clone)]
pub enum TradingEvent {
    /// Market data update from orderbook
    MarketUpdate {
        symbol: Symbol,
        bid: Option<(Px, Qty)>,
        ask: Option<(Px, Qty)>,
        mid: Px,
        spread: i64,
        imbalance: f64,
        vpin: f64,
        kyles_lambda: f64,
        timestamp: Ts,
    },
    /// Trading signal from strategy
    Signal {
        id: u64,
        symbol: Symbol,
        side: Side,
        signal_type: SignalType,
        strength: f64,
        confidence: f64,
        timestamp: Ts,
    },
    /// Order request
    OrderRequest {
        id: u64,
        symbol: Symbol,
        side: Side,
        order_type: OrderType,
        quantity: Qty,
        price: Option<Px>,
        time_in_force: TimeInForce,
        strategy_id: String,
    },
    /// Execution report
    ExecutionReport {
        order_id: u64,
        symbol: Symbol,
        side: Side,
        executed_qty: Qty,
        executed_price: Px,
        remaining_qty: Qty,
        status: OrderStatus,
        timestamp: Ts,
    },
    /// Risk alert
    RiskAlert {
        severity: Severity,
        message: String,
        action: RiskAction,
        timestamp: Ts,
    },
}

/// Order side
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Buy order
    Buy,
    /// Sell order
    Sell,
}

/// Signal type from strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalType {
    /// Momentum signal
    Momentum,
    /// Mean reversion signal
    MeanReversion,
    /// Market making signal
    MarketMaking,
    /// Arbitrage opportunity
    Arbitrage,
    /// Flow toxicity signal
    ToxicFlow,
    /// Microstructure signal
    Microstructure,
}

/// Order type
#[derive(Debug, Clone, Copy)]
pub enum OrderType {
    /// Market order
    Market,
    /// Limit order
    Limit,
    /// Stop order
    Stop,
    /// Iceberg order
    Iceberg,
    /// TWAP order
    Twap,
    /// VWAP order
    Vwap,
}

/// Time in force
#[derive(Debug, Clone, Copy)]
pub enum TimeInForce {
    /// Good till cancelled
    Gtc,
    /// Immediate or cancel
    Ioc,
    /// Fill or kill
    Fok,
    /// Good for day
    Day,
}

/// Order status
#[derive(Debug, Clone, Copy)]
pub enum OrderStatus {
    /// Order pending
    Pending,
    /// Order accepted
    Accepted,
    /// Partially filled
    PartiallyFilled,
    /// Fully filled
    Filled,
    /// Order cancelled
    Cancelled,
    /// Order rejected
    Rejected,
}

/// Risk severity levels
#[derive(Debug, Clone, Copy)]
pub enum Severity {
    /// Informational
    Info,
    /// Warning level
    Warning,
    /// Critical issue
    Critical,
    /// Emergency - kill switch triggered
    Emergency,
}

/// Risk action to take
#[derive(Debug, Clone)]
pub enum RiskAction {
    /// No action needed
    None,
    /// Reduce position size
    ReducePosition,
    /// Close all positions
    CloseAll,
    /// Halt trading
    HaltTrading,
    /// Kill switch activated
    KillSwitch,
}

/// Trading Gateway configuration
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// Maximum position size per symbol
    pub max_position_size: Qty,
    /// Maximum daily loss limit
    pub max_daily_loss: i64,
    /// Risk check interval
    pub risk_check_interval: Duration,
    /// Orderbook update throttle
    pub orderbook_throttle_ms: u64,
    /// Enable market making
    pub enable_market_making: bool,
    /// Enable momentum strategies
    pub enable_momentum: bool,
    /// Enable arbitrage
    pub enable_arbitrage: bool,
    /// Circuit breaker threshold
    pub circuit_breaker_threshold: f64,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            max_position_size: Qty::from_i64(100000), // 10 lots
            max_daily_loss: 1000000, // 100 USDT
            risk_check_interval: Duration::from_millis(100),
            orderbook_throttle_ms: 10,
            enable_market_making: true,
            enable_momentum: true,
            enable_arbitrage: true,
            circuit_breaker_threshold: 0.05, // 5% move triggers circuit breaker
        }
    }
}

/// Main Trading Gateway orchestrator
pub struct TradingGateway {
    /// Configuration
    config: Arc<GatewayConfig>,
    /// Event bus for all components
    event_bus: Arc<broadcast::Sender<TradingEvent>>,
    /// Component health status
    health_status: Arc<DashMap<String, ComponentHealth>>,
    /// Active strategies
    strategies: Arc<RwLock<Vec<Box<dyn TradingStrategy>>>>,
    /// Position manager
    position_manager: Arc<position_manager::PositionManager>,
    /// Risk gate for pre-trade checks
    risk_gate: Arc<risk_gate::RiskGate>,
    /// Execution engine
    execution_engine: Arc<execution_engine::ExecutionEngine>,
    /// Signal aggregator
    signal_aggregator: Arc<signal_aggregator::SignalAggregator>,
    /// Circuit breaker
    circuit_breaker: Arc<RwLock<CircuitBreaker>>,
    /// Telemetry
    telemetry: Arc<telemetry::TelemetryCollector>,
}

/// Component health status
#[derive(Debug, Clone)]
pub struct ComponentHealth {
    /// Component name
    pub name: String,
    /// Is healthy
    pub is_healthy: bool,
    /// Last heartbeat
    pub last_heartbeat: Instant,
    /// Error count
    pub error_count: u64,
    /// Success count
    pub success_count: u64,
    /// Average latency
    pub avg_latency_us: u64,
}

/// Circuit breaker for emergency stops
pub struct CircuitBreaker {
    /// Is tripped
    is_tripped: bool,
    /// Trip timestamp
    tripped_at: Option<Instant>,
    /// Trip reason
    trip_reason: Option<String>,
    /// Auto-reset duration
    auto_reset_duration: Duration,
}

/// Trading strategy trait
#[async_trait]
pub trait TradingStrategy: Send + Sync {
    /// Strategy name
    fn name(&self) -> &str;
    
    /// Process market update
    async fn on_market_update(&mut self, event: &TradingEvent) -> Result<Option<TradingEvent>>;
    
    /// Process execution report
    async fn on_execution(&mut self, report: &TradingEvent) -> Result<()>;
    
    /// Get strategy health
    fn health(&self) -> ComponentHealth;
    
    /// Reset strategy state
    async fn reset(&mut self) -> Result<()>;
}

impl TradingGateway {
    /// Create new trading gateway
    pub async fn new(config: GatewayConfig) -> Result<Self> {
        let (event_tx, _) = broadcast::channel(100000);
        let event_bus = Arc::new(event_tx);
        
        // Initialize components
        let position_manager = Arc::new(position_manager::PositionManager::new());
        let risk_gate = Arc::new(risk_gate::RiskGate::new(config.clone()));
        let execution_engine = Arc::new(execution_engine::ExecutionEngine::new(event_bus.clone()));
        let signal_aggregator = Arc::new(signal_aggregator::SignalAggregator::new());
        let telemetry = Arc::new(telemetry::TelemetryCollector::new());
        
        let circuit_breaker = Arc::new(RwLock::new(CircuitBreaker {
            is_tripped: false,
            tripped_at: None,
            trip_reason: None,
            auto_reset_duration: Duration::from_secs(60),
        }));
        
        Ok(Self {
            config: Arc::new(config),
            event_bus,
            health_status: Arc::new(DashMap::new()),
            strategies: Arc::new(RwLock::new(Vec::new())),
            position_manager,
            risk_gate,
            execution_engine,
            signal_aggregator,
            circuit_breaker,
            telemetry,
        })
    }
    
    /// Start the gateway
    pub async fn start(&self) -> Result<()> {
        info!("🚀 Starting ShrivenQuant Trading Gateway");
        
        // Start component health monitoring
        self.start_health_monitor();
        
        // Start risk monitoring
        self.start_risk_monitor();
        
        // Start telemetry collection
        self.telemetry.start().await?;
        
        // Initialize strategies
        self.initialize_strategies().await?;
        
        info!("✅ Trading Gateway started successfully");
        Ok(())
    }
    
    /// Process orderbook update
    pub async fn process_orderbook_update(
        &self,
        symbol: Symbol,
        orderbook: &orderbook::OrderBook,
        analytics: &orderbook::analytics::MicrostructureAnalytics,
    ) -> Result<()> {
        let start = Instant::now();
        
        // Check circuit breaker
        if self.is_circuit_breaker_tripped() {
            warn!("Circuit breaker tripped - ignoring orderbook update");
            return Ok(());
        }
        
        // Get orderbook metrics
        let (best_bid, best_ask) = orderbook.get_bbo();
        let spread = orderbook.get_spread();
        let mid = orderbook.get_mid().unwrap_or(Px::ZERO);
        
        // Get analytics
        let vpin = analytics.get_vpin();
        let kyles_lambda = analytics.get_kyles_lambda();
        let flow_imbalance = analytics.get_flow_imbalance();
        
        // Create market update event
        let event = TradingEvent::MarketUpdate {
            symbol,
            bid: best_bid.map(|p| (p, orderbook.get_bid_size_at(p).unwrap_or(Qty::ZERO))),
            ask: best_ask.map(|p| (p, orderbook.get_ask_size_at(p).unwrap_or(Qty::ZERO))),
            mid,
            spread: spread.unwrap_or(0),
            imbalance: flow_imbalance,
            vpin,
            kyles_lambda,
            timestamp: Ts::now(),
        };
        
        // Broadcast to all components
        let _ = self.event_bus.send(event.clone());
        
        // Process through strategies
        let strategies = self.strategies.read();
        for strategy in strategies.iter() {
            if let Some(signal) = strategy.on_market_update(&event).await? {
                self.process_signal(signal).await?;
            }
        }
        
        // Update telemetry
        let latency = start.elapsed().as_micros() as u64;
        self.telemetry.record_orderbook_update(latency);
        
        Ok(())
    }
    
    /// Process trading signal
    async fn process_signal(&self, signal: TradingEvent) -> Result<()> {
        // Aggregate with other signals
        let aggregated = self.signal_aggregator.aggregate(signal.clone()).await?;
        
        if let Some(order_request) = aggregated {
            // Pre-trade risk check
            if self.risk_gate.check_order(&order_request).await? {
                // Submit order
                self.execution_engine.submit_order(order_request).await?;
            } else {
                warn!("Order rejected by risk gate");
            }
        }
        
        Ok(())
    }
    
    /// Initialize trading strategies
    async fn initialize_strategies(&self) -> Result<()> {
        let mut strategies = self.strategies.write();
        
        if self.config.enable_market_making {
            strategies.push(Box::new(market_maker::MarketMakingStrategy::new()));
        }
        
        if self.config.enable_momentum {
            strategies.push(Box::new(strategy::MomentumStrategy::new()));
        }
        
        if self.config.enable_arbitrage {
            strategies.push(Box::new(strategy::ArbitrageStrategy::new()));
        }
        
        info!("Initialized {} strategies", strategies.len());
        Ok(())
    }
    
    /// Start health monitoring
    fn start_health_monitor(&self) {
        let health_status = self.health_status.clone();
        let event_bus = self.event_bus.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            
            loop {
                interval.tick().await;
                
                // Check component health
                for entry in health_status.iter() {
                    let (name, health) = entry.pair();
                    
                    if health.last_heartbeat.elapsed() > Duration::from_secs(5) {
                        warn!("Component {} is unhealthy", name);
                        
                        let _ = event_bus.send(TradingEvent::RiskAlert {
                            severity: Severity::Warning,
                            message: format!("Component {} is unhealthy", name),
                            action: RiskAction::None,
                            timestamp: Ts::now(),
                        });
                    }
                }
            }
        });
    }
    
    /// Start risk monitoring
    fn start_risk_monitor(&self) {
        let risk_gate = self.risk_gate.clone();
        let position_manager = self.position_manager.clone();
        let circuit_breaker = self.circuit_breaker.clone();
        let event_bus = self.event_bus.clone();
        let config = self.config.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.risk_check_interval);
            
            loop {
                interval.tick().await;
                
                // Check positions
                let positions = position_manager.get_all_positions().await;
                
                for (symbol, position) in positions {
                    // Check position limits
                    if position.quantity.abs() > config.max_position_size.as_i64() {
                        error!("Position limit breached for {}", symbol);
                        
                        // Trip circuit breaker
                        let mut cb = circuit_breaker.write();
                        cb.is_tripped = true;
                        cb.tripped_at = Some(Instant::now());
                        cb.trip_reason = Some("Position limit breached".to_string());
                        
                        let _ = event_bus.send(TradingEvent::RiskAlert {
                            severity: Severity::Emergency,
                            message: format!("Position limit breached for {}", symbol),
                            action: RiskAction::KillSwitch,
                            timestamp: Ts::now(),
                        });
                    }
                    
                    // Check P&L
                    if position.unrealized_pnl < -config.max_daily_loss {
                        error!("Daily loss limit breached");
                        
                        let _ = event_bus.send(TradingEvent::RiskAlert {
                            severity: Severity::Critical,
                            message: "Daily loss limit breached".to_string(),
                            action: RiskAction::CloseAll,
                            timestamp: Ts::now(),
                        });
                    }
                }
            }
        });
    }
    
    /// Check if circuit breaker is tripped
    pub fn is_circuit_breaker_tripped(&self) -> bool {
        let cb = self.circuit_breaker.read();
        
        if cb.is_tripped {
            // Check for auto-reset
            if let Some(tripped_at) = cb.tripped_at {
                if tripped_at.elapsed() > cb.auto_reset_duration {
                    drop(cb);
                    let mut cb = self.circuit_breaker.write();
                    cb.is_tripped = false;
                    cb.tripped_at = None;
                    cb.trip_reason = None;
                    info!("Circuit breaker auto-reset");
                    return false;
                }
            }
            true
        } else {
            false
        }
    }
    
    /// Emergency stop - kill switch
    pub async fn emergency_stop(&self) -> Result<()> {
        error!("🚨 EMERGENCY STOP TRIGGERED");
        
        // Trip circuit breaker
        {
            let mut cb = self.circuit_breaker.write();
            cb.is_tripped = true;
            cb.tripped_at = Some(Instant::now());
            cb.trip_reason = Some("Manual emergency stop".to_string());
        }
        
        // Cancel all orders
        self.execution_engine.cancel_all_orders().await?;
        
        // Close all positions
        self.position_manager.close_all_positions().await?;
        
        // Broadcast emergency alert
        let _ = self.event_bus.send(TradingEvent::RiskAlert {
            severity: Severity::Emergency,
            message: "Emergency stop activated".to_string(),
            action: RiskAction::KillSwitch,
            timestamp: Ts::now(),
        });
        
        Ok(())
    }
    
    /// Get gateway status
    pub async fn get_status(&self) -> GatewayStatus {
        GatewayStatus {
            is_running: !self.is_circuit_breaker_tripped(),
            component_health: self.health_status.iter()
                .map(|e| (e.key().clone(), e.value().clone()))
                .collect(),
            active_strategies: self.strategies.read().len(),
            total_positions: self.position_manager.get_position_count().await,
            telemetry_stats: self.telemetry.get_stats().await,
        }
    }
}

/// Gateway status
#[derive(Debug, Clone)]
pub struct GatewayStatus {
    /// Is gateway running
    pub is_running: bool,
    /// Component health
    pub component_health: Vec<(String, ComponentHealth)>,
    /// Number of active strategies
    pub active_strategies: usize,
    /// Total positions
    pub total_positions: usize,
    /// Telemetry statistics
    pub telemetry_stats: telemetry::TelemetryStats,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_gateway_creation() {
        let config = GatewayConfig::default();
        let gateway = TradingGateway::new(config).await;
        assert!(gateway.is_ok());
    }
    
    #[tokio::test]
    async fn test_circuit_breaker() {
        let config = GatewayConfig::default();
        let gateway = TradingGateway::new(config).await.unwrap();
        
        assert!(!gateway.is_circuit_breaker_tripped());
        
        gateway.emergency_stop().await.unwrap();
        assert!(gateway.is_circuit_breaker_tripped());
    }
}