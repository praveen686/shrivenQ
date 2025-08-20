//! Test utilities and factories for backtesting tests

use backtesting::*;
use chrono::{DateTime, Utc, Duration};
use std::collections::HashMap;

/// Factory for creating test configurations
pub struct TestConfigFactory;

impl TestConfigFactory {
    /// Create a basic test configuration for short backtests
    pub fn basic_config() -> BacktestConfig {
        BacktestConfig {
            start_date: Utc::now() - Duration::days(30),
            end_date: Utc::now() - Duration::days(1),
            initial_capital: 100_000.0,
            commission_rate: 0.001,
            slippage_model: SlippageModel::Fixed { bps: 5.0 },
            data_frequency: DataFrequency::Daily,
            enable_shorting: false,
            margin_requirement: 0.5,
            risk_free_rate: 0.02,
        }
    }

    /// Create configuration for high-frequency testing
    pub fn hf_config() -> BacktestConfig {
        BacktestConfig {
            start_date: Utc::now() - Duration::hours(24),
            end_date: Utc::now() - Duration::hours(1),
            initial_capital: 50_000.0,
            commission_rate: 0.0005,
            slippage_model: SlippageModel::Linear { impact: 0.01 },
            data_frequency: DataFrequency::Minute,
            enable_shorting: true,
            margin_requirement: 0.3,
            risk_free_rate: 0.025,
        }
    }

    /// Create configuration with shorting enabled
    pub fn shorting_config() -> BacktestConfig {
        BacktestConfig {
            start_date: Utc::now() - Duration::days(90),
            end_date: Utc::now() - Duration::days(1),
            initial_capital: 200_000.0,
            commission_rate: 0.0015,
            slippage_model: SlippageModel::Square { impact: 0.05 },
            data_frequency: DataFrequency::Hour,
            enable_shorting: true,
            margin_requirement: 0.4,
            risk_free_rate: 0.03,
        }
    }
}

/// Factory for creating test market data
pub struct TestDataFactory;

impl TestDataFactory {
    /// Generate trending upward price data
    pub fn trending_up_data(days: i64, start_price: f64) -> Vec<(DateTime<Utc>, OHLCV)> {
        let mut data = Vec::new();
        let mut price = start_price;
        let mut date = Utc::now() - Duration::days(days);
        
        for _ in 0..days {
            let daily_change = 0.5 + (TestRandom::next() * 2.0); // 0.5 to 2.5 daily change
            price += daily_change;
            
            let open = price - 0.3;
            let high = price + TestRandom::next() * 1.0;
            let low = price - TestRandom::next() * 1.0;
            let volume = 50_000.0 + TestRandom::next() * 100_000.0;
            
            data.push((date, OHLCV {
                open,
                high,
                low,
                close: price,
                volume,
            }));
            
            date = date + Duration::days(1);
        }
        
        data
    }

    /// Generate trending downward price data
    pub fn trending_down_data(days: i64, start_price: f64) -> Vec<(DateTime<Utc>, OHLCV)> {
        let mut data = Vec::new();
        let mut price = start_price;
        let mut date = Utc::now() - Duration::days(days);
        
        for _ in 0..days {
            let daily_change = -(0.3 + (TestRandom::next() * 1.5)); // -0.3 to -1.8 daily change
            price = (price + daily_change).max(1.0); // Don't go below $1
            
            let open = price + 0.2;
            let high = price + TestRandom::next() * 0.8;
            let low = price - TestRandom::next() * 0.5;
            let volume = 75_000.0 + TestRandom::next() * 50_000.0;
            
            data.push((date, OHLCV {
                open,
                high,
                low,
                close: price,
                volume,
            }));
            
            date = date + Duration::days(1);
        }
        
        data
    }

