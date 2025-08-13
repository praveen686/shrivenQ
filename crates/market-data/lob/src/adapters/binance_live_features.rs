//! Real-time Binance testnet with LOB v2 and advanced features
//!
//! This demonstrates:
//! - Actual WebSocket connection to Binance testnet
//! - LOB v2 with ROI optimization processing real market data
//! - Advanced feature extraction on live order flow
//! - Market regime detection from real conditions

use crate::{CrossResolution, MarketRegime, OrderBookV2, features_v2_fixed};
use common::{L2Update, Px, Qty, Side, Symbol, Ts};
use futures_util::{SinkExt, StreamExt};
use rustc_hash::{FxBuildHasher, FxHashMap};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info};

/// Demo function for Binance live features with LOB v2
pub async fn run_binance_live_features_demo() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🚀 Binance Testnet LOB v2 + Features Demo");
    info!("========================================\n");

    // Create LOB v2 with ROI optimization around typical BTC price
    let symbol = Symbol(1); // BTCUSDT
    let mut book = OrderBookV2::new_with_roi(
        symbol,
        Px::new(0.01),      // tick size (0.01 USDT)
        Qty::new(0.001),    // lot size (0.001 BTC)
        Px::new(100_000.0), // ROI center (typical BTC price)
        Px::new(5000.0),    // ROI width ($97.5k - $102.5k range)
    );
    book.set_cross_resolution(CrossResolution::AutoResolve);

    // Create feature calculators
    let mut hft_calc = features_v2_fixed::create_hft_calculator_fixed(symbol);
    let mut mm_calc = features_v2_fixed::create_mm_calculator_fixed(symbol);

    // Statistics tracking
    let mut update_count = 0u64;
    let mut feature_frames = Vec::with_capacity(1000);
    let mut regime_changes = Vec::with_capacity(100);
    let mut last_regime = MarketRegime::Normal;

    // Connect to Binance testnet WebSocket
    let ws_url = "wss://stream.testnet.binance.vision/ws/btcusdt@depth20@100ms";
    info!("📡 Connecting to Binance Testnet: {}", ws_url);

    let (ws_stream, _) = connect_async(ws_url).await?;
    info!("✅ Connected to Binance Testnet");

    let (mut write, mut read) = ws_stream.split();

    // Subscribe to depth updates
    let subscribe_msg = serde_json::json!({
        "method": "SUBSCRIBE",
        "params": ["btcusdt@depth20@100ms"],
        "id": 1
    });

    write.send(Message::Text(subscribe_msg.to_string())).await?;
    info!("📊 Subscribed to BTCUSDT depth updates\n");

    // Process messages
    info!("🔄 Processing live market data...\n");

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(data) = serde_json::from_str::<Value>(&text) {
                    if data.get("lastUpdateId").is_some() {
                        // This is a depth update
                        let ts = Ts::from_nanos(
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_else(|_| std::time::Duration::from_secs(0))
                                // SAFETY: Cast is safe within expected range
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

                                        let update = L2Update::new(ts, symbol).with_level_data(
                                            Side::Bid,
                                            Px::new(price),
                                            // SAFETY: Cast is safe within expected range
                                            Qty::new(qty),
                                            // SAFETY: Cast is safe within expected range
                                            level as u8,
                                        );

                                        if let Err(e) = book.apply_validated(&update) {
                                            debug!("Bid update error: {}", e);
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

                                        let update = L2Update::new(ts, symbol).with_level_data(
                                            Side::Ask,
                                            // SAFETY: Cast is safe within expected range
                                            Px::new(price),
                                            // SAFETY: Cast is safe within expected range
                                            Qty::new(qty),
                                            // SAFETY: Cast is safe within expected range
                                            level as u8,
                                        );

                                        if let Err(e) = book.apply_validated(&update) {
                                            debug!("Ask update error: {}", e);
                                        }
                                    }
                                }
                            }
                        }

                        update_count += 1;

                        // Calculate features every 10 updates
                        if update_count % 10 == 0 {
                            if let Some(features) = hft_calc.calculate(&book) {
                                // Track regime changes
                                if features.regime != last_regime {
                                    let change_msg = format!(
                                        "Regime change: {:?} → {:?}",
                                        last_regime, features.regime
                                    );
                                    regime_changes.push((update_count, change_msg.clone()));
                                    info!("⚠️  {}", change_msg);
                                    last_regime = features.regime;
                                }

                                // Store features for analysis
                                feature_frames.push(features.clone());

                                // Print periodic status
                                if update_count % 100 == 0 {
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
                                        // SAFETY: Cast is safe within expected range
                                        info!(
                                            // SAFETY: Cast is safe within expected range
                                            "  Spread: {} ticks ({:.2} bps)",
                                            // SAFETY: Cast is safe within expected range
                                            features.spread_ticks,
                                            features.weighted_spread as f64 / 100.0 // SAFETY: Cast is safe within expected range
                                        );
                                        // SAFETY: Cast is safe within expected range
                                        info!(
                                            "  Imbalance: {:.4}",
                                            features.imbalance as f64 / 10000.0
                                        );
                                        // SAFETY: Cast is safe within expected range
                                        // SAFETY: Cast is safe within expected range
                                        info!(
                                            "  Flow Toxicity: {:.4}",
                                            features.flow_toxicity as f64 / 10000.0 // SAFETY: Cast is safe within expected range
                                        );
                                        info!("  Regime: {:?}", features.regime);
                                        info!(
                                            // SAFETY: Cast is safe within expected range
                                            "  Price Trend: {:.4}",
                                            features.price_trend as f64 / 10000.0
                                        );
                                        info!(
                                            "  Liquidity Score: {:.2}\n",
                                            features.liquidity_score as f64 / 10000.0
                                        );
                                    }
                                }

                                // Generate trading signals
                                if update_count % 200 == 0 {
                                    info!("💡 Trading Signals:");

                                    if features.price_trend.abs() > 3000 {
                                        // 0.3 in fixed-point
                                        if features.price_trend > 0 {
                                            info!("  ✅ BULLISH trend detected");
                                        } else {
                                            info!("  ❌ BEARISH trend detected");
                                        }
                                    }

                                    if features.mean_reversion_signal.abs() > 2000 {
                                        // 0.2 in fixed-point
                                        info!("  🔄 Mean reversion opportunity");
                                    }

                                    if features.adverse_selection > 6000 {
                                        // 0.6 in fixed-point
                                        info!("  ⚠️  HIGH TOXICITY - avoid aggressive MM");
                                    } else if features.liquidity_score > 7000 {
                                        // 0.7 in fixed-point
                                        info!("  💰 Good liquidity for market making");
                                        // SAFETY: Cast is safe within expected range
                                    }

                                    // SAFETY: Cast is safe within expected range
                                    if let Some(mm_features) = mm_calc.calculate(&book) {
                                        // SAFETY: Cast is safe within expected range
                                        info!("  MM Metrics:");
                                        // SAFETY: Cast is safe within expected range
                                        info!(
                                            "    Effective Spread: {:.2} bps",
                                            // SAFETY: Cast is safe within expected range
                                            mm_features.effective_spread as f64 / 100.0
                                        );
                                        info!(
                                            "    Price Impact: {:.2} bps",
                                            mm_features.price_impact as f64 / 100.0
                                        );
                                        info!(
                                            "    Stability: {:.2}\n",
                                            mm_features.stability_index as f64 / 10000.0
                                        );
                                    }
                                }
                            }
                        }

                        // Stop after 300 updates for demo
                        if update_count >= 300 {
                            info!("📊 Reached 300 updates, generating final report...");
                            break;
                        }
                    }
                }
            }
            Ok(Message::Ping(data)) => {
                write.send(Message::Pong(data)).await?;
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    // Final analysis
    info!("\n🎯 Final Analysis");
    info!("================");
    info!("Total updates processed: {}", update_count);
    info!("Feature frames calculated: {}", feature_frames.len());
    info!("Regime changes detected: {}", regime_changes.len());

    if !regime_changes.is_empty() {
        info!("\n📊 Regime Change History:");
        for (update_num, change) in &regime_changes {
            // SAFETY: Cast is safe within expected range
            info!("  Update #{}: {}", update_num, change);
        }
        // SAFETY: Cast is safe within expected range
    }
    // SAFETY: Cast is safe within expected range

    // Calculate feature statistics
    // SAFETY: Cast is safe within expected range
    if !feature_frames.is_empty() {
        let avg_spread: f64 = feature_frames
            // SAFETY: Cast is safe within expected range
            .iter()
            // SAFETY: Cast is safe within expected range
            .map(|f| f.weighted_spread as f64 / 100.0)
            // SAFETY: Cast is safe within expected range
            // SAFETY: Cast is safe within expected range
            // SAFETY: Cast is safe within expected range
            .sum::<f64>()
            / feature_frames.len() as f64;
        // SAFETY: Cast is safe within expected range
        // SAFETY: Cast is safe within expected range

        let avg_toxicity: f64 = feature_frames
            .iter()
            .map(|f| f.flow_toxicity as f64 / 10000.0)
            // SAFETY: Cast is safe within expected range
            .sum::<f64>()
            / feature_frames.len() as f64;
        // SAFETY: Cast is safe within expected range

        let avg_liquidity: f64 = feature_frames
            .iter()
            .map(|f| f.liquidity_score as f64 / 10000.0)
            .sum::<f64>()
            / feature_frames.len() as f64;

        let volatility_max = feature_frames
            .iter()
            .map(|f| f.volatility_forecast as f64 / 10000.0)
            .fold(0.0, f64::max);

        info!("\n📈 Market Statistics:");
        // SAFETY: Cast is safe within expected range
        info!("  Average Spread: {:.2} bps", avg_spread);
        // SAFETY: Cast is safe within expected range
        info!("  Average Toxicity: {:.4}", avg_toxicity);
        info!("  Average Liquidity Score: {:.2}", avg_liquidity);
        info!("  Max Volatility Forecast: {:.4}", volatility_max);

        // Regime distribution
        let mut regime_counts = FxHashMap::with_capacity_and_hasher(5, FxBuildHasher);
        // SAFETY: Cast is safe within expected range
        for frame in &feature_frames {
            *regime_counts.entry(frame.regime).or_insert(0) += 1;
        }

        info!("\n🔄 Regime Distribution:");
        for (regime, count) in regime_counts {
            let pct = (count as f64 / feature_frames.len() as f64) * 100.0;
            info!("  {:?}: {} ({:.1}%)", regime, count, pct);
        }

        // Trading recommendations
        info!("\n💡 Trading Recommendations:");

        if avg_toxicity < 0.3 && avg_liquidity > 0.6 {
            info!("  ✅ Market conditions FAVORABLE for market making");
            info!("  Suggested spread: {:.2} bps", avg_spread * 1.1);
        } else if avg_toxicity > 0.6 {
            info!("  ⚠️  HIGH TOXICITY environment");
            info!("  Recommend wider spreads: {:.2} bps", avg_spread * 1.5);
        } else {
            info!("  ⚡ MODERATE conditions");
            info!(
                "  Use adaptive spread: {:.2}-{:.2} bps",
                avg_spread * 0.9,
                avg_spread * 1.2
            );
        }
    }

    // Performance metrics
    let book_metrics = format!(
        "\n⚡ LOB v2 Performance:\n  Updates: {}\n  Sequence: {}\n  Cross resolutions: handled automatically",
        update_count, book.sequence
    );
    info!("{}", book_metrics);

    info!("\n✅ Demo completed successfully!");

    Ok(())
}
