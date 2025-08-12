//! Benchmark comparison between OrderBook v1 and v2

use common::{L2Update, Px, Qty, Side, Symbol, Ts};
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use lob::{CrossResolution, OrderBook, OrderBookV2};
use rand::prelude::*;

fn generate_updates(n: usize, spread_bps: f64) -> Vec<L2Update> {
    let mut rng = StdRng::seed_from_u64(42);
    let mut updates = Vec::with_capacity(n);
    let base_price = 100.0;

    for i in 0..n {
        let side = if rng.r#gen::<bool>() {
            Side::Bid
        } else {
            Side::Ask
        };
        let spread = base_price * spread_bps / 10000.0;

        let price = if side == Side::Bid {
            base_price - spread * (1.0 + rng.r#gen::<f64>() * 2.0)
        } else {
            base_price + spread * (1.0 + rng.r#gen::<f64>() * 2.0)
        };

        let qty = if rng.r#gen::<f64>() < 0.1 {
            0.0 // Cancel
        } else {
            10.0 + rng.r#gen::<f64>() * 990.0
        };

        let level = (rng.r#gen::<f64>() * 10.0) as u8;

        updates.push(
            L2Update::new(Ts::from_nanos(i as u64), Symbol(1)).with_level_data(
                side,
                Px::new(price),
                Qty::new(qty),
                level,
            ),
        );
    }

    updates
}

fn bench_v1_updates(c: &mut Criterion) {
    let updates = generate_updates(10000, 5.0);

    c.bench_function("v1_apply", |b| {
        b.iter(|| {
            let mut book = OrderBook::new(Symbol(1));
            for update in &updates[..100] {
                let _ = black_box(book.apply(update));
            }
        });
    });
}

fn bench_v2_updates(c: &mut Criterion) {
    let updates = generate_updates(10000, 5.0);

    c.bench_function("v2_apply_fast", |b| {
        b.iter(|| {
            let mut book = OrderBookV2::new(Symbol(1), 0.01, 1.0);
            for update in &updates[..100] {
                black_box(book.apply_fast(update));
            }
        });
    });

    c.bench_function("v2_apply_validated", |b| {
        b.iter(|| {
            let mut book = OrderBookV2::new(Symbol(1), 0.01, 1.0);
            for update in &updates[..100] {
                let _ = black_box(book.apply_validated(update));
            }
        });
    });
}

fn bench_v2_roi(c: &mut Criterion) {
    let updates = generate_updates(10000, 5.0);

    c.bench_function("v2_roi_updates", |b| {
        b.iter(|| {
            let mut book = OrderBookV2::new_with_roi(
                Symbol(1),
                0.01,
                1.0,
                100.0, // center
                5.0,   // width
            );

            for update in &updates[..100] {
                black_box(book.apply_fast(update));
            }
        });
    });
}

fn bench_bbo_access(c: &mut Criterion) {
    let mut book_v1 = OrderBook::new(Symbol(1));
    let mut book_v2 = OrderBookV2::new(Symbol(1), 0.01, 1.0);

    // Populate books
    let updates = generate_updates(100, 5.0);
    for update in &updates {
        let _ = book_v1.apply(update);
        book_v2.apply_fast(update);
    }

    c.bench_function("v1_best_bid", |b| {
        b.iter(|| {
            black_box(book_v1.best_bid());
        });
    });

    c.bench_function("v2_best_bid", |b| {
        b.iter(|| {
            black_box(book_v2.best_bid());
        });
    });
}

fn bench_cross_resolution(c: &mut Criterion) {
    let mut updates = generate_updates(1000, 5.0);

    // Create some crossed updates
    for i in (0..updates.len()).step_by(10) {
        if i + 1 < updates.len() {
            // Make bid higher than ask
            updates[i] = L2Update::new(Ts::from_nanos(i as u64), Symbol(1)).with_level_data(
                Side::Bid,
                Px::new(101.0),
                Qty::new(100.0),
                0,
            );
            updates[i + 1] = L2Update::new(Ts::from_nanos((i + 1) as u64), Symbol(1))
                .with_level_data(Side::Ask, Px::new(99.0), Qty::new(100.0), 0);
        }
    }

    c.bench_function("v2_auto_resolve_cross", |b| {
        b.iter(|| {
            let mut book = OrderBookV2::new(Symbol(1), 0.01, 1.0);
            book.set_cross_resolution(CrossResolution::AutoResolve);

            for update in &updates[..100] {
                let _ = black_box(book.apply_validated(update));
            }
        });
    });
}

fn bench_imbalance_calculation(c: &mut Criterion) {
    let mut book_v1 = OrderBook::new(Symbol(1));
    let mut book_v2 = OrderBookV2::new(Symbol(1), 0.01, 1.0);

    // Populate with realistic depth
    let updates = generate_updates(200, 5.0);
    for update in &updates {
        let _ = book_v1.apply(update);
        book_v2.apply_fast(update);
    }

    c.bench_function("v1_imbalance", |b| {
        b.iter(|| {
            // v1 doesn't have built-in imbalance, calculate manually
            let bid_vol = book_v1.bids.total_qty(5).as_f64();
            let ask_vol = book_v1.asks.total_qty(5).as_f64();
            let total = bid_vol + ask_vol;
            black_box(if total > 0.0 {
                (bid_vol - ask_vol) / total
            } else {
                0.0
            });
        });
    });

    c.bench_function("v2_imbalance", |b| {
        b.iter(|| {
            black_box(book_v2.imbalance(5));
        });
    });
}

criterion_group!(
    benches,
    bench_v1_updates,
    bench_v2_updates,
    bench_v2_roi,
    bench_bbo_access,
    bench_cross_resolution,
    bench_imbalance_calculation
);
criterion_main!(benches);
