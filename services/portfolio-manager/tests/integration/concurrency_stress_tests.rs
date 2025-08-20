//! Concurrency and stress tests for portfolio manager
//! Tests high-frequency operations, concurrent access, and system limits

use portfolio_manager::{PortfolioManagerService, PortfolioManager, OptimizationStrategy, PortfolioConstraints};
use portfolio_manager::position::{Position, PositionTracker};
use portfolio_manager::market_feed::{MarketFeedManager, PriceUpdate};
use rstest::*;
use services_common::{Px, Qty, Side, Symbol, Ts};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tokio::task::JoinHandle;
use std::collections::HashMap;

// Test fixtures
#[fixture]
fn large_portfolio_manager() -> PortfolioManagerService {
    PortfolioManagerService::new(10000) // Large capacity
}

#[fixture]
fn stress_test_symbols() -> Vec<Symbol> {
    (1..=1000).map(Symbol::new).collect()
}

#[fixture]
fn concurrency_symbols() -> Vec<Symbol> {
    (1..=50).map(Symbol::new).collect()
}

mod high_frequency_position_updates {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_rapid_fill_processing(mut large_portfolio_manager: PortfolioManagerService) {
        let symbol = Symbol::new(1);
        let num_fills = 10000;
        let batch_size = 100;

        let start_time = Instant::now();

        // Process rapid fills in batches to avoid overwhelming the system
        for batch in 0..(num_fills / batch_size) {
            let mut handles = vec![];

            for i in 0..batch_size {
                let order_id = (batch * batch_size + i) as u64;
                let quantity = Qty::from_i64(10000 + (i as i64 * 100));
                let price = Px::from_i64(1000000 + (i as i64 * 1000));
                let side = if i % 2 == 0 { Side::Bid } else { Side::Ask };

                // Process fill
                large_portfolio_manager.process_fill(
                    order_id,
                    symbol,
                    side,
                    quantity,
                    price,
                    Ts::now()
                ).await.unwrap();
            }

            // Small delay between batches to prevent overwhelming
            if batch % 10 == 0 {
                sleep(Duration::from_millis(1)).await;
            }
        }

        let duration = start_time.elapsed();
        println!("Processed {} fills in {:?} ({:.2} fills/sec)", 
                num_fills, duration, num_fills as f64 / duration.as_secs_f64());

        // Verify final state
        let position = large_portfolio_manager.get_position(symbol).await;
        assert!(position.is_some());
        
        let metrics = large_portfolio_manager.get_metrics().await;
        assert_eq!(metrics.open_positions, 1);

        // Performance assertion: should complete within reasonable time
        assert!(duration < Duration::from_secs(30), "Processing took too long: {:?}", duration);
    }

    #[rstest]
    #[tokio::test] 
    async fn test_rapid_market_updates(mut large_portfolio_manager: PortfolioManagerService, concurrency_symbols: Vec<Symbol>) {
        // Create positions for symbols
        for (i, symbol) in concurrency_symbols.iter().take(10).enumerate() {
            large_portfolio_manager.process_fill(
                i as u64,
                *symbol,
                Side::Bid,
                Qty::from_i64(1000000),
                Px::from_i64(1000000),
                Ts::now()
            ).await.unwrap();
        }

        let num_updates = 50000;
        let start_time = Instant::now();

        // Rapid market updates
        for i in 0..num_updates {
            let symbol_idx = i % 10;
            let symbol = concurrency_symbols[symbol_idx];
            let base_price = 1000000 + ((i % 1000) as i64 * 100);
            
            large_portfolio_manager.update_market(
                symbol,
                Px::from_i64(base_price),
                Px::from_i64(base_price + 1000),
                Ts::now()
            ).await.unwrap();

            // Occasional small delay to prevent overwhelming
            if i % 1000 == 0 && i > 0 {
                sleep(Duration::from_micros(100)).await;
            }
        }

        let duration = start_time.elapsed();
        println!("Processed {} market updates in {:?} ({:.2} updates/sec)", 
                num_updates, duration, num_updates as f64 / duration.as_secs_f64());

        // Verify system is still responsive
        let metrics = large_portfolio_manager.get_metrics().await;
        assert_eq!(metrics.open_positions, 10);
        assert!(metrics.total_value != 0);

        // Performance assertion
        assert!(duration < Duration::from_secs(60), "Market updates took too long: {:?}", duration);
    }

