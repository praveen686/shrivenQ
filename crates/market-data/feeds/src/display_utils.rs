/// Display utilities for human-readable formatting
///
/// These functions convert internal integer types to human-readable formats.
/// They are ONLY for display/logging - never use for business logic.

/// Format bytes as GiB with 2 decimal places
#[allow(clippy::cast_precision_loss)] // Display only
pub fn fmt_bytes_gib(bytes: u64) -> f64 {
    (bytes as f64) / 1024_f64.powi(3)
}

/// Format bytes as MiB with 2 decimal places
#[allow(clippy::cast_precision_loss)] // Display only
pub fn fmt_bytes_mib(bytes: u64) -> f64 {
    (bytes as f64) / 1024_f64.powi(2)
}

/// Format KB as MB with 1 decimal place
#[allow(clippy::cast_precision_loss)] // Display only
pub fn fmt_kb_to_mb(kb: u64) -> f64 {
    (kb as f64) / 1024.0
}

/// Format nanoseconds as seconds
pub fn fmt_nanos_to_secs(nanos: u64) -> f64 {
    std::time::Duration::from_nanos(nanos).as_secs_f64()
}

/// Calculate events per second
#[allow(clippy::cast_precision_loss)] // Display only - f64 handles up to 2^53
pub fn calc_events_per_sec(events: u64, duration_secs: f64) -> f64 {
    if duration_secs > 0.0 {
        (events as f64) / duration_secs
    } else {
        0.0
    }
}

/// Calculate percentage for display
#[allow(clippy::cast_precision_loss)] // Display only
pub fn calc_percentage(part: u64, total: u64) -> f64 {
    if total > 0 {
        (part as f64 * 100.0) / (total as f64)
    } else {
        0.0
    }
}

// ============================================================================
// TEST UTILITIES MODULE
// These functions are ONLY for test code. They handle type conversions that
// would be unsafe in production but are acceptable for test data generation.
// ============================================================================

#[cfg(any(test, feature = "test-utils"))]
pub(crate) mod test_utils {
    //! Test-only utilities for safe type conversions in test data generation.
    //!
    //! # Safety Guidelines
    //! - These functions are ONLY for test code
    //! - Never use in production code paths
    //! - Prefer `From`/`TryFrom` where possible
    //! - Document all wraparound/truncation behavior

    // Allow these lints only within this module
    #![allow(clippy::cast_precision_loss)] // Acceptable for test assertions
    #![allow(clippy::cast_possible_truncation)] // Documented behavior
    #![allow(dead_code)] // These are utility functions that may not all be used

    /// Convert test index to u32.
    ///
    /// # Panics
    /// Panics if index > u32::MAX (which shouldn't happen in tests)
    pub fn index_to_u32(index: usize) -> u32 {
        u32::try_from(index).unwrap_or(u32::MAX)
    }

    /// Convert test index to u64 (lossless on all platforms).
    pub fn index_to_u64(index: usize) -> u64 {
        index as u64 // SAFETY: usize <= u64 on all platforms
    }

    /// Convert test index to f64 for generating test values.
    ///
    /// # Precision
    /// Exact for indices up to 2^53 (~9e15), which covers all practical test ranges.
    /// For larger values, use integer comparisons instead.
    pub fn index_to_f64(index: usize) -> f64 {
        debug_assert!(index < (1_usize << 53), "index too large for exact f64");
        index as f64
    }

    /// Convert test index to u8 for level generation with wraparound.
    ///
    /// # Wraparound Behavior
    /// Deliberately wraps at 256: `index % 256`
    /// This is useful for generating level indices (0-255) from any index.
    ///
    /// # Example
    /// ```ignore
    /// assert_eq!(index_to_u8(0), 0);
    /// assert_eq!(index_to_u8(255), 255);
    /// assert_eq!(index_to_u8(256), 0);  // Wraps
    /// assert_eq!(index_to_u8(257), 1);  // Wraps
    /// ```
    pub fn index_to_u8_wrapped(index: usize) -> u8 {
        (index % 256) as u8 // SAFETY: modulo 256 fits in u8
    }

