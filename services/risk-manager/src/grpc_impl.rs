//! gRPC `RiskService` implementation

use crate::grpc_service::{
    RiskManagerGrpcService, RiskEvent, RiskEventType,
    DAILY_LOSS_CRITICAL, DRAWDOWN_CRITICAL_THRESHOLD, FIXED_POINT_DIVISOR,
    FIXED_POINT_PERCENT_DIVISOR
};
use crate::{RiskCheckResult, RiskManager};

// Constants for conversion error states in protobuf messages
const PROTO_I64_OVERFLOW_VALUE: i64 = i64::MAX;
const PROTO_I32_OVERFLOW_VALUE: i32 = i32::MAX;
use services_common::{Px, Qty, Side as CommonSide, Symbol};
use services_common::risk::v1::{
    risk_service_server::RiskService,
    CheckOrderRequest, CheckOrderResponse, CheckResult,
    UpdatePositionRequest, UpdatePositionResponse,
    GetPositionsRequest, GetPositionsResponse,
    GetMetricsRequest, GetMetricsResponse,
    KillSwitchRequest, KillSwitchResponse,
    StreamAlertsRequest, RiskAlert, AlertLevel,
    Position as ProtoPosition, RiskMetrics as ProtoMetrics,
    Side as ProtoSide,
};
use std::pin::Pin;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::{info, warn};

#[tonic::async_trait]
impl RiskService for RiskManagerGrpcService {
    async fn check_order(
        &self,
        request: Request<CheckOrderRequest>,
    ) -> Result<Response<CheckOrderResponse>, Status> {
        // Clone required data for the synchronous handler
        let risk_manager = self.risk_manager.clone();
        let event_tx = self.event_tx.clone();
        
        // Use process_request with synchronous handler
        self.process_request("check_order", request, move |req| {
            // Convert proto types to internal types
            let symbol = Symbol(req.symbol.parse().map_err(|_| Status::invalid_argument("Invalid symbol"))?);
            let side = match ProtoSide::try_from(req.side).map_err(|_| Status::invalid_argument("Invalid side"))? {
                ProtoSide::Unspecified => return Err(Status::invalid_argument("Side must be specified")),
                ProtoSide::Buy => CommonSide::Bid,
                ProtoSide::Sell => CommonSide::Ask,
            };
            let qty = Qty::from_i64(req.quantity);
            let price = Px::from_i64(req.price);
            
            // Log the order check request
            tracing::info!(
                "Checking order: {:?} {} {} @ {}",
                side, qty.as_i64(), symbol.0, price.as_i64()
            );
            
            // Use block_on to call async method from synchronous context
            let handle = tokio::runtime::Handle::current();
            let check_result = handle.block_on(
                risk_manager.check_order(symbol, side, qty, price)
            );
            
            // Send event based on result
            let event = match &check_result {
                RiskCheckResult::Approved => {
                    RiskEvent {
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        event_type: RiskEventType::OrderChecked,
                        symbol: Some(symbol),
                        message: format!("Order approved: {} {} @ {}", qty.as_i64(), symbol.0, price.as_i64()),
                    }
                }
                RiskCheckResult::Rejected(reason) => {
                    RiskEvent {
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        event_type: RiskEventType::OrderRejected,
                        symbol: Some(symbol),
                        message: format!("Order rejected: {reason}"),
                    }
                }
                RiskCheckResult::RequiresApproval(reason) => {
                    RiskEvent {
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        event_type: RiskEventType::OrderRejected,
                        symbol: Some(symbol),
                        message: format!("Order requires approval: {reason}"),
                    }
                }
            };
            
            // Send event to event bus (non-blocking)
            let _ = event_tx.send(event);
            
            // Convert result to proto
            let (result, reason) = match check_result {
                RiskCheckResult::Approved => (CheckResult::Approved, String::new()),
                RiskCheckResult::Rejected(msg) => (CheckResult::Rejected, msg),
                RiskCheckResult::RequiresApproval(msg) => (CheckResult::RequiresApproval, msg),
            };
            
            // Get current metrics using block_on
            let metrics = handle.block_on(risk_manager.get_metrics());
            
            // Record risk check metric
            if let Some(metrics_prom) = crate::grpc_service::METRICS.as_ref() {
                let label = if result == CheckResult::Approved { "approved" } else { "rejected" };
                metrics_prom.risk_checks.with_label_values(&[label]).inc();
            }
            
            Ok(CheckOrderResponse {
                // Proto-generated enum already has i32 representation
                result: result.into(),
                reason,
                current_metrics: Some(ProtoMetrics {
                    // Safe conversion: monetary values to protobuf i64
                    total_exposure: if let Ok(val) = i64::try_from(metrics.total_exposure) { val } else {
                        warn!("Total exposure {} exceeds i64 range", metrics.total_exposure);
                        PROTO_I64_OVERFLOW_VALUE
                    },
                    current_drawdown: metrics.current_drawdown,
                    daily_pnl: metrics.daily_pnl,
                    // Safe conversion: count values to protobuf i32
                    open_positions: if let Ok(val) = i32::try_from(metrics.open_positions) { val } else {
                        warn!("Open positions {} exceeds i32 range", metrics.open_positions);
                        PROTO_I32_OVERFLOW_VALUE
                    },
                    orders_today: if let Ok(val) = i32::try_from(metrics.orders_today) { val } else {
                        warn!("Orders today {} exceeds i32 range", metrics.orders_today);
                        PROTO_I32_OVERFLOW_VALUE
                    },
                    circuit_breaker_active: metrics.circuit_breaker_active,
                    kill_switch_active: metrics.kill_switch_active,
                }),
            })
        }).await
    }
    
