//! gRPC Service Implementation for Risk Manager
//!
//! Production-grade implementation with all required features:
//! - Health checks
//! - Metrics endpoints
//! - Circuit breakers
//! - Rate limiting
//! - Graceful shutdown

use crate::{
    RiskManagerService,
    circuit_breaker::CircuitBreaker,
    monitor::RiskMonitor,
};
use anyhow::Result;
use services_common::{Symbol, constants};
use prometheus::{
    register_counter_vec, register_histogram_vec, register_gauge_vec,
    CounterVec, HistogramVec, GaugeVec,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};
use tonic::{Request, Response, Status};
use tracing::{warn, info_span, Instrument};
use uuid::Uuid;

// Constants for conversion overflow handling
const METRIC_OVERFLOW_VALUE: i64 = i64::MAX;

// Metrics storage
pub(crate) struct Metrics {
    pub(crate) request_counter: CounterVec,
    pub(crate) latency_histogram: HistogramVec,
    pub(crate) risk_checks: CounterVec,
    pub(crate) position_gauge: GaugeVec,
    pub(crate) exposure_gauge: GaugeVec,
}

lazy_static::lazy_static! {
    pub(crate) static ref METRICS: Option<Metrics> = init_metrics_internal();
}

// Initialize Prometheus metrics with proper error handling
fn init_metrics_internal() -> Option<Metrics> {
    let request_counter = match register_counter_vec!(
        "risk_grpc_requests_total",
        "Total number of gRPC requests to risk service",
        &["method", "status"]
    ) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("Failed to create REQUEST_COUNTER metric: {}", e);
            return None;
        }
    };
    
    let latency_histogram = match register_histogram_vec!(
        "risk_grpc_request_duration_seconds",
        "Risk service gRPC request latency",
        &["method"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]
    ) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("Failed to create LATENCY_HISTOGRAM metric: {}", e);
            return None;
        }
    };
    
    let risk_checks = match register_counter_vec!(
        "risk_checks_total",
        "Total risk checks performed",
        &["result"]
    ) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("Failed to create RISK_CHECKS metric: {}", e);
            return None;
        }
    };
    
    let position_gauge = match register_gauge_vec!(
        "risk_position_value",
        "Current position values",
        &["symbol"]
    ) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("Failed to create POSITION_GAUGE metric: {}", e);
            return None;
        }
    };
    
    let exposure_gauge = match register_gauge_vec!(
        "risk_total_exposure",
        "Total portfolio exposure",
        &["type"]
    ) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("Failed to create EXPOSURE_GAUGE metric: {}", e);
            return None;
        }
    };
    
    Some(Metrics {
        request_counter,
        latency_histogram,
        risk_checks,
        position_gauge,
        exposure_gauge,
    })
}

// Initialize metrics - returns success status
fn init_metrics() -> bool {
    METRICS.is_some()
}

// Constants
const MAX_CONCURRENT_REQUESTS: usize = constants::network::MAX_CONCURRENT_CONNECTIONS;
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(constants::time::INTERVAL_5_SECS);
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(constants::time::INTERVAL_10_SECS);
const MAX_RATE_LIMIT_WINDOW: u32 = constants::memory::LARGE_BUFFER_CAPACITY as u32;
pub(crate) const FIXED_POINT_DIVISOR: i64 = constants::fixed_point::SCALE_4;
pub(crate) const FIXED_POINT_PERCENT_DIVISOR: i32 = constants::fixed_point::SCALE_2 as i32;
const CIRCUIT_BREAKER_FAILURE_THRESHOLD: u64 = 5;
const CIRCUIT_BREAKER_TIMEOUT_MS: u64 = constants::network::DEFAULT_CONNECT_TIMEOUT_MS;
const DEFAULT_RATE_LIMIT: u32 = constants::trading::DEFAULT_MAX_ORDERS_PER_SEC;
const EVENT_CHANNEL_SIZE: usize = constants::memory::LARGE_BUFFER_CAPACITY;
pub(crate) const DRAWDOWN_CRITICAL_THRESHOLD: i32 = 2000;  // 20% in fixed-point
pub(crate) const DAILY_LOSS_CRITICAL: i64 = -10_000_000;   // $1M loss

/// Enhanced gRPC service with production features
#[derive(Clone)]
pub struct RiskManagerGrpcService {
    /// Core risk manager
    pub risk_manager: Arc<RiskManagerService>,
    /// Risk monitor for analytics
    pub monitor: Arc<RiskMonitor>,
    /// Circuit breaker for external calls
    pub circuit_breaker: Arc<CircuitBreaker>,
    /// Rate limiter
    pub rate_limiter: Arc<RateLimiter>,
    /// Shutdown signal
    pub shutdown_tx: broadcast::Sender<()>,
    /// Health status
    pub health_status: Arc<RwLock<HealthStatus>>,
    /// Event bus sender
    pub event_tx: broadcast::Sender<RiskEvent>,
}

