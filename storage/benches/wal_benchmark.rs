//! Performance benchmarks for WAL operations

use common::{Px, Qty, Symbol, Ts};
use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use storage::{TickEvent, Wal, WalEvent};
use tempfile::TempDir;

fn create_test_event(i: u64) -> WalEvent {
    WalEvent::Tick(TickEvent {
        ts: Ts::from_nanos(i * 1000),
        venue: "benchmark".to_string(),
        symbol: Symbol::new((i % 100) as u32),
        bid: Some(Px::new(100.0 + (i as f64 * 0.01))),
        ask: Some(Px::new(100.5 + (i as f64 * 0.01))),
        last: Some(Px::new(100.25 + (i as f64 * 0.01))),
        volume: Some(Qty::new(1000.0 + i as f64)),
    })
}

fn benchmark_sequential_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("wal_write");

    for size in &[100, 1000, 10000] {
        group.throughput(Throughput::Elements(*size));
        group.bench_function(format!("sequential_{}", size), |b| {
            b.iter_with_setup(
                || {
                    let temp_dir = TempDir::new().expect("Failed to create temp dir");
                    let wal = Wal::new(temp_dir.path(), Some(128 * 1024 * 1024))
                        .expect("Failed to create WAL");
                    (temp_dir, wal)
                },
                |(_temp_dir, mut wal)| {
                    for i in 0..*size {
                        let event = create_test_event(i);
                        wal.append(&event).expect("Failed to append event");
                    }
                    wal.flush().expect("Failed to flush WAL");
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
        group.bench_function(format!("stream_{}", size), |b| {
            // Setup: create WAL with events
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let wal_path = temp_dir.path();

            {
                let mut wal =
                    Wal::new(wal_path, Some(128 * 1024 * 1024)).expect("Failed to create WAL");
                for i in 0..*size {
                    let event = create_test_event(i);
                    wal.append(&event).expect("Failed to append event");
                }
                wal.flush().expect("Failed to flush WAL");
            }

            b.iter(|| {
                let wal =
                    Wal::new(wal_path, Some(128 * 1024 * 1024)).expect("Failed to create WAL");
                let mut iter = wal
                    .stream::<WalEvent>(None)
                    .expect("Failed to create stream");

                let mut count = 0;
                while let Some(event) = iter.next().expect("Failed to read event") {
                    black_box(event);
                    count += 1;
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
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut wal =
            Wal::new(temp_dir.path(), Some(128 * 1024 * 1024)).expect("Failed to create WAL");
        let event = create_test_event(0);

        b.iter(|| {
            wal.append(&event).expect("Failed to append event");
        });
    });

    group.bench_function("append_with_flush", |b| {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut wal =
            Wal::new(temp_dir.path(), Some(128 * 1024 * 1024)).expect("Failed to create WAL");
        let event = create_test_event(0);

        b.iter(|| {
            wal.append(&event).expect("Failed to append event");
            wal.flush().expect("Failed to flush WAL");
        });
    });

    group.finish();
}

fn benchmark_segment_rotation(c: &mut Criterion) {
    let mut group = c.benchmark_group("wal_rotation");

    group.bench_function("small_segments", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().expect("Failed to create temp dir");
                let wal = Wal::new(temp_dir.path(), Some(10 * 1024)).expect("Failed to create WAL"); // 10KB segments
                (temp_dir, wal)
            },
            |(_temp_dir, mut wal)| {
                // Write enough to trigger multiple rotations
                for i in 0..1000 {
                    let event = create_test_event(i);
                    wal.append(&event).expect("Failed to append event");
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
