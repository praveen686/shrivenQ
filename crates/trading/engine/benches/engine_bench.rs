//! Performance benchmarks for trading engine

// Benchmarks are not production code - unwrap/expect are acceptable here
#![allow(clippy::unwrap_used, clippy::expect_used)]

use bus::EventBus;
use common::constants::bench::{BENCH_ARENA_SIZE, BENCH_POOL_SIZE};
use common::{Px, Qty, Side, Symbol};
use criterion::{BatchSize, Criterion, black_box, criterion_group, criterion_main};
use engine::core::{Engine, EngineConfig, ExecutionMode};
use engine::memory::{Arena, ObjectPool};
use engine::risk::RiskEngine;
use engine::venue::{VenueConfig, create_binance_adapter};
use std::sync::Arc;

fn bench_memory_pool(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_pool");

    // Benchmark object pool acquire/release
    group.bench_function("object_pool_acquire_release", |b| {
        #[derive(Default)]
        struct TestObj {
            _data: [u64; 8],
        }

        let pool: ObjectPool<TestObj> = ObjectPool::new(BENCH_POOL_SIZE);

        b.iter(|| {
            if let Some(obj) = pool.acquire() {
                black_box(&obj);
                // obj automatically released when dropped
            }
        });
    });

    // Benchmark arena allocation
    group.bench_function("arena_allocation", |b| {
        let arena = match Arena::new(BENCH_ARENA_SIZE) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("Failed to create arena: {:?}", e);
                return;
            }
        };

        b.iter(|| {
            let ptr: Option<&mut u64> = arena.alloc();
            if let Some(p) = ptr {
                *p = 42;
                black_box(p);
            }
        });
    });

    group.finish();
}

fn bench_risk_checks(c: &mut Criterion) {
    let mut group = c.benchmark_group("risk_checks");

    group.bench_function("risk_order_check", |b| {
        let config = EngineConfig::default();
        let risk = RiskEngine::new(config);
        let symbol = Symbol(100);

        b.iter(|| {
            let pass = risk.check_order(
                black_box(symbol),
                black_box(Side::Bid),
                black_box(Qty::new(10.0)),
                black_box(Some(Px::new(100.0))),
            );
            black_box(pass);
        });
    });

    group.finish();
}

fn bench_order_execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("order_execution");

    // Benchmark paper trading execution
    group.bench_function("paper_order_execution", |b| {
        let mut config = EngineConfig::default();
        config.mode = ExecutionMode::Paper;

        let venue_config = VenueConfig {
            api_key: "test".to_string(),
            api_secret: "test".to_string(),
            testnet: true,
        };
        let venue = create_binance_adapter(venue_config);
        let bus = Arc::new(EventBus::new(1024));
        let engine = Engine::new(config, venue, bus);

        let symbol = Symbol(100);

        b.iter(|| {
            let result = engine.send_order(
                black_box(symbol),
                black_box(Side::Bid),
                black_box(Qty::new(10.0)),
                black_box(Some(Px::new(100.0))),
            );
            let _ = black_box(result);
        });
    });

    // Benchmark backtest execution
    group.bench_function("backtest_order_execution", |b| {
        let mut config = EngineConfig::default();
        config.mode = ExecutionMode::Backtest;

        let venue_config = VenueConfig {
            api_key: "test".to_string(),
            api_secret: "test".to_string(),
            testnet: true,
        };
        let venue = create_binance_adapter(venue_config);
        let bus = Arc::new(EventBus::new(1024));
        let engine = Engine::new(config, venue, bus);

        let symbol = Symbol(100);

        b.iter(|| {
            let result = engine.send_order(
                black_box(symbol),
                black_box(Side::Ask),
                black_box(Qty::new(5.0)),
                black_box(Some(Px::new(101.0))),
            );
            let _ = black_box(result);
        });
    });

    group.finish();
}

fn bench_config_copy(c: &mut Criterion) {
    let mut group = c.benchmark_group("config_copy");

    // Benchmark config copy (should be very fast now that it's Copy)
    group.bench_function("engine_config_copy", |b| {
        let config = EngineConfig::default();

        b.iter(|| {
            let config_copy = black_box(config);
            black_box(config_copy);
        });
    });

    // Compare with hypothetical Arc version for reference
    group.bench_function("arc_clone_comparison", |b| {
        let config = Arc::new(EngineConfig::default());

        b.iter(|| {
            let config_clone = config.clone();
            black_box(config_clone);
        });
    });

    group.finish();
}

fn bench_concurrent_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent");

    group.bench_function("concurrent_order_submission", |b| {
        let config = EngineConfig::default();
        let venue_config = VenueConfig {
            api_key: "test".to_string(),
            api_secret: "test".to_string(),
            testnet: true,
        };
        let venue = create_binance_adapter(venue_config);
        let bus = Arc::new(EventBus::new(1024));
        let engine = Arc::new(Engine::new(config, venue, bus));

        b.iter_batched(
            || engine.clone(),
            |engine| {
                let handles: Vec<_> = (0..4)
                    .map(|i| {
                        let engine = engine.clone();
                        std::thread::spawn(move || {
                            for j in 0..10 {
                                let symbol = Symbol(100 + i * 10 + j);
                                let _ = engine.send_order(
                                    symbol,
                                    Side::Bid,
                                    Qty::new(10.0),
                                    Some(Px::new(100.0)),
                                );
                            }
                        })
                    })
                    .collect();

                for handle in handles {
                    let _ = handle.join();
                }
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_memory_pool,
    bench_risk_checks,
    bench_order_execution,
    bench_config_copy,
    bench_concurrent_operations
);
criterion_main!(benches);
