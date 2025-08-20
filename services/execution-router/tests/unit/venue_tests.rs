//! Venue management tests
//!
//! Comprehensive tests for venue connection management:
//! - Connection state transitions and lifecycle management
//! - Statistics tracking and monitoring
//! - Primary venue failover logic
//! - Concurrent connection handling
//! - Error scenarios and recovery

use execution_router::venue_manager::{VenueManager, VenueStatus, VenueStats};
use rustc_hash::FxHashMap;
use rstest::*;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

/// Test fixtures and utilities
#[fixture]
fn test_venue_config() -> FxHashMap<String, String> {
    let mut config = FxHashMap::default();
    config.insert("api_key".to_string(), "test_key_123".to_string());
    config.insert("base_url".to_string(), "wss://test.venue.com".to_string());
    config.insert("timeout_ms".to_string(), "5000".to_string());
    config.insert("max_connections".to_string(), "10".to_string());
    config
}

#[fixture]
fn test_venue_manager() -> VenueManager {
    VenueManager::new("primary_venue".to_string())
}

/// Venue Manager Creation Tests
#[rstest]
fn test_venue_manager_creation(test_venue_manager: VenueManager) {
    assert_eq!(test_venue_manager.get_primary_venue(), "primary_venue");
}

#[rstest]
#[tokio::test]
async fn test_venue_manager_primary_venue_operations(test_venue_manager: VenueManager) {
    // Initially no primary venue available
    assert!(!test_venue_manager.is_primary_venue_available().await);
    
    // Add primary venue
    let config = FxHashMap::default();
    test_venue_manager.add_venue("primary_venue".to_string(), config).await;
    
    // Still not available until connected
    assert!(!test_venue_manager.is_primary_venue_available().await);
    
    // Connect primary venue
    test_venue_manager.connect_venue("primary_venue").await.expect("Failed to connect primary venue");
    
    // Now should be available
    assert!(test_venue_manager.is_primary_venue_available().await);
    
    // Get best available should return primary
    let best = test_venue_manager.get_best_available_venue().await;
    assert_eq!(best, Some("primary_venue".to_string()));
}

/// Venue Lifecycle Tests
#[rstest]
#[tokio::test]
async fn test_venue_lifecycle(test_venue_config: FxHashMap<String, String>) {
    let manager = VenueManager::new("binance".to_string());
    
    // Add venue
    manager.add_venue("binance".to_string(), test_venue_config).await;
    
    // Check initial status
    let status = manager.get_venue_status("binance").await;
    assert_eq!(status, Some(VenueStatus::Disconnected));
    
    // Connect venue
    manager.connect_venue("binance").await.expect("Failed to connect venue");
    let status = manager.get_venue_status("binance").await;
    assert_eq!(status, Some(VenueStatus::Connected));
    
    // Disconnect venue
    manager.disconnect_venue("binance").await.expect("Failed to disconnect venue");
    let status = manager.get_venue_status("binance").await;
    assert_eq!(status, Some(VenueStatus::Disconnected));
}

#[rstest]
#[tokio::test]
async fn test_multiple_venues_management() {
    let manager = VenueManager::new("binance".to_string());
    let config = FxHashMap::default();
    
    // Add multiple venues
    let venues = ["binance", "coinbase", "kraken", "bybit"];
    for venue in &venues {
        manager.add_venue(venue.to_string(), config.clone()).await;
    }
    
    // Connect some venues
    manager.connect_venue("binance").await.expect("Failed to connect binance");
    manager.connect_venue("coinbase").await.expect("Failed to connect coinbase");
    
    // Check all statuses
    let statuses = manager.get_all_statuses().await;
    assert_eq!(statuses.len(), venues.len());
    assert_eq!(statuses["binance"], VenueStatus::Connected);
    assert_eq!(statuses["coinbase"], VenueStatus::Connected);
    assert_eq!(statuses["kraken"], VenueStatus::Disconnected);
    assert_eq!(statuses["bybit"], VenueStatus::Disconnected);
    
    // Primary venue should be available
    assert!(manager.is_primary_venue_available().await);
    
    // Disconnect primary venue
    manager.disconnect_venue("binance").await.expect("Failed to disconnect binance");
    
    // Should fallback to next available venue
    let best = manager.get_best_available_venue().await;
    assert_eq!(best, Some("coinbase".to_string()));
}

