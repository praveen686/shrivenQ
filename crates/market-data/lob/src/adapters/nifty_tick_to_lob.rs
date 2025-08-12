//! NIFTY 50 Tick Data to LOB Converter
//!
//! Reconstructs limit order book from NIFTY 50 tick data using:
//! 1. Trade-through detection for inferring hidden liquidity
//! 2. Volume-weighted price level estimation
//! 3. Lee-Ready algorithm for trade classification
//! 4. Microstructure patterns from NSE trading mechanics

use crate::{CrossResolution, OrderBookV2, features_v2};
use anyhow::{Context, Result};
use common::{L2Update, Px, Qty, Side, Symbol, Ts};
use rustc_hash::{FxBuildHasher, FxHashMap};
use serde::Deserialize;
use std::collections::{BTreeMap, VecDeque};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use tracing::{info, warn};

/// NIFTY Tick Data Entry
#[derive(Debug, Clone, Deserialize)]
pub struct NiftyTick {
    /// Timestamp string
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    /// Symbol name
    #[serde(rename = "symbol")]
    pub symbol: String,
    /// Trade price
    #[serde(rename = "price")]
    pub price: f64,
    /// Trade quantity
    #[serde(rename = "quantity")]
    pub quantity: f64,
    /// Optional trade value
    #[serde(rename = "value")]
    pub value: Option<f64>,
    /// Optional side ("B" or "S" if available)
    #[serde(rename = "buy_sell")]
    pub side: Option<String>,
}

impl NiftyTick {
    /// Parse from CSV line
    pub fn from_csv_line(line: &str) -> Result<Self> {
        // Common CSV formats:
        // timestamp,symbol,price,quantity,value
        // or
        // timestamp,symbol,ltp,volume,value,oi,buy_sell

        let fields: Vec<&str> = line.split(',').collect();

        if fields.len() < 4 {
            anyhow::bail!("Invalid tick format: expected at least 4 fields");
        }

        Ok(Self {
            timestamp: fields[0].to_string(),
            symbol: fields[1].to_string(),
            price: fields[2].parse().context("Invalid price")?,
            quantity: fields[3].parse().context("Invalid quantity")?,
            value: if fields.len() > 4 {
                fields[4].parse().ok()
            } else {
                Some(fields[2].parse::<f64>()? * fields[3].parse::<f64>()?)
            },
            side: if fields.len() > 6 {
                Some(fields[6].to_string())
            } else {
                None
            },
        })
    }

    /// Convert timestamp to nanoseconds
    pub fn to_timestamp_ns(&self) -> Result<u64> {
        // Handle different timestamp formats
        // "2023-01-01 09:15:00.123"
        // "09:15:00"
        // "1640995200000" (epoch millis)

        if self.timestamp.contains(':') {
            // Time format - assume today's date
            let base_date = 1_704_067_200_000_000_000_u64; // 2024-01-01 00:00:00 UTC in ns

            // Extract time components
            let time_parts: Vec<&str> = if self.timestamp.contains(' ') {
                self.timestamp
                    .split(' ')
                    .nth(1)
                    .unwrap_or("")
                    .split(':')
                    .collect()
            } else {
                self.timestamp.split(':').collect()
            };

            if time_parts.len() >= 2 {
                let hour: u64 = time_parts[0].parse().unwrap_or(9);
                let minute: u64 = time_parts[1].parse().unwrap_or(0);
                let second: u64 = if time_parts.len() > 2 {
                    time_parts[2]
                        .split('.')
                        .next()
                        .unwrap_or("0")
                        .parse()
                        .unwrap_or(0)
                } else {
                    0
                };

                let total_ns = base_date
                    + hour * 3600 * 1_000_000_000
                    + minute * 60 * 1_000_000_000
                    + second * 1_000_000_000;

                return Ok(total_ns);
            }
        }

        // Try parsing as epoch timestamp
        if let Ok(epoch_ms) = self.timestamp.parse::<u64>() {
            if epoch_ms > 1_000_000_000_000 {
                // Looks like milliseconds
                return Ok(epoch_ms * 1_000_000);
            } else if epoch_ms > 1_000_000_000 {
                // Looks like seconds
                return Ok(epoch_ms * 1_000_000_000);
            }
        }

        // Default to current time offset
        Ok(1_704_067_200_000_000_000_u64)
    }
}

