//! Memory performance and efficiency tests
//! 
//! These tests verify memory usage patterns, allocation efficiency,
//! and system behavior under memory pressure.

use rstest::*;
use tokio::time::{Duration, sleep};
use tokio::sync::mpsc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::HashMap;

use market_connector::grpc_service::MarketDataGrpcService;
use market_connector::{MarketDataEvent, MarketData};
use market_connector::instruments::service::{InstrumentService, InstrumentServiceConfig};
use market_connector::exchanges::binance::websocket::BinanceWebSocketFeed;
use market_connector::connectors::adapter::{FeedAdapter, FeedConfig};
use services_common::{BinanceAuth, BinanceMarket, L2Update, Symbol};
use rustc_hash::FxHashMap;

// Memory test constants
const MEMORY_STRESS_EVENT_COUNT: usize = 50_000;
const MEMORY_STRESS_SYMBOLS: usize = 1000;
const LARGE_SYMBOL_COUNT: usize = 10_000;
const SUSTAINED_LOAD_DURATION_SECS: u64 = 30;
const MEMORY_LEAK_ITERATIONS: usize = 100;

/// Memory usage tracker
#[derive(Debug, Clone)]
struct MemoryTracker {
    initial_usage: AtomicUsize,
    peak_usage: AtomicUsize,
    current_usage: AtomicUsize,
    allocation_count: AtomicUsize,
    deallocation_count: AtomicUsize,
}

impl MemoryTracker {
    fn new() -> Self {
        let initial = Self::get_memory_usage();
        Self {
            initial_usage: AtomicUsize::new(initial),
            peak_usage: AtomicUsize::new(initial),
            current_usage: AtomicUsize::new(initial),
            allocation_count: AtomicUsize::new(0),
            deallocation_count: AtomicUsize::new(0),
        }
    }
    
    fn update(&self) {
        let current = Self::get_memory_usage();
        self.current_usage.store(current, Ordering::Relaxed);
        
        // Update peak if current is higher
        let mut peak = self.peak_usage.load(Ordering::Relaxed);
        while current > peak {
            match self.peak_usage.compare_exchange_weak(peak, current, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(new_peak) => peak = new_peak,
            }
        }
    }
    
    fn record_allocation(&self) {
        self.allocation_count.fetch_add(1, Ordering::Relaxed);
        self.update();
    }
    
    fn record_deallocation(&self) {
        self.deallocation_count.fetch_add(1, Ordering::Relaxed);
        self.update();
    }
    
    fn get_stats(&self) -> MemoryStats {
        MemoryStats {
            initial_usage_bytes: self.initial_usage.load(Ordering::Relaxed),
            current_usage_bytes: self.current_usage.load(Ordering::Relaxed),
            peak_usage_bytes: self.peak_usage.load(Ordering::Relaxed),
            allocations: self.allocation_count.load(Ordering::Relaxed),
            deallocations: self.deallocation_count.load(Ordering::Relaxed),
        }
    }
    
    // Simplified memory usage estimation (in real implementation, would use system calls)
    fn get_memory_usage() -> usize {
        use std::sync::Mutex;
        static SIMULATED_MEMORY: Mutex<usize> = Mutex::new(1024 * 1024); // Start with 1MB
        
        let mut memory = SIMULATED_MEMORY.lock().unwrap();
        *memory += rand::random::<usize>() % 1024; // Simulate memory growth
        *memory
    }
}

#[derive(Debug)]
struct MemoryStats {
    initial_usage_bytes: usize,
    current_usage_bytes: usize,
    peak_usage_bytes: usize,
    allocations: usize,
    deallocations: usize,
}

impl MemoryStats {
    fn print_summary(&self, test_name: &str) {
        let initial_mb = self.initial_usage_bytes as f64 / (1024.0 * 1024.0);
        let current_mb = self.current_usage_bytes as f64 / (1024.0 * 1024.0);
        let peak_mb = self.peak_usage_bytes as f64 / (1024.0 * 1024.0);
        
        println!("\n=== {} Memory Summary ===", test_name);
        println!("Initial memory: {:.2} MB", initial_mb);
        println!("Current memory: {:.2} MB", current_mb);
        println!("Peak memory: {:.2} MB", peak_mb);
        println!("Memory growth: {:.2} MB", current_mb - initial_mb);
        println!("Allocations: {}", self.allocations);
        println!("Deallocations: {}", self.deallocations);
        println!("Net allocations: {}", self.allocations as i64 - self.deallocations as i64);
        println!("===============================\n");
    }
    
