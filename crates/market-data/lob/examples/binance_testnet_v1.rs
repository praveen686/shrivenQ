//! Binance Testnet with LOB v1 demo
//!
//! This demonstrates the original LOB v1 with Binance testnet data

use common::{L2Update, Px, Qty, Side, Symbol, Ts};
use futures_util::{SinkExt, StreamExt};
use lob::{FeatureCalculator, OrderBook};
use serde_json::Value;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🚀 Binance Testnet LOB v1 Demo");
    info!("==================================\n");

    // Create LOB v1
    let symbol = Symbol(1); // BTCUSDT
    let mut book = OrderBook::new(symbol);

    // Create feature calculator for v1
    let mut feature_calc = FeatureCalculator::new(60_000_000_000, 1000); // 60s window, 1000 capacity

    // Performance tracking
    let mut update_count = 0u64;
    let mut update_times = Vec::new();
    let mut crossed_count = 0u64;

    // Try different testnet URLs
    let testnet_urls = vec![
        "wss://stream.testnet.binance.vision/ws",
        "wss://testnet.binance.vision/ws",
        "wss://stream.testnet.binance.vision:9443/ws",
    ];

    let mut connected = false;
    let mut ws_stream = None;

    for url in &testnet_urls {
        info!("📡 Trying testnet URL: {}", url);
        match connect_async(*url).await {
            Ok((stream, response)) => {
                info!("✅ Connected! Response: {:?}", response.status());
                ws_stream = Some(stream);
                connected = true;
                break;
            }
            Err(e) => {
                info!("❌ Failed: {}", e);
            }
        }
    }

    if !connected {
        error!("Could not connect to any testnet URL");
        return Err("Connection failed".into());
    }

    let ws = ws_stream.unwrap();
    let (mut write, mut read) = ws.split();

    // Subscribe to depth updates - try multiple formats
    let subscribe_msgs = vec![
        serde_json::json!({
            "method": "SUBSCRIBE",
            "params": ["btcusdt@depth20@100ms"],
            "id": 1
        }),
        serde_json::json!({
            "method": "SUBSCRIBE",
            "params": ["btcusdt@depth"],
            "id": 2
        }),
        serde_json::json!({
            "method": "SUBSCRIBE",
            "params": ["btcusdt@depth5"],
            "id": 3
        }),
    ];

    for msg in subscribe_msgs {
        info!("📊 Sending subscription: {}", msg);
        write.send(Message::Text(msg.to_string())).await?;
    }

    info!("🔄 Waiting for market data...\n");

    // Also send a list subscriptions request to see what's available
    let list_msg = serde_json::json!({
        "method": "LIST_SUBSCRIPTIONS",
        "id": 99
    });
    write.send(Message::Text(list_msg.to_string())).await?;

    let mut message_count = 0;

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                message_count += 1;

                // Log first few messages to understand format
                if message_count <= 5 {
                    info!(
                        "📨 Message #{}: {}",
                        message_count,
                        if text.len() > 200 {
                            format!("{}...", &text[..200])
                        } else {
                            text.clone()
                        }
                    );
                }

                if let Ok(data) = serde_json::from_str::<Value>(&text) {
                    // Check if it's a subscription response
                    if let Some(id) = data.get("id") {
                        info!(
                            "📌 Subscription response for id {}: {:?}",
                            id,
                            data.get("result")
                        );
                    }

                    // Check if it's depth data
                    if data.get("lastUpdateId").is_some() || data.get("e").is_some() {
                        let start = Instant::now();
                        let ts = Ts::from_nanos(
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_nanos() as u64,
                        );

                        // Process bids
                        if let Some(bids) = data.get("bids").and_then(|b| b.as_array()) {
                            for (level, bid) in bids.iter().take(20).enumerate() {
                                if let Some(bid_arr) = bid.as_array() {
                                    if bid_arr.len() >= 2 {
                                        let price = bid_arr[0]
                                            .as_str()
                                            .and_then(|s| s.parse::<f64>().ok())
                                            .unwrap_or(0.0);
                                        let qty = bid_arr[1]
                                            .as_str()
                                            .and_then(|s| s.parse::<f64>().ok())
                                            .unwrap_or(0.0);

                                        if price > 0.0 {
                                            let update = L2Update::new(ts, symbol).with_level_data(
                                                Side::Bid,
                                                Px::new(price),
                                                Qty::new(qty),
                                                level as u8,
                                            );

                                            if let Err(e) = book.apply(&update) {
                                                debug!("Bid update error: {}", e);
                                                crossed_count += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Process asks
                        if let Some(asks) = data.get("asks").and_then(|a| a.as_array()) {
                            for (level, ask) in asks.iter().take(20).enumerate() {
                                if let Some(ask_arr) = ask.as_array() {
                                    if ask_arr.len() >= 2 {
                                        let price = ask_arr[0]
                                            .as_str()
                                            .and_then(|s| s.parse::<f64>().ok())
                                            .unwrap_or(0.0);
                                        let qty = ask_arr[1]
                                            .as_str()
                                            .and_then(|s| s.parse::<f64>().ok())
                                            .unwrap_or(0.0);

                                        if price > 0.0 {
                                            let update = L2Update::new(ts, symbol).with_level_data(
                                                Side::Ask,
                                                Px::new(price),
                                                Qty::new(qty),
                                                level as u8,
                                            );

                                            if let Err(e) = book.apply(&update) {
                                                debug!("Ask update error: {}", e);
                                                crossed_count += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        let elapsed = start.elapsed();
                        update_times.push(elapsed.as_nanos() as u64);
                        update_count += 1;

                        // Calculate and display features every 10 updates
                        if update_count % 10 == 0 {
                            if let Some(features) = feature_calc.calculate(&book) {
                                if update_count % 50 == 0 {
                                    if let (Some((bid_px, bid_qty)), Some((ask_px, ask_qty))) =
                                        (book.best_bid(), book.best_ask())
                                    {
                                        info!("📈 Update #{}", update_count);
                                        info!(
                                            "  BBO: {:.2} x {:.3} | {:.2} x {:.3}",
                                            bid_px.as_f64(),
                                            bid_qty.as_f64(),
                                            ask_px.as_f64(),
                                            ask_qty.as_f64()
                                        );
                                        info!("  Spread: {} ticks", features.spread_ticks);
                                        info!("  Imbalance: {:.4}", features.imbalance);
                                        info!("  Crossed books: {}", crossed_count);

                                        // Calculate performance stats
                                        if !update_times.is_empty() {
                                            update_times.sort_unstable();
                                            let p50 = update_times[update_times.len() / 2];
                                            let p99 = update_times[update_times.len() * 99 / 100];
                                            info!(
                                                "  LOB v1 Performance: p50={} ns, p99={} ns\n",
                                                p50, p99
                                            );
                                        }
                                    }
                                }
                            }
                        }

                        // Stop after 200 updates for demo
                        if update_count >= 200 {
                            info!("📊 Reached 200 updates, generating final report...");
                            break;
                        }
                    }
                }
            }
            Ok(Message::Ping(data)) => {
                write.send(Message::Pong(data)).await?;
            }
            Ok(Message::Close(_)) => {
                info!("WebSocket closed by server");
                break;
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }

        // Stop if no depth data after many messages
        if message_count > 50 && update_count == 0 {
            info!("⚠️ No depth data received after {} messages", message_count);
            info!("The testnet might not be streaming data or the symbol might be different.");
            break;
        }
    }

    // Final analysis
    info!("\n🎯 Final Analysis");
    info!("================");
    info!("Total messages received: {}", message_count);
    info!("Total updates processed: {}", update_count);
    info!("Crossed book errors: {}", crossed_count);

    if !update_times.is_empty() {
        update_times.sort_unstable();
        let sum: u64 = update_times.iter().sum();
        let avg = sum / update_times.len() as u64;
        let p50 = update_times[update_times.len() / 2];
        let p90 = update_times[update_times.len() * 90 / 100];
        let p99 = update_times[update_times.len() * 99 / 100];

        info!("\n⚡ LOB v1 Performance Statistics:");
        info!("  Average: {} ns", avg);
        info!("  P50: {} ns", p50);
        info!("  P90: {} ns", p90);
        info!("  P99: {} ns", p99);

        if p50 < 200 {
            info!("  ✅ Meets <200ns p50 target!");
        } else {
            info!("  ⚠️ Above 200ns p50 target");
        }
    }

    if update_count == 0 {
        info!("\n⚠️ No market depth data was received.");
        info!("Possible reasons:");
        info!("  1. Testnet might not have active market makers");
        info!("  2. Symbol format might be different (e.g., BTC-USDT vs BTCUSDT)");
        info!("  3. Subscription format might have changed");
        info!("\nTry using the production Binance API with:");
        info!("  wss://stream.binance.com:9443/ws/btcusdt@depth20@100ms");
    }

    info!("\n✅ Demo completed!");

    Ok(())
}
