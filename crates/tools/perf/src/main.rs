//! `ShrivenQ` Performance Benchmarks

#![allow(clippy::print_stdout)] // This is a CLI tool that needs to print output
#![allow(clippy::print_stderr)] // This is a CLI tool that needs to print errors
#![allow(clippy::uninlined_format_args)] // Format args are fine for CLI output
#![allow(clippy::cast_precision_loss)] // Acceptable for benchmarking calculations
#![allow(clippy::cast_possible_truncation)] // Acceptable for benchmarking
#![allow(clippy::cast_sign_loss)] // Acceptable for benchmarking
#![allow(clippy::needless_pass_by_value)] // PathBuf can be passed by value for simplicity
#![allow(clippy::collapsible_if)] // Sometimes clearer to keep separate
#![allow(clippy::manual_flatten)] // Sometimes clearer as is
#![allow(clippy::redundant_closure_for_method_calls)] // Sometimes clearer
#![allow(clippy::unnecessary_debug_formatting)] // PathBuf::display() doesn't work as expected
#![allow(clippy::needless_borrows_for_generic_args)] // Sometimes clearer
#![allow(clippy::redundant_clone)] // Sometimes needed for borrow checker

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use hdrhistogram::Histogram;
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::{fs, path::PathBuf, time::Instant};
use tracing::{info, error, warn};
use tracing_subscriber::EnvFilter;

mod wire;
use wire::{SyntheticEvent, open_reader, open_writer};

#[derive(Parser, Debug)]
#[command(name = "sq-perf", about = "ShrivenQ WAL & Replay benchmarks")]
struct Cli {
    #[arg(long, default_value = "info")]
    log: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Generate/write synthetic events into the WAL and report write throughput/latency
    Write {
        #[arg(long)]
        path: PathBuf,
        #[arg(long, default_value_t = 1_000_000)]
        events: u64,
        #[arg(long, default_value_t = 128)]
        record_size: usize,
        #[arg(long, default_value_t = 256)]
        segment_mb: usize,
        /// fsync every N ms; omit for "never"
        #[arg(long)]
        fsync_ms: Option<u64>,
    },
    /// Replay from WAL and report throughput and latency histogram
    Replay {
        #[arg(long)]
        path: PathBuf,
        #[arg(long)]
        expect: Option<u64>,
    },
    /// Flip random bytes near WAL tail to test corruption handling
    Corrupt {
        #[arg(long)]
        path: PathBuf,
        /// number of bytes to flip
        #[arg(long, default_value_t = 4)]
        flip: usize,
    },
    /// Time the recovery path (open + scan tail)
    Recover {
        #[arg(long)]
        path: PathBuf,
    },
    /// Probe index: seek to random timestamps N times
    Seek {
        #[arg(long)]
        path: PathBuf,
        #[arg(long, default_value_t = 10)]
        probes: usize,
    },
    /// Verify deterministic replay
    Verify {
        #[arg(long)]
        path: PathBuf,
    },
    /// Run all benchmarks and report pass/fail
    All {
        #[arg(long)]
        path: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(cli.log))
        .with_target(false)
        .compact()
        .init();

    match cli.cmd {
        Cmd::Write {
            path,
            events,
            record_size,
            segment_mb,
            fsync_ms,
        } => cmd_write(path, events, record_size, segment_mb, fsync_ms),
        Cmd::Replay { path, expect } => cmd_replay(path, expect),
        Cmd::Corrupt { path, flip } => cmd_corrupt(path, flip),
        Cmd::Recover { path } => cmd_recover(path),
        Cmd::Seek { path, probes } => cmd_seek(path, probes),
        Cmd::Verify { path } => cmd_verify(path),
        Cmd::All { path } => cmd_all(path),
    }
}