    async fn update_position(
        &self,
        request: Request<UpdatePositionRequest>,
    ) -> Result<Response<UpdatePositionResponse>, Status> {
        let risk_manager = self.risk_manager.clone();
        let event_tx = self.event_tx.clone();
        
        self.process_request("update_position", request, move |req| {
            // Convert proto types to internal types
            let symbol = Symbol(req.symbol.parse().map_err(|_| Status::invalid_argument("Invalid symbol"))?);
            let side = match ProtoSide::try_from(req.side).map_err(|_| Status::invalid_argument("Invalid side"))? {
                ProtoSide::Unspecified => return Err(Status::invalid_argument("Side must be specified")),
                ProtoSide::Buy => CommonSide::Bid,
                ProtoSide::Sell => CommonSide::Ask,
            };
            let qty = Qty::from_i64(req.quantity);
            let price = Px::from_i64(req.price);
            
            let handle = tokio::runtime::Handle::current();
            
            // Update position in risk manager
            handle.block_on(
                risk_manager.update_position(symbol, side, qty, price)
            ).map_err(|e| Status::internal(format!("Failed to update position: {e}")))?;
            
            // Send position updated event
            let event = RiskEvent {
                timestamp: chrono::Utc::now().timestamp_millis(),
                event_type: RiskEventType::PositionUpdated,
                symbol: Some(symbol),
                message: format!("Position updated: {} {} @ {}", qty.as_i64(), symbol.0, price.as_i64()),
            };
            
            // Send event to event bus (non-blocking)
            let _ = event_tx.send(event);
            
            // Get updated position
            let position = handle.block_on(
                risk_manager.get_position(symbol)
            ).ok_or_else(|| Status::not_found("Position not found after update"))?;
            
            // Get current metrics
            let metrics = handle.block_on(risk_manager.get_metrics());
            
            Ok(UpdatePositionResponse {
            success: true,
            updated_position: Some(ProtoPosition {
                symbol: position.symbol.0.to_string(),
                net_quantity: position.net_qty,
                avg_price: position.avg_price.as_i64(),
                mark_price: position.mark_price.as_i64(),
                unrealized_pnl: position.unrealized_pnl,
                realized_pnl: position.realized_pnl,
                // Safe conversion: position value to protobuf i64
                position_value: if let Ok(val) = i64::try_from(position.position_value) { val } else {
                    warn!("Position value {} exceeds i64 range", position.position_value);
                    PROTO_I64_OVERFLOW_VALUE
                },
                exchange: String::new(),
            }),
            current_metrics: Some(ProtoMetrics {
                // Safe conversions for metric values
                total_exposure: if let Ok(val) = i64::try_from(metrics.total_exposure) { val } else {
                    warn!("Total exposure {} exceeds i64 range", metrics.total_exposure);
                    PROTO_I64_OVERFLOW_VALUE
                },
                current_drawdown: {
                    
                    i32::try_from(metrics.current_drawdown).unwrap_or_else(|_| {
                        warn!("Current drawdown {} exceeds i32 range", metrics.current_drawdown);
                        PROTO_I32_OVERFLOW_VALUE
                    })
                },
                daily_pnl: metrics.daily_pnl,
                open_positions: if let Ok(val) = i32::try_from(metrics.open_positions) { val } else {
                    warn!("Open positions {} exceeds i32 range", metrics.open_positions);
                    PROTO_I32_OVERFLOW_VALUE
                },
                orders_today: if let Ok(val) = i32::try_from(metrics.orders_today) { val } else {
                    warn!("Orders today {} exceeds i32 range", metrics.orders_today);
                    PROTO_I32_OVERFLOW_VALUE
                },
                circuit_breaker_active: metrics.circuit_breaker_active,
                kill_switch_active: metrics.kill_switch_active,
                }),
            })
        }).await
    }
    
