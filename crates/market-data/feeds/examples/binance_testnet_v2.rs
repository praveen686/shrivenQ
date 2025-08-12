//! Advanced Binance testnet integration with LOB v2
//!
//! This test demonstrates:
//! - Production-grade LOB v2 with ROI optimization
//! - Smart cross-book resolution
//! - Real-time Binance testnet feed processing
//! - Performance monitoring and validation

use auth::{BinanceAuth, BinanceConfig, BinanceMarket};
use common::{L2Update, Symbol};
use feeds::{BinanceWebSocketFeed, FeedAdapter, FeedConfig};
use lob::{CrossResolution, OrderBookV2};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Performance metrics tracker
#[derive(Debug)]
struct Metrics {
    updates_processed: AtomicU64,
    crossed_books_detected: AtomicU64,
    crossed_books_resolved: AtomicU64,
    validation_errors: AtomicU64,
    avg_latency_ns: AtomicU64,
    p50_latency_ns: AtomicU64,
    p95_latency_ns: AtomicU64,
    p99_latency_ns: AtomicU64,
}

impl Metrics {
    fn new() -> Self {
        Self {
            updates_processed: AtomicU64::new(0),
            crossed_books_detected: AtomicU64::new(0),
            crossed_books_resolved: AtomicU64::new(0),
            validation_errors: AtomicU64::new(0),
            avg_latency_ns: AtomicU64::new(0),
            p50_latency_ns: AtomicU64::new(0),
            p95_latency_ns: AtomicU64::new(0),
            p99_latency_ns: AtomicU64::new(0),
        }
    }

    fn update_latencies(&self, latencies: &mut Vec<u64>) {
        if latencies.is_empty() {
            return;
        }

        latencies.sort_unstable();
        let avg = latencies.iter().sum::<u64>() / latencies.len() as u64;
        let p50 = latencies[latencies.len() / 2];
        let p95 = latencies[latencies.len() * 95 / 100];
        let p99 = latencies[latencies.len() * 99 / 100];

        self.avg_latency_ns.store(avg, Ordering::Relaxed);
        self.p50_latency_ns.store(p50, Ordering::Relaxed);
        self.p95_latency_ns.store(p95, Ordering::Relaxed);
        self.p99_latency_ns.store(p99, Ordering::Relaxed);
    }