    #[rstest]
    #[tokio::test]
    async fn test_mixed_high_frequency_operations(mut large_portfolio_manager: PortfolioManagerService, concurrency_symbols: Vec<Symbol>) {
        let num_operations = 20000;
        let start_time = Instant::now();
        let mut order_counter = AtomicU64::new(1);

        // Mixed operations: fills, market updates, metrics queries
        for i in 0..num_operations {
            let symbol = concurrency_symbols[i % concurrency_symbols.len().min(20)];
            
            match i % 4 {
                0 => {
                    // Process fill
                    let order_id = order_counter.fetch_add(1, Ordering::Relaxed);
                    large_portfolio_manager.process_fill(
                        order_id,
                        symbol,
                        if i % 2 == 0 { Side::Bid } else { Side::Ask },
                        Qty::from_i64(10000 + (i as i64 % 100000)),
                        Px::from_i64(1000000 + (i as i64 % 50000)),
                        Ts::now()
                    ).await.unwrap();
                },
                1 => {
                    // Market update
                    let base_price = 1000000 + (i as i64 % 100000);
                    large_portfolio_manager.update_market(
                        symbol,
                        Px::from_i64(base_price),
                        Px::from_i64(base_price + 1000),
                        Ts::now()
                    ).await.unwrap();
                },
                2 => {
                    // Get metrics (read operation)
                    let _ = large_portfolio_manager.get_metrics().await;
                },
                3 => {
                    // Get position (read operation)
                    let _ = large_portfolio_manager.get_position(symbol).await;
                },
                _ => unreachable!()
            }

            // Occasional brief pause
            if i % 5000 == 0 && i > 0 {
                sleep(Duration::from_millis(1)).await;
            }
        }

        let duration = start_time.elapsed();
        println!("Completed {} mixed operations in {:?} ({:.2} ops/sec)", 
                num_operations, duration, num_operations as f64 / duration.as_secs_f64());

        // Verify system integrity
        let final_metrics = large_portfolio_manager.get_metrics().await;
        assert!(final_metrics.open_positions > 0);

        let all_positions = large_portfolio_manager.get_all_positions().await;
        assert!(!all_positions.is_empty());

        assert!(duration < Duration::from_secs(120), "Mixed operations took too long: {:?}", duration);
    }
}

mod concurrent_access_tests {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_concurrent_position_updates(large_portfolio_manager: PortfolioManagerService) {
        let manager = Arc::new(large_portfolio_manager);
        let num_threads = 10;
        let operations_per_thread = 1000;
        let symbols: Vec<Symbol> = (1..=5).map(Symbol::new).collect();

        let start_time = Instant::now();
        let mut handles: Vec<JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>> = vec![];

