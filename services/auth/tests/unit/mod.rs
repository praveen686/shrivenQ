//! Unit tests for the authentication service
//!
//! This module contains all unit tests organized by component

pub mod auth_service_tests;
pub mod binance_service_tests;
pub mod zerodha_service_tests;
pub mod grpc_service_tests;
pub mod token_management_tests;
pub mod error_handling_tests;
pub mod concurrency_tests;
pub mod security_tests;
pub mod rate_limiting_tests;
pub mod orchestrator_tests;

// Common test utilities
pub mod test_utils;