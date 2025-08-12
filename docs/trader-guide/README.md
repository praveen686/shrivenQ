# Trader Guide

## Table of Contents

1. [System Overview](#system-overview)
2. [Getting Started](#getting-started)
3. [Trading Modes](#trading-modes)
4. [Market Data](#market-data)
5. [Order Management](#order-management)
6. [Position & PnL Tracking](#position--pnl-tracking)
7. [Risk Management](#risk-management)
8. [Trading Strategies](#trading-strategies)
9. [Performance Monitoring](#performance-monitoring)

## System Overview

ShrivenQuant is an ultra-low latency trading system designed for:
- **Equity Trading**: NSE/BSE markets via Zerodha
- **Crypto Trading**: Spot and futures via Binance
- **Paper Trading**: Risk-free strategy testing
- **Backtesting**: Historical strategy validation

### Key Features
- Sub-microsecond order placement
- Real-time PnL calculation
- Advanced risk management
- Multi-venue support
- Zero-latency paper trading mode

## Getting Started

### 1. Account Setup

#### Zerodha (NSE/BSE)
```bash
#!/bin/bash
# ShrivenQuant Trading Platform - Zerodha Configuration
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Configure Zerodha API credentials for NSE/BSE trading
# USAGE: Run before starting trading system with Zerodha venue
# SAFETY: Never share or commit these credentials!

# Configure Zerodha credentials
export KITE_API_KEY="your_api_key"
export KITE_API_SECRET="your_api_secret"
export KITE_USER_ID="your_user_id"
export KITE_PASSWORD="your_password"
export KITE_PIN="your_pin"
```

#### Binance (Crypto)
```bash
#!/bin/bash
# ShrivenQuant Trading Platform - Binance Configuration
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Configure Binance API credentials for crypto trading
# USAGE: Run before starting trading system with Binance venue
# SAFETY: Use testnet for initial testing!

# Configure Binance credentials
export BINANCE_API_KEY="your_api_key"
export BINANCE_API_SECRET="your_api_secret"
export BINANCE_TESTNET=true  # Use testnet for testing
```

### 2. System Configuration

Create `config.toml`:
```toml
[engine]
mode = "paper"  # paper, live, or backtest
venue = "zerodha"  # zerodha or binance
max_positions = 100
max_orders_per_sec = 100

[risk]
max_position_size = 10000
max_position_value = 10000000  # 1 crore in paise
max_daily_loss = -500000  # 5 lakh loss limit
max_drawdown = -1000000  # 10 lakh drawdown

[data]
symbols = ["NIFTY", "BANKNIFTY", "RELIANCE", "TCS"]
subscribe_depth = true  # 5-level market depth
save_to_wal = true  # Persist market data

[paper_trading]
initial_capital = 1000000  # 10 lakh
commission_rate = 0.0003  # 0.03%
```

### 3. Running the System

```bash
#!/bin/bash
# ShrivenQuant Trading Platform - System Startup Commands
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Command examples for starting trading system in different modes
# USAGE: Choose appropriate mode for your trading strategy
# SAFETY: Always start with paper mode for testing!

# Start in paper trading mode
cargo run --release -- --mode paper

# Start with live trading (use with caution!)
cargo run --release -- --mode live

# Run backtest
cargo run --release -- --mode backtest --from 2024-01-01 --to 2024-12-31

# Monitor system
cargo run --release -- monitor
```

## Trading Modes

### Paper Trading Mode

Perfect for testing strategies without risk:

```rust
// ShrivenQuant Trading Platform - Paper Trading Configuration
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Paper trading mode configuration for risk-free strategy testing
//
// PERFORMANCE: Instant execution < 100ns, no network latency
//
// USAGE: Ideal for strategy development and validation
//
// SAFETY: No real money at risk, perfect for learning and testing

// Configuration
ExecutionMode::Paper

// Features:
- Simulated order fills at market price
- Real-time PnL tracking
- No actual orders sent to exchange
- Instant execution (< 100ns)
- Commission modeling
```

**Use Cases:**
- Strategy development
- Risk-free testing
- Performance validation
- Training and education

### Live Trading Mode

Real market execution:

```rust
// ShrivenQuant Trading Platform - Live Trading Configuration
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Live trading mode for real market execution
//
// PERFORMANCE: Sub-microsecond order placement, real market latency
//
// USAGE: Production trading with actual capital at risk
//
// SAFETY: All risk checks enforced, emergency stop available

// Configuration
ExecutionMode::Live

// Features:
- Direct exchange connectivity
- Real order placement
- Actual position management
- Live PnL tracking
- Risk checks enforced
```

**Safety Features:**
- Pre-trade risk validation
- Position limits
- Daily loss limits
- Emergency stop functionality
- Rate limiting

### Backtest Mode

Historical strategy testing:

```rust
// ShrivenQuant Trading Platform - Backtest Configuration
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Backtesting mode for historical strategy validation
//
// PERFORMANCE: High-speed historical data replay with realistic simulation
//
// USAGE: Strategy optimization and performance validation
//
// SAFETY: Historical simulation with accurate cost modeling

// Configuration
ExecutionMode::Backtest

// Features:
- Historical data replay
- Accurate simulation
- Slippage modeling
- Transaction costs
- Performance metrics
```

**Capabilities:**
- Tick-by-tick replay
- Multiple strategy testing
- Parameter optimization
- Walk-forward analysis

## Market Data

### Real-Time Data Feed

```rust
// ShrivenQuant Trading Platform - Market Data Subscription
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Real-time market data subscription and structure definitions
//
// PERFORMANCE: Nanosecond timestamp precision for accurate tick processing
//
// USAGE: Subscribe to symbols and process real-time market updates

// Subscribe to symbols
feed.subscribe(vec![
    "NIFTY",      // Nifty 50 Index
    "BANKNIFTY",  // Bank Nifty Index
    "RELIANCE",   // Reliance Industries
    "TCS",        // Tata Consultancy Services
]).await?;

// Market data structure
pub struct MarketData {
    pub symbol: Symbol,
    pub bid: Px,         // Best bid price
    pub ask: Px,         // Best ask price
    pub bid_qty: Qty,    // Bid quantity
    pub ask_qty: Qty,    // Ask quantity
    pub last: Px,        // Last traded price
    pub volume: u64,     // Volume traded
    pub timestamp: Ts,   // Nanosecond timestamp
}
```

### Market Depth (5-Level)

```rust
// ShrivenQuant Trading Platform - Market Depth Structure
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: 5-level market depth for advanced order book analysis
//
// USAGE: Deep market analysis and smart order routing

/// 5-level market depth with bid/ask prices and quantities
pub struct MarketDepth {
    pub bids: [(Px, Qty); 5],  // 5 best bids
    pub asks: [(Px, Qty); 5],  // 5 best asks
    pub timestamp: Ts,
}
```

### Historical Data

```bash
#!/bin/bash
# ShrivenQuant Trading Platform - Historical Data Management
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Download and replay historical market data for backtesting
# USAGE: Data preparation for strategy backtesting and analysis

# Download historical data
cargo run --bin download-history --symbol NIFTY --from 2024-01-01

# Replay historical data
cargo run --bin replay --file data/NIFTY_2024.wal
```

## Order Management

### Order Types

```rust
// ShrivenQuant Trading Platform - Order Type Examples
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Different order types for various trading strategies
//
// USAGE: Choose appropriate order type based on execution requirements
//
// SAFETY: All orders subject to pre-trade risk checks

// Market Order - Immediate execution at best price
let order = Order::market(Symbol::NIFTY, Side::Buy, Qty::new(50.0));

// Limit Order - Execute at specified price or better
let order = Order::limit(Symbol::NIFTY, Side::Buy, Qty::new(50.0), Px::new(25000.0));

// Stop Loss Order - Trigger when price crosses level
let order = Order::stop_loss(Symbol::NIFTY, Side::Sell, Qty::new(50.0), Px::new(24500.0));
```

### Order Placement

```rust
// ShrivenQuant Trading Platform - Order Placement Examples
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Order placement, management, and status tracking
//
// PERFORMANCE: Sub-microsecond order placement latency
//
// USAGE: Core trading operations for strategy execution
//
// SAFETY: All orders validated by risk engine before placement

// Place order
let order_id = engine.send_order(
    Symbol::NIFTY,
    Side::Buy,
    Qty::new(50.0),
    Some(Px::new(25000.0))  // None for market order
)?;

// Check order status
let status = engine.get_order_status(order_id)?;

// Cancel order
engine.cancel_order(order_id)?;

// Modify order
engine.modify_order(order_id, new_qty, new_price)?;
```

### Order Status

```rust
// ShrivenQuant Trading Platform - Order Status Types
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Order lifecycle status tracking for execution monitoring
//
// USAGE: Track order progress from placement to completion

/// Complete order lifecycle status enumeration
pub enum OrderStatus {
    New,             // Order accepted
    PartiallyFilled, // Partially executed
    Filled,          // Fully executed
    Cancelled,       // Cancelled by user
    Rejected,        // Rejected by exchange/risk
}
```

## Position & PnL Tracking

### Position Management

```rust
// ShrivenQuant Trading Platform - Position Management Examples
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Position tracking and management for portfolio monitoring
//
// PERFORMANCE: Real-time position updates with atomic operations
//
// USAGE: Monitor positions, calculate PnL, and manage risk
//
// SAFETY: Thread-safe position tracking with lock-free operations

// Get current position
let position = engine.get_position(Symbol::NIFTY)?;

pub struct Position {
    pub symbol: Symbol,
    pub quantity: i64,      // +ve = long, -ve = short
    pub avg_price: f64,     // Average entry price
    pub market_price: f64,  // Current market price
    pub realized_pnl: f64,  // Closed PnL
    pub unrealized_pnl: f64,// Open PnL
}

// Get all positions
let positions = engine.get_all_positions()?;

// Close position
engine.close_position(Symbol::NIFTY)?;
```

### PnL Calculation

```rust
// ShrivenQuant Trading Platform - PnL Calculation Examples
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Real-time and historical profit and loss calculations
//
// PERFORMANCE: Sub-nanosecond PnL updates using atomic operations
//
// USAGE: Continuous PnL monitoring for risk management and reporting
//
// SAFETY: Deterministic calculations with fixed-point arithmetic

// Real-time PnL
let pnl = engine.get_pnl();

pub struct PnL {
    pub realized: f64,    // Booked profit/loss
    pub unrealized: f64,  // Mark-to-market
    pub total: f64,       // Total PnL
    pub timestamp: u64,   // Last update time
}

// Historical PnL
let daily_pnl = engine.get_daily_pnl(date)?;
let monthly_pnl = engine.get_monthly_pnl(month)?;
```

### Performance Metrics

```rust
// ShrivenQuant Trading Platform - Performance Metrics
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Comprehensive trading performance metrics and analytics
//
// PERFORMANCE: Real-time metric calculation using SIMD operations
//
// USAGE: Strategy evaluation and performance monitoring
//
// SAFETY: Thread-safe atomic counters for accurate metrics

let metrics = engine.get_metrics();

pub struct TradingMetrics {
    pub total_trades: u64,
    pub winning_trades: u64,
    pub losing_trades: u64,
    pub win_rate: f64,          // Win percentage
    pub profit_factor: f64,     // Gross profit / Gross loss
    pub sharpe_ratio: f64,      // Risk-adjusted returns
    pub max_drawdown: f64,      // Maximum peak-to-trough
    pub avg_win: f64,           // Average winning trade
    pub avg_loss: f64,          // Average losing trade
}
```

## Risk Management

### Position Limits

```rust
// ShrivenQuant Trading Platform - Risk Management Limits
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Comprehensive risk limits for trading safety
//
// PERFORMANCE: Branch-free risk checks for minimal latency impact
//
// USAGE: Configure risk parameters for safe trading operations
//
// SAFETY: Conservative defaults prevent excessive risk exposure

/// Complete risk management limits structure
pub struct RiskLimits {
    // Position limits
    max_position_size: 10000,        // Max shares per position
    max_position_value: 10_000_000,  // Max value (1 crore)
    max_total_exposure: 50_000_000,  // Total exposure (5 crore)

    // Order limits
    max_order_size: 1000,             // Max shares per order
    max_order_value: 1_000_000,      // Max order value (10 lakh)
    max_orders_per_minute: 100,      // Rate limiting

    // Loss limits
    max_daily_loss: -500_000,        // Daily stop loss (5 lakh)
    max_drawdown: -1_000_000,        // Max drawdown (10 lakh)
}
```

### Risk Checks

Pre-trade validation (< 50 nanoseconds):
1. Position size check
2. Exposure limit check
3. Daily loss check
4. Order rate limiting
5. Price reasonability
6. Market hours validation

### Circuit Breakers

```rust
// ShrivenQuant Trading Platform - Circuit Breaker System
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Automatic trading halt system for risk protection
//
// PERFORMANCE: Instant halt capability with emergency stop
//
// USAGE: Automatic risk protection and manual emergency controls
//
// SAFETY: Multiple trigger conditions for comprehensive protection

// Automatic trading halt triggers:
- Daily loss exceeds limit
- Drawdown exceeds limit
- Unusual market volatility
- Technical errors

// Manual emergency stop
engine.emergency_stop();

// Resume after review
engine.resume_trading();
```

## Trading Strategies

### Strategy Framework

```rust
// ShrivenQuant Trading Platform - Strategy Framework
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Core strategy trait defining the trading strategy interface
//
// PERFORMANCE: Optimized callbacks for minimal latency impact
//
// USAGE: Implement this trait for custom trading strategies
//
// SAFETY: Type-safe strategy interface with error handling

/// Core trading strategy interface with performance-optimized callbacks
pub trait Strategy {
    // Called on every market tick
    fn on_tick(&mut self, tick: &MarketData) -> Option<Signal>;

    // Called on order fill
    fn on_fill(&mut self, fill: &Fill);

    // Risk management
    fn get_position_size(&self, signal: &Signal) -> Qty;
}
```

### Example Strategies

#### 1. Simple Moving Average Cross
```rust
// ShrivenQuant Trading Platform - SMA Cross Strategy
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Simple Moving Average crossover strategy implementation
//
// PERFORMANCE: Efficient SMA calculation with minimal state
//
// USAGE: Classic trend-following strategy for directional trading
//
// SAFETY: Risk-managed position sizing and clear entry/exit rules

/// Simple Moving Average crossover strategy
pub struct SMACross {
    fast_period: usize,  // e.g., 10
    slow_period: usize,  // e.g., 20
    fast_sma: f64,
    slow_sma: f64,
}

impl Strategy for SMACross {
    fn on_tick(&mut self, tick: &MarketData) -> Option<Signal> {
        self.update_sma(tick.last);

        if self.fast_sma > self.slow_sma {
            Some(Signal::Buy)
        } else if self.fast_sma < self.slow_sma {
            Some(Signal::Sell)
        } else {
            None
        }
    }
}
```

#### 2. Market Making
```rust
// ShrivenQuant Trading Platform - Market Making Strategy
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Market making strategy with inventory management
//
// PERFORMANCE: Real-time quote adjustment with position-based skew
//
// USAGE: Provide liquidity while managing inventory risk
//
// SAFETY: Position limits and spread management for risk control

/// Market making strategy with intelligent inventory management
pub struct MarketMaker {
    spread: f64,        // Desired spread
    position_limit: i64, // Max position
    skew: f64,          // Price skew based on position
}

impl Strategy for MarketMaker {
    fn on_tick(&mut self, tick: &MarketData) -> Option<Signal> {
        let mid = (tick.bid + tick.ask) / 2.0;
        let adjusted_mid = mid + (self.position * self.skew);

        Some(Signal::QuoteBoth {
            bid: adjusted_mid - self.spread/2.0,
            ask: adjusted_mid + self.spread/2.0,
        })
    }
}
```

#### 3. Statistical Arbitrage
```rust
// ShrivenQuant Trading Platform - Statistical Arbitrage Strategy
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Statistical arbitrage strategy for pairs trading
//
// PERFORMANCE: Real-time z-score calculation and mean reversion detection
//
// USAGE: Trade statistical relationships between correlated instruments
//
// SAFETY: Z-score thresholds and hedge ratio validation

/// Statistical arbitrage strategy for pairs trading
pub struct StatArb {
    pair: (Symbol, Symbol),  // Trading pair
    hedge_ratio: f64,       // Hedge ratio
    z_score_entry: f64,     // Entry threshold
    z_score_exit: f64,      // Exit threshold
}
```

### Strategy Configuration

```toml
[strategy.sma_cross]
type = "SMACross"
fast_period = 10
slow_period = 20
symbols = ["NIFTY", "BANKNIFTY"]

[strategy.market_maker]
type = "MarketMaker"
spread = 0.0010  # 10 basis points
position_limit = 1000
symbols = ["RELIANCE", "TCS"]
```

## Performance Monitoring

### Real-Time Metrics

```bash
# Terminal dashboard
cargo run --bin monitor

# Metrics displayed:
- Orders/second
- Tick-to-order latency
- Current positions
- Real-time PnL
- Risk utilization
- System health
```

### Latency Monitoring

```rust
// ShrivenQuant Trading Platform - Latency Performance Metrics
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Real-time latency monitoring for system performance tracking
//
// PERFORMANCE: Nanosecond precision timing for accurate latency measurement
//
// USAGE: Monitor system performance and identify bottlenecks
//
// SAFETY: Performance regression detection and alerting

/// Comprehensive latency metrics for system performance monitoring
pub struct PerformanceMetrics {
    pub tick_to_decision: u64,    // Nanoseconds
    pub decision_to_order: u64,   // Nanoseconds
    pub order_to_fill: u64,       // Nanoseconds
    pub total_latency: u64,       // Total end-to-end
}

// Typical latencies:
- Tick processing: < 100ns
- Risk check: < 50ns
- Order placement: < 1μs
- Total: < 2μs
```

### Daily Reports

```bash
# Generate daily report
cargo run --bin daily-report --date 2024-12-31

# Report includes:
- Trading summary
- PnL breakdown
- Risk metrics
- Performance statistics
- Error logs
```

### Alerts & Notifications

```toml
[alerts]
# Telegram notifications
telegram_enabled = true
telegram_token = "your_bot_token"
telegram_chat_id = "your_chat_id"

# Alert triggers
daily_loss_alert = -100000  # 1 lakh
drawdown_alert = -200000    # 2 lakh
error_alert = true
fill_notification = true
```

## Best Practices

### 1. Start with Paper Trading
Always test strategies in paper mode before going live.

### 2. Set Conservative Limits
Begin with small position sizes and tight risk limits.

### 3. Monitor System Health
```bash
# Check system status
cargo run --bin health-check

# Monitor logs
tail -f logs/trading.log
```

### 4. Regular Backups
```bash
# Backup trading data
./scripts/backup.sh

# Backup includes:
- WAL files
- Configuration
- Trading logs
- Performance metrics
```

### 5. Emergency Procedures

```bash
# Emergency stop
cargo run --bin emergency-stop

# Close all positions
cargo run --bin close-all

# Cancel all orders
cargo run --bin cancel-all
```

## Troubleshooting

### Common Issues

1. **Connection Issues**
   ```bash
   # Check connectivity
   cargo run --bin test-connection

   # Reconnect
   cargo run --bin reconnect
   ```

2. **Data Feed Problems**
   ```bash
   # Check data feed
   cargo run --bin check-feed

   # Restart feed
   cargo run --bin restart-feed
   ```

3. **Order Rejections**
   - Check risk limits
   - Verify market hours
   - Confirm sufficient margin
   - Check symbol validity

### Support

For issues or questions:
- GitHub Issues: https://github.com/praveen686/shrivenQ/issues
- Documentation: https://shrivenq.docs
- Email: support@shrivenq.com

## Disclaimer

**IMPORTANT**: Trading in financial markets involves substantial risk of loss and is not suitable for all investors. Past performance is not indicative of future results.

- Always start with paper trading
- Never trade with money you cannot afford to lose
- Thoroughly test all strategies before live deployment
- Monitor positions and risk continuously
- Have an emergency plan ready
