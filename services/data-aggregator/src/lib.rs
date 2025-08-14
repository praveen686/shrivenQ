//! Data Aggregator Service
//!
//! Aggregates raw market data into various timeframes and formats:
//! - OHLCV candles (1m, 5m, 15m, 1h, 1d)
//! - Volume profiles
//! - Trade aggregations
//! - Market microstructure features

pub mod aggregators;
pub mod config;
pub mod storage;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use common::constants::memory::WAL_REPLAY_BATCH_SIZE;
use common::{Px, Qty, Symbol, Ts};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

// Re-export WAL types
pub use storage::{CandleEvent, DataEvent, TradeEvent, Wal, WalEntry};

/// Timeframe for aggregation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Timeframe {
    /// 1 minute bars
    M1,
    /// 5 minute bars
    M5,
    /// 15 minute bars
    M15,
    /// 30 minute bars
    M30,
    /// 1 hour bars
    H1,
    /// 4 hour bars
    H4,
    /// Daily bars
    D1,
    /// Weekly bars
    W1,
}

impl Timeframe {
    /// Get duration in seconds
    pub fn duration_seconds(&self) -> i64 {
        match self {
            Timeframe::M1 => 60,
            Timeframe::M5 => 300,
            Timeframe::M15 => 900,
            Timeframe::M30 => 1800,
            Timeframe::H1 => 3600,
            Timeframe::H4 => 14400,
            Timeframe::D1 => 86400,
            Timeframe::W1 => 604800,
        }
    }

    /// Get chrono duration
    pub fn to_duration(&self) -> Duration {
        Duration::seconds(self.duration_seconds())
    }
}

/// OHLCV candle data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    /// Symbol
    pub symbol: Symbol,
    /// Timeframe
    pub timeframe: Timeframe,
    /// Open time
    pub open_time: DateTime<Utc>,
    /// Close time
    pub close_time: DateTime<Utc>,
    /// Open price
    pub open: Px,
    /// High price
    pub high: Px,
    /// Low price
    pub low: Px,
    /// Close price
    pub close: Px,
    /// Volume
    pub volume: Qty,
    /// Number of trades
    pub trades: u32,
    /// Buy volume
    pub buy_volume: Qty,
    /// Sell volume
    pub sell_volume: Qty,
}

impl Candle {
    /// Create new candle
    pub fn new(symbol: Symbol, timeframe: Timeframe, open_time: DateTime<Utc>) -> Self {
        let close_time = open_time + timeframe.to_duration();
        Self {
            symbol,
            timeframe,
            open_time,
            close_time,
            open: Px::ZERO,
            high: Px::from_i64(i64::MIN), // Will be updated on first trade
            low: Px::from_i64(i64::MAX),  // Will be updated on first trade
            close: Px::ZERO,
            volume: Qty::ZERO,
            trades: 0,
            buy_volume: Qty::ZERO,
            sell_volume: Qty::ZERO,
        }
    }

    /// Update candle with trade
    pub fn update_trade(&mut self, price: Px, qty: Qty, is_buy: bool) {
        if self.trades == 0 {
            self.open = price;
            self.high = price;
            self.low = price;
        } else {
            if price > self.high {
                self.high = price;
            }
            if price < self.low {
                self.low = price;
            }
        }

        self.close = price;
        self.volume = Qty::from_i64(self.volume.as_i64() + qty.as_i64());
        self.trades += 1;

        if is_buy {
            self.buy_volume = Qty::from_i64(self.buy_volume.as_i64() + qty.as_i64());
        } else {
            self.sell_volume = Qty::from_i64(self.sell_volume.as_i64() + qty.as_i64());
        }
    }

    /// Check if candle is complete
    pub fn is_complete(&self, current_time: DateTime<Utc>) -> bool {
        current_time >= self.close_time
    }
}

/// Volume profile level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeLevel {
    /// Price level
    pub price: Px,
    /// Total volume at this level
    pub volume: Qty,
    /// Buy volume
    pub buy_volume: Qty,
    /// Sell volume
    pub sell_volume: Qty,
    /// Number of trades
    pub trades: u32,
}