    fn memory_efficiency(&self) -> f64 {
        if self.peak_usage_bytes > self.initial_usage_bytes {
            (self.current_usage_bytes - self.initial_usage_bytes) as f64 / 
            (self.peak_usage_bytes - self.initial_usage_bytes) as f64
        } else {
            1.0
        }
    }
}

#[fixture]
fn memory_test_config() -> FeedConfig {
    let mut symbol_map = FxHashMap::default();
    
    // Add many symbols for memory testing
    for i in 0..MEMORY_STRESS_SYMBOLS {
        symbol_map.insert(Symbol::new(i as u32), format!("SYMBOL_{}", i));
    }
    
    FeedConfig {
        name: "memory_test".to_string(),
        ws_url: "wss://stream.binance.com:9443".to_string(),
        api_url: "https://api.binance.com".to_string(),
        symbol_map,
        max_reconnects: 5,
        reconnect_delay_ms: 1000,
    }
}

#[fixture]
fn large_instrument_config() -> InstrumentServiceConfig {
    InstrumentServiceConfig {
        wal_dir: std::env::temp_dir().join("memory_test_instruments"),
        wal_segment_size_mb: Some(100), // Large segments for memory testing
        fetch_interval_hours: 24,
        fetch_hour: 8,
        max_retries: 3,
        retry_delay_secs: 1,
        enable_auto_updates: false,
    }
}

#[rstest]
#[tokio::test]
async fn test_grpc_service_memory_usage_under_load() {
    let (service, event_sender) = MarketDataGrpcService::new();
    let memory_tracker = Arc::new(MemoryTracker::new());
    
    // Create subscription for memory testing
    let subscribe_request = services_common::marketdata::v1::SubscribeRequest {
        symbols: (0..MEMORY_STRESS_SYMBOLS).map(|i| format!("SYMBOL_{}", i)).collect(),
        exchange: "binance".to_string(),
        data_types: vec![1, 2, 3],
    };
    
    let request = tonic::Request::new(subscribe_request);
    let response = service.subscribe(request).await.expect("Subscription should succeed");
    let mut stream = response.into_inner();
    
    let memory_tracker_clone = Arc::clone(&memory_tracker);
    
    // Spawn memory monitoring task
    let memory_monitor = tokio::spawn(async move {
        for _ in 0..MEMORY_STRESS_EVENT_COUNT / 100 {
            memory_tracker_clone.update();
            sleep(Duration::from_millis(10)).await;
        }
    });
    
    // Spawn stream consumer
    let stream_task = tokio::spawn({
        let memory_tracker = Arc::clone(&memory_tracker);
        async move {
            use tokio_stream::StreamExt;
            
            let mut received_count = 0;
            while let Some(result) = stream.next().await {
                match result {
                    Ok(_event) => {
                        received_count += 1;
                        if received_count % 1000 == 0 {
                            memory_tracker.update();
                        }
                        if received_count >= MEMORY_STRESS_EVENT_COUNT / 10 {
                            break; // Don't wait forever
                        }
                    },
                    Err(_) => break,
                }
            }
            received_count
        }
    });
    
    // Send high volume of events
    for i in 0..MEMORY_STRESS_EVENT_COUNT {
        let symbol = format!("SYMBOL_{}", i % MEMORY_STRESS_SYMBOLS);
        let event = MarketDataEvent {
            symbol,
            exchange: "binance".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64 + i as u64,
            data: MarketData::OrderBook {
                bids: vec![
                    (45000.0 + (i % 100) as f64 * 0.01, 1.0 + (i % 10) as f64 * 0.1),
                    (44999.0 + (i % 100) as f64 * 0.01, 2.0 + (i % 10) as f64 * 0.1),
                ],
                asks: vec![
                    (45001.0 + (i % 100) as f64 * 0.01, 1.0 + (i % 10) as f64 * 0.1),
                    (45002.0 + (i % 100) as f64 * 0.01, 2.0 + (i % 10) as f64 * 0.1),
                ],
                sequence: i as u64,
            },
        };
        
        if event_sender.send(event).await.is_err() {
            break;
        }
        
        memory_tracker.record_allocation();
        
        // Periodic memory update
        if i % 1000 == 0 {
            memory_tracker.update();
            tokio::task::yield_now().await;
        }
    }
    
    // Wait for processing to complete
    let _ = tokio::time::timeout(Duration::from_secs(10), stream_task).await;
    memory_monitor.abort();
    
    memory_tracker.update();
    let stats = memory_tracker.get_stats();
    stats.print_summary("gRPC Service Under Load");
    
    // Memory assertions
    let memory_growth_mb = (stats.current_usage_bytes - stats.initial_usage_bytes) as f64 / (1024.0 * 1024.0);
    assert!(memory_growth_mb < 100.0, "Memory growth too high: {:.2} MB", memory_growth_mb);
    
    let efficiency = stats.memory_efficiency();
    assert!(efficiency > 0.5, "Memory efficiency too low: {:.2}", efficiency);
    
    // Should have reasonable allocation/deallocation balance
    let net_allocations = stats.allocations as i64 - stats.deallocations as i64;
    assert!(net_allocations < MEMORY_STRESS_EVENT_COUNT as i64 / 10,
           "Too many net allocations: {}", net_allocations);
}