    async fn get_positions(
        &self,
        request: Request<GetPositionsRequest>,
    ) -> Result<Response<GetPositionsResponse>, Status> {
        let risk_manager = self.risk_manager.clone();
        
        self.process_request("get_positions", request, move |req| {
            let handle = tokio::runtime::Handle::current();
            
            let positions = if req.symbol.is_empty() {
                // Get all positions
                handle.block_on(risk_manager.get_all_positions())
            } else {
                // Get specific position
                let symbol = Symbol(req.symbol.parse().map_err(|_| Status::invalid_argument("Invalid symbol"))?);
                if let Some(pos) = handle.block_on(risk_manager.get_position(symbol)) {
                    vec![pos]
                } else {
                    vec![]
                }
            };
            
            // Convert to proto positions
            let proto_positions: Vec<ProtoPosition> = positions.iter().map(|p| ProtoPosition {
            symbol: p.symbol.0.to_string(),
            net_quantity: p.net_qty,
            avg_price: p.avg_price.as_i64(),
            mark_price: p.mark_price.as_i64(),
            unrealized_pnl: p.unrealized_pnl,
            realized_pnl: p.realized_pnl,
            position_value: if let Ok(val) = i64::try_from(p.position_value) { val } else {
                warn!("Position value {} exceeds i64 range", p.position_value);
                PROTO_I64_OVERFLOW_VALUE
            },
            exchange: String::new(),
            }).collect();
            
            let total_exposure: u64 = positions.iter().map(|p| p.position_value).sum();
            
            Ok(GetPositionsResponse {
                positions: proto_positions,
                total_exposure: if let Ok(val) = i64::try_from(total_exposure) { val } else {
                    warn!("Total exposure {} exceeds i64 range", total_exposure);
                    PROTO_I64_OVERFLOW_VALUE
                },
            })
        }).await
    }
    
