//! Metrics collection for the event bus

use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Bus metrics collector
pub struct BusMetrics {
    /// Publish attempts by topic
    publish_attempts: RwLock<FxHashMap<String, AtomicU64>>,
    /// Publish successes by topic
    publish_successes: RwLock<FxHashMap<String, AtomicU64>>,
    /// Handle attempts by topic
    handle_attempts: RwLock<FxHashMap<String, AtomicU64>>,
    /// Handle successes by topic
    handle_successes: RwLock<FxHashMap<String, AtomicU64>>,
    /// Handle failures by topic
    handle_failures: RwLock<FxHashMap<String, AtomicU64>>,
    /// Messages sent to dead letter queue by topic
    dead_letters: RwLock<FxHashMap<String, AtomicU64>>,
    /// Expired messages by topic
    expired_messages: RwLock<FxHashMap<String, AtomicU64>>,
    /// Messages with no subscribers by topic
    no_subscribers: RwLock<FxHashMap<String, AtomicU64>>,
    /// Handle duration tracking by topic
    handle_durations: RwLock<FxHashMap<String, DurationTracker>>,
    /// Start time for uptime calculation
    start_time: Instant,
}

impl BusMetrics {
    /// Create new metrics collector
    #[must_use] pub fn new() -> Self {
        Self {
            publish_attempts: RwLock::new(FxHashMap::default()),
            publish_successes: RwLock::new(FxHashMap::default()),
            handle_attempts: RwLock::new(FxHashMap::default()),
            handle_successes: RwLock::new(FxHashMap::default()),
            handle_failures: RwLock::new(FxHashMap::default()),
            dead_letters: RwLock::new(FxHashMap::default()),
            expired_messages: RwLock::new(FxHashMap::default()),
            no_subscribers: RwLock::new(FxHashMap::default()),
            handle_durations: RwLock::new(FxHashMap::default()),
            start_time: Instant::now(),
        }
    }

    /// Record a publish attempt
    pub fn record_publish_attempt(&self, topic: &str) {
        let attempts = self.publish_attempts.read();
        if let Some(counter) = attempts.get(topic) {
            counter.fetch_add(1, Ordering::Relaxed);
        } else {
            drop(attempts);
            let mut attempts = self.publish_attempts.write();
            attempts
                .entry(topic.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a successful publish
    pub fn record_publish_success(&self, topic: &str) {
        let successes = self.publish_successes.read();
        if let Some(counter) = successes.get(topic) {
            counter.fetch_add(1, Ordering::Relaxed);
        } else {
            drop(successes);
            let mut successes = self.publish_successes.write();
            successes
                .entry(topic.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a handle attempt
    pub fn record_handle_attempt(&self, topic: &str) {
        let attempts = self.handle_attempts.read();
        if let Some(counter) = attempts.get(topic) {
            counter.fetch_add(1, Ordering::Relaxed);
        } else {
            drop(attempts);
            let mut attempts = self.handle_attempts.write();
            attempts
                .entry(topic.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a successful handle
    pub fn record_handle_success(&self, topic: &str) {
        let successes = self.handle_successes.read();
        if let Some(counter) = successes.get(topic) {
            counter.fetch_add(1, Ordering::Relaxed);
        } else {
            drop(successes);
            let mut successes = self.handle_successes.write();
            successes
                .entry(topic.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a handle failure
    pub fn record_handle_failure(&self, topic: &str) {
        let failures = self.handle_failures.read();
        if let Some(counter) = failures.get(topic) {
            counter.fetch_add(1, Ordering::Relaxed);
        } else {
            drop(failures);
            let mut failures = self.handle_failures.write();
            failures
                .entry(topic.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a dead letter
    pub fn record_dead_letter(&self, topic: &str) {
        let dead_letters = self.dead_letters.read();
        if let Some(counter) = dead_letters.get(topic) {
            counter.fetch_add(1, Ordering::Relaxed);
        } else {
            drop(dead_letters);
            let mut dead_letters = self.dead_letters.write();
            dead_letters
                .entry(topic.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record an expired message
    pub fn record_expired(&self, topic: &str) {
        let expired = self.expired_messages.read();
        if let Some(counter) = expired.get(topic) {
            counter.fetch_add(1, Ordering::Relaxed);
        } else {
            drop(expired);
            let mut expired = self.expired_messages.write();
            expired
                .entry(topic.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a message with no subscribers
    pub fn record_no_subscribers(&self, topic: &str) {
        let no_subs = self.no_subscribers.read();
        if let Some(counter) = no_subs.get(topic) {
            counter.fetch_add(1, Ordering::Relaxed);
        } else {
            drop(no_subs);
            let mut no_subs = self.no_subscribers.write();
            no_subs
                .entry(topic.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record handle duration
    pub fn record_handle_duration(&self, topic: &str, duration: Duration) {
        let mut durations = self.handle_durations.write();
        durations
            .entry(topic.to_string())
            .or_insert_with(DurationTracker::new)
            .record(duration);
    }

    /// Get publish count for topic
    pub fn get_publish_count(&self, topic: &str) -> u64 {
        self.publish_successes
            .read()
            .get(topic)
            .map_or(0, |c| c.load(Ordering::Relaxed))
    }

    /// Get handle count for topic
    pub fn get_handle_count(&self, topic: &str) -> u64 {
        self.handle_successes
            .read()
            .get(topic)
            .map_or(0, |c| c.load(Ordering::Relaxed))
    }

    /// Get failure count for topic
    pub fn get_failure_count(&self, topic: &str) -> u64 {
        self.handle_failures
            .read()
            .get(topic)
            .map_or(0, |c| c.load(Ordering::Relaxed))
    }

    /// Get success rate for topic
    pub fn get_success_rate(&self, topic: &str) -> f64 {
        let successes = self.get_handle_count(topic);
        let failures = self.get_failure_count(topic);
        let total = successes + failures;

        if total > 0 {
            // SAFETY: u64 to f64 for success rate calculation
            successes as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Get average handle duration for topic
    pub fn get_avg_handle_duration(&self, topic: &str) -> Option<Duration> {
        self.handle_durations
            .read()
            .get(topic)
            .map(DurationTracker::average)
    }

    /// Get comprehensive metrics snapshot
    pub fn snapshot(&self) -> EventBusMetrics {
        let mut topic_metrics = FxHashMap::default();

        // Collect all topics
        let mut all_topics = std::collections::HashSet::new();
        all_topics.extend(self.publish_successes.read().keys().cloned());
        all_topics.extend(self.handle_successes.read().keys().cloned());
        all_topics.extend(self.handle_failures.read().keys().cloned());

        for topic in all_topics {
            let publish_count = self.get_publish_count(&topic);
            let handle_count = self.get_handle_count(&topic);
            let failure_count = self.get_failure_count(&topic);
            let success_rate = self.get_success_rate(&topic);
            let avg_duration = self.get_avg_handle_duration(&topic);
            let dead_letter_count = self
                .dead_letters
                .read()
                .get(&topic)
                .map_or(0, |c| c.load(Ordering::Relaxed));
            let expired_count = self
                .expired_messages
                .read()
                .get(&topic)
                .map_or(0, |c| c.load(Ordering::Relaxed));

            topic_metrics.insert(
                topic,
                TopicMetrics {
                    publish_count,
                    handle_count,
                    failure_count,
                    success_rate,
                    // SAFETY: u128 to f64 for duration in milliseconds
                    avg_duration_ms: avg_duration.map_or(0.0, |d| d.as_millis() as f64),
                    dead_letter_count,
                    expired_count,
                },
            );
        }

        EventBusMetrics {
            uptime_seconds: self.start_time.elapsed().as_secs(),
            topic_metrics,
        }
    }
}

impl Default for BusMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Duration tracker for calculating averages
struct DurationTracker {
    total_duration: Duration,
    count: u64,
}

impl DurationTracker {
    const fn new() -> Self {
        Self {
            total_duration: Duration::ZERO,
            count: 0,
        }
    }

    fn record(&mut self, duration: Duration) {
        self.total_duration += duration;
        self.count += 1;
    }

    fn average(&self) -> Duration {
        if self.count > 0 {
            // SAFETY: u64 to u32 - count should be reasonable
            self.total_duration / self.count as u32
        } else {
            Duration::ZERO
        }
    }
}

/// Comprehensive metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBusMetrics {
    /// Bus uptime in seconds
    pub uptime_seconds: u64,
    /// Metrics by topic
    pub topic_metrics: FxHashMap<String, TopicMetrics>,
}

/// Per-topic metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicMetrics {
    /// Number of messages published
    pub publish_count: u64,
    /// Number of messages handled successfully
    pub handle_count: u64,
    /// Number of handle failures
    pub failure_count: u64,
    /// Success rate (0.0 to 1.0)
    pub success_rate: f64,
    /// Average handle duration in milliseconds
    pub avg_duration_ms: f64,
    /// Messages sent to dead letter queue
    pub dead_letter_count: u64,
    /// Expired messages
    pub expired_count: u64,
}

impl EventBusMetrics {
    /// Get total messages published across all topics
    #[must_use] pub fn total_published(&self) -> u64 {
        self.topic_metrics.values().map(|m| m.publish_count).sum()
    }

    /// Get total messages handled across all topics
    #[must_use] pub fn total_handled(&self) -> u64 {
        self.topic_metrics.values().map(|m| m.handle_count).sum()
    }

    /// Get overall success rate
    #[must_use] pub fn overall_success_rate(&self) -> f64 {
        let total_handled = self.total_handled();
        let total_failed: u64 = self.topic_metrics.values().map(|m| m.failure_count).sum();
        let total_attempts = total_handled + total_failed;

        if total_attempts > 0 {
            // SAFETY: u64 to f64 for rate calculation
            total_handled as f64 / total_attempts as f64
        } else {
            0.0
        }
    }

    /// Get topics sorted by message count
    #[must_use] pub fn busiest_topics(&self) -> Vec<(String, u64)> {
        let mut topics: Vec<_> = self
            .topic_metrics
            .iter()
            .map(|(topic, metrics)| (topic.clone(), metrics.publish_count))
            .collect();
        topics.sort_by(|a, b| b.1.cmp(&a.1));
        topics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_metrics_creation() {
        let metrics = BusMetrics::new();
        assert_eq!(metrics.get_publish_count("test"), 0);
        assert_eq!(metrics.get_handle_count("test"), 0);
        assert_eq!(metrics.get_success_rate("test"), 0.0);
    }

    #[test]
    fn test_publish_metrics() {
        let metrics = BusMetrics::new();

        metrics.record_publish_attempt("test");
        metrics.record_publish_success("test");

        assert_eq!(metrics.get_publish_count("test"), 1);
    }

    #[test]
    fn test_handle_metrics() {
        let metrics = BusMetrics::new();

        metrics.record_handle_attempt("test");
        metrics.record_handle_success("test");
        metrics.record_handle_attempt("test");
        metrics.record_handle_failure("test");

        assert_eq!(metrics.get_handle_count("test"), 1);
        assert_eq!(metrics.get_failure_count("test"), 1);
        assert_eq!(metrics.get_success_rate("test"), 0.5);
    }

    #[test]
    fn test_duration_tracking() {
        let metrics = BusMetrics::new();

        metrics.record_handle_duration("test", Duration::from_millis(100));
        metrics.record_handle_duration("test", Duration::from_millis(200));

        let avg = metrics.get_avg_handle_duration("test").unwrap();
        assert_eq!(avg.as_millis(), 150);
    }

    #[test]
    fn test_metrics_snapshot() {
        let metrics = BusMetrics::new();

        metrics.record_publish_success("topic1");
        metrics.record_publish_success("topic2");
        metrics.record_handle_success("topic1");
        metrics.record_handle_failure("topic2");

        let snapshot = metrics.snapshot();

        assert_eq!(snapshot.topic_metrics.len(), 2);
        assert_eq!(snapshot.total_published(), 2);
        assert_eq!(snapshot.total_handled(), 1);
        assert_eq!(snapshot.overall_success_rate(), 0.5);
    }

    #[test]
    fn test_busiest_topics() {
        let metrics = BusMetrics::new();

        // Topic A: 3 messages
        metrics.record_publish_success("topic_a");
        metrics.record_publish_success("topic_a");
        metrics.record_publish_success("topic_a");

        // Topic B: 1 message
        metrics.record_publish_success("topic_b");

        // Topic C: 2 messages
        metrics.record_publish_success("topic_c");
        metrics.record_publish_success("topic_c");

        let snapshot = metrics.snapshot();
        let busiest = snapshot.busiest_topics();

        assert_eq!(busiest.len(), 3);
        assert_eq!(busiest[0].0, "topic_a");
        assert_eq!(busiest[0].1, 3);
        assert_eq!(busiest[1].0, "topic_c");
        assert_eq!(busiest[1].1, 2);
        assert_eq!(busiest[2].0, "topic_b");
        assert_eq!(busiest[2].1, 1);
    }
}