/// Volume profile for a time period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeProfile {
    /// Symbol
    pub symbol: Symbol,
    /// Start time
    pub start_time: DateTime<Utc>,
    /// End time
    pub end_time: DateTime<Utc>,
    /// Price levels
    pub levels: Vec<VolumeLevel>,
    /// Point of control (price with highest volume)
    pub poc: Px,
    /// Value area high (70% volume area)
    pub vah: Px,
    /// Value area low (70% volume area)
    pub val: Px,
}

/// Trade aggregation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeAggregation {
    /// Symbol
    pub symbol: Symbol,
    /// Aggregation period
    pub period: Duration,
    /// Total trades
    pub total_trades: u64,
    /// Total volume
    pub total_volume: Qty,
    /// Average trade size
    pub avg_trade_size: Qty,
    /// Large trades (> 10x average)
    pub large_trades: u64,
    /// Buy/sell imbalance (fixed-point: -10000 to 10000 for -100% to 100%)
    pub imbalance: i32,
}

/// Data aggregator trait
#[async_trait]
pub trait DataAggregator: Send + Sync {
    /// Process trade event
    async fn process_trade(
        &mut self,
        symbol: Symbol,
        ts: Ts,
        price: Px,
        qty: Qty,
        is_buy: bool,
    ) -> Result<()>;

    /// Process order book update
    async fn process_book_update(
        &mut self,
        symbol: Symbol,
        ts: Ts,
        bid: Px,
        ask: Px,
        bid_qty: Qty,
        ask_qty: Qty,
    ) -> Result<()>;

    /// Get current candle for timeframe
    async fn get_current_candle(&self, symbol: Symbol, timeframe: Timeframe) -> Option<Candle>;

    /// Get completed candles
    async fn get_candles(&self, symbol: Symbol, timeframe: Timeframe, limit: usize) -> Vec<Candle>;

    /// Get volume profile
    async fn get_volume_profile(
        &self,
        symbol: Symbol,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Option<VolumeProfile>;

    /// Get trade aggregation
    async fn get_trade_aggregation(
        &self,
        symbol: Symbol,
        period: Duration,
    ) -> Option<TradeAggregation>;
}

/// Data aggregator service implementation with WAL persistence
pub struct DataAggregatorService {
    /// Active candles by symbol and timeframe
    candles: Arc<RwLock<FxHashMap<(Symbol, Timeframe), Candle>>>,
    /// Completed candles storage
    completed_candles: Arc<RwLock<FxHashMap<(Symbol, Timeframe), Vec<Candle>>>>,
    /// Volume profiles
    volume_profiles: Arc<RwLock<FxHashMap<Symbol, VolumeProfile>>>,
    /// Trade aggregations
    trade_aggregations: Arc<RwLock<FxHashMap<Symbol, TradeAggregation>>>,
    /// Write-ahead log for persistence
    wal: Option<Arc<RwLock<storage::wal::Wal>>>,
}

impl DataAggregatorService {
    /// Create new aggregator service
    pub fn new() -> Self {
        Self {
            candles: Arc::new(RwLock::new(FxHashMap::default())),
            completed_candles: Arc::new(RwLock::new(FxHashMap::default())),
            volume_profiles: Arc::new(RwLock::new(FxHashMap::default())),
            trade_aggregations: Arc::new(RwLock::new(FxHashMap::default())),
            wal: None,
        }
    }

    /// Create new aggregator service with WAL persistence
    pub fn with_wal(wal_path: &Path) -> Result<Self> {
        let wal = storage::wal::Wal::new(wal_path, None)?;

        let mut service = Self::new();
        service.wal = Some(Arc::new(RwLock::new(wal)));

        info!(
            "Data aggregator initialized with WAL at {}",
            wal_path.display()
        );
        Ok(service)
    }

    /// Persist event to WAL
    async fn persist_event(&self, event: DataEvent) -> Result<()> {
        if let Some(wal) = &self.wal {
            let mut wal = wal.write().await;
            wal.append(&event)?;
        }
        Ok(())
    }

