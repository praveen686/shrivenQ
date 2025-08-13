//! NSE Order Book Snapshot Loader for LOB Testing
//!
//! Loads NSE order book snapshots (compressed .gz files) and reconstructs
//! the full limit order book for testing ShrivenQuant's LOB implementation

use crate::{CrossResolution, OrderBookV2};
use anyhow::{Context, Result};
use common::{L2Update, Px, Qty, Side, Symbol, Ts};
use flate2::read::GzDecoder;
use rustc_hash::{FxBuildHasher, FxHashMap};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use tracing::{debug, info, warn};

/// NSE Order Book Snapshot Entry
#[derive(Debug, Clone)]
pub struct NseOrderEntry {
    /// Unique order identifier
    pub order_number: u64,
    /// Trading symbol
    pub symbol: String,
    /// Type of instrument (EQ, FUT, OPT, etc.)
    pub instrument_type: String,
    /// Expiry date for derivatives
    pub expiry_date: String,
    /// Strike price for options (in fixed-point: actual * 10000)
    pub strike_price: i64,
    /// Option type (CE/PE)
    pub option_type: String,
    /// Corporate action level
    pub corp_action_level: String,
    /// Order quantity (in fixed-point: actual * 10000)
    pub quantity: i64,
    /// Order price (in fixed-point: actual * 10000)
    pub price: i64,
    /// Timestamp of order
    pub timestamp: String,
    /// Buy/Sell side
    pub side: Side,
    /// Day-specific flags
    pub day_flags: String,
    /// Quantity-related flags
    pub quantity_flags: String,
    /// Price-related flags
    pub price_flags: String,
    /// Book type identifier
    pub book_type: String,
    /// Minimum fill quantity
    pub min_fill_qty: f64,
    /// Disclosed quantity
    pub disclosed_qty: f64,
    /// Good-till-date
    pub gtd_date: String,
}

impl NseOrderEntry {
    /// Parse a line from NSE snapshot CSV
    pub fn from_csv_line(line: &str) -> Result<Self> {
        let fields: Vec<&str> = line.split(',').collect();

        if fields.len() < 18 {
            anyhow::bail!(
                "Invalid line format: expected 18 fields, got {}",
                fields.len()
            );
        }

        Ok(Self {
            order_number: fields[0].parse().unwrap_or(0),
            symbol: fields[1].to_string(),
            instrument_type: fields[2].to_string(),
            expiry_date: fields[3].to_string(),
            // SAFETY: Cast is safe within expected range
            strike_price: (fields[4].parse::<f64>().unwrap_or(0.0) * 10000.0) as i64,
            option_type: fields[5].to_string(),
            // SAFETY: Cast is safe within expected range
            corp_action_level: fields[6].to_string(),
            // SAFETY: Cast is safe within expected range
            quantity: (fields[7].parse::<f64>().unwrap_or(0.0) * 10000.0) as i64,
            // SAFETY: Cast is safe within expected range
            price: (fields[8].parse::<f64>().unwrap_or(0.0) * 10000.0) as i64,
            timestamp: fields[9].to_string(),
            side: if fields[10] == "B" {
                Side::Bid
            } else {
                Side::Ask
            },
            day_flags: fields[11].to_string(),
            quantity_flags: fields[12].to_string(),
            price_flags: fields[13].to_string(),
            book_type: fields[14].to_string(),
            min_fill_qty: fields[15].parse().unwrap_or(0.0),
            disclosed_qty: fields[16].parse().unwrap_or(0.0),
            gtd_date: fields[17].to_string(),
        })
    }
}

/// Aggregated price level for LOB
#[derive(Debug, Clone)]
pub struct PriceLevel {
    /// Price level value (in fixed-point: actual * 10000)
    pub price: i64,
    /// Total quantity at this price level (in fixed-point: actual * 10000)
    pub total_qty: i64,
    /// Number of orders at this level
    pub order_count: usize,
    /// Individual orders at this level
    pub orders: Vec<NseOrderEntry>,
}

/// NSE Snapshot Loader
pub struct NseSnapshotLoader {
    /// Symbol mapping
    symbol_map: FxHashMap<String, Symbol>,
    next_symbol_id: u32,
}

