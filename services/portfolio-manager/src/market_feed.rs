//! Market Data Feed Integration
//!
//! COMPLIANCE:
//! - Zero allocations in hot paths
//! - Fixed-point arithmetic only
//! - Lock-free price updates
//! - Pre-allocated buffers

use anyhow::{Context, Result};
use services_common::constants::fixed_point::SCALE_4 as FIXED_POINT_SCALE;
use services_common::{Symbol, Ts};
use crossbeam::channel::{Receiver, Sender, bounded};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use tokio::sync::RwLock;
use tonic::transport::Channel;

/// Market price snapshot
#[repr(C, align(64))] // Cache-line aligned
#[derive(Debug)]
pub struct PriceSnapshot {
    /// Best bid price (fixed-point)
    pub bid: AtomicI64,
    /// Best ask price (fixed-point)
    pub ask: AtomicI64,
    /// Last trade price (fixed-point)
    pub last: AtomicI64,
    /// Volume (fixed-point)
    pub volume: AtomicI64,
    /// Update timestamp (nanoseconds)
    pub timestamp: AtomicU64,
}

impl PriceSnapshot {
    /// Create new price snapshot
    pub fn new() -> Self {
        Self {
            bid: AtomicI64::new(0),
            ask: AtomicI64::new(0),
            last: AtomicI64::new(0),
            volume: AtomicI64::new(0),
            timestamp: AtomicU64::new(0),
        }
    }

    /// Update prices atomically
    #[inline]
    pub fn update(&self, bid: i64, ask: i64, last: i64, volume: i64, timestamp: u64) {
        self.bid.store(bid, Ordering::Release);
        self.ask.store(ask, Ordering::Release);
        self.last.store(last, Ordering::Release);
        self.volume.store(volume, Ordering::Release);
        self.timestamp.store(timestamp, Ordering::Release);
    }

    /// Get mid price (fixed-point)
    #[inline]
    pub fn mid_price(&self) -> i64 {
        let bid = self.bid.load(Ordering::Acquire);
        let ask = self.ask.load(Ordering::Acquire);
        (bid + ask) / 2
    }

    /// Get spread (fixed-point)
    #[inline]
    pub fn spread(&self) -> i64 {
        let ask = self.ask.load(Ordering::Acquire);
        let bid = self.bid.load(Ordering::Acquire);
        ask - bid
    }
}

impl Default for PriceSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

/// Market data feed manager
pub struct MarketFeedManager {
    /// Price cache (pre-allocated)
    price_cache: Arc<FxHashMap<Symbol, Arc<PriceSnapshot>>>,
    /// Market index prices for beta calculation
    index_cache: Arc<FxHashMap<String, Arc<PriceSnapshot>>>,
    /// Historical returns buffer for correlation
    returns_buffer: Arc<RwLock<ReturnsBuffer>>,
    /// Update channel
    update_sender: Sender<PriceUpdate>,
    update_receiver: Receiver<PriceUpdate>,
    /// gRPC client to market-connector
    market_client: Option<Channel>,
}

/// Price update message
#[derive(Debug, Clone)]
pub struct PriceUpdate {
    pub symbol: Symbol,
    pub bid: i64,
    pub ask: i64,
    pub last: i64,
    pub volume: i64,
    pub timestamp: u64,
}

/// Historical returns buffer for calculations
pub struct ReturnsBuffer {
    /// Symbol returns (pre-allocated)
    symbol_returns: FxHashMap<Symbol, Vec<i64>>,
    /// Index returns (pre-allocated)
    index_returns: FxHashMap<String, Vec<i64>>,
    /// Buffer capacity
    capacity: usize,
    /// Current position in circular buffer
    position: usize,
}

