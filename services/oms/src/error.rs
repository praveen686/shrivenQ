//! Error types for the Order Management System

use thiserror::Error;

/// OMS-specific error types
#[derive(Error, Debug)]
pub enum OmsError {
    /// Order not found in the system
    #[error("Order not found: {order_id}")]
    OrderNotFound { 
        /// The identifier of the order that could not be found
        order_id: String 
    },

    /// Order is in an invalid state for the requested operation
    #[error("Order {order_id} cannot be {operation} in current state {current_state}")]
    InvalidOrderState {
        /// The identifier of the order in invalid state
        order_id: String,
        /// The operation that was attempted on the order
        operation: String,
        /// The current state of the order that prevents the operation
        current_state: String,
    },

    /// Invalid quantity specified
    #[error("Invalid quantity: {reason}")]
    InvalidQuantity { 
        /// The reason why the quantity is invalid
        reason: String 
    },

    /// Persistence layer error
    #[error("Persistence error: {0}")]
    Persistence(#[from] anyhow::Error),

    /// Validation error
    #[error("Validation error: {message}")]
    Validation { 
        /// Detailed validation error message
        message: String 
    },

    /// Risk check failure
    #[error("Risk check failed: {reason}")]
    RiskCheckFailed { 
        /// The reason why the risk check failed
        reason: String 
    },

    /// Audit trail error
    #[error("Audit trail error: {0}")]
    AuditError(#[from] std::io::Error),

    /// Configuration error
    #[error("Configuration error: {message}")]
    Configuration { 
        /// Configuration error message detailing what went wrong
        message: String 
    },

    /// System capacity exceeded
    #[error("System capacity exceeded: {details}")]
    CapacityExceeded { 
        /// Details about which capacity limit was exceeded
        details: String 
    },
    
    /// Date/time parsing error
    #[error("Invalid date/time: {context}")]
    InvalidDateTime { 
        /// Context information about the invalid date/time
        context: String 
    },
}

/// Type alias for OMS results
pub type OmsResult<T> = Result<T, OmsError>;