    /// Generate sideways/range-bound price data
    pub fn sideways_data(days: i64, center_price: f64, range: f64) -> Vec<(DateTime<Utc>, OHLCV)> {
        let mut data = Vec::new();
        let mut date = Utc::now() - Duration::days(days);
        
        for _ in 0..days {
            let deviation = (TestRandom::next() - 0.5) * range;
            let price = center_price + deviation;
            
            let open = price + (TestRandom::next() - 0.5) * 0.5;
            let high = price + TestRandom::next() * 1.0;
            let low = price - TestRandom::next() * 1.0;
            let volume = 60_000.0 + TestRandom::next() * 80_000.0;
            
            data.push((date, OHLCV {
                open,
                high,
                low,
                close: price,
                volume,
            }));
            
            date = date + Duration::days(1);
        }
        
        data
    }

    /// Generate volatile/random walk price data
    pub fn volatile_data(days: i64, start_price: f64, volatility: f64) -> Vec<(DateTime<Utc>, OHLCV)> {
        let mut data = Vec::new();
        let mut price = start_price;
        let mut date = Utc::now() - Duration::days(days);
        
        for _ in 0..days {
            let change = (TestRandom::next() - 0.5) * volatility;
            price = (price + change).max(1.0);
            
            let open = price + (TestRandom::next() - 0.5) * 0.3;
            let high = price + TestRandom::next() * 2.0;
            let low = price - TestRandom::next() * 2.0;
            let volume = 40_000.0 + TestRandom::next() * 120_000.0;
            
            data.push((date, OHLCV {
                open,
                high,
                low,
                close: price,
                volume,
            }));
            
            date = date + Duration::days(1);
        }
        
        data
    }

    /// Generate intraday minute data
    pub fn intraday_data(hours: i64, start_price: f64) -> Vec<(DateTime<Utc>, OHLCV)> {
        let mut data = Vec::new();
        let mut price = start_price;
        let mut date = Utc::now() - Duration::hours(hours);
        
        for _ in 0..(hours * 60) {
            let change = (TestRandom::next() - 0.5) * 0.1; // Small minute-by-minute changes
            price = (price + change).max(1.0);
            
            let open = price + (TestRandom::next() - 0.5) * 0.05;
            let high = price + TestRandom::next() * 0.1;
            let low = price - TestRandom::next() * 0.1;
            let volume = 1_000.0 + TestRandom::next() * 5_000.0;
            
            data.push((date, OHLCV {
                open,
                high,
                low,
                close: price,
                volume,
            }));
            
            date = date + Duration::minutes(1);
        }
        
        data
    }

    /// Generate data with gaps (missing days)
    pub fn gapped_data(days: i64, start_price: f64, gap_probability: f64) -> Vec<(DateTime<Utc>, OHLCV)> {
        let mut data = Vec::new();
        let mut price = start_price;
        let mut date = Utc::now() - Duration::days(days);
        
        for _ in 0..days {
            // Randomly skip days to create gaps
            if TestRandom::next() < gap_probability {
                date = date + Duration::days(1);
                continue;
            }
            
            let change = (TestRandom::next() - 0.5) * 2.0;
            price = (price + change).max(1.0);
            
            let open = price + (TestRandom::next() - 0.5) * 0.5;
            let high = price + TestRandom::next() * 1.0;
            let low = price - TestRandom::next() * 1.0;
            let volume = 30_000.0 + TestRandom::next() * 70_000.0;
            
            data.push((date, OHLCV {
                open,
                high,
                low,
                close: price,
                volume,
            }));
            
            date = date + Duration::days(1);
        }
        
        data
    }