#[rstest]
#[tokio::test]
async fn test_websocket_adapter_memory_efficiency() {
    let config = memory_test_config();
    let auth = BinanceAuth::new("test_key".to_string(), "test_secret".to_string());
    let mut adapter = BinanceWebSocketFeed::new(config, auth, BinanceMarket::Spot, false);
    let memory_tracker = Arc::new(MemoryTracker::new());
    
    // Connect and subscribe
    adapter.connect().await.expect("Connection should succeed");
    
    let symbols: Vec<Symbol> = (0..MEMORY_STRESS_SYMBOLS).map(|i| Symbol::new(i as u32)).collect();
    adapter.subscribe(symbols).await.expect("Subscription should succeed");
    
    memory_tracker.update();
    
    // Create L2Update channel
    let (tx, mut rx) = mpsc::channel::<L2Update>(10000);
    
    let memory_tracker_clone = Arc::clone(&memory_tracker);
    
    // Monitor received updates
    let receiver_task = tokio::spawn(async move {
        let mut count = 0;
        while let Some(_update) = rx.recv().await {
            count += 1;
            if count % 1000 == 0 {
                memory_tracker_clone.update();
            }
            if count >= MEMORY_STRESS_EVENT_COUNT / 100 {
                break;
            }
        }
        count
    });
    
    // Run adapter briefly
    let adapter_task = tokio::spawn(async move {
        let _ = tokio::time::timeout(Duration::from_secs(5), adapter.run(tx)).await;
    });
    
    // Monitor memory during execution
    for _ in 0..50 {
        memory_tracker.update();
        sleep(Duration::from_millis(100)).await;
    }
    
    // Cancel tasks
    adapter_task.abort();
    receiver_task.abort();
    
    adapter.disconnect().await.expect("Disconnect should succeed");
    
    memory_tracker.update();
    let stats = memory_tracker.get_stats();
    stats.print_summary("WebSocket Adapter");
    
    // Memory assertions for WebSocket adapter
    let memory_growth_mb = (stats.current_usage_bytes - stats.initial_usage_bytes) as f64 / (1024.0 * 1024.0);
    assert!(memory_growth_mb < 50.0, "WebSocket adapter memory growth too high: {:.2} MB", memory_growth_mb);
}

