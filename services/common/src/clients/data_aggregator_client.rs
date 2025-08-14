//! Data Aggregator Client
//!
//! Client for interacting with the data aggregator service

use anyhow::Result;
use common::{Px, Qty, Symbol, Ts};
use data_aggregator::{Candle, DataAggregator, DataAggregatorService, Timeframe};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Client for data aggregator service
pub struct DataAggregatorClient {
    /// The aggregator service
    service: Arc<RwLock<DataAggregatorService>>,
    /// Service name
    name: String,
}

impl DataAggregatorClient {
    /// Create new client without WAL
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            service: Arc::new(RwLock::new(DataAggregatorService::new())),
            name: name.into(),
        }
    }

    /// Create new client with WAL persistence
    pub fn with_wal(name: impl Into<String>, wal_path: &Path) -> Result<Self> {
        let service = DataAggregatorService::with_wal(wal_path)?;
        Ok(Self {
            service: Arc::new(RwLock::new(service)),
            name: name.into(),
        })
    }

    /// Process a trade
    pub async fn process_trade(
        &self,
        symbol: Symbol,
        price: Px,
        quantity: Qty,
        is_buy: bool,
    ) -> Result<()> {
        let ts = Ts::now();
        let mut service = self.service.write().await;
        service
            .process_trade(symbol, ts, price, quantity, is_buy)
            .await?;
        debug!(
            "[{}] Processed trade: {:?} {} @ {}",
            self.name, symbol, quantity, price
        );
        Ok(())
    }

    /// Process a trade with timestamp
    pub async fn process_trade_with_ts(
        &self,
        symbol: Symbol,
        ts: Ts,
        price: Px,
        quantity: Qty,
        is_buy: bool,
    ) -> Result<()> {
        let mut service = self.service.write().await;
        service
            .process_trade(symbol, ts, price, quantity, is_buy)
            .await?;
        debug!(
            "[{}] Processed trade: {:?} {} @ {} at {}",
            self.name, symbol, quantity, price, ts
        );
        Ok(())
    }

    /// Get current candle for symbol and timeframe
    pub async fn get_current_candle(&self, symbol: Symbol, timeframe: Timeframe) -> Option<Candle> {
        let service = self.service.read().await;
        service.get_current_candle(symbol, timeframe).await
    }

    /// Get completed candles
    pub async fn get_candles(
        &self,
        symbol: Symbol,
        timeframe: Timeframe,
        limit: usize,
    ) -> Vec<Candle> {
        let service = self.service.read().await;
        service.get_candles(symbol, timeframe, limit).await
    }

    /// Flush WAL to disk
    pub async fn flush_wal(&self) -> Result<()> {
        let service = self.service.read().await;
        service.flush_wal().await?;
        info!("[{}] WAL flushed to disk", self.name);
        Ok(())
    }

    /// Get WAL statistics
    pub async fn get_wal_stats(&self) -> Result<Option<data_aggregator::storage::wal::WalStats>> {
        let service = self.service.read().await;
        service.wal_stats().await
    }

    /// Replay events from WAL
    pub async fn replay_from_wal(&self, from_ts: Option<Ts>) -> Result<u64> {
        info!("[{}] Starting WAL replay from {:?}", self.name, from_ts);

        let service = self.service.read().await;
        let count = service.replay_from_wal(from_ts).await?;

        info!(
            "[{}] Completed WAL replay: {} events processed",
            self.name, count
        );
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_client_basic() {
        let client = DataAggregatorClient::new("test");

        let symbol = Symbol::new(1);
        let price = Px::from_i64(100_0000);
        let qty = Qty::from_i64(10_0000);

        // Process trade
        client
            .process_trade(symbol, price, qty, true)
            .await
            .unwrap();

        // Get current candle
        let candle = client.get_current_candle(symbol, Timeframe::M1).await;
        assert!(candle.is_some());

        let candle = candle.unwrap();
        assert_eq!(candle.symbol, symbol);
        assert_eq!(candle.volume, qty);
    }

    #[tokio::test]
    async fn test_client_with_wal() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path();

        let client = DataAggregatorClient::with_wal("test_wal", wal_path).unwrap();

        let symbol = Symbol::new(1);
        let price = Px::from_i64(100_0000);
        let qty = Qty::from_i64(10_0000);

        // Process multiple trades
        for i in 0..10 {
            let price = Px::from_i64(100_0000 + i * 1000);
            client
                .process_trade(symbol, price, qty, i % 2 == 0)
                .await
                .unwrap();
        }

        // Flush WAL
        client.flush_wal().await.unwrap();

        // Get WAL stats
        let stats = client.get_wal_stats().await.unwrap();
        assert!(stats.is_some());

        let stats = stats.unwrap();
        // We should have at least 1 segment with data
        assert!(stats.segment_count >= 1);
        assert!(stats.total_size > 0);
    }
}
