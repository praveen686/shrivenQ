//! Production-grade instrument management with WAL storage
//!
//! COMPLIANCE:
//! - Zero allocations in hot paths
//! - Fixed-point arithmetic for all financial data
//! - FxHashMap for performance
//! - Proper error handling with anyhow::Context

pub mod service;
pub mod store;
pub mod types;

pub use service::{InstrumentService, InstrumentServiceConfig};
pub use store::InstrumentWalStore;
pub use types::*;
