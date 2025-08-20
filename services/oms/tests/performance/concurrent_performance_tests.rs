//! Performance tests for concurrent OMS operations

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use rstest::*;
use services_common::{Px, Qty, Symbol};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use uuid::Uuid;

use oms::{
    OrderManagementSystem, OmsConfig,
    order::{OrderRequest, OrderSide, OrderStatus, OrderType, TimeInForce, Fill, LiquidityIndicator},
    matching::MatchingEngine,
    lifecycle::OrderLifecycleManager,
};

/// Helper to create test order request
fn create_test_order_request(id: usize) -> OrderRequest {
    OrderRequest {
        client_order_id: Some(format!("PERF-{:06}", id)),
        parent_order_id: None,
        symbol: Symbol((id % 5) as u32 + 1), // Distribute across 5 symbols
        side: if id % 2 == 0 { OrderSide::Buy } else { OrderSide::Sell },
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Day,
        quantity: Qty::from_i64(1000 + (id % 10000) as i64),
        price: Some(Px::from_i64(1_000_000_000 + (id as i64 * 1000))),
        stop_price: None,
        account: format!("perf_account_{}", id % 100),
        exchange: "binance".to_string(),
        strategy_id: Some("performance_test".to_string()),
        tags: vec!["performance".to_string(), "benchmark".to_string()],
    }
}

/// Helper to create test OMS configuration
fn create_test_config() -> OmsConfig {
    OmsConfig {
        database_url: "postgresql://test:test@localhost/test_oms_perf".to_string(),
        max_orders_memory: 100000,
        retention_days: 1,
        enable_audit: false, // Disable for pure performance testing
        enable_matching: true,
        persist_batch_size: 1000,
    }
}

