//! Sprint 3 Unit Tests
//! 
//! Unit tests for Sprint 3 modules (no external API calls)

use auth::{AuthProvider, ZerodhaAuth, BinanceSigner};
use bus::{Bus, Publisher, Subscriber};
use common::{L2Update, Px, Qty, Side, Symbol, Ts};
use feeds::{FeedManager, FeedManagerConfig, ZerodhaConfig, BinanceConfig, MarketEvent};
use lob::OrderBook;
use std::sync::Arc;
use std::collections::HashMap;

#[tokio::test]
async fn test_zerodha_auth() {
    // Test Zerodha authentication
    let auth = ZerodhaAuth::new(
        "test_api_key".to_string(),
        "test_api_secret".to_string(),
        "/tmp/test_zerodha_token.json".into(),
    );
    
    // Zerodha auth requires manual token entry via web flow
    // In real usage, user would save token via CLI after web auth
    // For testing, we just verify the auth object was created
    assert_eq!(auth.api_key, "test_api_key");
    
    // Clean up
    std::fs::remove_file("/tmp/test_zerodha_token.json").ok();
}

#[tokio::test]
async fn test_binance_signer() {
    // Test Binance HMAC signing
    let signer = BinanceSigner::new(
        "test_api_key".to_string(),
        "test_api_secret".to_string(),
    );
    
    // Test query signing
    let query = "symbol=BTCUSDT&timestamp=1234567890";
    let signature = signer.sign_query(query);
    
    // Signature should be deterministic for same input
    let signature2 = signer.sign_query(query);
    assert_eq!(signature, signature2);
    
    // Signature should be hex string
    assert!(signature.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_lob_performance() {
    // Test LOB meets performance requirements
    let symbol = Symbol::new(1);
    let mut book = OrderBook::new(symbol);
    
    // Add some initial depth
    for i in 0..10 {
        let bid = L2Update::new(
            Ts::from_nanos(i),
            symbol,
            Side::Bid,
            Px::new(99.5 - i as f64 * 0.1),
            Qty::new(100.0 * (i + 1) as f64),
            i as u8,
        );
        book.apply(&bid).unwrap();
        
        let ask = L2Update::new(
            Ts::from_nanos(i + 100),
            symbol,
            Side::Ask,
            Px::new(100.5 + i as f64 * 0.1),
            Qty::new(100.0 * (i + 1) as f64),
            i as u8,
        );
        book.apply(&ask).unwrap();
    }
    
    // Measure update time
    use std::time::Instant;
    let update = L2Update::new(
        Ts::from_nanos(1000),
        symbol,
        Side::Bid,
        Px::new(99.45),
        Qty::new(250.0),
        0,
    );
    
    let start = Instant::now();
    for _ in 0..10000 {
        let _ = book.apply(&update);
    }
    let elapsed = start.elapsed();
    
    let avg_ns = elapsed.as_nanos() / 10000;
    println!("Average LOB update time: {}ns", avg_ns);
    
    // Should be well under 200ns
    assert!(avg_ns < 200, "LOB update exceeded 200ns target: {}ns", avg_ns);
}

#[test]
fn test_crossed_book_prevention() {
    let symbol = Symbol::new(1);
    let mut book = OrderBook::new(symbol);
    
    // Add ask at 100
    book.apply(&L2Update::new(
        Ts::from_nanos(1),
        symbol,
        Side::Ask,
        Px::new(100.0),
        Qty::new(100.0),
        0,
    )).unwrap();
    
    // Try to add bid at 101 (would cross)
    let result = book.apply(&L2Update::new(
        Ts::from_nanos(2),
        symbol,
        Side::Bid,
        Px::new(101.0),
        Qty::new(100.0),
        0,
    ));
    
    assert!(result.is_err());
}

#[tokio::test]
async fn test_event_bus_integration() {
    // Create event bus
    let bus = Arc::new(Bus::<MarketEvent>::new(1000));
    
    // Create subscriber
    let subscriber = bus.subscriber();
    let receiver = subscriber.subscribe().unwrap();
    
    // Create publisher
    let publisher = bus.publisher();
    
    // Publish some events
    let update = L2Update::new(
        Ts::now(),
        Symbol::new(1),
        Side::Bid,
        Px::new(99.5),
        Qty::new(100.0),
        0,
    );
    
    publisher.publish(MarketEvent::L2Update(update.clone())).unwrap();
    
    // Receive and verify
    let received = receiver.try_recv().unwrap();
    match received {
        Some(MarketEvent::L2Update(recv_update)) => {
            assert_eq!(recv_update.symbol, update.symbol);
            assert_eq!(recv_update.side, update.side);
        }
        _ => panic!("Expected L2Update event"),
    }
}

#[tokio::test]
async fn test_feed_manager_config() {
    // Test feed manager configuration
    let mut symbol_map = HashMap::new();
    symbol_map.insert(Symbol::new(256100), "NIFTY".to_string()); // NIFTY index
    symbol_map.insert(Symbol::new(738561), "RELIANCE".to_string()); // Reliance
    
    let config = FeedManagerConfig {
        zerodha: Some(ZerodhaConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            token_file: "/tmp/zerodha_token.json".to_string(),
            ws_url: "wss://ws.kite.trade".to_string(),
            api_url: "https://api.kite.trade".to_string(),
            symbols: symbol_map.clone(),
        }),
        binance: Some(BinanceConfig {
            api_key: "test_key".to_string(),
            api_secret: "test_secret".to_string(),
            ws_url: "wss://stream.binance.com:9443/ws".to_string(),
            api_url: "https://api.binance.com".to_string(),
            symbols: {
                let mut map = HashMap::new();
                map.insert(Symbol::new(1001), "BTCUSDT".to_string());
                map.insert(Symbol::new(1002), "ETHUSDT".to_string());
                map
            },
        }),
        buffer_size: 10000,
    };
    
    // Create bus and manager
    let bus = Arc::new(Bus::<MarketEvent>::new(10000));
    let manager = FeedManager::new(config, bus);
    
    // Initialize books
    let symbols = vec![
        Symbol::new(256100),
        Symbol::new(738561),
        Symbol::new(1001),
        Symbol::new(1002),
    ];
    manager.init_books(symbols).await;
    
    // Verify books were created
    let active_symbols = manager.get_symbols().await;
    assert_eq!(active_symbols.len(), 4);
}

#[test]
fn test_feature_extraction() {
    use lob::FeatureCalculator;
    
    let symbol = Symbol::new(1);
    let mut book = OrderBook::new(symbol);
    let mut calc = FeatureCalculator::new(60_000_000_000, 1000); // 60s window
    
    // Setup book with depth
    for i in 0..5 {
        book.apply(&L2Update::new(
            Ts::from_nanos(i * 1000),
            symbol,
            Side::Bid,
            Px::new(99.5 - i as f64 * 0.1),
            Qty::new(100.0 + i as f64 * 50.0),
            i as u8,
        )).unwrap();
        
        book.apply(&L2Update::new(
            Ts::from_nanos(i * 1000 + 500),
            symbol,
            Side::Ask,
            Px::new(100.5 + i as f64 * 0.1),
            Qty::new(100.0 + i as f64 * 50.0),
            i as u8,
        )).unwrap();
    }
    
    // Calculate features
    let features = calc.calculate(&book).unwrap();
    
    // Verify features
    assert_eq!(features.symbol, symbol);
    assert!(features.spread_ticks > 0);
    assert!(features.mid > 0);
    assert!(features.micro > 0);
    assert!(features.imbalance.abs() <= 1.0);
}

#[test]
fn test_sprint3_performance_targets() {
    // Verify all Sprint 3 performance targets
    
    // Target: LOB updates ≥ 200k/sec
    let symbol = Symbol::new(1);
    let mut book = OrderBook::new(symbol);
    
    use std::time::Instant;
    let start = Instant::now();
    let updates = 100_000;
    
    for i in 0..updates {
        let update = L2Update::new(
            Ts::from_nanos(i),
            symbol,
            if i % 2 == 0 { Side::Bid } else { Side::Ask },
            Px::new(100.0 + (i % 10) as f64 * 0.1),
            Qty::new(100.0),
            (i % 10) as u8,
        );
        let _ = book.apply(&update);
    }
    
    let elapsed = start.elapsed();
    let rate = updates as f64 / elapsed.as_secs_f64();
    
    println!("LOB update rate: {:.0} updates/sec", rate);
    assert!(rate >= 200_000.0, "LOB update rate below 200k/sec: {:.0}", rate);
    
    // Target: apply() p50 ≤ 200ns
    let mut times = Vec::new();
    for i in 0..1000 {
        let update = L2Update::new(
            Ts::from_nanos(i),
            symbol,
            Side::Bid,
            Px::new(99.5),
            Qty::new(100.0),
            0,
        );
        
        let start = Instant::now();
        let _ = book.apply(&update);
        let elapsed = start.elapsed();
        times.push(elapsed.as_nanos());
    }
    
    times.sort_unstable();
    let p50 = times[times.len() / 2];
    let p99 = times[times.len() * 99 / 100];
    
    println!("LOB apply() p50: {}ns, p99: {}ns", p50, p99);
    assert!(p50 <= 200, "LOB apply() p50 exceeded 200ns: {}ns", p50);
    assert!(p99 <= 900, "LOB apply() p99 exceeded 900ns: {}ns", p99);
}

#[test]
fn test_deterministic_arithmetic() {
    // Verify fixed-point arithmetic is deterministic
    let px1 = Px::new(100.1234);
    let px2 = Px::new(100.1234);
    
    assert_eq!(px1, px2);
    assert_eq!(px1.as_i64(), px2.as_i64());
    
    let qty1 = Qty::new(500.5678);
    let qty2 = Qty::new(500.5678);
    
    assert_eq!(qty1, qty2);
    assert_eq!(qty1.as_i64(), qty2.as_i64());
    
    // Test ordering
    let px3 = Px::new(100.1235);
    assert!(px3 > px1);
    
    // Test hashing (should be consistent)
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher1 = DefaultHasher::new();
    px1.hash(&mut hasher1);
    let hash1 = hasher1.finish();
    
    let mut hasher2 = DefaultHasher::new();
    px2.hash(&mut hasher2);
    let hash2 = hasher2.finish();
    
    assert_eq!(hash1, hash2);
}

fn main() {
    println!("Sprint 3 Integration Tests");
    println!("Run with: cargo test --test sprint3_integration");
}