//! Arena allocator for bulk allocations
//!
//! COMPLIANCE:
//! - Pre-allocated chunks
//! - No allocations in hot paths
//! - Cache-line aligned
//! - Fast reset for reuse

use services_common::constants::memory::CACHE_LINE_SIZE;
use std::alloc::{Layout, alloc, dealloc};
use std::mem::{align_of, size_of};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Arena allocator for bulk allocations
///
/// Pre-allocates memory in chunks for fast allocation
/// Reset to reuse without deallocation
pub struct Arena {
    chunks: Vec<ArenaChunk>,
    current: AtomicUsize,
    chunk_size: usize,
}

struct ArenaChunk {
    data: NonNull<u8>,
    size: usize,
    used: AtomicUsize,
}

// Safety: Arena only hands out references with proper lifetime bounds
unsafe impl Send for Arena {}
unsafe impl Sync for Arena {}

impl Arena {
    /// Create arena with specified chunk size
    ///
    /// Pre-allocates first chunk
    /// All chunks are cache-line aligned (`CACHE_LINE_SIZE` bytes)
    pub fn new(chunk_size: usize) -> Result<Self, String> {
        let mut chunks = Vec::with_capacity(16);

        // Pre-allocate first chunk
        if chunk_size > 0 {
            // Ensure alignment is power of 2 and size is multiple of alignment
            const ALIGN: usize = CACHE_LINE_SIZE;
            let size = (chunk_size + ALIGN - 1) & !(ALIGN - 1); // Round up to alignment

            let layout = Layout::from_size_align(size, ALIGN)
                .map_err(|e| format!("Invalid layout: {e}"))?;

            let data = unsafe {
                let ptr = alloc(layout);
                if ptr.is_null() {
                    return Err(format!("Failed to allocate {size} bytes for arena chunk"));
                }
                NonNull::new_unchecked(ptr)
            };

            chunks.push(ArenaChunk {
                data,
                size,
                used: AtomicUsize::new(0),
            });
        }

        Ok(Self {
            chunks,
            current: AtomicUsize::new(0),
            chunk_size,
        })
    }

    /// Allocate memory from arena
    ///
    /// Returns None if object is too large for chunk size
    /// No allocations - uses pre-allocated chunks
    #[inline]
    pub fn alloc<T>(&self) -> Option<&mut T> {
        let size = size_of::<T>();
        let align = align_of::<T>();

        if size > self.chunk_size {
            return None; // Too large for arena
        }

        // Try current chunk
        let current_idx = self.current.load(Ordering::Acquire);
        if current_idx < self.chunks.len() {
            let chunk = &self.chunks[current_idx];

            // Align and allocate atomically
            loop {
                let current_offset = chunk.used.load(Ordering::Acquire);
                let aligned_offset = (current_offset + align - 1) & !(align - 1);
                let end_offset = aligned_offset + size;

                if end_offset > chunk.size {
                    break; // Need new chunk
                }

                // Try to claim the aligned space
                if chunk
                    .used
                    .compare_exchange_weak(
                        current_offset,
                        end_offset,
                        Ordering::Release,
                        Ordering::Acquire,
                    )
                    .is_ok()
                {
                    let ptr = unsafe { chunk.data.as_ptr().add(aligned_offset).cast::<T>() };
                    return Some(unsafe { &mut *ptr });
                }
            }
        }

        None // Would need new chunk (requires mutex for growth)
    }

    /// Reset arena for reuse
    ///
    /// Doesn't deallocate - just resets offsets
    /// Very fast O(n) where n = number of chunks
    pub fn reset(&mut self) {
        for chunk in &self.chunks {
            chunk.used.store(0, Ordering::Release);
        }
        self.current.store(0, Ordering::Release);
    }

    /// Get memory statistics
    pub fn stats(&self) -> ArenaStats {
        let mut total_size = 0;
        let mut total_used = 0;

        for chunk in &self.chunks {
            total_size += chunk.size;
            total_used += chunk.used.load(Ordering::Relaxed);
        }

        ArenaStats {
            chunks: self.chunks.len(),
            total_size,
            total_used,
            chunk_size: self.chunk_size,
        }
    }
}

impl Drop for ArenaChunk {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::from_size_align_unchecked(self.size, CACHE_LINE_SIZE);
            dealloc(self.data.as_ptr(), layout);
        }
    }
}

/// Arena statistics
#[derive(Debug, Clone, Copy)]
pub struct ArenaStats {
    pub chunks: usize,
    pub total_size: usize,
    pub total_used: usize,
    pub chunk_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_basic() {
        let arena = Arena::new(1024).unwrap();

        // Allocate some objects
        let obj1: &mut i32 = arena.alloc().unwrap();
        *obj1 = 42;

        let obj2: &mut i64 = arena.alloc().unwrap();
        *obj2 = 100;

        assert_eq!(*obj1, 42);
        assert_eq!(*obj2, 100);

        let stats = arena.stats();
        assert_eq!(stats.chunks, 1);
        assert!(stats.total_used > 0);
    }

    #[test]
    fn test_arena_alignment() {
        let arena = Arena::new(1024).unwrap();

        // Allocate with different alignments
        // First allocation - single byte to potentially misalign
        let byte: &mut u8 = arena.alloc().unwrap();
        *byte = 42; // Use the allocation
        let dword: &mut u64 = arena.alloc().unwrap();

        // Check alignment
        // SAFETY: Pointer to usize for alignment check
        let addr = dword as *mut u64 as usize;
        assert_eq!(addr % align_of::<u64>(), 0);
    }

    #[test]
    fn test_arena_reset() {
        let mut arena = Arena::new(1024).unwrap();

        // Allocate and use memory
        for i in 0..10 {
            let obj: &mut i32 = arena.alloc().unwrap();
            *obj = i;
        }

        let stats_before = arena.stats();
        assert!(stats_before.total_used > 0);

        // Reset arena
        arena.reset();

        let stats_after = arena.stats();
        assert_eq!(stats_after.total_used, 0);
        assert_eq!(stats_after.chunks, stats_before.chunks); // No deallocation
    }
}
