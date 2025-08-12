# ShrivenQuant Documentation

## 🚀 Ultra-Low Latency Trading System

ShrivenQuant is an institutional-grade, ultra-low-latency trading platform designed for Indian index options (via Zerodha) and cryptocurrency markets (via Binance). The platform emphasizes deterministic performance, crash-safety, and rigorous risk management with sub-microsecond decision making and zero-allocation hot paths.

### 📚 Documentation Structure

- **[Architecture Overview](./architecture/README.md)** - System design and component architecture
- **[Developer Guide](./developer-guide/README.md)** - Complete guide for developers
- **[Trader Guide](./trader-guide/README.md)** - Usage guide for traders
- **[API Reference](./api-reference/README.md)** - Detailed API documentation
- **[Deployment Guide](./deployment/README.md)** - Production deployment instructions

### 🎯 Current Status

#### ✅ Completed (Sprint 1-3)
- Core infrastructure with zero-copy, lock-free architecture
- Multi-venue support (Zerodha NSE/BSE, Binance Crypto)
- Full authentication system with TOTP 2FA support
- Real-time WebSocket market data feeds
- Write-Ahead Log (WAL) for data persistence
- Replay engine for historical data
- Ultra-fast limit order book (LOB) implementation (89.9M updates/sec)
- Feed adapters with event bus integration
- Feature extraction (spread, microprice, imbalance)
- Paper trading mode with PnL calculation

#### 🚧 In Progress (Sprint 4+)
- Strategy runtime and paper broker
- Complete live trading integration
- Backtesting framework

#### 📋 Planned (Sprint 4-5)
- Strategy framework
- Advanced analytics
- Production deployment

### 🏎️ Performance Characteristics

- **Tick-to-Decision**: < 100 nanoseconds
- **Order Placement**: < 1 microsecond
- **Market Data Processing**: 1M+ messages/second
- **Memory Usage**: Zero allocations in hot paths
- **Architecture**: Lock-free, cache-aligned data structures

### 🔧 Technology Stack

- **Language**: Rust (nightly for SIMD optimizations)
- **Networking**: Tokio async runtime
- **Data Storage**: Custom WAL, memory-mapped files
- **Message Passing**: Lock-free MPMC/SPSC channels
- **Serialization**: Bincode for speed, JSON for config

### 📖 Quick Links

- [Getting Started](./developer-guide/getting-started.md)
- [System Architecture](./architecture/system-design.md)
- [Component Overview](./architecture/components.md)
- [Trading Strategies](./trader-guide/strategies.md)
- [Performance Tuning](./developer-guide/performance.md)