fn cmd_write(
    path: PathBuf,
    events: u64,
    record_size: usize,
    segment_mb: usize,
    fsync_ms: Option<u64>,
) -> Result<()> {
    fs::create_dir_all(&path).ok();
    let segment_bytes = segment_mb * 1024 * 1024;
    let mut w = open_writer(&path, segment_bytes, fsync_ms).context("open_writer")?;

    let mut rng = StdRng::seed_from_u64(42);
    let mut latency_histogram = Histogram::<u64>::new(3)?;

    let t0 = Instant::now();
    for i in 0..events {
        let mut payload = vec![0u8; record_size];
        rng.fill(&mut payload[..]);
        // embed event index in first 8 bytes for sanity
        if record_size >= 8 {
            payload[..8].copy_from_slice(&i.to_le_bytes());
        }

        let ev = SyntheticEvent {
            ts_ns: now_ns(),
            payload,
        };
        let a0 = Instant::now();
        w.append_synth(&ev)?;
        let us = a0.elapsed().as_micros() as u64;
        let _ = latency_histogram.record(us);
        if i % 100_000 == 0 && i > 0 {
            w.flush()?;
        }
    }
    w.flush()?;
    let dt = t0.elapsed().as_secs_f64();

    let total_bytes = (events as usize) * record_size;
    let mb = total_bytes as f64 / (1024.0 * 1024.0);

    info!("\n=== WRITE BENCHMARK ===");
    info!("Events: {}", events);
    info!("Record size: {} bytes", record_size);
    info!("Segment size: {} MB", segment_mb);
    info!("Fsync interval: {:?} ms", fsync_ms);
    info!("\n--- Results ---");
    info!("Total: {:.2} MB in {:.3}s", mb, dt);
    info!("Throughput: {:.2} MB/s", mb / dt);
    info!("Event rate: {:.0} events/sec", events as f64 / dt);
    print_hist("Write latency", &latency_histogram);

    // Pass/Fail check
    let pass_mb_s = 80.0;
    let pass_p50 = 120;
    let pass_p99 = 700;
    let p50 = latency_histogram.value_at_percentile(50.0);
    let p99 = latency_histogram.value_at_percentile(99.0);

    info!("\n--- Pass/Fail (Dev Laptop Target) ---");
    info!(
        "Throughput: {} (target ≥ {} MB/s)",
        if mb / dt >= pass_mb_s {
            "✅ PASS"
        } else {
            "❌ FAIL"
        },
        pass_mb_s
    );
    info!(
        "Latency p50: {} (target ≤ {} µs)",
        if p50 <= pass_p50 {
            "✅ PASS"
        } else {
            "❌ FAIL"
        },
        pass_p50
    );
    info!(
        "Latency p99: {} (target ≤ {} µs)",
        if p99 <= pass_p99 {
            "✅ PASS"
        } else {
            "❌ FAIL"
        },
        pass_p99
    );

    Ok(())
}

fn cmd_replay(path: PathBuf, expect: Option<u64>) -> Result<()> {
    let r = open_reader(&path).context("open_reader")?;

    let mut latency_histogram = Histogram::<u64>::new(3)?;
    let mut count: u64 = 0;

    let t0 = Instant::now();
    let mut last_event_time = Instant::now();

    let n = r.replay(|_ts_ns, _len| {
        let us = last_event_time.elapsed().as_micros() as u64;
        if us > 0 {
            // Skip first event
            let _ = latency_histogram.record(us);
        }
        last_event_time = Instant::now();
        count += 1;
    })?;
    let dt = t0.elapsed().as_secs_f64();

    if let Some(e) = expect {
        if n != e {
            error!("❌ Count mismatch: got {} expected {}", n, e);
        }
    }

    let eps = n as f64 / dt;
    let events_per_minute = eps * 60.0;

    info!("\n=== REPLAY BENCHMARK ===");
    info!("Events: {}", n);
    info!("Time: {:.3}s", dt);
    info!("\n--- Results ---");
    info!("Throughput: {:.0} events/sec", eps);
    info!("Throughput: {:.2}M events/min", events_per_minute / 1e6);
    print_hist("Replay latency", &latency_histogram);

    // Pass/Fail check
    let pass_epm = 3_000_000.0; // 3M/min
    let pass_p50 = 150;
    let pass_p99 = 900;
    let p50 = latency_histogram.value_at_percentile(50.0);
    let p99 = latency_histogram.value_at_percentile(99.0);

    info!("\n--- Pass/Fail (Dev Laptop Target) ---");
    info!(
        "Throughput: {} (target ≥ {}M events/min)",
        if events_per_minute >= pass_epm {
            "✅ PASS"
        } else {
            "❌ FAIL"
        },
        pass_epm / 1e6
    );
    info!(
        "Latency p50: {} (target ≤ {} µs)",
        if p50 <= pass_p50 {
            "✅ PASS"
        } else {
            "❌ FAIL"
        },
        pass_p50
    );
    info!(
        "Latency p99: {} (target ≤ {} µs)",
        if p99 <= pass_p99 {
            "✅ PASS"
        } else {
            "❌ FAIL"
        },
        pass_p99
    );

    Ok(())
}

