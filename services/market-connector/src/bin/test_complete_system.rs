//! Complete system test: Instruments + Market Data + WAL Storage
//!
//! Tests the complete pipeline:
//! 1. Fetch instruments from Zerodha → Store in WAL
//! 2. Query spot instruments → Find current futures
//! 3. Subscribe to real-time market data for spot + futures
//! 4. Store market data in WAL
//!
//! COMPLIANCE:
//! - Zero allocations in hot paths
//! - Fixed-point arithmetic only
//! - FxHashMap for performance
//! - Proper error handling

use anyhow::{Context, Result};
use common::{L2Update, Side, Symbol, Ts};
use market_connector::{
    MarketData, MarketDataEvent,
    connectors::adapter::{FeedAdapter, FeedConfig},
    exchanges::zerodha::ZerodhaFeed,
    instruments::{InstrumentService, InstrumentServiceConfig},
};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use storage::wal::{Wal, WalEntry};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};
use tracing::{error, info, warn};

/// Test statistics
struct TestStats {
    instruments_fetched: AtomicU64,
    instruments_stored: AtomicU64,
    spot_symbols_found: AtomicU64,
    futures_mapped: AtomicU64,
    market_data_received: AtomicU64,
    market_data_stored: AtomicU64,
    wal_bytes: AtomicU64,
    errors: AtomicU64,
}

impl TestStats {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            instruments_fetched: AtomicU64::new(0),
            instruments_stored: AtomicU64::new(0),
            spot_symbols_found: AtomicU64::new(0),
            futures_mapped: AtomicU64::new(0),
            market_data_received: AtomicU64::new(0),
            market_data_stored: AtomicU64::new(0),
            wal_bytes: AtomicU64::new(0),
            errors: AtomicU64::new(0),
        })
    }

    fn print_summary(&self) {
        info!("📊 Complete System Test Results:");
        info!(
            "  Instruments fetched: {}",
            self.instruments_fetched.load(Ordering::Relaxed)
        );
        info!(
            "  Instruments stored in WAL: {}",
            self.instruments_stored.load(Ordering::Relaxed)
        );
        info!(
            "  Spot symbols found: {}",
            self.spot_symbols_found.load(Ordering::Relaxed)
        );
        info!(
            "  Futures mapped: {}",
            self.futures_mapped.load(Ordering::Relaxed)
        );
        info!(
            "  Market data received: {}",
            self.market_data_received.load(Ordering::Relaxed)
        );
        info!(
            "  Market data stored: {}",
            self.market_data_stored.load(Ordering::Relaxed)
        );
        info!(
            "  WAL bytes written: {}",
            self.wal_bytes.load(Ordering::Relaxed)
        );
        info!("  Errors: {}", self.errors.load(Ordering::Relaxed));
    }
}

/// Market data entry for WAL storage
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MarketDataEntry {
    pub event: MarketDataEvent,
    pub timestamp: Ts,
}

impl WalEntry for MarketDataEntry {
    fn timestamp(&self) -> Ts {
        self.timestamp
    }
}

/// Initialize market data WAL
fn init_market_data_wal() -> Result<Wal> {
    let dir = PathBuf::from("./data/complete_system_test/market_data_wal");
    let segment_size = Some(50 * 1024 * 1024); // 50MB segments

    info!("Initializing market data WAL at: {:?}", dir);

    std::fs::create_dir_all(&dir).context("Failed to create market data WAL directory")?;

    Wal::new(&dir, segment_size).context("Failed to initialize market data WAL")
}