        // Spawn concurrent tasks
        for thread_id in 0..num_threads {
            let mgr = Arc::clone(&manager);
            let syms = symbols.clone();
            
            let handle = tokio::spawn(async move {
                for i in 0..operations_per_thread {
                    let symbol = syms[i % syms.len()];
                    let order_id = (thread_id * operations_per_thread + i) as u64;
                    let quantity = Qty::from_i64((i + 1) as i64 * 1000);
                    let price = Px::from_i64(1000000 + (thread_id * 10000 + i * 100) as i64);
                    let side = if (thread_id + i) % 2 == 0 { Side::Bid } else { Side::Ask };

                    // Process fill
                    mgr.process_fill(order_id, symbol, side, quantity, price, Ts::now()).await?;

                    // Occasionally update market
                    if i % 100 == 0 {
                        let bid = Px::from_i64(price.as_i64() + 1000);
                        let ask = Px::from_i64(price.as_i64() + 2000);
                        mgr.update_market(symbol, bid, ask, Ts::now()).await?;
                    }

                    // Brief yield to allow other tasks
                    if i % 50 == 0 {
                        tokio::task::yield_now().await;
                    }
                }
                Ok(())
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        let duration = start_time.elapsed();
        let total_operations = num_threads * operations_per_thread;
        println!("Completed {} concurrent operations in {:?} ({:.2} ops/sec)", 
                total_operations, duration, total_operations as f64 / duration.as_secs_f64());

        // Verify final state consistency
        let metrics = manager.get_metrics().await;
        assert!(metrics.open_positions > 0);
        assert!(metrics.open_positions <= symbols.len() as u32);

        let positions = manager.get_all_positions().await;
        assert!(!positions.is_empty());

        // All positions should have consistent state
        for position in positions {
            assert_ne!(position.quantity, 0); // Should have some quantity
        }

        assert!(duration < Duration::from_secs(30), "Concurrent operations took too long: {:?}", duration);
    }

    #[rstest]
    #[tokio::test]
    async fn test_concurrent_read_write_operations(large_portfolio_manager: PortfolioManagerService, concurrency_symbols: Vec<Symbol>) {
        let manager = Arc::new(large_portfolio_manager);
        let num_readers = 5;
        let num_writers = 5;
        let operations_per_task = 2000;

        // Initialize some positions
        for (i, symbol) in concurrency_symbols.iter().take(10).enumerate() {
            manager.process_fill(
                i as u64,
                *symbol,
                Side::Bid,
                Qty::from_i64(1000000),
                Px::from_i64(1000000),
                Ts::now()
            ).await.unwrap();
        }

        let start_time = Instant::now();
        let mut handles = vec![];

        // Spawn reader tasks
        for reader_id in 0..num_readers {
            let mgr = Arc::clone(&manager);
            let symbols = concurrency_symbols.clone();
            
            let handle = tokio::spawn(async move {
                for i in 0..operations_per_task {
                    match i % 4 {
                        0 => { let _ = mgr.get_metrics().await; },
                        1 => { let _ = mgr.get_all_positions().await; },
                        2 => { let _ = mgr.get_pnl_breakdown().await; },
                        3 => { 
                            let symbol = symbols[i % symbols.len().min(10)];
                            let _ = mgr.get_position(symbol).await; 
                        },
                        _ => unreachable!()
                    }

                    if i % 200 == 0 {
                        tokio::task::yield_now().await;
                    }
                }
            });
            handles.push(handle);
        }

        // Spawn writer tasks
        for writer_id in 0..num_writers {
            let mgr = Arc::clone(&manager);
            let symbols = concurrency_symbols.clone();
            
            let handle = tokio::spawn(async move {
                for i in 0..operations_per_task {
                    let symbol = symbols[i % symbols.len().min(10)];
                    
                    if i % 2 == 0 {
                        // Market update
                        let base_price = 1000000 + (writer_id * 10000 + i * 100) as i64;
                        let _ = mgr.update_market(
                            symbol,
                            Px::from_i64(base_price),
                            Px::from_i64(base_price + 1000),
                            Ts::now()
                        ).await;
                    } else {
                        // Fill processing
                        let order_id = ((writer_id + num_readers) * operations_per_task + i) as u64;
                        let _ = mgr.process_fill(
                            order_id,
                            symbol,
                            if i % 2 == 0 { Side::Bid } else { Side::Ask },
                            Qty::from_i64(1000 + (i * 100) as i64),
                            Px::from_i64(1000000 + (i * 1000) as i64),
                            Ts::now()
                        ).await;
                    }

                    if i % 100 == 0 {
                        tokio::task::yield_now().await;
                    }
                }
            });
            handles.push(handle);
        }

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }

        let duration = start_time.elapsed();
        let total_operations = (num_readers + num_writers) * operations_per_task;
        println!("Completed {} concurrent read/write operations in {:?} ({:.2} ops/sec)", 
                total_operations, duration, total_operations as f64 / duration.as_secs_f64());

        // Verify data consistency
        let final_metrics = manager.get_metrics().await;
        assert!(final_metrics.open_positions > 0);

        let positions = manager.get_all_positions().await;
        assert!(!positions.is_empty());

        assert!(duration < Duration::from_secs(60), "Concurrent read/write took too long: {:?}", duration);
    }

    #[rstest]
    #[tokio::test]
    async fn test_concurrent_optimization_operations(large_portfolio_manager: PortfolioManagerService, concurrency_symbols: Vec<Symbol>) {
        let manager = Arc::new(large_portfolio_manager);

        // Create diverse portfolio
        for (i, symbol) in concurrency_symbols.iter().take(20).enumerate() {
            manager.process_fill(
                i as u64,
                *symbol,
                Side::Bid,
                Qty::from_i64((i + 1) as i64 * 100000),
                Px::from_i64(1000000 + (i * 50000) as i64),
                Ts::now()
            ).await.unwrap();
        }

        let num_optimization_tasks = 5;
        let optimizations_per_task = 100;
        let start_time = Instant::now();
        let mut handles = vec![];

        // Spawn concurrent optimization tasks
        for task_id in 0..num_optimization_tasks {
            let mgr = Arc::clone(&manager);
            
            let handle = tokio::spawn(async move {
                let strategies = vec![
                    OptimizationStrategy::EqualWeight,
                    OptimizationStrategy::MinimumVariance,
                    OptimizationStrategy::MaxSharpe,
                    OptimizationStrategy::RiskParity,
                ];
                
                for i in 0..optimizations_per_task {
                    let strategy = strategies[i % strategies.len()];
                    let constraints = PortfolioConstraints::default();
                    
                    let result = mgr.optimize(strategy, &constraints).await;
                    assert!(result.is_ok());
                    
                    // Occasionally execute rebalance
                    if i % 20 == 0 && result.is_ok() {
                        let changes = result.unwrap();
                        if !changes.is_empty() {
                            let _ = mgr.rebalance(changes).await;
                        }
                    }

                    if i % 10 == 0 {
                        tokio::task::yield_now().await;
                    }
                }
            });
            handles.push(handle);
        }

        // Also run concurrent market updates
        let update_handle = {
            let mgr = Arc::clone(&manager);
            let symbols = concurrency_symbols.clone();
            tokio::spawn(async move {
                for i in 0..1000 {
                    let symbol = symbols[i % symbols.len().min(20)];
                    let base_price = 1000000 + (i as i64 * 1000);
                    let _ = mgr.update_market(
                        symbol,
                        Px::from_i64(base_price),
                        Px::from_i64(base_price + 1000),
                        Ts::now()
                    ).await;
                    
                    if i % 50 == 0 {
                        sleep(Duration::from_millis(1)).await;
                    }
                }
            })
        };
        handles.push(update_handle);

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }

        let duration = start_time.elapsed();
        println!("Completed concurrent optimization operations in {:?}", duration);

        // Verify final state
        let metrics = manager.get_metrics().await;
        assert!(metrics.open_positions > 0);

        assert!(duration < Duration::from_secs(120), "Concurrent optimizations took too long: {:?}", duration);
    }
}

mod stress_testing {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_large_number_of_symbols(stress_test_symbols: Vec<Symbol>) {
        let mut manager = PortfolioManagerService::new(stress_test_symbols.len());
        let num_symbols = stress_test_symbols.len().min(1000); // Limit for test performance
        