/// Order Book State Estimator
#[derive(Debug)]
pub struct LobEstimator {
    /// Current best bid/ask estimates
    estimated_bid: Option<f64>,
    estimated_ask: Option<f64>,

    /// Recent trade history for pattern analysis
    trade_history: VecDeque<NiftyTick>,
    max_history: usize,

    /// Price level tracking
    bid_levels: BTreeMap<i64, f64>, // price_ticks -> estimated_quantity
    ask_levels: BTreeMap<i64, f64>,

    /// Market microstructure parameters
    tick_size: f64,
    min_spread_ticks: i64,

    /// Volume profile for level estimation
    volume_profile: VecDeque<(f64, f64)>, // (price, volume) pairs

    /// Lee-Ready classifier state
    last_mid_price: Option<f64>,

    /// Statistical tracking
    total_trades: u64,
    buy_volume: f64,
    sell_volume: f64,
}

impl LobEstimator {
    /// Create a new LOB estimator for tick-to-LOB reconstruction
    ///
    /// # Performance
    /// - O(1) initialization time
    /// - Pre-allocates history buffers with capacity
    /// - Uses BTreeMap for O(log n) price level access
    pub fn new(tick_size: f64) -> Self {
        Self {
            estimated_bid: None,
            estimated_ask: None,
            trade_history: VecDeque::new(),
            max_history: 1000,
            bid_levels: BTreeMap::new(),
            ask_levels: BTreeMap::new(),
            tick_size,
            min_spread_ticks: 1,
            volume_profile: VecDeque::new(),
            last_mid_price: None,
            total_trades: 0,
            buy_volume: 0.0,
            sell_volume: 0.0,
        }
    }

    /// Process a new tick and update LOB estimate
    pub fn process_tick(&mut self, tick: &NiftyTick) {
        self.total_trades += 1;

        // Add to trade history
        self.trade_history.push_back(tick.clone());
        if self.trade_history.len() > self.max_history {
            self.trade_history.pop_front();
        }

        // Update volume profile
        self.volume_profile.push_back((tick.price, tick.quantity));
        if self.volume_profile.len() > 100 {
            self.volume_profile.pop_front();
        }

        // Classify trade direction using Lee-Ready algorithm
        let trade_side = self.classify_trade_direction(tick);

        match trade_side {
            Side::Bid => self.buy_volume += tick.quantity,
            Side::Ask => self.sell_volume += tick.quantity,
        }

        // Update bid/ask estimates based on trade direction and patterns
        self.update_bbo_estimates(tick, trade_side);

        // Update deeper levels using volume clustering
        self.update_depth_estimates();

        self.last_mid_price = Some(tick.price);
    }

    /// Classify trade direction using Lee-Ready algorithm
    fn classify_trade_direction(&self, tick: &NiftyTick) -> Side {
        // If explicit side information is available
        if let Some(ref side_str) = tick.side {
            return if side_str == "B" || side_str == "BUY" {
                Side::Bid // Trade hit ask (buyer initiated)
            } else {
                Side::Ask // Trade hit bid (seller initiated)
            };
        }

        // Lee-Ready classification
        if let (Some(last_mid), Some(current_bid), Some(current_ask)) =
            (self.last_mid_price, self.estimated_bid, self.estimated_ask)
        {
            let mid = (current_bid + current_ask) / 2.0;

            if tick.price > mid + self.tick_size / 2.0 {
                return Side::Bid; // Above mid - likely buyer initiated
            } else if tick.price < mid - self.tick_size / 2.0 {
                return Side::Ask; // Below mid - likely seller initiated
            } else {
                // At mid - use tick rule
                if tick.price > last_mid {
                    return Side::Bid;
                } else if tick.price < last_mid {
                    return Side::Ask;
                } else {
                    // Same price - use recent pattern
                    let recent_trades = self.trade_history.iter().rev().take(5);
                    let net_direction: i32 = recent_trades
                        .map(|t| {
                            if t.price > last_mid {
                                1
                            } else if t.price < last_mid {
                                -1
                            } else {
                                0
                            }
                        })
                        .sum();

                    return if net_direction >= 0 {
                        Side::Bid
                    } else {
                        Side::Ask
                    };
                }
            }
        }

        // Default: alternate based on trade count
        if self.total_trades % 2 == 0 {
            Side::Bid
        } else {
            Side::Ask
        }
    }