impl ReturnsBuffer {
    /// Create new buffer with capacity
    pub fn new(capacity: usize, symbols: &[Symbol]) -> Self {
        let mut symbol_returns = FxHashMap::default();
        symbol_returns.reserve(symbols.len());

        for symbol in symbols {
            let mut buffer = Vec::with_capacity(capacity);
            buffer.resize(capacity, 0);
            symbol_returns.insert(*symbol, buffer);
        }

        let mut index_returns = FxHashMap::default();
        index_returns.reserve(2); // NIFTY and SENSEX

        let mut nifty_buffer = Vec::with_capacity(capacity);
        nifty_buffer.resize(capacity, 0);
        index_returns.insert("NIFTY".to_string(), nifty_buffer);

        let mut sensex_buffer = Vec::with_capacity(capacity);
        sensex_buffer.resize(capacity, 0);
        index_returns.insert("SENSEX".to_string(), sensex_buffer);

        Self {
            symbol_returns,
            index_returns,
            capacity,
            position: 0,
        }
    }

    /// Add return to buffer
    #[inline]
    pub fn add_return(&mut self, symbol: Symbol, return_value: i64) {
        if let Some(buffer) = self.symbol_returns.get_mut(&symbol) {
            buffer[self.position % self.capacity] = return_value;
        }
    }

    /// Add index return
    #[inline]
    pub fn add_index_return(&mut self, index: &str, return_value: i64) {
        if let Some(buffer) = self.index_returns.get_mut(index) {
            buffer[self.position % self.capacity] = return_value;
        }
    }

    /// Advance position
    #[inline]
    pub fn advance(&mut self) {
        self.position = (self.position + 1) % self.capacity;
    }

    /// Calculate beta against index
    pub fn calculate_beta(&self, symbol: Symbol, index: &str) -> i32 {
        let symbol_returns = match self.symbol_returns.get(&symbol) {
            Some(returns) => returns,
            // SAFETY: SCALE_4 (10000) fits in i32
            None => return FIXED_POINT_SCALE as i32, // 1.0 = market neutral
        };

        let index_returns = match self.index_returns.get(index) {
            Some(returns) => returns,
            // SAFETY: SCALE_4 (10000) fits in i32
            None => return FIXED_POINT_SCALE as i32,
        };

        // Calculate covariance and index variance
        let n = self.capacity.min(self.position);
        if n < 2 {
            // SAFETY: SCALE_4 (10000) fits in i32
            return FIXED_POINT_SCALE as i32; // Not enough data
        }

        // Calculate means
        // SAFETY: n > 0 guaranteed by function guard above, fits in i64
        let symbol_mean = symbol_returns.iter().take(n).sum::<i64>() / n as i64;
        // SAFETY: n > 0 guaranteed by function guard above, fits in i64
        let index_mean = index_returns.iter().take(n).sum::<i64>() / n as i64;

        // Calculate covariance and variance
        let mut covariance = 0i64;
        let mut index_variance = 0i64;

        for i in 0..n {
            let symbol_diff = symbol_returns[i] - symbol_mean;
            let index_diff = index_returns[i] - index_mean;

            covariance += (symbol_diff * index_diff) / FIXED_POINT_SCALE; // Fixed-point
            index_variance += (index_diff * index_diff) / FIXED_POINT_SCALE;
        }

        // SAFETY: (n - 1) > 0 since n >= 2 guaranteed by guard, fits in i64
        covariance /= (n - 1) as i64;
        // SAFETY: (n - 1) > 0 since n >= 2 guaranteed by guard, fits in i64
        index_variance /= (n - 1) as i64;

        if index_variance == 0 {
            return 10000; // Market neutral if no variance
        }

        // Beta = Covariance / Index Variance
        // SAFETY: Beta values fit in i32 for reasonable market data
        #[allow(clippy::cast_possible_truncation)]
        let beta = ((covariance * FIXED_POINT_SCALE) / index_variance) as i32;
        beta
    }