        println!("Testing with {} symbols", num_symbols);
        let start_time = Instant::now();

        // Create position for each symbol
        for (i, symbol) in stress_test_symbols.iter().take(num_symbols).enumerate() {
            manager.process_fill(
                i as u64,
                *symbol,
                Side::Bid,
                Qty::from_i64((i + 1) as i64 * 1000),
                Px::from_i64(1000000 + (i * 1000) as i64),
                Ts::now()
            ).await.unwrap();

            // Progress indicator
            if i % 100 == 0 && i > 0 {
                println!("Created {} positions", i);
            }
        }

        let creation_duration = start_time.elapsed();
        println!("Created {} positions in {:?}", num_symbols, creation_duration);

        // Update market prices for all symbols
        let update_start = Instant::now();
        for (i, symbol) in stress_test_symbols.iter().take(num_symbols).enumerate() {
            let base_price = 1100000 + (i * 1000) as i64;
            manager.update_market(
                *symbol,
                Px::from_i64(base_price),
                Px::from_i64(base_price + 1000),
                Ts::now()
            ).await.unwrap();
        }
        let update_duration = update_start.elapsed();
        println!("Updated {} market prices in {:?}", num_symbols, update_duration);

        // Verify final state
        let metrics = manager.get_metrics().await;
        assert_eq!(metrics.open_positions, num_symbols as u32);
        assert!(metrics.total_value != 0);