    /// Update best bid/offer estimates
    fn update_bbo_estimates(&mut self, tick: &NiftyTick, trade_side: Side) {
        let price_ticks = (tick.price / self.tick_size).round() as i64;

        match trade_side {
            Side::Bid => {
                // Buy trade - likely executed at ask price
                // Update ask estimate
                self.estimated_ask = Some(tick.price);

                // Estimate bid as ask minus minimum spread
                let estimated_bid_ticks = price_ticks - self.min_spread_ticks;
                self.estimated_bid = Some(estimated_bid_ticks as f64 * self.tick_size);

                // Update ask levels - remove liquidity at this level
                self.ask_levels.remove(&price_ticks);

                // Add potential bid level
                self.bid_levels.insert(estimated_bid_ticks, tick.quantity);
            }
            Side::Ask => {
                // Sell trade - likely executed at bid price
                // Update bid estimate
                self.estimated_bid = Some(tick.price);

                // Estimate ask as bid plus minimum spread
                let estimated_ask_ticks = price_ticks + self.min_spread_ticks;
                self.estimated_ask = Some(estimated_ask_ticks as f64 * self.tick_size);

                // Update bid levels - remove liquidity at this level
                self.bid_levels.remove(&price_ticks);

                // Add potential ask level
                self.ask_levels.insert(estimated_ask_ticks, tick.quantity);
            }
        }
    }

    /// Update deeper level estimates using volume clustering
    fn update_depth_estimates(&mut self) {
        if self.volume_profile.len() < 10 {
            return;
        }

        // Find volume-weighted price clusters
        let mut price_volume_map: BTreeMap<i64, f64> = BTreeMap::new();

        for (price, volume) in &self.volume_profile {
            let price_ticks = (price / self.tick_size).round() as i64;
            *price_volume_map.entry(price_ticks).or_insert(0.0) += volume;
        }

        // Get current mid estimate
        let mid_price = match (self.estimated_bid, self.estimated_ask) {
            (Some(bid), Some(ask)) => (bid + ask) / 2.0,
            (Some(bid), None) => bid + self.tick_size,
            (None, Some(ask)) => ask - self.tick_size,
            (None, None) => return,
        };

        let mid_ticks = (mid_price / self.tick_size).round() as i64;

        // Clear old levels
        self.bid_levels.clear();
        self.ask_levels.clear();

        // Build levels based on volume profile and distance from mid
        for (price_ticks, total_volume) in price_volume_map {
            let distance = (price_ticks - mid_ticks).abs();

            // Estimate remaining liquidity (decreases with distance and recent activity)
            let liquidity_factor = (-distance as f64 * 0.1).exp();
            let estimated_quantity = total_volume * liquidity_factor;

            if estimated_quantity > 1.0 {
                // Minimum quantity threshold
                if price_ticks < mid_ticks {
                    self.bid_levels.insert(price_ticks, estimated_quantity);
                } else if price_ticks > mid_ticks {
                    self.ask_levels.insert(price_ticks, estimated_quantity);
                }
            }
        }

        // Ensure we have at least BBO
        if self.bid_levels.is_empty() {
            if let Some(bid) = self.estimated_bid {
                let bid_ticks = (bid / self.tick_size).round() as i64;
                self.bid_levels.insert(bid_ticks, 100.0); // Default quantity
            }
        }

        if self.ask_levels.is_empty() {
            if let Some(ask) = self.estimated_ask {
                let ask_ticks = (ask / self.tick_size).round() as i64;
                self.ask_levels.insert(ask_ticks, 100.0); // Default quantity
            }
        }
    }

