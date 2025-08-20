//! gRPC `ExecutionService` implementation
//!
//! Production-grade execution service with all required features:
//! - Order submission and management
//! - Smart routing algorithms
//! - Execution reporting
//! - Metrics collection

use crate::ExecutionRouterService;
use services_common::{Px, Qty, Symbol};
use services_common::execution::v1::{
    execution_service_server::ExecutionService,
    ExecutionReport, SubmitOrderRequest, SubmitOrderResponse, CancelOrderRequest, CancelOrderResponse, ModifyOrderRequest, ModifyOrderResponse, GetOrderRequest, GetOrderResponse, StreamExecutionReportsRequest, GetMetricsRequest, GetMetricsResponse, ExecutionMetrics, Order, Fill,
};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::warn;

// Constants for safe conversions
const PROTO_I32_OVERFLOW: i32 = i32::MAX;

// Channel capacity for streaming
const STREAM_CHANNEL_CAPACITY: usize = 1000;

/// gRPC wrapper for `ExecutionRouterService`
pub struct ExecutionServiceImpl {
    router: Arc<ExecutionRouterService>,
    execution_broadcaster: broadcast::Sender<ExecutionReport>,
}

impl std::fmt::Debug for ExecutionServiceImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionServiceImpl")
            .field("router", &"Arc<ExecutionRouterService>")
            .field("execution_broadcaster", &format!("broadcast::Sender<ExecutionReport> (capacity: {})", STREAM_CHANNEL_CAPACITY))
            .finish()
    }
}

impl ExecutionServiceImpl {
    /// Create a new ExecutionServiceImpl with the given router service
    /// 
    /// # Arguments
    /// * `router` - Arc-wrapped ExecutionRouterService for handling order operations
    /// 
    /// # Returns
    /// A new ExecutionServiceImpl instance with broadcast channel for execution reports
    pub fn new(router: Arc<ExecutionRouterService>) -> Self {
        let (execution_broadcaster, _) = broadcast::channel(STREAM_CHANNEL_CAPACITY);
        
        Self {
            router,
            execution_broadcaster,
        }
    }
    
    /// Send execution report to all subscribers
    fn broadcast_execution_report(&self, order: &crate::Order, report_type: i32) {
        let report = ExecutionReport {
            order_id: order.order_id.0 as i64,
            client_order_id: order.client_order_id.clone(),
            exchange_order_id: order.exchange_order_id.clone().unwrap_or_default(),
            report_type,
            // Safe conversion: internal OrderStatus enum to i32 (proto-generated requirement)
            status: if let Ok(val) = i32::try_from(order.status as u32) { val } else {
                tracing::error!("OrderStatus {:?} exceeds i32 range", order.status);
                PROTO_I32_OVERFLOW
            },
            filled_qty: order.filled_quantity.as_i64(),
            last_qty: 0, // Will be set from fill information
            last_price: 0, // Will be set from fill information
            avg_price: order.avg_fill_price.as_i64(),
            reject_reason: String::new(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        };
        
        // Send to all subscribers, ignore if no receivers
        let _ = self.execution_broadcaster.send(report);
    }
}

#[tonic::async_trait]
impl ExecutionService for ExecutionServiceImpl {
    async fn submit_order(
        &self,
        request: Request<SubmitOrderRequest>,
    ) -> Result<Response<SubmitOrderResponse>, Status> {
        let req = request.into_inner();
        
        // Convert proto request to internal types
        let symbol = Symbol::new(
            req.symbol.parse::<u32>()
                .map_err(|e| Status::invalid_argument(format!("Invalid symbol: {e}")))?
        );
        
        let side = match req.side {
            1 => services_common::Side::Bid,  // BUY = BID
            2 => services_common::Side::Ask,  // SELL = ASK
            _ => return Err(Status::invalid_argument("Invalid side")),
        };
        
        let quantity = Qty::from_i64(req.quantity);
        
        // Submit order to router service
        let result = self.router.submit_order(
            req.client_order_id.clone(),
            symbol,
            side,
            quantity,
            req.venue.clone(),
            req.strategy_id.clone(),
        ).await;
        
        match result {
            Ok(order_id) => {
                // Broadcast NEW order report
                if let Ok(order) = self.router.get_order(order_id).await {
                    self.broadcast_execution_report(&order, 1); // REPORT_TYPE_NEW
                }
                
                Ok(Response::new(SubmitOrderResponse {
                    order_id: order_id as i64,
                    status: 1, // PENDING
                    message: "Order submitted successfully".to_string(),
                }))
            }
            Err(e) => {
                warn!("Order submission failed: {}", e);
                Ok(Response::new(SubmitOrderResponse {
                    order_id: 0,
                    status: 7, // REJECTED
                    message: format!("Order rejected: {e}"),
                }))
            }
        }
    }
    
