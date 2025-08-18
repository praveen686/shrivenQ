//! Factory patterns for generating test data

use fake::{Fake, Faker};
use uuid::Uuid;
use std::collections::HashMap;

/// Factory for creating test orders with customization
pub struct OrderFactory {
    default_symbol: String,
    default_quantity: f64,
}

impl OrderFactory {
    pub fn new() -> Self {
        Self {
            default_symbol: "BTCUSDT".to_string(),
            default_quantity: 1.0,
        }
    }
    
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.default_symbol = symbol.into();
        self
    }
    
    pub fn with_quantity(mut self, quantity: f64) -> Self {
        self.default_quantity = quantity;
        self
    }
    
    pub fn build_limit_order(&self, price: f64) -> TestOrder {
        TestOrder {
            id: Uuid::new_v4(),
            symbol: self.default_symbol.clone(),
            order_type: "LIMIT".to_string(),
            side: if Faker.fake::<bool>() { "BUY" } else { "SELL" },
            quantity: self.default_quantity,
            price: Some(price),
            stop_price: None,
            status: "NEW".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
    
    pub fn build_market_order(&self) -> TestOrder {
        TestOrder {
            id: Uuid::new_v4(),
            symbol: self.default_symbol.clone(),
            order_type: "MARKET".to_string(),
            side: if Faker.fake::<bool>() { "BUY" } else { "SELL" },
            quantity: self.default_quantity,
            price: None,
            stop_price: None,
            status: "NEW".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
    
    pub fn build_batch(&self, count: usize, price_range: (f64, f64)) -> Vec<TestOrder> {
        (0..count)
            .map(|_| {
                let price = Faker.fake::<f64>() * (price_range.1 - price_range.0) + price_range.0;
                self.build_limit_order(price)
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct TestOrder {
    pub id: Uuid,
    pub symbol: String,
    pub order_type: String,
    pub side: String,
    pub quantity: f64,
    pub price: Option<f64>,
    pub stop_price: Option<f64>,
    pub status: String,
    pub timestamp: i64,
}

/// Factory for creating market data
pub struct MarketDataFactory {
    base_price: f64,
    volatility: f64,
}

impl MarketDataFactory {
    pub fn new(base_price: f64) -> Self {
        Self {
            base_price,
            volatility: 0.01, // 1% volatility
        }
    }
    
    pub fn with_volatility(mut self, volatility: f64) -> Self {
        self.volatility = volatility;
        self
    }
    
    pub fn generate_tick(&self) -> MarketTick {
        let random_walk = (Faker.fake::<f64>() - 0.5) * 2.0 * self.volatility;
        let new_price = self.base_price * (1.0 + random_walk);
        
        MarketTick {
            symbol: "BTCUSDT".to_string(),
            bid: new_price - 0.01,
            ask: new_price + 0.01,
            last: new_price,
            volume: Faker.fake::<f64>() * 1000.0,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
    
    pub fn generate_orderbook(&self, levels: usize) -> OrderBook {
        let mut bids = Vec::new();
        let mut asks = Vec::new();
        
        for i in 0..levels {
            let bid_price = self.base_price - (i as f64 * 0.1);
            let ask_price = self.base_price + (i as f64 * 0.1);
            
            bids.push(PriceLevel {
                price: bid_price,
                quantity: Faker.fake::<f64>() * 100.0,
            });
            
            asks.push(PriceLevel {
                price: ask_price,
                quantity: Faker.fake::<f64>() * 100.0,
            });
        }
        
        OrderBook {
            symbol: "BTCUSDT".to_string(),
            bids,
            asks,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
    
    pub fn generate_candles(&self, count: usize, interval: &str) -> Vec<Candle> {
        let mut candles = Vec::new();
        let mut current_price = self.base_price;
        let mut timestamp = chrono::Utc::now().timestamp_millis() - (count as i64 * 60000);
        
        for _ in 0..count {
            let random_walk = (Faker.fake::<f64>() - 0.5) * 2.0 * self.volatility;
            let high = current_price * (1.0 + self.volatility);
            let low = current_price * (1.0 - self.volatility);
            let close = current_price * (1.0 + random_walk);
            
            candles.push(Candle {
                symbol: "BTCUSDT".to_string(),
                interval: interval.to_string(),
                open: current_price,
                high,
                low,
                close,
                volume: Faker.fake::<f64>() * 10000.0,
                timestamp,
            });
            
            current_price = close;
            timestamp += 60000; // 1 minute
        }
        
        candles
    }
}

#[derive(Debug, Clone)]
pub struct MarketTick {
    pub symbol: String,
    pub bid: f64,
    pub ask: f64,
    pub last: f64,
    pub volume: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone)]
pub struct OrderBook {
    pub symbol: String,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub timestamp: i64,
}

#[derive(Debug, Clone)]
pub struct PriceLevel {
    pub price: f64,
    pub quantity: f64,
}

#[derive(Debug, Clone)]
pub struct Candle {
    pub symbol: String,
    pub interval: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub timestamp: i64,
}

/// Factory for creating test portfolios
pub struct PortfolioFactory;

impl PortfolioFactory {
    pub fn create_with_positions(positions: Vec<(String, f64, f64)>) -> TestPortfolio {
        let mut holdings = HashMap::new();
        let mut total_value = 100000.0; // Starting cash
        
        for (symbol, quantity, avg_price) in positions {
            holdings.insert(symbol.clone(), Position {
                symbol,
                quantity,
                avg_price,
                current_value: quantity * avg_price,
            });
            total_value += quantity * avg_price;
        }
        
        TestPortfolio {
            account_id: Uuid::new_v4(),
            cash: 100000.0,
            holdings,
            total_value,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
    
    pub fn create_empty() -> TestPortfolio {
        TestPortfolio {
            account_id: Uuid::new_v4(),
            cash: 100000.0,
            holdings: HashMap::new(),
            total_value: 100000.0,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestPortfolio {
    pub account_id: Uuid,
    pub cash: f64,
    pub holdings: HashMap<String, Position>,
    pub total_value: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: String,
    pub quantity: f64,
    pub avg_price: f64,
    pub current_value: f64,
}

/// Factory for creating test signals
pub struct SignalFactory;

impl SignalFactory {
    pub fn create_buy_signal(symbol: &str, strength: f64) -> TestSignal {
        TestSignal {
            id: Uuid::new_v4(),
            symbol: symbol.to_string(),
            signal_type: SignalType::Buy,
            strength: strength.clamp(0.0, 1.0),
            source: "TestStrategy".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            metadata: HashMap::new(),
        }
    }
    
    pub fn create_sell_signal(symbol: &str, strength: f64) -> TestSignal {
        TestSignal {
            id: Uuid::new_v4(),
            symbol: symbol.to_string(),
            signal_type: SignalType::Sell,
            strength: strength.clamp(0.0, 1.0),
            source: "TestStrategy".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            metadata: HashMap::new(),
        }
    }
    
    pub fn create_neutral_signal(symbol: &str) -> TestSignal {
        TestSignal {
            id: Uuid::new_v4(),
            symbol: symbol.to_string(),
            signal_type: SignalType::Neutral,
            strength: 0.5,
            source: "TestStrategy".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            metadata: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestSignal {
    pub id: Uuid,
    pub symbol: String,
    pub signal_type: SignalType,
    pub strength: f64,
    pub source: String,
    pub timestamp: i64,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub enum SignalType {
    Buy,
    Sell,
    Neutral,
}