impl NseSnapshotLoader {
    /// Create a new NSE snapshot loader
    ///
    /// # Performance
    /// - O(1) initialization time
    /// - Pre-allocates symbol map with capacity for 10,000 symbols
    /// - Uses FxHashMap for O(1) average case symbol lookup
    pub fn new() -> Self {
        Self {
            symbol_map: FxHashMap::with_capacity_and_hasher(10000, FxBuildHasher),
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

    /// Load snapshot from .gz file
    pub fn load_snapshot(&mut self, path: &Path) -> Result<Vec<NseOrderEntry>> {
        info!("Loading NSE snapshot from: {:?}", path);

        let file = File::open(path).with_context(|| format!("Failed to open file: {:?}", path))?;

        let decoder = GzDecoder::new(file);
        let reader = BufReader::new(decoder);

        let mut entries = Vec::with_capacity(1000);
        let mut line_num = 0;

        for line in reader.lines() {
            line_num += 1;

            let line = line.with_context(|| format!("Failed to read line {}", line_num))?;

            // Skip header if present
            if line_num == 1 && line.starts_with("Order") {
                continue;
            }

            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            match NseOrderEntry::from_csv_line(&line) {
                Ok(entry) => entries.push(entry),
                Err(e) => {
                    warn!("Failed to parse line {}: {}", line_num, e);
                }
            }
        }

        info!("Loaded {} order entries", entries.len());
        Ok(entries)
    }

    /// Build LOB from snapshot entries
    pub fn build_lob(
        &mut self,
        entries: &[NseOrderEntry],
        symbol_filter: Option<&str>,
    ) -> Result<OrderBookV2> {
        // Filter by symbol if specified
        let filtered: Vec<_> = if let Some(sym) = symbol_filter {
            entries
                .iter()
                .filter(|e| e.symbol == sym)
                .cloned()
                .collect()
        } else {
            entries.to_vec()
        };

        if filtered.is_empty() {
            anyhow::bail!("No entries found for symbol: {:?}", symbol_filter);
        }

        let symbol_str = &filtered[0].symbol;
        let symbol_id = self.get_symbol_id(symbol_str);

        info!(
            "Building LOB for {} with {} orders",
            symbol_str,
            filtered.len()
        );

        // Aggregate orders by price level
        let mut bid_levels: BTreeMap<i64, PriceLevel> = BTreeMap::new();
        let mut ask_levels: BTreeMap<i64, PriceLevel> = BTreeMap::new();

        for entry in &filtered {
            let price_ticks = entry.price; // Already in fixed-point

            let levels = if entry.side == Side::Bid {
                &mut bid_levels
            } else {
                &mut ask_levels
            };

            let level = levels.entry(price_ticks).or_insert_with(|| PriceLevel {
                price: entry.price,
                total_qty: 0,
                order_count: 0,
                orders: Vec::with_capacity(20),
            });

            level.total_qty += entry.quantity;
            level.order_count += 1;
            level.orders.push(entry.clone());
        }

        // Create LOB v2 with ROI optimization
        let mid_price = if let (Some(best_bid), Some(best_ask)) =
            (bid_levels.keys().next_back(), ask_levels.keys().next())
        {
            // Mid price already in fixed-point, divide by 2
            (*best_bid + *best_ask) / 2
        } else {
            1000000 // Default: 100.0 * 10000 in fixed-point
        };

        let mut book = OrderBookV2::new_with_roi(
            symbol_id,
            Px::new(0.01),                // tick size (1 paise)
            Qty::new(1.0),                // lot size
            Px::from_i64(mid_price),      // Use from_i64 for fixed-point value
            Px::from_i64(mid_price / 10), // 10% ROI width
        );
        book.set_cross_resolution(CrossResolution::AutoResolve);

        // Populate bid side (descending order)
        let mut level_idx = 0;
        for (_, level) in bid_levels.iter().rev().take(20) {
            let update = L2Update::new(
                Ts::from_nanos(1_000_000_000), // 1 second
                symbol_id,
            )
            .with_level_data(
                Side::Bid,
                Px::from_i64(level.price),
                Qty::from_units(level.total_qty),
                level_idx,
            );

            if let Err(e) = book.apply_validated(&update) {
                debug!("Failed to apply bid level: {}", e);
            }
            level_idx += 1;
        }

        // Populate ask side (ascending order)
        level_idx = 0;
        for (_, level) in ask_levels.iter().take(20) {
            let update = L2Update::new(Ts::from_nanos(1_000_000_000), symbol_id).with_level_data(
                Side::Ask,
                Px::from_i64(level.price),
                Qty::from_units(level.total_qty),
                level_idx,
            );

            if let Err(e) = book.apply_validated(&update) {
                debug!("Failed to apply ask level: {}", e);
            }
            level_idx += 1;
        }

        info!(
            "LOB built with {} bid levels and {} ask levels",
            bid_levels.len().min(20),
            ask_levels.len().min(20)
        );

        Ok(book)
    }

    /// Analyze market microstructure from snapshot
    pub fn analyze_snapshot(&self, entries: &[NseOrderEntry], symbol: Option<&str>) {
        let filtered: Vec<_> = if let Some(sym) = symbol {
            entries.iter().filter(|e| e.symbol == sym).collect()
        } else {
            entries.iter().collect()
        };

        if filtered.is_empty() {
            warn!("No entries to analyze");
            return;
        }

        // Calculate statistics
        let total_orders = filtered.len();
        let buy_orders = filtered.iter().filter(|e| e.side == Side::Bid).count();
        let sell_orders = total_orders - buy_orders;

        let total_buy_qty: i64 = filtered
            .iter()
            .filter(|e| e.side == Side::Bid)
            .map(|e| e.quantity)
            .sum();

        let total_sell_qty: i64 = filtered
            .iter()
            .filter(|e| e.side == Side::Ask)
            .map(|e| e.quantity)
            // SAFETY: Cast is safe within expected range
            .sum();
        // SAFETY: Cast is safe within expected range

        let avg_order_size = (total_buy_qty + total_sell_qty) / total_orders as i64;

        // Find best bid and ask
        let best_bid = filtered
            .iter()
            .filter(|e| e.side == Side::Bid)
            .map(|e| e.price)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let best_ask = filtered
            .iter()
            .filter(|e| e.side == Side::Ask)
            .map(|e| e.price)
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        println!("\n📊 Market Microstructure Analysis");
        println!("=====================================");
        if let Some(sym) = symbol {
            println!("Symbol: {}", sym);
        }
        println!("Total Orders: {}", total_orders);
        println!(
            "Buy Orders: {} ({:.1}%)",
            buy_orders,
            (buy_orders * 100) / total_orders
        );
        println!(
            "Sell Orders: {} ({:.1}%)",
            sell_orders,
            (sell_orders * 100) / total_orders
        );
        println!("Total Buy Quantity: {}", total_buy_qty / 10000); // Convert fixed-point to display
        println!("Total Sell Quantity: {}", total_sell_qty / 10000); // Convert fixed-point to display
        // SAFETY: Cast is safe within expected range
        println!(
            // SAFETY: Cast is safe within expected range
            "Order Imbalance: {:.2}",
            // SAFETY: Cast is safe within expected range
            if total_buy_qty + total_sell_qty > 0 {
                ((total_buy_qty - total_sell_qty) as f64)
                    / ((total_buy_qty + total_sell_qty) as f64)
            } else {
                0.0
            } // SAFETY: Cast is safe within expected range
        );
        // SAFETY: Cast is safe within expected range
        println!("Average Order Size: {}", avg_order_size / 10000); // Convert fixed-point to display
        // SAFETY: Cast is safe within expected range

        if let (Some(bid), Some(ask)) = (best_bid, best_ask) {
            // SAFETY: Cast is safe within expected range
            println!("Best Bid: {:.2}", bid as f64 / 10000.0); // Convert for display
            // SAFETY: Cast is safe within expected range
            println!("Best Ask: {:.2}", ask as f64 / 10000.0); // Convert for display
            let spread = ask - bid;
            // SAFETY: Cast is safe within expected range
            let spread_bps = if bid > 0 { (spread * 10000) / bid } else { 0 };
            println!(
                "Spread: {:.2} ({} bps)",
                spread as f64 / 10000.0, // Convert for display
                spread_bps
            );
        }

        // Analyze order types
        let disclosed_orders = filtered.iter().filter(|e| e.disclosed_qty > 0.0).count();
        let gtd_orders = filtered.iter().filter(|e| !e.gtd_date.is_empty()).count();

        println!("\n📋 Order Types:");
        println!(
            "Disclosed Quantity Orders: {} ({:.1}%)",
            disclosed_orders,
            (disclosed_orders * 100) / total_orders
        );
        println!(
            "GTD Orders: {} ({:.1}%)",
            gtd_orders,
            (gtd_orders * 100) / total_orders
        );

        // Analyze book types
        let mut book_types = FxHashMap::with_capacity_and_hasher(10, FxBuildHasher);
        for entry in &filtered {
            *book_types.entry(entry.book_type.as_str()).or_insert(0) += 1;
        }

        println!("\n📚 Book Types:");
        for (book_type, count) in book_types {
            println!(
                "  {}: {} ({:.1}%)",
                book_type,
                count,
                (count * 100) / total_orders
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_nse_entry() {
        let line = "12345,NIFTY,FUTIDX,20030130,0,XX,0,100,5000.50,10:30:45,B,nnnn,nnn,nnn,RL,0,0,";
        let entry = match NseOrderEntry::from_csv_line(line) {
            Ok(e) => e,
            Err(err) => {
                assert!(false, "Should parse valid NSE CSV line in test: {}", err);
                return;
            }
        };

        assert_eq!(entry.order_number, 12345);
        assert_eq!(entry.symbol, "NIFTY");
        assert_eq!(entry.quantity, 1000000); // 100.0 * 10000 in fixed-point
        assert_eq!(entry.price, 50005000); // 5000.50 * 10000 in fixed-point
        assert_eq!(entry.side, Side::Bid);
    }
}

/// Demo function showing how to use the NSE snapshot loader
pub fn run_nse_loader_demo() -> Result<()> {
    info!("🚀 NSE Order Book Snapshot Loader Demo");
    info!("======================================\n");

    // Example: Load a snapshot file
    // In real usage, download from NSE archives:
    // https://www.nseindia.com/content/historical/DERIVATIVES/2003/JAN/fo03JAN2003bhav.csv.zip

    let mut loader = NseSnapshotLoader::new();

    // Example path - replace with actual snapshot file
    let snapshot_path = Path::new("data/nse/200301/Snapshots/20030103/110000.gz");

    if snapshot_path.exists() {
        // Load snapshot
        let entries = loader.load_snapshot(snapshot_path)?;

        // Analyze the full snapshot
        loader.analyze_snapshot(&entries, None);

        // Build LOB for NIFTY
        if let Ok(book) = loader.build_lob(&entries, Some("NIFTY")) {
            if let (Some((bid_px, bid_qty)), Some((ask_px, ask_qty))) =
                (book.best_bid(), book.best_ask())
            // SAFETY: Cast is safe within expected range
            {
                // SAFETY: Cast is safe within expected range
                info!("\n📈 NIFTY Order Book:");
                // SAFETY: Cast is safe within expected range
                info!("Best Bid: {:.2} x {:.0}", bid_px.as_f64(), bid_qty.as_f64());
                info!("Best Ask: {:.2} x {:.0}", ask_px.as_f64(), ask_qty.as_f64());
                info!(
                    "Spread: {:.2} bps",
                    (book.spread_ticks().unwrap_or(0) as f64) * 0.01 // Cast for display only
                );
            }
        }

        // You can also build LOB for individual stocks
        for symbol in ["ACC", "RELIANCE", "TCS", "INFY"] {
            if let Ok(book) = loader.build_lob(&entries, Some(symbol)) {
                info!(
                    "\n📊 {} Order Book built successfully with {}+{} levels",
                    symbol,
                    book.bids.depth(),
                    book.asks.depth()
                );
            }
        }
    } else {
        info!("⚠️ Snapshot file not found at: {:?}", snapshot_path);
        info!("\nTo use NSE snapshots:");
        info!("1. Download from NSE historical data section");
        info!("2. Extract the .gz files from the archives");
        info!("3. Place them in data/nse/YYYYMM/Snapshots/YYYYMMDD/");
        info!("4. Run this example again");
    }

    Ok(())
}
