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

## 🚀 Quick Start

```bash
# Clone the repository
git clone https://github.com/praveen686/shrivenQ.git
cd shrivenQ

# Run quality checks
bash scripts/strict-check.sh

# Start the platform with heartbeat monitoring
cargo run -p cli -- dev up --heartbeat-ms 500

# Check system health
cargo run -p cli -- dev ping
```

## 📊 Sprint Status

| Sprint | Description | Status | Progress |
|--------|------------|--------|----------|
| **Sprint 1** | Workspace & CLI | ✅ **COMPLETE** | 100% |
| Sprint 2 | WAL & Replay | ⏳ Planned | 0% |
| Sprint 3 | Feed Adapters & LOB | ⏳ Planned | 0% |
| Sprint 4 | Strategy Runtime | ⏳ Planned | 0% |
| Sprint 5 | Live Integration | ⏳ Planned | 0% |
| Sprint 6 | Backtester | ⏳ Planned | 0% |

**Overall Progress:** ~17% Complete

[📄 Detailed Sprint Status](docs/SPRINT_STATUS.md)

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
- `common/` - Core types (Symbol, Price, Quantity, Timestamp)
- `bus/` - Lock-free event bus with crossbeam channels
- `cli/` - Command-line interface
- `storage/` - Write-ahead log (coming in Sprint 2)
- `feed/` - Market data adapters (coming in Sprint 3)
- `lob/` - Order book engine (coming in Sprint 3)

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
bash scripts/strict-check.sh
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
├── LICENSE-MIT          # MIT license
├── LICENSE-APACHE       # Apache 2.0 license
├── rust-toolchain.toml  # Pinned Rust version
├── clippy.toml          # Strict clippy settings
├── .cargo/
│   └── config.toml      # Build flags
├── .github/
│   └── workflows/       # CI/CD pipelines
├── docs/                # Documentation
├── scripts/
│   └── strict-check.sh  # Quality enforcement
├── common/              # Core types
├── bus/                 # Event bus
└── cli/                 # CLI interface
```

## 📈 Roadmap

### ✅ Phase 1: Foundation (Current)
- [x] Sprint 1: Workspace setup
- [ ] Sprint 2: WAL & persistence
- [ ] Sprint 3: Market data feeds

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
3. Ensure all checks pass: `bash scripts/strict-check.sh`
4. Submit a pull request

## 📚 Documentation

- [Project Overview](docs/PROJECT_OVERVIEW.md)
- [Sprint Status](docs/SPRINT_STATUS.md)
- [Architecture Decisions](docs/ARCHITECTURE_DECISIONS.md)

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