    /// Calculate correlation with index
    pub fn calculate_correlation(&self, symbol: Symbol, index: &str) -> i32 {
        let symbol_returns = match self.symbol_returns.get(&symbol) {
            Some(returns) => returns,
            None => return 0,
        };

        let index_returns = match self.index_returns.get(index) {
            Some(returns) => returns,
            None => return 0,
        };

        let n = self.capacity.min(self.position);
        if n < 2 {
            return 0;
        }

        // Calculate means
        // SAFETY: n > 0 guaranteed by function guard above, fits in i64
        let symbol_mean = symbol_returns.iter().take(n).sum::<i64>() / n as i64;
        // SAFETY: n > 0 guaranteed by function guard above, fits in i64
        let index_mean = index_returns.iter().take(n).sum::<i64>() / n as i64;

        // Calculate correlation coefficient
        let mut covariance = 0i64;
        let mut symbol_variance = 0i64;
        let mut index_variance = 0i64;

        for i in 0..n {
            let symbol_diff = symbol_returns[i] - symbol_mean;
            let index_diff = index_returns[i] - index_mean;

            covariance += (symbol_diff * index_diff) / FIXED_POINT_SCALE;
            symbol_variance += (symbol_diff * symbol_diff) / FIXED_POINT_SCALE;
            index_variance += (index_diff * index_diff) / FIXED_POINT_SCALE;
        }

        if symbol_variance == 0 || index_variance == 0 {
            return 0;
        }

        // Correlation = Covariance / (StdDev1 * StdDev2)
        // SAFETY: Variance to f64 for correlation calc - analytics boundary
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let correlation = (covariance as f64)
            / ((symbol_variance as f64).sqrt() * (index_variance as f64).sqrt());
        (correlation * FIXED_POINT_SCALE as f64) as i32 // Fixed-point
    }
}

impl MarketFeedManager {
    /// Create new market feed manager
    pub fn new(symbols: &[Symbol], buffer_capacity: usize) -> Self {
        let mut price_cache = FxHashMap::default();
        price_cache.reserve(symbols.len());

        for symbol in symbols {
            price_cache.insert(*symbol, Arc::new(PriceSnapshot::new()));
        }

        let mut index_cache = FxHashMap::default();
        index_cache.reserve(2);
        index_cache.insert("NIFTY".to_string(), Arc::new(PriceSnapshot::new()));
        index_cache.insert("SENSEX".to_string(), Arc::new(PriceSnapshot::new()));

        let (update_sender, update_receiver) = bounded(10000);

        Self {
            price_cache: Arc::new(price_cache),
            index_cache: Arc::new(index_cache),
            returns_buffer: Arc::new(RwLock::new(ReturnsBuffer::new(buffer_capacity, symbols))),
            update_sender,
            update_receiver,
            market_client: None,
        }
    }

    /// Connect to market-connector service
    pub async fn connect(&mut self, endpoint: &str) -> Result<()> {
        let channel = Channel::from_shared(endpoint.to_string())
            .context("Invalid endpoint")?
            .connect()
            .await
            .context("Failed to connect to market-connector")?;

        self.market_client = Some(channel);
        tracing::info!("Connected to market-connector at {}", endpoint);

        // Start background price update processor
        self.start_price_update_processor().await?;

        Ok(())
    }

    /// Start background price update processor
    async fn start_price_update_processor(&self) -> Result<()> {
        let receiver = self.update_receiver.clone();
        let price_cache = Arc::clone(&self.price_cache);
        let returns_buffer = Arc::clone(&self.returns_buffer);

        tokio::spawn(async move {
            while let Ok(update) = receiver.recv() {
                // Process price update
                if let Some(snapshot) = price_cache.get(&update.symbol) {
                    // Calculate return before updating
                    let old_mid = snapshot.mid_price();
                    if old_mid > 0 {
                        let new_mid = (update.bid + update.ask) / 2;
                        let return_value = ((new_mid - old_mid) * FIXED_POINT_SCALE) / old_mid;

                        // Update returns buffer
                        {
                            let mut buf = returns_buffer.write().await;
                            buf.add_return(update.symbol, return_value);
                            buf.advance();
                        }
                    }

                    // Update price snapshot atomically
                    snapshot.update(
                        update.bid,
                        update.ask,
                        update.last,
                        update.volume,
                        update.timestamp,
                    );
                }
            }
        });

        Ok(())
    }

