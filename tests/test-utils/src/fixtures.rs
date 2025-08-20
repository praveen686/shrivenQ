//! Test fixtures for common test data

// Allow missing docs for rstest generated code
#![allow(missing_docs)]

use rstest::*;
use uuid::Uuid;

/// Standard test fixture for market data
/// 
/// Returns a `MarketDataFixture` with realistic BTC/USDT market data for testing.
/// The fixture includes typical bid/ask spread and volume data commonly used
/// in trading system tests.
#[fixture]
pub fn market_data() -> MarketDataFixture {
    MarketDataFixture {
        symbol: "BTCUSDT".to_string(),
        bid_price: 45000.0,
        ask_price: 45010.0,
        last_price: 45005.0,
        volume: 1000000.0,
        timestamp: chrono::Utc::now().timestamp_millis(),
    }
}

/// Market data fixture for testing market data scenarios
/// 
/// Contains standardized market data fields commonly used across trading tests.
/// All price values are in the quote currency (typically USD for crypto pairs).
#[derive(Debug, Clone)]
pub struct MarketDataFixture {
    /// Trading pair symbol (e.g., "BTCUSDT", "ETHUSDT")
    pub symbol: String,
    /// Current highest bid price in quote currency
    pub bid_price: f64,
    /// Current lowest ask price in quote currency
    pub ask_price: f64,
    /// Last executed trade price in quote currency
    pub last_price: f64,
    /// 24-hour trading volume in base currency
    pub volume: f64,
    /// Unix timestamp in milliseconds when this data was captured
    pub timestamp: i64,
}

/// Standard test fixture for order data
/// 
/// Returns an `OrderFixture` with a limit buy order for BTC/USDT.
/// The fixture includes all necessary fields for testing order management
/// and execution scenarios.
#[fixture]
pub fn order_data() -> OrderFixture {
    OrderFixture {
        order_id: Uuid::new_v4(),
        client_order_id: format!("TEST-{}", Uuid::new_v4()),
        symbol: "BTCUSDT".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        quantity: 0.1,
        price: Some(45000.0),
        status: OrderStatus::New,
    }
}

/// Order fixture for testing order management scenarios
/// 
/// Represents a trading order with all necessary fields for order lifecycle testing.
/// Supports both market and limit orders with various execution states.
#[derive(Debug, Clone)]
pub struct OrderFixture {
    /// Unique system-generated order identifier
    pub order_id: Uuid,
    /// Client-provided order identifier for tracking
    pub client_order_id: String,
    /// Trading pair symbol for this order
    pub symbol: String,
    /// Order side indicating buy or sell direction
    pub side: OrderSide,
    /// Type of order (market, limit, stop, etc.)
    pub order_type: OrderType,
    /// Order quantity in base currency units
    pub quantity: f64,
    /// Order price in quote currency (None for market orders)
    pub price: Option<f64>,
    /// Current execution status of the order
    pub status: OrderStatus,
}

/// Order side enumeration for buy/sell direction
#[derive(Debug, Clone)]
pub enum OrderSide {
    /// Buy order - purchasing the base currency
    Buy,
    /// Sell order - selling the base currency
    Sell,
}

/// Order type enumeration for different execution strategies
#[derive(Debug, Clone)]
pub enum OrderType {
    /// Market order - execute immediately at current market price
    Market,
    /// Limit order - execute only at specified price or better
    Limit,
    /// Stop order - convert to market order when stop price is reached
    Stop,
    /// Stop-limit order - convert to limit order when stop price is reached
    StopLimit,
}

/// Order status enumeration for tracking order lifecycle
#[derive(Debug, Clone)]
pub enum OrderStatus {
    /// Order has been accepted but not yet executed
    New,
    /// Order has been partially executed with remaining quantity pending
    PartiallyFilled,
    /// Order has been completely executed
    Filled,
    /// Order has been cancelled before full execution
    Cancelled,
    /// Order was rejected due to validation or risk management rules
    Rejected,
}