/// Health status tracking
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub is_healthy: bool,
    pub last_check: Instant,
    pub consecutive_failures: u32,
    pub error_message: Option<String>,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self {
            is_healthy: true,
            last_check: Instant::now(),
            consecutive_failures: 0,
            error_message: None,
        }
    }
}

/// Rate limiter implementation
pub struct RateLimiter {
    max_requests_per_second: u32,
    window: Arc<RwLock<Vec<Instant>>>,
}

impl RateLimiter {
    #[must_use] pub fn new(max_rps: u32) -> Self {
        // Use MAX_CONCURRENT_REQUESTS to bound the rate limiter
        let effective_max_rps = max_rps.min(MAX_CONCURRENT_REQUESTS as u32);
        
        // Safely convert rate limit to usize for vector capacity
        let capacity = effective_max_rps.min(MAX_RATE_LIMIT_WINDOW);
        let capacity_usize = if usize::try_from(capacity).is_ok() {
            #[allow(clippy::cast_possible_truncation)] // u32 bounded to usize::MAX
            { capacity as usize }
        } else {
            usize::MAX
        };
        
        Self {
            max_requests_per_second: effective_max_rps,
            window: Arc::new(RwLock::new(Vec::with_capacity(capacity_usize))),
        }
    }

    pub async fn check_rate_limit(&self) -> Result<(), Status> {
        let now = Instant::now();
        let mut window = self.window.write().await;
        
        // Remove old entries
        window.retain(|&t| now.duration_since(t) < Duration::from_secs(1));
        
        // Convert u32 to usize with bounds check
        let max_requests = if usize::try_from(self.max_requests_per_second).is_ok() {
            #[allow(clippy::cast_possible_truncation)] // u32 bounded to usize::MAX
            { self.max_requests_per_second as usize }
        } else {
            usize::MAX
        };
        if window.len() >= max_requests {
            return Err(Status::resource_exhausted("Rate limit exceeded"));
        }
        
        window.push(now);
        Ok(())
    }
}

impl RiskManagerGrpcService {
    /// Create new enhanced gRPC service  
    pub fn new(limits: crate::RiskLimits) -> Result<(Self, broadcast::Receiver<RiskEvent>)> {
        // Initialize metrics if not already done
        if !init_metrics() {
            warn!("Metrics initialization failed - service will run without metrics");
        }
        
        // Create core components
        let risk_manager = Arc::new(RiskManagerService::new(limits));
        let monitor = Arc::new(RiskMonitor::new());
        
        // Create circuit breaker
        let circuit_breaker = Arc::new(CircuitBreaker::new(
            CIRCUIT_BREAKER_FAILURE_THRESHOLD,
            CIRCUIT_BREAKER_TIMEOUT_MS,
        ));
        
        // Create rate limiter
        let rate_limiter = Arc::new(RateLimiter::new(DEFAULT_RATE_LIMIT));
        
        // Create shutdown broadcast
        let (shutdown_tx, _) = broadcast::channel(1);
        
        // Create health status
        let health_status = Arc::new(RwLock::new(HealthStatus::default()));
        
        // Create event broadcast channel
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_SIZE);
        let event_rx = event_tx.subscribe();
        
        // Start health check task
        let health_clone = health_status.clone();
        let manager_clone = risk_manager.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(HEALTH_CHECK_INTERVAL);
            loop {
                interval.tick().await;
                
                // Check system health
                let is_healthy = check_system_health(&manager_clone).await;
                
                let mut status = health_clone.write().await;
                status.last_check = Instant::now();
                
                if is_healthy {
                    status.is_healthy = true;
                    status.consecutive_failures = 0;
                    status.error_message = None;
                } else {
                    status.consecutive_failures += 1;
                    if status.consecutive_failures >= 3 {
                        status.is_healthy = false;
                        status.error_message = Some("System unhealthy".to_string());
                    }
                }
            }
        });
        
        // Start metrics reporter
        let monitor_clone = monitor.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                update_prometheus_metrics(&monitor_clone).await;
            }
        });
        
        let service = Self {
            risk_manager,
            monitor,
            circuit_breaker,
            rate_limiter,
            shutdown_tx,
            health_status,
            event_tx,
        };
        
        Ok((service, event_rx))
    }
    
    /// Process request with middleware (kept for complex request processing)
    pub(crate) async fn process_request<F, Req, Res>(
        &self,
        method: &str,
        request: Request<Req>,
        handler: F,
    ) -> Result<Response<Res>, Status>
    where
        F: FnOnce(Req) -> Result<Res, Status> + Send + 'static,
        Req: Send + 'static,
        Res: Send + 'static,
    {
        // Generate correlation ID
        let correlation_id = Uuid::new_v4();
        let span = info_span!("risk_request", 
            method = %method,
            correlation_id = %correlation_id
        );
        
        async move {
            // Check rate limit
            self.rate_limiter.check_rate_limit().await?;
            
            // Record metrics
            let start = Instant::now();
            if let Some(metrics) = METRICS.as_ref() {
                metrics.request_counter.with_label_values(&[method, "started"]).inc();
            }
            
            // Check circuit breaker
            if !self.circuit_breaker.is_open() {
                if let Some(metrics) = METRICS.as_ref() {
                    metrics.request_counter.with_label_values(&[method, "circuit_breaker"]).inc();
                }
                return Err(Status::unavailable("Circuit breaker open"));
            }
            
            // Process request
            let result = tokio::time::timeout(
                REQUEST_TIMEOUT,
                tokio::task::spawn_blocking(move || handler(request.into_inner()))
            ).await;
            
            // Record latency
            let duration = start.elapsed().as_secs_f64();
            if let Some(metrics) = METRICS.as_ref() {
                metrics.latency_histogram.with_label_values(&[method]).observe(duration);
            }
            
            match result {
                Ok(Ok(Ok(response))) => {
                    if let Some(metrics) = METRICS.as_ref() {
                        metrics.request_counter.with_label_values(&[method, "success"]).inc();
                    }
                    self.circuit_breaker.record_success();
                    Ok(Response::new(response))
                }
                Ok(Ok(Err(e))) => {
                    if let Some(metrics) = METRICS.as_ref() {
                        metrics.request_counter.with_label_values(&[method, "error"]).inc();
                    }
                    self.circuit_breaker.record_failure();
                    Err(e)
                }
                Ok(Err(e)) => {
                    if let Some(metrics) = METRICS.as_ref() {
                        metrics.request_counter.with_label_values(&[method, "error"]).inc();
                    }
                    self.circuit_breaker.record_failure();
                    Err(Status::internal(format!("Task error: {e}")))
                }
                Err(_timeout_err) => {
                    if let Some(metrics) = METRICS.as_ref() {
                        metrics.request_counter.with_label_values(&[method, "timeout"]).inc();
                    }
                    self.circuit_breaker.record_failure();
                    Err(Status::deadline_exceeded("Request timeout"))
                }
            }
        }.instrument(span).await
    }
}

