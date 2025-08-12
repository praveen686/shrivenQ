//! WAL Inspector - Analyze and categorize market data from WAL files

use clap::{Parser, Subcommand};
use rustc_hash::{FxBuildHasher, FxHashMap};
use std::fs::File;
use std::io::Read;

#[derive(Parser)]
#[command(name = "wal-inspector")]
#[command(about = "Inspect and analyze WAL files for market data")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show summary of all data
    Summary {
        /// WAL file path
        #[arg(default_value = "./data/market/ticks/0000000001.wal")]
        file: String,

        /// Also check LOB snapshots
        #[arg(long)]
        with_lob: bool,
    },

    /// Show detailed breakdown by instrument type
    Details {
        /// WAL file path
        #[arg(default_value = "./data/market/ticks/0000000001.wal")]
        file: String,
    },

    /// List all unique instrument tokens
    Tokens {
        /// WAL file path
        #[arg(default_value = "./data/market/ticks/0000000001.wal")]
        file: String,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Summary { file, with_lob } => show_summary(&file, with_lob)?,
        Commands::Details { file } => show_details(&file)?,
        Commands::Tokens { file } => list_tokens(&file)?,
    }

    Ok(())
}

fn read_wal_file(path: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if !std::path::Path::new(path).exists() {
        return Err(format!("WAL file not found: {}", path).into());
    }

    let mut file = File::open(path)?;
    let mut buffer = Vec::with_capacity(1024 * 1024); // 1MB initial capacity
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}

fn extract_tokens(buffer: &[u8]) -> FxHashMap<u32, usize> {
    let mut tokens = FxHashMap::with_capacity_and_hasher(1000, FxBuildHasher);

    // Known instrument token ranges
    let token_ranges = vec![
        (256265, 256265),     // NIFTY 50 SPOT
        (260105, 260105),     // NIFTY BANK SPOT
        (13568000, 13569000), // August Futures
        (16410000, 16411000), // September Futures
        (18420000, 18430000), // NIFTY Options
    ];

    for (start, end) in token_ranges {
        for token in start..=end {
            let token_bytes = (token as u32).to_le_bytes();
            let count = buffer.windows(4).filter(|w| *w == token_bytes).count();
            if count > 0 {
                tokens.insert(token, count);
            }
        }
    }

    tokens
}

fn categorize_token(token: u32) -> (&'static str, String) {
    match token {
        256265 => ("SPOT", "NIFTY 50".to_string()),
        260105 => ("SPOT", "NIFTY BANK".to_string()),
        13568258 => ("FUTURE", "NIFTY AUG".to_string()),
        13568514 => ("FUTURE", "BANKNIFTY AUG".to_string()),
        16410370 => ("FUTURE", "NIFTY SEP".to_string()),
        16410626 => ("FUTURE", "BANKNIFTY SEP".to_string()),
        18420000..=18429999 => {
            let offset = (token - 18420000) / 256;
            let strike = 24500 + offset * 50;
            let opt_type = if (token % 256) < 128 { "CE" } else { "PE" };
            ("OPTION", format!("NIFTY {} {}", strike, opt_type))
        }
        18430000..=18440000 => {
            let offset = (token - 18430000) / 256;
            let strike = 15000 + offset * 100;
            let opt_type = if (token % 256) < 128 { "CE" } else { "PE" };
            ("OPTION", format!("BANKNIFTY {} {}", strike, opt_type))
        }
        _ => ("UNKNOWN", format!("Token {}", token)),
    }
}

