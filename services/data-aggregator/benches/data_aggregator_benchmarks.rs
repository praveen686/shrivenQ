//! Performance benchmarks for data aggregator components

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use data_aggregator::{DataAggregatorService, DataAggregator, Candle, Timeframe};
use data_aggregator::storage::{Wal, DataEvent, TradeEvent};
use services_common::{Px, Qty, Symbol, Ts};
use chrono::{DateTime, Utc, Duration};
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_trade_processing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("trade_processing");
    group.sample_size(100);
    
    for &trade_count in &[100, 1000, 10000] {
        group.bench_with_input(
            BenchmarkId::new("memory_only", trade_count),
            &trade_count,
            |b, &trade_count| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut aggregator = DataAggregatorService::new();
                        let symbol = Symbol::new(1);
                        let base_time = Utc::now();
                        
                        for i in 0..trade_count {
                            let ts = Ts::from_nanos(
                                (base_time + Duration::microseconds(i * 100))
                                    .timestamp_nanos_opt()
                                    .unwrap() as u64
                            );
                            
                            let price = Px::from_price_i32(100_0000 + (i % 1000) as i64);
                            let qty = Qty::from_qty_i32(1_0000 + (i % 100) as i64 * 100);
                            let is_buy = i % 2 == 0;
                            
                            black_box(
                                aggregator.process_trade(symbol, ts, price, qty, is_buy).await
                            ).unwrap();
                        }
                    });
                });
            },
        );
    }
    
    for &trade_count in &[100, 1000, 5000] {
        group.bench_with_input(
            BenchmarkId::new("with_wal", trade_count),
            &trade_count,
            |b, &trade_count| {
                b.iter(|| {
                    rt.block_on(async {
                        let temp_dir = TempDir::new().unwrap();
                        let wal_path = temp_dir.path();
                        let mut aggregator = DataAggregatorService::with_wal(wal_path).unwrap();
                        let symbol = Symbol::new(1);
                        let base_time = Utc::now();
                        
                        for i in 0..trade_count {
                            let ts = Ts::from_nanos(
                                (base_time + Duration::microseconds(i * 100))
                                    .timestamp_nanos_opt()
                                    .unwrap() as u64
                            );
                            
                            let price = Px::from_price_i32(100_0000 + (i % 1000) as i64);
                            let qty = Qty::from_qty_i32(1_0000 + (i % 100) as i64 * 100);
                            let is_buy = i % 2 == 0;
                            
                            black_box(
                                aggregator.process_trade(symbol, ts, price, qty, is_buy).await
                            ).unwrap();
                        }
                        
                        aggregator.flush_wal().await.unwrap();
                    });
                });
            },
        );
    }
    
    group.finish();
}

fn bench_candle_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("candle_operations");
    
    // Benchmark candle creation
    group.bench_function("candle_creation", |b| {
        let symbol = Symbol::new(1);
        let timeframe = Timeframe::M1;
        let now = Utc::now();
        
        b.iter(|| {
            black_box(Candle::new(symbol, timeframe, now))
        });
    });
    
    // Benchmark candle updates
    group.bench_function("candle_update_single_trade", |b| {
        let symbol = Symbol::new(1);
        let timeframe = Timeframe::M1;
        let now = Utc::now();
        let mut candle = Candle::new(symbol, timeframe, now);
        let price = Px::from_price_i32(100_0000);
        let qty = Qty::from_qty_i32(10_0000);
        
        b.iter(|| {
            candle.update_trade(black_box(price), black_box(qty), black_box(true));
        });
    });
    
    // Benchmark multiple candle updates
    for &update_count in &[10, 100, 1000] {
        group.bench_with_input(
            BenchmarkId::new("candle_update_multiple", update_count),
            &update_count,
            |b, &update_count| {
                b.iter(|| {
                    let symbol = Symbol::new(1);
                    let timeframe = Timeframe::M1;
                    let now = Utc::now();
                    let mut candle = Candle::new(symbol, timeframe, now);
                    
                    for i in 0..update_count {
                        let price = Px::from_price_i32(100_0000 + (i % 100) as i64 * 10);
                        let qty = Qty::from_qty_i32(1_0000 + (i % 10) as i64 * 1000);
                        let is_buy = i % 2 == 0;
                        
                        candle.update_trade(price, qty, is_buy);
                    }
                    
                    black_box(candle)
                });
            },
        );
    }
    
    // Benchmark candle retrieval
    group.bench_function("get_current_candle", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut aggregator = DataAggregatorService::new();
                let symbol = Symbol::new(1);
                let ts = Ts::from_nanos(Utc::now().timestamp_nanos_opt().unwrap() as u64);
                let price = Px::from_price_i32(100_0000);
                let qty = Qty::from_qty_i32(10_0000);
                
                aggregator.process_trade(symbol, ts, price, qty, true).await.unwrap();
                
                black_box(
                    aggregator.get_current_candle(symbol, Timeframe::M1).await
                )
            });
        });
    });
    
    group.finish();
}

