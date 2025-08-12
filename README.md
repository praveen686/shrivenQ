# ShrivenQ 🚀

**Institutional-Grade, Ultra-Low-Latency Trading Platform for Indian & Crypto Markets**

[![CI](https://github.com/praveen686/shrivenQ/actions/workflows/strict-ci.yml/badge.svg)](https://github.com/praveen686/shrivenQ/actions/workflows/strict-ci.yml)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Proprietary-red.svg)](LICENSE)

> ⚡ Sub-millisecond latency | 🛡️ Zero-tolerance quality | 🔄 Deterministic replay

## 🎯 Mission

ShrivenQ is a world-class, institutional-grade trading platform designed for:
- **Indian Markets:** NIFTY/BANKNIFTY options via Zerodha
- **Crypto Markets:** Spot & options via Binance
- **Performance:** < 1ms end-to-end latency (paper trading)
- **Reliability:** Crash-safe with WAL persistence
- **Quality:** Zero warnings, zero dead code, 100% safe

## ✨ Key Features

- **📊 Market Data Pipeline:** Real-time tick and LOB data collection with nanosecond precision
- **🔄 Historical Replay:** Replay market data from any time period with symbol filtering
- **💾 WAL Persistence:** Crash-safe storage with deterministic replay guarantees
- **📈 LOB Snapshots:** Full order book snapshots with efficient storage and retrieval
- **🎯 Smart Filtering:** Intelligent symbol resolution using instrument metadata
- **⚡ Performance:** Achieves 298M events/min replay (measured), 229 MB/s writes
- **🔍 Monitoring:** Real-time dashboard for system health and performance metrics

## 🚀 Quick Start

```bash
# Clone the repository
git clone https://github.com/praveen686/shrivenQ.git
cd shrivenQ

# Run quality checks
./scripts/compliance/strict-check.sh

# Start the platform with heartbeat monitoring
cargo run -p cli -- dev up --heartbeat-ms 500

# Check system health
cargo run -p cli -- dev ping

# Start market data collection
cargo run --bin market-data-service -- run --symbols "NIFTY 50,NIFTY BANK"

# Replay historical data
cargo run --bin market-data-service -- replay \
  --start "2024-01-15T09:15:00+05:30" \
  --end "2024-01-15T15:30:00+05:30" \
  --symbol "NIFTY"
```

## 📊 Sprint Status

| Sprint | Description | Status | Progress |
|--------|------------|--------|----------|
| **Sprint 1** | Workspace & CLI | ✅ **COMPLETE** | 100% |
| **Sprint 2** | WAL & Replay | ✅ **COMPLETE** | 100% |
| **Sprint 3** | Feed Adapters & LOB | ✅ **COMPLETE** | 100% |
| Sprint 4 | Strategy Runtime | ⏳ Planned | 0% |
| Sprint 5 | Live Integration | ⏳ Planned | 0% |
| Sprint 6 | Backtester | ⏳ Planned | 0% |

**Overall Progress:** ~50% Complete

**[📄 Detailed Sprint Progress & Architecture](docs/architecture/README.md#sprint-progress--development-roadmap)**

## 🏗️ Architecture

```
┌─────────────────────────────────────────┐
│          Event Bus (Lock-Free)          │
├────────┬────────┬────────┬─────────────┤
│  Feed  │  LOB   │ Strategy│    Risk     │
│Adapters│ Engine │ Runtime │   Engine    │
├────────┴────────┴────────┴─────────────┤
│            WAL Persistence              │
└─────────────────────────────────────────┘
```

**Core Components:**
- `crates/core/common/` - Core types (Symbol, Price, Quantity, Timestamp)
- `crates/infra/bus/` - Lock-free event bus with crossbeam channels
- `crates/infra/storage/` - Write-ahead log with deterministic replay
- `crates/infra/auth/` - Multi-venue authentication (Zerodha, Binance)
- `crates/market-data/lob/` - Ultra-fast order book engine with adapters & loaders
- `crates/market-data/feeds/` - Market data adapters, WebSocket feeds & integration
- `crates/trading/engine/` - Zero-allocation trading engine
- `crates/trading/sim/` - Simulation and backtesting framework
- `crates/tools/cli/` - Command-line interface
- `crates/tools/perf/` - Performance monitoring tools

## 🎯 Performance Targets

| Metric | Target | Current |
|--------|--------|---------|
| Tick → Bus | ≤ 300 µs | - |
| Bus → Strategy | ≤ 200 µs | - |
| Strategy → Order | ≤ 300 µs | - |
| **End-to-End** | **≤ 1 ms** | ✅ < 1ms |
| Heartbeat | 1 Hz | ✅ Working |

## 🛡️ Quality Standards

**Zero Tolerance Policy:**
- ❌ No compiler warnings
- ❌ No dead or unused code
- ❌ No TODO/FIXME comments
- ❌ No `unwrap()`, `expect()`, `panic!`
- ❌ No `println!`, `dbg!` macros
- ❌ No missing documentation
- ❌ No unsafe code

**Enforcement:**
```bash
# Run before every commit
./scripts/compliance/strict-check.sh
```

## 🔧 Development

### Building
```bash
# Debug build
cargo build

# Release build with optimizations
cargo build --release

# Run tests
cargo test --workspace

# Format code
cargo fmt --all

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings
```

### Project Structure
```
shrivenq/
├── Cargo.toml           # Workspace configuration
├── README.md            # This file
├── LICENSE              # Proprietary license
├── rust-toolchain.toml  # Pinned Rust version
├── clippy.toml          # Strict clippy settings
├── .cargo/
│   └── config.toml      # Build flags
├── .github/
│   └── workflows/       # CI/CD pipelines
├── .pre-commit-config.yaml # Pre-commit hooks
├── docs/                # Documentation
├── scripts/             # Automation & build scripts (see below)
└── crates/              # All source code
    ├── core/            # Core functionality
    │   └── common/      # Shared types and utilities
    ├── infra/           # Infrastructure
    │   ├── auth/        # Authentication
    │   ├── bus/         # Event bus
    │   └── storage/     # WAL persistence
    ├── market-data/     # Market data processing
    │   ├── feeds/       # Feed adapters & integration modules
    │   └── lob/         # Order book, data loaders & adapters
    ├── trading/         # Trading logic
    │   ├── engine/      # Execution engine
    │   └── sim/         # Simulation
    └── tools/           # Development tools
        ├── cli/         # CLI interface
        └── perf/        # Performance tools
```

## 🛠️ Scripts & Automation

The `scripts/` directory contains comprehensive automation for building, testing, and maintaining the ShrivenQuant platform. All scripts are organized into logical categories:

### Directory Structure
```
scripts/
├── build/           # Build automation
├── compliance/      # Code quality checking  
├── deployment/      # Deployment automation
├── development/     # Development tools
├── performance/     # Performance testing
└── testing/         # Test execution
```

### Key Scripts

**Build & Compilation:**
- `./scripts/build/orchestrator.sh [quick|release|docker|cross|all]` - Main build pipeline
- `./scripts/build/build.rs [release|debug|bench|check]` - Rust build automation (requires rust-script)
- `./scripts/build/cross-compile.sh` - Cross-platform binary generation
- `./scripts/build/docker-build.sh` - Multi-stage Docker builds

**Compliance & Quality:**
- `./scripts/compliance/strict-check.sh` - **Primary compliance check (MUST PASS)**
- `./scripts/compliance/agent-compliance-check.sh` - AI agent code validation
- `./scripts/compliance/compliance-summary.sh` - Detailed compliance report

**Testing & Performance:**
- `./scripts/testing/run-integration-tests.sh` - Full integration test suite
- `./scripts/performance/performance-check.sh` - Performance benchmarks
- `./scripts/performance/check-hot-path-allocations.sh` - Detect critical path allocations

### Quick Commands

```bash
# Before committing (mandatory)
./scripts/compliance/strict-check.sh

# Quick build with tests
./scripts/build/orchestrator.sh quick

# Full release build
./scripts/build/orchestrator.sh release

# Setup pre-commit hooks
./scripts/development/install-precommit.sh

# Cross-platform builds
./scripts/build/cross-compile.sh
```

**Note:** Build scripts require `rust-script` installed: `cargo install rust-script`

For complete documentation, see [scripts/README.md](scripts/README.md)

## 📈 Roadmap

### ✅ Phase 1: Foundation (Complete)
- [x] Sprint 1: Workspace setup
- [x] Sprint 2: WAL & persistence
- [x] Sprint 3: Market data feeds

### 🔄 Phase 2: Trading Core
- [ ] Sprint 4: Strategy runtime
- [ ] Sprint 5: Live integration
- [ ] Sprint 6: Backtesting

### 🚀 Phase 3: Production
- [ ] Risk management suite
- [ ] Multi-venue support
- [ ] GPU acceleration
- [ ] Distributed deployment

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Ensure all checks pass: `./scripts/compliance/strict-check.sh`
4. Submit a pull request

## 📚 Documentation

- **[Complete Documentation](docs/README.md)** - Main documentation entry point
- **[Architecture Overview](docs/architecture/README.md)** - System design and components
- **[Developer Guide](docs/developer-guide/README.md)** - Development setup and workflow
- **[Trader Guide](docs/trader-guide/README.md)** - Usage guide for traders
- **[API Reference](docs/api-reference/README.md)** - Detailed API documentation
- **[Deployment Guide](docs/deployment/README.md)** - Production deployment

## 📄 License

**PROPRIETARY SOFTWARE**

Copyright © 2025 Praveen Ayyasola. All rights reserved.

This software is proprietary and confidential. See [LICENSE](LICENSE) for details.

For licensing inquiries: praveenkumar.avln@gmail.com

## 👨‍💻 Author

**Praveen Ayyasola**  
📧 praveenkumar.avln@gmail.com  
🔗 [GitHub](https://github.com/praveen686)

---

⭐ If you find ShrivenQ useful, please star the repository!