    fn report(&self) {
        info!("📊 Performance Metrics:");
        info!(
            "  Updates Processed: {}",
            self.updates_processed.load(Ordering::Relaxed)
        );
        info!(
            "  Crossed Books Detected: {}",
            self.crossed_books_detected.load(Ordering::Relaxed)
        );
        info!(
            "  Crossed Books Resolved: {}",
            self.crossed_books_resolved.load(Ordering::Relaxed)
        );
        info!(
            "  Validation Errors: {}",
            self.validation_errors.load(Ordering::Relaxed)
        );
        info!("  Latencies:");
        info!(
            "    Average: {} ns",
            self.avg_latency_ns.load(Ordering::Relaxed)
        );
        info!(
            "    P50: {} ns",
            self.p50_latency_ns.load(Ordering::Relaxed)
        );
        info!(
            "    P95: {} ns",
            self.p95_latency_ns.load(Ordering::Relaxed)
        );
        info!(
            "    P99: {} ns",
            self.p99_latency_ns.load(Ordering::Relaxed)
        );
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Load environment
    if std::fs::metadata(".env").is_ok() {
        dotenv::dotenv().ok();
    }

    info!("🚀 Binance Testnet LOB v2 Integration Test");
    info!("{}", "=".repeat(50));

    // Get API credentials
    let api_key = env::var("BINANCE_TESTNET_API_KEY")
        .or_else(|_| env::var("BINANCE_SPOT_API_KEY"))
        .unwrap_or_else(|_| {
            warn!("No API key found, using demo mode");
            "demo".to_string()
        });

    let api_secret = env::var("BINANCE_TESTNET_API_SECRET")
        .or_else(|_| env::var("BINANCE_SPOT_API_SECRET"))
        .unwrap_or_else(|_| "demo".to_string());

    // Setup symbols
    let btcusdt = Symbol(100);
    let ethusdt = Symbol(101);

    // Create auth
    let mut auth = BinanceAuth::new();
    let _ = auth.add_market(BinanceConfig::new_testnet(
        api_key,
        api_secret,
        BinanceMarket::Spot,
    ));

    // Create feed config
    let mut symbol_map = HashMap::new();
    symbol_map.insert(btcusdt, "btcusdt".to_string());
    symbol_map.insert(ethusdt, "ethusdt".to_string());

    let feed_config = FeedConfig {
        name: "binance_testnet".to_string(),
        ws_url: "wss://testnet.binance.vision/ws".to_string(),
        api_url: "https://testnet.binance.vision".to_string(),
        symbol_map,
        max_reconnects: 3,
        reconnect_delay_ms: 1000,
    };

    // Create feed
    let mut feed = BinanceWebSocketFeed::new(
        feed_config,
        auth,
        BinanceMarket::Spot,
        true, // testnet
    );

    // Connect and subscribe
    info!("📡 Connecting to Binance testnet...");
    feed.connect().await?;
    feed.subscribe(vec![btcusdt, ethusdt]).await?;
    info!("✅ Connected and subscribed");

    // Create channel for updates
    let (tx, mut rx) = mpsc::channel::<L2Update>(10000);

    // Metrics
    let metrics = Arc::new(Metrics::new());
    let _metrics_clone = metrics.clone();

    // Shutdown flag
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();

    // Run feed in background
    let _feed_handle = tokio::spawn(async move {
        if let Err(e) = feed.run(tx).await {
            error!("Feed error: {}", e);
        }
    });

    // Create LOB v2 instances with different strategies
    let mut btc_book = OrderBookV2::new_with_roi(
        btcusdt, 0.01,    // tick size
        0.00001, // lot size
        50000.0, // ROI center (approximate BTC price)
        1000.0,  // ROI width ($1000 range)
    );
    btc_book.set_cross_resolution(CrossResolution::AutoResolve);

    let mut eth_book = OrderBookV2::new_with_roi(
        ethusdt, 0.01,   // tick size
        0.0001, // lot size
        3000.0, // ROI center (approximate ETH price)
        100.0,  // ROI width ($100 range)
    );
    eth_book.set_cross_resolution(CrossResolution::AutoResolve);

    // Processing loop
    info!("📈 Starting order book processing...");

    let mut update_count = 0u64;
    let mut latencies = Vec::with_capacity(10000);
    let mut last_report = Instant::now();
    let mut crossed_detections = 0u64;
    let mut crossed_resolutions = 0u64;

    // Ctrl+C handler
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for Ctrl+C");
        info!("\n⚠️  Shutdown signal received");
        shutdown_clone.store(true, Ordering::Relaxed);
    });

    while !shutdown.load(Ordering::Relaxed) {
        // Receive with timeout
        match tokio::time::timeout(Duration::from_secs(1), rx.recv()).await {
            Ok(Some(update)) => {
                update_count += 1;
                let start = Instant::now();

                // Was book crossed before update?
                let was_crossed = match update.symbol {
                    s if s == btcusdt => btc_book.is_crossed(),
                    s if s == ethusdt => eth_book.is_crossed(),
                    _ => false,
                };

                // Apply update
                let result = match update.symbol {
                    s if s == btcusdt => btc_book.apply_validated(&update),
                    s if s == ethusdt => eth_book.apply_validated(&update),
                    _ => Ok(()),
                };

                // Track metrics
                match result {
                    Ok(()) => {
                        // Check if cross was resolved
                        let is_crossed_now = match update.symbol {
                            s if s == btcusdt => btc_book.is_crossed(),
                            s if s == ethusdt => eth_book.is_crossed(),
                            _ => false,
                        };

                        if was_crossed && !is_crossed_now {
                            crossed_resolutions += 1;
                            debug!("Cross resolved for {:?}", update.symbol);
                        } else if !was_crossed && is_crossed_now {
                            crossed_detections += 1;
                            warn!("New cross detected for {:?}", update.symbol);
                        }
                    }
                    Err(e) => {
                        metrics.validation_errors.fetch_add(1, Ordering::Relaxed);
                        debug!("Validation error: {}", e);
                    }
                }

                let latency = start.elapsed().as_nanos() as u64;
                latencies.push(latency);

                // Log BBO periodically
                if update_count % 100 == 0 {
                    log_book_state("BTCUSDT", &btc_book);
                    log_book_state("ETHUSDT", &eth_book);
                }

                // Report metrics every 5 seconds
                if last_report.elapsed() > Duration::from_secs(5) {
                    metrics
                        .updates_processed
                        .store(update_count, Ordering::Relaxed);
                    metrics
                        .crossed_books_detected
                        .store(crossed_detections, Ordering::Relaxed);
                    metrics
                        .crossed_books_resolved
                        .store(crossed_resolutions, Ordering::Relaxed);
                    metrics.update_latencies(&mut latencies);
                    metrics.report();

                    // Market microstructure analysis
                    analyze_microstructure("BTCUSDT", &btc_book);
                    analyze_microstructure("ETHUSDT", &eth_book);

                    latencies.clear();
                    last_report = Instant::now();
                }

                // Stop after enough updates for demo
                if update_count >= 10000 {
                    info!("📊 Reached 10,000 updates, stopping test");
                    break;
                }
            }
            Ok(None) => {
                info!("Channel closed");
                break;
            }
            Err(_) => {
                // Timeout, continue
            }
        }
    }

    info!("\n🏁 Final Statistics:");
    metrics
        .updates_processed
        .store(update_count, Ordering::Relaxed);
    metrics
        .crossed_books_detected
        .store(crossed_detections, Ordering::Relaxed);
    metrics
        .crossed_books_resolved
        .store(crossed_resolutions, Ordering::Relaxed);
    metrics.report();

    // Final book analysis
    info!("\n📚 Final Order Book State:");
    log_book_state("BTCUSDT", &btc_book);
    log_book_state("ETHUSDT", &eth_book);
    analyze_microstructure("BTCUSDT", &btc_book);
    analyze_microstructure("ETHUSDT", &eth_book);

    info!("\n✅ Test completed successfully!");

    Ok(())
}

