//! Execution Router Error Types
//!
//! Specific error types for production-grade error handling

use thiserror::Error;

/// Execution router errors
#[derive(Debug, Error)]
pub enum ExecutionError {
    /// Order not found
    #[error("Order not found: {id}")]
    OrderNotFound { 
        /// The unique identifier of the order that was not found
        id: u64 
    },
    
    /// Client order ID not found
    #[error("Client order ID not found: {client_id}")]
    ClientOrderNotFound { 
        /// The client-provided order identifier that was not found
        client_id: String 
    },
    
    /// Cannot cancel filled order
    #[error("Cannot cancel filled order: {id}")]
    CannotCancelFilledOrder { 
        /// The unique identifier of the filled order that cannot be cancelled
        id: u64 
    },
    
    /// Cannot modify filled order
    #[error("Cannot modify filled order: {id}")]
    CannotModifyFilledOrder { 
        /// The unique identifier of the filled order that cannot be modified
        id: u64 
    },
    
    /// Risk check failed
    #[error("Risk check failed: {reason}")]
    RiskCheckFailed { 
        /// The specific reason why the risk check failed
        reason: String 
    },
    
    /// Unsupported venue
    #[error("Unsupported venue: {venue}")]
    UnsupportedVenue { 
        /// The name of the trading venue that is not supported
        venue: String 
    },
    
    /// Exchange submission failed
    #[error("Exchange submission failed: {reason}")]
    ExchangeSubmissionFailed { 
        /// The specific reason why the order submission to the exchange failed
        reason: String 
    },
    
    /// Exchange cancellation failed
    #[error("Exchange cancellation failed: {reason}")]
    ExchangeCancellationFailed { 
        /// The specific reason why the order cancellation at the exchange failed
        reason: String 
    },
    
    /// Exchange modification failed  
    #[error("Exchange modification failed: {reason}")]
    ExchangeModificationFailed { 
        /// The specific reason why the order modification at the exchange failed
        reason: String 
    },
    
    /// Invalid order parameters
    #[error("Invalid order parameters: {reason}")]
    InvalidOrderParameters { 
        /// The specific reason why the order parameters are invalid
        reason: String 
    },
    
    /// Service unavailable
    #[error("Service unavailable: {service}")]
    ServiceUnavailable { 
        /// The name of the service that is currently unavailable
        service: String 
    },
    
    /// Internal error
    #[error("Internal error: {reason}")]
    InternalError { 
        /// The specific reason for the internal system error
        reason: String 
    },
    
    /// Algorithm execution failed
    #[error("Algorithm execution failed: {reason}")]
    AlgorithmExecutionFailed { 
        /// The specific reason why the trading algorithm execution failed
        reason: String 
    },
    
    /// Unsupported algorithm
    #[error("Unsupported algorithm: {algorithm}")]
    UnsupportedAlgorithm { 
        /// The name of the trading algorithm that is not supported
        algorithm: String 
    },
    
    /// Venue not connected
    #[error("Venue not connected: {venue}")]
    VenueNotConnected { 
        /// The name of the trading venue that is not currently connected
        venue: String 
    },
    
    /// Venue not found
    #[error("Venue not found: {venue}")]
    VenueNotFound { 
        /// The name of the trading venue that was not found in the system
        venue: String 
    },
    
    /// No venues available
    #[error("No venues available")]
    NoVenuesAvailable,
    
    /// No market data available
    #[error("No market data available for symbol: {symbol}")]
    NoMarketData { 
        /// The symbol identifier for which market data is not available
        symbol: u32 
    },
    
    /// Order already terminal
    #[error("Order already terminal: {order_id}")]
    OrderAlreadyTerminal { 
        /// The unique identifier of the order that is already in a terminal state
        order_id: u64 
    },
    
    /// Order not found
    #[error("Order not found: {order_id}")]
    OrderNotFoundById { 
        /// The unique identifier of the order that was not found
        order_id: u64 
    },
    
    /// Market data service error
    #[error("Market data service error: {error}")]
    MarketDataServiceError { 
        /// The specific error message from the market data service
        error: String 
    },
    
    /// WebSocket connection failed
    #[error("WebSocket connection failed: {error}")]
    WebSocketConnectionFailed { 
        /// The specific error message from the WebSocket connection failure
        error: String 
    },
    
    /// `OrderBook` parse error
    #[error("OrderBook parse error: {error}")]
    OrderBookParseError { 
        /// The specific error message from parsing the order book data
        error: String 
    },
    
    /// Unexpected message format
    #[error("Unexpected message format")]
    UnexpectedMessageFormat,
    
    /// Market data timeout
    #[error("Market data timeout")]
    MarketDataTimeout,
}

/// Result type for execution operations
pub type ExecutionResult<T> = Result<T, ExecutionError>;