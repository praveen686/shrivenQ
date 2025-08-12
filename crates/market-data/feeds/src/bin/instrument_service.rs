//! Instrument fetcher service binary
//!
//! Production-grade service for fetching and managing instruments
//! with automatic daily updates and monitoring.

use anyhow::Result;
use auth::{ZerodhaAuth, ZerodhaConfig};
use chrono::Local;
use clap::{Parser, Subcommand};
use feeds::{InstrumentFetcher, InstrumentFetcherConfig};
use std::env;
use std::path::PathBuf;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "instrument-service")]
#[command(about = "Instrument fetcher service for ShrivenQ")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Cache directory
    #[arg(long, default_value = "./cache/instruments")]
    cache_dir: String,

    /// Enable debug logging
    #[arg(long)]
    debug: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the service (default)
    Run {
        /// Run once and exit
        #[arg(long)]
        once: bool,
    },

    /// Fetch instruments immediately
    Fetch,

    /// Show cached instruments
    Show {
        /// Symbol to search
        #[arg(long)]
        symbol: Option<String>,

        /// Show indices only
        #[arg(long)]
        indices: bool,

        /// Show futures for underlying
        #[arg(long)]
        futures: Option<String>,
    },

    /// Validate cache
    Validate,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.debug {
        EnvFilter::from_default_env()
            .add_directive("instrument_service=debug".parse()?)
            .add_directive("feeds=debug".parse()?)
            .add_directive("auth=debug".parse()?)
    } else {
        EnvFilter::from_default_env()
            .add_directive("instrument_service=info".parse()?)
            .add_directive("feeds=info".parse()?)
            .add_directive("auth=info".parse()?)
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .init();

    info!("🚀 ShrivenQ Instrument Service");
    info!("================================");
    info!("Time: {}", Local::now().format("%Y-%m-%d %H:%M:%S IST"));

    // Load environment variables
    dotenv::dotenv().ok();

    // Create configuration
    let config = InstrumentFetcherConfig {
        cache_dir: PathBuf::from(&cli.cache_dir),
        fetch_interval_hours: 24,
        fetch_hour: 8, // 8 AM IST
        max_retries: 3,
        retry_delay_secs: 5,
        enable_zerodha: true,
        enable_binance: false,
    };

    // Setup Zerodha auth if credentials are available
    let zerodha_auth = if env::var("ZERODHA_API_KEY").is_ok() {
        let zerodha_config = ZerodhaConfig::new(
            env::var("ZERODHA_USER_ID").unwrap_or_default(),
            env::var("ZERODHA_PASSWORD").unwrap_or_default(),
            env::var("ZERODHA_TOTP_SECRET").unwrap_or_default(),
            env::var("ZERODHA_API_KEY").unwrap_or_default(),
            env::var("ZERODHA_API_SECRET").unwrap_or_default(),
        )
        .with_cache_dir("./cache/zerodha".to_string());

        Some(ZerodhaAuth::new(zerodha_config))
    } else {
        info!("Zerodha credentials not found, skipping Zerodha instruments");
        None
    };

    // Create fetcher
    let mut fetcher = InstrumentFetcher::new(config, zerodha_auth, None)?;

    // Execute command
    match cli.command.unwrap_or(Commands::Run { once: false }) {
        Commands::Run { once } => {
            if once {
                info!("Running once and exiting");
                fetcher.fetch_all().await?;
                info!("✅ Fetch completed successfully");
            } else {
                info!("Starting continuous service");
                info!("Will fetch instruments daily at 8:00 AM IST");
                fetcher.start().await?;
            }
        }

        Commands::Fetch => {
            info!("Fetching instruments immediately");
            fetcher.fetch_all().await?;
            info!("✅ Fetch completed");

            let store = fetcher.store();
            info!("Total instruments: {}", store.count().await);

            // Show summary
            let indices = store.get_indices().await;
            info!("Indices: {}", indices.len());
            for index in indices.iter().take(5) {
                info!("  - {} ({})", index.trading_symbol, index.instrument_token);
            }
        }

        Commands::Show {
            symbol,
            indices,
            futures,
        } => {
            // Load from cache
            let cache_file = PathBuf::from(&cli.cache_dir).join("instruments.json");
            if !cache_file.exists() {
                error!("No cached instruments found. Run 'fetch' first.");
                return Ok(());
            }

            let store = fetcher.store();
            store.load_from_cache(cache_file.to_str().unwrap()).await?;

            info!("Loaded {} instruments from cache", store.count().await);

            if indices {
                let indices = store.get_indices().await;
                info!("\n📊 Indices ({}):", indices.len());
                for index in indices {
                    info!(
                        "  {} - {} ({})",
                        index.instrument_token, index.trading_symbol, index.name
                    );
                }
            } else if let Some(underlying) = futures {
                let futures = store.get_active_futures(&underlying).await;
                info!(
                    "\n📈 Active futures for {} ({}):",
                    underlying,
                    futures.len()
                );
                for future in futures {
                    info!(
                        "  {} - {} (expires: {})",
                        future.instrument_token,
                        future.trading_symbol,
                        future
                            .expiry
                            .map(|e| e.format("%Y-%m-%d").to_string())
                            .unwrap_or_else(|| "N/A".to_string())
                    );
                }
            } else if let Some(symbol) = symbol {
                let instruments = store.get_by_symbol(&symbol).await;
                if instruments.is_empty() {
                    info!("No instruments found for symbol: {}", symbol);
                } else {
                    info!(
                        "\n🔍 Instruments matching '{}' ({}):",
                        symbol,
                        instruments.len()
                    );
                    for inst in instruments {
                        info!(
                            "  {} - {} ({}, {})",
                            inst.instrument_token, inst.trading_symbol, inst.exchange, inst.segment
                        );
                        info!(
                            "    Type: {:?}, Tick: {}, Lot: {}",
                            inst.instrument_type, inst.tick_size, inst.lot_size
                        );
                    }
                }
            } else {
                // Show summary
                info!("\n📊 Instrument Summary:");
                info!("Total instruments: {}", store.count().await);

                let indices = store.get_indices().await;
                info!("\nIndices: {}", indices.len());
                for index in indices.iter().take(5) {
                    info!("  - {} ({})", index.trading_symbol, index.instrument_token);
                }

                info!("\nUse --symbol, --indices, or --futures to see specific instruments");
            }
        }

        Commands::Validate => {
            let cache_file = PathBuf::from(&cli.cache_dir).join("instruments.json");
            let meta_file = PathBuf::from(&cli.cache_dir).join("metadata.json");

            if !cache_file.exists() {
                error!("Cache file not found: {:?}", cache_file);
                return Ok(());
            }

            info!("Validating cache files...");

            // Load and validate instruments
            let store = fetcher.store();
            store.load_from_cache(cache_file.to_str().unwrap()).await?;
            info!(
                "✅ Instruments file valid: {} instruments",
                store.count().await
            );

            // Check metadata
            if meta_file.exists() {
                let metadata = tokio::fs::read_to_string(&meta_file).await?;
                let _: serde_json::Value = serde_json::from_str(&metadata)?;
                info!("✅ Metadata file valid");

                // Parse and show metadata
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&metadata) {
                    if let Some(last_fetch) = meta.get("last_fetch").and_then(|v| v.as_str()) {
                        info!("Last fetch: {}", last_fetch);
                    }
                    if let Some(venues) = meta.get("venues").and_then(|v| v.as_array()) {
                        info!("Venues: {:?}", venues);
                    }
                }
            } else {
                info!("⚠️ Metadata file not found");
            }

            // Validate some key instruments
            info!("\nValidating key instruments:");

            // Check for NIFTY
            let nifty = store.get_by_symbol("NIFTY").await;
            if !nifty.is_empty() {
                info!("✅ NIFTY found: {} variants", nifty.len());
            } else {
                error!("❌ NIFTY not found");
            }

            // Check for BANKNIFTY
            let banknifty = store.get_by_symbol("BANKNIFTY").await;
            if !banknifty.is_empty() {
                info!("✅ BANKNIFTY found: {} variants", banknifty.len());
            } else {
                error!("❌ BANKNIFTY not found");
            }

            // Check indices
            let indices = store.get_indices().await;
            info!("✅ {} indices found", indices.len());

            info!("\n✅ Cache validation completed");
        }
    }

    Ok(())
}