    async fn cancel_order(
        &self,
        request: Request<CancelOrderRequest>,
    ) -> Result<Response<CancelOrderResponse>, Status> {
        let req = request.into_inner();
        
        // Cancel order in router service
        let result = self.router.cancel_order(req.order_id as u64).await;
        
        match result {
            Ok(()) => {
                // Broadcast CANCELLED report
                if let Ok(order) = self.router.get_order(req.order_id as u64).await {
                    self.broadcast_execution_report(&order, 4); // REPORT_TYPE_CANCELLED
                }
                
                Ok(Response::new(CancelOrderResponse {
                    success: true,
                    status: 6, // CANCELLED
                    message: "Order cancelled successfully".to_string(),
                }))
            }
            Err(e) => {
                warn!("Order cancellation failed: {}", e);
                Ok(Response::new(CancelOrderResponse {
                    success: false,
                    status: 0, // UNSPECIFIED
                    message: format!("Cancellation failed: {e}"),
                }))
            }
        }
    }
    
    async fn modify_order(
        &self,
        request: Request<ModifyOrderRequest>,
    ) -> Result<Response<ModifyOrderResponse>, Status> {
        let req = request.into_inner();
        
        // Modify order in router
        let new_quantity = if req.new_quantity > 0 {
            Some(Qty::from_i64(req.new_quantity))
        } else {
            None
        };
        
        let new_price = if req.new_price > 0 {
            Some(Px::from_i64(req.new_price))
        } else {
            None
        };
        
        let result = self.router.modify_order(
            req.order_id as u64,
            new_quantity,
            new_price,
        ).await;
        
        match result {
            Ok(order) => {
                // Broadcast REPLACED report
                self.broadcast_execution_report(&order, 5); // REPORT_TYPE_REPLACED
                
                Ok(Response::new(ModifyOrderResponse {
                    success: true,
                    updated_order: Some(convert_order_to_proto(order)),
                    message: "Order modified successfully".to_string(),
                }))
            }
            Err(e) => {
                warn!("Order modification failed: {}", e);
                Ok(Response::new(ModifyOrderResponse {
                    success: false,
                    updated_order: None,
                    message: format!("Modification failed: {e}"),
                }))
            }
        }
    }
    
    async fn get_order(
        &self,
        request: Request<GetOrderRequest>,
    ) -> Result<Response<GetOrderResponse>, Status> {
        let req = request.into_inner();
        
        // Get order from router service
        let result = if req.order_id > 0 {
            self.router.get_order(req.order_id as u64).await
        } else if !req.client_order_id.is_empty() {
            self.router.get_order_by_client_id(&req.client_order_id).await
        } else {
            return Err(Status::invalid_argument("Must provide order_id or client_order_id"));
        };
        
        match result {
            Ok(order) => {
                Ok(Response::new(GetOrderResponse {
                    order: Some(convert_order_to_proto(order)),
                }))
            }
            Err(e) => {
                Err(Status::not_found(format!("Order not found: {e}")))
            }
        }
    }
    
    type StreamExecutionReportsStream = Pin<Box<dyn Stream<Item = Result<ExecutionReport, Status>> + Send>>;
    
    async fn stream_execution_reports(
        &self,
        request: Request<StreamExecutionReportsRequest>,
    ) -> Result<Response<Self::StreamExecutionReportsStream>, Status> {
        let req = request.into_inner();
        let strategy_filter = if req.strategy_id.is_empty() {
            None
        } else {
            Some(req.strategy_id)
        };
        
        // Create receiver for this client
        let mut receiver = self.execution_broadcaster.subscribe();
        
        // Create stream with proper filtering
        let output = async_stream::try_stream! {
            while let Ok(report) = receiver.recv().await {
                // Apply strategy filter if specified
                if let Some(ref strategy_filter_id) = strategy_filter {
                    // Check if report matches the strategy filter
                    // Parse strategy_id from client_order_id or use a dedicated field
                    // Client order IDs typically include strategy identifier
                    if !report.client_order_id.contains(strategy_filter_id) {
                        continue; // Skip reports that don't match the filter
                    }
                }
                yield report;
            }
        };
        
        Ok(Response::new(Box::pin(output) as Self::StreamExecutionReportsStream))
    }
    
