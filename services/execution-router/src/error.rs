//! Execution Router Error Types
//!
//! Specific error types for production-grade error handling

use thiserror::Error;

/// Execution router errors
#[derive(Debug, Error)]
pub enum ExecutionError {
    /// Order not found
    #[error("Order not found: {id}")]
    OrderNotFound { id: u64 },
    
    /// Client order ID not found
    #[error("Client order ID not found: {client_id}")]
    ClientOrderNotFound { client_id: String },
    
    /// Cannot cancel filled order
    #[error("Cannot cancel filled order: {id}")]
    CannotCancelFilledOrder { id: u64 },
    
    /// Cannot modify filled order
    #[error("Cannot modify filled order: {id}")]
    CannotModifyFilledOrder { id: u64 },
    
    /// Risk check failed
    #[error("Risk check failed: {reason}")]
    RiskCheckFailed { reason: String },
    
    /// Unsupported venue
    #[error("Unsupported venue: {venue}")]
    UnsupportedVenue { venue: String },
    
    /// Exchange submission failed
    #[error("Exchange submission failed: {reason}")]
    ExchangeSubmissionFailed { reason: String },
    
    /// Exchange cancellation failed
    #[error("Exchange cancellation failed: {reason}")]
    ExchangeCancellationFailed { reason: String },
    
    /// Exchange modification failed  
    #[error("Exchange modification failed: {reason}")]
    ExchangeModificationFailed { reason: String },
    
    /// Invalid order parameters
    #[error("Invalid order parameters: {reason}")]
    InvalidOrderParameters { reason: String },
    
    /// Service unavailable
    #[error("Service unavailable: {service}")]
    ServiceUnavailable { service: String },
    
    /// Internal error
    #[error("Internal error: {reason}")]
    InternalError { reason: String },
    
    /// Algorithm execution failed
    #[error("Algorithm execution failed: {reason}")]
    AlgorithmExecutionFailed { reason: String },
    
    /// Unsupported algorithm
    #[error("Unsupported algorithm: {algorithm}")]
    UnsupportedAlgorithm { algorithm: String },
    
    /// Venue not connected
    #[error("Venue not connected: {venue}")]
    VenueNotConnected { venue: String },
    
    /// Venue not found
    #[error("Venue not found: {venue}")]
    VenueNotFound { venue: String },
    
    /// No venues available
    #[error("No venues available")]
    NoVenuesAvailable,
    
    /// No market data available
    #[error("No market data available for symbol: {symbol}")]
    NoMarketData { symbol: u32 },
    
    /// Order already terminal
    #[error("Order already terminal: {order_id}")]
    OrderAlreadyTerminal { order_id: u64 },
    
    /// Order not found
    #[error("Order not found: {order_id}")]
    OrderNotFoundById { order_id: u64 },
    
    /// Market data service error
    #[error("Market data service error: {error}")]
    MarketDataServiceError { error: String },
    
    /// WebSocket connection failed
    #[error("WebSocket connection failed: {error}")]
    WebSocketConnectionFailed { error: String },
    
    /// OrderBook parse error
    #[error("OrderBook parse error: {error}")]
    OrderBookParseError { error: String },
    
    /// Unexpected message format
    #[error("Unexpected message format")]
    UnexpectedMessageFormat,
    
    /// Market data timeout
    #[error("Market data timeout")]
    MarketDataTimeout,
}

/// Result type for execution operations
pub type ExecutionResult<T> = Result<T, ExecutionError>;