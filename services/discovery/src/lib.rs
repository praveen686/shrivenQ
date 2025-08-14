//! Service discovery and health checking

use anyhow::Result;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Service instance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstance {
    pub name: String,
    pub id: String,
    pub address: String,
    pub port: u16,
    pub health_check_url: String,
    pub metadata: FxHashMap<String, String>,
    pub last_heartbeat: u64,
}

/// Service registry
pub struct ServiceRegistry {
    instances: Arc<RwLock<FxHashMap<String, Vec<ServiceInstance>>>>,
    health_check_interval: u64,
}

impl ServiceRegistry {
    /// Create new service registry
    pub fn new(health_check_interval: u64) -> Self {
        Self {
            instances: Arc::new(RwLock::new(FxHashMap::default())),
            health_check_interval,
        }
    }

    /// Register a service instance
    pub async fn register(&self, instance: ServiceInstance) -> Result<()> {
        let mut instances = self.instances.write().await;
        let service_instances = instances
            .entry(instance.name.clone())
            .or_insert_with(Vec::new);

        // Check if instance already exists
        if let Some(existing) = service_instances.iter_mut().find(|i| i.id == instance.id) {
            // Update existing instance
            existing.address = instance.address.clone();
            existing.port = instance.port;
            existing.health_check_url = instance.health_check_url.clone();
            existing.metadata = instance.metadata.clone();
            // SAFETY: timestamp().max(0) ensures non-negative value before cast to u64
            existing.last_heartbeat = chrono::Utc::now().timestamp().max(0) as u64;
            info!(
                "Updated service instance: {} - {}",
                instance.name, instance.id
            );
        } else {
            // Add new instance
            let mut new_instance = instance;
            // SAFETY: timestamp().max(0) ensures non-negative value before cast to u64
            new_instance.last_heartbeat = chrono::Utc::now().timestamp().max(0) as u64;
            service_instances.push(new_instance.clone());
            info!(
                "Registered new service instance: {} - {}",
                new_instance.name, new_instance.id
            );
        }

        Ok(())
    }

    /// Deregister a service instance
    pub async fn deregister(&self, service_name: &str, instance_id: &str) -> Result<()> {
        let mut instances = self.instances.write().await;

        if let Some(service_instances) = instances.get_mut(service_name) {
            service_instances.retain(|i| i.id != instance_id);
            info!(
                "Deregistered service instance: {} - {}",
                service_name, instance_id
            );
        }

        Ok(())
    }

    /// Get healthy instances of a service
    pub async fn get_instances(&self, service_name: &str) -> Vec<ServiceInstance> {
        let instances = self.instances.read().await;
        // SAFETY: timestamp().max(0) ensures non-negative value before cast to u64
        let now = chrono::Utc::now().timestamp().max(0) as u64;

        instances
            .get(service_name)
            .map(|instances| {
                instances
                    .iter()
                    .filter(|i| {
                        // Consider instance healthy if heartbeat is within 3x check interval
                        now - i.last_heartbeat < self.health_check_interval * 3
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get a single healthy instance (load balancing can be added here)
    pub async fn get_instance(&self, service_name: &str) -> Option<ServiceInstance> {
        let instances = self.get_instances(service_name).await;
        // Simple round-robin or random selection can be implemented here
        instances.into_iter().next()
    }

    /// Update heartbeat for an instance
    pub async fn heartbeat(&self, service_name: &str, instance_id: &str) -> Result<()> {
        let mut instances = self.instances.write().await;

        if let Some(service_instances) = instances.get_mut(service_name) {
            if let Some(instance) = service_instances.iter_mut().find(|i| i.id == instance_id) {
                // SAFETY: timestamp().max(0) ensures non-negative value before cast to u64
                instance.last_heartbeat = chrono::Utc::now().timestamp().max(0) as u64;
                debug!("Updated heartbeat for: {} - {}", service_name, instance_id);
            }
        }

        Ok(())
    }

    /// Remove unhealthy instances
    pub async fn cleanup_unhealthy(&self) {
        let mut instances = self.instances.write().await;
        // SAFETY: timestamp().max(0) ensures non-negative value before cast to u64
        let now = chrono::Utc::now().timestamp().max(0) as u64;
        let timeout = self.health_check_interval * 3;

        for (service_name, service_instances) in instances.iter_mut() {
            let before_count = service_instances.len();
            service_instances.retain(|i| {
                let is_healthy = now - i.last_heartbeat < timeout;
                if !is_healthy {
                    warn!("Removing unhealthy instance: {} - {}", service_name, i.id);
                }
                is_healthy
            });

            let removed = before_count - service_instances.len();
            if removed > 0 {
                info!(
                    "Removed {} unhealthy instances from {}",
                    removed, service_name
                );
            }
        }
    }

    /// Start background health check task
    pub async fn start_health_checker(self: Arc<Self>) {
        let interval = self.health_check_interval;

        tokio::spawn(async move {
            let mut interval_timer =
                tokio::time::interval(tokio::time::Duration::from_secs(interval));

            loop {
                interval_timer.tick().await;
                self.cleanup_unhealthy().await;
            }
        });

        info!("Started health checker with {}s interval", interval);
    }
}

/// Client for service discovery
pub struct DiscoveryClient {
    _registry_url: String, // Reserved for external registry integration
    service_name: String,
    instance_id: String,
}

impl DiscoveryClient {
    /// Create new discovery client
    pub fn new(registry_url: &str, service_name: &str, instance_id: &str) -> Self {
        Self {
            _registry_url: registry_url.to_string(),
            service_name: service_name.to_string(),
            instance_id: instance_id.to_string(),
        }
    }

    /// Register this service instance
    pub async fn register(&self, address: &str, port: u16) -> Result<()> {
        // In a real implementation, this would create a ServiceInstance and
        // make an HTTP/gRPC call to the registry
        info!(
            "Registering service: {} (id: {}) at {}:{}",
            self.service_name, self.instance_id, address, port
        );
        Ok(())
    }

    /// Send heartbeat
    pub async fn heartbeat(&self) -> Result<()> {
        // In a real implementation, this would make an HTTP/gRPC call to the registry
        debug!("Sending heartbeat for: {}", self.instance_id);
        Ok(())
    }

    /// Deregister this instance
    pub async fn deregister(&self) -> Result<()> {
        // In a real implementation, this would make an HTTP/gRPC call to the registry
        info!("Deregistering service: {}", self.instance_id);
        Ok(())
    }

    /// Discover instances of a service
    pub async fn discover(&self, service_name: &str) -> Result<Vec<ServiceInstance>> {
        // In a real implementation, this would make an HTTP/gRPC call to the registry
        debug!("Discovering instances of: {}", service_name);
        Ok(vec![])
    }
}