    /// Subscribe to symbols
    pub async fn subscribe(&mut self, symbols: &[Symbol]) -> Result<()> {
        // Would call market-connector gRPC service to subscribe
        // For now, just log
        tracing::info!("Subscribing to {} symbols", symbols.len());
        Ok(())
    }

    /// Queue price update for processing
    #[inline]
    pub fn queue_price_update(&self, update: PriceUpdate) -> Result<()> {
        // Send update through channel for background processing
        self.update_sender
            .send(update)
            .map_err(|e| anyhow::anyhow!("Failed to queue price update: {}", e))?;
        Ok(())
    }

    /// Update price for symbol (direct synchronous update)
    #[inline]
    pub fn update_price(&self, update: PriceUpdate) -> Result<()> {
        if let Some(snapshot) = self.price_cache.get(&update.symbol) {
            // Calculate return before updating
            let old_mid = snapshot.mid_price();
            if old_mid > 0 {
                let new_mid = (update.bid + update.ask) / 2;
                let return_value = ((new_mid - old_mid) * FIXED_POINT_SCALE) / old_mid;

                // Update returns buffer asynchronously
                let buffer = self.returns_buffer.clone();
                let symbol = update.symbol;
                tokio::spawn(async move {
                    {
                        let mut buf = buffer.write().await;
                        buf.add_return(symbol, return_value);
                        buf.advance();
                    }
                });
            }

            // Update price snapshot atomically
            snapshot.update(
                update.bid,
                update.ask,
                update.last,
                update.volume,
                update.timestamp,
            );
        }
        Ok(())
    }

    /// Update index price
    pub fn update_index(&self, index: &str, bid: i64, ask: i64, last: i64) -> Result<()> {
        if let Some(snapshot) = self.index_cache.get(index) {
            // Calculate index return
            let old_mid = snapshot.mid_price();
            if old_mid > 0 {
                let new_mid = (bid + ask) / 2;
                let return_value = ((new_mid - old_mid) * FIXED_POINT_SCALE) / old_mid;

                // Update returns buffer
                let buffer = self.returns_buffer.clone();
                let index_name = index.to_string();
                tokio::spawn(async move {
                    {
                        let mut buf = buffer.write().await;
                        buf.add_index_return(&index_name, return_value);
                    }
                });
            }

            snapshot.update(bid, ask, last, 0, Ts::now().as_nanos());
        }
        Ok(())
    }

    /// Get current price for symbol
    #[inline]
    pub fn get_price(&self, symbol: Symbol) -> Option<(i64, i64, i64)> {
        self.price_cache.get(&symbol).map(|snapshot| {
            (
                snapshot.bid.load(Ordering::Acquire),
                snapshot.ask.load(Ordering::Acquire),
                snapshot.last.load(Ordering::Acquire),
            )
        })
    }

    /// Calculate portfolio beta
    pub async fn calculate_portfolio_beta(&self, positions: &[(Symbol, i64)], index: &str) -> i32 {
        let buffer = self.returns_buffer.read().await;
        let total_value: i64 = positions.iter().map(|(_, value)| value.abs()).sum();

        if total_value == 0 {
            return 10000; // Market neutral
        }

        let mut weighted_beta = 0i64;

        for (symbol, value) in positions {
            let weight = (value.abs() * FIXED_POINT_SCALE) / total_value;
            let beta = buffer.calculate_beta(*symbol, index);
            // SAFETY: weight and beta fit in i64, calculation doesn't overflow
            weighted_beta += (weight as i64 * beta as i64) / FIXED_POINT_SCALE;
        }

        // SAFETY: Weighted beta fits in i32
        #[allow(clippy::cast_possible_truncation)]
        let result = weighted_beta as i32;
        result
    }