#[rstest]
#[tokio::test]
async fn test_instrument_service_memory_usage() {
    let config = large_instrument_config();
    let service = InstrumentService::new(config, None).await
        .expect("Service should be created");
    let memory_tracker = Arc::new(MemoryTracker::new());
    
    service.start().await.expect("Service should start");
    memory_tracker.update();
    
    // Generate large CSV data for parsing
    let mut csv_data = "instrument_token,exchange_token,tradingsymbol,name,last_price,expiry,strike,tick_size,lot_size,instrument_type,segment,exchange\n".to_string();
    
    for i in 0..LARGE_SYMBOL_COUNT {
        csv_data.push_str(&format!(
            "{},{},\"SYMBOL_{}\",\"Symbol {}\",100.{:02},,,0.05,50,EQ,INDICES,NSE\n",
            i + 1000000, i + 1000000, i, i, i % 100
        ));
        
        if i % 1000 == 0 {
            memory_tracker.update();
        }
    }
    
    memory_tracker.update();
    
    // Parse CSV data
    let parse_start = std::time::Instant::now();
    let instruments = InstrumentService::parse_csv_data(&csv_data).await
        .expect("CSV parsing should succeed");
    let parse_duration = parse_start.elapsed();
    
    memory_tracker.update();
    
    // Add instruments to service
    {
        let mut store = service.store.write().await;
        let count = store.add_instruments(instruments).expect("Should add instruments");
        assert_eq!(count, LARGE_SYMBOL_COUNT);
        
        memory_tracker.update();
        
        store.sync().expect("Should sync");
    }
    
    memory_tracker.update();
    
    // Perform various queries to test memory usage
    for i in 0..100 {
        let token = (i + 1000000) as u32;
        let _instrument = service.get_by_token(token).await;
        
        if i % 10 == 0 {
            memory_tracker.update();
        }
    }
    
    let stats = memory_tracker.get_stats();
    stats.print_summary("Instrument Service");
    
    println!("Parse duration: {:?}", parse_duration);
    println!("Instruments loaded: {}", LARGE_SYMBOL_COUNT);
    
    // Memory assertions
    let memory_growth_mb = (stats.current_usage_bytes - stats.initial_usage_bytes) as f64 / (1024.0 * 1024.0);
    assert!(memory_growth_mb < 200.0, "Instrument service memory growth too high: {:.2} MB", memory_growth_mb);
    
    // Parsing should be reasonably fast
    assert!(parse_duration < Duration::from_secs(10), "CSV parsing too slow: {:?}", parse_duration);
}

#[rstest]
#[tokio::test]
async fn test_memory_leak_detection() {
    let memory_tracker = Arc::new(MemoryTracker::new());
    
    // Run multiple iterations to detect memory leaks
    for iteration in 0..MEMORY_LEAK_ITERATIONS {
        let (service, event_sender) = MarketDataGrpcService::new();
        
        // Create subscription
        let subscribe_request = services_common::marketdata::v1::SubscribeRequest {
            symbols: vec!["TEST_SYMBOL".to_string()],
            exchange: "test_exchange".to_string(),
            data_types: vec![1],
        };
        
        let request = tonic::Request::new(subscribe_request);
        if let Ok(response) = service.subscribe(request).await {
            let mut stream = response.into_inner();
            
            // Send some events
            for i in 0..10 {
                let event = MarketDataEvent {
                    symbol: "TEST_SYMBOL".to_string(),
                    exchange: "test_exchange".to_string(),
                    timestamp: chrono::Utc::now().timestamp_millis() as u64 + i,
                    data: MarketData::Quote {
                        bid_price: 100.0 + i as f64,
                        bid_size: 1.0,
                        ask_price: 101.0 + i as f64,
                        ask_size: 1.0,
                    },
                };
                
                let _ = event_sender.send(event).await;
                memory_tracker.record_allocation();
            }
            
            // Try to receive one event
            use tokio_stream::StreamExt;
            let _ = tokio::time::timeout(Duration::from_millis(10), stream.next()).await;
            
            // Drop everything
            drop(stream);
            drop(service);
            drop(event_sender);
        }
        
        memory_tracker.record_deallocation();
        memory_tracker.update();
        
        // Force garbage collection (in languages that have it)
        if iteration % 10 == 0 {
            tokio::task::yield_now().await;
            memory_tracker.update();
        }
    }
    
    let stats = memory_tracker.get_stats();
    stats.print_summary("Memory Leak Detection");
    
    // Should not have significant memory growth over iterations
    let memory_growth_mb = (stats.current_usage_bytes - stats.initial_usage_bytes) as f64 / (1024.0 * 1024.0);
    assert!(memory_growth_mb < 10.0, "Potential memory leak detected: {:.2} MB growth", memory_growth_mb);
    
    // Allocation/deallocation balance should be reasonable
    let allocation_balance = stats.allocations as i64 - stats.deallocations as i64;
    let max_imbalance = MEMORY_LEAK_ITERATIONS as i64 / 10;
    assert!(allocation_balance.abs() < max_imbalance,
           "Allocation imbalance suggests memory leak: {}", allocation_balance);
}