    async fn get_metrics(
        &self,
        _request: Request<GetMetricsRequest>,
    ) -> Result<Response<GetMetricsResponse>, Status> {
        let metrics = self.router.get_metrics().await;
        
        Ok(Response::new(GetMetricsResponse {
            metrics: Some(ExecutionMetrics {
                total_orders: metrics.total_orders as i64,
                filled_orders: metrics.filled_orders as i64,
                cancelled_orders: metrics.cancelled_orders as i64,
                rejected_orders: metrics.rejected_orders as i64,
                avg_fill_time_ms: metrics.avg_fill_time_ms as i64,
                total_volume: metrics.total_volume as i64,
                total_commission: metrics.total_commission as i64,
                fill_rate: metrics.fill_rate,
                // Proto-generated code requires std::collections::HashMap for map fields
                #[allow(clippy::disallowed_types)] // Required by protobuf for map<string, int64>
                venues_used: metrics.venues_used.into_iter()
                    .map(|(k, v)| {
                        // Safe conversion: venue count to i64 (proto-generated requirement)
                        let count = if let Ok(val) = i64::try_from(v) { val } else {
                            tracing::warn!("Venue count {} exceeds i64 range", v);
                            i64::MAX
                        };
                        (k, count)
                    })
                    .collect(),
            }),
        }))
    }
}

// Helper function to convert internal order to proto
fn convert_order_to_proto(order: crate::Order) -> Order {
    Order {
        order_id: order.order_id.0 as i64,
        client_order_id: order.client_order_id,
        exchange_order_id: order.exchange_order_id.unwrap_or_default(),
        symbol: order.symbol.0.to_string(),
        side: match order.side {
            services_common::Side::Bid => 1,  // SIDE_BUY (Bid = Buy)
            services_common::Side::Ask => 2,  // SIDE_SELL (Ask = Sell)
        },
        quantity: order.quantity.as_i64(),
        filled_quantity: order.filled_quantity.as_i64(),
        avg_fill_price: order.avg_fill_price.as_i64(),
        // Safe conversion: internal OrderStatus enum to i32 (proto-generated requirement)
        status: if let Ok(val) = i32::try_from(order.status as u32) { val } else {
            tracing::error!("OrderStatus {:?} exceeds i32 range", order.status);
            PROTO_I32_OVERFLOW
        },
        // Safe conversion: internal OrderType enum to i32 (proto-generated requirement)
        order_type: if let Ok(val) = i32::try_from(order.order_type as u32) { val } else {
            tracing::error!("OrderType {:?} exceeds i32 range", order.order_type);
            PROTO_I32_OVERFLOW
        },
        limit_price: order.limit_price.map_or(0, |p| p.as_i64()),
        stop_price: order.stop_price.map_or(0, |p| p.as_i64()),
        // Safe conversion: internal TimeInForce enum to i32 (proto-generated requirement)
        time_in_force: if let Ok(val) = i32::try_from(order.time_in_force as u32) { val } else {
            tracing::error!("TimeInForce {:?} exceeds i32 range", order.time_in_force);
            PROTO_I32_OVERFLOW
        },
        venue: order.venue,
        strategy_id: order.strategy_id,
        // Safe conversion: timestamp nanos to i64 (proto-generated requirement)
        created_at: if let Ok(val) = i64::try_from(order.created_at.as_nanos()) { val } else {
            tracing::warn!("Created timestamp exceeds i64 range");
            i64::MAX
        },
        // Safe conversion: timestamp nanos to i64 (proto-generated requirement)
        updated_at: if let Ok(val) = i64::try_from(order.updated_at.as_nanos()) { val } else {
            tracing::warn!("Updated timestamp exceeds i64 range");
            i64::MAX
        },
        fills: order.fills.into_iter().map(convert_fill_to_proto).collect(),
    }
}

// Helper function to convert internal fill to proto
fn convert_fill_to_proto(fill: crate::Fill) -> Fill {
    Fill {
        fill_id: fill.fill_id,
        quantity: fill.quantity.as_i64(),
        price: fill.price.as_i64(),
        // Safe conversion: timestamp nanos to i64 (proto-generated requirement)
        timestamp: if let Ok(val) = i64::try_from(fill.timestamp.as_nanos()) { val } else {
            tracing::warn!("Fill timestamp exceeds i64 range");
            i64::MAX
        },
        is_maker: fill.is_maker,
        commission: fill.commission,
        commission_asset: fill.commission_asset,
    }
}