        let positions = manager.get_all_positions().await;
        assert_eq!(positions.len(), num_symbols);

        // Performance assertions
        let total_duration = start_time.elapsed();
        assert!(total_duration < Duration::from_secs(300), "Large symbol test took too long: {:?}", total_duration);
        
        println!("Total test duration: {:?}", total_duration);
    }

    #[rstest]
    #[tokio::test]
    async fn test_memory_intensive_operations() {
        let num_positions = 5000;
        let mut manager = PortfolioManagerService::new(num_positions);
        
        println!("Testing memory intensive operations with {} positions", num_positions);
        let start_time = Instant::now();

        // Create many positions
        for i in 0..num_positions {
            let symbol = Symbol::new((i % 1000 + 1) as u32); // Reuse symbols to create larger positions
            manager.process_fill(
                i as u64,
                symbol,
                if i % 2 == 0 { Side::Bid } else { Side::Ask },
                Qty::from_i64((i + 1) as i64 * 1000),
                Px::from_i64(1000000 + (i * 100) as i64),
                Ts::now()
            ).await.unwrap();
        }

        // Multiple optimization runs (memory intensive)
        for i in 0..10 {
            let result = manager.optimize(
                OptimizationStrategy::EqualWeight,
                &PortfolioConstraints::default()
            ).await;
            assert!(result.is_ok());
            
            if i % 3 == 0 {
                println!("Completed optimization round {}", i + 1);
            }
        }

        // Multiple metrics calculations
        for _ in 0..100 {
            let _ = manager.get_metrics().await;
            let _ = manager.get_all_positions().await;
            let _ = manager.get_pnl_breakdown().await;
        }

        let duration = start_time.elapsed();
        println!("Memory intensive test completed in {:?}", duration);

        // Verify system is still responsive
        let final_metrics = manager.get_metrics().await;
        assert!(final_metrics.open_positions > 0);

        assert!(duration < Duration::from_secs(180), "Memory intensive test took too long: {:?}", duration);
    }

    #[rstest]
    #[tokio::test]
    async fn test_extreme_position_sizes() {
        let mut manager = PortfolioManagerService::new(10);
        let symbol = Symbol::new(1);

        // Extremely large quantities
        let large_quantities = vec![
            1_000_000_000_000i64,      // 1 trillion
            500_000_000_000i64,        // 500 billion  
            2_000_000_000_000i64,      // 2 trillion
        ];

        let large_prices = vec![
            100_000_000i64,            // $10,000
            50_000_000i64,             // $5,000
            200_000_000i64,            // $20,000
        ];

        println!("Testing extreme position sizes");

        for (i, (qty, price)) in large_quantities.iter().zip(large_prices.iter()).enumerate() {
            let result = manager.process_fill(
                i as u64,
                symbol,
                Side::Bid,
                Qty::from_i64(*qty),
                Px::from_i64(*price),
                Ts::now()
            ).await;
            assert!(result.is_ok(), "Failed to process large fill: qty={}, price={}", qty, price);
        }

        // Update with extreme market prices
        manager.update_market(
            symbol,
            Px::from_i64(300_000_000), // $30,000
            Px::from_i64(301_000_000), // $30,100
            Ts::now()
        ).await.unwrap();

        // Verify calculations still work
        let position = manager.get_position(symbol).await;
        assert!(position.is_some());

        let metrics = manager.get_metrics().await;
        assert_eq!(metrics.open_positions, 1);
        assert_ne!(metrics.total_value, 0);

        println!("Extreme position sizes handled successfully");
    }

