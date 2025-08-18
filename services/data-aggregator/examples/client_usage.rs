//! Example usage of DataAggregatorClient with WAL persistence

use anyhow::Result;
use services_common::{Px, Qty, Symbol, Ts};
use data_aggregator::Timeframe;
use services_common::clients::DataAggregatorClient;
use std::path::Path;
use tracing::{Level, info};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("🚀 Data Aggregator Client Example");
    info!("==================================");

    // Create client with WAL persistence
    let wal_path = Path::new("/tmp/shrivenquant_wal");
    std::fs::create_dir_all(wal_path)?;

    let client = DataAggregatorClient::with_wal("example_client", wal_path)?;
    info!("✅ Client created with WAL at {}", wal_path.display());

    // Simulate trading data for BTCUSDT
    let symbol = Symbol::new(1); // BTCUSDT
    info!("\n📊 Simulating trades for symbol {:?}", symbol);

    // Generate some realistic trades
    let base_price = 45000_0000; // $45,000 in fixed-point
    let mut current_price = base_price;

    for i in 0..100 {
        // Simulate price movement
        let price_change = (i % 10) as i64 - 5; // Random walk
        current_price += price_change * 10000; // $1 increments

        let price = Px::from_i64(current_price);
        let qty = Qty::from_i64((100 + i * 10) * 10000); // Increasing volume
        let is_buy = i % 3 != 0; // 2/3 buys, 1/3 sells

        // Process trade
        client.process_trade(symbol, price, qty, is_buy).await?;

        if i % 10 == 0 {
            info!(
                "  Trade {}: {} @ {} ({})",
                i,
                qty.as_f64() / 10000.0,
                price.as_f64() / 10000.0,
                if is_buy { "BUY" } else { "SELL" }
            );
        }
    }

    info!("\n📈 Retrieving candle data...");

    // Get current candles for different timeframes
    for timeframe in &[Timeframe::M1, Timeframe::M5, Timeframe::M15, Timeframe::H1] {
        if let Some(candle) = client.get_current_candle(symbol, *timeframe).await {
            info!("  {:?} Candle:", timeframe);
            info!("    Open:  ${:.2}", candle.open.as_f64() / 10000.0);
            info!("    High:  ${:.2}", candle.high.as_f64() / 10000.0);
            info!("    Low:   ${:.2}", candle.low.as_f64() / 10000.0);
            info!("    Close: ${:.2}", candle.close.as_f64() / 10000.0);
            info!("    Volume: {:.2}", candle.volume.as_f64() / 10000.0);
            info!("    Trades: {}", candle.trades);
        }
    }

    // Get completed candles
    info!("\n📊 Completed candles:");
    let completed = client.get_candles(symbol, Timeframe::M1, 10).await;
    info!("  Found {} completed M1 candles", completed.len());

    // Flush WAL to ensure persistence
    info!("\n💾 Flushing WAL to disk...");
    client.flush_wal().await?;

    // Get WAL statistics
    if let Some(stats) = client.get_wal_stats().await? {
        info!("\n📊 WAL Statistics:");
        info!("  Segments: {}", stats.segment_count);
        info!("  Total size: {} bytes", stats.total_size);
        info!("  Total entries: {}", stats.total_entries);
        if let Some(current_size) = stats.current_segment_size {
            info!("  Current segment: {} bytes", current_size);
        }
    }

    info!("\n✅ Example completed successfully!");

    Ok(())
}
