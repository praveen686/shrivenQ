//! WAL Replay Tool - Reconstruct orderbook from historical data
//!
//! This tool reads market data events from the WAL and reconstructs
//! the orderbook at any point in time, crucial for backtesting and analysis.

use anyhow::Result;
use clap::{Arg, Command};
use common::{Px, Qty, Ts};
use data_aggregator::{DataEvent, Wal};
use std::collections::BTreeMap;
use std::path::Path;
use tracing::{info, warn};
use tracing_subscriber;

/// Constants for orderbook display
const MAX_ORDERBOOK_LEVELS: usize = 5;
const SPREAD_BPS_MULTIPLIER: f64 = 10000.0;
const PROGRESS_LOG_INTERVAL: usize = 100;
const SEPARATOR_LENGTH: usize = 60;
const DEFAULT_EVENT_LIMIT: &str = "1000";

/// Orderbook level representation
#[derive(Debug, Clone)]
struct OrderBookLevel {
    price: Px,
    quantity: Qty,
}

/// Orderbook state for a single symbol
#[derive(Debug)]
struct OrderBook {
    symbol: String,
    bids: BTreeMap<i64, Qty>, // price (in ticks) -> quantity
    asks: BTreeMap<i64, Qty>, // price (in ticks) -> quantity  
    last_update: Ts,
}

impl OrderBook {
    fn new(symbol: String) -> Self {
        Self {
            symbol,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            last_update: Ts::from_nanos(0),
        }
    }
    
    /// Update orderbook with depth data
    fn update_depth(&mut self, bids: Vec<(f64, f64)>, asks: Vec<(f64, f64)>, timestamp: Ts) {
        // Clear old levels
        self.bids.clear();
        self.asks.clear();
        
        // Add new bid levels
        for (price, qty) in bids {
            if qty > 0.0 {
                let price_ticks = Px::new(price).as_i64();
                let quantity = Qty::new(qty);
                self.bids.insert(price_ticks, quantity);
            }
        }
        
        // Add new ask levels
        for (price, qty) in asks {
            if qty > 0.0 {
                let price_ticks = Px::new(price).as_i64();
                let quantity = Qty::new(qty);
                self.asks.insert(price_ticks, quantity);
            }
        }
        
        self.last_update = timestamp;
    }
    
    /// Get orderbook levels as OrderBookLevel structs
    fn get_levels(&self) -> (Vec<OrderBookLevel>, Vec<OrderBookLevel>) {
        let bid_levels: Vec<OrderBookLevel> = self.bids
            .iter()
            .rev()
            .take(MAX_ORDERBOOK_LEVELS)
            .map(|(price_ticks, qty)| OrderBookLevel {
                price: Px::from_i64(*price_ticks),
                quantity: *qty,
            })
            .collect();
            
        let ask_levels: Vec<OrderBookLevel> = self.asks
            .iter()
            .take(MAX_ORDERBOOK_LEVELS)
            .map(|(price_ticks, qty)| OrderBookLevel {
                price: Px::from_i64(*price_ticks),
                quantity: *qty,
            })
            .collect();
            
        (bid_levels, ask_levels)
    }
    
    /// Display the orderbook in a nice format
    fn display(&self) {
        info!("📋 ORDERBOOK: {} (Last Update: {})", self.symbol, self.last_update.as_nanos());
        info!("===============================================");
        info!("       ASK PRICE    |    ASK QTY");
        
        let (bid_levels, ask_levels) = self.get_levels();
        
        // Show top asks (in reverse order for display)
        for level in ask_levels.iter().rev() {
            info!("    🔴 {:<12.2} | {:<10.4}", level.price.as_f64(), level.quantity.as_f64());
        }
        
        info!("    --------------------------------");
        
        // Show top bids
        for level in &bid_levels {
            info!("    🟢 {:<12.2} | {:<10.4}", level.price.as_f64(), level.quantity.as_f64());
        }
        
        info!("       BID PRICE    |    BID QTY");
        info!("===============================================");
        
        // Show spread
        if !bid_levels.is_empty() && !ask_levels.is_empty() {
            let best_bid = &bid_levels[0];
            let best_ask = &ask_levels[0];
            let spread = best_ask.price.as_f64() - best_bid.price.as_f64();
            let spread_bps = (spread / best_bid.price.as_f64()) * SPREAD_BPS_MULTIPLIER;
            info!("💹 SPREAD: {:.2} ({:.1} bps)", spread, spread_bps);
        }
        info!("");
    }
}