    /// Generate invalid/corrupted data for testing edge cases
    pub fn invalid_data() -> Vec<(DateTime<Utc>, OHLCV)> {
        let date = Utc::now() - Duration::days(1);
        vec![
            // Valid data point
            (date, OHLCV {
                open: 100.0,
                high: 105.0,
                low: 95.0,
                close: 102.0,
                volume: 50_000.0,
            }),
            // Invalid: high < low
            (date + Duration::days(1), OHLCV {
                open: 102.0,
                high: 98.0,  // Invalid: high < low
                low: 101.0,
                close: 99.0,
                volume: 45_000.0,
            }),
            // Invalid: close outside high-low range
            (date + Duration::days(2), OHLCV {
                open: 99.0,
                high: 103.0,
                low: 97.0,
                close: 105.0,  // Invalid: close > high
                volume: 55_000.0,
            }),
            // Negative values
            (date + Duration::days(3), OHLCV {
                open: -100.0,
                high: -95.0,
                low: -105.0,
                close: -98.0,
                volume: -1000.0,
            }),
        ]
    }
}

/// Factory for creating test orders
pub struct TestOrderFactory;

impl TestOrderFactory {
    /// Create a basic market buy order
    pub fn market_buy(symbol: &str, quantity: f64) -> Order {
        Order {
            id: format!("TEST_BUY_{}", TestRandom::next_id()),
            symbol: symbol.to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            quantity,
            price: None,
            timestamp: Utc::now(),
            time_in_force: TimeInForce::GTC,
        }
    }

    /// Create a basic market sell order
    pub fn market_sell(symbol: &str, quantity: f64) -> Order {
        Order {
            id: format!("TEST_SELL_{}", TestRandom::next_id()),
            symbol: symbol.to_string(),
            side: OrderSide::Sell,
            order_type: OrderType::Market,
            quantity,
            price: None,
            timestamp: Utc::now(),
            time_in_force: TimeInForce::GTC,
        }
    }

    /// Create a limit buy order
    pub fn limit_buy(symbol: &str, quantity: f64, price: f64) -> Order {
        Order {
            id: format!("TEST_LIMIT_BUY_{}", TestRandom::next_id()),
            symbol: symbol.to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            quantity,
            price: Some(price),
            timestamp: Utc::now(),
            time_in_force: TimeInForce::GTC,
        }
    }

    /// Create a limit sell order
    pub fn limit_sell(symbol: &str, quantity: f64, price: f64) -> Order {
        Order {
            id: format!("TEST_LIMIT_SELL_{}", TestRandom::next_id()),
            symbol: symbol.to_string(),
            side: OrderSide::Sell,
            order_type: OrderType::Limit,
            quantity,
            price: Some(price),
            timestamp: Utc::now(),
            time_in_force: TimeInForce::GTC,
        }
    }
}

/// Simple test strategy implementations
pub struct AlwaysBuyStrategy {
    pub symbol: String,
    pub position_size: f64,
}

impl Strategy for AlwaysBuyStrategy {
    fn generate_signals(&self, market: &MarketSnapshot, portfolio: &PortfolioState) -> Vec<TradingSignal> {
        // Always buy if we have cash and no position
        let has_position = portfolio.positions
            .iter()
            .any(|p| p.symbol == self.symbol && p.quantity > 0.0);
            
        if !has_position && portfolio.cash > self.position_size {
            vec![TradingSignal {
                symbol: self.symbol.clone(),
                side: OrderSide::Buy,
                order_type: OrderType::Market,
                quantity: self.position_size,
                price: None,
            }]
        } else {
            vec![]
        }
    }
}

pub struct AlwaysSellStrategy {
    pub symbol: String,
}

impl Strategy for AlwaysSellStrategy {
    fn generate_signals(&self, _market: &MarketSnapshot, portfolio: &PortfolioState) -> Vec<TradingSignal> {
        // Always sell if we have a position
        if let Some(position) = portfolio.positions.iter().find(|p| p.symbol == self.symbol && p.quantity > 0.0) {
            vec![TradingSignal {
                symbol: self.symbol.clone(),
                side: OrderSide::Sell,
                order_type: OrderType::Market,
                quantity: position.quantity,
                price: None,
            }]
        } else {
            vec![]
        }
    }
}

pub struct DoNothingStrategy;