    /// Flush WAL to disk
    pub async fn flush_wal(&self) -> Result<()> {
        if let Some(wal) = &self.wal {
            let mut wal = wal.write().await;
            wal.flush()?;
        }
        Ok(())
    }

    /// Get WAL statistics
    pub async fn wal_stats(&self) -> Result<Option<storage::wal::WalStats>> {
        if let Some(wal) = &self.wal {
            let wal = wal.read().await;
            Ok(Some(wal.stats()?))
        } else {
            Ok(None)
        }
    }

    /// Replay events from WAL starting from optional timestamp
    pub async fn replay_from_wal(&self, from_ts: Option<Ts>) -> Result<u64> {
        if let Some(wal) = &self.wal {
            let wal = wal.read().await;
            let mut iterator = wal.stream::<DataEvent>(from_ts)?;
            let mut count = 0u64;

            while let Some(event) = iterator.read_next_entry()? {
                // Process each replayed event
                // Count and log replayed events without re-processing them
                // (WAL replay is for auditing/verification, not re-processing)
                match &event {
                    DataEvent::Trade(trade_event) => {
                        debug!(
                            "Replayed trade: {:?} {} @ {} ({})",
                            trade_event.symbol,
                            trade_event.quantity,
                            trade_event.price,
                            if trade_event.is_buy { "BUY" } else { "SELL" }
                        );
                        count += 1;
                    }
                    DataEvent::Candle(candle_event) => {
                        debug!(
                            "Replayed candle: {:?} {} OHLCV({}, {}, {}, {}, {})",
                            candle_event.symbol,
                            candle_event.timeframe,
                            candle_event.open,
                            candle_event.high,
                            candle_event.low,
                            candle_event.close,
                            candle_event.volume
                        );
                        count += 1;
                    }
                    DataEvent::System(system_event) => {
                        info!(
                            "Replayed system event: {:?} at {:?}",
                            system_event.event_type, system_event.ts
                        );
                        count += 1;
                    }
                    DataEvent::VolumeProfile(volume_event) => {
                        debug!(
                            "Replayed volume profile: {:?} at {:?}",
                            volume_event.symbol, volume_event.ts
                        );
                        count += 1;
                    }
                    DataEvent::Microstructure(micro_event) => {
                        debug!(
                            "Replayed microstructure event: {:?} at {:?}",
                            micro_event.symbol, micro_event.ts
                        );
                        count += 1;
                    }
                }

                // Prevent memory buildup during large replays
                if count % WAL_REPLAY_BATCH_SIZE == 0 {
                    tokio::task::yield_now().await;
                }
            }

            info!(
                "Replayed {} events from WAL starting at {:?}",
                count, from_ts
            );
            Ok(count)
        } else {
            anyhow::bail!("WAL not configured for this service");
        }
    }