fn log_book_state(name: &str, book: &OrderBookV2) {
    info!("📖 {} Order Book:", name);

    if let Some((bid_px, bid_qty)) = book.best_bid() {
        info!(
            "  Best Bid: {:.2} @ {:.4}",
            bid_px.as_f64(),
            bid_qty.as_f64()
        );
    } else {
        info!("  Best Bid: None");
    }

    if let Some((ask_px, ask_qty)) = book.best_ask() {
        info!(
            "  Best Ask: {:.2} @ {:.4}",
            ask_px.as_f64(),
            ask_qty.as_f64()
        );
    } else {
        info!("  Best Ask: None");
    }

    if let Some(mid) = book.mid_price() {
        info!("  Mid Price: {:.2}", mid);
    }

    if let Some(micro) = book.microprice() {
        info!("  Microprice: {:.2}", micro);
    }

    if let Some(spread) = book.spread_ticks() {
        info!("  Spread: {} ticks", spread);
    }
}

fn analyze_microstructure(name: &str, book: &OrderBookV2) {
    info!("🔬 {} Microstructure Analysis:", name);

    // Order book imbalance
    let imbalance_1 = book.imbalance(1);
    let imbalance_5 = book.imbalance(5);
    let imbalance_10 = book.imbalance(10);

    info!("  Imbalance (1 level): {:.4}", imbalance_1);
    info!("  Imbalance (5 levels): {:.4}", imbalance_5);
    info!("  Imbalance (10 levels): {:.4}", imbalance_10);

    // Price prediction signal
    if let (Some(mid), Some(micro)) = (book.mid_price(), book.microprice()) {
        let signal = (micro - mid) / mid * 10000.0; // in bps
        info!("  Price Signal: {:.2} bps", signal);

        if signal.abs() > 1.0 {
            if signal > 0.0 {
                info!("  📈 Bullish pressure detected");
            } else {
                info!("  📉 Bearish pressure detected");
            }
        }
    }

    // Spread analysis
    if let Some(spread_ticks) = book.spread_ticks() {
        if spread_ticks == 1 {
            info!("  ⚡ Tight spread - high liquidity");
        } else if spread_ticks > 10 {
            info!("  ⚠️  Wide spread - low liquidity");
        }
    }
}
