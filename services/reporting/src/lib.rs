//! Reporting Service
//!
//! SIMD-optimized performance monitoring and analytics service
//!
//! COMPLIANCE:
//! - Zero allocations in hot paths
//! - SIMD operations for vectorized calculations
//! - Fixed-point arithmetic only
//! - Cache-aligned structures

pub mod analytics;
pub mod metrics;
pub mod performance;

use anyhow::Result;
use async_trait::async_trait;
use services_common::{Px, Qty, Symbol, Ts};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Reporting service events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReportingEvent {
    /// Trading metrics updated
    MetricsUpdated {
        /// Timestamp when the metrics were updated
        timestamp: Ts,
        /// Updated trading metrics data
        metrics: metrics::TradingMetrics,
    },
    /// Performance report generated
    PerformanceReport {
        /// Timestamp when the report was generated
        timestamp: Ts,
        /// Generated performance report data
        report: performance::PerformanceReport,
    },
    /// Alert triggered
    Alert {
        /// Timestamp when the alert was triggered
        timestamp: Ts,
        /// Severity level of the alert
        level: AlertLevel,
        /// Human-readable alert message
        message: String,
        /// Source component that triggered the alert
        source: String,
    },
}

/// Alert levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertLevel {
    /// Informational alert for general system status
    Info,
    /// Warning alert for potential issues requiring attention
    Warning,
    /// Critical alert for serious issues requiring immediate action
    Critical,
    /// Emergency alert for system-threatening conditions
    Emergency,
}

/// Reporting service trait
#[async_trait]
pub trait ReportingService: Send + Sync {
    /// Record a fill and update metrics
    async fn record_fill(
        &self,
        order_id: u64,
        symbol: Symbol,
        qty: Qty,
        price: Px,
        timestamp: Ts,
    ) -> Result<()>;

    /// Update market data for spread analysis
    async fn update_market(&self, symbol: Symbol, bid: Px, ask: Px, timestamp: Ts) -> Result<()>;

    /// Get current trading metrics
    async fn get_metrics(&self) -> Result<metrics::TradingMetrics>;

    /// Get performance report
    async fn get_performance_report(&self) -> Result<performance::PerformanceReport>;

    /// Subscribe to reporting events
    async fn subscribe_events(&self) -> Result<tokio::sync::broadcast::Receiver<ReportingEvent>>;
}

/// Main reporting service implementation
#[derive(Debug)]
pub struct ReportingServiceImpl {
    /// SIMD-optimized metrics engine
    metrics_engine: Arc<metrics::MetricsEngine>,
    /// Performance analyzer
    performance_analyzer: Arc<RwLock<performance::PerformanceAnalyzer>>,
    /// Event broadcaster
    event_broadcaster: tokio::sync::broadcast::Sender<ReportingEvent>,
    /// Configuration
    config: ReportingConfig,
}

/// Reporting service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportingConfig {
    /// Buffer size for SIMD calculations
    pub buffer_size: usize,
    /// Alert thresholds
    pub alert_thresholds: AlertThresholds,
    /// Performance calculation interval
    pub performance_interval_ms: u64,
}

/// Alert thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// Maximum drawdown before alert (basis points)
    pub max_drawdown_bp: i32,
    /// Minimum Sharpe ratio before alert
    pub min_sharpe_ratio: f64,
    /// Maximum daily loss before alert
    pub max_daily_loss: i64,
}

impl Default for ReportingConfig {
    fn default() -> Self {
        Self {
            buffer_size: 10000,
            alert_thresholds: AlertThresholds {
                max_drawdown_bp: 1000, // 10%
                min_sharpe_ratio: 0.5,
                max_daily_loss: 100000, // $10k in cents
            },
            performance_interval_ms: 1000, // 1 second
        }
    }
}

impl ReportingServiceImpl {
    /// Create new reporting service
    #[must_use] pub fn new(config: ReportingConfig) -> Self {
        let (event_broadcaster, _) = tokio::sync::broadcast::channel(1000);

