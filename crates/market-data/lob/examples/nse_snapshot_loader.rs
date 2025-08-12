//! NSE Order Book Snapshot Loader for LOB Testing
//!
//! Loads NSE order book snapshots (compressed .gz files) and reconstructs
//! the full limit order book for testing ShrivenQuant's LOB implementation

use anyhow::{Context, Result};
use common::{L2Update, Px, Qty, Side, Symbol, Ts};
use flate2::read::GzDecoder;
use lob::{CrossResolution, OrderBookV2};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use tracing::{debug, info, warn};

/// NSE Order Book Snapshot Entry
#[derive(Debug, Clone)]
pub struct NseOrderEntry {
    pub order_number: u64,
    pub symbol: String,
    pub instrument_type: String,
    pub expiry_date: String,
    pub strike_price: f64,
    pub option_type: String,
    pub corp_action_level: String,
    pub quantity: f64,
    pub price: f64,
    pub timestamp: String,
    pub side: Side,
    pub day_flags: String,
    pub quantity_flags: String,
    pub price_flags: String,
    pub book_type: String,
    pub min_fill_qty: f64,
    pub disclosed_qty: f64,
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
            strike_price: fields[4].parse().unwrap_or(0.0),
            option_type: fields[5].to_string(),
            corp_action_level: fields[6].to_string(),
            quantity: fields[7].parse().unwrap_or(0.0),
            price: fields[8].parse().unwrap_or(0.0),
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
    pub price: f64,
    pub total_qty: f64,
    pub order_count: usize,
    pub orders: Vec<NseOrderEntry>,
}

/// NSE Snapshot Loader
pub struct NseSnapshotLoader {
    /// Symbol mapping
    symbol_map: std::collections::HashMap<String, Symbol>,
    next_symbol_id: u32,
}

impl NseSnapshotLoader {
    pub fn new() -> Self {
        Self {
            symbol_map: std::collections::HashMap::new(),
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

        let mut entries = Vec::new();
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
            let price_ticks = (entry.price * 100.0) as i64; // Convert to ticks (paise)

            let levels = if entry.side == Side::Bid {
                &mut bid_levels
            } else {
                &mut ask_levels
            };

            levels
                .entry(price_ticks)
                .or_insert_with(|| PriceLevel {
                    price: entry.price,
                    total_qty: 0.0,
                    order_count: 0,
                    orders: Vec::new(),
                })
                .total_qty += entry.quantity;

            levels.get_mut(&price_ticks).unwrap().order_count += 1;
            levels
                .get_mut(&price_ticks)
                .unwrap()
                .orders
                .push(entry.clone());
        }

        // Create LOB v2 with ROI optimization
        let mid_price = if let (Some(best_bid), Some(best_ask)) =
            (bid_levels.keys().next_back(), ask_levels.keys().next())
        {
            (*best_bid + *best_ask) as f64 / 200.0
        } else {
            100.0 // Default
        };

        let mut book = OrderBookV2::new_with_roi(
            symbol_id,
            0.01, // tick size (1 paise)
            1.0,  // lot size
            mid_price,
            mid_price * 0.1, // 10% ROI width
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
                Px::new(level.price),
                Qty::new(level.total_qty),
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
                Px::new(level.price),
                Qty::new(level.total_qty),
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

        let total_buy_qty: f64 = filtered
            .iter()
            .filter(|e| e.side == Side::Bid)
            .map(|e| e.quantity)
            .sum();

        let total_sell_qty: f64 = filtered
            .iter()
            .filter(|e| e.side == Side::Ask)
            .map(|e| e.quantity)
            .sum();

        let avg_order_size = (total_buy_qty + total_sell_qty) / total_orders as f64;

        // Find best bid and ask
        let best_bid = filtered
            .iter()
            .filter(|e| e.side == Side::Bid)
            .map(|e| e.price)
            .max_by(|a, b| a.partial_cmp(b).unwrap());

        let best_ask = filtered
            .iter()
            .filter(|e| e.side == Side::Ask)
            .map(|e| e.price)
            .min_by(|a, b| a.partial_cmp(b).unwrap());

        println!("\n📊 Market Microstructure Analysis");
        println!("=====================================");
        if let Some(sym) = symbol {
            println!("Symbol: {}", sym);
        }
        println!("Total Orders: {}", total_orders);
        println!(
            "Buy Orders: {} ({:.1}%)",
            buy_orders,
            buy_orders as f64 / total_orders as f64 * 100.0
        );
        println!(
            "Sell Orders: {} ({:.1}%)",
            sell_orders,
            sell_orders as f64 / total_orders as f64 * 100.0
        );
        println!("Total Buy Quantity: {:.0}", total_buy_qty);
        println!("Total Sell Quantity: {:.0}", total_sell_qty);
        println!(
            "Order Imbalance: {:.2}",
            (total_buy_qty - total_sell_qty) / (total_buy_qty + total_sell_qty)
        );
        println!("Average Order Size: {:.0}", avg_order_size);

        if let (Some(bid), Some(ask)) = (best_bid, best_ask) {
            println!("Best Bid: {:.2}", bid);
            println!("Best Ask: {:.2}", ask);
            println!(
                "Spread: {:.2} ({:.2} bps)",
                ask - bid,
                (ask - bid) / bid * 10000.0
            );
        }

        // Analyze order types
        let disclosed_orders = filtered.iter().filter(|e| e.disclosed_qty > 0.0).count();
        let gtd_orders = filtered.iter().filter(|e| !e.gtd_date.is_empty()).count();

        println!("\n📋 Order Types:");
        println!(
            "Disclosed Quantity Orders: {} ({:.1}%)",
            disclosed_orders,
            disclosed_orders as f64 / total_orders as f64 * 100.0
        );
        println!(
            "GTD Orders: {} ({:.1}%)",
            gtd_orders,
            gtd_orders as f64 / total_orders as f64 * 100.0
        );

        // Analyze book types
        let mut book_types = std::collections::HashMap::new();
        for entry in &filtered {
            *book_types.entry(entry.book_type.as_str()).or_insert(0) += 1;
        }

        println!("\n📚 Book Types:");
        for (book_type, count) in book_types {
            println!(
                "  {}: {} ({:.1}%)",
                book_type,
                count,
                count as f64 / total_orders as f64 * 100.0
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
        let entry = NseOrderEntry::from_csv_line(line).unwrap();

        assert_eq!(entry.order_number, 12345);
        assert_eq!(entry.symbol, "NIFTY");
        assert_eq!(entry.quantity, 100.0);
        assert_eq!(entry.price, 5000.50);
        assert_eq!(entry.side, Side::Bid);
    }
}

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

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
            {
                info!("\n📈 NIFTY Order Book:");
                info!("Best Bid: {:.2} x {:.0}", bid_px.as_f64(), bid_qty.as_f64());
                info!("Best Ask: {:.2} x {:.0}", ask_px.as_f64(), ask_qty.as_f64());
                info!(
                    "Spread: {:.2} bps",
                    book.spread_ticks().unwrap_or(0) as f64 * 0.01
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
