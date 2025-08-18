//! Simple Moving Average Crossover Strategy Example

use backtesting::{
    Strategy, MarketSnapshot, PortfolioState, TradingSignal,
    OrderSide, OrderType, BacktestEngine, BacktestConfig,
    DataFrequency, SlippageModel, OHLCV,
};
use chrono::{Utc, Duration};
use std::collections::VecDeque;

/// Simple Moving Average Crossover Strategy
pub struct MAStrategy {
    symbol: String,
    fast_period: usize,
    slow_period: usize,
    price_history: VecDeque<f64>,
}

impl MAStrategy {
    pub fn new(symbol: String, fast_period: usize, slow_period: usize) -> Self {
        Self {
            symbol,
            fast_period,
            slow_period,
            price_history: VecDeque::with_capacity(slow_period),
        }
    }
    
    fn calculate_ma(&self, period: usize) -> Option<f64> {
        if self.price_history.len() < period {
            return None;
        }
        
        let sum: f64 = self.price_history
            .iter()
            .rev()
            .take(period)
            .sum();
        
        Some(sum / period as f64)
    }
}

impl Strategy for MAStrategy {
    fn generate_signals(&self, market: &MarketSnapshot, portfolio: &PortfolioState) -> Vec<TradingSignal> {
        let mut signals = Vec::new();
        
        // Get current price
        let current_price = match market.prices.get(&self.symbol) {
            Some(price) => *price,
            None => return signals,
        };
        
        // Calculate moving averages
        let fast_ma = match self.calculate_ma(self.fast_period) {
            Some(ma) => ma,
            None => return signals,
        };
        
        let slow_ma = match self.calculate_ma(self.slow_period) {
            Some(ma) => ma,
            None => return signals,
        };
        
        // Check if we have a position
        let has_position = portfolio.positions
            .iter()
            .any(|p| p.symbol == self.symbol && p.quantity.abs() > 0.0);
        
        // Generate signals based on MA crossover
        if fast_ma > slow_ma && !has_position {
            // Buy signal
            let quantity = (portfolio.cash * 0.95) / current_price; // Use 95% of cash
            
            signals.push(TradingSignal {
                symbol: self.symbol.clone(),
                side: OrderSide::Buy,
                order_type: OrderType::Market,
                quantity,
                price: None,
            });
        } else if fast_ma < slow_ma && has_position {
            // Sell signal
            if let Some(position) = portfolio.positions.iter().find(|p| p.symbol == self.symbol) {
                signals.push(TradingSignal {
                    symbol: self.symbol.clone(),
                    side: OrderSide::Sell,
                    order_type: OrderType::Market,
                    quantity: position.quantity,
                    price: None,
                });
            }
        }
        
        signals
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Simple Moving Average Strategy Backtest Example");
    
    // Create backtest configuration
    let config = BacktestConfig {
        start_date: Utc::now() - Duration::days(365),
        end_date: Utc::now(),
        initial_capital: 100000.0,
        commission_rate: 0.001,
        slippage_model: SlippageModel::Fixed { bps: 5.0 },
        data_frequency: DataFrequency::Daily,
        enable_shorting: false,
        margin_requirement: 0.5,
        risk_free_rate: 0.05,
    };
    
    // Create backtest engine
    let engine = BacktestEngine::new(config);
    
    // Generate sample data (in production, load from database)
    let mut sample_data = Vec::new();
    let mut price = 100.0;
    let mut current = Utc::now() - Duration::days(365);
    
    for _ in 0..365 {
        // Random walk for price
        let change = (rand::random::<f64>() - 0.5) * 2.0;
        price = (price + change).max(1.0);
        
        sample_data.push((
            current,
            OHLCV {
                open: price - 0.5,
                high: price + 1.0,
                low: price - 1.0,
                close: price,
                volume: 1000000.0,
            },
        ));
        
        current = current + Duration::days(1);
    }
    
    // Load data
    engine.load_data("TEST", sample_data).await?;
    
    // Create strategy
    let strategy = MAStrategy::new("TEST".to_string(), 10, 30);
    
    // Run backtest
    println!("Running backtest...");
    let result = engine.run(&strategy).await?;
    
    // Print results
    println!("\n=== Backtest Results ===");
    println!("Total Return: {:.2}%", result.metrics.total_return * 100.0);
    println!("Sharpe Ratio: {:.2}", result.metrics.sharpe_ratio);
    println!("Max Drawdown: {:.2}%", result.metrics.max_drawdown * 100.0);
    println!("Total Trades: {}", result.metrics.total_trades);
    println!("Win Rate: {:.2}%", result.metrics.win_rate * 100.0);
    
    Ok(())
}

// Simple random number generator (in production use rand crate)
mod rand {
    use std::sync::atomic::{AtomicU64, Ordering};
    
    static SEED: AtomicU64 = AtomicU64::new(12345);
    
    pub fn random<T>() -> T 
    where 
        T: From<f64>
    {
        let mut x = SEED.load(Ordering::Relaxed);
        x = x.wrapping_mul(1103515245).wrapping_add(12345);
        SEED.store(x, Ordering::Relaxed);
        let val = ((x / 65536) % 1000) as f64 / 1000.0;
        T::from(val)
    }
}