fn bench_wal_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("wal_operations");
    group.sample_size(50);
    
    // Benchmark WAL append operations
    for &entry_count in &[100, 1000, 10000] {
        group.bench_with_input(
            BenchmarkId::new("wal_append", entry_count),
            &entry_count,
            |b, &entry_count| {
                b.iter(|| {
                    let temp_dir = TempDir::new().unwrap();
                    let wal_path = temp_dir.path();
                    let mut wal = Wal::new(wal_path, Some(10 * 1024 * 1024)).unwrap(); // 10MB
                    
                    for i in 0..entry_count {
                        let event = DataEvent::Trade(TradeEvent {
                            ts: Ts::from_nanos(1_000_000 + i as u64),
                            symbol: Symbol::new(1),
                            price: Px::from_price_i32(100_0000 + (i % 1000) as i64),
                            quantity: Qty::from_qty_i32(1_0000),
                            is_buy: i % 2 == 0,
                            trade_id: i as u64,
                        });
                        
                        wal.append(&event).unwrap();
                    }
                    
                    wal.flush().unwrap();
                    black_box(wal)
                });
            },
        );
    }
    
    // Benchmark WAL read operations
    for &entry_count in &[100, 1000, 5000] {
        group.bench_with_input(
            BenchmarkId::new("wal_read_all", entry_count),
            &entry_count,
            |b, &entry_count| {
                // Setup: create WAL with data
                let temp_dir = TempDir::new().unwrap();
                let wal_path = temp_dir.path();
                
                {
                    let mut wal = Wal::new(wal_path, Some(10 * 1024 * 1024)).unwrap();
                    for i in 0..entry_count {
                        let event = DataEvent::Trade(TradeEvent {
                            ts: Ts::from_nanos(1_000_000 + i as u64),
                            symbol: Symbol::new(1),
                            price: Px::from_price_i32(100_0000),
                            quantity: Qty::from_qty_i32(1_0000),
                            is_buy: true,
                            trade_id: i as u64,
                        });
                        wal.append(&event).unwrap();
                    }
                    wal.flush().unwrap();
                }
                
                // Benchmark reading
                b.iter(|| {
                    let wal = Wal::new(wal_path, Some(10 * 1024 * 1024)).unwrap();
                    let events: Vec<DataEvent> = wal.read_all().unwrap();
                    black_box(events)
                });
            },
        );
    }
    
    // Benchmark WAL streaming
    group.bench_function("wal_stream", |b| {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path();
        
        // Setup WAL with data
        {
            let mut wal = Wal::new(wal_path, Some(1024 * 1024)).unwrap();
            for i in 0..1000 {
                let event = DataEvent::Trade(TradeEvent {
                    ts: Ts::from_nanos(1_000_000 + i as u64),
                    symbol: Symbol::new(1),
                    price: Px::from_price_i32(100_0000),
                    quantity: Qty::from_qty_i32(1_0000),
                    is_buy: true,
                    trade_id: i as u64,
                });
                wal.append(&event).unwrap();
            }
            wal.flush().unwrap();
        }
        
        b.iter(|| {
            let wal = Wal::new(wal_path, Some(1024 * 1024)).unwrap();
            let mut iterator = wal.stream::<DataEvent>(None).unwrap();
            let mut count = 0;
            
            while let Ok(Some(_event)) = iterator.read_next_entry() {
                count += 1;
            }
            
            black_box(count)
        });
    });
    
    group.finish();
}

