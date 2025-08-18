//! Venue connection management

use anyhow::Result;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Venue connection status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VenueStatus {
    /// Not connected
    Disconnected,
    /// Connecting
    Connecting,
    /// Connected and ready
    Connected,
    /// Connection error
    Error,
    /// Venue is down
    Down,
}

/// Venue statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VenueStats {
    /// Messages sent
    pub messages_sent: u64,
    /// Messages received  
    pub messages_received: u64,
    /// Orders sent
    pub orders_sent: u64,
    /// Orders filled
    pub orders_filled: u64,
    /// Average latency (microseconds)
    pub avg_latency_us: u64,
    /// Connection uptime (seconds)
    pub uptime_seconds: u64,
    /// Last error
    pub last_error: Option<String>,
}

/// Venue connection
pub struct VenueConnection {
    /// Venue name
    pub name: String,
    /// Connection status
    pub status: VenueStatus,
    /// Statistics
    pub stats: VenueStats,
    /// Configuration
    pub config: FxHashMap<String, String>,
}

/// Venue manager
pub struct VenueManager {
    /// Active connections
    connections: Arc<RwLock<FxHashMap<String, VenueConnection>>>,
    /// Primary venue
    primary_venue: String,
}

impl VenueManager {
    /// Create new venue manager
    #[must_use] pub fn new(primary_venue: String) -> Self {
        Self {
            connections: Arc::new(RwLock::new(FxHashMap::default())),
            primary_venue,
        }
    }

    /// Add venue
    pub async fn add_venue(&self, name: String, config: FxHashMap<String, String>) {
        let connection = VenueConnection {
            name: name.clone(),
            status: VenueStatus::Disconnected,
            stats: VenueStats::default(),
            config,
        };

        self.connections.write().await.insert(name, connection);
    }

    /// Connect to venue
    pub async fn connect_venue(&self, name: &str) -> Result<()> {
        if let Some(conn) = self.connections.write().await.get_mut(name) {
            conn.status = VenueStatus::Connecting;
            // Connection logic implemented in venue-specific adapters
            conn.status = VenueStatus::Connected;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Venue {} not found", name))
        }
    }

    /// Disconnect from venue
    pub async fn disconnect_venue(&self, name: &str) -> Result<()> {
        if let Some(conn) = self.connections.write().await.get_mut(name) {
            conn.status = VenueStatus::Disconnected;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Venue {} not found", name))
        }
    }

    /// Get venue status
    pub async fn get_venue_status(&self, name: &str) -> Option<VenueStatus> {
        self.connections.read().await.get(name).map(|c| c.status)
    }

    /// Get all venue statuses
    pub async fn get_all_statuses(&self) -> FxHashMap<String, VenueStatus> {
        self.connections
            .read()
            .await
            .iter()
            .map(|(name, conn)| (name.clone(), conn.status))
            .collect()
    }

    /// Update venue statistics
    pub async fn update_stats<F>(&self, name: &str, updater: F) -> Result<()>
    where
        F: FnOnce(&mut VenueStats),
    {
        if let Some(conn) = self.connections.write().await.get_mut(name) {
            updater(&mut conn.stats);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Venue {} not found", name))
        }
    }

    /// Get primary venue name
    #[must_use] pub fn get_primary_venue(&self) -> &str {
        &self.primary_venue
    }

    /// Set new primary venue
    pub fn set_primary_venue(&mut self, venue: String) {
        self.primary_venue = venue;
    }

    /// Check if primary venue is available
    pub async fn is_primary_venue_available(&self) -> bool {
        matches!(
            self.get_venue_status(&self.primary_venue).await,
            Some(VenueStatus::Connected)
        )
    }

    /// Get best available venue (primary if available, otherwise any connected)
    pub async fn get_best_available_venue(&self) -> Option<String> {
        // First try primary venue
        if self.is_primary_venue_available().await {
            return Some(self.primary_venue.clone());
        }

        // Otherwise find any connected venue
        self.connections
            .read()
            .await
            .iter()
            .find(|(_, conn)| conn.status == VenueStatus::Connected)
            .map(|(name, _)| name.clone())
    }
}
