//! Test Zerodha live market data during market hours
//!
//! This test connects to Zerodha WebSocket and streams real NIFTY/BANKNIFTY data

use auth::{ZerodhaAuth, ZerodhaConfig};
use chrono::Timelike;
use common::Symbol;
use dotenv::dotenv;
use feeds::common::adapter::{FeedAdapter, FeedConfig};
use feeds::zerodha::websocket::ZerodhaWebSocketFeed;
use lob::OrderBookV2;
use std::collections::HashMap;
use std::env;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    // Load environment
    dotenv().ok();

    info!("🚀 Testing Zerodha Live Market Data");
    info!("=====================================");
    info!("Time: {} IST", chrono::Local::now().format("%H:%M:%S"));

    // Check if markets are open (9:15 AM - 3:30 PM IST)
    let now = chrono::Local::now();
    let hour = now.hour();
    let minute = now.minute();
    let is_market_hours =
        (hour == 9 && minute >= 15) || (hour > 9 && hour < 15) || (hour == 15 && minute <= 30);

    if !is_market_hours {
        info!("⚠️ Markets are closed. Using synthetic data.");
    } else {
        info!("✅ Markets are open! Connecting to live data...");
    }

    // Setup authentication
    let config = ZerodhaConfig::new(
        env::var("ZERODHA_USER_ID")?,
        env::var("ZERODHA_PASSWORD")?,
        env::var("ZERODHA_TOTP_SECRET")?,
        env::var("ZERODHA_API_KEY")?,
        env::var("ZERODHA_API_SECRET")?,
    )
    .with_cache_dir("./cache/zerodha".to_string());

    let auth = ZerodhaAuth::new(config);

    // Create feed configuration with correct instrument tokens
    let mut symbol_map = HashMap::new();
    symbol_map.insert(Symbol(256265), "256265".to_string()); // NIFTY 50
    symbol_map.insert(Symbol(260105), "260105".to_string()); // NIFTY BANK

    let feed_config = FeedConfig {
        name: "zerodha".to_string(),
        ws_url: "wss://ws.kite.trade".to_string(),
        api_url: "https://api.kite.trade".to_string(),
        symbol_map,
        max_reconnects: 3,
        reconnect_delay_ms: 5000,
    };

    // Create WebSocket feed
    let mut ws_feed = ZerodhaWebSocketFeed::new(feed_config.clone(), auth);

    // Create channel for updates
    let (tx, mut rx) = mpsc::channel(1000);

    info!("📊 Connecting to feed...");

    // Connect and subscribe
    ws_feed.connect().await?;
    ws_feed
        .subscribe(vec![Symbol(256265), Symbol(260105)])
        .await?;

    // Start WebSocket in background
    let ws_handle = tokio::spawn(async move {
        if let Err(e) = ws_feed.run(tx).await {
            error!("WebSocket error: {}", e);
        }
    });

    // Create order books
    let mut nifty_book = OrderBookV2::new_with_roi(
        Symbol(256265),
        0.05,    // tick size
        25.0,    // lot size
        25000.0, // ROI center
        250.0,   // ROI width
    );

    let mut banknifty_book = OrderBookV2::new_with_roi(
        Symbol(260105),
        0.05,    // tick size
        15.0,    // lot size
        52000.0, // ROI center
        500.0,   // ROI width
    );

    // Statistics
    let mut update_count = 0;
    let mut latencies = Vec::new();
    let start_time = Instant::now();

    info!("\n📈 Streaming market data...\n");

    // Process updates
    while let Some(update) = rx.recv().await {
        let process_start = Instant::now();

        // Apply to appropriate book
        match update.symbol.0 {
            256265 => {
                if let Err(e) = nifty_book.apply_validated(&update) {
                    error!("NIFTY update error: {}", e);
                }
            }
            260105 => {
                if let Err(e) = banknifty_book.apply_validated(&update) {
                    error!("BANKNIFTY update error: {}", e);
                }
            }
            _ => {}
        }

        let latency = process_start.elapsed();
        latencies.push(latency.as_nanos() as u64);
        update_count += 1;

        // Display stats every 50 updates
        if update_count % 50 == 0 {
            // Calculate percentiles
            latencies.sort_unstable();
            let p50 = latencies[latencies.len() / 2];
            let p99 = latencies[latencies.len() * 99 / 100];

            info!("📊 Stats after {} updates:", update_count);

            // NIFTY stats
            if let (Some((bid, bid_qty)), Some((ask, ask_qty))) =
                (nifty_book.best_bid(), nifty_book.best_ask())
            {
                info!(
                    "  NIFTY: {:.2} x {} | {:.2} x {} (spread: {:.2})",
                    bid.as_f64(),
                    bid_qty.as_i64(),
                    ask.as_f64(),
                    ask_qty.as_i64(),
                    ask.as_f64() - bid.as_f64()
                );
            }

            // BANKNIFTY stats
            if let (Some((bid, bid_qty)), Some((ask, ask_qty))) =
                (banknifty_book.best_bid(), banknifty_book.best_ask())
            {
                info!(
                    "  BANKNIFTY: {:.2} x {} | {:.2} x {} (spread: {:.2})",
                    bid.as_f64(),
                    bid_qty.as_i64(),
                    ask.as_f64(),
                    ask_qty.as_i64(),
                    ask.as_f64() - bid.as_f64()
                );
            }

            info!("  Latency: p50={} ns, p99={} ns", p50, p99);
            info!(
                "  Rate: {:.1} updates/sec\n",
                update_count as f64 / start_time.elapsed().as_secs_f64()
            );
        }

        // Stop after 500 updates for demo
        if update_count >= 500 {
            info!("✅ Test completed successfully!");
            break;
        }
    }

    // Clean up
    ws_handle.abort();

    // Final summary
    info!("\n📈 Final Summary:");
    info!("==================");
    info!("Total updates: {}", update_count);
    info!("Duration: {:.1}s", start_time.elapsed().as_secs_f64());
    info!(
        "Average rate: {:.1} updates/sec",
        update_count as f64 / start_time.elapsed().as_secs_f64()
    );

    if !latencies.is_empty() {
        latencies.sort_unstable();
        info!("Latency p50: {} ns", latencies[latencies.len() / 2]);
        info!("Latency p99: {} ns", latencies[latencies.len() * 99 / 100]);
    }

    Ok(())
}