/// Check system health
async fn check_system_health(manager: &Arc<RiskManagerService>) -> bool {
    // Check if kill switch is active
    if manager.is_kill_switch_active() {
        return false;
    }
    
    // Check metrics
    let metrics = manager.get_metrics().await;
    
    // Check for excessive losses
    if metrics.daily_pnl < DAILY_LOSS_CRITICAL {
        return false;
    }
    
    // Check for high drawdown
    if metrics.current_drawdown > DRAWDOWN_CRITICAL_THRESHOLD {
        return false;
    }
    
    true
}

/// Convert fixed-point value to float for metrics
/// Handles bounds checking to prevent precision loss
#[allow(clippy::cast_precision_loss)] // Controlled conversion for metrics
fn convert_fixed_to_float(value: i64) -> f64 {
    // Check if value can be safely converted
    const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_992; // 2^53
    const SCALE_FACTOR: i64 = 1000;
    
    if value.abs() > MAX_SAFE_INTEGER / FIXED_POINT_DIVISOR {
        // Value too large, scale down to prevent precision loss
        let scaled_value = value / SCALE_FACTOR;
        let scaled_divisor = FIXED_POINT_DIVISOR / SCALE_FACTOR;
        return (scaled_value as f64) / (scaled_divisor as f64);
    }
    
    (value as f64) / (FIXED_POINT_DIVISOR as f64)
}

/// Update Prometheus metrics
async fn update_prometheus_metrics(monitor: &Arc<RiskMonitor>) {
    if let Some(prom_metrics) = METRICS.as_ref() {
        if let Ok(metrics) = monitor.get_current_metrics().await {
            prom_metrics.exposure_gauge
                .with_label_values(&["total"])
                .set(convert_fixed_to_float({
                // Safely convert u64 to i64 with bounds check
                match i64::try_from(metrics.total_exposure) {
                    Ok(val) => val,
                    Err(e) => {
                        tracing::warn!("Total exposure conversion error: {}, using MAX value", e);
                        METRIC_OVERFLOW_VALUE
                    }
                }
            }));
            
            prom_metrics.exposure_gauge
                .with_label_values(&["daily_pnl"])
                .set(convert_fixed_to_float(metrics.daily_pnl));
            
            // Update position gauges
            for position in metrics.positions {
                prom_metrics.position_gauge
                    .with_label_values(&[&format!("{}", position.symbol.0)])
                    .set(convert_fixed_to_float({
                    // Safely convert u64 to i64 with bounds check
                    match i64::try_from(position.position_value) {
                        Ok(val) => val,
                        Err(e) => {
                            tracing::warn!("Position value conversion error for {}: {}, using MAX value", position.symbol.0, e);
                            METRIC_OVERFLOW_VALUE
                        }
                    }
                }));
            }
        }
    }
}

/// Risk event for streaming
#[derive(Debug, Clone)]
pub struct RiskEvent {
    pub timestamp: i64,
    pub event_type: RiskEventType,
    pub symbol: Option<Symbol>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum RiskEventType {
    OrderChecked,
    OrderRejected,
    PositionUpdated,
    LimitBreached,
    CircuitBreakerTriggered,
    KillSwitchActivated,
}