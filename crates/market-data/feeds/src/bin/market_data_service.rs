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
use feeds::{
    FeedAdapter, FeedConfig, InstrumentFetcher, InstrumentFetcherConfig, MarketDataPipeline,
    PipelineConfig, ZerodhaWebSocketFeed,
};
use std::collections::HashMap;
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
            .load_from_cache(cache_file.to_str().unwrap())
            .await?;
        info!("Loaded {} instruments", instrument_store.count().await);
    } else {
        info!("No cached instruments found. Fetching...");
        fetch_instruments(cli).await?;
        instrument_store
            .load_from_cache(cache_file.to_str().unwrap())
            .await?;
    }

    // Create pipeline configuration
    let pipeline_config = PipelineConfig {
        data_dir: PathBuf::from(&cli.data_dir),
        spot_symbols: spot_symbols.clone(),
        option_strike_range: strike_range,
        strike_interval: 50.0, // TODO: Make configurable per symbol
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
    let mut symbol_map = HashMap::new();
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
        .load_from_cache(cache_file.to_str().unwrap())
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

async fn replay_data(_cli: &Cli, start: &str, end: &str, symbol: Option<&str>) -> Result<()> {
    info!("Replaying data from {} to {}", start, end);

    if let Some(sym) = symbol {
        info!("Symbol filter: {}", sym);
    }

    // TODO: Implement replay functionality
    warn!("Replay functionality not yet implemented");

    Ok(())
}

async fn monitor_service(_cli: &Cli) -> Result<()> {
    info!("Monitoring service metrics");

    // TODO: Implement monitoring dashboard
    warn!("Monitoring functionality not yet implemented");

    Ok(())
}