/// Standard test fixture for portfolio data
/// 
/// Returns a `PortfolioFixture` with a diversified portfolio containing
/// cash balance and positions in BTC and ETH. Includes realistic P&L
/// and margin data for comprehensive portfolio testing.
#[fixture]
pub fn portfolio() -> PortfolioFixture {
    PortfolioFixture {
        account_id: Uuid::new_v4(),
        cash_balance: 100000.0,
        positions: vec![
            PositionFixture {
                symbol: "BTCUSDT".to_string(),
                quantity: 2.5,
                avg_price: 44000.0,
                current_price: 45000.0,
                pnl: 2500.0,
            },
            PositionFixture {
                symbol: "ETHUSDT".to_string(),
                quantity: 10.0,
                avg_price: 2800.0,
                current_price: 2850.0,
                pnl: 500.0,
            },
        ],
        total_value: 145000.0,
        margin_used: 50000.0,
        margin_available: 50000.0,
    }
}

/// Portfolio fixture for testing portfolio management scenarios
/// 
/// Contains comprehensive portfolio state including cash, positions, and margin data.
/// All monetary values are in the account's base currency (typically USD).
#[derive(Debug, Clone)]
pub struct PortfolioFixture {
    /// Unique identifier for the trading account
    pub account_id: Uuid,
    /// Available cash balance in account base currency
    pub cash_balance: f64,
    /// List of open positions across different trading pairs
    pub positions: Vec<PositionFixture>,
    /// Total portfolio value including cash and position values in base currency
    pub total_value: f64,
    /// Amount of margin currently used for leveraged positions in base currency
    pub margin_used: f64,
    /// Available margin for new positions in base currency
    pub margin_available: f64,
}

/// Position fixture for testing individual position scenarios
/// 
/// Represents a single trading position with entry price, current valuation, and P&L.
/// Positive quantity indicates long position, negative indicates short position.
#[derive(Debug, Clone)]
pub struct PositionFixture {
    /// Trading pair symbol for this position
    pub symbol: String,
    /// Position size in base currency units (positive = long, negative = short)
    pub quantity: f64,
    /// Average entry price in quote currency
    pub avg_price: f64,
    /// Current market price in quote currency
    pub current_price: f64,
    /// Unrealized profit/loss in account base currency
    pub pnl: f64,
}

/// Standard test fixture for risk management parameters
/// 
/// Returns a `RiskParamsFixture` with conservative risk limits suitable
/// for testing risk management scenarios. Includes position limits,
/// leverage constraints, and circuit breaker thresholds.
#[fixture]
pub fn risk_params() -> RiskParamsFixture {
    RiskParamsFixture {
        max_position_size: 100000.0,
        max_order_size: 10000.0,
        max_daily_loss: 5000.0,
        max_leverage: 3.0,
        min_margin_ratio: 0.25,
        circuit_breaker_threshold: 0.1,
        rate_limit: 100,
    }
}

/// Risk management parameters fixture for testing risk controls
/// 
/// Defines comprehensive risk limits and thresholds used by the risk management system.
/// All monetary values are in the account's base currency.
#[derive(Debug, Clone)]
pub struct RiskParamsFixture {
    /// Maximum allowed position size in base currency
    pub max_position_size: f64,
    /// Maximum allowed single order size in base currency
    pub max_order_size: f64,
    /// Maximum allowed daily loss in base currency
    pub max_daily_loss: f64,
    /// Maximum leverage ratio allowed (e.g., 3.0 = 3:1 leverage)
    pub max_leverage: f64,
    /// Minimum margin ratio required to maintain positions (e.g., 0.25 = 25%)
    pub min_margin_ratio: f64,
    /// Price movement threshold that triggers circuit breaker (e.g., 0.1 = 10%)
    pub circuit_breaker_threshold: f64,
    /// Maximum number of orders allowed per time window
    pub rate_limit: u32,
}

/// Standard test fixture for options contract data
/// 
/// Returns an `OptionsFixture` with a NIFTY call option contract.
/// Includes all parameters necessary for options pricing calculations
/// and Greeks computation in testing scenarios.
#[fixture]
pub fn options_data() -> OptionsFixture {
    OptionsFixture {
        underlying: "NIFTY".to_string(),
        strike: 20000.0,
        expiry: chrono::Utc::now() + chrono::Duration::days(30),
        option_type: OptionType::Call,
        spot_price: 19800.0,
        volatility: 0.25,
        risk_free_rate: 0.065,
        dividend_yield: 0.01,
    }
}