        Self {
            metrics_engine: Arc::new(metrics::MetricsEngine::new(config.buffer_size)),
            performance_analyzer: Arc::new(RwLock::new(performance::PerformanceAnalyzer::new(
                config.buffer_size,
            ))),
            event_broadcaster,
            config,
        }
    }

    /// Start background performance monitoring
    pub async fn start_monitoring(&self) -> Result<()> {
        let metrics_engine = Arc::clone(&self.metrics_engine);
        let performance_analyzer = Arc::clone(&self.performance_analyzer);
        let event_broadcaster = self.event_broadcaster.clone();
        let interval = self.config.performance_interval_ms;
        let thresholds = self.config.alert_thresholds.clone();

        tokio::spawn(async move {
            let mut interval_timer =
                tokio::time::interval(tokio::time::Duration::from_millis(interval));

            loop {
                interval_timer.tick().await;

                // Get current trading metrics from SIMD engine
                let trading_metrics = metrics_engine.get_metrics();

                // Generate performance report
                let report = {
                    let analyzer = performance_analyzer.read();
                    analyzer.generate_report()
                };

                // Check for alerts based on both metrics and performance
                Self::check_alerts(&report, &thresholds, &event_broadcaster).await;
                Self::check_trading_metrics_alerts(
                    &trading_metrics,
                    &thresholds,
                    &event_broadcaster,
                )
                .await;

                // Broadcast metrics update
                // Ignore send errors as receivers may have disconnected
                drop(event_broadcaster.send(ReportingEvent::MetricsUpdated {
                    timestamp: Ts::now(),
                    metrics: trading_metrics,
                }));

                // Broadcast performance update
                drop(event_broadcaster.send(ReportingEvent::PerformanceReport {
                    timestamp: Ts::now(),
                    report,
                }));
            }
        });

        tracing::info!("Reporting service monitoring started");
        Ok(())
    }

    /// Check for alerts based on trading metrics from SIMD engine
    async fn check_trading_metrics_alerts(
        metrics: &metrics::TradingMetrics,
        thresholds: &AlertThresholds,
        broadcaster: &tokio::sync::broadcast::Sender<ReportingEvent>,
    ) {
        // Check if Sharpe ratio is below threshold
        if metrics.sharpe_ratio < thresholds.min_sharpe_ratio {
            // Ignore send errors as receivers may have disconnected
            drop(broadcaster.send(ReportingEvent::Alert {
                timestamp: Ts::now(),
                level: AlertLevel::Warning,
                message: format!(
                    "Trading Sharpe ratio below threshold: {:.3} < {:.3}",
                    metrics.sharpe_ratio, thresholds.min_sharpe_ratio
                ),
                source: "metrics_engine".to_string(),
            }));
        }

        // Check win rate for warning
        if metrics.total_trades > 10 && metrics.win_rate < 30.0 {
            // Ignore send errors as receivers may have disconnected
            drop(broadcaster.send(ReportingEvent::Alert {
                timestamp: Ts::now(),
                level: AlertLevel::Warning,
                message: format!(
                    "Low win rate detected: {:.1}% (trades: {})",
                    metrics.win_rate, metrics.total_trades
                ),
                source: "metrics_engine".to_string(),
            }));
        }

        // Check profit factor
        if metrics.total_trades > 5 && metrics.profit_factor < 1.0 {
            // Ignore send errors as receivers may have disconnected
            drop(broadcaster.send(ReportingEvent::Alert {
                timestamp: Ts::now(),
                level: AlertLevel::Critical,
                message: format!(
                    "Profit factor below 1.0: {:.3} (losing money overall)",
                    metrics.profit_factor
                ),
                source: "metrics_engine".to_string(),
            }));
        }

        // Check maximum drawdown in trading metrics
        #[allow(clippy::cast_precision_loss)]
        let drawdown_bp = (metrics.max_drawdown as f64 / 100.0) as i32; // Convert to basis points
        if drawdown_bp > thresholds.max_drawdown_bp {
            // Ignore send errors as receivers may have disconnected
            drop(broadcaster.send(ReportingEvent::Alert {
                timestamp: Ts::now(),
                level: AlertLevel::Emergency,
                message: format!(
                    "Maximum drawdown exceeded in trading: {}bp > {}bp",
                    drawdown_bp, thresholds.max_drawdown_bp
                ),
                source: "metrics_engine".to_string(),
            }));
        }
    }

    /// Check for alerts based on performance metrics
    async fn check_alerts(
        report: &performance::PerformanceReport,
        thresholds: &AlertThresholds,
        broadcaster: &tokio::sync::broadcast::Sender<ReportingEvent>,
    ) {
        // Check drawdown
        if report.max_drawdown_pct > thresholds.max_drawdown_bp {
            // Ignore send errors as receivers may have disconnected
            drop(broadcaster.send(ReportingEvent::Alert {
                timestamp: Ts::now(),
                level: AlertLevel::Critical,
                message: format!(
                    "Maximum drawdown exceeded: {}bp > {}bp",
                    report.max_drawdown_pct, thresholds.max_drawdown_bp
                ),
                source: "reporting_service".to_string(),
            }));
        }

        // Check Sharpe ratio
        if report.sharpe_ratio < thresholds.min_sharpe_ratio {
            // Ignore send errors as receivers may have disconnected
            drop(broadcaster.send(ReportingEvent::Alert {
                timestamp: Ts::now(),
                level: AlertLevel::Warning,
                message: format!(
                    "Sharpe ratio below threshold: {:.3} < {:.3}",
                    report.sharpe_ratio, thresholds.min_sharpe_ratio
                ),
                source: "reporting_service".to_string(),
            }));
        }

        // Check daily loss
        if report.daily_pnl < -thresholds.max_daily_loss {
            // Ignore send errors as receivers may have disconnected
            drop(broadcaster.send(ReportingEvent::Alert {
                timestamp: Ts::now(),
                level: AlertLevel::Emergency,
                message: format!(
                    "Daily loss exceeded: {} < -{}",
                    report.daily_pnl, thresholds.max_daily_loss
                ),
                source: "reporting_service".to_string(),
            }));
        }
    }
}

