//! Unit test modules for execution-router service
//!
//! Comprehensive test coverage for all service components

pub mod lib_tests;              // Basic service functionality tests (existing)
pub mod order_slicing_tests;    // Algorithm slicing and timing tests (existing)  
pub mod smart_router_tests;     // Smart router algorithm tests (existing)

// New comprehensive test modules
pub mod memory_tests;           // Memory management (Arena, ObjectPool, RingBuffer)
pub mod venue_tests;            // Venue management and failover
pub mod algorithm_engine_tests; // Advanced algorithm testing
pub mod grpc_tests;             // gRPC service implementation
pub mod error_handling_tests;   // Error scenarios and recovery
pub mod performance_tests;      // Performance and concurrency