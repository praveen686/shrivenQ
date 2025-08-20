//! Custom assertions for testing

use std::fmt::Debug;
use anyhow::Result;

/// Assert that two floating point values are approximately equal
pub fn assert_approx_eq(left: f64, right: f64, tolerance: f64) {
    let diff = (left - right).abs();
    assert!(
        diff <= tolerance,
        "Values not approximately equal: {} != {} (diff: {}, tolerance: {})",
        left,
        right,
        diff,
        tolerance
    );
}

/// Assert that a value is within a range
pub fn assert_in_range<T: PartialOrd + Debug>(value: T, min: T, max: T) {
    assert!(
        value >= min && value <= max,
        "Value {:?} not in range [{:?}, {:?}]",
        value,
        min,
        max
    );
}

/// Assert that a collection contains an element
pub fn assert_contains<T: PartialEq + Debug>(collection: &[T], element: &T) {
    assert!(
        collection.contains(element),
        "Collection does not contain element: {:?}",
        element
    );
}

/// Assert that a collection is sorted
pub fn assert_sorted<T: PartialOrd + Debug>(collection: &[T]) {
    for window in collection.windows(2) {
        assert!(
            window[0] <= window[1],
            "Collection not sorted at elements: {:?} > {:?}",
            window[0],
            window[1]
        );
    }
}

/// Assert that an async operation completes within a duration
#[macro_export]
macro_rules! assert_completes_within {
    ($duration:expr, $future:expr) => {
        tokio::time::timeout($duration, $future)
            .await
            .expect("Operation did not complete within timeout")
    };
}

/// Assert that an error contains a specific message
pub fn assert_error_contains<E: std::fmt::Display>(error: &E, expected: &str) {
    let error_str = error.to_string();
    assert!(
        error_str.contains(expected),
        "Error message '{}' does not contain '{}'",
        error_str,
        expected
    );
}

/// Assert that a Result is Ok and matches a pattern
#[macro_export]
macro_rules! assert_ok_matches {
    ($result:expr, $pattern:pat) => {
        match $result {
            Ok($pattern) => (),
            Ok(other) => panic!("Ok value does not match pattern: {:?}", other),
            Err(e) => panic!("Expected Ok, got Err: {:?}", e),
        }
    };
}

/// Assert that a Result is Err and matches a pattern
#[macro_export]
macro_rules! assert_err_matches {
    ($result:expr, $pattern:pat) => {
        match $result {
            Err($pattern) => (),
            Err(other) => panic!("Err value does not match pattern: {:?}", other),
            Ok(v) => panic!("Expected Err, got Ok: {:?}", v),
        }
    };
}

/// Assert that two collections have the same elements (order independent)
/// 
/// This function checks that two collections contain the same elements with
/// the same frequencies, regardless of order. It properly handles duplicates
/// by counting occurrences of each element.
pub fn assert_same_elements<T: PartialEq + Debug + Clone>(left: &[T], right: &[T]) {
    // First check lengths for fast failure
    assert_eq!(
        left.len(),
        right.len(),
        "Collections have different lengths: left has {} elements, right has {} elements",
        left.len(),
        right.len()
    );
    
    // Count occurrences of each element in both collections
    for element in left {
        let left_count = left.iter().filter(|&e| e == element).count();
        let right_count = right.iter().filter(|&e| e == element).count();
        
        assert_eq!(
            left_count,
            right_count,
            "Element {:?} appears {} times in left collection but {} times in right collection",
            element,
            left_count,
            right_count
        );
    }
}

/// Assert that a future panics
#[macro_export]
macro_rules! assert_panics {
    ($future:expr) => {
        let result = std::panic::AssertUnwindSafe($future)
            .catch_unwind()
            .await;
        assert!(result.is_err(), "Expected panic but operation succeeded");
    };
}

/// Performance assertion for testing execution time limits
/// 
/// This struct measures the time taken by operations and automatically
/// asserts that they complete within specified time limits when dropped.
/// Useful for ensuring performance requirements in tests.
#[derive(Debug)]
pub struct PerformanceAssertion {
    name: String,
    start: std::time::Instant,
    max_duration: std::time::Duration,
}

impl PerformanceAssertion {
    /// Creates a new performance assertion that starts timing immediately
    /// 
    /// The assertion will automatically check performance when dropped.
    /// 
    /// # Arguments
    /// * `name` - A descriptive name for the operation being timed
    /// * `max_duration` - The maximum allowed duration for the operation
    /// 
    /// # Returns
    /// * A new PerformanceAssertion that starts timing immediately
    pub fn new(name: impl Into<String>, max_duration: std::time::Duration) -> Self {
        Self {
            name: name.into(),
            start: std::time::Instant::now(),
            max_duration,
        }
    }
}

