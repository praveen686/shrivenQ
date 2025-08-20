//! Memory management module for zero-allocation hot paths
//!
//! COMPLIANCE:
//! - Zero allocations in hot paths
//! - Lock-free operations
//! - Cache-line aligned structures
//! - Pre-allocated pools

mod arena;
mod pools;
mod ring_buffer;

pub use arena::Arena;
pub use pools::{ObjectPool, PoolRef};
pub use ring_buffer::RingBuffer;

/// Memory statistics for monitoring
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryStats {
    /// Number of object pools currently allocated in memory
    pub pools_allocated: usize,
    /// Number of arena memory chunks allocated for large objects
    pub arena_chunks: usize,
    /// Amount of stack memory currently used in bytes
    pub stack_used: usize,
    /// Total memory usage across all allocation types in bytes
    pub total_bytes: usize,
}