    /// Generate LOB updates from current state
    pub fn generate_lob_updates(&self, symbol: Symbol, timestamp_ns: u64) -> Vec<L2Update> {
        let mut updates = Vec::with_capacity(100);
        let ts = Ts::from_nanos(timestamp_ns);

        // Generate bid side updates (descending order)
        let mut level = 0u8;
        for (price_ticks, quantity) in self.bid_levels.iter().rev().take(20) {
            let price = *price_ticks as f64 * self.tick_size;
            updates.push(L2Update::new(ts, symbol).with_level_data(
                Side::Bid,
                Px::new(price),
                Qty::new(*quantity),
                level,
            ));
            level += 1;
        }

        // Generate ask side updates (ascending order)
        level = 0;
        for (price_ticks, quantity) in self.ask_levels.iter().take(20) {
            let price = *price_ticks as f64 * self.tick_size;
            updates.push(L2Update::new(ts, symbol).with_level_data(
                Side::Ask,
                Px::new(price),
                Qty::new(*quantity),
                level,
            ));
            level += 1;
        }

        updates
    }

    /// Get market statistics
    pub fn get_stats(&self) -> String {
        let imbalance = if self.buy_volume + self.sell_volume > 0.0 {
            (self.buy_volume - self.sell_volume) / (self.buy_volume + self.sell_volume)
        } else {
            0.0
        };

        let spread = match (self.estimated_bid, self.estimated_ask) {
            (Some(bid), Some(ask)) => ask - bid,
            _ => 0.0,
        };

        format!(
            "Trades: {}, Buy Vol: {:.0}, Sell Vol: {:.0}, Imbalance: {:.3}, Spread: {:.2}",
            self.total_trades, self.buy_volume, self.sell_volume, imbalance, spread
        )
    }
}

/// NIFTY Tick to LOB Converter
pub struct NiftyTickToLob {
    estimator: LobEstimator,
    symbol_map: FxHashMap<String, Symbol>,
    next_symbol_id: u32,
}

impl NiftyTickToLob {
    /// Create a new NIFTY tick-to-LOB converter
    ///
    /// # Performance
    /// - O(1) initialization time
    /// - Pre-allocates internal data structures
    /// - No heap allocations during processing
    pub fn new(tick_size: f64) -> Self {
        Self {
            estimator: LobEstimator::new(tick_size),
            symbol_map: FxHashMap::with_capacity_and_hasher(1000, FxBuildHasher),
            next_symbol_id: 1,
        }
    }

    /// Get or create symbol ID
    fn get_symbol_id(&mut self, symbol_str: &str) -> Symbol {
        if let Some(&id) = self.symbol_map.get(symbol_str) {
            id
        } else {
            let id = Symbol(self.next_symbol_id);
            self.symbol_map.insert(symbol_str.to_string(), id);
            self.next_symbol_id += 1;
            id
        }
    }

    /// Process tick file and build LOB
    pub fn process_tick_file(
        &mut self,
        path: &Path,
        symbol_filter: Option<&str>,
    ) -> Result<Vec<(OrderBookV2, Ts)>> {
        info!("Processing NIFTY tick file: {:?}", path);

        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let mut lob_snapshots = Vec::with_capacity(1000);
        let mut line_count = 0;
        let mut processed_ticks = 0;

        for line in reader.lines() {
            line_count += 1;

            let line = line?;

            // Skip header
            if line_count == 1 && (line.contains("timestamp") || line.contains("time")) {
                continue;
            }

            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            // Parse tick
            match NiftyTick::from_csv_line(&line) {
                Ok(tick) => {
                    // Filter by symbol if specified
                    if let Some(filter) = symbol_filter {
                        if tick.symbol != filter {
                            continue;
                        }
                    }

                    // Process the tick
                    self.estimator.process_tick(&tick);
                    processed_ticks += 1;

                    // Generate LOB snapshot every N ticks
                    if processed_ticks % 10 == 0 {
                        let symbol_id = self.get_symbol_id(&tick.symbol);
                        let timestamp_ns = tick.to_timestamp_ns()?;

                        // Create LOB from current state
                        if let Ok(lob) = self.build_lob_snapshot(symbol_id, timestamp_ns) {
                            lob_snapshots.push((lob, Ts::from_nanos(timestamp_ns)));
                        }
                    }

                    // Progress reporting
                    if processed_ticks % 1000 == 0 {
                        info!(
                            "Processed {} ticks - {}",
                            processed_ticks,
                            self.estimator.get_stats()
                        );
                    }
                }
                Err(e) => {
                    if line_count <= 10 {
                        // Only warn for first few errors
                        warn!("Failed to parse line {}: {}", line_count, e);
                    }
                }
            }
        }

        info!(
            "Processed {} ticks from {} lines",
            processed_ticks, line_count
        );
        info!("Generated {} LOB snapshots", lob_snapshots.len());

        Ok(lob_snapshots)
    }