fn cmd_corrupt(path: PathBuf, flip: usize) -> Result<()> {
    // Find WAL segment files (*.wal extension)
    let mut segs = std::fs::read_dir(&path)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("wal"))
        .collect::<Vec<_>>();
    segs.sort();

    let Some(last) = segs.last() else {
        anyhow::bail!("No segment files found in {:?}", path);
    };

    let mut data = std::fs::read(&last).context("read last segment")?;
    if data.len() < flip {
        anyhow::bail!("Segment too small");
    }

    let mut rng = StdRng::seed_from_u64(1337);
    info!("\n=== CORRUPTION TEST ===");
    info!("Target: {:?}", last);
    info!("Flipping {} bytes", flip);

    for _ in 0..flip {
        let idx = rng.gen_range(0..data.len());
        info!("Flipping byte at offset {:#x}", idx);
        data[idx] ^= 0xFF;
    }

    std::fs::write(&last, &data).context("write corrupted segment")?;
    info!("✅ Corruption injected successfully");

    // Now try to read and see if CRC catches it
    info!("\nAttempting to read corrupted WAL...");
    match open_reader(&path) {
        Ok(r) => {
            let result = r.replay(|_, _| {});
            match result {
                Ok(n) => info!("⚠️  Read {} events (CRC may have skipped corrupt ones)", n),
                Err(e) => info!("✅ CRC detected corruption: {}", e),
            }
        }
        Err(e) => info!("✅ Failed to open corrupted WAL: {}", e),
    }

    Ok(())
}

fn cmd_recover(path: PathBuf) -> Result<()> {
    info!("\n=== RECOVERY BENCHMARK ===");

    // Get WAL size
    let mut total_size = 0u64;
    for entry in std::fs::read_dir(&path)? {
        if let Ok(e) = entry {
            if let Ok(meta) = e.metadata() {
                total_size += meta.len();
            }
        }
    }
    let gb = total_size as f64 / (1024.0 * 1024.0 * 1024.0);

    info!("WAL size: {:.2} GB", gb);

    let t0 = Instant::now();
    // Open reader to validate recovery time
    open_reader(&path).context("open_reader")?;
    let ms = t0.elapsed().as_millis();

    info!("Recovery time: {} ms", ms);

    // Pass/Fail check (1.5s per 10GB)
    let expected_ms = (gb / 10.0 * 1500.0) as u128;
    info!("\n--- Pass/Fail ---");
    info!(
        "Recovery: {} (target ≤ {} ms for {:.2} GB)",
        if ms <= expected_ms {
            "✅ PASS"
        } else {
            "❌ FAIL"
        },
        expected_ms,
        gb
    );

    Ok(())
}