    /// Check and complete candles
    async fn check_complete_candles(&self, current_time: DateTime<Utc>) {
        let mut candles = self.candles.write().await;
        let mut completed = self.completed_candles.write().await;

        let mut to_remove = Vec::new();

        for (key, candle) in candles.iter() {
            if candle.is_complete(current_time) {
                to_remove.push(*key);

                // Store completed candle
                completed
                    .entry(*key)
                    .or_insert_with(Vec::new)
                    .push(candle.clone());

                // Keep only last 1000 candles per symbol/timeframe
                if let Some(candles) = completed.get_mut(key) {
                    if candles.len() > 1000 {
                        candles.drain(0..candles.len() - 1000);
                    }
                }

                info!("Completed {:?} candle for symbol {:?}", key.1, key.0);
            }
        }

        // Remove completed candles and create new ones
        for key in to_remove {
            if let Some(old_candle) = candles.remove(&key) {
                // Create new candle for next period
                let new_candle = Candle::new(key.0, key.1, old_candle.close_time);
                candles.insert(key, new_candle);
            }
        }
    }
}

#[async_trait]
impl DataAggregator for DataAggregatorService {
    async fn process_trade(
        &mut self,
        symbol: Symbol,
        ts: Ts,
        price: Px,
        qty: Qty,
        is_buy: bool,
    ) -> Result<()> {
        // SAFETY: Timestamps in nanoseconds since epoch fit in i64 until year 2262
        // SAFETY: u64 to i64 - timestamps before year 2262 fit in i64
        let current_time = DateTime::from_timestamp_nanos(ts.as_nanos() as i64);

        // Persist trade event to WAL
        let trade_event = DataEvent::Trade(TradeEvent {
            ts,
            symbol,
            price,
            quantity: qty,
            is_buy,
            trade_id: 0, // Would be set by exchange
        });
        self.persist_event(trade_event).await?;

        // Check for completed candles
        self.check_complete_candles(current_time).await;

        let mut candles = self.candles.write().await;

        // Update candles for all timeframes
        for timeframe in &[
            Timeframe::M1,
            Timeframe::M5,
            Timeframe::M15,
            Timeframe::M30,
            Timeframe::H1,
            Timeframe::H4,
            Timeframe::D1,
        ] {
            let key = (symbol, *timeframe);
            let candle = candles.entry(key).or_insert_with(|| {
                // Calculate candle start time
                let duration_secs = timeframe.duration_seconds();
                let timestamp_secs = current_time.timestamp();
                let candle_start_secs = (timestamp_secs / duration_secs) * duration_secs;
                let open_time =
                    DateTime::from_timestamp(candle_start_secs, 0).unwrap_or_else(|| Utc::now());

                Candle::new(symbol, *timeframe, open_time)
            });

            candle.update_trade(price, qty, is_buy);
        }

        debug!("Processed trade for {:?}: {} @ {}", symbol, qty, price);
        Ok(())
    }

    async fn process_book_update(
        &mut self,
        symbol: Symbol,
        _ts: Ts,
        _bid: Px,
        _ask: Px,
        _bid_qty: Qty,
        _ask_qty: Qty,
    ) -> Result<()> {
        // For now, we only track trades for candles
        // Book updates could be used for other analytics
        debug!("Processed book update for {:?}", symbol);
        Ok(())
    }

    async fn get_current_candle(&self, symbol: Symbol, timeframe: Timeframe) -> Option<Candle> {
        let candles = self.candles.read().await;
        candles.get(&(symbol, timeframe)).cloned()
    }

    async fn get_candles(&self, symbol: Symbol, timeframe: Timeframe, limit: usize) -> Vec<Candle> {
        let completed = self.completed_candles.read().await;
        if let Some(candles) = completed.get(&(symbol, timeframe)) {
            let start = if candles.len() > limit {
                candles.len() - limit
            } else {
                0
            };
            candles[start..].to_vec()
        } else {
            Vec::new()
        }
    }

    async fn get_volume_profile(
        &self,
        symbol: Symbol,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Option<VolumeProfile> {
        let profiles = self.volume_profiles.read().await;
        profiles.get(&symbol).cloned()
    }

    async fn get_trade_aggregation(
        &self,
        symbol: Symbol,
        _period: Duration,
    ) -> Option<TradeAggregation> {
        let aggregations = self.trade_aggregations.read().await;
        aggregations.get(&symbol).cloned()
    }
}

impl Default for DataAggregatorService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeframe_duration() {
        assert_eq!(Timeframe::M1.duration_seconds(), 60);
        assert_eq!(Timeframe::H1.duration_seconds(), 3600);
        assert_eq!(Timeframe::D1.duration_seconds(), 86400);
    }

    #[tokio::test]
    async fn test_candle_update() {
        let mut candle = Candle::new(Symbol::new(1), Timeframe::M1, Utc::now());

        let price = Px::from_price_i32(100_0000);
        let qty = Qty::from_qty_i32(10_0000);

        candle.update_trade(price, qty, true);

        assert_eq!(candle.open, price);
        assert_eq!(candle.high, price);
        assert_eq!(candle.low, price);
        assert_eq!(candle.close, price);
        assert_eq!(candle.volume, qty);
        assert_eq!(candle.buy_volume, qty);
        assert_eq!(candle.trades, 1);
    }
}
