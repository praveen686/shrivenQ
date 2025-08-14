//! Circuit breaker implementation

#![allow(clippy::cast_possible_truncation)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub struct CircuitBreaker {
    is_open: AtomicBool,
    failure_count: AtomicU64,
    last_failure_time: AtomicU64,
    threshold: u64,
    timeout_ms: u64,
}

impl CircuitBreaker {
    pub fn new(threshold: u64, timeout_ms: u64) -> Self {
        Self {
            is_open: AtomicBool::new(false),
            failure_count: AtomicU64::new(0),
            last_failure_time: AtomicU64::new(0),
            threshold,
            timeout_ms,
        }
    }

    pub fn is_open(&self) -> bool {
        if self.is_open.load(Ordering::Relaxed) {
            // Check if timeout has elapsed
            // SAFETY: timestamp_millis() returns i64, max(0) ensures non-negative, safe to cast to u64
            let now = chrono::Utc::now().timestamp_millis().max(0) as u64;
            let last_failure = self.last_failure_time.load(Ordering::Relaxed);

            if now > last_failure + self.timeout_ms {
                // Timeout elapsed, reset circuit breaker
                self.is_open.store(false, Ordering::Relaxed);
                self.failure_count.store(0, Ordering::Relaxed);
                false
            } else {
                true
            }
        } else {
            false
        }
    }

    pub fn record_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        self.is_open.store(false, Ordering::Relaxed);
    }

    pub fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count >= self.threshold {
            self.is_open.store(true, Ordering::Relaxed);
            self.last_failure_time.store(
                // SAFETY: timestamp_millis() returns i64, max(0) ensures non-negative, safe to cast to u64
                chrono::Utc::now().timestamp_millis().max(0) as u64,
                Ordering::Relaxed,
            );
        }
    }
}