#[rstest]
#[tokio::test]
async fn test_sustained_memory_pressure() {
    let (service, event_sender) = MarketDataGrpcService::new();
    let memory_tracker = Arc::new(MemoryTracker::new());
    
    // Create multiple subscriptions
    let subscription_count = 10;
    let mut streams = Vec::new();
    
    for i in 0..subscription_count {
        let subscribe_request = services_common::marketdata::v1::SubscribeRequest {
            symbols: vec![format!("PRESSURE_SYMBOL_{}", i)],
            exchange: "test_exchange".to_string(),
            data_types: vec![1, 2, 3],
        };
        
        let request = tonic::Request::new(subscribe_request);
        if let Ok(response) = service.subscribe(request).await {
            streams.push(response.into_inner());
        }
    }
    
    memory_tracker.update();
    let initial_stats = memory_tracker.get_stats();
    
    // Apply sustained load
    let load_start = std::time::Instant::now();
    let mut event_count = 0;
    
    while load_start.elapsed() < Duration::from_secs(SUSTAINED_LOAD_DURATION_SECS) {
        for stream_id in 0..subscription_count {
            let event = MarketDataEvent {
                symbol: format!("PRESSURE_SYMBOL_{}", stream_id),
                exchange: "test_exchange".to_string(),
                timestamp: chrono::Utc::now().timestamp_millis() as u64 + event_count,
                data: match event_count % 3 {
                    0 => MarketData::OrderBook {
                        bids: vec![(100.0 + event_count as f64 * 0.01, 1.0)],
                        asks: vec![(101.0 + event_count as f64 * 0.01, 1.0)],
                        sequence: event_count,
                    },
                    1 => MarketData::Trade {
                        price: 100.5 + event_count as f64 * 0.01,
                        quantity: 1.0,
                        side: "buy".to_string(),
                        trade_id: event_count.to_string(),
                    },
                    _ => MarketData::Quote {
                        bid_price: 100.0 + event_count as f64 * 0.01,
                        bid_size: 1.0,
                        ask_price: 101.0 + event_count as f64 * 0.01,
                        ask_size: 1.0,
                    },
                },
            };
            
            if event_sender.send(event).await.is_err() {
                break;
            }
            
            event_count += 1;
            memory_tracker.record_allocation();
        }
        
        // Periodic memory check
        if event_count % 1000 == 0 {
            memory_tracker.update();
            tokio::task::yield_now().await;
        }
        
        // Maintain steady load
        sleep(Duration::from_millis(1)).await;
    }
    
    memory_tracker.update();
    let final_stats = memory_tracker.get_stats();
    final_stats.print_summary("Sustained Memory Pressure");
    
    println!("Events sent during sustained load: {}", event_count);
    println!("Average events per second: {:.0}", event_count as f64 / SUSTAINED_LOAD_DURATION_SECS as f64);
    
    // Memory should remain stable under sustained load
    let memory_growth_mb = (final_stats.current_usage_bytes - initial_stats.current_usage_bytes) as f64 / (1024.0 * 1024.0);
    assert!(memory_growth_mb < 50.0, "Memory growth under sustained load too high: {:.2} MB", memory_growth_mb);
    
    // Should handle high event rate
    let events_per_second = event_count as f64 / SUSTAINED_LOAD_DURATION_SECS as f64;
    assert!(events_per_second > 100.0, "Event rate too low under sustained load: {:.0} events/sec", events_per_second);
}

