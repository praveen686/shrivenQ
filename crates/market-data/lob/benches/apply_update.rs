//! Benchmarks for LOB update performance

use common::{L2Update, Px, Qty, Side, Symbol, Ts};
use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use lob::{DEPTH, OrderBook};
use rand::{Rng, SeedableRng, rngs::StdRng};

fn create_random_update(rng: &mut StdRng, symbol: Symbol, ts: u64) -> L2Update {
    let side = if rng.gen_bool(0.5) {
        Side::Bid
    } else {
        Side::Ask
    };
    let level = rng.gen_range(0..5) as u8; // Focus on top 5 levels

    // Generate realistic prices
    let base_price = 100.0;
    let offset: f64 = rng.gen_range(-5.0..5.0);
    let price = if side == Side::Bid {
        base_price - offset.abs()
    } else {
        base_price + offset.abs()
    };

    let qty = if rng.gen_bool(0.1) {
        0.0 // 10% chance of removal
    } else {
        rng.gen_range(10.0..1000.0)
    };

    L2Update::new(Ts::from_nanos(ts), symbol).with_level_data(
        side,
        Px::new(price),
        Qty::new(qty),
        level,
    )
}

fn benchmark_apply_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("lob_apply");

    let symbol = Symbol::new(1);
    let mut book = OrderBook::new(symbol);
    let mut rng = StdRng::seed_from_u64(42);

    // Pre-populate book with some levels
    for i in 0..10 {
        let bid_update = L2Update::new(Ts::from_nanos(i), symbol).with_level_data(
            Side::Bid,
            Px::new(99.5 - i as f64 * 0.1),
            Qty::new(100.0 * (i + 1) as f64),
            i as u8,
        );
        let _ = book.apply(&bid_update);

        let ask_update = L2Update::new(Ts::from_nanos(i + 100), symbol).with_level_data(
            Side::Ask,
            Px::new(100.5 + i as f64 * 0.1),
            Qty::new(100.0 * (i + 1) as f64),
            i as u8,
        );
        let _ = book.apply(&ask_update);
    }

    group.bench_function("single_update", |b| {
        let mut ts = 1000u64;
        b.iter(|| {
            let update = create_random_update(&mut rng, symbol, ts);
            ts += 1;
            let _ = black_box(book.apply(&update));
        });
    });

    group.finish();
}

fn benchmark_apply_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("lob_batch");

    for size in &[100, 1000, 10000] {
        group.throughput(Throughput::Elements(*size));
        group.bench_function(format!("updates_{}", size), |b| {
            let symbol = Symbol::new(1);
            let mut rng = StdRng::seed_from_u64(42);

            // Generate updates
            let updates: Vec<L2Update> = (0..*size)
                .map(|i| create_random_update(&mut rng, symbol, i))
                .collect();

            b.iter(|| {
                let mut book = OrderBook::new(symbol);
                for update in &updates {
                    let _ = black_box(book.apply(update));
                }
            });
        });
    }

    group.finish();
}

fn benchmark_features(c: &mut Criterion) {
    let mut group = c.benchmark_group("lob_features");

    let symbol = Symbol::new(1);
    let mut book = OrderBook::new(symbol);

    // Populate book
    for i in 0..DEPTH / 2 {
        let _ = book.apply(
            &L2Update::new(Ts::from_nanos(i as u64), symbol).with_level_data(
                Side::Bid,
                Px::new(99.5 - i as f64 * 0.05),
                Qty::new(100.0 + i as f64 * 10.0),
                i as u8,
            ),
        );

        let _ = book.apply(
            &L2Update::new(Ts::from_nanos((i + 100) as u64), symbol).with_level_data(
                Side::Ask,
                Px::new(100.5 + i as f64 * 0.05),
                Qty::new(100.0 + i as f64 * 10.0),
                i as u8,
            ),
        );
    }

    group.bench_function("mid_price", |b| {
        b.iter(|| {
            black_box(book.mid());
        });
    });

    group.bench_function("microprice", |b| {
        b.iter(|| {
            black_box(book.microprice());
        });
    });

    group.bench_function("imbalance", |b| {
        b.iter(|| {
            black_box(book.imbalance(5));
        });
    });

    group.bench_function("spread", |b| {
        b.iter(|| {
            black_box(book.spread_ticks());
        });
    });

    group.finish();
}

fn benchmark_crossed_check(c: &mut Criterion) {
    let mut group = c.benchmark_group("lob_safety");

    let symbol = Symbol::new(1);
    let mut book = OrderBook::new(symbol);

    // Setup book close to crossing
    let _ = book.apply(&L2Update::new(Ts::from_nanos(1), symbol).with_level_data(
        Side::Bid,
        Px::new(99.99),
        Qty::new(100.0),
        0,
    ));

    let _ = book.apply(&L2Update::new(Ts::from_nanos(2), symbol).with_level_data(
        Side::Ask,
        Px::new(100.01),
        Qty::new(100.0),
        0,
    ));

    group.bench_function("crossed_check", |b| {
        b.iter(|| {
            black_box(book.is_crossed());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_apply_single,
    benchmark_apply_batch,
    benchmark_features,
    benchmark_crossed_check
);
criterion_main!(benches);