impl Strategy for DoNothingStrategy {
    fn generate_signals(&self, _market: &MarketSnapshot, _portfolio: &PortfolioState) -> Vec<TradingSignal> {
        vec![]
    }
}

/// Deterministic random number generator for reproducible tests
pub struct TestRandom {
    state: std::sync::atomic::AtomicU64,
}

static TEST_RANDOM: TestRandom = TestRandom {
    state: std::sync::atomic::AtomicU64::new(12345),
};

impl TestRandom {
    /// Get next random value between 0.0 and 1.0
    pub fn next() -> f64 {
        use std::sync::atomic::Ordering;
        let mut x = TEST_RANDOM.state.load(Ordering::Relaxed);
        x = x.wrapping_mul(1103515245).wrapping_add(12345);
        TEST_RANDOM.state.store(x, Ordering::Relaxed);
        ((x / 65536) % 1000) as f64 / 1000.0
    }

    /// Get next ID for test objects
    pub fn next_id() -> u64 {
        use std::sync::atomic::Ordering;
        TEST_RANDOM.state.fetch_add(1, Ordering::Relaxed)
    }

    /// Reset random state for reproducible tests
    pub fn reset() {
        use std::sync::atomic::Ordering;
        TEST_RANDOM.state.store(12345, Ordering::Relaxed);
    }
}

/// Test assertion helpers
pub struct TestAssertions;

impl TestAssertions {
    /// Assert that two f64 values are approximately equal
    pub fn assert_approx_eq(actual: f64, expected: f64, tolerance: f64) {
        let diff = (actual - expected).abs();
        assert!(
            diff <= tolerance,
            "Values not approximately equal: actual={}, expected={}, diff={}, tolerance={}",
            actual, expected, diff, tolerance
        );
    }

    /// Assert that a performance metric is within reasonable bounds
    pub fn assert_metric_reasonable(metric: f64, name: &str, min: f64, max: f64) {
        assert!(
            metric >= min && metric <= max,
            "Metric {} = {} is outside reasonable bounds [{}, {}]",
            name, metric, min, max
        );
    }

    /// Assert that equity curve is monotonically increasing (for trending up strategy)
    pub fn assert_equity_trend_positive(equity_curve: &[(DateTime<Utc>, f64)], tolerance: f64) {
        if equity_curve.len() < 2 {
            return;
        }

        let initial_value = equity_curve.first().unwrap().1;
        let final_value = equity_curve.last().unwrap().1;
        
        assert!(
            final_value >= initial_value - tolerance,
            "Equity curve should trend upward: initial={}, final={}",
            initial_value, final_value
        );
    }

    /// Assert portfolio state is valid
    pub fn assert_portfolio_valid(state: &PortfolioState) {
        assert!(state.cash >= 0.0, "Cash balance cannot be negative: {}", state.cash);
        assert!(state.total_value > 0.0, "Total portfolio value must be positive: {}", state.total_value);
        
        for position in &state.positions {
            assert!(!position.symbol.is_empty(), "Position symbol cannot be empty");
            assert!(position.quantity != 0.0, "Position quantity cannot be zero");
        }
    }
}

/// Helper for creating market snapshots
pub struct MarketSnapshotBuilder {
    timestamp: DateTime<Utc>,
    prices: HashMap<String, f64>,
}

impl MarketSnapshotBuilder {
    pub fn new() -> Self {
        Self {
            timestamp: Utc::now(),
            prices: HashMap::new(),
        }
    }

    pub fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }

    pub fn with_price(mut self, symbol: &str, price: f64) -> Self {
        self.prices.insert(symbol.to_string(), price);
        self
    }

    pub fn build(self) -> MarketSnapshot {
        use dashmap::DashMap;
        let snapshot = MarketSnapshot {
            timestamp: self.timestamp,
            prices: DashMap::new(),
        };
        
        for (symbol, price) in self.prices {
            snapshot.prices.insert(symbol, price);
        }
        
        snapshot
    }
}