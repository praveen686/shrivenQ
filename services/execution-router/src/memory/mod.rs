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
    pub pools_allocated: usize,
    pub arena_chunks: usize,
    pub stack_used: usize,
    pub total_bytes: usize,
}
