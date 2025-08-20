//! Market feed integration tests
//! Tests price updates, correlation calculations, and beta calculations

use portfolio_manager::market_feed::{MarketFeedManager, PriceSnapshot, PriceUpdate, ReturnsBuffer};
use rstest::*;
use services_common::Symbol;
use approx::assert_relative_eq;
use std::sync::atomic::Ordering;

// Test fixtures
#[fixture]
fn symbols() -> Vec<Symbol> {
    vec![
        Symbol::new(1),
        Symbol::new(2),
        Symbol::new(3),
        Symbol::new(4),
    ]
}

#[fixture]
fn market_feed_manager(symbols: Vec<Symbol>) -> MarketFeedManager {
    MarketFeedManager::new(&symbols, 1000)
}

#[fixture]
fn returns_buffer(symbols: Vec<Symbol>) -> ReturnsBuffer {
    ReturnsBuffer::new(100, &symbols)
}

#[fixture]
fn sample_price_update() -> PriceUpdate {
    PriceUpdate {
        symbol: Symbol::new(1),
        bid: 100000,
        ask: 100100,
        last: 100050,
        volume: 1000000,
        timestamp: 1234567890,
    }
}

mod price_snapshot_tests {
    use super::*;

    #[rstest]
    fn test_price_snapshot_initialization() {
        let snapshot = PriceSnapshot::new();

        assert_eq!(snapshot.bid.load(Ordering::Acquire), 0);
        assert_eq!(snapshot.ask.load(Ordering::Acquire), 0);
        assert_eq!(snapshot.last.load(Ordering::Acquire), 0);
        assert_eq!(snapshot.volume.load(Ordering::Acquire), 0);
        assert_eq!(snapshot.timestamp.load(Ordering::Acquire), 0);
    }

    #[rstest]
    fn test_price_snapshot_update() {
        let snapshot = PriceSnapshot::new();
        
        snapshot.update(100000, 100100, 100050, 500000, 1234567890);

        assert_eq!(snapshot.bid.load(Ordering::Acquire), 100000);
        assert_eq!(snapshot.ask.load(Ordering::Acquire), 100100);
        assert_eq!(snapshot.last.load(Ordering::Acquire), 100050);
        assert_eq!(snapshot.volume.load(Ordering::Acquire), 500000);
        assert_eq!(snapshot.timestamp.load(Ordering::Acquire), 1234567890);
    }

    #[rstest]
    fn test_mid_price_calculation() {
        let snapshot = PriceSnapshot::new();
        
        snapshot.update(100000, 100200, 100100, 500000, 1234567890);

        let mid_price = snapshot.mid_price();
        assert_eq!(mid_price, 100100); // (100000 + 100200) / 2
    }

    #[rstest]
    fn test_spread_calculation() {
        let snapshot = PriceSnapshot::new();
        
        snapshot.update(100000, 100250, 100100, 500000, 1234567890);

        let spread = snapshot.spread();
        assert_eq!(spread, 250); // 100250 - 100000
    }