    #[rstest]
    #[tokio::test]
    async fn test_rapid_position_flipping() {
        let mut manager = PortfolioManagerService::new(10);
        let symbol = Symbol::new(1);
        let num_flips = 1000;

        println!("Testing rapid position flipping ({} flips)", num_flips);
        let start_time = Instant::now();

        for i in 0..num_flips {
            let quantity = Qty::from_i64(1000000 + (i % 500000) as i64);
            let price = Px::from_i64(1000000 + (i % 100000) as i64);
            let side = if i % 2 == 0 { Side::Bid } else { Side::Ask };

            manager.process_fill(
                i as u64,
                symbol,
                side,
                quantity,
                price,
                Ts::now()
            ).await.unwrap();

            // Update market occasionally
            if i % 100 == 0 {
                let market_price = 1000000 + ((i / 100) * 5000) as i64;
                manager.update_market(
                    symbol,
                    Px::from_i64(market_price),
                    Px::from_i64(market_price + 1000),
                    Ts::now()
                ).await.unwrap();
            }
        }

        let duration = start_time.elapsed();
        println!("Completed {} position flips in {:?} ({:.2} flips/sec)", 
                num_flips, duration, num_flips as f64 / duration.as_secs_f64());

        // Verify final state is consistent
        let position = manager.get_position(symbol).await;
        assert!(position.is_some());

        let metrics = manager.get_metrics().await;
        assert_eq!(metrics.open_positions, 1);

        assert!(duration < Duration::from_secs(60), "Position flipping took too long: {:?}", duration);
    }

    #[rstest]
    #[tokio::test]
    async fn test_market_feed_stress() {
        let symbols: Vec<Symbol> = (1..=100).map(Symbol::new).collect();
        let manager = MarketFeedManager::new(&symbols, 10000);

        let num_updates = 100000;
        println!("Stress testing market feed with {} updates", num_updates);
        
        let start_time = Instant::now();

        for i in 0..num_updates {
            let symbol = symbols[i % symbols.len()];
            let base_price = 1000000 + (i % 100000) as i64;
            
            let update = PriceUpdate {
                symbol,
                bid: base_price,
                ask: base_price + 1000,
                last: base_price + 500,
                volume: 1000000 + (i % 5000000) as i64,
                timestamp: Ts::now().as_nanos(),
            };

            manager.update_price(update).unwrap();

            // Occasional index updates
            if i % 1000 == 0 {
                let index_price = 180000 + (i / 1000 * 1000) as i64;
                manager.update_index("NIFTY", index_price, index_price + 100, index_price + 50).unwrap();
            }
        }

        let duration = start_time.elapsed();
        println!("Processed {} market updates in {:?} ({:.2} updates/sec)", 
                num_updates, duration, num_updates as f64 / duration.as_secs_f64());

        // Verify final state
        for symbol in symbols.iter().take(10) {
            let price = manager.get_price(*symbol);
            assert!(price.is_some());
        }

        assert!(duration < Duration::from_secs(120), "Market feed stress test took too long: {:?}", duration);
    }
}

mod atomic_operations_stress {
    use super::*;

