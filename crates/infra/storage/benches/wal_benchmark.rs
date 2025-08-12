//! Performance benchmarks for WAL operations

#![allow(clippy::expect_used)] // Benchmarks can use expect for simplicity
#![allow(clippy::cast_precision_loss)] // Acceptable for test data generation

use common::{Px, Qty, Symbol, Ts};
use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use storage::{TickEvent, Wal, WalEvent};
use tempfile::TempDir;

fn create_test_event(i: u64) -> WalEvent {
    WalEvent::Tick(TickEvent {
        ts: Ts::from_nanos(i * 1000),
        venue: "benchmark".to_string(),
        symbol: Symbol::new((i % 100) as u32),
        bid: Some(Px::new((i as f64).mul_add(0.01, 100.0))),
        ask: Some(Px::new((i as f64).mul_add(0.01, 100.5))),
        last: Some(Px::new((i as f64).mul_add(0.01, 100.25))),
        volume: Some(Qty::new(1000.0 + i as f64)),
    })
}

fn benchmark_sequential_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("wal_write");

    for size in &[100, 1000, 10000] {
        group.throughput(Throughput::Elements(*size));
        group.bench_function(format!("sequential_{size}"), |b| {
            b.iter_with_setup(
                || {
                    // Benchmarks need to fail if setup fails - we can't measure performance without proper setup
                    let temp_dir = match TempDir::new() {
                        Ok(dir) => dir,
                        Err(e) => {
                            eprintln!("Benchmark setup failed: Could not create temp dir: {}", e);
                            std::process::exit(1);
                        }
                    };
                    let wal = match Wal::new(temp_dir.path(), Some(128 * 1024 * 1024)) {
                        Ok(wal) => wal,
                        Err(e) => {
                            eprintln!("Benchmark setup failed: Could not create WAL: {}", e);
                            std::process::exit(1);
                        }
                    };
                    (temp_dir, wal)
                },
                |(_temp_dir, mut wal)| {
                    for i in 0..*size {
                        let event = create_test_event(i);
                        if let Err(e) = wal.append(&event) {
                            eprintln!("Benchmark error: Failed to append event: {}", e);
                            return;
                        }
                    }
                    if let Err(e) = wal.flush() {
                        eprintln!("Benchmark error: Failed to flush WAL: {}", e);
                        return;
                    }
                },
            );
        });
    }

    group.finish();
}

fn benchmark_read_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("wal_read");

    for size in &[1000, 10000] {
        group.throughput(Throughput::Elements(*size));
        group.bench_function(format!("stream_{size}"), |b| {
            // Setup: create WAL with events
            let temp_dir = match TempDir::new() {
                Ok(dir) => dir,
                Err(e) => {
                    eprintln!("Benchmark setup error: Failed to create temp dir: {}", e);
                    return;
                }
            };
            let wal_path = temp_dir.path();

            {
                let mut wal = match Wal::new(wal_path, Some(128 * 1024 * 1024)) {
                    Ok(w) => w,
                    Err(e) => {
                        eprintln!("Benchmark setup error: Failed to create WAL: {}", e);
                        return;
                    }
                };
                for i in 0..*size {
                    let event = create_test_event(i);
                    if let Err(e) = wal.append(&event) {
                        eprintln!("Benchmark setup error: Failed to append event: {}", e);
                        return;
                    }
                }
                if let Err(e) = wal.flush() {
                    eprintln!("Benchmark setup error: Failed to flush WAL: {}", e);
                    return;
                }
            }

            b.iter(|| {
                let wal = match Wal::new(wal_path, Some(128 * 1024 * 1024)) {
                    Ok(w) => w,
                    Err(e) => {
                        eprintln!("Benchmark error: Failed to create WAL: {}", e);
                        return;
                    }
                };
                let mut iter = match wal.stream::<WalEvent>(None) {
                    Ok(it) => it,
                    Err(e) => {
                        eprintln!("Benchmark error: Failed to create stream: {}", e);
                        return;
                    }
                };

                let mut count = 0;
                loop {
                    match iter.read_next_entry() {
                        Ok(Some(event)) => {
                            black_box(event);
                            count += 1;
                        }
                        Ok(None) => break,
                        Err(e) => {
                            eprintln!("Benchmark error: Failed to read event: {}", e);
                            return;
                        }
                    }
                }
                assert_eq!(count, *size);
            });
        });
    }

    group.finish();
}

fn benchmark_append_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("wal_latency");

    group.bench_function("single_append", |b| {
        let temp_dir = match TempDir::new() {
            Ok(dir) => dir,
            Err(e) => {
                eprintln!("Benchmark setup error: Failed to create temp dir: {}", e);
                return;
            }
        };
        let mut wal = match Wal::new(temp_dir.path(), Some(128 * 1024 * 1024)) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Benchmark setup error: Failed to create WAL: {}", e);
                return;
            }
        };
        let event = create_test_event(0);

        b.iter(|| {
            if let Err(e) = wal.append(&event) {
                eprintln!("Benchmark error: Failed to append event: {}", e);
                return;
            }
        });
    });

    group.bench_function("append_with_flush", |b| {
        let temp_dir = match TempDir::new() {
            Ok(dir) => dir,
            Err(e) => {
                eprintln!("Benchmark setup error: Failed to create temp dir: {}", e);
                return;
            }
        };
        let mut wal = match Wal::new(temp_dir.path(), Some(128 * 1024 * 1024)) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Benchmark setup error: Failed to create WAL: {}", e);
                return;
            }
        };
        let event = create_test_event(0);

        b.iter(|| {
            if let Err(e) = wal.append(&event) {
                eprintln!("Benchmark error: Failed to append event: {}", e);
                return;
            }
            if let Err(e) = wal.flush() {
                eprintln!("Benchmark error: Failed to flush WAL: {}", e);
                return;
            }
        });
    });

    group.finish();
}

fn benchmark_segment_rotation(c: &mut Criterion) {
    let mut group = c.benchmark_group("wal_rotation");

    group.bench_function("small_segments", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = match TempDir::new() {
                    Ok(dir) => dir,
                    Err(e) => {
                        eprintln!("Benchmark setup failed: Could not create temp dir: {}", e);
                        std::process::exit(1);
                    }
                };
                let wal = match Wal::new(temp_dir.path(), Some(10 * 1024)) { // 10KB segments
                    Ok(w) => w,
                    Err(e) => {
                        eprintln!("Benchmark setup failed: Could not create WAL: {}", e);
                        std::process::exit(1);
                    }
                };
                (temp_dir, wal)
            },
            |(_temp_dir, mut wal)| {
                // Write enough to trigger multiple rotations
                for i in 0..1000 {
                    let event = create_test_event(i);
                    if let Err(e) = wal.append(&event) {
                        eprintln!("Benchmark error: Failed to append event: {}", e);
                        return;
                    }
                }
            },
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_sequential_write,
    benchmark_read_throughput,
    benchmark_append_latency,
    benchmark_segment_rotation
);
criterion_main!(benches);
