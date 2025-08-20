//! Centralized Logging Service for ShrivenQuant
//! 
//! Provides structured logging, aggregation, and forwarding to external systems

use anyhow::Result;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{Level, Metadata, debug};

/// Log entry structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// UTC timestamp when the log entry was created
    pub timestamp: DateTime<Utc>,
    /// Log level (Trace, Debug, Info, Warn, Error)
    pub level: LogLevel,
    /// Name of the service that generated this log entry
    pub service: String,
    /// The log message content
    pub message: String,
    /// Additional structured fields as JSON
    pub fields: serde_json::Value,
    /// Distributed tracing trace ID
    pub trace_id: Option<String>,
    /// Distributed tracing span ID
    pub span_id: Option<String>,
    /// Request correlation ID for tracking requests across services
    pub correlation_id: Option<String>,
}

/// Log level enumeration for categorizing log entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    /// Trace level - very detailed information, typically of interest only during debugging
    Trace,
    /// Debug level - detailed information for debugging purposes
    Debug,
    /// Info level - informational messages that highlight the progress of the application
    Info,
    /// Warn level - warning messages for potentially harmful situations
    Warn,
    /// Error level - error events that might still allow the application to continue running
    Error,
}

impl From<Level> for LogLevel {
    fn from(level: Level) -> Self {
        match level {
            Level::TRACE => LogLevel::Trace,
            Level::DEBUG => LogLevel::Debug,
            Level::INFO => LogLevel::Info,
            Level::WARN => LogLevel::Warn,
            Level::ERROR => LogLevel::Error,
        }
    }
}

/// Log aggregator for collecting logs from all services
pub struct LogAggregator {
    buffer: Arc<DashMap<String, Vec<LogEntry>>>,
    sender: mpsc::Sender<LogEntry>,
    buffer_limit: usize,
}

impl std::fmt::Debug for LogAggregator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogAggregator")
            .field("buffer_keys", &self.buffer.iter().map(|entry| entry.key().clone()).collect::<Vec<_>>())
            .field("sender_capacity", &"<channel_sender>")
            .field("buffer_limit", &self.buffer_limit)
            .finish()
    }
}

impl LogAggregator {
    /// Creates a new log aggregator with the specified buffer size
    /// 
    /// Returns a tuple containing the aggregator and a receiver for processing log entries
    pub fn new(buffer_size: usize) -> (Self, mpsc::Receiver<LogEntry>) {
        let (sender, receiver) = mpsc::channel(buffer_size);
        
        (Self {
            buffer: Arc::new(DashMap::new()),
            sender,
            buffer_limit: buffer_size,
        }, receiver)
    }
    
    /// Ingest a log entry
    pub async fn ingest(&self, entry: LogEntry) -> Result<()> {
        // Add to service-specific buffer with size limit
        {
            let mut service_logs = self.buffer
                .entry(entry.service.clone())
                .or_insert_with(Vec::new);
            
            // If buffer is at capacity, remove oldest entries (FIFO)
            if service_logs.len() >= self.buffer_limit {
                let excess_count = service_logs.len() - self.buffer_limit + 1;
                debug!(
                    service = %entry.service,
                    buffer_limit = self.buffer_limit,
                    current_size = service_logs.len(),
                    removing_entries = excess_count,
                    "Service log buffer at capacity, removing oldest entries"
                );
                service_logs.drain(0..excess_count);
            }
            
            service_logs.push(entry.clone());
        }
        
        // Send to processing pipeline
        self.sender.send(entry).await?;
        
        Ok(())
    }
    
    /// Get recent logs for a service
    pub fn get_service_logs(&self, service: &str, limit: usize) -> Vec<LogEntry> {
        self.buffer
            .get(service)
            .map(|logs| {
                let len = logs.len();
                let start = if len > limit { len - limit } else { 0 };
                logs[start..].to_vec()
            })
            .unwrap_or_default()
    }
}