impl Drop for PerformanceAssertion {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        assert!(
            elapsed <= self.max_duration,
            "Performance assertion '{}' failed: {:?} > {:?}",
            self.name,
            elapsed,
            self.max_duration
        );
    }
}

/// Assert that a value changes after an operation
pub async fn assert_changes<T, F, Fut>(
    getter: impl Fn() -> T,
    operation: F,
) -> (T, T)
where
    T: PartialEq + Debug + Clone,
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let before = getter();
    operation().await.expect("Operation failed");
    let after = getter();
    
    assert_ne!(
        before, after,
        "Value did not change after operation"
    );
    
    (before, after)
}

/// Assert that a value does not change after an operation
pub async fn assert_unchanged<T, F, Fut>(
    getter: impl Fn() -> T,
    operation: F,
) -> T
where
    T: PartialEq + Debug + Clone,
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let before = getter();
    operation().await.expect("Operation failed");
    let after = getter();
    
    assert_eq!(
        before, after,
        "Value changed when it should not have"
    );
    
    after
}

/// Assert that an operation is idempotent
pub async fn assert_idempotent<T, F>(
    operation: F,
    times: usize,
) -> Vec<T>
where
    T: PartialEq + Debug,
    F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>>>>,
{
    assert!(times >= 2, "Need at least 2 executions to test idempotency");
    
    let mut results = Vec::new();
    
    for _ in 0..times {
        let result = operation().await.expect("Operation failed");
        results.push(result);
    }
    
    // All results should be the same
    for i in 1..results.len() {
        assert_eq!(
            results[0], results[i],
            "Operation not idempotent: result {} differs from first result",
            i
        );
    }
    
    results
}

/// Extension trait providing additional assertion methods for Option types
/// 
/// This trait adds convenient assertion methods to Option types for testing,
/// making it easier to assert on Some/None variants with clear error messages.
pub trait AssertionExt<T> {
    /// Asserts that the Option is Some and returns the contained value
    /// 
    /// # Panics
    /// Panics if the Option is None
    /// 
    /// # Returns
    /// The contained value if Some
    fn assert_some(self) -> T;
    
    /// Asserts that the Option is None
    /// 
    /// # Panics
    /// Panics if the Option is Some
    fn assert_none(self);
}

impl<T: Debug> AssertionExt<T> for Option<T> {
    /// Implementation of assert_some for Option types
    fn assert_some(self) -> T {
        self.expect("Expected Some, got None")
    }
    
    /// Implementation of assert_none for Option types
    fn assert_none(self) {
        assert!(self.is_none(), "Expected None, got Some({:?})", self);
    }
}

/// Extension trait providing assertion methods for async streams
/// 
/// This trait adds convenient assertion methods for testing async streams,
/// making it easier to verify stream behavior and state in tests.
#[allow(async_fn_in_trait)]
pub trait StreamAssertionExt {
    /// Asserts that the stream produces a next item and returns it
    /// 
    /// # Type Parameters
    /// * `T` - The type of items produced by the stream
    /// 
    /// # Returns
    /// The next item from the stream
    /// 
    /// # Panics
    /// Panics if the stream has ended unexpectedly
    async fn assert_next<T>(&mut self) -> T
    where
        Self: futures::Stream<Item = T> + Unpin,
        T: Debug;
    
    /// Asserts that the stream is closed (produces no more items)
    /// 
    /// # Panics
    /// Panics if the stream produces an item when it should be closed
    async fn assert_closed(&mut self)
    where
        Self: futures::Stream + Unpin,
        Self::Item: Debug;
}

#[allow(async_fn_in_trait)]
impl<S> StreamAssertionExt for S
where
    S: futures::Stream + Unpin,
{
    /// Implementation of assert_next for stream types
    async fn assert_next<T>(&mut self) -> T
    where
        Self: futures::Stream<Item = T>,
        T: Debug,
    {
        use futures::StreamExt;
        self.next()
            .await
            .expect("Stream ended unexpectedly")
    }
    
    /// Implementation of assert_closed for stream types
    async fn assert_closed(&mut self)
    where
        <Self as futures::Stream>::Item: Debug,
    {
        use futures::StreamExt;
        assert!(
            self.next().await.is_none(),
            "Expected stream to be closed"
        );
    }
}