/// Venue Statistics Tests
#[rstest]
#[tokio::test]
async fn test_venue_statistics_tracking() -> anyhow::Result<()> {
    let manager = VenueManager::new("test_venue".to_string());
    manager.add_venue("test_venue".to_string(), FxHashMap::default()).await;
    
    // Update statistics multiple times
    manager.update_stats("test_venue", |stats| {
        stats.messages_sent += 10;
        stats.messages_received += 8;
        stats.orders_sent += 5;
        stats.orders_filled += 3;
        stats.avg_latency_us = 1500;
        stats.uptime_seconds = 3600;
    }).await?;
    
    manager.update_stats("test_venue", |stats| {
        stats.messages_sent += 5;
        stats.messages_received += 7;
        stats.orders_sent += 2;
        stats.orders_filled += 2;
        stats.avg_latency_us = 1200; // Better latency
        stats.uptime_seconds = 3700;
    }).await?;
    
    // Verify accumulated statistics
    // Note: We can't directly read stats from the current API, 
    // but we verify the operations don't fail
    
    Ok(())
}

#[rstest]
#[tokio::test]
async fn test_venue_error_scenarios() {
    let manager = VenueManager::new("primary".to_string());
    
    // Try to connect non-existent venue
    let result = manager.connect_venue("non_existent").await;
    assert!(result.is_err(), "Should fail to connect non-existent venue");
    
    // Try to disconnect non-existent venue
    let result = manager.disconnect_venue("non_existent").await;
    assert!(result.is_err(), "Should fail to disconnect non-existent venue");
    
    // Try to get status of non-existent venue
    let status = manager.get_venue_status("non_existent").await;
    assert_eq!(status, None);
    
    // Try to update stats of non-existent venue
    let result = manager.update_stats("non_existent", |_| {}).await;
    assert!(result.is_err(), "Should fail to update stats of non-existent venue");
}

/// Primary Venue Failover Tests
#[rstest]
#[tokio::test]
async fn test_primary_venue_failover_logic() {
    let mut manager = VenueManager::new("primary".to_string());
    let config = FxHashMap::default();
    
    // Add venues
    manager.add_venue("primary".to_string(), config.clone()).await;
    manager.add_venue("backup1".to_string(), config.clone()).await;
    manager.add_venue("backup2".to_string(), config).await;
    
    // Connect all venues
    manager.connect_venue("primary").await.expect("Failed to connect primary");
    manager.connect_venue("backup1").await.expect("Failed to connect backup1");
    manager.connect_venue("backup2").await.expect("Failed to connect backup2");
    
    // Should prefer primary
    assert_eq!(manager.get_best_available_venue().await, Some("primary".to_string()));
    
    // Disconnect primary
    manager.disconnect_venue("primary").await.expect("Failed to disconnect primary");
    
    // Should fallback to any available venue
    let best = manager.get_best_available_venue().await;
    assert!(best.is_some());
    assert!(best.unwrap() != "primary");
    
    // Change primary venue
    manager.set_primary_venue("backup1".to_string());
    assert_eq!(manager.get_primary_venue(), "backup1");
    
    // Should now prefer backup1 as primary
    assert_eq!(manager.get_best_available_venue().await, Some("backup1".to_string()));
}

#[rstest]
#[tokio::test]
async fn test_venue_failover_no_available_venues() {
    let manager = VenueManager::new("primary".to_string());
    let config = FxHashMap::default();
    
    // Add venues but don't connect them
    manager.add_venue("primary".to_string(), config.clone()).await;
    manager.add_venue("backup".to_string(), config).await;
    
    // No venues should be available
    assert!(!manager.is_primary_venue_available().await);
    assert_eq!(manager.get_best_available_venue().await, None);
}

/// Concurrent Access Tests
#[rstest]
fn test_venue_manager_concurrent_access() {
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        let manager = Arc::new(VenueManager::new("primary".to_string()));
        let num_venues = 10;
        let num_threads = 4;
        
        // Add venues concurrently
        let add_handles: Vec<_> = (0..num_threads).map(|thread_id| {
            let manager = Arc::clone(&manager);
            
            tokio::spawn(async move {
                for i in 0..num_venues / num_threads {
                    let venue_name = format!("venue_{}_{}", thread_id, i);
                    let config = FxHashMap::default();
                    manager.add_venue(venue_name, config).await;
                }
            })
        }).collect();
        
        // Wait for all venues to be added
        for handle in add_handles {
            handle.await.unwrap();
        }
        
        // Connect venues concurrently
        let connect_handles: Vec<_> = (0..num_threads).map(|thread_id| {
            let manager = Arc::clone(&manager);
            
            tokio::spawn(async move {
                for i in 0..num_venues / num_threads {
                    let venue_name = format!("venue_{}_{}", thread_id, i);
                    let _ = manager.connect_venue(&venue_name).await;
                }
            })
        }).collect();
        
        for handle in connect_handles {
            handle.await.unwrap();
        }
        
        // Verify all venues are present
        let all_statuses = manager.get_all_statuses().await;
        assert_eq!(all_statuses.len(), num_venues);
        
        // Count connected venues
        let connected_count = all_statuses.values()
            .filter(|&&status| status == VenueStatus::Connected)
            .count();
        
        assert_eq!(connected_count, num_venues, "All venues should be connected");
    });
}

