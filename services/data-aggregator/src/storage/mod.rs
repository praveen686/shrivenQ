//! Storage backends for aggregated data

pub mod events;
pub mod segment;
pub mod wal;

use crate::{Candle, Symbol, Timeframe, VolumeProfile};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

// Re-export commonly used types
pub use events::{
    CandleEvent, DataEvent, MicrostructureEvent, SystemEvent, TradeEvent, VolumeProfileEvent,
};
pub use segment::{Segment, SegmentReader};
pub use wal::{Wal, WalEntry, WalStats};

/// Storage backend trait
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Store candle
    async fn store_candle(&mut self, candle: &Candle) -> Result<()>;

    /// Get candles for symbol and timeframe
    async fn get_candles(
        &self,
        symbol: Symbol,
        timeframe: Timeframe,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Candle>>;

    /// Store volume profile
    async fn store_volume_profile(&mut self, profile: &VolumeProfile) -> Result<()>;

    /// Get volume profiles
    async fn get_volume_profiles(
        &self,
        symbol: Symbol,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<VolumeProfile>>;
}

/// Redis storage backend
pub struct RedisStorage {
    client: redis::aio::ConnectionManager,
}

impl RedisStorage {
    /// Create new Redis storage
    pub async fn new(url: &str) -> Result<Self> {
        let client = redis::Client::open(url)?;
        let connection = client.get_connection_manager().await?;
        Ok(Self { client: connection })
    }
}

#[async_trait]
impl StorageBackend for RedisStorage {
    async fn store_candle(&mut self, candle: &Candle) -> Result<()> {
        use redis::AsyncCommands;

        let key = format!(
            "candle:{}:{}:{}",
            candle.symbol.0,
            format!("{:?}", candle.timeframe).to_lowercase(),
            candle.open_time.timestamp()
        );

        let value = serde_json::to_string(candle)?;
        // Redis returns () for SET operations, explicitly typed for clarity
        let (): () = self.client.set_ex(&key, value, 86400 * 7).await?; // Keep for 7 days

        Ok(())
    }

    async fn get_candles(
        &self,
        _symbol: Symbol,
        _timeframe: Timeframe,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Result<Vec<Candle>> {
        // Implementation would scan Redis keys and deserialize
        Ok(Vec::new())
    }

    async fn store_volume_profile(&mut self, _profile: &VolumeProfile) -> Result<()> {
        // Store volume profile in Redis
        Ok(())
    }

    async fn get_volume_profiles(
        &self,
        _symbol: Symbol,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Result<Vec<VolumeProfile>> {
        Ok(Vec::new())
    }
}
