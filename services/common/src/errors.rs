//! Common error types for services

use thiserror::Error;

/// Service error types
#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Timeout: {0}")]
    Timeout(String),

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
