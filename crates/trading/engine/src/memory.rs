//! Memory pool for zero-allocation in hot paths

use std::alloc::{Layout, alloc, dealloc};
use std::cell::UnsafeCell;
use std::mem::{MaybeUninit, align_of, size_of};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

/// Object pool for zero-allocation - lock-free MPSC
pub struct ObjectPool<T> {
    storage: Box<[UnsafeCell<MaybeUninit<T>>]>,
    free_list: AtomicPtr<FreeNode>,
    nodes: Box<[FreeNode]>,
    allocated: AtomicUsize,
}

struct FreeNode {
    next: AtomicPtr<FreeNode>,
    index: usize,
}

unsafe impl<T: Send> Send for ObjectPool<T> {}
unsafe impl<T: Send> Sync for ObjectPool<T> {}

impl<T: Default> ObjectPool<T> {
    /// Create pool with pre-allocated objects
    pub fn new(capacity: usize) -> Self {
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
                next: AtomicPtr::new(std::ptr::null_mut()),
                index: i,
            });
        }
        let mut nodes = nodes.into_boxed_slice();

        // Link free list
        for i in 0..capacity - 1 {
            let next_ptr = &nodes[i + 1] as *const FreeNode as *mut FreeNode;
            nodes[i].next.store(next_ptr, Ordering::Relaxed);
        }

        let head = if capacity > 0 {
            &mut nodes[0] as *mut _
        } else {
            std::ptr::null_mut()
        };

        Self {
            storage,
            free_list: AtomicPtr::new(head),
            nodes,
            allocated: AtomicUsize::new(0),
        }
    }

    /// Acquire object from pool - lock-free
    #[inline(always)]
    pub fn acquire(&self) -> Option<&mut T> {
        loop {
            let head = self.free_list.load(Ordering::Acquire);
            if head.is_null() {
                return None; // Pool exhausted
            }

            let next = unsafe { (*head).next.load(Ordering::Acquire) };

            // Try to swap head
            if self
                .free_list
                .compare_exchange_weak(head, next, Ordering::Release, Ordering::Acquire)
                .is_ok()
            {
                let node = unsafe { &*head };
                let obj = unsafe { &mut *self.storage[node.index].get() };

                self.allocated.fetch_add(1, Ordering::Relaxed);
                return Some(unsafe { obj.assume_init_mut() });
            }
        }
    }

    /// Release object back to pool - lock-free
    #[inline(always)]
    pub fn release(&self, obj: &mut T) {
        // Find index from pointer
        let obj_ptr = obj as *mut T as usize;
        let base_ptr = self.storage.as_ptr() as usize;
        let offset = obj_ptr - base_ptr;
        let index = offset / size_of::<UnsafeCell<MaybeUninit<T>>>();

        if index >= self.nodes.len() {
            return; // Invalid pointer
        }

        let node = &self.nodes[index] as *const _ as *mut FreeNode;

        loop {
            let head = self.free_list.load(Ordering::Acquire);
            unsafe { (*node).next.store(head, Ordering::Release) };

            if self
                .free_list
                .compare_exchange_weak(head, node, Ordering::Release, Ordering::Acquire)
                .is_ok()
            {
                self.allocated.fetch_sub(1, Ordering::Relaxed);
                break;
            }
        }
    }

    /// Get number of allocated objects
    pub fn allocated(&self) -> usize {
        self.allocated.load(Ordering::Relaxed)
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
    pub fn new(chunk_size: usize) -> Self {
        let mut chunks = Vec::new();
        // Pre-allocate first chunk
        if chunk_size > 0 {
            let layout = Layout::from_size_align(chunk_size, 64).unwrap();
            let data = unsafe {
                let ptr = alloc(layout);
                if ptr.is_null() {
                    panic!("Failed to allocate arena chunk");
                }
                NonNull::new_unchecked(ptr)
            };
            chunks.push(ArenaChunk {
                data,
                size: chunk_size,
                used: AtomicUsize::new(0),
            });
        }

        Self {
            chunks,
            current: AtomicUsize::new(0),
            chunk_size,
        }
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