/// Replay engine for reconstructing market state
struct MarketReplay {
    orderbooks: std::collections::HashMap<String, OrderBook>,
}

impl MarketReplay {
    fn new() -> Self {
        Self {
            orderbooks: std::collections::HashMap::new(),
        }
    }
    
    /// Process a market data event and update state
    fn process_event(&mut self, event: &DataEvent) {
        match event {
            DataEvent::Trade(trade) => {
                let symbol = trade.symbol.to_string();
                info!("📈 TRADE: {} @ {} (qty: {})", 
                      symbol, trade.price.as_f64(), trade.quantity.as_f64());
                
                // Create or update orderbook for this symbol
                let book = self.orderbooks
                    .entry(symbol.clone())
                    .or_insert_with(|| OrderBook::new(symbol));
                    
                // Build orderbook from trades by accumulating at price levels
                let price_f64 = trade.price.as_f64();
                let qty_f64 = trade.quantity.as_f64();
                let price_ticks = trade.price.as_i64();
                
                // Update timestamp
                book.last_update = trade.ts;
                
                // Add or update the price level based on trade direction
                if trade.is_buy {
                    // Buy trade executed - means there was an ask at this price
                    // Add to asks (or update existing)
                    let existing_qty = book.asks.get(&price_ticks).copied().unwrap_or(Qty::new(0.0));
                    book.asks.insert(price_ticks, Qty::new(existing_qty.as_f64() + qty_f64));
                    
                    // Also create a bid slightly below for spread
                    let bid_price = price_ticks - 100; // 0.01 below in ticks
                    let existing_bid = book.bids.get(&bid_price).copied().unwrap_or(Qty::new(0.0));
                    book.bids.insert(bid_price, Qty::new(existing_bid.as_f64() + qty_f64 * 0.5));
                } else {
                    // Sell trade executed - means there was a bid at this price
                    // Add to bids (or update existing)
                    let existing_qty = book.bids.get(&price_ticks).copied().unwrap_or(Qty::new(0.0));
                    book.bids.insert(price_ticks, Qty::new(existing_qty.as_f64() + qty_f64));
                    
                    // Also create an ask slightly above for spread
                    let ask_price = price_ticks + 100; // 0.01 above in ticks
                    let existing_ask = book.asks.get(&ask_price).copied().unwrap_or(Qty::new(0.0));
                    book.asks.insert(ask_price, Qty::new(existing_ask.as_f64() + qty_f64 * 0.5));
                }
                
                // Keep orderbook size reasonable - remove far levels if too many
                const MAX_LEVELS_PER_SIDE: usize = 10;
                if book.bids.len() > MAX_LEVELS_PER_SIDE {
                    // Keep only the best (highest) bids
                    let mut bid_prices: Vec<i64> = book.bids.keys().copied().collect();
                    bid_prices.sort_by(|a, b| b.cmp(a)); // Sort descending
                    for price in bid_prices.iter().skip(MAX_LEVELS_PER_SIDE) {
                        book.bids.remove(price);
                    }
                }
                if book.asks.len() > MAX_LEVELS_PER_SIDE {
                    // Keep only the best (lowest) asks
                    let mut ask_prices: Vec<i64> = book.asks.keys().copied().collect();
                    ask_prices.sort(); // Sort ascending
                    for price in ask_prices.iter().skip(MAX_LEVELS_PER_SIDE) {
                        book.asks.remove(price);
                    }
                }
            }
            DataEvent::Candle(candle) => {
                let symbol = candle.symbol.to_string();
                info!("🕯️ CANDLE: {} OHLC [{:.2}, {:.2}, {:.2}, {:.2}] Vol: {:.2}", 
                      symbol, candle.open.as_f64(), candle.high.as_f64(), 
                      candle.low.as_f64(), candle.close.as_f64(), candle.volume.as_f64());
            }
            DataEvent::OrderBook(orderbook) => {
                let symbol = orderbook.symbol.to_string();
                info!("📚 ORDERBOOK: {} with {} bid levels, {} ask levels (seq: {})", 
                      symbol, orderbook.bid_levels.len(), orderbook.ask_levels.len(), orderbook.sequence);
                
                // Create or update orderbook for this symbol
                let book = self.orderbooks
                    .entry(symbol.clone())
                    .or_insert_with(|| OrderBook::new(symbol));
                
                // Update with new levels
                book.bids.clear();
                for (price, qty, _count) in &orderbook.bid_levels {
                    book.bids.insert(price.as_i64(), *qty);
                }
                
                book.asks.clear();
                for (price, qty, _count) in &orderbook.ask_levels {
                    book.asks.insert(price.as_i64(), *qty);
                }
                
                book.last_update = orderbook.ts;
            }
            _ => {
                warn!("📊 Unhandled market event type");
            }
        }
    }
    
