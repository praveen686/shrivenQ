//! Production-grade market data service
//!
//! Complete pipeline for fetching, processing, and persisting:
//! - Tick data for spot, futures, and option chains
//! - LOB snapshots and reconstruction
//! - Real-time market data streaming

use anyhow::Result;

use auth::{ZerodhaAuth, ZerodhaConfig};
use chrono::Local;
use clap::{Parser, Subcommand};
use common::instrument::InstrumentStore;
use common::{L2Update, Symbol};
use feeds::display_utils::*;
use feeds::{
    FeedAdapter, FeedConfig, InstrumentFetcher, InstrumentFetcherConfig, MarketDataPipeline,
    PipelineConfig, ZerodhaWebSocketFeed,
};
use rustc_hash::{FxBuildHasher, FxHashMap};
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "market-data-service")]
#[command(about = "ShrivenQ Market Data Service")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Data directory
    #[arg(long, default_value = "./data/market")]
    data_dir: String,

    /// Cache directory
    #[arg(long, default_value = "./cache")]
    cache_dir: String,

    /// Enable debug logging
    #[arg(long)]
    debug: bool,
}

#[derive(Subcommand, Clone)]
enum Commands {
    /// Run the complete market data service
    Run {
        /// Spot symbols to track (comma-separated)
        #[arg(long, default_value = "NIFTY 50,NIFTY BANK")]
        symbols: String,

        /// Number of option strikes above/below spot
        #[arg(long, default_value = "10")]
        strike_range: u32,

        /// Dry run (no actual connections)
        #[arg(long)]
        dry_run: bool,
    },

    /// Fetch latest instruments
    FetchInstruments,

    /// Show subscriptions for a symbol
    ShowSubscriptions {
        /// Symbol to check
        symbol: String,
    },

    /// Replay historical data
    Replay {
        /// Start time (ISO format)
        #[arg(long)]
        start: String,

        /// End time (ISO format)
        #[arg(long)]
        end: String,

        /// Symbol to replay
        #[arg(long)]
        symbol: Option<String>,
    },

