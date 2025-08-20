//! Integration tests module for market-connector service
//! 
//! This module organizes integration tests that test the full market data
//! flow from WebSocket connections through to gRPC clients.

pub mod end_to_end_tests;
pub mod websocket_resilience_tests;
pub mod grpc_integration_tests;