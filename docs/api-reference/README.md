# API Reference

## Table of Contents

1. [Core Types](#core-types)
   - [Symbol](#symbol)
   - [Price (Px)](#price-px)
   - [Quantity (Qty)](#quantity-qty)
   - [Timestamp (Ts)](#timestamp-ts)

2. [Trading Engine](#trading-engine)
   - [Engine Configuration](#engine-configuration)
   - [Engine Core](#engine-core)
   - [Order Management](#order-management)
   - [Position Tracking](#position-tracking)

3. [Risk Management](#risk-management)

4. [Performance Metrics](#performance-metrics)

5. [Event Bus](#event-bus)

6. [Market Data](#market-data)

7. [Venue Adapters](#venue-adapters)

8. [Memory Management](#memory-management)

9. [Performance Targets](#performance-targets)
   - [Latency (Nanoseconds)](#latency-nanoseconds)
   - [Throughput](#throughput)
   - [Memory Usage](#memory-usage)

10. [Error Handling](#error-handling)

11. [Thread Safety](#thread-safety)

12. [Instrument Service](#instrument-service)
    - [Daily Instrument Management](#daily-instrument-management)
    - [Configuration](#configuration)
    - [Features](#features)

---

## Core Types

### Symbol
Unique identifier for trading instruments.

```rust
// ShrivenQuant Trading Platform - Core Symbol Type
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Ultra-fast symbol representation using u32 integers instead of strings
//          for zero-allocation lookups and cache-friendly operations in hot paths.
//
// PERFORMANCE: Zero-cost type safety wrapper around u32, constant-time operations,
//              cache-line friendly (4 bytes vs 24+ bytes for String)
//
// USAGE: Used throughout trading engine for instrument identification,
//        order management, position tracking, and market data processing.
//
// SAFETY: All methods are const-safe and overflow-protected.

/// Core symbol identifier for trading instruments
/// Uses u32 internally for maximum performance and cache efficiency
pub struct Symbol(pub u32);

impl Symbol {
    pub const fn new(id: u32) -> Self
    pub fn from_string(symbol: &str) -> Option<Self>
    pub fn to_string(&self) -> String
}

// Usage Examples - Real Trading Scenarios
let nifty = Symbol::new(256265);  // NSE NIFTY 50 index
let reliance = Symbol::from_string("RELIANCE").unwrap();  // Reliance Industries
```

### Price (Px)
Deterministic price representation with 4 decimal places.

```rust
// ShrivenQuant Trading Platform - Deterministic Price Type
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Fixed-point price representation using i64 to eliminate floating-point
//          precision errors that can cause financial discrepancies in trading systems.
//
// PRECISION: 4 decimal places (1 tick = 0.0001), range ±922,337,203,685.4775,
//            deterministic arithmetic operations (no floating-point drift)
//
// PERFORMANCE: Integer operations only (faster than f64), cache-friendly 8-byte,
//              branch-free comparison and arithmetic operations
//
// COMPLIANCE: Meets financial industry standards for price precision and determinism.
//             Used by major exchanges and algorithmic trading systems.
//
// SAFETY: All operations check for overflow and maintain precision invariants.

/// Deterministic price representation with 4 decimal places
/// Internal storage uses i64 ticks where 1 tick = 0.0001 currency units
pub struct Px(i64);  // Internal: price in ticks (1 tick = 0.0001)

impl Px {
    pub fn new(value: f64) -> Self
    pub const fn from_i64(ticks: i64) -> Self
    pub fn as_f64(&self) -> f64
    pub const fn as_i64(&self) -> i64
    pub const ZERO: Self = Self(0)
}

// Usage Examples - Real Trading Prices
let nifty_price = Px::new(25000.50);  // ₹25,000.50 (NIFTY level)
let option_price = Px::new(127.25);   // ₹127.25 (option premium)
assert_eq!(nifty_price.as_i64(), 250005000);  // Internal tick representation
```

### Quantity (Qty)
Share/contract quantity with 4 decimal precision.

```rust
// ShrivenQuant Trading Platform - Quantity Type
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Fixed-point quantity representation for precise share/contract counting
//          without floating-point precision errors in position calculations.
//
// PRECISION: 4 decimal places (1 unit = 0.0001), supports both whole shares
//            and fractional contracts, range ±922,337,203,685.4775
//
// PERFORMANCE: Integer operations only (faster than f64), zero-copy const ops,
//              cache-friendly 8-byte representation
//
// USAGE: Used throughout system for order quantities, position sizes,
//        fill quantities, and risk calculations.
//
// SAFETY: All operations maintain precision invariants and check for overflow.

/// Fixed-point quantity representation with 4 decimal places
/// Internal storage uses i64 units where 1 unit = 0.0001 shares/contracts
pub struct Qty(i64);  // Internal: quantity in units

impl Qty {
    pub fn new(value: f64) -> Self
    pub const fn from_i64(units: i64) -> Self
    pub fn as_f64(&self) -> f64
    pub const fn raw(&self) -> i64
    pub const fn is_zero(&self) -> bool
    pub const ZERO: Self = Self(0)
}

// Usage Examples - Real Trading Quantities
let qty = Qty::new(100.0);  // 100 shares (equity)
let fractional = Qty::new(0.25);  // 0.25 contracts (crypto)
let option_lot = Qty::new(75.0);  // 75 units (1 NIFTY option lot)
```

### Timestamp (Ts)
High-precision timestamp in nanoseconds.

```rust
// ShrivenQuant Trading Platform - High-Precision Timestamp
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Nanosecond-precision timestamps for ultra-low latency trading where
//          precise timing is critical for order sequencing and latency measurement.
//
// PRECISION: Nanosecond resolution (1e-9 seconds), range 584 years from UNIX epoch,
//            monotonic time source for consistent ordering
//
// PERFORMANCE: Single u64 for cache efficiency, const operations for zero-cost,
//              direct hardware timestamp counter access
//
// USAGE: Critical for order timestamps, fill timestamps, latency measurement,
//        event sequencing, and performance monitoring in trading systems.
//
// COMPLIANCE: Meets regulatory requirements for trade reporting and audit trails.

/// High-precision timestamp with nanosecond resolution
/// Stores nanoseconds since UNIX epoch for maximum precision and range
pub struct Ts(pub u64);  // Nanoseconds since UNIX epoch

impl Ts {
    pub fn now() -> Self
    pub const fn from_nanos(nanos: u64) -> Self
    pub const fn nanos(&self) -> u64
    pub const fn as_micros(&self) -> u64
    pub const fn as_millis(&self) -> u64
}

// Usage Examples - Real Trading Timestamps
let now = Ts::now();
let market_open = Ts::from_nanos(1640995200000000000);  // 2022-01-01 00:00:00 UTC
let order_time = Ts::now();  // Precise order placement timestamp
```

## Trading Engine

### Engine Configuration

```rust
// ShrivenQuant Trading Platform - Engine Configuration
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Central configuration structure for the trading engine that controls
//          execution mode, venue selection, performance limits, and safety features.
//
// SAFETY: Risk checks enabled by default for safety, conservative defaults prevent
//         accidental high-frequency trading, paper mode default prevents accidental live trading
//
// PERFORMANCE: Pre-allocated memory pools for zero-allocation hot paths,
//              configurable rate limiting to prevent broker API abuse,
//              memory pool sizing for optimal cache utilization
//
// USAGE: Loaded at startup from configuration files or environment variables.
//        Changes require engine restart for thread safety.

/// Central configuration for the trading engine with safety-first defaults
pub struct EngineConfig {
    pub mode: ExecutionMode,        // Paper, Live, or Backtest
    pub venue: VenueType,          // Zerodha or Binance
    pub max_positions: usize,      // Maximum open positions
    pub max_orders_per_sec: u32,   // Rate limiting
    pub risk_check_enabled: bool,  // Enable pre-trade risk checks
    pub metrics_enabled: bool,     // Enable performance metrics
    pub memory_pool_size: usize,   // Memory pool size in bytes
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            mode: ExecutionMode::Paper,
            venue: VenueType::Zerodha,
            max_positions: 1000,
            max_orders_per_sec: 1000,
            risk_check_enabled: true,
            metrics_enabled: true,
            memory_pool_size: 1024 * 1024,  // 1MB
        }
    }
}
```

### Engine Core

```rust
// ShrivenQuant Trading Platform - Core Trading Engine
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: The heart of the ShrivenQuant trading system - a lock-free, ultra-low
//          latency engine that processes market data, executes trades, manages
//          positions, and enforces risk limits in sub-microsecond timeframes.
//
// ARCHITECTURE: Lock-free design using atomic operations throughout, generic over
//               venue adapters for multi-broker support, event-driven with zero-copy
//               message passing, pre-allocated memory pools to avoid runtime allocation
//
// PERFORMANCE: Market tick processing < 100ns (hot path), order placement < 1μs (critical path),
//              fill processing < 500ns (critical path), risk validation < 50ns (branch-free),
//              position updates using lock-free atomic operations
//
// SAFETY: No panics in production code paths, all operations return Result types,
//         risk engine enforces pre-trade checks, emergency stop capability
//
// USAGE: Central component coordinating all trading activities across venues.
//        Designed for 24/7 operation with microsecond-level precision.

/// Core trading engine with sub-microsecond latency guarantees
pub struct Engine<V: VenueAdapter> {
    // Core components
    config: Arc<EngineConfig>,
    venue: V,
    execution: ExecutionLayer<V>,
    positions: PositionTracker,
    metrics: MetricsEngine,
    risk: RiskEngine,
    bus: Arc<EventBus>,

    // Performance counters (atomic)
    order_counter: AtomicU64,
    fill_counter: AtomicU64,
    reject_counter: AtomicU64,

    // Latency tracking (nanoseconds)
    tick_to_decision: AtomicU64,
    decision_to_order: AtomicU64,
    order_to_fill: AtomicU64,
}

impl<V: VenueAdapter> Engine<V> {
    /// Create new engine instance
    pub fn new(config: EngineConfig, venue: V, bus: Arc<EventBus>) -> Self

    /// Process market tick (ULTRA HOT PATH - < 100ns)
    #[inline(always)]
    pub fn on_tick(&self, symbol: Symbol, bid: Px, ask: Px, ts: Ts)

    /// Send order (CRITICAL PATH - < 1μs)
    #[inline(always)]
    pub fn send_order(
        &self,
        symbol: Symbol,
        side: Side,
        qty: Qty,
        price: Option<Px>,
    ) -> Result<OrderId, OrderError>

    /// Process fill (CRITICAL PATH - < 500ns)
    #[inline(always)]
    pub fn on_fill(&self, order_id: u64, fill_qty: Qty, fill_price: Px, ts: Ts)

    /// Get current PnL (lock-free read)
    #[inline(always)]
    pub fn get_pnl(&self) -> PnL

    /// Get performance metrics
    pub fn get_performance(&self) -> PerformanceMetrics
}
```

### Order Management

```rust
// ShrivenQuant Trading Platform - Order Structure
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Cache-aligned order structure optimized for lock-free updates in
//          high-frequency trading environments. Uses atomic operations for
//          thread-safe status and fill quantity tracking.
//
// PERFORMANCE: 64-byte cache line alignment for optimal CPU cache usage,
//              atomic fields for lock-free concurrent access, packed bit
//              representation for side and order type, zero-copy design
//
// USAGE: Created for every trading order, maintained in order pools,
//        updated atomically during execution, and tracked for compliance.
//
// SAFETY: status and filled_qty are atomic for concurrent updates,
//         immutable fields after creation for race-free access

/// Cache-aligned order structure for high-frequency trading
pub struct Order {
    pub id: u64,
    pub symbol: Symbol,
    pub side: u8,              // 0=Buy, 1=Sell
    pub order_type: u8,        // 0=Market, 1=Limit
    pub status: AtomicU8,      // Order status (atomic)
    pub quantity: Qty,
    pub filled_qty: AtomicU64, // Filled quantity (atomic)
    pub price: u64,            // 0 for market orders
    pub timestamp: u64,        // Creation timestamp
    pub venue_id: u64,         // Exchange order ID
}

pub enum OrderStatus {
    New = 0,
    PartiallyFilled = 1,
    Filled = 2,
    Cancelled = 3,
    Rejected = 4,
}

pub enum Side {
    Buy = 0,
    Sell = 1,
}

pub enum OrderError {
    RiskRejected = 0,
    RateLimited = 1,
    InvalidPrice = 2,
    InvalidQuantity = 3,
    VenueError = 4,
}
```

### Position Tracking

```rust
// ShrivenQuant Trading Platform - Position Tracking System
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Lock-free position tracking with atomic updates for high-frequency
//          trading systems requiring real-time PnL calculations.
//
// PERFORMANCE: All operations are lock-free using atomic fields, zero-allocation
//              updates, cache-friendly 64-byte alignment for optimal CPU performance
//
// USAGE: Maintains real-time position sizes, average prices, and PnL calculations
//        for all trading instruments across multiple venues.
//
// SAFETY: Thread-safe atomic operations prevent race conditions in concurrent
//         trading environments, consistent PnL calculations under high load

/// Lock-free position tracking with atomic PnL calculations
pub struct Position {
    pub symbol: Symbol,
    pub quantity: AtomicI64,      // +ve = long, -ve = short
    pub avg_price: AtomicU64,     // Average price (atomic)
    pub realized_pnl: AtomicI64,  // Realized PnL (atomic)
    pub unrealized_pnl: AtomicI64,// Unrealized PnL (atomic)
    pub last_update: AtomicU64,   // Last update timestamp
    pub last_bid: AtomicU64,      // Last bid price
    pub last_ask: AtomicU64,      // Last ask price
}

impl Position {
    /// Create new position
    pub fn new(symbol: Symbol) -> Self

    /// Apply fill - LOCK-FREE
    #[inline(always)]
    pub fn apply_fill(&self, side: u8, qty: Qty, price: Px, ts: Ts)

    /// Update market prices - LOCK-FREE
    #[inline(always)]
    pub fn update_market(&self, bid: Px, ask: Px, ts: Ts)

    /// Get total PnL
    #[inline(always)]
    pub fn total_pnl(&self) -> i64
}

pub struct PositionTracker {
    positions: DashMap<Symbol, Position>,
    pending_orders: DashMap<u64, (Symbol, u8, Qty)>,
    total_realized: AtomicI64,
    total_unrealized: AtomicI64,
}

impl PositionTracker {
    /// Create new tracker
    pub fn new(capacity: usize) -> Self

    /// Add pending order
    pub fn add_pending(&self, order_id: u64, symbol: Symbol, side: u8, qty: Qty)

    /// Apply fill to position
    pub fn apply_fill(&self, order_id: u64, fill_qty: Qty, fill_price: Px, ts: Ts)

    /// Update market prices for all positions
    pub fn update_market(&self, symbol: Symbol, bid: Px, ask: Px, ts: Ts)

    /// Get global PnL (realized, unrealized, total)
    pub fn get_global_pnl(&self) -> (i64, i64, i64)
}
```

## Risk Management

```rust
// ShrivenQuant Trading Platform - Risk Management System
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Comprehensive risk management system with real-time limit checking
//          and emergency stop capabilities for trading safety.
//
// PERFORMANCE: Branch-free risk checks using bit operations, pre-computed limits
//              for sub-50ns validation in hot trading paths
//
// USAGE: Applied to every order before execution, monitors portfolio exposure,
//        enforces position limits, and provides emergency stop functionality.
//
// SAFETY: Conservative defaults, fail-safe behavior, automatic position limits
//         enforcement, real-time drawdown monitoring

/// Comprehensive risk limits for trading safety
pub struct RiskLimits {
    // Position limits
    pub max_position_size: u64,     // Max shares per position
    pub max_position_value: u64,    // Max position value
    pub max_total_exposure: u64,    // Total portfolio exposure

    // Order limits
    pub max_order_size: u64,        // Max shares per order
    pub max_order_value: u64,       // Max order value
    pub max_orders_per_minute: u32, // Rate limiting

    // Loss limits
    pub max_daily_loss: i64,        // Daily stop loss
    pub max_drawdown: i64,          // Maximum drawdown
}

pub struct RiskEngine {
    config: Arc<EngineConfig>,
    limits: RiskLimits,
    symbol_risks: DashMap<Symbol, SymbolRisk>,
    total_exposure: AtomicU64,
    daily_pnl: AtomicI64,
    emergency_stop: AtomicU64,      // Emergency stop flag
}

impl RiskEngine {
    /// Create new risk engine
    pub fn new(config: Arc<EngineConfig>) -> Self

    /// Check if order passes risk checks - BRANCH-FREE HOT PATH
    #[inline(always)]
    pub fn check_order(
        &self,
        symbol: Symbol,
        side: Side,
        qty: Qty,
        price: Option<Px>,
    ) -> bool

    /// Update position risk after fill
    pub fn update_position(&self, symbol: Symbol, side: Side, qty: Qty, price: Px)

    /// Update PnL for risk tracking
    pub fn update_pnl(&self, pnl: i64)

    /// Emergency stop
    pub fn emergency_stop(&self)

    /// Resume trading
    pub fn resume(&self)
}
```

## Performance Metrics

```rust
// ShrivenQuant Trading Platform - Performance Metrics Engine
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Real-time trading performance analytics with atomic counters for
//          accurate metrics collection without impacting trading latency.
//
// PERFORMANCE: Atomic operations for lock-free updates, SIMD-accelerated
//              calculations for Sharpe ratio and other complex metrics
//
// USAGE: Continuous monitoring of trading performance, risk metrics,
//        and profitability analysis for strategy optimization.
//
// SAFETY: Thread-safe atomic counters prevent data races, overflow-protected
//         calculations, consistent metric snapshots

/// High-performance trading metrics with atomic counters
pub struct MetricsEngine {
    // Trading metrics (atomic counters)
    total_trades: AtomicU64,
    winning_trades: AtomicU64,
    losing_trades: AtomicU64,

    // Volume metrics
    total_volume: AtomicU64,
    buy_volume: AtomicU64,
    sell_volume: AtomicU64,

    // PnL metrics
    gross_profit: AtomicI64,
    gross_loss: AtomicI64,
    max_drawdown: AtomicI64,
    peak_equity: AtomicI64,

    // Performance metrics
    sharpe_ratio: AtomicU64,
    win_rate: AtomicU64,
    profit_factor: AtomicU64,
}

impl MetricsEngine {
    /// Create new metrics engine
    pub fn new() -> Self

    /// Update market data
    pub fn update_market(&self, symbol: Symbol, bid: Px, ask: Px, ts: Ts)

    /// Record fill and update metrics
    pub fn record_fill(&self, order_id: u64, qty: Qty, price: Px, ts: Ts)

    /// Calculate PnL using position tracker
    pub fn calculate_pnl(&self, positions: &PositionTracker) -> PnL

    /// Calculate Sharpe ratio using SIMD
    pub fn calculate_sharpe(&self) -> f64

    /// Get comprehensive metrics
    pub fn get_metrics(&self) -> TradingMetrics
}

pub struct TradingMetrics {
    pub total_trades: u64,
    pub winning_trades: u64,
    pub losing_trades: u64,
    pub win_rate: f64,          // Win percentage
    pub profit_factor: f64,     // Gross profit / Gross loss
    pub sharpe_ratio: f64,      // Risk-adjusted returns
    pub max_drawdown: i64,      // Maximum drawdown
    pub total_volume: u64,      // Total volume traded
}
```

## Event Bus

```rust
// ShrivenQuant Trading Platform - High-Performance Event Bus
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Ultra-fast message passing system for zero-copy event distribution
//          between trading components using lock-free MPMC channels.
//
// PERFORMANCE: Lock-free MPMC (Multi-Producer Multi-Consumer) design,
//              zero-copy message passing for hot trading events,
//              sub-microsecond event propagation latency, bounded channels
//
// USAGE: Central nervous system of the trading engine, routing market data,
//        order updates, fills, and system events between all components.
//
// SAFETY: Bounded channels with backpressure handling, type-safe message
//         contracts, no message loss in normal operation

/// Ultra-fast lock-free event bus for trading components
pub trait Message: Send + Sync + 'static {}

pub struct EventBus {
    tx: channel::Sender<Event>,
    rx: channel::Receiver<Event>,
}

impl EventBus {
    /// Create new event bus
    pub fn new(capacity: usize) -> Self

    /// Send event
    pub fn send(&self, event: Event) -> Result<()>

    /// Receive event
    pub fn recv(&self) -> Result<Event>
}

pub enum Event {
    /// Market data event
    MarketData {
        symbol: u32,
        bid: i64,
        ask: i64,
        ts: u64
    },
    /// Order event
    Order {
        id: u64,
        symbol: u32,
        side: u8,
        qty: i64
    },
    /// Fill event
    Fill {
        order_id: u64,
        qty: i64,
        price: i64,
        ts: u64
    },
}

// SPSC Channel for high-performance communication
pub struct SpscChannel;

impl SpscChannel {
    /// Create bounded SPSC channel
    pub fn new<T: Send + 'static>(capacity: usize) -> (Sender<T>, Receiver<T>)

    /// Create unbounded SPSC channel
    pub fn unbounded<T: Send + 'static>() -> (Sender<T>, Receiver<T>)
}
```

## Market Data

```rust
// ShrivenQuant Trading Platform - Market Data Structures
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Highly optimized market data structures for real-time tick processing,
//          order book management, and trading decision making with nanosecond
//          precision timestamps.
//
// PERFORMANCE: Cache-aligned structures for optimal memory access, packed data
//              layout to minimize cache misses, zero-allocation design for hot paths,
//              fixed-size arrays for predictable memory usage
//
// ACCURACY: Nanosecond timestamp precision for exact event ordering,
//           fixed-point price representation prevents rounding errors,
//           atomic quantity tracking for thread-safe updates
//
// USAGE: Core data structures for market data feeds, order book maintenance,
//        and trading signal generation across all supported venues.

/// Optimized market data structures with nanosecond precision
pub struct MarketData {
    pub symbol: Symbol,
    pub bid: Px,              // Best bid price
    pub ask: Px,              // Best ask price
    pub bid_qty: Qty,         // Bid quantity
    pub ask_qty: Qty,         // Ask quantity
    pub last: Px,             // Last traded price
    pub volume: u64,          // Volume traded
    pub timestamp: Ts,        // Market timestamp
}

pub struct MarketDepth {
    pub symbol: Symbol,
    pub bids: [(Px, Qty); 5], // 5 best bids
    pub asks: [(Px, Qty); 5], // 5 best asks
    pub timestamp: Ts,
}

pub struct OrderBookUpdate {
    pub symbol: Symbol,
    pub side: Side,           // Bid or Ask
    pub price: Px,            // Price level
    pub qty: Qty,             // New quantity (0 = remove)
    pub level: u8,            // Depth level (0-4)
}
```

## Venue Adapters

```rust
// ShrivenQuant Trading Platform - Venue Adapter System
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Abstraction layer for multiple trading venues (Zerodha, Binance) with
//          unified API while preserving venue-specific optimizations and features.
//
// ARCHITECTURE: Generic trait for venue independence, async-aware for I/O-bound
//               operations, clone-safe for multi-threaded usage, error codes
//               optimized for branch-free processing
//
// PERFORMANCE: Venue-specific latency tracking and optimization, symbol mapping
//              caching for fast lookups, connection pooling and keep-alive management,
//              batch operations where supported
//
// USAGE: Enables seamless multi-venue trading with consistent APIs
//        while maximizing performance for each specific exchange.
//
// SAFETY: Automatic reconnection and failover, order state synchronization,
//         market hours validation, rate limiting compliance

/// Multi-venue trading adapter with unified API
pub trait VenueAdapter: Send + Sync + Clone + 'static {
    /// Send order to venue
    fn send_order(&self, symbol: Symbol, side: u8, qty: Qty, price: Option<Px>)
        -> Result<u64, u8>;

    /// Cancel order
    fn cancel_order(&self, order_id: u64) -> Result<(), u8>;

    /// Get venue-specific symbol mapping
    fn map_symbol(&self, symbol: Symbol) -> u32;

    /// Check if market is open
    fn is_market_open(&self) -> bool;

    /// Get venue latency estimate (nanoseconds)
    fn get_latency_ns(&self) -> u64;
}

pub struct ZerodhaAdapter {
    auth: Arc<auth::ZerodhaAuth>,
    symbol_map: Arc<DashMap<Symbol, u32>>,
    market_open_hour: u8,     // 9 (IST)
    market_close_hour: u8,    // 15 (IST)
    avg_latency_ns: AtomicU64,
}

pub struct BinanceAdapter {
    auth: Arc<auth::BinanceAuth>,
    symbol_map: Arc<DashMap<Symbol, String>>,
    testnet: bool,
    avg_latency_ns: AtomicU64,
}
```

## Memory Management

```rust
// ShrivenQuant Trading Platform - Lock-Free Memory Management
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Ultra-fast lock-free object pools and ring buffers for zero-allocation
//          hot path operations in high-frequency trading systems.
//
// PERFORMANCE: Lock-free acquire/release operations using atomic pointers,
//              pre-allocated storage eliminates runtime allocation, cache-aligned
//              memory layout for optimal CPU performance, O(1) constant-time operations
//
// SAFETY: Memory safety through careful use of UnsafeCell and MaybeUninit,
//         ABA problem prevention using tagged pointers, double-free protection
//         through debug assertions, overflow protection for pool capacity
//
// USAGE: Critical for maintaining zero-allocation guarantees in trading hot paths,
//        particularly for order objects, market data, and temporary calculations.
//
// COMPLIANCE: Enables deterministic latency required for regulatory compliance.

/// Lock-free object pool for zero-allocation
pub struct ObjectPool<T> {
    storage: Box<[UnsafeCell<MaybeUninit<T>>]>,
    free_list: AtomicPtr<FreeNode>,
    allocated: AtomicUsize,
}

impl<T> ObjectPool<T> {
    /// Create pool with pre-allocated objects
    pub fn new(capacity: usize) -> Self

    /// Acquire object from pool - lock-free
    pub fn acquire(&self) -> Option<&mut T>

    /// Release object back to pool - lock-free
    pub fn release(&self, obj: &mut T)

    /// Get number of allocated objects
    pub fn allocated(&self) -> usize
}

/// Ring buffer for lock-free communication
pub struct RingBuffer<T, const N: usize> {
    buffer: [UnsafeCell<MaybeUninit<T>>; N],
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl<T, const N: usize> RingBuffer<T, N> {
    /// Create new ring buffer
    pub fn new() -> Self

    /// Push to ring buffer - single producer
    pub fn push(&self, value: T) -> bool

    /// Pop from ring buffer - single consumer
    pub fn pop(&self) -> Option<T>

    /// Check if empty
    pub fn is_empty(&self) -> bool
}
```

## Performance Targets

### Latency (Nanoseconds)
- **Tick Processing**: < 100ns
- **Risk Check**: < 50ns
- **Order Creation**: < 200ns
- **Position Update**: < 100ns
- **PnL Calculation**: < 50ns
- **Event Propagation**: < 100ns

### Throughput
- **Market Data**: 1M+ ticks/second
- **Order Processing**: 500k orders/second
- **Position Updates**: 1M+ updates/second
- **Event Bus**: 10M+ events/second

### Memory Usage
- **Hot Path**: 0 allocations
- **Order Pool**: Pre-allocated (configurable)
- **Position Cache**: Cache-aligned (64 bytes)
- **Event Buffer**: Ring buffer (lock-free)

## Error Handling

All functions use `Result<T, E>` types for explicit error handling. No panics are allowed in production code paths.

```rust
// ShrivenQuant Trading Platform - Error Handling System
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Comprehensive error handling for trading operations with specific
//          error codes for different failure modes.
//
// PERFORMANCE: Error codes designed for branch-free processing, minimal
//              allocation for error paths, fast error propagation
//
// USAGE: All trading operations return Result types, no panics in production
//        code paths, explicit error handling throughout the system.
//
// SAFETY: Fail-safe error handling, no silent failures, comprehensive
//         error reporting for audit and debugging

/// Common error types for trading operations
// Common error types
pub enum EngineError {
    OrderRejected(OrderError),
    RiskViolation(String),
    VenueError(String),
    InternalError(String),
}

pub enum OrderError {
    InvalidSymbol,
    InvalidPrice,
    InvalidQuantity,
    MarketClosed,
    InsufficientMargin,
    RiskLimitsExceeded,
}
```

## Thread Safety

All shared data structures use atomic operations or lock-free algorithms:
- `AtomicU64`, `AtomicI64` for counters and metrics
- `DashMap` for concurrent hash maps
- `crossbeam::channel` for message passing
- Memory ordering: `Relaxed` for counters, `Acquire`/`Release` for synchronization

## Instrument Service

### Daily Instrument Management
```bash
#!/bin/bash
# ShrivenQuant Trading Platform - Instrument Service Commands
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Daily instrument data management for trading operations
# USAGE: Run daily at 8 AM IST for latest instrument updates

# Fetch latest instruments (run daily at 8 AM IST)
cargo run --bin instrument-service fetch

# Show cached instruments
cargo run --bin instrument-service list --segment NFO --limit 10

# Find specific instrument
cargo run --bin instrument-service search "NIFTY" --type OPTION
```

### Configuration
```rust
// ShrivenQuant Trading Platform - Instrument Service Configuration
//
// Copyright © 2025 Praveen Ayyasola. All rights reserved.
// License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
//
// PURPOSE: Configuration structures for instrument data management service
//          with caching, retry logic, and automated updates.
//
// PERFORMANCE: Efficient caching with JSON storage, fast lookups using
//              multiple indices, optimized for high-frequency access
//
// USAGE: Maintains up-to-date instrument reference data for trading operations
//        across multiple exchanges and segments.
//
// SAFETY: Robust retry logic, timeout protection, data validation

/// Configuration for instrument service with caching and updates
pub struct InstrumentConfig {
    pub cache_dir: PathBuf,          // ./cache/instruments/
    pub update_hour: u8,             // 8 (AM IST)
    pub retry_attempts: u8,          // 3
    pub timeout_seconds: u64,        // 30
}

pub struct Instrument {
    pub token: u32,                  // Exchange token
    pub symbol: String,              // Trading symbol
    pub name: String,                // Display name
    pub expiry: Option<String>,      // Expiry date (YYYY-MM-DD)
    pub strike: Option<f64>,         // Strike price
    pub lot_size: u32,               // Minimum trading quantity
    pub tick_size: f64,              // Minimum price increment
    pub segment: String,             // NSE, BSE, NFO, etc.
    pub instrument_type: String,     // EQ, FUT, CE, PE, etc.
}
```

### Features
- **Daily automatic updates** at 8:00 AM IST
- **Multi-venue support** (Zerodha, Binance, extensible)
- **Persistent caching** with JSON storage
- **Efficient lookups** with multiple indices
- **Retry logic** with exponential backoff