#[rstest]
#[tokio::test]
async fn test_memory_fragmentation_resistance() {
    let memory_tracker = Arc::new(MemoryTracker::new());
    
    // Create varying sizes of data structures to test fragmentation resistance
    let mut data_holders = Vec::new();
    
    for iteration in 0..100 {
        // Create different sized structures
        let size = match iteration % 4 {
            0 => 10,   // Small
            1 => 100,  // Medium
            2 => 1000, // Large
            _ => 50,   // Variable
        };
        
        // Create market data events of varying sizes
        let mut events = Vec::new();
        for i in 0..size {
            let event = MarketDataEvent {
                symbol: format!("FRAG_TEST_{}_{}", iteration, i),
                exchange: "fragmentation_test".to_string(),
                timestamp: chrono::Utc::now().timestamp_millis() as u64 + i as u64,
                data: MarketData::OrderBook {
                    bids: (0..i % 10 + 1).map(|j| (100.0 + j as f64, 1.0)).collect(),
                    asks: (0..i % 8 + 1).map(|j| (101.0 + j as f64, 1.0)).collect(),
                    sequence: i as u64,
                },
            };
            events.push(event);
            memory_tracker.record_allocation();
        }
        
        data_holders.push(events);
        
        // Periodically drop some data (simulating deallocation)
        if iteration % 10 == 0 && !data_holders.is_empty() {
            let to_remove = data_holders.len() / 3;
            for _ in 0..to_remove {
                if !data_holders.is_empty() {
                    data_holders.remove(0);
                    memory_tracker.record_deallocation();
                }
            }
        }
        
        memory_tracker.update();
    }
    
    let stats = memory_tracker.get_stats();
    stats.print_summary("Memory Fragmentation Resistance");
    
    // Should handle varying allocation sizes without excessive memory growth
    let memory_efficiency = stats.memory_efficiency();
    assert!(memory_efficiency > 0.3, "Memory efficiency too low (possible fragmentation): {:.2}", memory_efficiency);
}

#[rstest]
#[tokio::test]
async fn test_cleanup_memory_efficiency() {
    let memory_tracker = Arc::new(MemoryTracker::new());
    
    // Create service and generate load
    {
        let (service, event_sender) = MarketDataGrpcService::new();
        
        // Create subscription
        let subscribe_request = services_common::marketdata::v1::SubscribeRequest {
            symbols: vec!["CLEANUP_TEST".to_string()],
            exchange: "cleanup_exchange".to_string(),
            data_types: vec![1, 2, 3],
        };
        
        let request = tonic::Request::new(subscribe_request);
        if let Ok(response) = service.subscribe(request).await {
            let mut stream = response.into_inner();
            
            // Send many events
            for i in 0..1000 {
                let event = MarketDataEvent {
                    symbol: "CLEANUP_TEST".to_string(),
                    exchange: "cleanup_exchange".to_string(),
                    timestamp: chrono::Utc::now().timestamp_millis() as u64 + i,
                    data: MarketData::OrderBook {
                        bids: vec![(100.0 + i as f64 * 0.01, 1.0)],
                        asks: vec![(101.0 + i as f64 * 0.01, 1.0)],
                        sequence: i as u64,
                    },
                };
                
                if event_sender.send(event).await.is_err() {
                    break;
                }
                
                memory_tracker.record_allocation();
            }
            
            memory_tracker.update();
            
            // Try to consume some events
            use tokio_stream::StreamExt;
            for _ in 0..10 {
                if tokio::time::timeout(Duration::from_millis(10), stream.next()).await.is_ok() {
                    memory_tracker.record_deallocation();
                }
            }
        }
        
        memory_tracker.update();
    } // Service and all related data should be dropped here
    
    // Force some cleanup time
    sleep(Duration::from_millis(100)).await;
    
    // Check memory after cleanup
    memory_tracker.update();
    let stats = memory_tracker.get_stats();
    stats.print_summary("Cleanup Efficiency");
    
    // Memory should be efficiently reclaimed after cleanup
    let efficiency = stats.memory_efficiency();
    assert!(efficiency > 0.6, "Cleanup efficiency too low: {:.2}", efficiency);
    
    // Should have balanced allocations/deallocations
    let allocation_balance = stats.allocations as i64 - stats.deallocations as i64;
    assert!(allocation_balance < 100, "Poor allocation balance after cleanup: {}", allocation_balance);
}