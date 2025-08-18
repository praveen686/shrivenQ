//! Common utilities and client wrappers for inter-service communication

pub mod clients;
pub mod config;
// Use constants from types module instead of separate constants.rs
pub mod errors;
pub mod event_bus;
pub mod proto;
pub mod types;

pub use clients::*;
pub use config::*;
pub use errors::*;
pub use event_bus::*;
// Re-export proto without auth to avoid conflict
pub use proto::{execution, marketdata, risk};
pub use types::*;