/// Log forwarder for sending logs to external systems
#[derive(Debug)]
pub struct LogForwarder {
    config: ForwarderConfig,
}

/// Configuration for log forwarding destinations
#[derive(Debug, Clone)]
pub struct ForwarderConfig {
    /// Optional Elasticsearch URL for log forwarding
    pub elasticsearch_url: Option<String>,
    /// Optional Loki URL for log forwarding
    pub loki_url: Option<String>,
    /// Whether to output logs to stdout
    pub stdout: bool,
    /// Optional file path for log output
    pub file_path: Option<String>,
}

impl LogForwarder {
    /// Creates a new log forwarder with the specified configuration
    pub fn new(config: ForwarderConfig) -> Self {
        Self { config }
    }
    
    /// Forward log entry to configured destinations
    pub async fn forward(&self, entry: &LogEntry) -> Result<()> {
        // JSON format for structured logging
        let json = serde_json::to_string(entry)?;
        
        // Write to stdout if enabled
        if self.config.stdout {
            println!("{}", json);
        }
        
        // Forward to Elasticsearch
        if let Some(url) = &self.config.elasticsearch_url {
            self.forward_to_elasticsearch(url, entry).await?;
        }
        
        // Forward to Loki
        if let Some(url) = &self.config.loki_url {
            self.forward_to_loki(url, entry).await?;
        }
        
        Ok(())
    }
    
    async fn forward_to_elasticsearch(&self, _url: &str, _entry: &LogEntry) -> Result<()> {
        // Implementation for Elasticsearch forwarding
        // Would use reqwest or elasticsearch-rs client
        Ok(())
    }
    
    async fn forward_to_loki(&self, _url: &str, _entry: &LogEntry) -> Result<()> {
        // Implementation for Loki forwarding
        // Would use HTTP API
        Ok(())
    }
}

/// Check if metadata should be included based on level and target
pub fn should_include_metadata(metadata: &Metadata<'_>) -> bool {
    // Filter based on target and level
    let target = metadata.target();
    let level = metadata.level();
    
    // Always include ERROR and WARN levels
    if level <= &Level::WARN {
        return true;
    }
    
    // Include INFO for our services
    if level == &Level::INFO && target.starts_with("shrivenquant") {
        return true;
    }
    
    // Include DEBUG for specific modules during development
    if level == &Level::DEBUG {
        return target.starts_with("shrivenquant::") 
            || target.starts_with("trading_gateway::")
            || target.starts_with("execution_router::");
    }
    
    // Exclude TRACE unless explicitly requested
    false
}

/// Initialize centralized logging for a service
pub fn init_logging(service_name: &str) -> Result<()> {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
    
    // JSON formatting for structured logs
    let json_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(true)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true);
    
    // Console output for development
    let console_layer = tracing_subscriber::fmt::layer()
        .pretty()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);
    
    // Environment filter
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    
    // Combine layers based on environment
    let is_production = std::env::var("SHRIVENQUANT_ENV")
        .map(|e| e == "production")
        .unwrap_or(false);
    
    if is_production {
        tracing_subscriber::registry()
            .with(filter)
            .with(json_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(console_layer)
            .init();
    }
    
    tracing::info!(
        service = service_name,
        version = env!("CARGO_PKG_VERSION"),
        "Service initialized"
    );
    
    Ok(())
}

/// Correlation ID for request tracing
pub fn generate_correlation_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    
    let timestamp = Utc::now().timestamp_micros();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    
    format!("{:x}-{:x}", timestamp, counter)
}

/// Log rotation configuration
#[derive(Debug, Clone)]
pub struct RotationConfig {
    /// Maximum size of a log file in megabytes before rotation
    pub max_size_mb: u64,
    /// Maximum age of log files in days before deletion
    pub max_age_days: u32,
    /// Maximum number of backup log files to keep
    pub max_backups: u32,
    /// Whether to compress rotated log files
    pub compress: bool,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            max_size_mb: 100,
            max_age_days: 7,
            max_backups: 10,
            compress: true,
        }
    }
}