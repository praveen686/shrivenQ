//! Test LOB integration with synthetic data

use feeds::synthetic::{SyntheticDataGenerator, create_nifty_config, create_banknifty_config};
use common::Symbol;
use lob::OrderBook;
use tracing::info;
use std::time::Instant;

fn main() {
    // Initialize logging with error level for performance
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    info!("🚀 Testing LOB Integration with Synthetic Data");
    info!("{}", "=".repeat(50));
    
    // Create symbols
    let nifty = Symbol(1);
    let banknifty = Symbol(2);
    
    // Create generators
    let mut nifty_gen = SyntheticDataGenerator::new(nifty, create_nifty_config());
    let mut banknifty_gen = SyntheticDataGenerator::new(banknifty, create_banknifty_config());
    
    // Create order books
    let mut nifty_book = OrderBook::new(nifty);
    let mut banknifty_book = OrderBook::new(banknifty);
    
    info!("\n📊 Running LOB performance test...\n");
    
    let mut total_updates = 0;
    let mut update_times = Vec::new();
    let start = Instant::now();
    
    // Generate and process updates
    for i in 0..100 {
        // Generate NIFTY updates
        let nifty_updates = nifty_gen.generate_updates();
        for update in nifty_updates {
            let update_start = Instant::now();
            if let Err(e) = nifty_book.apply(&update) {
                info!("Failed to apply update: {:?}", e);
            }
            let update_time = update_start.elapsed();
            update_times.push(update_time.as_nanos());
            total_updates += 1;
        }
        
        // Generate BANKNIFTY updates
        let banknifty_updates = banknifty_gen.generate_updates();
        for update in banknifty_updates {
            let update_start = Instant::now();
            if let Err(e) = banknifty_book.apply(&update) {
                info!("Failed to apply update: {:?}", e);
            }
            let update_time = update_start.elapsed();
            update_times.push(update_time.as_nanos());
            total_updates += 1;
        }
        
        // Occasionally trigger market events
        if i % 10 == 0 {
            nifty_gen.simulate_market_event();
            banknifty_gen.simulate_market_event();
        }
        
        // Display stats every 25 iterations
        if i % 25 == 0 && i > 0 {
            display_book_stats("NIFTY", &nifty_book);
            display_book_stats("BANKNIFTY", &banknifty_book);
        }
    }
    
    let elapsed = start.elapsed();
    
    // Calculate latency percentiles
    update_times.sort_unstable();
    let p50 = update_times[update_times.len() / 2];
    let p95 = update_times[update_times.len() * 95 / 100];
    let p99 = update_times[update_times.len() * 99 / 100];
    
    info!("\n📈 Performance Statistics:");
    info!("  Total Updates: {}", total_updates);
    info!("  Total Time: {:?}", elapsed);
    info!("  Throughput: {:.0} updates/sec", total_updates as f64 / elapsed.as_secs_f64());
    info!("\n  Latency (per update):");
    info!("    p50: {} ns", p50);
    info!("    p95: {} ns", p95);
    info!("    p99: {} ns", p99);
    
    // Verify target performance
    if p50 <= 200 {
        info!("  ✅ p50 latency target (<200ns) achieved!");
    } else {
        info!("  ⚠️  p50 latency target (<200ns) missed: {}ns", p50);
    }
    
    info!("\n✅ Test completed successfully!");
}

fn display_book_stats(name: &str, book: &OrderBook) {
    let best_bid = book.best_bid();
    let best_ask = book.best_ask();
    
    let spread_ticks = if let (Some((bid_px, _)), Some((ask_px, _))) = (best_bid, best_ask) {
        ask_px.as_i64() - bid_px.as_i64()
    } else {
        0
    };
    
    let mid = if let (Some((bid_px, _)), Some((ask_px, _))) = (best_bid, best_ask) {
        (bid_px.as_f64() + ask_px.as_f64()) / 2.0
    } else {
        0.0
    };
    
    info!("\n📊 {} Order Book:", name);
    if let Some((px, qty)) = best_bid {
        info!("  Best Bid: {:.2} @ {:.0}", px.as_f64(), qty.as_f64());
    }
    if let Some((px, qty)) = best_ask {
        info!("  Best Ask: {:.2} @ {:.0}", px.as_f64(), qty.as_f64());
    }
    info!("  Spread: {} ticks", spread_ticks);
    info!("  Mid: {:.2}", mid);
    if let Some(microprice) = book.microprice() {
        info!("  Microprice: {:.2}", microprice.as_f64());
    }
}