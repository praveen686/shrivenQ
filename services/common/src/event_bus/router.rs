//! Message routing for the event bus

use super::{BusMessage, MessageEnvelope};
use parking_lot::RwLock;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tracing::{debug, warn};

/// Message router trait for custom routing logic
pub trait MessageRouter<T: BusMessage>: Send + Sync {
    /// Route a message to appropriate topics
    fn route(&self, envelope: &MessageEnvelope<T>) -> Vec<String>;

    /// Check if router handles a specific topic
    fn handles_topic(&self, topic: &str) -> bool;
}

/// Topic-based router (default implementation)
pub struct TopicRouter {
    /// Topic patterns and their targets
    routes: Arc<RwLock<FxHashMap<String, FxHashSet<String>>>>,
}

impl TopicRouter {
    /// Create a new topic router
    pub fn new() -> Self {
        Self {
            routes: Arc::new(RwLock::new(FxHashMap::default())),
        }
    }

    /// Add a routing rule
    pub fn add_route(&self, pattern: impl Into<String>, target: impl Into<String>) {
        let pattern = pattern.into();
        let target = target.into();

        let mut routes = self.routes.write();
        routes
            .entry(pattern.clone())
            .or_insert_with(FxHashSet::default)
            .insert(target.clone());

        debug!(
            pattern = %pattern,
            target = %target,
            "Added routing rule"
        );
    }

    /// Remove a routing rule
    pub fn remove_route(&self, pattern: &str, target: &str) {
        let mut routes = self.routes.write();
        if let Some(targets) = routes.get_mut(pattern) {
            targets.remove(target);
            if targets.is_empty() {
                routes.remove(pattern);
            }
        }

        debug!(
            pattern = %pattern,
            target = %target,
            "Removed routing rule"
        );
    }