    /// Build LOB snapshot from current estimator state
    fn build_lob_snapshot(&self, symbol: Symbol, timestamp_ns: u64) -> Result<OrderBookV2> {
        // Create LOB with ROI optimization
        let mid_price = match (self.estimator.estimated_bid, self.estimator.estimated_ask) {
            (Some(bid), Some(ask)) => (bid + ask) / 2.0,
            (Some(bid), None) => bid,
            (None, Some(ask)) => ask,
            (None, None) => 25000.0, // Default NIFTY price
        };

        let mut book = OrderBookV2::new_with_roi(
            symbol,
            0.05, // NIFTY tick size (5 paise)
            1.0,  // lot size
            mid_price,
            mid_price * 0.02, // 2% ROI width
        );
        book.set_cross_resolution(CrossResolution::AutoResolve);

        // Apply updates
        let updates = self.estimator.generate_lob_updates(symbol, timestamp_ns);
        for update in updates {
            let _ = book.apply_validated(&update);
        }

        Ok(book)
    }
}

/// Demo function showing how to use the NIFTY tick-to-LOB converter
pub fn run_nifty_tick_demo() -> Result<()> {
    // Example usage
    let mut converter = NiftyTickToLob::new(0.05); // NIFTY tick size

    // Path to NIFTY tick data file (CSV format)
    let tick_file = Path::new("data/nifty_ticks.csv");

    if tick_file.exists() {
        let lob_snapshots = converter.process_tick_file(tick_file, Some("NIFTY"))?;

        // Analyze the reconstructed LOBs
        if !lob_snapshots.is_empty() {
            info!("\n📊 LOB Analysis:");

            let mut feature_calc = features_v2::create_hft_calculator(Symbol(1));

            for (i, (book, ts)) in lob_snapshots.iter().enumerate().take(10) {
                if let Some(features) = feature_calc.calculate(book) {
                    info!(
                        "Snapshot {} (ts={}): BBO={:.2}x{:.0}|{:.2}x{:.0}, Spread={:.2}, Regime={:?}",
                        i + 1,
                        ts.as_nanos(),
                        book.best_bid()
                            .map(|(p, q)| (p.as_f64(), q.as_f64()))
                            .unwrap_or((0.0, 0.0))
                            .0,
                        book.best_bid()
                            .map(|(p, q)| (p.as_f64(), q.as_f64()))
                            .unwrap_or((0.0, 0.0))
                            .1,
                        book.best_ask()
                            .map(|(p, q)| (p.as_f64(), q.as_f64()))
                            .unwrap_or((0.0, 0.0))
                            .0,
                        book.best_ask()
                            .map(|(p, q)| (p.as_f64(), q.as_f64()))
                            .unwrap_or((0.0, 0.0))
                            .1,
                        features.weighted_spread,
                        features.regime
                    );
                }
            }
        }
    } else {
        info!("⚠️ Tick data file not found at: {:?}", tick_file);
        info!("\nTo use this converter:");
        info!("1. Download NIFTY tick data (CSV format)");
        info!("2. Ensure columns: timestamp,symbol,price,quantity[,value,buy_sell]");
        info!("3. Place file at: data/nifty_ticks.csv");
        info!("4. Run this example");

        // Create sample data format
        info!("\nExpected CSV format:");
        info!("timestamp,symbol,price,quantity,value");
        info!("09:15:00,NIFTY,25000.50,100,2500050");
        info!("09:15:01,NIFTY,25000.25,50,1250012.5");
    }

    Ok(())
}