    #[rstest]
    fn test_position_atomic_operations_stress() {
        let position = Arc::new(Position::new(Symbol::new(1)));
        let num_threads = 20;
        let operations_per_thread = 10000;

        println!("Stress testing position atomic operations");
        let start_time = Instant::now();

        let handles: Vec<_> = (0..num_threads)
            .map(|thread_id| {
                let pos = Arc::clone(&position);
                std::thread::spawn(move || {
                    for i in 0..operations_per_thread {
                        let quantity = Qty::from_i64(1000 + (i % 1000) as i64);
                        let price = Px::from_i64(1000000 + (thread_id * 1000 + i) as i64);
                        let side = if (thread_id + i) % 2 == 0 { Side::Bid } else { Side::Ask };

                        // Apply fill
                        pos.apply_fill(side, quantity, price, Ts::now());

                        // Market update
                        if i % 10 == 0 {
                            let bid = Px::from_i64(price.as_i64() + 100);
                            let ask = Px::from_i64(price.as_i64() + 200);
                            pos.update_market(bid, ask, Ts::now());
                        }

                        // Read operations
                        let _ = pos.total_pnl();
                        let _ = pos.snapshot();
                    }
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        let duration = start_time.elapsed();
        let total_operations = num_threads * operations_per_thread;
        println!("Completed {} atomic operations in {:?} ({:.2} ops/sec)", 
                total_operations, duration, total_operations as f64 / duration.as_secs_f64());

        // Verify position is still consistent
        let final_snapshot = position.snapshot();
        assert_ne!(final_snapshot.quantity, 0); // Should have some position

        assert!(duration < Duration::from_secs(30), "Atomic operations stress test took too long: {:?}", duration);
    }

    #[rstest]
    fn test_position_tracker_stress() {
        let tracker = Arc::new(PositionTracker::new(1000));
        let num_threads = 15;
        let operations_per_thread = 5000;
        let symbols: Vec<Symbol> = (1..=50).map(Symbol::new).collect();

        println!("Stress testing position tracker with concurrent access");
        let start_time = Instant::now();

        let handles: Vec<_> = (0..num_threads)
            .map(|thread_id| {
                let t = Arc::clone(&tracker);
                let syms = symbols.clone();
                std::thread::spawn(move || {
                    for i in 0..operations_per_thread {
                        let symbol = syms[i % syms.len()];
                        let order_id = (thread_id * operations_per_thread + i) as u64;
                        let quantity = Qty::from_i64((i + 1) as i64 * 100);
                        let price = Px::from_i64(1000000 + (thread_id * 1000 + i) as i64);
                        let side = if (thread_id + i) % 2 == 0 { Side::Bid } else { Side::Ask };

                        // Add and fill order
                        t.add_pending(order_id, symbol, side, quantity);
                        t.apply_fill(order_id, quantity, price, Ts::now());

                        // Market update
                        if i % 20 == 0 {
                            let bid = Px::from_i64(price.as_i64() + 500);
                            let ask = Px::from_i64(price.as_i64() + 600);
                            t.update_market(symbol, bid, ask, Ts::now());
                        }

                        // Read operations
                        if i % 50 == 0 {
                            let _ = t.get_all_positions();
                            let _ = t.get_global_pnl();
                        }
                    }
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        let duration = start_time.elapsed();
        let total_operations = num_threads * operations_per_thread;
        println!("Completed {} tracker operations in {:?} ({:.2} ops/sec)", 
                total_operations, duration, total_operations as f64 / duration.as_secs_f64());

        // Verify final consistency
        let positions = tracker.get_all_positions();
        assert!(!positions.is_empty());

        let (realized, unrealized, total) = tracker.get_global_pnl();
        assert_eq!(total, realized + unrealized);

        // Force reconciliation and verify
        tracker.reconcile_global_pnl();
        let (r2, u2, t2) = tracker.get_global_pnl();
        assert_eq!(t2, r2 + u2);

        assert!(duration < Duration::from_secs(45), "Position tracker stress test took too long: {:?}", duration);
    }
}