fn bench_concurrent_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("concurrent_operations");
    group.sample_size(20);
    
    for &symbol_count in &[1, 5, 10] {
        group.bench_with_input(
            BenchmarkId::new("multi_symbol_processing", symbol_count),
            &symbol_count,
            |b, &symbol_count| {
                b.iter(|| {
                    rt.block_on(async {
                        let temp_dir = TempDir::new().unwrap();
                        let wal_path = temp_dir.path();
                        let aggregator = std::sync::Arc::new(tokio::sync::RwLock::new(
                            DataAggregatorService::with_wal(wal_path).unwrap()
                        ));
                        
                        let mut handles = Vec::new();
                        
                        for symbol_id in 1..=symbol_count {
                            let aggregator = aggregator.clone();
                            let handle = tokio::spawn(async move {
                                let symbol = Symbol::new(symbol_id);
                                let base_time = Utc::now();
                                
                                for i in 0..100 {
                                    let ts = Ts::from_nanos(
                                        (base_time + Duration::microseconds(i * 100))
                                            .timestamp_nanos_opt()
                                            .unwrap() as u64
                                    );
                                    
                                    let price = Px::from_price_i32(100_0000 + i as i64 * 10);
                                    let qty = Qty::from_qty_i32(1_0000);
                                    let is_buy = i % 2 == 0;
                                    
                                    let mut agg = aggregator.write().await;
                                    agg.process_trade(symbol, ts, price, qty, is_buy).await.unwrap();
                                }
                            });
                            handles.push(handle);
                        }
                        
                        for handle in handles {
                            handle.await.unwrap();
                        }
                        
                        let mut agg = aggregator.write().await;
                        agg.flush_wal().await.unwrap();
                        
                        black_box(aggregator)
                    });
                });
            },
        );
    }
    
    group.finish();
}

fn bench_timeframe_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("timeframe_operations");
    
    let timeframes = vec![
        Timeframe::M1, 
        Timeframe::M5, 
        Timeframe::M15, 
        Timeframe::H1, 
        Timeframe::D1
    ];
    
    for timeframe in timeframes {
        group.bench_with_input(
            BenchmarkId::new("multi_timeframe_update", format!("{:?}", timeframe)),
            &timeframe,
            |b, &timeframe| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut aggregator = DataAggregatorService::new();
                        let symbol = Symbol::new(1);
                        let base_time = Utc::now();
                        
                        // Process trades across the timeframe duration
                        let duration_seconds = timeframe.duration_seconds();
                        let trades_per_second = 10;
                        let total_trades = std::cmp::min(duration_seconds * trades_per_second, 1000);
                        
                        for i in 0..total_trades {
                            let ts = Ts::from_nanos(
                                (base_time + Duration::seconds(i / trades_per_second))
                                    .timestamp_nanos_opt()
                                    .unwrap() as u64
                            );
                            
                            let price = Px::from_price_i32(100_0000 + (i % 100) as i64 * 10);
                            let qty = Qty::from_qty_i32(1_0000);
                            let is_buy = i % 2 == 0;
                            
                            aggregator.process_trade(symbol, ts, price, qty, is_buy).await.unwrap();
                        }
                        
                        let candle = aggregator.get_current_candle(symbol, timeframe).await;
                        black_box(candle)
                    });
                });
            },
        );
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_trade_processing,
    bench_candle_operations,
    bench_wal_operations,
    bench_concurrent_operations,
    bench_timeframe_operations
);
criterion_main!(benches);