#[rstest]
fn test_venue_statistics_concurrent_updates() {
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        let manager = Arc::new(VenueManager::new("test_venue".to_string()));
        manager.add_venue("test_venue".to_string(), FxHashMap::default()).await;
        manager.connect_venue("test_venue").await.expect("Failed to connect test venue");
        
        let num_threads = 8;
        let updates_per_thread = 100;
        
        let update_handles: Vec<_> = (0..num_threads).map(|thread_id| {
            let manager = Arc::clone(&manager);
            
            tokio::spawn(async move {
                for i in 0..updates_per_thread {
                    let result = manager.update_stats("test_venue", |stats| {
                        stats.messages_sent += 1;
                        stats.messages_received += 1;
                        stats.avg_latency_us = (thread_id * 100 + i) as u64;
                        stats.uptime_seconds += 1;
                    }).await;
                    
                    if result.is_err() {
                        println!("Update failed: {:?}", result);
                    }
                }
            })
        }).collect();
        
        // Wait for all updates to complete
        for handle in update_handles {
            handle.await.unwrap();
        }
        
        // All updates should complete without panicking
    });
}

/// Venue Configuration Tests
#[rstest]
#[tokio::test]
async fn test_venue_configuration_handling() {
    let manager = VenueManager::new("binance".to_string());
    
    // Test various configuration scenarios
    let configs = vec![
        {
            let mut config = FxHashMap::default();
            config.insert("api_key".to_string(), "key123".to_string());
            config.insert("secret".to_string(), "secret456".to_string());
            config.insert("sandbox".to_string(), "true".to_string());
            ("binance_sandbox", config)
        },
        {
            let mut config = FxHashMap::default();
            config.insert("api_key".to_string(), "prod_key".to_string());
            config.insert("secret".to_string(), "prod_secret".to_string());
            config.insert("sandbox".to_string(), "false".to_string());
            config.insert("rate_limit".to_string(), "1000".to_string());
            ("binance_prod", config)
        },
        {
            // Empty configuration
            ("coinbase", FxHashMap::default())
        }
    ];
    
    for (venue_name, config) in configs {
        manager.add_venue(venue_name.to_string(), config).await;
        
        // Should be able to connect regardless of configuration
        manager.connect_venue(venue_name).await.expect("Should connect with any config");
        
        let status = manager.get_venue_status(venue_name).await;
        assert_eq!(status, Some(VenueStatus::Connected));
    }
}

/// Performance and Scaling Tests
#[rstest]
fn test_venue_manager_performance() {
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        let manager = VenueManager::new("primary".to_string());
        let num_venues = 100;
        
        // Measure venue addition performance
        let start = Instant::now();
        for i in 0..num_venues {
            let venue_name = format!("venue_{}", i);
            let config = FxHashMap::default();
            manager.add_venue(venue_name, config).await;
        }
        let add_time = start.elapsed();
        
        // Measure connection performance
        let start = Instant::now();
        for i in 0..num_venues {
            let venue_name = format!("venue_{}", i);
            manager.connect_venue(&venue_name).await.expect("Should connect");
        }
        let connect_time = start.elapsed();
        
        // Measure status query performance
        let start = Instant::now();
        let all_statuses = manager.get_all_statuses().await;
        let query_time = start.elapsed();
        
        assert_eq!(all_statuses.len(), num_venues);
        
        println!("Venue manager performance:");
        println!("  Add {} venues: {:?} ({:.2} µs/venue)", 
                 num_venues, add_time, add_time.as_micros() as f64 / num_venues as f64);
        println!("  Connect {} venues: {:?} ({:.2} µs/venue)", 
                 num_venues, connect_time, connect_time.as_micros() as f64 / num_venues as f64);
        println!("  Query all statuses: {:?}", query_time);
        
        // Performance assertions
        assert!(add_time.as_millis() < 100, "Adding venues should be fast");
        assert!(connect_time.as_millis() < 1000, "Connecting venues should be reasonable");
        assert!(query_time.as_micros() < 1000, "Status queries should be very fast");
    });
}

