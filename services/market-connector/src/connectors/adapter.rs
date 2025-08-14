//! Common feed adapter traits and configuration

use common::{L2Update, Symbol};
use rustc_hash::FxHashMap;
use tokio::sync::mpsc;

/// Feed adapter trait for market data sources
#[async_trait::async_trait]
pub trait FeedAdapter: Send + Sync {
    /// Connect to the feed
    async fn connect(&mut self) -> anyhow::Result<()>;

    /// Subscribe to symbols
    async fn subscribe(&mut self, symbols: Vec<Symbol>) -> anyhow::Result<()>;

    /// Start receiving updates
    async fn run(&mut self, tx: mpsc::Sender<L2Update>) -> anyhow::Result<()>;

    /// Disconnect from feed
    async fn disconnect(&mut self) -> anyhow::Result<()>;
}

/// Feed configuration
#[derive(Debug, Clone)]
pub struct FeedConfig {
    /// Feed name
    pub name: String,
    /// WebSocket URL
    pub ws_url: String,
    /// REST API URL
    pub api_url: String,
    /// Symbol mappings (internal -> exchange)
    pub symbol_map: FxHashMap<Symbol, String>,
    /// Max reconnect attempts
    pub max_reconnects: u32,
    /// Reconnect delay in milliseconds
    pub reconnect_delay_ms: u64,
}