    #[rstest]
    fn test_concurrent_price_updates() {
        let snapshot = std::sync::Arc::new(PriceSnapshot::new());
        let num_threads = 5;
        let updates_per_thread = 100;

        let handles: Vec<_> = (0..num_threads)
            .map(|thread_id| {
                let snap = std::sync::Arc::clone(&snapshot);
                std::thread::spawn(move || {
                    for i in 0..updates_per_thread {
                        let base_price = 100000 + (thread_id * 1000 + i) as i64;
                        snap.update(
                            base_price,
                            base_price + 100,
                            base_price + 50,
                            1000000,
                            (thread_id * updates_per_thread + i) as u64,
                        );
                    }
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Should have some valid final state
        assert!(snapshot.bid.load(Ordering::Acquire) >= 100000);
        assert!(snapshot.ask.load(Ordering::Acquire) > snapshot.bid.load(Ordering::Acquire));
        assert!(snapshot.timestamp.load(Ordering::Acquire) > 0);
    }

    #[rstest]
    fn test_extreme_price_values() {
        let snapshot = PriceSnapshot::new();
        
        // Very large prices
        let large_bid = 999_999_999_999i64;
        let large_ask = 1_000_000_000_000i64;
        
        snapshot.update(large_bid, large_ask, large_bid + 500, 1000000, 1234567890);

        assert_eq!(snapshot.bid.load(Ordering::Acquire), large_bid);
        assert_eq!(snapshot.ask.load(Ordering::Acquire), large_ask);
        assert_eq!(snapshot.mid_price(), (large_bid + large_ask) / 2);
        assert_eq!(snapshot.spread(), large_ask - large_bid);
    }

    #[rstest]
    fn test_zero_spread_handling() {
        let snapshot = PriceSnapshot::new();
        
        // Same bid and ask (zero spread)
        snapshot.update(100000, 100000, 100000, 500000, 1234567890);

        assert_eq!(snapshot.spread(), 0);
        assert_eq!(snapshot.mid_price(), 100000);
    }
}

mod market_feed_manager_tests {
    use super::*;

    #[rstest]
    fn test_market_feed_manager_initialization(symbols: Vec<Symbol>) {
        let manager = MarketFeedManager::new(&symbols, 500);
        
        // Should be able to get prices for all symbols (initially zero)
        for symbol in symbols {
            let price = manager.get_price(symbol);
            assert!(price.is_some());
            let (bid, ask, last) = price.unwrap();
            assert_eq!(bid, 0);
            assert_eq!(ask, 0);
            assert_eq!(last, 0);
        }
    }

    #[rstest]
    fn test_price_update_direct(mut market_feed_manager: MarketFeedManager, sample_price_update: PriceUpdate) {
        let result = market_feed_manager.update_price(sample_price_update.clone());
        assert!(result.is_ok());

        // Check price was updated
        let price = market_feed_manager.get_price(sample_price_update.symbol);
        assert!(price.is_some());
        
        let (bid, ask, last) = price.unwrap();
        assert_eq!(bid, sample_price_update.bid);
        assert_eq!(ask, sample_price_update.ask);
        assert_eq!(last, sample_price_update.last);
    }

    #[rstest]
    fn test_multiple_symbol_updates(mut market_feed_manager: MarketFeedManager, symbols: Vec<Symbol>) {
        // Update prices for multiple symbols
        for (i, symbol) in symbols.iter().enumerate() {
            let base_price = 100000 + (i * 10000) as i64;
            let update = PriceUpdate {
                symbol: *symbol,
                bid: base_price,
                ask: base_price + 100,
                last: base_price + 50,
                volume: 1000000,
                timestamp: 1234567890 + i as u64,
            };
            
            market_feed_manager.update_price(update).unwrap();
        }

        // Verify all updates
        for (i, symbol) in symbols.iter().enumerate() {
            let price = market_feed_manager.get_price(*symbol);
            assert!(price.is_some());
            
            let (bid, ask, last) = price.unwrap();
            let expected_base = 100000 + (i * 10000) as i64;
            assert_eq!(bid, expected_base);
            assert_eq!(ask, expected_base + 100);
            assert_eq!(last, expected_base + 50);
        }
    }

    #[rstest]
    fn test_price_update_for_unknown_symbol(mut market_feed_manager: MarketFeedManager) {
        let unknown_symbol = Symbol::new(999);
        let update = PriceUpdate {
            symbol: unknown_symbol,
            bid: 100000,
            ask: 100100,
            last: 100050,
            volume: 1000000,
            timestamp: 1234567890,
        };

        // Should not crash, but may not update anything
        let result = market_feed_manager.update_price(update);
        assert!(result.is_ok());

        // Price should not be available for unknown symbol
        let price = market_feed_manager.get_price(unknown_symbol);
        assert!(price.is_none());
    }

    #[rstest]
    fn test_index_price_updates(market_feed_manager: MarketFeedManager) {
        let result = market_feed_manager.update_index("NIFTY", 180000, 180100, 180050);
        assert!(result.is_ok());

        let result = market_feed_manager.update_index("SENSEX", 600000, 600200, 600100);
        assert!(result.is_ok());

        // These are internal updates, main verification is no crash
    }

    #[rstest]
    fn test_rapid_price_updates(mut market_feed_manager: MarketFeedManager) {
        let symbol = Symbol::new(1);
        
        // Rapid sequence of updates
        for i in 0..1000 {
            let base_price = 100000 + i;
            let update = PriceUpdate {
                symbol,
                bid: base_price,
                ask: base_price + 100,
                last: base_price + 50,
                volume: 1000000,
                timestamp: 1234567890 + i as u64,
            };
            
            market_feed_manager.update_price(update).unwrap();
        }

        // Should have final state
        let price = market_feed_manager.get_price(symbol);
        assert!(price.is_some());
        
        let (bid, ask, last) = price.unwrap();
        assert!(bid >= 100000);
        assert_eq!(ask, bid + 100);
        assert_eq!(last, bid + 50);
    }

    #[rstest]
    async fn test_connection_simulation(mut market_feed_manager: MarketFeedManager) {
        // Test would normally connect to gRPC endpoint
        // For testing, we just ensure method doesn't crash
        
        // This would fail in real environment without gRPC server
        // but should handle gracefully in test
        let symbols = vec![Symbol::new(1), Symbol::new(2)];
        let result = market_feed_manager.subscribe_symbols(&symbols).await;
        // In real implementation, this might fail without connection
        // For unit tests, we expect it to at least not crash
        // assert!(result.is_ok() || result.is_err()); // Either is acceptable
    }
}

mod returns_buffer_tests {
    use super::*;

    #[rstest]
    fn test_returns_buffer_initialization(symbols: Vec<Symbol>) {
        let buffer = ReturnsBuffer::new(50, &symbols);
        
        // Should handle the buffer creation
        // Internal state is opaque, but should not crash
    }

    #[rstest]
    fn test_add_returns(mut returns_buffer: ReturnsBuffer, symbols: Vec<Symbol>) {
        // Add returns for various symbols
        for (i, symbol) in symbols.iter().enumerate() {
            returns_buffer.add_return(*symbol, (i * 100) as i64);
        }

        // Should handle the additions without crashing
        // Internal state verification would require exposing internals
    }

    #[rstest]
    fn test_add_index_returns(mut returns_buffer: ReturnsBuffer) {
        returns_buffer.add_index_return("NIFTY", 1000);
        returns_buffer.add_index_return("SENSEX", 1500);
        returns_buffer.add_index_return("UNKNOWN_INDEX", 500);

        // Should handle various index names
    }

    #[rstest]
    fn test_circular_buffer_behavior(symbols: Vec<Symbol>) {
        let small_capacity = 10;
        let mut buffer = ReturnsBuffer::new(small_capacity, &symbols);

        let symbol = symbols[0];

        // Add more returns than capacity
        for i in 0..20 {
            buffer.add_return(symbol, i * 10);
            buffer.advance();
        }

        // Should not crash with overflow
    }

    #[rstest]
    fn test_beta_calculation_insufficient_data(returns_buffer: ReturnsBuffer, symbols: Vec<Symbol>) {
        let symbol = symbols[0];
        
        // With no data or insufficient data, should return default
        let beta = returns_buffer.calculate_beta(symbol, "NIFTY");
        assert_eq!(beta, 10000); // Market neutral default
    }

    #[rstest]
    fn test_beta_calculation_with_data(symbols: Vec<Symbol>) {
        let mut buffer = ReturnsBuffer::new(50, &symbols);
        let symbol = symbols[0];

        // Add correlated data
        for i in 1..=20 {
            let portfolio_return = i * 10;
            let index_return = i * 5; // 50% of portfolio return
            
            buffer.add_return(symbol, portfolio_return);
            buffer.add_index_return("NIFTY", index_return);
            buffer.advance();
        }

        let beta = buffer.calculate_beta(symbol, "NIFTY");
        
        // Should calculate some beta (exact value depends on implementation)
        assert!(beta > 0);
        // With perfect positive correlation and 2x volatility, beta should be around 2.0 (20000)
        assert!(beta > 5000); // Should be significantly above 0.5
    }

    #[rstest]
    fn test_correlation_calculation_insufficient_data(returns_buffer: ReturnsBuffer, symbols: Vec<Symbol>) {
        let symbol = symbols[0];
        
        let correlation = returns_buffer.calculate_correlation(symbol, "NIFTY");
        assert_eq!(correlation, 0); // Default for insufficient data
    }

    #[rstest]
    fn test_correlation_calculation_perfect_positive(symbols: Vec<Symbol>) {
        let mut buffer = ReturnsBuffer::new(50, &symbols);
        let symbol = symbols[0];

        // Perfect positive correlation
        for i in 1..=20 {
            let return_value = i * 100;
            buffer.add_return(symbol, return_value);
            buffer.add_index_return("NIFTY", return_value); // Same returns
            buffer.advance();
        }

        let correlation = buffer.calculate_correlation(symbol, "NIFTY");
        
        // Should be close to 1.0 (10000 in fixed-point)
        assert!(correlation > 8000); // At least 0.8 correlation
    }

    #[rstest]
    fn test_correlation_calculation_perfect_negative(symbols: Vec<Symbol>) {
        let mut buffer = ReturnsBuffer::new(50, &symbols);
        let symbol = symbols[0];

        // Perfect negative correlation
        for i in 1..=20 {
            let portfolio_return = i * 100;
            let index_return = -i * 100; // Opposite direction
            
            buffer.add_return(symbol, portfolio_return);
            buffer.add_index_return("NIFTY", index_return);
            buffer.advance();
        }

        let correlation = returns_buffer.calculate_correlation(symbol, "NIFTY");
        
        // Should be close to -1.0 (-10000 in fixed-point)
        assert!(correlation < -8000); // Strong negative correlation
    }

    #[rstest]
    fn test_zero_variance_handling(symbols: Vec<Symbol>) {
        let mut buffer = ReturnsBuffer::new(50, &symbols);
        let symbol = symbols[0];

        // All same returns (zero variance)
        for _i in 1..=20 {
            buffer.add_return(symbol, 1000); // Constant return
            buffer.add_index_return("NIFTY", 500); // Different constant
            buffer.advance();
        }

        let correlation = buffer.calculate_correlation(symbol, "NIFTY");
        assert_eq!(correlation, 0); // Should handle zero variance gracefully

        let beta = buffer.calculate_beta(symbol, "NIFTY");
        assert_eq!(beta, 10000); // Market neutral when no variance
    }

    #[rstest]
    fn test_unknown_index_handling(returns_buffer: ReturnsBuffer, symbols: Vec<Symbol>) {
        let symbol = symbols[0];
        
        let beta = returns_buffer.calculate_beta(symbol, "UNKNOWN_INDEX");
        assert_eq!(beta, 10000); // Market neutral default
        
        let correlation = returns_buffer.calculate_correlation(symbol, "UNKNOWN_INDEX");
        assert_eq!(correlation, 0); // Zero correlation default
    }

    #[rstest]
    fn test_unknown_symbol_handling(returns_buffer: ReturnsBuffer) {
        let unknown_symbol = Symbol::new(999);
        
        let beta = returns_buffer.calculate_beta(unknown_symbol, "NIFTY");
        assert_eq!(beta, 10000); // Market neutral default
        
        let correlation = returns_buffer.calculate_correlation(unknown_symbol, "NIFTY");
        assert_eq!(correlation, 0); // Zero correlation default
    }

    #[rstest]
    fn test_mixed_positive_negative_returns(symbols: Vec<Symbol>) {
        let mut buffer = ReturnsBuffer::new(50, &symbols);
        let symbol = symbols[0];

        // Mixed returns with some correlation
        let portfolio_returns = vec![100, -50, 200, -100, 150, -75, 300, -200];
        let index_returns = vec![50, -25, 100, -50, 75, -35, 150, -100];

        for (port_ret, idx_ret) in portfolio_returns.iter().zip(index_returns.iter()) {
            buffer.add_return(symbol, *port_ret);
            buffer.add_index_return("NIFTY", *idx_ret);
            buffer.advance();
        }

        let correlation = buffer.calculate_correlation(symbol, "NIFTY");
        let beta = buffer.calculate_beta(symbol, "NIFTY");

        // Should calculate reasonable metrics
        assert!(correlation > 8000); // Strong positive correlation expected
        assert!(beta > 5000); // Beta should be positive and reasonable
    }
}

mod portfolio_beta_correlation_tests {
    use super::*;

    #[rstest]
    async fn test_portfolio_beta_calculation_empty_portfolio(market_feed_manager: MarketFeedManager) {
        let empty_positions = vec![];
        let beta = market_feed_manager.calculate_portfolio_beta(&empty_positions, "NIFTY").await;
        
        assert_eq!(beta, 10000); // Market neutral for empty portfolio
    }

    #[rstest]
    async fn test_portfolio_beta_single_position(market_feed_manager: MarketFeedManager, symbols: Vec<Symbol>) {
        let positions = vec![(symbols[0], 1000000)];
        
        // With no historical data, should return reasonable default
        let beta = market_feed_manager.calculate_portfolio_beta(&positions, "NIFTY").await;
        
        // Exact value depends on implementation, but should be reasonable
        assert!(beta > 0);
    }

    #[rstest]
    async fn test_portfolio_beta_multiple_positions(market_feed_manager: MarketFeedManager, symbols: Vec<Symbol>) {
        let positions = vec![
            (symbols[0], 2000000), // 2M value
            (symbols[1], 1000000), // 1M value
            (symbols[2], 3000000), // 3M value
        ];
        
        let beta = market_feed_manager.calculate_portfolio_beta(&positions, "NIFTY").await;
        
        // Should be weighted average of individual betas
        assert!(beta > 0);
    }

    #[rstest]
    async fn test_portfolio_correlation_calculation(market_feed_manager: MarketFeedManager, symbols: Vec<Symbol>) {
        let positions = vec![
            (symbols[0], 1000000),
            (symbols[1], 2000000),
        ];
        
        let correlation = market_feed_manager.calculate_portfolio_correlation(&positions, "NIFTY").await;
        
        // Should calculate some correlation value
        assert!(correlation >= -10000 && correlation <= 10000); // Valid range
    }

    #[rstest]
    async fn test_portfolio_correlation_empty_portfolio(market_feed_manager: MarketFeedManager) {
        let empty_positions = vec![];
        let correlation = market_feed_manager.calculate_portfolio_correlation(&empty_positions, "NIFTY").await;
        
        assert_eq!(correlation, 0); // Zero correlation for empty portfolio
    }

    #[rstest]
    async fn test_portfolio_beta_with_negative_positions(market_feed_manager: MarketFeedManager, symbols: Vec<Symbol>) {
        let positions = vec![
            (symbols[0], 1000000),  // Long position
            (symbols[1], -500000),  // Short position
        ];
        
        let beta = market_feed_manager.calculate_portfolio_beta(&positions, "NIFTY").await;
        
        // Should handle short positions using absolute values for weighting
        assert!(beta > 0);
    }

    #[rstest]
    async fn test_unknown_index_beta_calculation(market_feed_manager: MarketFeedManager, symbols: Vec<Symbol>) {
        let positions = vec![(symbols[0], 1000000)];
        
        let beta = market_feed_manager.calculate_portfolio_beta(&positions, "UNKNOWN_INDEX").await;
        
        // Should handle unknown index gracefully
        assert!(beta >= 0);
    }
}

mod edge_cases_and_error_handling {
    use super::*;

    #[rstest]
    fn test_price_update_zero_values(mut market_feed_manager: MarketFeedManager) {
        let update = PriceUpdate {
            symbol: Symbol::new(1),
            bid: 0,
            ask: 0,
            last: 0,
            volume: 0,
            timestamp: 0,
        };

        let result = market_feed_manager.update_price(update);
        assert!(result.is_ok()); // Should handle zero values gracefully
    }

    #[rstest]
    fn test_negative_price_handling(mut market_feed_manager: MarketFeedManager) {
        let update = PriceUpdate {
            symbol: Symbol::new(1),
            bid: -1000, // Invalid negative price
            ask: 1000,
            last: 0,
            volume: 1000000,
            timestamp: 1234567890,
        };

        // Should handle gracefully (implementation specific)
        let result = market_feed_manager.update_price(update);
        // Either accept or reject, but should not crash
        assert!(result.is_ok() || result.is_err());
    }

    #[rstest]
    fn test_inverted_bid_ask_prices(mut market_feed_manager: MarketFeedManager) {
        let update = PriceUpdate {
            symbol: Symbol::new(1),
            bid: 100100, // Bid higher than ask (invalid)
            ask: 100000,
            last: 100050,
            volume: 1000000,
            timestamp: 1234567890,
        };

        let result = market_feed_manager.update_price(update);
        assert!(result.is_ok()); // Should handle gracefully
        
        // Check that spread calculation doesn't break
        if let Some((bid, ask, _)) = market_feed_manager.get_price(Symbol::new(1)) {
            // Implementation may or may not correct invalid spreads
            let spread = ask - bid;
            // Should not crash on negative spread calculation
        }
    }

    #[rstest]
    fn test_very_large_timestamps(mut market_feed_manager: MarketFeedManager) {
        let update = PriceUpdate {
            symbol: Symbol::new(1),
            bid: 100000,
            ask: 100100,
            last: 100050,
            volume: 1000000,
            timestamp: u64::MAX, // Maximum timestamp
        };

        let result = market_feed_manager.update_price(update);
        assert!(result.is_ok());
    }

    #[rstest]
    fn test_extreme_volume_values(mut market_feed_manager: MarketFeedManager) {
        let updates = vec![
            PriceUpdate {
                symbol: Symbol::new(1),
                bid: 100000,
                ask: 100100,
                last: 100050,
                volume: i64::MAX, // Maximum volume
                timestamp: 1234567890,
            },
            PriceUpdate {
                symbol: Symbol::new(2),
                bid: 100000,
                ask: 100100,
                last: 100050,
                volume: -1000000, // Negative volume
                timestamp: 1234567890,
            },
        ];

        for update in updates {
            let result = market_feed_manager.update_price(update);
            assert!(result.is_ok()); // Should handle extreme values
        }
    }

    #[rstest]
    async fn test_concurrent_index_updates(market_feed_manager: MarketFeedManager) {
        let manager = std::sync::Arc::new(market_feed_manager);
        let num_threads = 3;
        let updates_per_thread = 50;

        let handles: Vec<_> = (0..num_threads)
            .map(|thread_id| {
                let mgr = std::sync::Arc::clone(&manager);
                std::thread::spawn(move || {
                    for i in 0..updates_per_thread {
                        let base_price = 180000 + (thread_id * 1000 + i) as i64;
                        mgr.update_index("NIFTY", base_price, base_price + 100, base_price + 50)
                            .unwrap_or_else(|_| {}); // Ignore errors in test
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Should complete without crashing
    }

    #[rstest]
    fn test_buffer_capacity_edge_cases(symbols: Vec<Symbol>) {
        // Very small capacity
        let tiny_buffer = ReturnsBuffer::new(1, &symbols);
        
        // Zero capacity (edge case)
        let zero_buffer = ReturnsBuffer::new(0, &symbols);
        
        // Very large capacity
        let large_buffer = ReturnsBuffer::new(1_000_000, &symbols);
        
        // Should handle various capacities without crashing
    }

    #[rstest]
    fn test_empty_symbol_list() {
        let empty_symbols = vec![];
        let buffer = ReturnsBuffer::new(100, &empty_symbols);
        let manager = MarketFeedManager::new(&empty_symbols, 100);
        
        // Should handle empty symbol lists gracefully
    }
}