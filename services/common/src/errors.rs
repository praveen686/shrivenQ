//! Common error types for services

use thiserror::Error;

/// Service error types
#[derive(Debug, Error)]
pub enum ServiceError {
    /// Connection failed error
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Authentication failed error
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    /// Service unavailable error
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    /// Invalid request error
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Internal error
    #[error("Internal error: {0}")]
    InternalError(String),

    /// Timeout error
    #[error("Timeout: {0}")]
    Timeout(String),

    /// Rate limited error
    #[error("Rate limited: {0}")]
    RateLimited(String),
}

impl From<tonic::Status> for ServiceError {
    fn from(status: tonic::Status) -> Self {
        match status.code() {
            tonic::Code::Unauthenticated => {
                Self::AuthenticationFailed(status.message().to_string())
            }
            tonic::Code::Unavailable => {
                Self::ServiceUnavailable(status.message().to_string())
            }
            tonic::Code::InvalidArgument => {
                Self::InvalidRequest(status.message().to_string())
            }
            tonic::Code::DeadlineExceeded => Self::Timeout(status.message().to_string()),
            tonic::Code::ResourceExhausted => {
                Self::RateLimited(status.message().to_string())
            }
            _ => Self::InternalError(status.message().to_string()),
        }
    }
}