    async fn get_metrics(
        &self,
        request: Request<GetMetricsRequest>,
    ) -> Result<Response<GetMetricsResponse>, Status> {
        let risk_manager = self.risk_manager.clone();
        
        self.process_request("get_metrics", request, move |_req| {
            let handle = tokio::runtime::Handle::current();
            let metrics = handle.block_on(risk_manager.get_metrics());
            
            Ok(GetMetricsResponse {
                metrics: Some(ProtoMetrics {
                total_exposure: if let Ok(val) = i64::try_from(metrics.total_exposure) { val } else {
                    warn!("Total exposure {} exceeds i64 range", metrics.total_exposure);
                    PROTO_I64_OVERFLOW_VALUE
                },
                current_drawdown: metrics.current_drawdown,
                daily_pnl: metrics.daily_pnl,
                open_positions: if let Ok(val) = i32::try_from(metrics.open_positions) { val } else {
                    warn!("Open positions {} exceeds i32 range", metrics.open_positions);
                    PROTO_I32_OVERFLOW_VALUE
                },
                orders_today: if let Ok(val) = i32::try_from(metrics.orders_today) { val } else {
                    warn!("Orders today {} exceeds i32 range", metrics.orders_today);
                    PROTO_I32_OVERFLOW_VALUE
                },
                circuit_breaker_active: metrics.circuit_breaker_active,
                kill_switch_active: metrics.kill_switch_active,
                }),
            })
        }).await
    }
    
    async fn activate_kill_switch(
        &self,
        request: Request<KillSwitchRequest>,
    ) -> Result<Response<KillSwitchResponse>, Status> {
        let risk_manager = self.risk_manager.clone();
        let event_tx = self.event_tx.clone();
        
        self.process_request("activate_kill_switch", request, move |req| {
            // Implement actual kill switch control
            let success = if req.activate {
                // Activate kill switch
                let activated = risk_manager.activate_kill_switch(&req.reason);
                if activated {
                    tracing::error!("Kill switch ACTIVATED by gRPC request: {}", req.reason);
                } else {
                    warn!("Kill switch already active, request ignored");
                }
                activated
            } else {
                // Deactivate kill switch
                let deactivated = risk_manager.deactivate_kill_switch(&req.reason);
                if deactivated {
                    warn!("Kill switch DEACTIVATED by gRPC request: {}", req.reason);
                } else {
                    tracing::info!("Kill switch already inactive, request ignored");
                }
                deactivated
            };
            
            let is_active = risk_manager.is_kill_switch_active();
            
            // Send event about kill switch change
            if success {
                let event = RiskEvent {
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    event_type: if req.activate {
                        RiskEventType::KillSwitchActivated
                    } else {
                        RiskEventType::KillSwitchActivated // Should have KillSwitchDeactivated
                    },
                    symbol: None,
                    message: format!("Kill switch {}: {}", 
                        if req.activate { "activated" } else { "deactivated" },
                        req.reason),
                };
                let _ = event_tx.send(event);
            }
            
            Ok(KillSwitchResponse {
                success,
                is_active,
            })
        }).await
    }
    
    type StreamAlertsStream = Pin<Box<dyn Stream<Item = Result<RiskAlert, Status>> + Send>>;
    
