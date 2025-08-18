//! Error types for the Order Management System

use thiserror::Error;

/// OMS-specific error types
#[derive(Error, Debug)]
pub enum OmsError {
    /// Order not found in the system
    #[error("Order not found: {order_id}")]
    OrderNotFound { order_id: String },

    /// Order is in an invalid state for the requested operation
    #[error("Order {order_id} cannot be {operation} in current state {current_state}")]
    InvalidOrderState {
        order_id: String,
        operation: String,
        current_state: String,
    },

    /// Invalid quantity specified
    #[error("Invalid quantity: {reason}")]
    InvalidQuantity { reason: String },

    /// Persistence layer error
    #[error("Persistence error: {0}")]
    Persistence(#[from] anyhow::Error),

    /// Validation error
    #[error("Validation error: {message}")]
    Validation { message: String },

    /// Risk check failure
    #[error("Risk check failed: {reason}")]
    RiskCheckFailed { reason: String },

    /// Audit trail error
    #[error("Audit trail error: {0}")]
    AuditError(#[from] std::io::Error),

    /// Configuration error
    #[error("Configuration error: {message}")]
    Configuration { message: String },

    /// System capacity exceeded
    #[error("System capacity exceeded: {details}")]
    CapacityExceeded { details: String },
    
    /// Date/time parsing error
    #[error("Invalid date/time: {context}")]
    InvalidDateTime { context: String },
}

/// Type alias for OMS results
pub type OmsResult<T> = Result<T, OmsError>;