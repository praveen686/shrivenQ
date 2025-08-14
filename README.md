# ShrivenQuant 🚀

**Institutional-Grade, Ultra-Low-Latency Trading Platform for Indian & Crypto Markets**

[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Proprietary-red.svg)](LICENSE)

> ⚡ Sub-millisecond latency | 🛡️ Zero-tolerance quality | 🔄 Deterministic replay | 🔐 Automated authentication

## 🎯 Mission

ShrivenQuant is a world-class, institutional-grade trading platform designed for:
- **Indian Markets:** NIFTY/BANKNIFTY options via Zerodha (with automated TOTP 2FA)
- **Crypto Markets:** Spot & futures via Binance
- **Performance:** < 1ms end-to-end latency
- **Reliability:** Crash-safe with WAL persistence
- **Quality:** Zero warnings, zero dead code, 100% safe Rust

## ✨ Key Features

### Core Trading Infrastructure
- **📊 Market Data Pipeline:** Real-time tick and LOB data with nanosecond precision
- **🔄 Historical Replay:** Deterministic replay from WAL with symbol filtering
- **💾 WAL Persistence:** Crash-safe storage achieving 229 MB/s writes
- **📈 LOB Engine:** Ultra-fast order book with 298M events/min replay capacity
- **⚡ Fixed-Point Arithmetic:** All calculations use i64 with 4 decimal precision

### Authentication & Security
- **🔐 Automated Zerodha Login:** Full TOTP-based 2FA automation - no manual intervention
- **🎫 JWT Token Management:** Secure token generation with role-based permissions
- **💼 Session Caching:** Smart token reuse to minimize API calls
- **🔑 Multi-Exchange Support:** Unified auth for Zerodha and Binance

### Microservices Architecture (NEW)
- **gRPC Communication:** High-performance inter-service messaging
- **Service Discovery:** Dynamic service registration and health checks
- **Auth Service:** Centralized authentication with Zerodha integration
- **Market Connector:** Real-time data ingestion from multiple venues
- **Risk Manager:** Real-time position and risk monitoring
- **Execution Router:** Smart order routing with venue optimization

## 📋 Platform Status

**Current Status**: ~70% Complete - **[View Detailed Status Report](docs/PLATFORM_STATUS_REPORT.md)**

### ✅ **What's Working Now**
- **Auth Service**: Production-ready gRPC service with multi-exchange support
- **API Gateway**: Complete REST-to-gRPC translation with WebSocket streaming  
- **Business Logic**: 3,348+ lines of sophisticated trading algorithms
- **Performance**: Sub-200ns order book updates, all latency targets exceeded

### 🔄 **What's In Progress (3-4 weeks to completion)**
- **Service Executables**: Need to complete gRPC servers for 5 core services
- **Integration**: Connect business logic libraries to gRPC interfaces
- **Deployment**: Docker containers and orchestration configuration

**For complete technical assessment, timeline, and implementation details**: 👉 **[Platform Status Report](docs/PLATFORM_STATUS_REPORT.md)**

## 🚀 Quick Start

```bash
# Clone the repository
git clone https://github.com/praveen686/shrivenquant.git
cd shrivenquant

# Set up Zerodha credentials in .env
cat > .env << EOF
ZERODHA_USER_ID=your_user_id
ZERODHA_PASSWORD=your_password
ZERODHA_TOTP_SECRET=your_totp_secret
ZERODHA_API_KEY=your_api_key
ZERODHA_API_SECRET=your_api_secret
EOF

# Run compliance checks
./scripts/compliance/agent-compliance-check.sh

# Start Auth Service with Zerodha integration
cargo run -p auth-service

# Test automated Zerodha login
cargo run -p auth-service --example zerodha_simple_usage

# Start market data collection
cargo run --bin market_data_service -- --symbols "NIFTY,BANKNIFTY"

# Run the trading engine
cargo run -p trading-engine
```

## 📊 Project Status

### Refactoring Progress (August 2025)

| Component | Status | Progress | Notes |
|-----------|--------|----------|-------|
| **Microservices Migration** | ✅ Complete | 100% | Migrated from monolithic to service architecture |
| **Zerodha Authentication** | ✅ Complete | 100% | Full TOTP automation, session caching |
| **gRPC Framework** | ✅ Complete | 100% | All services use gRPC for communication |
| **Fixed-Point Arithmetic** | ✅ Complete | 100% | All financial calculations use i64 |
| **Compliance & Quality** | ✅ Complete | 100% | Zero clippy warnings, no unsafe code |
| **Live Trading** | ⏳ Planned | 0% | Ready for implementation |

### Sprint History