fn cmd_seek(path: PathBuf, probes: usize) -> Result<()> {
    info!("\n=== SEEK BENCHMARK ===");

    // First, get timestamp range
    let reader = open_reader(&path)?;
    let mut first_ts = 0u64;
    let mut last_ts = 0u64;
    let mut count = 0u64;

    reader.replay(|ts, _| {
        if count == 0 {
            first_ts = ts;
        }
        last_ts = ts;
        count += 1;
    })?;

    info!("WAL contains {} events", count);
    info!("Timestamp range: {} to {}", first_ts, last_ts);

    let mut rng = StdRng::seed_from_u64(99);
    let mut latency_histogram = Histogram::<u64>::new(3)?;

    info!("\nRunning {} seek probes...", probes);

    for i in 0..probes {
        let target_ts = rng.gen_range(first_ts..=last_ts);
        let pct = ((target_ts - first_ts) as f64 / (last_ts - first_ts) as f64 * 100.0) as u32;

        let t0 = Instant::now();
        let r = open_reader(&path)?;
        r.seek_to(target_ts)?;
        let us = t0.elapsed().as_micros() as u64;
        let _ = latency_histogram.record(us / 1000); // Convert to ms for display

        info!("  Probe {}: seek to {}% took {} ms", i + 1, pct, us / 1000);
    }

    print_hist("Seek time (ms)", &latency_histogram);

    // Pass/Fail check (40ms target)
    let p99 = latency_histogram.value_at_percentile(99.0);
    info!("\n--- Pass/Fail ---");
    info!(
        "Seek p99: {} (target ≤ 40 ms)",
        if p99 <= 40 { "✅ PASS" } else { "❌ FAIL" }
    );

    Ok(())
}

fn cmd_verify(path: PathBuf) -> Result<()> {
    info!("\n=== DETERMINISTIC REPLAY VERIFICATION ===");

    // Read events twice and compare
    let r1 = open_reader(&path)?;
    let r2 = open_reader(&path)?;

    let mut events1 = Vec::with_capacity(1000);
    let mut events2 = Vec::with_capacity(1000);

    r1.replay(|ts, len| {
        events1.push((ts, len));
    })?;

    r2.replay(|ts, len| {
        events2.push((ts, len));
    })?;

    info!("First read: {} events", events1.len());
    info!("Second read: {} events", events2.len());

    if events1.len() != events2.len() {
        info!("❌ FAIL: Event count mismatch!");
        return Ok(());
    }

    let mut mismatches = 0;
    for (i, (e1, e2)) in events1.iter().zip(events2.iter()).enumerate() {
        if e1 != e2 {
            if mismatches < 10 {
                info!("  Mismatch at event {}: {:?} vs {:?}", i, e1, e2);
            }
            mismatches += 1;
        }
    }

    if mismatches > 0 {
        info!("❌ FAIL: {} events differ between reads", mismatches);
    } else {
        info!(
            "✅ PASS: All {} events identical across reads",
            events1.len()
        );
    }

    Ok(())
}

fn cmd_all(path: PathBuf) -> Result<()> {
    let separator = "=".repeat(60);
    info!("\n{}", separator);
    info!("RUNNING FULL SPRINT 2 VERIFICATION SUITE");
    info!("{}", separator);

    // Clean start
    let _ = std::fs::remove_dir_all(&path);

    // 1. Write test
    info!("\n[1/5] Write Performance Test");
    cmd_write(path.clone(), 1_000_000, 128, 256, Some(100))?;

    // 2. Replay test
    info!("\n[2/5] Replay Performance Test");
    cmd_replay(path.clone(), Some(1_000_000))?;

    // 3. Recovery test
    info!("\n[3/5] Recovery Time Test");
    cmd_recover(path.clone())?;

    // 4. Determinism test
    info!("\n[4/5] Deterministic Replay Test");
    cmd_verify(path.clone())?;

    // 5. Seek test
    info!("\n[5/5] Seek Performance Test");
    cmd_seek(path.clone(), 10)?;

    let separator = "=".repeat(60);
    info!("\n{}", separator);
    info!("SPRINT 2 VERIFICATION COMPLETE");
    info!("{}", separator);

    Ok(())
}

#[inline]
fn now_ns() -> u64 {
    // Use a monotonic counter for synthetic timestamps (not wall-clock)
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let s = *START.get_or_init(Instant::now);
    s.elapsed().as_nanos() as u64
}

fn print_hist(name: &str, h: &Histogram<u64>) {
    if h.is_empty() {
        info!("{}: No data", name);
        return;
    }

    let p50 = h.value_at_percentile(50.0);
    let p95 = h.value_at_percentile(95.0);
    let p99 = h.value_at_percentile(99.0);
    let p999 = h.value_at_percentile(99.9);
    let max = h.max();

    info!(
        "{}: p50={}µs p95={}µs p99={}µs p99.9={}µs max={}µs",
        name, p50, p95, p99, p999, max
    );
}
