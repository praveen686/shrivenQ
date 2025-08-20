//! Factory patterns for generating test data

use fake::{Fake, Faker};
use uuid::Uuid;
use std::collections::HashMap;

/// Factory for creating test orders with customization capabilities.
/// 
/// Provides a fluent interface for building various types of orders (limit, market, stop)
/// with configurable default values for symbol and quantity.
/// 
/// # Examples
/// 
/// ```
/// let factory = OrderFactory::new()
///     .with_symbol("ETHUSDT")
///     .with_quantity(2.5);
/// 
/// let limit_order = factory.build_limit_order(1500.0);
/// let market_order = factory.build_market_order();
/// ```
#[derive(Debug)]
pub struct OrderFactory {
    /// Default symbol for orders created by this factory
    default_symbol: String,
    /// Default quantity for orders created by this factory
    default_quantity: f64,
}

impl OrderFactory {
    /// Creates a new OrderFactory with default settings.
    /// 
    /// Default symbol is "BTCUSDT" and default quantity is 1.0.
    pub fn new() -> Self {
        Self {
            default_symbol: "BTCUSDT".to_string(),
            default_quantity: 1.0,
        }
    }
    
    /// Sets the default symbol for orders created by this factory.
    /// 
    /// # Arguments
    /// 
    /// * `symbol` - The trading symbol (e.g., "BTCUSDT", "ETHUSDT")
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.default_symbol = symbol.into();
        self
    }
    
    /// Sets the default quantity for orders created by this factory.
    /// 
    /// # Arguments
    /// 
    /// * `quantity` - The order quantity
    pub fn with_quantity(mut self, quantity: f64) -> Self {
        self.default_quantity = quantity;
        self
    }
    
    /// Builds a limit order with the specified price.
    /// 
    /// Creates a TestOrder with type "LIMIT", randomly assigned side (BUY/SELL),
    /// and the configured default symbol and quantity.
    /// 
    /// # Arguments
    /// 
    /// * `price` - The limit price for the order
    /// 
    /// # Returns
    /// 
    /// A TestOrder configured as a limit order
    pub fn build_limit_order(&self, price: f64) -> TestOrder {
        TestOrder {
            id: Uuid::new_v4(),
            symbol: self.default_symbol.clone(),
            order_type: "LIMIT".to_string(),
            side: if Faker.fake::<bool>() { "BUY".to_string() } else { "SELL".to_string() },
            quantity: self.default_quantity,
            price: Some(price),
            stop_price: None,
            status: "NEW".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
    
    /// Builds a market order with no specified price.
    /// 
    /// Creates a TestOrder with type "MARKET", randomly assigned side (BUY/SELL),
    /// and the configured default symbol and quantity.
    /// 
    /// # Returns
    /// 
    /// A TestOrder configured as a market order
    pub fn build_market_order(&self) -> TestOrder {
        TestOrder {
            id: Uuid::new_v4(),
            symbol: self.default_symbol.clone(),
            order_type: "MARKET".to_string(),
            side: if Faker.fake::<bool>() { "BUY".to_string() } else { "SELL".to_string() },
            quantity: self.default_quantity,
            price: None,
            stop_price: None,
            status: "NEW".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
    
    /// Builds a batch of limit orders with prices within the specified range.
    /// 
    /// Each order will have a randomly generated price within the given range
    /// and will use the factory's default symbol and quantity settings.
    /// 
    /// # Arguments
    /// 
    /// * `count` - Number of orders to generate
    /// * `price_range` - Tuple of (min_price, max_price) for random price generation
    /// 
    /// # Returns
    /// 
    /// A vector of TestOrder instances
    pub fn build_batch(&self, count: usize, price_range: (f64, f64)) -> Vec<TestOrder> {
        (0..count)
            .map(|_| {
                let price = Faker.fake::<f64>() * (price_range.1 - price_range.0) + price_range.0;
                self.build_limit_order(price)
            })
            .collect()
    }
}

/// Represents a test order with all relevant order information.
/// 
/// Used for testing order processing, validation, and execution logic.
/// Contains fields for order identification, trading parameters, and status tracking.
#[derive(Debug, Clone)]
pub struct TestOrder {
    /// Unique identifier for the order
    pub id: Uuid,
    /// Trading symbol (e.g., "BTCUSDT")
    pub symbol: String,
    /// Order type ("LIMIT", "MARKET", "STOP", etc.)
    pub order_type: String,
    /// Order side ("BUY" or "SELL")
    pub side: String,
    /// Quantity to buy or sell
    pub quantity: f64,
    /// Limit price (None for market orders)
    pub price: Option<f64>,
    /// Stop price for stop orders (None for other order types)
    pub stop_price: Option<f64>,
    /// Order status ("NEW", "FILLED", "CANCELLED", etc.)
    pub status: String,
    /// Order creation timestamp in milliseconds
    pub timestamp: i64,
}

/// Factory for creating realistic market data for testing.
/// 
/// Generates market ticks, order books, and candlestick data with configurable
/// volatility and base price. Uses random walk models to simulate realistic
/// price movements.
/// 
/// # Examples
/// 
/// ```
/// let factory = MarketDataFactory::new(50000.0)
///     .with_volatility(0.02); // 2% volatility
/// 
/// let tick = factory.generate_tick();
/// let orderbook = factory.generate_orderbook(10);
/// let candles = factory.generate_candles(100, "1m");
/// ```
#[derive(Debug)]
pub struct MarketDataFactory {
    /// Base price for generating market data
    base_price: f64,
    /// Volatility factor for price movements (as decimal, e.g., 0.01 = 1%)
    volatility: f64,
}

impl MarketDataFactory {
    /// Creates a new MarketDataFactory with the specified base price.
    /// 
    /// # Arguments
    /// 
    /// * `base_price` - The baseline price for generating market data
    pub fn new(base_price: f64) -> Self {
        Self {
            base_price,
            volatility: 0.01, // 1% volatility
        }
    }
    
    /// Sets the volatility factor for price movements.
    /// 
    /// # Arguments
    /// 
    /// * `volatility` - Volatility as a decimal (e.g., 0.02 for 2%)
    pub fn with_volatility(mut self, volatility: f64) -> Self {
        self.volatility = volatility;
        self
    }
    
    /// Generates a single market tick with bid, ask, and last prices.
    /// 
    /// Uses a random walk model based on the configured volatility to
    /// generate realistic price movements around the base price.
    /// 
    /// # Returns
    /// 
    /// A MarketTick with simulated bid/ask spread and volume
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
    
    /// Generates an order book with the specified number of price levels.
    /// 
    /// Creates bid and ask levels around the base price with random quantities.
    /// Each level is spaced 0.1 price units from the previous level.
    /// 
    /// # Arguments
    /// 
    /// * `levels` - Number of price levels on each side of the order book
    /// 
    /// # Returns
    /// 
    /// An OrderBook with populated bid and ask levels
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
    
    /// Generates a series of candlestick data.
    /// 
    /// Creates historical candle data using a random walk model that evolves
    /// the price over time. Each candle includes OHLC prices and volume.
    /// 
    /// # Arguments
    /// 
    /// * `count` - Number of candles to generate
    /// * `interval` - Time interval string (e.g., "1m", "5m", "1h")
    /// 
    /// # Returns
    /// 
    /// A vector of Candle instances representing historical price data
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

/// Represents a single market tick with bid, ask, and last price information.
/// 
/// Contains real-time market data including the current bid/ask spread,
/// last traded price, volume, and timestamp.
#[derive(Debug, Clone)]
pub struct MarketTick {
    /// Trading symbol for this tick
    pub symbol: String,
    /// Highest bid price
    pub bid: f64,
    /// Lowest ask price
    pub ask: f64,
    /// Last traded price
    pub last: f64,
    /// Trading volume
    pub volume: f64,
    /// Tick timestamp in milliseconds
    pub timestamp: i64,
}

/// Represents a market order book with bid and ask levels.
/// 
/// Contains the current state of buy and sell orders at different price levels,
/// providing depth information for market analysis.
#[derive(Debug, Clone)]
pub struct OrderBook {
    /// Trading symbol for this order book
    pub symbol: String,
    /// Bid levels (buy orders) sorted by price descending
    pub bids: Vec<PriceLevel>,
    /// Ask levels (sell orders) sorted by price ascending
    pub asks: Vec<PriceLevel>,
    /// Order book timestamp in milliseconds
    pub timestamp: i64,
}

/// Represents a single price level in an order book.
/// 
/// Contains the price and total quantity available at that price level.
#[derive(Debug, Clone)]
pub struct PriceLevel {
    /// Price for this level
    pub price: f64,
    /// Total quantity available at this price
    pub quantity: f64,
}

/// Represents a candlestick/OHLC bar for technical analysis.
/// 
/// Contains open, high, low, close prices and volume for a specific time interval.
#[derive(Debug, Clone)]
pub struct Candle {
    /// Trading symbol for this candle
    pub symbol: String,
    /// Time interval (e.g., "1m", "5m", "1h")
    pub interval: String,
    /// Opening price for the interval
    pub open: f64,
    /// Highest price during the interval
    pub high: f64,
    /// Lowest price during the interval
    pub low: f64,
    /// Closing price for the interval
    pub close: f64,
    /// Trading volume during the interval
    pub volume: f64,
    /// Candle timestamp in milliseconds
    pub timestamp: i64,
}

/// Factory for creating test portfolios with various configurations.
/// 
/// Provides methods to create portfolios with predefined positions or
/// empty portfolios for testing portfolio management functionality.
#[derive(Debug)]
pub struct PortfolioFactory;

impl PortfolioFactory {
    /// Creates a portfolio with the specified positions.
    /// 
    /// Each position is defined by a tuple of (symbol, quantity, average_price).
    /// The portfolio starts with $100,000 cash and calculates total value
    /// including the value of all positions.
    /// 
    /// # Arguments
    /// 
    /// * `positions` - Vector of (symbol, quantity, avg_price) tuples
    /// 
    /// # Returns
    /// 
    /// A TestPortfolio with the specified positions and calculated total value
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
    
    /// Creates an empty portfolio with only cash.
    /// 
    /// Starts with $100,000 cash and no positions.
    /// 
    /// # Returns
    /// 
    /// An empty TestPortfolio with only cash holdings
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

/// Represents a test portfolio with cash and asset positions.
/// 
/// Tracks account balance, individual positions, and total portfolio value
/// for testing portfolio management and trading strategies.
#[derive(Debug, Clone)]
pub struct TestPortfolio {
    /// Unique identifier for the account
    pub account_id: Uuid,
    /// Available cash balance
    pub cash: f64,
    /// Map of symbol to position holdings
    pub holdings: HashMap<String, Position>,
    /// Total portfolio value (cash + positions)
    pub total_value: f64,
    /// Portfolio snapshot timestamp in milliseconds
    pub timestamp: i64,
}

/// Represents a position in a specific trading instrument.
/// 
/// Tracks quantity held, average purchase price, and current market value
/// for portfolio valuation and risk management.
#[derive(Debug, Clone)]
pub struct Position {
    /// Trading symbol for this position
    pub symbol: String,
    /// Quantity held (positive for long, negative for short)
    pub quantity: f64,
    /// Average purchase price
    pub avg_price: f64,
    /// Current market value of the position
    pub current_value: f64,
}

/// Factory for creating trading signals for strategy testing.
/// 
/// Generates buy, sell, and neutral signals with configurable strength
/// and metadata for testing trading algorithms and signal processing.
#[derive(Debug)]
pub struct SignalFactory;

impl SignalFactory {
    /// Creates a buy signal for the specified symbol.
    /// 
    /// # Arguments
    /// 
    /// * `symbol` - Trading symbol for the signal
    /// * `strength` - Signal strength between 0.0 and 1.0 (clamped)
    /// 
    /// # Returns
    /// 
    /// A TestSignal configured as a buy signal
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
    
    /// Creates a sell signal for the specified symbol.
    /// 
    /// # Arguments
    /// 
    /// * `symbol` - Trading symbol for the signal
    /// * `strength` - Signal strength between 0.0 and 1.0 (clamped)
    /// 
    /// # Returns
    /// 
    /// A TestSignal configured as a sell signal
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
    
    /// Creates a neutral signal for the specified symbol.
    /// 
    /// Neutral signals indicate no strong directional bias and have
    /// a default strength of 0.5.
    /// 
    /// # Arguments
    /// 
    /// * `symbol` - Trading symbol for the signal
    /// 
    /// # Returns
    /// 
    /// A TestSignal configured as a neutral signal
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

/// Represents a trading signal generated by a strategy or indicator.
/// 
/// Contains signal direction, strength, source information, and optional
/// metadata for testing signal processing and trading algorithms.
#[derive(Debug, Clone)]
pub struct TestSignal {
    /// Unique identifier for the signal
    pub id: Uuid,
    /// Trading symbol this signal applies to
    pub symbol: String,
    /// Type of signal (Buy, Sell, or Neutral)
    pub signal_type: SignalType,
    /// Signal strength between 0.0 and 1.0
    pub strength: f64,
    /// Source that generated this signal
    pub source: String,
    /// Signal generation timestamp in milliseconds
    pub timestamp: i64,
    /// Additional metadata about the signal
    pub metadata: HashMap<String, String>,
}

/// Enumeration of trading signal types.
/// 
/// Represents the directional bias of a trading signal for strategy
/// decision making and order generation.
#[derive(Debug, Clone)]
pub enum SignalType {
    /// Bullish signal indicating a buy recommendation
    Buy,
    /// Bearish signal indicating a sell recommendation
    Sell,
    /// Neutral signal indicating no strong directional bias
    Neutral,
}