    /// Get all targets for a pattern
    pub fn get_targets(&self, pattern: &str) -> Vec<String> {
        let routes = self.routes.read();
        routes
            .get(pattern)
            .map(|targets| targets.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// List all routing patterns
    pub fn list_patterns(&self) -> Vec<String> {
        let routes = self.routes.read();
        routes.keys().cloned().collect()
    }

    /// Check if a topic matches a pattern
    fn topic_matches_pattern(&self, topic: &str, pattern: &str) -> bool {
        // Simple wildcard matching
        if pattern == "*" {
            return true;
        }

        if pattern.ends_with("*") {
            let prefix = &pattern[..pattern.len() - 1];
            return topic.starts_with(prefix);
        }

        if pattern.starts_with("*") {
            let suffix = &pattern[1..];
            return topic.ends_with(suffix);
        }

        // Exact match
        topic == pattern
    }
}

impl Default for TopicRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: BusMessage> MessageRouter<T> for TopicRouter {
    fn route(&self, envelope: &MessageEnvelope<T>) -> Vec<String> {
        let topic = envelope.topic();
        let routes = self.routes.read();

        let mut targets = FxHashSet::default();

        for (pattern, pattern_targets) in routes.iter() {
            if self.topic_matches_pattern(topic, pattern) {
                targets.extend(pattern_targets.iter().cloned());
            }
        }

        // If no routes found, route to original topic
        if targets.is_empty() {
            vec![topic.to_string()]
        } else {
            targets.into_iter().collect()
        }
    }

    fn handles_topic(&self, topic: &str) -> bool {
        let routes = self.routes.read();
        routes
            .keys()
            .any(|pattern| self.topic_matches_pattern(topic, pattern))
    }
}

/// Content-based router using message content for routing
pub struct ContentBasedRouter<T: BusMessage> {
    /// Routing rules based on message content
    rules: Arc<RwLock<Vec<ContentRule<T>>>>,
}

impl<T: BusMessage> ContentBasedRouter<T> {
    /// Create a new content-based router
    pub fn new() -> Self {
        Self {
            rules: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add a content-based routing rule
    pub fn add_rule(&self, rule: ContentRule<T>) {
        let mut rules = self.rules.write();
        rules.push(rule);
        debug!("Added content-based routing rule");
    }

    /// Clear all routing rules
    pub fn clear_rules(&self) {
        let mut rules = self.rules.write();
        rules.clear();
        debug!("Cleared all content-based routing rules");
    }
}

impl<T: BusMessage> Default for ContentBasedRouter<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: BusMessage> MessageRouter<T> for ContentBasedRouter<T> {
    fn route(&self, envelope: &MessageEnvelope<T>) -> Vec<String> {
        let rules = self.rules.read();
        let mut targets = FxHashSet::default();

        for rule in rules.iter() {
            if (rule.predicate)(envelope) {
                targets.extend((rule.target_fn)(envelope));
            }
        }

        // If no rules matched, route to original topic
        if targets.is_empty() {
            vec![envelope.topic().to_string()]
        } else {
            targets.into_iter().collect()
        }
    }

    fn handles_topic(&self, _topic: &str) -> bool {
        // Content-based router can handle any topic
        true
    }
}

/// Content-based routing rule
pub struct ContentRule<T: BusMessage> {
    /// Predicate function to check if rule applies
    pub predicate: Box<dyn Fn(&MessageEnvelope<T>) -> bool + Send + Sync>,
    /// Function to determine target topics
    pub target_fn: Box<dyn Fn(&MessageEnvelope<T>) -> Vec<String> + Send + Sync>,
}

/// Priority-based router that routes based on message priority
pub struct PriorityRouter {
    /// Priority thresholds and their target topics
    priority_routes: Arc<RwLock<Vec<(u8, String)>>>,
}

impl PriorityRouter {
    /// Create a new priority router
    pub fn new() -> Self {
        Self {
            priority_routes: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add a priority-based route
    pub fn add_priority_route(&self, max_priority: u8, target: impl Into<String>) {
        let mut routes = self.priority_routes.write();
        routes.push((max_priority, target.into()));
        routes.sort_by_key(|&(priority, _)| priority);

        // Log the newly added route - we just pushed so last() is safe
        if let Some((_, target)) = routes.last() {
            debug!(
                max_priority = max_priority,
                target = %target,
                "Added priority route"
            );
        }
    }

    /// Remove a priority route
    pub fn remove_priority_route(&self, max_priority: u8, target: &str) {
        let mut routes = self.priority_routes.write();
        routes.retain(|(p, t)| !(*p == max_priority && t == target));

        debug!(
            max_priority = max_priority,
            target = %target,
            "Removed priority route"
        );
    }
}

impl Default for PriorityRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: BusMessage> MessageRouter<T> for PriorityRouter {
    fn route(&self, envelope: &MessageEnvelope<T>) -> Vec<String> {
        let priority = envelope.priority();
        let routes = self.priority_routes.read();

        // Find the first route that handles this priority level
        for &(max_priority, ref target) in routes.iter() {
            if priority <= max_priority {
                return vec![target.clone()];
            }
        }

        // If no priority route found, use original topic
        vec![envelope.topic().to_string()]
    }

    fn handles_topic(&self, _topic: &str) -> bool {
        // Priority router can handle any topic
        true
    }
}

/// Load balancing router that distributes messages across multiple targets
pub struct LoadBalancingRouter {
    /// Available targets
    targets: Arc<RwLock<Vec<String>>>,
    /// Current index for round-robin
    current_index: Arc<parking_lot::Mutex<usize>>,
}

impl LoadBalancingRouter {
    /// Create a new load balancing router
    pub fn new(targets: Vec<String>) -> Self {
        Self {
            targets: Arc::new(RwLock::new(targets)),
            current_index: Arc::new(parking_lot::Mutex::new(0)),
        }
    }

    /// Add a target
    pub fn add_target(&self, target: impl Into<String>) {
        let mut targets = self.targets.write();
        targets.push(target.into());
        debug!(target_count = targets.len(), "Added load balancing target");
    }

    /// Remove a target
    pub fn remove_target(&self, target: &str) {
        let mut targets = self.targets.write();
        targets.retain(|t| t != target);

        // Reset index if it's out of bounds
        let mut index = self.current_index.lock();
        if *index >= targets.len() && !targets.is_empty() {
            *index = 0;
        }

        debug!(
            target_count = targets.len(),
            "Removed load balancing target"
        );
    }

    /// Get next target using round-robin
    fn next_target(&self) -> Option<String> {
        let targets = self.targets.read();
        if targets.is_empty() {
            return None;
        }

        let mut index = self.current_index.lock();
        let target = targets[*index].clone();
        *index = (*index + 1) % targets.len();

        Some(target)
    }
}

impl<T: BusMessage> MessageRouter<T> for LoadBalancingRouter {
    fn route(&self, envelope: &MessageEnvelope<T>) -> Vec<String> {
        if let Some(target) = self.next_target() {
            vec![target]
        } else {
            warn!("No targets available for load balancing");
            vec![envelope.topic().to_string()]
        }
    }

    fn handles_topic(&self, _topic: &str) -> bool {
        !self.targets.read().is_empty()
    }
}

/// Composite router that combines multiple routing strategies
pub struct CompositeRouter<T: BusMessage> {
    /// List of routers to apply in order
    routers: Vec<Arc<dyn MessageRouter<T>>>,
}

impl<T: BusMessage> CompositeRouter<T> {
    /// Create a new composite router
    pub fn new() -> Self {
        Self {
            routers: Vec::new(),
        }
    }

    /// Add a router to the chain
    pub fn add_router(&mut self, router: Arc<dyn MessageRouter<T>>) {
        self.routers.push(router);
        debug!(
            router_count = self.routers.len(),
            "Added router to composite"
        );
    }
}

impl<T: BusMessage> Default for CompositeRouter<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: BusMessage> MessageRouter<T> for CompositeRouter<T> {
    fn route(&self, envelope: &MessageEnvelope<T>) -> Vec<String> {
        let mut all_targets = FxHashSet::default();

        for router in &self.routers {
            let targets = router.route(envelope);
            all_targets.extend(targets);
        }

        if all_targets.is_empty() {
            vec![envelope.topic().to_string()]
        } else {
            all_targets.into_iter().collect()
        }
    }

    fn handles_topic(&self, topic: &str) -> bool {
        self.routers
            .iter()
            .any(|router| router.handles_topic(topic))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::{MessageMetadata, ShrivenQuantMessage};

    #[test]
    fn test_topic_router_exact_match() {
        let router = TopicRouter::new();
        router.add_route("market_data", "topic_a");
        router.add_route("orders", "topic_b");

        let message = ShrivenQuantMessage::MarketData {
            symbol: "BTCUSDT".to_string(),
            exchange: "binance".to_string(),
            bid: 50000,
            ask: 50001,
            timestamp: 123456789,
        };

        let envelope = MessageEnvelope::new(message, MessageMetadata::default());
        let targets = router.route(&envelope);

        assert_eq!(targets, vec!["topic_a"]);
    }

    #[test]
    fn test_topic_router_wildcard() {
        let router = TopicRouter::new();
        router.add_route("market_*", "all_market_data");

        let message = ShrivenQuantMessage::MarketData {
            symbol: "BTCUSDT".to_string(),
            exchange: "binance".to_string(),
            bid: 50000,
            ask: 50001,
            timestamp: 123456789,
        };

        let envelope = MessageEnvelope::new(message, MessageMetadata::default());
        let targets = router.route(&envelope);

        assert_eq!(targets, vec!["all_market_data"]);
    }

    #[test]
    fn test_priority_router() {
        let router = PriorityRouter::new();
        router.add_priority_route(50, "medium_priority_queue");
        router.add_priority_route(10, "high_priority_queue");

        // High priority message (priority 5)
        let high_priority_msg = ShrivenQuantMessage::RiskAlert {
            level: "EMERGENCY".to_string(),
            message: "Test".to_string(),
            source: "test".to_string(),
            symbol: None,
            value: None,
            timestamp: 123456789,
        };

        let envelope = MessageEnvelope::new(high_priority_msg, MessageMetadata::default());
        let targets = router.route(&envelope);

        assert_eq!(targets, vec!["high_priority_queue"]);
    }

    #[test]
    fn test_load_balancing_router() {
        let router = LoadBalancingRouter::new(vec![
            "target_1".to_string(),
            "target_2".to_string(),
            "target_3".to_string(),
        ]);

        let message = ShrivenQuantMessage::MarketData {
            symbol: "BTCUSDT".to_string(),
            exchange: "binance".to_string(),
            bid: 50000,
            ask: 50001,
            timestamp: 123456789,
        };

        let envelope = MessageEnvelope::new(message.clone(), MessageMetadata::default());

        // Should cycle through targets
        let target1 = router.route(&envelope);
        let target2 = router.route(&envelope);
        let target3 = router.route(&envelope);
        let target4 = router.route(&envelope); // Should wrap back to first

        assert_eq!(target1, vec!["target_1"]);
        assert_eq!(target2, vec!["target_2"]);
        assert_eq!(target3, vec!["target_3"]);
        assert_eq!(target4, vec!["target_1"]);
    }

    #[test]
    fn test_composite_router() {
        let mut composite = CompositeRouter::new();

        let topic_router = TopicRouter::new();
        topic_router.add_route("market_data", "topic_route");
        composite.add_router(Arc::new(topic_router));

        let priority_router = PriorityRouter::new();
        priority_router.add_priority_route(100, "priority_route");
        composite.add_router(Arc::new(priority_router));

        let message = ShrivenQuantMessage::MarketData {
            symbol: "BTCUSDT".to_string(),
            exchange: "binance".to_string(),
            bid: 50000,
            ask: 50001,
            timestamp: 123456789,
        };

        let envelope = MessageEnvelope::new(message, MessageMetadata::default());
        let targets = router.route(&envelope);

        // Should get targets from both routers
        assert!(targets.contains(&"topic_route".to_string()));
        assert!(targets.contains(&"priority_route".to_string()));
    }
}