    async fn stream_alerts(
        &self,
        request: Request<StreamAlertsRequest>,
    ) -> Result<Response<Self::StreamAlertsStream>, Status> {
        let req = request.into_inner();
        
        // Filter requested alert levels
        let requested_levels: Vec<_> = req.levels.into_iter().collect();
        
        // Create a stream that monitors for alerts
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        // Subscribe to risk events
        let mut event_rx = self.event_tx.subscribe();
        
        // Start monitoring task
        let monitor = self.monitor.clone();
        let event_tx = self.event_tx.clone();
        
        tokio::spawn(async move {
            info!("Starting alert stream for levels: {:?}", requested_levels);
            
            // Send initial alert
            let _ = tx.send(Ok(RiskAlert {
                // Proto-generated enum already has i32 representation
                level: AlertLevel::Info.into(),
                message: "Risk monitoring started".to_string(),
                timestamp: chrono::Utc::now().timestamp_millis(),
                source: "risk-manager".to_string(),
                // Proto-generated code requires std::collections::HashMap for metadata field
                #[allow(clippy::disallowed_types)]
                metadata: std::collections::HashMap::new(),
            })).await;
            
            // Start monitoring metrics periodically and generate alerts
            const MONITOR_INTERVAL_SECS: u64 = 5;
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(MONITOR_INTERVAL_SECS));
            
            loop {
                tokio::select! {
                    // Check metrics periodically
                    _ = interval.tick() => {
                        if let Ok(metrics) = monitor.get_current_metrics().await {
                            // Check for critical conditions
                            if metrics.daily_pnl < DAILY_LOSS_CRITICAL {
                                let alert = RiskAlert {
                                    // Proto-generated enum already has i32 representation
                                    level: AlertLevel::Critical.into(),
                                    message: format!("Daily loss exceeds limit: ${}", metrics.daily_pnl / FIXED_POINT_DIVISOR),
                                    timestamp: chrono::Utc::now().timestamp_millis(),
                                    source: "risk-monitor".to_string(),
                                    #[allow(clippy::disallowed_types)] // proto-generated metadata field
                                    metadata: std::collections::HashMap::new(),
                                };
                                
                                // Send alert to stream
                                // Proto-generated enum already has i32 representation
                                if (requested_levels.is_empty() || requested_levels.contains(&AlertLevel::Critical.into()))
                                    && tx.send(Ok(alert.clone())).await.is_err() {
                                        break;
                                    }
                                
                                // Also send as event
                                let _ = event_tx.send(RiskEvent {
                                    timestamp: chrono::Utc::now().timestamp_millis(),
                                    event_type: RiskEventType::LimitBreached,
                                    symbol: None,
                                    message: format!("Daily loss limit breached: ${}", metrics.daily_pnl / FIXED_POINT_DIVISOR),
                                });
                            }
                            
                            // Safe conversion: threshold constant to i64 for comparison
                            let threshold_i64 = i64::try_from(DRAWDOWN_CRITICAL_THRESHOLD).unwrap_or_else(|_| {
                                tracing::error!("DRAWDOWN_CRITICAL_THRESHOLD exceeds i64 range");
                                i64::MAX
                            });
                            if metrics.current_drawdown > threshold_i64 {
                                let alert = RiskAlert {
                                    // Proto-generated enum already has i32 representation
                                    level: AlertLevel::Warning.into(),
                                    message: format!("High drawdown detected: {}%", metrics.current_drawdown / i64::from(FIXED_POINT_PERCENT_DIVISOR)),
                                    timestamp: chrono::Utc::now().timestamp_millis(),
                                    source: "risk-monitor".to_string(),
                                    #[allow(clippy::disallowed_types)] // proto-generated metadata field
                                    metadata: std::collections::HashMap::new(),
                                };
                                
                                // Proto-generated enum already has i32 representation
                                if (requested_levels.is_empty() || requested_levels.contains(&AlertLevel::Warning.into()))
                                    && tx.send(Ok(alert)).await.is_err() {
                                        break;
                                    }
                            }
                        }
                    }
                    
                    // Monitor for events and convert to alerts
                    Ok(event) = event_rx.recv() => {
                        let alert_level = match event.event_type {
                            RiskEventType::OrderRejected => AlertLevel::Warning,
                            RiskEventType::LimitBreached => AlertLevel::Critical,
                            RiskEventType::CircuitBreakerTriggered => AlertLevel::Critical,
                            RiskEventType::KillSwitchActivated => AlertLevel::Emergency,
                            _ => AlertLevel::Info,
                        };
                        
                        // Filter by requested levels
                        // Proto-generated enum already has i32 representation
                        if requested_levels.is_empty() || requested_levels.contains(&(alert_level.into())) {
                            let alert = RiskAlert {
                                // Proto-generated enum already has i32 representation
                                level: alert_level.into(),
                                message: event.message,
                                timestamp: event.timestamp,
                                source: "risk-manager".to_string(),
                                #[allow(clippy::disallowed_types)]
                                metadata: std::collections::HashMap::new(),
                            };
                            
                            if tx.send(Ok(alert)).await.is_err() {
                                break; // Client disconnected
                            }
                        }
                    }
                }
            }
        });
        
        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        
        Ok(Response::new(Box::pin(stream) as Self::StreamAlertsStream))
    }
}

