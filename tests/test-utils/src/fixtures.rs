//! Test fixtures for common test data

use rstest::*;
use fake::{Fake, Faker};
use uuid::Uuid;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Standard test fixture for market data
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

#[derive(Debug, Clone)]
pub struct MarketDataFixture {
    pub symbol: String,
    pub bid_price: f64,
    pub ask_price: f64,
    pub last_price: f64,
    pub volume: f64,
    pub timestamp: i64,
}

/// Standard test fixture for order data
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

#[derive(Debug, Clone)]
pub struct OrderFixture {
    pub order_id: Uuid,
    pub client_order_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: f64,
    pub price: Option<f64>,
    pub status: OrderStatus,
}

#[derive(Debug, Clone)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
pub enum OrderType {
    Market,
    Limit,
    Stop,
    StopLimit,
}

#[derive(Debug, Clone)]
pub enum OrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

/// Portfolio fixture for testing
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

#[derive(Debug, Clone)]
pub struct PortfolioFixture {
    pub account_id: Uuid,
    pub cash_balance: f64,
    pub positions: Vec<PositionFixture>,
    pub total_value: f64,
    pub margin_used: f64,
    pub margin_available: f64,
}

#[derive(Debug, Clone)]
pub struct PositionFixture {
    pub symbol: String,
    pub quantity: f64,
    pub avg_price: f64,
    pub current_price: f64,
    pub pnl: f64,
}

/// Risk parameters fixture
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

#[derive(Debug, Clone)]
pub struct RiskParamsFixture {
    pub max_position_size: f64,
    pub max_order_size: f64,
    pub max_daily_loss: f64,
    pub max_leverage: f64,
    pub min_margin_ratio: f64,
    pub circuit_breaker_threshold: f64,
    pub rate_limit: u32,
}

/// Options data fixture
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

#[derive(Debug, Clone)]
pub struct OptionsFixture {
    pub underlying: String,
    pub strike: f64,
    pub expiry: chrono::DateTime<chrono::Utc>,
    pub option_type: OptionType,
    pub spot_price: f64,
    pub volatility: f64,
    pub risk_free_rate: f64,
    pub dividend_yield: f64,
}

#[derive(Debug, Clone)]
pub enum OptionType {
    Call,
    Put,
}

/// Test database fixture
#[fixture]
pub async fn test_db() -> TestDatabase {
    TestDatabase::new().await
}

pub struct TestDatabase {
    pub url: String,
    _temp_dir: tempfile::TempDir,
}

impl TestDatabase {
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

/// WebSocket test fixture
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

#[derive(Debug, Clone)]
pub struct WebSocketFixture {
    pub url: String,
    pub messages: Vec<String>,
    pub expected_responses: usize,
}