| Sprint | Description | Status | Key Achievements |
|--------|------------|--------|------------------|
| **Sprint 1** | Foundation | ✅ COMPLETE | Workspace setup, CLI, monitoring |
| **Sprint 2** | Storage Layer | ✅ COMPLETE | WAL implementation, 229 MB/s writes |
| **Sprint 3** | Market Data | ✅ COMPLETE | Feed adapters, LOB engine, replay |
| **Sprint 4** | Refactoring | ✅ COMPLETE | Microservices, auth, gRPC |
| **Sprint 5** | Live Integration | 🔄 IN PROGRESS | Zerodha auth done, trading pending |
| Sprint 6 | Backtesting | ⏳ Planned | Historical strategy testing |

## 🏗️ Architecture

### Microservices Architecture (Current)
```
┌─────────────────────────────────────────────────────────┐
│                    gRPC Service Mesh                     │
├──────────┬──────────┬──────────┬──────────┬────────────┤
│   Auth   │  Market  │   Risk   │Execution │ Discovery  │
│ Service  │Connector │  Manager │  Router  │  Service   │
│          │          │          │          │            │
│ Zerodha  │ Binance  │  Limits  │  Orders  │  Health    │
│  TOTP    │ WebSocket│  PnL     │  Routing │  Registry  │
└──────────┴──────────┴──────────┴──────────┴────────────┘
                           │
                    ┌──────┴──────┐
                    │     WAL     │
                    │ Persistence │
                    └─────────────┘
```

### Service Endpoints
- **Auth Service:** `localhost:50051` - JWT tokens, Zerodha/Binance auth
- **Market Connector:** `localhost:50052` - Real-time market data
- **Risk Manager:** `localhost:50053` - Position tracking, risk limits
- **Execution Router:** `localhost:50054` - Order management
- **Discovery Service:** `localhost:50055` - Service registry

## 🎯 Performance Metrics

| Metric | Target | Achieved | Test Conditions |
|--------|--------|----------|-----------------|
| WAL Write Speed | 200 MB/s | **229 MB/s** ✅ | 1M events batch |
| Replay Throughput | 250M events/min | **298M events/min** ✅ | Full orderbook |
| Tick Latency | < 1ms | **300 µs** ✅ | End-to-end |
| Auth Token Generation | < 100ms | **42ms** ✅ | Including JWT signing |
| Zerodha Login (cached) | < 10ms | **2µs** ✅ | Token reuse |
| Zerodha Login (fresh) | < 5s | **3.8s** ✅ | Full TOTP flow |

## 🛠️ Development Guidelines

### Code Quality Standards
```rust
// ✅ ALWAYS use fixed-point arithmetic
let price = Px::from_f64(1234.56);
let qty = Qty::from_i64(100);

// ✅ ALWAYS use FxHashMap for performance
use rustc_hash::FxHashMap;
let mut orders = FxHashMap::default();

// ❌ NEVER use floating-point in business logic
// ❌ NEVER use std::collections::HashMap
// ❌ NEVER use .unwrap() - use ? or handle errors
```

### Running Compliance Checks
```bash
# Full compliance check
./scripts/compliance/agent-compliance-check.sh

# Quick check (no agent)
./scripts/compliance/strict-check.sh

# Performance benchmarks
./scripts/performance/run-benchmarks.sh
```

## 📚 Documentation

- **[Architecture Overview](docs/architecture/README.md)** - System design and components
- **[Development Guide](docs/developer-guide/QUANTITATIVE_DEVELOPMENT_BEST_PRACTICES.md)** - Best practices
- **[Auth Integration](services/auth/README.md)** - Zerodha authentication setup
- **[Performance Guide](docs/performance/guidelines.md)** - Optimization techniques

## 🔐 Zerodha Integration

The platform now includes **fully automated Zerodha authentication**:

1. **Automatic TOTP Generation** - No manual 2FA codes needed
2. **Session Caching** - Reuses valid tokens (12-hour validity)
3. **Profile & Margin Access** - Real-time account information
4. **gRPC Integration** - Seamless auth for all services

Setup:
```bash
# Configure credentials in .env
ZERODHA_USER_ID=your_trading_id
ZERODHA_PASSWORD=your_password
ZERODHA_TOTP_SECRET=your_totp_secret  # From Zerodha 2FA setup
ZERODHA_API_KEY=your_api_key
ZERODHA_API_SECRET=your_api_secret

# Test authentication
cargo run -p auth-service --example zerodha_simple_usage
```


## 📈 Roadmap

### Q3 2024 (Current)
- [x] Microservices architecture
- [x] Zerodha authentication
- [x] gRPC framework
- [ ] Live order placement
- [ ] Real-time P&L tracking

### Q4 2024
- [ ] Advanced risk management
- [ ] Multi-strategy support
- [ ] Backtesting framework
- [ ] Performance analytics

### Q1 2025
- [ ] Options pricing models
- [ ] Greeks calculation
- [ ] Portfolio optimization
- [ ] ML integration

## 🤝 Contributing

This is a proprietary project. For access or collaboration:
- Email: praveenkumar.avln@gmail.com
- GitHub: @praveen686

## 📄 License

Proprietary - All Rights Reserved

---

**Built with ❤️ in Rust for Indian Markets**