/// Benchmark order creation throughput
fn bench_order_creation_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("order_creation_throughput");
    group.measurement_time(Duration::from_secs(10));
    
    for batch_size in [100, 500, 1000, 2000, 5000].iter() {
        group.bench_with_input(
            BenchmarkId::new("sequential", batch_size),
            batch_size,
            |b, &batch_size| {
                b.to_async(&rt).iter(|| async {
                    let config = create_test_config();
                    let oms = OrderManagementSystem::new(config).await.unwrap();
                    
                    let start = Instant::now();
                    for i in 0..batch_size {
                        let request = create_test_order_request(i);
                        let _order = oms.create_order(request).await.unwrap();
                    }
                    let duration = start.elapsed();
                    
                    let throughput = batch_size as f64 / duration.as_secs_f64();
                    black_box(throughput);
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark concurrent order creation
fn bench_concurrent_order_creation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("concurrent_order_creation");
    group.measurement_time(Duration::from_secs(15));
    
    for concurrency in [1, 2, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::new("concurrent", concurrency),
            concurrency,
            |b, &concurrency| {
                b.to_async(&rt).iter(|| async {
                    let config = create_test_config();
                    let oms = Arc::new(OrderManagementSystem::new(config).await.unwrap());
                    
                    let orders_per_thread = 100;
                    let mut handles = Vec::new();
                    
                    let start = Instant::now();
                    
                    for thread_id in 0..concurrency {
                        let oms_clone = Arc::clone(&oms);
                        let handle = tokio::spawn(async move {
                            for i in 0..orders_per_thread {
                                let request = create_test_order_request(thread_id * orders_per_thread + i);
                                let _order = oms_clone.create_order(request).await.unwrap();
                            }
                        });
                        handles.push(handle);
                    }
                    
                    for handle in handles {
                        handle.await.unwrap();
                    }
                    
                    let duration = start.elapsed();
                    let total_orders = concurrency * orders_per_thread;
                    let throughput = total_orders as f64 / duration.as_secs_f64();
                    
                    black_box(throughput);
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark order matching performance
fn bench_order_matching_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("order_matching");
    group.measurement_time(Duration::from_secs(8));
    
    for order_count in [100, 500, 1000, 2000].iter() {
        group.bench_with_input(
            BenchmarkId::new("matching", order_count),
            order_count,
            |b, &order_count| {
                b.iter(|| {
                    let engine = MatchingEngine::new();
                    
                    // Add sell orders first
                    for i in 0..order_count / 2 {
                        let order = oms::order::Order {
                            id: Uuid::new_v4(),
                            client_order_id: Some(format!("SELL-{}", i)),
                            parent_order_id: None,
                            symbol: Symbol(1),
                            side: OrderSide::Sell,
                            order_type: OrderType::Limit,
                            time_in_force: TimeInForce::Gtc,
                            quantity: Qty::from_i64(1000),
                            executed_quantity: Qty::ZERO,
                            remaining_quantity: Qty::from_i64(1000),
                            price: Some(Px::from_i64(1_000_000 + (i as i64 * 1000))),
                            stop_price: None,
                            status: OrderStatus::New,
                            created_at: chrono::Utc::now(),
                            updated_at: chrono::Utc::now(),
                            account: "bench_account".to_string(),
                            exchange: "internal".to_string(),
                            strategy_id: None,
                            tags: vec![],
                            fills: vec![],
                            amendments: vec![],
                            version: 1,
                            sequence_number: i as u64,
                        };
                        let _matches = engine.add_order(&order).unwrap();
                    }
                    
                    // Add buy orders that will match
                    let start = Instant::now();
                    for i in 0..order_count / 2 {
                        let order = oms::order::Order {
                            id: Uuid::new_v4(),
                            client_order_id: Some(format!("BUY-{}", i)),
                            parent_order_id: None,
                            symbol: Symbol(1),
                            side: OrderSide::Buy,
                            order_type: OrderType::Limit,
                            time_in_force: TimeInForce::Gtc,
                            quantity: Qty::from_i64(500), // Smaller size to create partial matches
                            executed_quantity: Qty::ZERO,
                            remaining_quantity: Qty::from_i64(500),
                            price: Some(Px::from_i64(1_050_000)), // High enough to match multiple levels
                            stop_price: None,
                            status: OrderStatus::New,
                            created_at: chrono::Utc::now(),
                            updated_at: chrono::Utc::now(),
                            account: "bench_account".to_string(),
                            exchange: "internal".to_string(),
                            strategy_id: None,
                            tags: vec![],
                            fills: vec![],
                            amendments: vec![],
                            version: 1,
                            sequence_number: (order_count / 2 + i) as u64,
                        };
                        let _matches = engine.add_order(&order).unwrap();
                    }
                    let duration = start.elapsed();
                    
                    black_box(duration);
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark order lifecycle validation performance
fn bench_lifecycle_validation(c: &mut Criterion) {
    let lifecycle_manager = OrderLifecycleManager::new();
    
    let test_order = oms::order::Order {
        id: Uuid::new_v4(),
        client_order_id: Some("LIFECYCLE-BENCH".to_string()),
        parent_order_id: None,
        symbol: Symbol(1),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Day,
        quantity: Qty::from_i64(10000),
        executed_quantity: Qty::ZERO,
        remaining_quantity: Qty::from_i64(10000),
        price: Some(Px::from_i64(1_000_000)),
        stop_price: None,
        status: OrderStatus::New,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        account: "bench_account".to_string(),
        exchange: "binance".to_string(),
        strategy_id: Some("bench_strategy".to_string()),
        tags: vec!["bench".to_string()],
        fills: vec![],
        amendments: vec![],
        version: 1,
        sequence_number: 1,
    };
    
    let mut group = c.benchmark_group("lifecycle_validation");
    
    group.bench_function("validate_order", |b| {
        b.iter(|| {
            let result = lifecycle_manager.validate_order(black_box(&test_order));
            black_box(result);
        });
    });
    
    group.bench_function("validate_transition", |b| {
        b.iter(|| {
            let result = lifecycle_manager.validate_transition(
                black_box(&test_order),
                black_box(OrderStatus::Pending)
            );
            black_box(result);
        });
    });
    
    group.bench_function("can_cancel", |b| {
        b.iter(|| {
            let result = lifecycle_manager.can_cancel(black_box(&test_order));
            black_box(result);
        });
    });
    
    group.bench_function("can_amend", |b| {
        b.iter(|| {
            let result = lifecycle_manager.can_amend(black_box(&test_order));
            black_box(result);
        });
    });
    
    group.finish();
}

/// Benchmark memory usage and order book depth calculation
fn bench_order_book_depth(c: &mut Criterion) {
    let mut group = c.benchmark_group("order_book_depth");
    
    for order_count in [100, 500, 1000, 5000].iter() {
        group.bench_with_input(
            BenchmarkId::new("depth_calculation", order_count),
            order_count,
            |b, &order_count| {
                // Setup order book with many orders
                let engine = MatchingEngine::new();
                
                // Add orders at many different price levels
                for i in 0..order_count {
                    let buy_order = oms::order::Order {
                        id: Uuid::new_v4(),
                        client_order_id: None,
                        parent_order_id: None,
                        symbol: Symbol(1),
                        side: OrderSide::Buy,
                        order_type: OrderType::Limit,
                        time_in_force: TimeInForce::Gtc,
                        quantity: Qty::from_i64(1000),
                        executed_quantity: Qty::ZERO,
                        remaining_quantity: Qty::from_i64(1000),
                        price: Some(Px::from_i64(1_000_000 - (i as i64 * 100))),
                        stop_price: None,
                        status: OrderStatus::New,
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                        account: "depth_account".to_string(),
                        exchange: "internal".to_string(),
                        strategy_id: None,
                        tags: vec![],
                        fills: vec![],
                        amendments: vec![],
                        version: 1,
                        sequence_number: i as u64 * 2,
                    };
                    
                    let sell_order = oms::order::Order {
                        id: Uuid::new_v4(),
                        client_order_id: None,
                        parent_order_id: None,
                        symbol: Symbol(1),
                        side: OrderSide::Sell,
                        order_type: OrderType::Limit,
                        time_in_force: TimeInForce::Gtc,
                        quantity: Qty::from_i64(1000),
                        executed_quantity: Qty::ZERO,
                        remaining_quantity: Qty::from_i64(1000),
                        price: Some(Px::from_i64(1_001_000 + (i as i64 * 100))),
                        stop_price: None,
                        status: OrderStatus::New,
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                        account: "depth_account".to_string(),
                        exchange: "internal".to_string(),
                        strategy_id: None,
                        tags: vec![],
                        fills: vec![],
                        amendments: vec![],
                        version: 1,
                        sequence_number: i as u64 * 2 + 1,
                    };
                    
                    engine.add_order(&buy_order).unwrap();
                    engine.add_order(&sell_order).unwrap();
                }
                
                b.iter(|| {
                    let depth = engine.get_depth(Symbol(1), black_box(20)).unwrap();
                    black_box(depth);
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark fill processing performance
fn bench_fill_processing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("fill_processing");
    group.measurement_time(Duration::from_secs(10));
    
    for fill_count in [100, 500, 1000, 2000].iter() {
        group.bench_with_input(
            BenchmarkId::new("process_fills", fill_count),
            fill_count,
            |b, &fill_count| {
                b.to_async(&rt).iter(|| async {
                    let config = create_test_config();
                    let oms = OrderManagementSystem::new(config).await.unwrap();
                    
                    // Create a large order
                    let request = OrderRequest {
                        client_order_id: Some("FILL-BENCH-ORDER".to_string()),
                        parent_order_id: None,
                        symbol: Symbol(1),
                        side: OrderSide::Buy,
                        order_type: OrderType::Limit,
                        time_in_force: TimeInForce::Day,
                        quantity: Qty::from_i64(fill_count as i64 * 1000),
                        price: Some(Px::from_i64(1_000_000)),
                        stop_price: None,
                        account: "fill_bench_account".to_string(),
                        exchange: "binance".to_string(),
                        strategy_id: Some("fill_bench".to_string()),
                        tags: vec![],
                    };
                    
                    let order = oms.create_order(request).await.unwrap();
                    
                    // Process many small fills
                    let start = Instant::now();
                    for i in 0..fill_count {
                        let fill = Fill {
                            id: Uuid::new_v4(),
                            order_id: order.id,
                            execution_id: format!("FILL-BENCH-{}", i),
                            quantity: Qty::from_i64(1000),
                            price: Px::from_i64(1_000_000 + (i as i64 * 10)),
                            commission: 10,
                            commission_currency: "USDT".to_string(),
                            timestamp: chrono::Utc::now(),
                            liquidity: if i % 2 == 0 { LiquidityIndicator::Maker } else { LiquidityIndicator::Taker },
                        };
                        
                        oms.process_fill(order.id, fill).await.unwrap();
                    }
                    let duration = start.elapsed();
                    
                    black_box(duration);
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark concurrent order operations (mixed workload)
fn bench_mixed_concurrent_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("mixed_concurrent_operations");
    group.measurement_time(Duration::from_secs(20));
    
    for thread_count in [1, 2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::new("mixed_workload", thread_count),
            thread_count,
            |b, &thread_count| {
                b.to_async(&rt).iter(|| async {
                    let config = create_test_config();
                    let oms = Arc::new(OrderManagementSystem::new(config).await.unwrap());
                    
                    let operations_per_thread = 50;
                    let mut handles = Vec::new();
                    
                    let start = Instant::now();
                    
                    for thread_id in 0..thread_count {
                        let oms_clone = Arc::clone(&oms);
                        let handle = tokio::spawn(async move {
                            let mut created_orders = Vec::new();
                            
                            // Mixed operations: create, submit, fill, amend, cancel
                            for i in 0..operations_per_thread {
                                match i % 5 {
                                    0 => {
                                        // Create order
                                        let request = create_test_order_request(thread_id * operations_per_thread + i);
                                        let order = oms_clone.create_order(request).await.unwrap();
                                        created_orders.push(order.id);
                                    },
                                    1 => {
                                        // Submit order
                                        if let Some(&order_id) = created_orders.get((i / 5) % created_orders.len()) {
                                            let _ = oms_clone.submit_order(order_id).await;
                                        }
                                    },
                                    2 => {
                                        // Process fill
                                        if let Some(&order_id) = created_orders.get((i / 5) % created_orders.len()) {
                                            let fill = Fill {
                                                id: Uuid::new_v4(),
                                                order_id,
                                                execution_id: format!("MIXED-FILL-{}-{}", thread_id, i),
                                                quantity: Qty::from_i64(100),
                                                price: Px::from_i64(1_000_000),
                                                commission: 1,
                                                commission_currency: "USDT".to_string(),
                                                timestamp: chrono::Utc::now(),
                                                liquidity: LiquidityIndicator::Maker,
                                            };
                                            let _ = oms_clone.process_fill(order_id, fill).await;
                                        }
                                    },
                                    3 => {
                                        // Amend order
                                        if let Some(&order_id) = created_orders.get((i / 5) % created_orders.len()) {
                                            let amendment = oms::order::Amendment {
                                                id: Uuid::new_v4(),
                                                order_id,
                                                new_quantity: Some(Qty::from_i64(2000)),
                                                new_price: None,
                                                reason: "Benchmark amendment".to_string(),
                                                timestamp: chrono::Utc::now(),
                                            };
                                            let _ = oms_clone.amend_order(order_id, amendment).await;
                                        }
                                    },
                                    4 => {
                                        // Cancel order
                                        if let Some(&order_id) = created_orders.get((i / 5) % created_orders.len()) {
                                            let _ = oms_clone.cancel_order(order_id, "Benchmark cancellation".to_string()).await;
                                        }
                                    },
                                    _ => unreachable!(),
                                }
                            }
                        });
                        handles.push(handle);
                    }
                    
                    for handle in handles {
                        handle.await.unwrap();
                    }
                    
                    let duration = start.elapsed();
                    black_box(duration);
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark memory efficiency under load
fn bench_memory_efficiency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("memory_efficiency");
    group.measurement_time(Duration::from_secs(15));
    
    for order_count in [1000, 5000, 10000, 20000].iter() {
        group.bench_with_input(
            BenchmarkId::new("memory_usage", order_count),
            order_count,
            |b, &order_count| {
                b.to_async(&rt).iter(|| async {
                    let config = create_test_config();
                    let oms = OrderManagementSystem::new(config).await.unwrap();
                    
                    // Create many orders and measure memory usage patterns
                    let start = Instant::now();
                    for i in 0..order_count {
                        let request = create_test_order_request(i);
                        let _order = oms.create_order(request).await.unwrap();
                        
                        // Periodically check metrics to simulate real usage
                        if i % 1000 == 0 {
                            let _metrics = oms.get_metrics();
                            let _active_orders = oms.get_active_orders();
                        }
                    }
                    
                    let duration = start.elapsed();
                    let final_metrics = oms.get_metrics();
                    
                    black_box((duration, final_metrics));
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark order retrieval performance
fn bench_order_retrieval(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("order_retrieval");
    group.measurement_time(Duration::from_secs(8));
    
    for order_count in [1000, 5000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::new("retrieval_performance", order_count),
            order_count,
            |b, &order_count| {
                b.to_async(&rt).iter_batched_ref(
                    || {
                        // Setup: create OMS with many orders
                        rt.block_on(async {
                            let config = create_test_config();
                            let oms = OrderManagementSystem::new(config).await.unwrap();
                            let mut order_ids = Vec::new();
                            
                            for i in 0..order_count {
                                let request = create_test_order_request(i);
                                let order = oms.create_order(request).await.unwrap();
                                order_ids.push(order.id);
                            }
                            
                            (oms, order_ids)
                        })
                    },
                    |(oms, order_ids)| async {
                        // Benchmark: retrieve orders
                        let start = Instant::now();
                        
                        // Test various retrieval patterns
                        let _active_orders = oms.get_active_orders();
                        
                        for &order_id in order_ids.iter().take(100) {
                            let _order = oms.get_order(&order_id);
                        }
                        
                        let _symbol_orders = oms.get_orders_by_symbol(Symbol(1));
                        let _metrics = oms.get_metrics();
                        
                        let duration = start.elapsed();
                        black_box(duration);
                    },
                    criterion::BatchSize::LargeInput,
                );
            },
        );
    }
    
    group.finish();
}

criterion_group!(
    name = oms_benches;
    config = Criterion::default()
        .sample_size(20)
        .measurement_time(Duration::from_secs(10))
        .warm_up_time(Duration::from_secs(2));
    targets = 
        bench_order_creation_throughput,
        bench_concurrent_order_creation,
        bench_order_matching_performance,
        bench_lifecycle_validation,
        bench_order_book_depth,
        bench_fill_processing,
        bench_mixed_concurrent_operations,
        bench_memory_efficiency,
        bench_order_retrieval
);

criterion_main!(oms_benches);

#[cfg(test)]
mod performance_tests {
    use super::*;
    use tokio::time::{sleep, Duration as TokioDuration};
    
    /// Test sustained throughput under load
    #[tokio::test]
    async fn test_sustained_throughput() {
        let config = create_test_config();
        let oms = Arc::new(OrderManagementSystem::new(config).await.unwrap());
        
        let duration = Duration::from_secs(30);
        let start = Instant::now();
        let mut order_count = 0;
        
        while start.elapsed() < duration {
            let request = create_test_order_request(order_count);
            let _order = oms.create_order(request).await.unwrap();
            order_count += 1;
            
            // Small delay to prevent overwhelming the system
            if order_count % 100 == 0 {
                sleep(TokioDuration::from_millis(1)).await;
            }
        }
        
        let elapsed = start.elapsed();
        let throughput = order_count as f64 / elapsed.as_secs_f64();
        
        println!("Sustained throughput: {:.2} orders/sec over {} seconds", throughput, elapsed.as_secs());
        assert!(throughput > 100.0, "Should sustain at least 100 orders/sec");
    }
    
    /// Test memory usage remains stable under load
    #[tokio::test]
    async fn test_memory_stability() {
        let config = create_test_config();
        let oms = OrderManagementSystem::new(config).await.unwrap();
        
        // Create many orders
        for i in 0..5000 {
            let request = create_test_order_request(i);
            let order = oms.create_order(request).await.unwrap();
            
            // Fill and remove some orders to test cleanup
            if i % 10 == 0 {
                let fill = Fill {
                    id: Uuid::new_v4(),
                    order_id: order.id,
                    execution_id: format!("MEM-TEST-{}", i),
                    quantity: order.quantity,
                    price: order.price.unwrap(),
                    commission: 100,
                    commission_currency: "USDT".to_string(),
                    timestamp: chrono::Utc::now(),
                    liquidity: LiquidityIndicator::Maker,
                };
                oms.process_fill(order.id, fill).await.unwrap();
            }
        }
        
        let final_metrics = oms.get_metrics();
        let active_orders = oms.get_active_orders();
        
        // Most orders should have been filled and removed from active set
        assert!(active_orders.len() < 5000, "Active orders should be less than total created");
        assert_eq!(final_metrics.orders_created, 5000);
        assert!(final_metrics.orders_filled > 0);
        
        println!("Final active orders: {}", active_orders.len());
        println!("Total orders created: {}", final_metrics.orders_created);
        println!("Orders filled: {}", final_metrics.orders_filled);
    }
    
    /// Test concurrent stress with mixed operations
    #[tokio::test]
    async fn test_concurrent_stress() {
        let config = create_test_config();
        let oms = Arc::new(OrderManagementSystem::new(config).await.unwrap());
        
        let num_threads = 8;
        let operations_per_thread = 500;
        let mut handles = Vec::new();
        
        let start = Instant::now();
        
        for thread_id in 0..num_threads {
            let oms_clone = Arc::clone(&oms);
            let handle = tokio::spawn(async move {
                let mut thread_orders = Vec::new();
                
                for i in 0..operations_per_thread {
                    let operation = i % 4;
                    
                    match operation {
                        0 => {
                            // Create order
                            let request = create_test_order_request(thread_id * operations_per_thread + i);
                            let order = oms_clone.create_order(request).await.unwrap();
                            thread_orders.push(order.id);
                        },
                        1 => {
                            // Submit random existing order
                            if !thread_orders.is_empty() {
                                let idx = i % thread_orders.len();
                                let _ = oms_clone.submit_order(thread_orders[idx]).await;
                            }
                        },
                        2 => {
                            // Fill random existing order
                            if !thread_orders.is_empty() {
                                let idx = i % thread_orders.len();
                                let fill = Fill {
                                    id: Uuid::new_v4(),
                                    order_id: thread_orders[idx],
                                    execution_id: format!("STRESS-{}-{}", thread_id, i),
                                    quantity: Qty::from_i64(100),
                                    price: Px::from_i64(1_000_000),
                                    commission: 1,
                                    commission_currency: "USDT".to_string(),
                                    timestamp: chrono::Utc::now(),
                                    liquidity: LiquidityIndicator::Taker,
                                };
                                let _ = oms_clone.process_fill(thread_orders[idx], fill).await;
                            }
                        },
                        3 => {
                            // Cancel random existing order
                            if !thread_orders.is_empty() {
                                let idx = i % thread_orders.len();
                                let _ = oms_clone.cancel_order(thread_orders[idx], "Stress test".to_string()).await;
                            }
                        },
                        _ => unreachable!(),
                    }
                }
            });
            handles.push(handle);
        }
        
        // Wait for all threads to complete
        for handle in handles {
            handle.await.unwrap();
        }
        
        let duration = start.elapsed();
        let total_operations = num_threads * operations_per_thread;
        let ops_per_second = total_operations as f64 / duration.as_secs_f64();
        
        println!("Completed {} concurrent operations in {:.2}s ({:.2} ops/sec)", 
                 total_operations, duration.as_secs_f64(), ops_per_second);
        
        let final_metrics = oms.get_metrics();
        println!("Final metrics: {:?}", final_metrics);
        
        // System should still be responsive
        let active_orders = oms.get_active_orders();
        assert!(active_orders.len() <= final_metrics.orders_created as usize);
        
        // Should achieve reasonable throughput
        assert!(ops_per_second > 1000.0, "Should achieve at least 1000 ops/sec under stress");
    }
    
    /// Test system behavior under resource constraints
    #[tokio::test]
    async fn test_resource_constraints() {
        let mut config = create_test_config();
        config.max_orders_memory = 1000; // Low limit
        
        let oms = OrderManagementSystem::new(config).await.unwrap();
        
        // Try to create more orders than the limit
        for i in 0..1500 {
            let request = create_test_order_request(i);
            let result = oms.create_order(request).await;
            
            // Should either succeed or fail gracefully
            if result.is_err() {
                println!("Order creation failed at {}: {:?}", i, result.err());
                break;
            }
        }
        
        let metrics = oms.get_metrics();
        let active_orders = oms.get_active_orders();
        
        println!("Orders created under constraints: {}", metrics.orders_created);
        println!("Active orders: {}", active_orders.len());
        
        // System should remain stable even if hitting limits
        assert!(metrics.orders_created > 0);
        assert!(active_orders.len() <= 1000);
    }
}