/// Options contract fixture for testing options trading scenarios
/// 
/// Contains all parameters necessary for options pricing and risk calculations.
/// Used primarily for testing the options trading engine and Greeks calculations.
#[derive(Debug, Clone)]
pub struct OptionsFixture {
    /// Symbol of the underlying asset (e.g., "NIFTY", "BANKNIFTY")
    pub underlying: String,
    /// Strike price of the option contract
    pub strike: f64,
    /// Expiration date and time of the option contract
    pub expiry: chrono::DateTime<chrono::Utc>,
    /// Type of option contract (call or put)
    pub option_type: OptionType,
    /// Current spot price of the underlying asset
    pub spot_price: f64,
    /// Implied volatility as a decimal (e.g., 0.25 = 25%)
    pub volatility: f64,
    /// Risk-free interest rate as a decimal (e.g., 0.065 = 6.5%)
    pub risk_free_rate: f64,
    /// Dividend yield of the underlying as a decimal (e.g., 0.01 = 1%)
    pub dividend_yield: f64,
}

/// Option type enumeration for call and put options
#[derive(Debug, Clone)]
pub enum OptionType {
    /// Call option - right to buy the underlying at strike price
    Call,
    /// Put option - right to sell the underlying at strike price
    Put,
}

/// Standard test fixture for database connections
/// 
/// Returns a `TestDatabase` with an isolated SQLite database instance.
/// Each test gets a fresh database that is automatically cleaned up
/// after the test completes.
#[fixture]
pub async fn test_db() -> TestDatabase {
    TestDatabase::new().await
}

/// Test database fixture for integration testing
/// 
/// Provides an isolated SQLite database instance for each test.
/// The database is automatically cleaned up when the fixture is dropped.
pub struct TestDatabase {
    /// Database connection URL for the test database
    pub url: String,
    /// Temporary directory handle (kept private to prevent accidental deletion)
    _temp_dir: tempfile::TempDir,
}

impl std::fmt::Debug for TestDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestDatabase")
            .field("url", &self.url)
            .field("temp_dir_path", &self._temp_dir.path())
            .finish()
    }
}

impl TestDatabase {
    /// Creates a new isolated test database instance
    /// 
    /// Creates a temporary directory and SQLite database file for testing.
    /// The database and directory are automatically cleaned up when this
    /// instance is dropped.
    /// 
    /// # Returns
    /// 
    /// A new `TestDatabase` instance with a unique database URL
    /// 
    /// # Panics
    /// 
    /// Panics if the temporary directory cannot be created
    pub async fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test.db");
        let url = format!("sqlite://{}", db_path.display());
        
        Self {
            url,
            _temp_dir: temp_dir,
        }
    }
}

/// Standard test fixture for WebSocket connections
/// 
/// Returns a `WebSocketFixture` with predefined orderbook and trade messages
/// for testing real-time data streaming scenarios. Includes expected response
/// counts for validation.
#[fixture]
pub fn websocket_data() -> WebSocketFixture {
    WebSocketFixture {
        url: "ws://localhost:8080/ws".to_string(),
        messages: vec![
            r#"{"type":"orderbook","symbol":"BTCUSDT","bids":[[45000,10]],"asks":[[45010,10]]}"#.to_string(),
            r#"{"type":"trade","symbol":"BTCUSDT","price":45005,"quantity":0.5}"#.to_string(),
        ],
        expected_responses: 2,
    }
}

/// WebSocket fixture for testing real-time data streaming
/// 
/// Provides standardized WebSocket test scenarios with predefined messages
/// and expected response counts for testing WebSocket connections and handlers.
#[derive(Debug, Clone)]
pub struct WebSocketFixture {
    /// WebSocket server URL for connection testing
    pub url: String,
    /// List of JSON messages to send during testing
    pub messages: Vec<String>,
    /// Expected number of responses from the WebSocket server
    pub expected_responses: usize,
}