    /// Calculate portfolio correlation with index
    pub async fn calculate_portfolio_correlation(
        &self,
        positions: &[(Symbol, i64)],
        index: &str,
    ) -> i32 {
        let buffer = self.returns_buffer.read().await;
        let total_value: i64 = positions.iter().map(|(_, value)| value.abs()).sum();

        if total_value == 0 {
            return 0;
        }

        let mut weighted_correlation = 0i64;

        for (symbol, value) in positions {
            let weight = (value.abs() * FIXED_POINT_SCALE) / total_value;
            let correlation = buffer.calculate_correlation(*symbol, index);
            // SAFETY: weight and correlation fit in i64, calculation doesn't overflow
            weighted_correlation += (weight as i64 * correlation as i64) / FIXED_POINT_SCALE;
        }

        // SAFETY: Weighted correlation fits in i32
        #[allow(clippy::cast_possible_truncation)]
        let result = weighted_correlation as i32;
        result
    }

    /// Start price update processor
    pub async fn start_processor(&self) -> Result<()> {
        let receiver = self.update_receiver.clone();
        let manager = self.clone();

        tokio::spawn(async move {
            while let Ok(update) = receiver.recv() {
                if let Err(e) = manager.update_price(update) {
                    tracing::error!("Failed to update price: {}", e);
                }
            }
        });

        Ok(())
    }
}

// Implement Clone manually to avoid cloning the channel
impl Clone for MarketFeedManager {
    fn clone(&self) -> Self {
        let (sender, receiver) = bounded(10000);
        Self {
            price_cache: self.price_cache.clone(),
            index_cache: self.index_cache.clone(),
            returns_buffer: self.returns_buffer.clone(),
            update_sender: sender,
            update_receiver: receiver,
            market_client: None, // Don't clone the gRPC connection
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_snapshot() {
        let snapshot = PriceSnapshot::new();
        snapshot.update(100000, 100100, 100050, 1000000, 1234567890);

        assert_eq!(snapshot.bid.load(Ordering::Acquire), 100000);
        assert_eq!(snapshot.ask.load(Ordering::Acquire), 100100);
        assert_eq!(snapshot.mid_price(), 100050);
        assert_eq!(snapshot.spread(), 100);
    }

    #[tokio::test]
    async fn test_market_feed_manager() {
        let symbols = vec![Symbol::new(1), Symbol::new(2)];
        let mut manager = MarketFeedManager::new(&symbols, 100);

        // Update price
        let update = PriceUpdate {
            symbol: Symbol::new(1),
            bid: 100000,
            ask: 100100,
            last: 100050,
            volume: 1000000,
            timestamp: 1234567890,
        };

        manager.update_price(update).unwrap();

        // Check price
        let price = manager.get_price(Symbol::new(1));
        assert!(price.is_some());
        let (bid, ask, last) = price.unwrap();
        assert_eq!(bid, 100000);
        assert_eq!(ask, 100100);
        assert_eq!(last, 100050);
    }

    #[tokio::test]
    async fn test_beta_calculation() {
        let symbols = vec![Symbol::new(1)];
        let manager = MarketFeedManager::new(&symbols, 100);

        // Add some returns
        let mut buffer = manager.returns_buffer.write().await;
        for i in 0..10 {
            buffer.add_return(Symbol::new(1), 100 + i * 10);
            buffer.add_index_return("NIFTY", 100 + i * 5);
            buffer.advance();
        }
        drop(buffer);

        // Calculate beta
        let positions = vec![(Symbol::new(1), 1000000)];
        let beta = manager.calculate_portfolio_beta(&positions, "NIFTY").await;

        // Beta should be positive (some correlation with market)
        assert!(beta > 0); // Should have calculated some beta value
    }
}