    /// Display all current orderbooks
    fn display_orderbooks(&self) {
        for (_symbol, orderbook) in &self.orderbooks {
            orderbook.display();
        }
    }
    
    /// Get statistics about processed data
    fn get_stats(&self) -> (usize, usize) {
        let total_books = self.orderbooks.len();
        let total_levels: usize = self.orderbooks.values()
            .map(|book| book.bids.len() + book.asks.len())
            .sum();
        (total_books, total_levels)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    let matches = Command::new("wal_replay")
        .about("Replay market data from WAL and reconstruct orderbooks")
        .arg(
            Arg::new("wal_dir")
                .short('d')
                .long("dir")
                .value_name("DIR")
                .help("WAL directory path")
                .default_value("./data/wal")
        )
        .arg(
            Arg::new("symbol")
                .short('s')
                .long("symbol")
                .value_name("SYMBOL")
                .help("Filter by symbol (optional)")
        )
        .arg(
            Arg::new("limit")
                .short('l')
                .long("limit")
                .value_name("NUMBER")
                .help("Limit number of events to process")
                .default_value(DEFAULT_EVENT_LIMIT)
        )
        .get_matches();
    
    let wal_dir = matches.get_one::<String>("wal_dir").unwrap();
    let symbol_filter = matches.get_one::<String>("symbol");
    let limit: usize = matches.get_one::<String>("limit").unwrap().parse()?;
    
    info!("🚀 Starting WAL replay from: {}", wal_dir);
    
    // Open WAL
    let wal = Wal::new(Path::new(wal_dir), None)?;
    
    // Read all entries
    info!("📖 Reading all entries from WAL...");
    let entries: Vec<DataEvent> = match wal.read_all() {
        Ok(events) => events,
        Err(e) => {
            warn!("❌ Failed to read WAL: {}", e);
            Vec::new()
        }
    };
    
    info!("✅ Loaded {} events from WAL", entries.len());
    
    if entries.is_empty() {
        warn!("⚠️ No events found in WAL. The WAL might be empty or corrupt.");
        warn!("  Try running ./test_live_data.sh first to capture some market data.");
        return Ok(());
    }
    
    // Create replay engine
    let mut replay = MarketReplay::new();
    
    // Process events
    let mut processed = 0;
    for event in entries.iter().take(limit) {
        // Apply symbol filter if specified
        let symbol = match event {
            DataEvent::Trade(trade) => trade.symbol.to_string(),
            DataEvent::Candle(candle) => candle.symbol.to_string(),
            DataEvent::OrderBook(book) => book.symbol.to_string(),
            _ => String::new(),
        };
        
        if let Some(filter) = symbol_filter {
            if !symbol.contains(filter) {
                continue;
            }
        }
        
        // Warn if we get empty symbol
        if symbol.is_empty() && !matches!(event, DataEvent::System(_) | DataEvent::VolumeProfile(_) | DataEvent::Microstructure(_)) {
            warn!("Event with empty symbol detected");
        }
        
        replay.process_event(event);
        processed += 1;
        
        // Show progress every PROGRESS_LOG_INTERVAL events
        if processed % PROGRESS_LOG_INTERVAL == 0 {
            info!("Processed {} events...", processed);
        }
    }
    
    let (total_books, total_levels) = replay.get_stats();
    
    info!("🎯 Replay Summary:");
    info!("  📊 Processed Events: {}", processed);
    info!("  📋 Orderbooks: {}", total_books);
    info!("  📈 Total Levels: {}", total_levels);
    
    let separator = "=".repeat(SEPARATOR_LENGTH);
    info!("");
    info!("{}", separator);
    info!("📊 FINAL MARKET STATE FROM WAL REPLAY");
    info!("{}", separator);
    
    replay.display_orderbooks();
    
    Ok(())
}