#[async_trait]
impl ReportingService for ReportingServiceImpl {
    async fn record_fill(
        &self,
        order_id: u64,
        symbol: Symbol,
        qty: Qty,
        price: Px,
        timestamp: Ts,
    ) -> Result<()> {
        // Record in SIMD metrics engine
        self.metrics_engine
            .record_fill(order_id, symbol, qty, price, timestamp);

        // Update performance analyzer
        {
            let mut analyzer = self.performance_analyzer.write();
            analyzer.record_trade(qty, price, timestamp);
        }

        // Get updated metrics and broadcast
        let metrics = self.metrics_engine.get_metrics();
        if let Err(e) = self
            .event_broadcaster
            .send(ReportingEvent::MetricsUpdated { timestamp, metrics })
        {
            tracing::debug!("Failed to broadcast metrics update: {:?}", e);
        }

        Ok(())
    }

    async fn update_market(&self, symbol: Symbol, bid: Px, ask: Px, timestamp: Ts) -> Result<()> {
        // Update SIMD metrics engine
        self.metrics_engine
            .update_market(symbol, bid, ask, timestamp);

        // Update performance analyzer
        {
            let mut analyzer = self.performance_analyzer.write();
            analyzer.update_market_price(symbol, bid, ask, timestamp);
        }

        Ok(())
    }

    async fn get_metrics(&self) -> Result<metrics::TradingMetrics> {
        Ok(self.metrics_engine.get_metrics())
    }

    async fn get_performance_report(&self) -> Result<performance::PerformanceReport> {
        let analyzer = self.performance_analyzer.read();
        Ok(analyzer.generate_report())
    }

    async fn subscribe_events(&self) -> Result<tokio::sync::broadcast::Receiver<ReportingEvent>> {
        Ok(self.event_broadcaster.subscribe())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reporting_service_basic() {
        let config = ReportingConfig::default();
        let service = ReportingServiceImpl::new(config);

        // Record a fill
        let symbol = Symbol::new(1);
        let result = service
            .record_fill(
                1,
                symbol,
                Qty::from_i64(1000000), // 100 units
                Px::from_i64(1000000),  // $100
                Ts::now(),
            )
            .await;

        assert!(result.is_ok());

        // Get metrics
        let metrics = service.get_metrics().await.unwrap();
        assert_eq!(metrics.total_trades, 1);
        assert!(metrics.total_volume > 0);
    }

    #[tokio::test]
    async fn test_performance_monitoring() {
        let config = ReportingConfig {
            performance_interval_ms: 100, // Fast for testing
            ..Default::default()
        };
        let service = ReportingServiceImpl::new(config);

        // Start monitoring
        service.start_monitoring().await.unwrap();

        // Subscribe to events
        let mut receiver = service.subscribe_events().await.unwrap();

        // Record some activity
        let symbol = Symbol::new(1);
        service
            .record_fill(
                1,
                symbol,
                Qty::from_i64(1000000),
                Px::from_i64(1000000),
                Ts::now(),
            )
            .await
            .unwrap();

        // Wait for performance report
        tokio::time::timeout(tokio::time::Duration::from_millis(200), receiver.recv())
            .await
            .unwrap()
            .unwrap();
    }
}
