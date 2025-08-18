//! Centralized Logging Service
//! 
//! Collects, processes, and forwards logs from all ShrivenQuant services

use anyhow::Result;
use logging::{LogAggregator, LogEntry, LogForwarder, ForwarderConfig};
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{info, warn, error};

pub mod proto {
    tonic::include_proto!("logging");
}

use proto::logging_service_server::{LoggingService, LoggingServiceServer};
use proto::{LogRequest, LogResponse, LogBatch, GetLogsRequest, GetLogsResponse};

struct LoggingServiceImpl {
    aggregator: Arc<LogAggregator>,
    forwarder: Arc<LogForwarder>,
}

#[tonic::async_trait]
impl LoggingService for LoggingServiceImpl {
    async fn log(&self, request: Request<LogRequest>) -> Result<Response<LogResponse>, Status> {
        let log_req = request.into_inner();
        
        let entry = LogEntry {
            timestamp: chrono::Utc::now(),
            level: match log_req.level.as_str() {
                "TRACE" => logging::LogLevel::Trace,
                "DEBUG" => logging::LogLevel::Debug,
                "INFO" => logging::LogLevel::Info,
                "WARN" => logging::LogLevel::Warn,
                "ERROR" => logging::LogLevel::Error,
                _ => logging::LogLevel::Info,
            },
            service: log_req.service,
            message: log_req.message,
            fields: serde_json::from_str(&log_req.fields).unwrap_or_default(),
            trace_id: if log_req.trace_id.is_empty() { None } else { Some(log_req.trace_id) },
            span_id: if log_req.span_id.is_empty() { None } else { Some(log_req.span_id) },
            correlation_id: if log_req.correlation_id.is_empty() { None } else { Some(log_req.correlation_id) },
        };
        
        // Ingest the log
        self.aggregator.ingest(entry.clone()).await.map_err(|e| {
            Status::internal(format!("Failed to ingest log: {}", e))
        })?;
        
        // Forward to external systems
        if let Err(e) = self.forwarder.forward(&entry).await {
            warn!("Failed to forward log: {}", e);
        }
        
        Ok(Response::new(LogResponse { success: true }))
    }
    
    async fn batch_log(&self, request: Request<LogBatch>) -> Result<Response<LogResponse>, Status> {
        let batch = request.into_inner();
        
        for log_req in batch.logs {
            let entry = LogEntry {
                timestamp: chrono::Utc::now(),
                level: match log_req.level.as_str() {
                    "TRACE" => logging::LogLevel::Trace,
                    "DEBUG" => logging::LogLevel::Debug,
                    "INFO" => logging::LogLevel::Info,
                    "WARN" => logging::LogLevel::Warn,
                    "ERROR" => logging::LogLevel::Error,
                    _ => logging::LogLevel::Info,
                },
                service: log_req.service,
                message: log_req.message,
                fields: serde_json::from_str(&log_req.fields).unwrap_or_default(),
                trace_id: if log_req.trace_id.is_empty() { None } else { Some(log_req.trace_id) },
                span_id: if log_req.span_id.is_empty() { None } else { Some(log_req.span_id) },
                correlation_id: if log_req.correlation_id.is_empty() { None } else { Some(log_req.correlation_id) },
            };
            
            // Ingest the log
            if let Err(e) = self.aggregator.ingest(entry.clone()).await {
                error!("Failed to ingest log: {}", e);
            }
            
            // Forward to external systems
            if let Err(e) = self.forwarder.forward(&entry).await {
                warn!("Failed to forward log: {}", e);
            }
        }
        
        Ok(Response::new(LogResponse { success: true }))
    }
    
    async fn get_logs(&self, request: Request<GetLogsRequest>) -> Result<Response<GetLogsResponse>, Status> {
        let req = request.into_inner();
        
        let logs = self.aggregator.get_service_logs(&req.service, req.limit as usize);
        
        let proto_logs = logs.into_iter().map(|log| {
            proto::LogEntry {
                timestamp: log.timestamp.timestamp_millis(),
                level: format!("{:?}", log.level),
                service: log.service,
                message: log.message,
                fields: log.fields.to_string(),
                trace_id: log.trace_id.unwrap_or_default(),
                span_id: log.span_id.unwrap_or_default(),
                correlation_id: log.correlation_id.unwrap_or_default(),
            }
        }).collect();
        
        Ok(Response::new(GetLogsResponse {
            logs: proto_logs,
        }))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging for this service
    logging::init_logging("logging-service")?;
    
    info!("Starting ShrivenQuant Logging Service");
    
    // Create aggregator with 100k buffer
    let (aggregator, mut receiver) = LogAggregator::new(100_000);
    let aggregator = Arc::new(aggregator);
    
    // Configure forwarder
    let forwarder_config = ForwarderConfig {
        elasticsearch_url: std::env::var("ELASTICSEARCH_URL").ok(),
        loki_url: std::env::var("LOKI_URL").ok(),
        stdout: std::env::var("LOG_STDOUT").map(|v| v == "true").unwrap_or(false),
        file_path: std::env::var("LOG_FILE").ok(),
    };
    let forwarder = Arc::new(LogForwarder::new(forwarder_config));
    
    // Spawn log processor
    let forwarder_clone = forwarder.clone();
    tokio::spawn(async move {
        while let Some(entry) = receiver.recv().await {
            if let Err(e) = forwarder_clone.forward(&entry).await {
                error!("Failed to forward log: {}", e);
            }
        }
    });
    
    // Create gRPC service
    let service = LoggingServiceImpl {
        aggregator: aggregator.clone(),
        forwarder: forwarder.clone(),
    };
    
    // Start gRPC server
    let addr = "0.0.0.0:50058".parse()?;
    info!("Logging service listening on {}", addr);
    
    Server::builder()
        .add_service(LoggingServiceServer::new(service))
        .serve(addr)
        .await?;
    
    Ok(())
}