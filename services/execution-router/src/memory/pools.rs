//! Lock-free object pools for zero-allocation in hot paths
//!
//! COMPLIANCE:
//! - No allocations after initialization
//! - Lock-free MPSC operations
//! - ABA prevention with tagged pointers
//! - RAII automatic return to pool

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};

// Tagged pointer implementation for ABA prevention
// Pack a 32-bit generation counter and 32-bit index into a usize
#[cfg(target_pointer_width = "64")]
const TAG_BITS: usize = 32;
#[cfg(target_pointer_width = "64")]
const INDEX_MASK: usize = 0xFFFFFFFF;
#[cfg(target_pointer_width = "64")]
// SAFETY: u32::MAX to usize - widening on 64-bit
const MAX_POOL_SIZE: usize = u32::MAX as usize;

// Helper functions for tagged pointer manipulation
#[inline(always)]
#[cfg(target_pointer_width = "64")]
fn pack_tagged(generation: u32, index: u32) -> usize {
    // SAFETY: u32 to usize for pointer tagging on 64-bit
    ((generation as usize) << TAG_BITS) | (index as usize)
}

#[inline(always)]
#[cfg(target_pointer_width = "64")]
fn unpack_generation(tagged: usize) -> u32 {
    // SAFETY: usize to u32 - upper 32 bits extracted
    (tagged >> TAG_BITS) as u32
}

#[inline(always)]
#[cfg(target_pointer_width = "64")]
fn unpack_index(tagged: usize) -> u32 {
    // SAFETY: usize to u32 - lower 32 bits masked
    (tagged & INDEX_MASK) as u32
}

// 32-bit fallback - no tagging
#[cfg(not(target_pointer_width = "64"))]
const MAX_POOL_SIZE: usize = usize::MAX;

#[cfg(not(target_pointer_width = "64"))]
#[inline(always)]
fn pack_tagged(_generation: u32, index: u32) -> usize {
    // SAFETY: u32 to usize - simple widening
    index as usize
}

#[cfg(not(target_pointer_width = "64"))]
#[inline(always)]
fn unpack_generation(_tagged: usize) -> u32 {
    0 // No ABA prevention on 32-bit
}

#[cfg(not(target_pointer_width = "64"))]
#[inline(always)]
fn unpack_index(tagged: usize) -> u32 {
    tagged as u32
}

/// Lock-free object pool with ABA prevention
///
/// Performance characteristics:
/// - Acquire: O(1) with potential retry under contention
/// - Release: O(1) with potential retry under contention
/// - No allocations after initialization
pub struct ObjectPool<T> {
    /// Pre-allocated storage
    storage: Box<[UnsafeCell<MaybeUninit<T>>]>,
    /// Free list head (tagged pointer)
    free_list: AtomicUsize,
    /// Free list nodes
    nodes: Box<[FreeNode]>,
    /// Counter for monitoring
    allocated: AtomicUsize,
}

/// Free list node with atomic next pointer
struct FreeNode {
    next: AtomicUsize,
    index: usize,
}

/// RAII wrapper for pool-allocated objects
///
/// Automatically returns object to pool when dropped
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

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.obj
    }
}

impl<'a, T> std::ops::DerefMut for PoolRef<'a, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.obj
    }
}

unsafe impl<T: Send> Send for ObjectPool<T> {}
unsafe impl<T: Send> Sync for ObjectPool<T> {}

impl<T> ObjectPool<T> {
    /// Internal method to return object to pool
    unsafe fn release_internal(&self, _obj: &mut T, known_index: usize) {
        debug_assert!(known_index < self.nodes.len(), "Invalid index for release");

        let node = &self.nodes[known_index];
        debug_assert_eq!(node.index, known_index, "Index mismatch in node");

        loop {
            let head_tagged = self.free_list.load(Ordering::Acquire);

            // Store current head as our next
            let head_index = unpack_index(head_tagged);
            node.next.store(head_index as usize, Ordering::Release);

            // Create new tagged value with incremented generation
            let new_generation = unpack_generation(head_tagged).wrapping_add(1);
            let new_tagged = pack_tagged(new_generation, known_index as u32);

            // Try to update head to point to us
            if self
                .free_list
                .compare_exchange_weak(
                    head_tagged,
                    new_tagged,
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

    /// Get number of currently allocated objects
    #[inline]
    pub fn allocated(&self) -> usize {
        self.allocated.load(Ordering::Relaxed)
    }

    /// Get total capacity
    #[inline]
    pub fn capacity(&self) -> usize {
        self.storage.len()
    }

    /// Check if pool is exhausted
    #[inline]
    pub fn is_exhausted(&self) -> bool {
        self.allocated() >= self.capacity()
    }
}

impl<T: Default> ObjectPool<T> {
    /// Create new pool with specified capacity
    ///
    /// Pre-allocates all objects using T::default()
    /// No allocations after this point
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "Pool capacity must be > 0");
        assert!(
            capacity <= MAX_POOL_SIZE,
            "Pool capacity {} exceeds maximum {}",
            capacity,
            MAX_POOL_SIZE
        );

        // Pre-allocate storage
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

        let head = if capacity > 0 { 0 } else { usize::MAX };

        Self {
            storage,
            free_list: AtomicUsize::new(head),
            nodes,
            allocated: AtomicUsize::new(0),
        }
    }

    /// Acquire object from pool
    ///
    /// Returns None if pool is exhausted
    /// No allocations - returns pre-allocated object
    #[inline]
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

            let node = &self.nodes[head_index];
            let next_tagged = node.next.load(Ordering::Acquire);

            // Create new tagged value with incremented generation
            let new_generation = unpack_generation(head_tagged).wrapping_add(1);
            let next_index = unpack_index(next_tagged as usize);
            let new_tagged = pack_tagged(new_generation, next_index);

            // Try to swap head with CAS
            if self
                .free_list
                .compare_exchange_weak(
                    head_tagged,
                    new_tagged,
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
                    index: node.index,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default, Debug, PartialEq)]
    struct TestObject {
        value: i32,
    }

    #[test]
    fn test_pool_basic() {
        let pool = ObjectPool::<TestObject>::new(10);
        assert_eq!(pool.capacity(), 10);
        assert_eq!(pool.allocated(), 0);

        // Acquire object
        {
            let mut obj = pool.acquire().unwrap();
            obj.value = 42;
            assert_eq!(pool.allocated(), 1);
        }

        // Object returned after drop
        assert_eq!(pool.allocated(), 0);
    }

    #[test]
    fn test_pool_exhaustion() {
        let pool = ObjectPool::<TestObject>::new(2);

        let obj1 = pool.acquire().unwrap();
        let obj2 = pool.acquire().unwrap();

        // Pool exhausted
        assert!(pool.acquire().is_none());
        assert!(pool.is_exhausted());

        drop(obj1);
        assert!(!pool.is_exhausted());

        // Can acquire again - keep obj3 alive to test pool exhaustion
        let obj3 = pool.acquire().unwrap();
        assert_eq!(pool.allocated(), 1); // Verify obj3 is allocated

        drop(obj2);
    }

    #[test]
    fn test_pool_concurrent() {
        use std::sync::Arc;
        use std::thread;

        let pool = Arc::new(ObjectPool::<TestObject>::new(100));
        let mut handles = vec![];

        for i in 0..10 {
            let pool = pool.clone();
            let handle = thread::spawn(move || {
                for j in 0..10 {
                    if let Some(mut obj) = pool.acquire() {
                        obj.value = i * 10 + j;
                        // Simulate work
                        std::thread::yield_now();
                    }
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(pool.allocated(), 0);
    }
}