#[rstest]
#[tokio::test]
async fn test_venue_manager_memory_efficiency() {
    let manager = VenueManager::new("primary".to_string());
    
    // Add and remove venues repeatedly to test for memory leaks
    for round in 0..10 {
        // Add many venues
        for i in 0..100 {
            let venue_name = format!("venue_{}_{}", round, i);
            let config = FxHashMap::default();
            manager.add_venue(venue_name.clone(), config).await;
            manager.connect_venue(&venue_name).await.expect("Should connect");
        }
        
        let statuses = manager.get_all_statuses().await;
        assert_eq!(statuses.len(), 100, "Should have 100 venues in round {}", round);
        
        // Disconnect all venues (but keep them in manager)
        for i in 0..100 {
            let venue_name = format!("venue_{}_{}", round, i);
            manager.disconnect_venue(&venue_name).await.expect("Should disconnect");
        }
        
        // Note: Current API doesn't support venue removal, so we can't fully test cleanup
        // In a real implementation, we'd want a remove_venue method
    }
}

/// Edge Cases and Error Recovery
#[rstest]
#[tokio::test]
async fn test_venue_edge_cases() {
    let manager = VenueManager::new("".to_string()); // Empty primary venue name
    
    // Should handle empty primary venue name gracefully
    assert!(!manager.is_primary_venue_available().await);
    assert_eq!(manager.get_best_available_venue().await, None);
    
    // Add venue with empty name
    manager.add_venue("".to_string(), FxHashMap::default()).await;
    manager.connect_venue("").await.expect("Should handle empty venue name");
    
    let status = manager.get_venue_status("").await;
    assert_eq!(status, Some(VenueStatus::Connected));
}

#[rstest]
#[tokio::test]
async fn test_venue_unicode_names() {
    let manager = VenueManager::new("主要交易所".to_string());
    
    let unicode_venues = [
        "币安", "火币", "OKEx", "Торговля", "証券取引所"
    ];
    
    for venue in &unicode_venues {
        let config = FxHashMap::default();
        manager.add_venue(venue.to_string(), config).await;
        manager.connect_venue(venue).await.expect("Should handle unicode venue names");
        
        let status = manager.get_venue_status(venue).await;
        assert_eq!(status, Some(VenueStatus::Connected));
    }
    
    let all_statuses = manager.get_all_statuses().await;
    assert_eq!(all_statuses.len(), unicode_venues.len());
}

/// Integration-style Tests
#[rstest]
#[tokio::test]
async fn test_venue_realistic_workflow() {
    let manager = VenueManager::new("binance".to_string());
    
    // Simulate realistic venue setup
    let venues_config = vec![
        ("binance", {
            let mut config = FxHashMap::default();
            config.insert("api_key".to_string(), "binance_key".to_string());
            config.insert("weight_limit".to_string(), "1200".to_string());
            config
        }),
        ("coinbase", {
            let mut config = FxHashMap::default();
            config.insert("api_key".to_string(), "coinbase_key".to_string());
            config.insert("passphrase".to_string(), "cb_passphrase".to_string());
            config
        }),
        ("kraken", {
            let mut config = FxHashMap::default();
            config.insert("api_key".to_string(), "kraken_key".to_string());
            config.insert("tier".to_string(), "intermediate".to_string());
            config
        })
    ];
    
    // Add all venues
    for (venue_name, config) in &venues_config {
        manager.add_venue(venue_name.to_string(), config.clone()).await;
    }
    
    // Connect in order of preference
    manager.connect_venue("binance").await.expect("Primary venue should connect");
    
    // Simulate connection issues with secondary venues
    // (In real implementation, connect_venue might fail)
    let _ = manager.connect_venue("coinbase").await;
    let _ = manager.connect_venue("kraken").await;
    
    // Verify primary is preferred
    assert!(manager.is_primary_venue_available().await);
    assert_eq!(manager.get_best_available_venue().await, Some("binance".to_string()));
    
    // Simulate primary venue issues
    manager.disconnect_venue("binance").await.expect("Should disconnect primary");
    
    // System should failover gracefully
    let backup = manager.get_best_available_venue().await;
    assert!(backup.is_some());
    assert_ne!(backup.unwrap(), "binance");
    
    // Update statistics during operation
    for venue_name in &["coinbase", "kraken"] {
        let _ = manager.update_stats(venue_name, |stats| {
            stats.messages_sent += 100;
            stats.orders_sent += 10;
            stats.avg_latency_us = 2000;
            stats.uptime_seconds = 1800; // 30 minutes
        }).await;
    }
    
    // Reconnect primary venue
    manager.connect_venue("binance").await.expect("Should reconnect primary");
    
    // Should prefer primary again
    assert_eq!(manager.get_best_available_venue().await, Some("binance".to_string()));
}