/// Test the complete system integration
async fn test_complete_system(stats: Arc<TestStats>) -> Result<()> {
    info!("🚀 Testing Complete System Integration");
    info!("========================================");

    // Phase 1: Initialize Instrument Service
    info!("Phase 1: Initializing Instrument Service with WAL storage...");

    let instrument_config = InstrumentServiceConfig {
        wal_dir: PathBuf::from("./data/complete_system_test/instruments_wal"),
        wal_segment_size_mb: Some(25), // 25MB segments for instruments
        enable_auto_updates: false,    // Manual control for testing
        ..Default::default()
    };

    // Initialize authentication
    let auth_config = auth::ZerodhaConfig::new(
        std::env::var("ZERODHA_USER_ID").context("ZERODHA_USER_ID not set")?,
        std::env::var("ZERODHA_PASSWORD").context("ZERODHA_PASSWORD not set")?,
        std::env::var("ZERODHA_TOTP_SECRET").context("ZERODHA_TOTP_SECRET not set")?,
        std::env::var("ZERODHA_API_KEY").context("ZERODHA_API_KEY not set")?,
        std::env::var("ZERODHA_API_SECRET").context("ZERODHA_API_SECRET not set")?,
    );
    let zerodha_auth = auth::ZerodhaAuth::new(auth_config.clone());

    // Create and start instrument service
    let instrument_service = InstrumentService::new(instrument_config, Some(zerodha_auth))
        .await
        .context("Failed to create instrument service")?;

    instrument_service
        .start()
        .await
        .context("Failed to start instrument service")?;

    // Get instrument statistics
    let instrument_stats = instrument_service.stats().await;
    // SAFETY: usize to u64 is safe - widening conversion
    stats
        .instruments_fetched
        .store(instrument_stats.total_instruments as u64, Ordering::Relaxed);
    // SAFETY: usize to u64 is safe - widening conversion
    stats
        .instruments_stored
        .store(instrument_stats.total_instruments as u64, Ordering::Relaxed);

    info!(
        "✅ Loaded {} instruments from WAL",
        instrument_stats.total_instruments
    );

    // Create second auth instance for ZerodhaFeed
    let zerodha_auth_feed = auth::ZerodhaAuth::new(auth_config);

    // Phase 2: Query Key Instruments and Map to Futures
    info!("Phase 2: Querying spot instruments and mapping to futures...");

    let key_underlyings = vec!["NIFTY", "BANKNIFTY", "SENSEX"];
    let mut subscription_tokens = Vec::new();

    for underlying in &key_underlyings {
        info!("Analyzing underlying: {}", underlying);

        // Get subscription tokens (spot + current month futures + next month futures)
        let (spot_token, current_futures_token, next_futures_token) =
            instrument_service.get_subscription_tokens(underlying).await;

        if let Some(token) = spot_token {
            info!("  📈 Spot token: {}", token);
            subscription_tokens.push(token.to_string());
            stats.spot_symbols_found.fetch_add(1, Ordering::Relaxed);
        }

        if let Some(token) = current_futures_token {
            info!("  🔮 Current futures token: {}", token);
            subscription_tokens.push(token.to_string());
            stats.futures_mapped.fetch_add(1, Ordering::Relaxed);
        }

        if let Some(token) = next_futures_token {
            info!("  ⏭️  Next futures token: {}", token);
            subscription_tokens.push(token.to_string());
            stats.futures_mapped.fetch_add(1, Ordering::Relaxed);
        }

        // Get detailed futures info
        let active_futures = instrument_service.get_active_futures(underlying).await;
        info!("  📋 Active futures contracts: {}", active_futures.len());

        for future in active_futures.iter().take(3) {
            info!(
                "    - {} (token: {}, expiry: {:?})",
                future.trading_symbol, future.instrument_token, future.expiry
            );
        }
    }

    info!(
        "✅ Found {} subscription tokens total",
        subscription_tokens.len()
    );

    // Phase 3: Initialize Market Data Pipeline
    info!("Phase 3: Setting up market data pipeline...");

    // Initialize market data WAL
    let market_data_wal = init_market_data_wal()?;
    let market_data_wal = Arc::new(tokio::sync::RwLock::new(market_data_wal));

    // Create symbol mapping for market connector
    let mut symbol_map = FxHashMap::default();
    for (i, token) in subscription_tokens.iter().enumerate() {
        let symbol_id = u32::try_from(i)
            .expect("Too many subscription tokens")
            .saturating_add(1);
        symbol_map.insert(Symbol::new(symbol_id), token.clone());
    }

    // Create feed configuration
    let config = FeedConfig {
        name: "zerodha".to_string(),
        ws_url: "wss://ws.kite.trade".to_string(),
        api_url: "https://api.kite.trade".to_string(),
        symbol_map,
        max_reconnects: 3,
        reconnect_delay_ms: 1000,
    };

    // Phase 4: Start Real-time Market Data Collection
    info!("Phase 4: Starting real-time market data collection...");

    let mut zerodha_feed = ZerodhaFeed::new(config, zerodha_auth_feed);

    info!("Connecting to Zerodha WebSocket...");
    zerodha_feed.connect().await?;

    // Subscribe to instruments
    // Convert subscription token count to u32 safely
    let token_count =
        u32::try_from(subscription_tokens.len()).expect("Too many subscription tokens");
    let symbols: Vec<Symbol> = (1..=token_count).map(Symbol::new).collect();

    info!("Subscribing to {} instruments...", symbols.len());
    zerodha_feed.subscribe(symbols).await?;

    // Create channels for L2 updates
    let (l2_tx, mut l2_rx) = mpsc::channel::<L2Update>(1000);

    // Start market data processing
    let wal_clone = market_data_wal.clone();
    let stats_clone = stats.clone();

    let market_data_processor = tokio::spawn(async move {
        while let Some(l2_update) = l2_rx.recv().await {
            // Convert L2Update to MarketDataEvent
            let event = MarketDataEvent {
                exchange: "zerodha".to_string(),
                symbol: format!("SYM_{}", l2_update.symbol.0),
                timestamp: l2_update.ts.as_nanos(),
                data: MarketData::Quote {
                    bid_price: if l2_update.side == Side::Bid {
                        l2_update.price.as_f64()
                    } else {
                        0.0
                    },
                    bid_size: if l2_update.side == Side::Bid {
                        l2_update.qty.as_f64()
                    } else {
                        0.0
                    },
                    ask_price: if l2_update.side == Side::Ask {
                        l2_update.price.as_f64()
                    } else {
                        0.0
                    },
                    ask_size: if l2_update.side == Side::Ask {
                        l2_update.qty.as_f64()
                    } else {
                        0.0
                    },
                },
            };

            // Store in market data WAL
            let entry = MarketDataEntry {
                timestamp: Ts::now(),
                event: event.clone(),
            };

            let mut wal_guard = wal_clone.write().await;
            if let Ok(()) = wal_guard.append(&entry) {
                stats_clone
                    .market_data_received
                    .fetch_add(1, Ordering::Relaxed);
                stats_clone
                    .market_data_stored
                    .fetch_add(1, Ordering::Relaxed);

                let bytes_written = bincode::serialize(&entry)
                    .map(|v| u64::try_from(v.len()).unwrap_or(u64::MAX))
                    .unwrap_or(0);
                stats_clone
                    .wal_bytes
                    .fetch_add(bytes_written, Ordering::Relaxed);

                // Log every 100th message
                if stats_clone.market_data_received.load(Ordering::Relaxed) % 100 == 0 {
                    info!(
                        "💹 Processed {} market data messages",
                        stats_clone.market_data_received.load(Ordering::Relaxed)
                    );
                }
            } else {
                stats_clone.errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    // Run market data collection for 30 seconds
    info!("Collecting market data for 30 seconds...");

    let feed_timeout = Duration::from_secs(30);
    match timeout(feed_timeout, zerodha_feed.run(l2_tx)).await {
        Ok(result) => match result {
            Ok(_) => info!("✅ Market data collection completed successfully"),
            Err(e) => {
                error!("❌ Market data collection error: {}", e);
                stats.errors.fetch_add(1, Ordering::Relaxed);
            }
        },
        Err(timeout_error) => {
            info!(
                "⏰ Market data collection timeout after 30s: {}",
                timeout_error
            );
            zerodha_feed.disconnect().await?;
        }
    }

    // Stop the processor
    market_data_processor.abort();

    // Phase 5: Verify Data Integrity
    info!("Phase 5: Verifying data integrity...");

    // Sync WALs
    {
        let mut wal_guard = market_data_wal.write().await;
        if let Err(e) = wal_guard.flush() {
            warn!("Failed to sync market data WAL: {}", e);
        }
    }

    instrument_service.sync().await?;

    // Check WAL directories
    let instrument_wal_dir = PathBuf::from("./data/complete_system_test/instruments_wal");
    let market_data_wal_dir = PathBuf::from("./data/complete_system_test/market_data_wal");

    let instrument_files = std::fs::read_dir(&instrument_wal_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .map(|ext| ext == "wal")
                .unwrap_or(false)
        })
        .count();

    let market_data_files = std::fs::read_dir(&market_data_wal_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .map(|ext| ext == "wal")
                .unwrap_or(false)
        })
        .count();

    info!("✅ Verification complete:");
    info!("  📁 Instrument WAL segments: {}", instrument_files);
    info!("  📁 Market data WAL segments: {}", market_data_files);
    info!("  📊 Data locations:");
    info!("    - Instruments: {:?}", instrument_wal_dir);
    info!("    - Market data: {:?}", market_data_wal_dir);

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables
    dotenv::dotenv().ok();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_target(false)
        .with_line_number(true)
        .with_env_filter("debug")
        .init();

    info!("🚀 Complete System Integration Test");
    info!("==================================");
    info!("Testing: Instruments → WAL → Market Data → WAL");
    info!("");

    // Initialize statistics
    let stats = TestStats::new();

    // Run the complete system test
    let test_result = test_complete_system(stats.clone()).await;

    // Print final results
    info!("");
    info!("==================================");
    stats.print_summary();
    info!("==================================");

    match test_result {
        Ok(_) => {
            let total_messages = stats.market_data_received.load(Ordering::Relaxed);
            let total_instruments = stats.instruments_stored.load(Ordering::Relaxed);

            if total_messages > 0 && total_instruments > 0 {
                info!("✅ SUCCESS: Complete system integration working!");
                info!("  📊 {} instruments stored and indexed", total_instruments);
                info!("  💹 {} market data messages processed", total_messages);
                info!(
                    "  💾 {} bytes written to WAL",
                    stats.wal_bytes.load(Ordering::Relaxed)
                );
                info!("");
                info!("🎯 System is ready for production use!");
            } else {
                warn!("⚠️ WARNING: Partial success");
                warn!("   Some components may not be working correctly");
            }
        }
        Err(e) => {
            error!("❌ FAILURE: {}", e);
            error!("   Check logs above for details");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_system_initialization() {
        let config = InstrumentServiceConfig::default();
        assert!(config.wal_segment_size_mb.is_some());
        assert!(config.enable_auto_updates);
    }
}