    /// Convert i64 to f64 for tolerance-based test assertions.
    ///
    /// # Precision Warning
    /// Only exact for |value| < 2^53. For larger values:
    /// - Use integer comparisons, or
    /// - Explicitly document acceptable precision loss
    ///
    /// # Example
    /// ```ignore
    /// // Good: Small values with epsilon comparison
    /// let expected = i64_to_f64_for_assert(1000);
    /// assert!((actual - expected).abs() < 0.001);
    ///
    /// // Bad: Large values lose precision
    /// let bad = i64_to_f64_for_assert(1_i64 << 54);  // Precision loss!
    /// ```
    pub fn i64_to_f64_for_assert(value: i64) -> f64 {
        const MAX_EXACT: i64 = 1_i64 << 53;
        debug_assert!(
            value.abs() < MAX_EXACT,
            "i64 value {} exceeds exact f64 range",
            value
        );
        value as f64
    }

    /// Format a pointer address for logging/debugging.
    ///
    /// # Safety
    /// - ONLY for displaying addresses in logs
    /// - NEVER perform arithmetic on the result
    /// - Use `ptr::offset_from` for pointer distance calculations
    ///
    /// # Example
    /// ```ignore
    /// let ptr = &value as *const T;
    /// info!("Allocated at: 0x{:x}", addr_for_log(ptr));
    /// ```
    pub fn addr_for_log<T>(ptr: *const T) -> usize {
        ptr as usize
    }

    /// Calculate pointer alignment for testing.
    ///
    /// # Example
    /// ```ignore
    /// let ptr = &value as *const u64;
    /// assert_eq!(alignment_of(ptr) % 8, 0, "u64 should be 8-byte aligned");
    /// ```
    pub fn alignment_of<T>(ptr: *const T) -> usize {
        (ptr as usize) % std::mem::align_of::<T>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gib_formatting() {
        // 1.75 GiB should print as 1.75
        let bytes = (1.75 * 1024.0 * 1024.0 * 1024.0) as u64; // SAFETY: known memory size
        let gib = fmt_bytes_gib(bytes);
        assert!((gib - 1.75).abs() < 0.01);
    }

    #[test]
    fn test_mib_to_gib() {
        // 1536 MiB should be 1.50 GiB
        let bytes = 1536 * 1024 * 1024;
        let gib = fmt_bytes_gib(bytes);
        assert!((gib - 1.50).abs() < 0.01);
    }

    #[test]
    fn test_large_event_count() {
        // 10 billion events in 2 seconds = 5e9 events/s
        let events = 10_000_000_000;
        let duration = 2.0;
        let rate = calc_events_per_sec(events, duration);
        assert!((rate - 5e9).abs() < 1.0);
    }

    #[cfg(feature = "test-utils")]
    mod test_utils_tests {
        use super::super::test_utils::*;

        #[test]
        fn test_index_wraparound() {
            // Test u8 wraparound behavior
            assert_eq!(index_to_u8_wrapped(0), 0);
            assert_eq!(index_to_u8_wrapped(255), 255);
            assert_eq!(index_to_u8_wrapped(256), 0); // Wraps
            assert_eq!(index_to_u8_wrapped(257), 1); // Wraps
            assert_eq!(index_to_u8_wrapped(512), 0); // Double wrap
        }

        #[test]
        fn test_index_conversions() {
            // Test basic conversions
            assert_eq!(index_to_u32(100), 100);
            assert_eq!(index_to_u64(100), 100);
            assert_eq!(index_to_f64(100), 100.0);
        }

        #[test]
        fn test_i64_precision_boundary() {
            // Test at precision boundary
            let max_exact = (1_i64 << 53) - 1;
            let f = i64_to_f64_for_assert(max_exact);
            assert_eq!(f, max_exact as f64);
        }

        #[test]
        #[should_panic(expected = "test index should fit in u32")]
        fn test_u32_overflow() {
            // This should panic on 64-bit systems
            #[cfg(target_pointer_width = "64")]
            {
                let large = (u32::MAX as usize) + 1;
                index_to_u32(large);
            }
        }
    }
}
