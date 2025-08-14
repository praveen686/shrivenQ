//! Lock-free ring buffer for inter-thread communication
//!
//! COMPLIANCE:
//! - Zero allocations
//! - Single producer, single consumer (SPSC)
//! - Cache-line aligned to prevent false sharing
//! - Wait-free operations

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Lock-free ring buffer for SPSC communication
///
/// Performance characteristics:
/// - Push: O(1) wait-free
/// - Pop: O(1) wait-free
/// - No allocations after creation
/// - Cache-line aligned to prevent false sharing
#[repr(C, align(64))]
pub struct RingBuffer<T, const N: usize> {
    /// Buffer storage
    buffer: [UnsafeCell<MaybeUninit<T>>; N],
    /// Producer position
    head: AtomicUsize,
    /// Consumer position
    tail: AtomicUsize,
    /// Cached head for producer (avoid false sharing)
    cached_head: UnsafeCell<usize>,
    /// Cached tail for consumer (avoid false sharing)
    cached_tail: UnsafeCell<usize>,
    /// Padding to fill cache line
    _padding: [u8; 48],
}

unsafe impl<T: Send, const N: usize> Send for RingBuffer<T, N> {}
unsafe impl<T: Send, const N: usize> Sync for RingBuffer<T, N> {}

impl<T, const N: usize> RingBuffer<T, N> {
    /// Create new ring buffer
    ///
    /// No allocations - all memory is stack allocated
    pub const fn new() -> Self {
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

    /// Push to ring buffer (single producer)
    ///
    /// Returns false if buffer is full
    /// Wait-free O(1) operation
    #[inline(always)]
    pub fn push(&self, value: T) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let next_tail = (tail + 1) % N;

        // Check if full using cached read
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
            slot.write(value);
        }

        // Update tail
        self.tail.store(next_tail, Ordering::Release);
        true
    }

    /// Pop from ring buffer (single consumer)
    ///
    /// Returns None if buffer is empty
    /// Wait-free O(1) operation
    #[inline(always)]
    pub fn pop(&self) -> Option<T> {
        let head = self.head.load(Ordering::Relaxed);

        // Check if empty using cached read
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

    /// Check if buffer is empty
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Relaxed) == self.tail.load(Ordering::Acquire)
    }

    /// Check if buffer is full
    #[inline(always)]
    pub fn is_full(&self) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let next_tail = (tail + 1) % N;
        next_tail == self.head.load(Ordering::Acquire)
    }

    /// Get number of items in buffer
    #[inline]
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);

        if tail >= head {
            tail - head
        } else {
            N - head + tail
        }
    }

    /// Get buffer capacity
    #[inline]
    pub const fn capacity(&self) -> usize {
        N - 1 // One slot is always empty to distinguish full from empty
    }
}

impl<T, const N: usize> Default for RingBuffer<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_basic() {
        let buffer = RingBuffer::<i32, 4>::new();

        assert!(buffer.is_empty());
        assert_eq!(buffer.capacity(), 3); // N-1

        // Push some values
        assert!(buffer.push(1));
        assert!(buffer.push(2));
        assert!(buffer.push(3));
        assert!(!buffer.push(4)); // Full

        assert!(buffer.is_full());

        // Pop values
        assert_eq!(buffer.pop(), Some(1));
        assert_eq!(buffer.pop(), Some(2));
        assert_eq!(buffer.pop(), Some(3));
        assert_eq!(buffer.pop(), None); // Empty

        assert!(buffer.is_empty());
    }

    #[test]
    fn test_ring_buffer_wrap() {
        let buffer = RingBuffer::<i32, 4>::new();

        // Fill and empty multiple times to test wrapping
        for round in 0..10 {
            for i in 0..3 {
                assert!(buffer.push(round * 10 + i));
            }

            for i in 0..3 {
                assert_eq!(buffer.pop(), Some(round * 10 + i));
            }
        }
    }

    #[test]
    fn test_ring_buffer_concurrent() {
        use std::sync::Arc;
        use std::thread;

        let buffer = Arc::new(RingBuffer::<i32, 1024>::new());
        let count = 10000usize;

        let buffer_producer = buffer.clone();
        let producer = thread::spawn(move || {
            for i in 0..count {
                // SAFETY: usize to i32 for test values
                while !buffer_producer.push(i as i32) {
                    std::thread::yield_now();
                }
            }
        });

        let buffer_consumer = buffer.clone();
        let consumer = thread::spawn(move || {
            let mut received = Vec::with_capacity(count);
            while received.len() < count {
                if let Some(val) = buffer_consumer.pop() {
                    received.push(val);
                } else {
                    std::thread::yield_now();
                }
            }
            received
        });

        producer.join().unwrap();
        let received = consumer.join().unwrap();

        // Verify all values received in order
        for (i, val) in received.iter().enumerate() {
            // SAFETY: usize to i32 for test comparison
            assert_eq!(*val, i as i32);
        }
    }
}