    /// Monitor live metrics
    Monitor,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.debug {
        EnvFilter::from_default_env()
            .add_directive("market_data_service=debug".parse()?)
            .add_directive("feeds=debug".parse()?)
            .add_directive("auth=info".parse()?)
            .add_directive("lob=info".parse()?)
            .add_directive("storage=info".parse()?)
    } else {
        EnvFilter::from_default_env()
            .add_directive("market_data_service=info".parse()?)
            .add_directive("feeds=info".parse()?)
            .add_directive("auth=info".parse()?)
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .init();

    info!("🚀 ShrivenQ Market Data Service");
    info!("=================================");
    info!("Time: {}", Local::now().format("%Y-%m-%d %H:%M:%S IST"));

    // Load environment variables
    dotenv::dotenv().ok();

    // Execute command
    let command = cli.command.clone().unwrap_or(Commands::Run {
        symbols: "NIFTY 50,NIFTY BANK".to_string(),
        strike_range: 10,
        dry_run: false,
    });

    match command {
        Commands::Run {
            symbols,
            strike_range,
            dry_run,
        } => {
            run_service(&cli, symbols, strike_range, dry_run).await?;
        }

        Commands::FetchInstruments => {
            fetch_instruments(&cli).await?;
        }

        Commands::ShowSubscriptions { symbol } => {
            show_subscriptions(&cli, &symbol).await?;
        }

        Commands::Replay { start, end, symbol } => {
            replay_data(&cli, &start, &end, symbol.as_deref()).await?;
        }

        Commands::Monitor => {
            monitor_service(&cli).await?;
        }
    }

    Ok(())
}

async fn run_service(cli: &Cli, symbols: String, strike_range: u32, dry_run: bool) -> Result<()> {
    info!("Starting market data service");
    info!("Symbols: {}", symbols);
    info!("Strike range: ±{}", strike_range);

    // Parse symbols
    let spot_symbols: Vec<String> = symbols.split(',').map(|s| s.trim().to_string()).collect();

    // Load or fetch instruments
    let instrument_store = Arc::new(InstrumentStore::new());
    let cache_file = PathBuf::from(&cli.cache_dir).join("instruments/instruments.json");

    if cache_file.exists() {
        info!("Loading instruments from cache");
        instrument_store
            .load_from_cache(
                cache_file
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Invalid cache file path"))?,
            )
            .await?;
        info!("Loaded {} instruments", instrument_store.count().await);
    } else {
        info!("No cached instruments found. Fetching...");
        fetch_instruments(cli).await?;
        instrument_store
            .load_from_cache(
                cache_file
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Invalid cache file path"))?,
            )
            .await?;
    }

    // Create pipeline configuration
    let pipeline_config = PipelineConfig {
        data_dir: PathBuf::from(&cli.data_dir),
        spot_symbols: spot_symbols.clone(),
        option_strike_range: strike_range,
        strike_interval: if symbols.contains(&"BANKNIFTY".to_string()) {
            100.0 // BANKNIFTY has 100 point strike intervals
        } else {
            50.0 // NIFTY 50 has 50 point strike intervals
        },
        wal_segment_size: 100 * 1024 * 1024,
        snapshot_interval_secs: 60,
        enable_reconstruction: true,
        max_queue_size: 100_000,
        enable_compression: true,
    };

    // Create market data pipeline
    let pipeline =
        Arc::new(MarketDataPipeline::new(pipeline_config, instrument_store.clone()).await?);

    // Initialize subscriptions
    pipeline.initialize_subscriptions().await?;

    // Get all tokens to subscribe
    let tokens = pipeline.get_subscribed_tokens().await;
    info!("Total instruments to subscribe: {}", tokens.len());

    if dry_run {
        info!("Dry run mode - not connecting to market data");
        info!("Would subscribe to {} instruments", tokens.len());
        return Ok(());
    }

    // Start the pipeline
    let pipeline_handle = pipeline.clone();
    tokio::spawn(async move {
        if let Err(e) = pipeline_handle.start().await {
            error!("Pipeline error: {}", e);
        }
    });

    // Setup Zerodha authentication
    let zerodha_config = ZerodhaConfig::new(
        env::var("ZERODHA_USER_ID")?,
        env::var("ZERODHA_PASSWORD")?,
        env::var("ZERODHA_TOTP_SECRET")?,
        env::var("ZERODHA_API_KEY")?,
        env::var("ZERODHA_API_SECRET")?,
    )
    .with_cache_dir(format!("{}/zerodha", cli.cache_dir));

    let auth = ZerodhaAuth::new(zerodha_config);

    // Create feed configuration
    let mut symbol_map = FxHashMap::with_capacity_and_hasher(tokens.len(), FxBuildHasher);
    for token in &tokens {
        symbol_map.insert(Symbol::new(*token), token.to_string());
    }

    let feed_config = FeedConfig {
        name: "zerodha".to_string(),
        ws_url: "wss://ws.kite.trade".to_string(),
        api_url: "https://api.kite.trade".to_string(),
        symbol_map,
        max_reconnects: 3,
        reconnect_delay_ms: 5000,
    };

    // Create WebSocket feed
    info!("Connecting to Zerodha WebSocket");
    let mut ws_feed = ZerodhaWebSocketFeed::new(feed_config, auth);

    // Subscribe to all symbols
    let symbols: Vec<Symbol> = tokens.iter().map(|t| Symbol::new(*t)).collect();
    ws_feed.subscribe(symbols).await?;

    // Create channel for receiving updates
    let (tx, mut rx) = mpsc::channel::<L2Update>(10000);

    // Start WebSocket feed
    let ws_handle = tokio::spawn(async move {
        if let Err(e) = ws_feed.run(tx).await {
            error!("WebSocket error: {}", e);
        }
    });

    // Process incoming updates
    info!("Processing market data updates...");
    let mut update_count = 0;

    while let Some(update) = rx.recv().await {
        // Process LOB update
        if let Err(e) = pipeline.process_lob_update(update.clone()).await {
            error!("Failed to process LOB update: {}", e);
            continue;
        }

        // Convert to tick event and persist
        let tick = storage::TickEvent {
            ts: update.ts,
            venue: "zerodha".to_string(),
            symbol: update.symbol,
            bid: if update.side == common::market::Side::Bid {
                Some(update.price)
            } else {
                None
            },
            ask: if update.side == common::market::Side::Ask {
                Some(update.price)
            } else {
                None
            },
            last: None,
            volume: Some(update.qty),
        };

        if let Err(e) = pipeline.process_tick(tick).await {
            error!("Failed to process tick: {}", e);
        }

        update_count += 1;
        if update_count % 1000 == 0 {
            info!("Processed {} updates", update_count);
        }
    }

    // Wait for handles
    ws_handle.await?;

    info!("Market data service stopped");
    Ok(())
}

async fn fetch_instruments(cli: &Cli) -> Result<()> {
    info!("Fetching instruments");

    let zerodha_config = ZerodhaConfig::new(
        env::var("ZERODHA_USER_ID")?,
        env::var("ZERODHA_PASSWORD")?,
        env::var("ZERODHA_TOTP_SECRET")?,
        env::var("ZERODHA_API_KEY")?,
        env::var("ZERODHA_API_SECRET")?,
    )
    .with_cache_dir(format!("{}/zerodha", cli.cache_dir));

    let zerodha_auth = ZerodhaAuth::new(zerodha_config);

    let fetcher_config = InstrumentFetcherConfig {
        cache_dir: PathBuf::from(&cli.cache_dir).join("instruments"),
        fetch_interval_hours: 24,
        fetch_hour: 8,
        max_retries: 3,
        retry_delay_secs: 5,
        enable_zerodha: true,
        enable_binance: false,
    };

    let mut fetcher = InstrumentFetcher::new(fetcher_config, Some(zerodha_auth), None)?;

    fetcher.fetch_all().await?;

    let store = fetcher.store();
    info!("Fetched {} instruments", store.count().await);

    Ok(())
}

async fn show_subscriptions(cli: &Cli, symbol: &str) -> Result<()> {
    info!("Showing subscriptions for: {}", symbol);

    // Load instruments
    let instrument_store = Arc::new(InstrumentStore::new());
    let cache_file = PathBuf::from(&cli.cache_dir).join("instruments/instruments.json");

    if !cache_file.exists() {
        error!("No cached instruments found. Run 'fetch-instruments' first.");
        return Ok(());
    }

    instrument_store
        .load_from_cache(
            cache_file
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid cache file path"))?,
        )
        .await?;

    // Create pipeline to build subscriptions
    let pipeline_config = PipelineConfig {
        data_dir: PathBuf::from(&cli.data_dir),
        spot_symbols: vec![symbol.to_string()],
        option_strike_range: 10,
        strike_interval: 50.0,
        ..Default::default()
    };

    let pipeline = MarketDataPipeline::new(pipeline_config, instrument_store).await?;
    pipeline.initialize_subscriptions().await?;

    let tokens = pipeline.get_subscribed_tokens().await;

    info!("\n📊 Subscription Summary for {}", symbol);
    info!("================================");
    info!("Total instruments: {}", tokens.len());
    info!("\nInstrument tokens:");

    for (i, token) in tokens.iter().enumerate() {
        if i < 20 {
            info!("  {}", token);
        } else if i == 20 {
            info!("  ... and {} more", tokens.len() - 20);
            break;
        }
    }

    Ok(())
}

async fn replay_data(cli: &Cli, start: &str, end: &str, symbol: Option<&str>) -> Result<()> {
    use chrono::{DateTime, Utc};
    use common::Ts;
    use storage::{Wal, WalEvent};

    info!("Starting data replay from {} to {}", start, end);

    if let Some(sym) = symbol {
        info!("Symbol filter: {}", sym);
    }

    // Parse timestamps
    let start_time = DateTime::parse_from_rfc3339(start)
        .map_err(|e| anyhow::anyhow!("Invalid start time format (use ISO format): {}", e))?
        .with_timezone(&Utc);
    let end_time = DateTime::parse_from_rfc3339(end)
        .map_err(|e| anyhow::anyhow!("Invalid end time format (use ISO format): {}", e))?
        .with_timezone(&Utc);

    let start_nanos = start_time
        .timestamp_nanos_opt()
        .ok_or_else(|| anyhow::anyhow!("Invalid start timestamp"))?;
    let start_ts = Ts::from_nanos(
        u64::try_from(start_nanos).map_err(|_| anyhow::anyhow!("Start timestamp is negative"))?,
    );
    let end_nanos = end_time
        .timestamp_nanos_opt()
        .ok_or_else(|| anyhow::anyhow!("Invalid end timestamp"))?;
    let end_ts = Ts::from_nanos(
        u64::try_from(end_nanos).map_err(|_| anyhow::anyhow!("End timestamp is negative"))?,
    );

    info!(
        "Timestamp range: {} to {}",
        start_ts.as_nanos(),
        end_ts.as_nanos()
    );

    // Load instrument store for symbol resolution if filtering is needed
    let instrument_store = if symbol.is_some() {
        let store = Arc::new(InstrumentStore::new());
        let cache_file = PathBuf::from(&cli.cache_dir).join("instruments/instruments.json");

        if cache_file.exists() {
            store
                .load_from_cache(
                    cache_file
                        .to_str()
                        .ok_or_else(|| anyhow::anyhow!("Invalid cache file path"))?,
                )
                .await?;
            info!(
                "Loaded {} instruments for symbol filtering",
                store.count().await
            );
            Some(store)
        } else {
            warn!("No instrument cache found - symbol filtering will be limited");
            None
        }
    } else {
        None
    };

    // Open WAL files for replay
    let data_dir = PathBuf::from(&cli.data_dir);
    let tick_wal_path = data_dir.join("ticks");
    let lob_wal_path = data_dir.join("lob");

    let mut total_events = 0u64;
    let mut tick_events = 0u64;
    let mut lob_events = 0u64;
    let mut skipped_events = 0u64;

    // Replay tick events
    if tick_wal_path.exists() {
        info!("Replaying tick events from {:?}", tick_wal_path);
        let wal = Wal::new(&tick_wal_path, None)?;
        let mut stream = wal.stream::<WalEvent>(Some(start_ts))?;

        while let Some(event) = stream.read_next_entry()? {
            let event_ts = event.timestamp();

            // Stop if we've passed the end time
            if event_ts > end_ts {
                break;
            }

            // Apply symbol filter if specified
            if let Some(sym_filter) = symbol {
                if let WalEvent::Tick(tick) = &event {
                    // Check if this tick matches the symbol filter
                    let mut should_skip = true;

                    // First try to resolve via instrument store
                    if let Some(ref store) = instrument_store {
                        if let Some(instrument) = store.get_by_token(tick.symbol.0).await {
                            if instrument.trading_symbol.contains(sym_filter) {
                                should_skip = false;
                            }
                        }
                    } else {
                        // Fallback to venue check if no instrument store
                        if tick.venue.contains(sym_filter) {
                            should_skip = false;
                        }
                    }

                    if should_skip {
                        skipped_events += 1;
                        continue;
                    }
                }
            }

            tick_events += 1;
            total_events += 1;

            // Log progress every 10000 events
            if total_events % 10000 == 0 {
                info!("Processed {} tick events", tick_events);
            }
        }
        info!("Completed replay of {} tick events", tick_events);
    } else {
        warn!("No tick data found at {:?}", tick_wal_path);
    }

    // Replay LOB events
    if lob_wal_path.exists() {
        info!("Replaying LOB events from {:?}", lob_wal_path);
        let wal = Wal::new(&lob_wal_path, None)?;
        let mut stream = wal.stream::<WalEvent>(Some(start_ts))?;

        while let Some(event) = stream.read_next_entry()? {
            let event_ts = event.timestamp();

            // Stop if we've passed the end time
            if event_ts > end_ts {
                break;
            }

            // Apply symbol filter if specified
            if let Some(sym_filter) = symbol {
                if let WalEvent::Lob(lob_snapshot) = &event {
                    // Check if this LOB snapshot matches the symbol filter
                    let mut should_skip = true;

                    // First try to resolve via instrument store
                    if let Some(ref store) = instrument_store {
                        if let Some(instrument) = store.get_by_token(lob_snapshot.symbol.0).await {
                            if instrument.trading_symbol.contains(sym_filter) {
                                should_skip = false;
                            }
                        }
                    } else {
                        // Fallback to venue check if no instrument store
                        if lob_snapshot.venue.contains(sym_filter) {
                            should_skip = false;
                        }
                    }

                    if should_skip {
                        skipped_events += 1;
                        continue;
                    }
                }
            }

            lob_events += 1;
            total_events += 1;

            // Log progress every 10000 events
            if total_events % 10000 == 0 {
                info!("Processed {} LOB events", lob_events);
            }
        }
        info!("Completed replay of {} LOB events", lob_events);
    } else {
        warn!("No LOB data found at {:?}", lob_wal_path);
    }

    // Summary
    info!("\n📊 Replay Summary");
    info!("=================");
    info!("Time range: {} to {}", start, end);
    if let Some(sym) = symbol {
        info!("Symbol filter: {}", sym);
    }
    info!("Total events replayed: {}", total_events);
    info!("  - Tick events: {}", tick_events);
    info!("  - LOB events: {}", lob_events);
    if skipped_events > 0 {
        info!("  - Skipped (filtered): {}", skipped_events);
    }

    // Convert to seconds for display
    let nanos_diff = end_ts.as_nanos().saturating_sub(start_ts.as_nanos());
    let duration = fmt_nanos_to_secs(nanos_diff);
    if duration > 0.0 && total_events > 0 {
        let events_per_sec = calc_events_per_sec(total_events, duration);
        info!("Replay rate: {:.0} events/second", events_per_sec);
    }

    Ok(())
}

async fn monitor_service(_cli: &Cli) -> Result<()> {
    use std::fs;
    use std::path::Path;
    use std::time::Duration;
    use tokio::time::interval;

    info!("Starting service metrics monitoring dashboard");

    // Monitor data directory size and WAL segments
    let data_dir = Path::new("data");
    let mut refresh_interval = interval(Duration::from_secs(5));

    loop {
        refresh_interval.tick().await;

        // Clear screen for dashboard effect
        print!("\x1B[2J\x1B[1;1H");

        info!("╔══════════════════════════════════════════════════════════╗");
        info!("║       ShrivenQuant Market Data Service Monitor          ║");
        info!("╠══════════════════════════════════════════════════════════╣");
        info!("║ Timestamp: {:?}", chrono::Local::now());
        info!("╠══════════════════════════════════════════════════════════╣");

        // Check WAL directories
        if data_dir.exists() {
            let mut total_size = 0u64;
            let mut wal_count = 0;
            let mut segment_count = 0;
            let mut tick_events = 0u64;
            let mut l2_events = 0u64;

            // Scan data directory
            if let Ok(entries) = fs::read_dir(data_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let dirname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                        // Count different WAL types
                        if dirname.contains("tick") {
                            tick_events += 1;
                        } else if dirname.contains("lob") {
                            l2_events += 1;
                        }
                        wal_count += 1;

                        // Count segments in each WAL
                        if let Ok(wal_entries) = fs::read_dir(&path) {
                            for wal_entry in wal_entries.flatten() {
                                segment_count += 1;
                                if let Ok(metadata) = wal_entry.metadata() {
                                    total_size += metadata.len();
                                }
                            }
                        }
                    }
                }
            }

            info!("║ 📊 Storage Metrics:");
            info!("║   WAL Directories: {}", wal_count);
            info!("║   - Tick WALs:    {}", tick_events);
            info!("║   - LOB WALs:     {}", l2_events);
            info!("║   Total Segments:  {}", segment_count);
            info!("║   Total Size:      {:.2} GB", fmt_bytes_gib(total_size));

            // Calculate average segment size
            if segment_count > 0 {
                let avg_bytes = total_size / segment_count;
                let avg_segment_mb = fmt_bytes_mib(avg_bytes);
                info!("║   Avg Segment:     {:.1} MB", avg_segment_mb);
            }

            info!("║");

            // Check latest modified times for activity
            info!("║ 📈 Data Pipeline Activity:");

            let mut latest_activity = None;
            let mut latest_file = String::from("");

            if let Ok(entries) = fs::read_dir(data_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        // Check segments within each WAL
                        if let Ok(wal_entries) = fs::read_dir(&path) {
                            for wal_entry in wal_entries.flatten() {
                                if let Ok(metadata) = wal_entry.metadata() {
                                    if let Ok(modified) = metadata.modified() {
                                        if latest_activity.map_or(true, |last| modified > last) {
                                            latest_activity = Some(modified);
                                            latest_file = wal_entry
                                                .path()
                                                .file_name()
                                                .and_then(|n| n.to_str())
                                                .unwrap_or("")
                                                .to_string();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(last_mod) = latest_activity {
                if let Ok(elapsed) = last_mod.elapsed() {
                    info!("║   Last Update:     {:?} ago", elapsed);
                    info!("║   Latest File:     {}", latest_file);

                    // Check if pipeline is stale
                    if elapsed > Duration::from_secs(300) {
                        info!("║   Status:          ❌ DEAD (no updates > 5min)");
                    } else if elapsed > Duration::from_secs(60) {
                        info!("║   Status:          ⚠️  STALE (no updates > 60s)");
                    } else if elapsed > Duration::from_secs(10) {
                        info!("║   Status:          ⚡ SLOW (no updates > 10s)");
                    } else {
                        info!("║   Status:          ✅ ACTIVE");
                    }
                }
            } else {
                info!("║   Status:          ❌ NO DATA");
            }

            // Performance metrics
            info!("║");
            info!("║ ⚡ Performance Targets:");
            info!("║   LOB Updates:     > 200k/sec");
            info!("║   WAL Writes:      > 80 MB/s");
            info!("║   Replay Speed:    > 3M events/min");
            info!("║   Apply p50:       < 200ns");
            info!("║   Apply p99:       < 900ns");

            // System info
            info!("║");
            info!("║ 💻 System Info:");

            // Memory usage approximation
            if let Ok(contents) = fs::read_to_string("/proc/self/status") {
                for line in contents.lines() {
                    if line.starts_with("VmRSS:") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            if let Ok(kb) = parts[1].parse::<u64>() {
                                info!("║   Memory Usage:    {:.1} MB", fmt_kb_to_mb(kb));
                            }
                        }
                    }
                }
            }
        } else {
            info!("║ ❌ Data directory not found!");
            info!("║");
            info!("║ Run with 'run' command first to start collecting data");
        }

        info!("╠══════════════════════════════════════════════════════════╣");
        info!("║ Press Ctrl+C to exit                                    ║");
        info!("╚══════════════════════════════════════════════════════════╝");
    }
}