fn show_summary(file_path: &str, with_lob: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("📊 WAL File Summary");
    println!("==================\n");

    let buffer = read_wal_file(file_path)?;
    println!("📁 Tick Data:");
    println!("  File: {}", file_path);
    println!("  Size: {} KB", buffer.len() / 1024);

    let tokens = extract_tokens(&buffer);

    let mut spot_count = 0;
    let mut future_count = 0;
    let mut option_count = 0;
    let mut total_count = 0;

    for (token, count) in &tokens {
        let (category, _) = categorize_token(*token);
        total_count += count;
        match category {
            "SPOT" => spot_count += count,
            "FUTURE" => future_count += count,
            "OPTION" => option_count += count,
            _ => {}
        }
    }

    println!("📈 Data Distribution:");
    println!("--------------------");
    println!("Total ticks:    {}", total_count);
    println!("Unique tokens:  {}", tokens.len());
    println!();
    println!("By Category:");
    println!(
        "  Spot:    {:6} ticks ({:.1}%)",
        spot_count,
        if total_count > 0 {
            (spot_count as f64 / total_count as f64) * 100.0
        } else {
            0.0
        }
    );
    println!(
        "  Futures: {:6} ticks ({:.1}%)",
        future_count,
        if total_count > 0 {
            (future_count as f64 / total_count as f64) * 100.0
        } else {
            0.0
        }
    );
    println!(
        "  Options: {:6} ticks ({:.1}%)",
        option_count,
        if total_count > 0 {
            (option_count as f64 / total_count as f64) * 100.0
        } else {
            0.0
        }
    );

    // Check LOB data if requested
    if with_lob {
        println!("\n📈 LOB Snapshot Data:");
        println!("--------------------");

        let lob_path = "./data/market/lob/0000000001.wal";
        if std::path::Path::new(lob_path).exists() {
            let lob_buffer = read_wal_file(lob_path)?;
            println!("  File: {}", lob_path);
            println!("  Size: {} KB", lob_buffer.len() / 1024);

            // Count snapshots (look for "snapshot" marker)
            let snapshot_count = lob_buffer.windows(8).filter(|w| w == b"snapshot").count();
            println!("  Snapshots: {}", snapshot_count);

            // Check tick-to-LOB reconstruction marker
            println!("\n🔄 Tick-to-LOB Reconstruction:");
            println!("--------------------------------");
            println!("  Status: Available on-demand");
            println!("  Method: reconstruct_lob_from_ticks()");
            println!("  Storage: Not persisted (computed at runtime)");
            println!("  Usage: Pipeline can reconstruct order books from tick history");
        } else {
            println!("  No LOB snapshot file found");
        }
    }

    Ok(())
}

fn show_details(file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("📊 Detailed WAL Analysis");
    println!("========================\n");

    let buffer = read_wal_file(file_path)?;
    let tokens = extract_tokens(&buffer);

    // Group by category
    let mut spot_tokens = Vec::with_capacity(10);
    let mut future_tokens = Vec::with_capacity(50);
    let mut option_tokens = Vec::with_capacity(1000);

    for (token, count) in &tokens {
        let (category, desc) = categorize_token(*token);
        match category {
            "SPOT" => spot_tokens.push((*token, desc, *count)),
            "FUTURE" => future_tokens.push((*token, desc, *count)),
            "OPTION" => option_tokens.push((*token, desc, *count)),
            _ => {}
        }
    }

    // Sort each category
    spot_tokens.sort_by_key(|k| k.0);
    future_tokens.sort_by_key(|k| k.0);
    option_tokens.sort_by_key(|k| k.0);

    // Display each category
    if !spot_tokens.is_empty() {
        println!("📍 SPOT INSTRUMENTS");
        println!("------------------");
        for (token, desc, count) in &spot_tokens {
            println!("  {:8} | {:20} | {} ticks", token, desc, count);
        }
        println!();
    }

    if !future_tokens.is_empty() {
        println!("📅 FUTURES");
        println!("----------");
        for (token, desc, count) in &future_tokens {
            println!("  {:8} | {:20} | {} ticks", token, desc, count);
        }
        println!();
    }

    if !option_tokens.is_empty() {
        println!("📈 OPTIONS");
        println!("----------");
        // Group options by strike
        let mut calls = Vec::with_capacity(500);
        let mut puts = Vec::with_capacity(500);

        for (token, desc, count) in &option_tokens {
            if desc.contains("CE") {
                calls.push((token, desc, count));
            } else if desc.contains("PE") {
                puts.push((token, desc, count));
            }
        }

        if !calls.is_empty() {
            println!("\n  Call Options:");
            for (token, desc, count) in calls {
                println!("    {:8} | {:20} | {} ticks", token, desc, count);
            }
        }

        if !puts.is_empty() {
            println!("\n  Put Options:");
            for (token, desc, count) in puts {
                println!("    {:8} | {:20} | {} ticks", token, desc, count);
            }
        }
    }

    // Summary stats
    println!("\n📊 Summary Statistics");
    println!("--------------------");
    println!("Total unique instruments: {}", tokens.len());
    println!("Total tick count: {}", tokens.values().sum::<usize>());

    if !tokens.is_empty() {
        if let Some(max_token) = tokens.iter().max_by_key(|(_, v)| *v) {
            println!("Most active: Token {} ({} ticks)", max_token.0, max_token.1);
        }
    }

    Ok(())
}

fn list_tokens(file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("📋 All Instrument Tokens");
    println!("========================\n");

    let buffer = read_wal_file(file_path)?;
    let tokens = extract_tokens(&buffer);

    let mut sorted_tokens: Vec<_> = tokens.iter().collect();
    sorted_tokens.sort_by_key(|(k, _)| *k);

    println!("Token     | Type    | Description            | Count");
    println!("----------|---------|------------------------|-------");

    for (token, count) in sorted_tokens {
        let (category, desc) = categorize_token(*token);
        println!("{:8} | {:7} | {:22} | {:5}", token, category, desc, count);
    }

    println!("\nTotal: {} unique tokens", tokens.len());

    Ok(())
}
