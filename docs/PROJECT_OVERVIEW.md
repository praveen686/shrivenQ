# ShrivenQ Project Overview

## 🎯 Mission Statement

ShrivenQ is an institutional-grade, ultra-low-latency (ULL) trading platform designed for Indian index options (via Zerodha) and cryptocurrency markets (via Binance). The platform emphasizes deterministic performance, crash-safety, and rigorous risk management.

## 🏗️ Current Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     ShrivenQ Platform                    │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐             │
│  │  Common  │  │   Bus    │  │   CLI    │             │
│  │  Types   │  │  Events  │  │ Commands │             │
│  └──────────┘  └──────────┘  └──────────┘             │
│                                                          │
│  ┌──────────────────────────────────────┐              │
│  │        Planned Components             │              │
│  ├──────────────────────────────────────┤              │
│  │ • Feed Adapters (Zerodha, Binance)   │              │
│  │ • LOB Engine (Order Book)            │              │
│  │ • Strategy Runtime                   │              │
│  │ • Risk Engine                        │              │
│  │ • Order Manager                      │              │
│  │ • Storage (WAL)                      │              │
│  │ • Backtester                         │              │
│  └──────────────────────────────────────┘              │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

## 💻 Technology Stack

### Core Technologies
- **Language:** Rust (Edition 2024)
- **Async Runtime:** Tokio
- **Message Passing:** Crossbeam channels
- **Serialization:** Bincode, Serde
- **CLI Framework:** Clap
- **Logging:** Tracing

### Development Tools
- **Build System:** Cargo with workspace
- **Quality:** Clippy (pedantic + nursery), Rustfmt
- **CI/CD:** GitHub Actions
- **Testing:** Built-in Rust test framework

## 📊 Performance Targets

### Latency Goals (v0.1)
- Market tick → Bus: **≤ 300 µs**
- Bus → Strategy: **≤ 200 µs**  
- Strategy → Risk → Order: **≤ 300 µs**
- **End-to-end:** ≤ 1 ms (paper trading)

### Throughput Goals
- Tick ingestion: > 100K msgs/sec
- LOB updates: > 200K updates/sec
- Strategy decisions: > 50K/sec
- WAL writes: > 1M events/sec

## 🛡️ Quality Standards

### Zero Tolerance Policy
The codebase enforces strict quality standards with **zero tolerance** for:

- ❌ Compiler warnings
- ❌ Dead or unused code
- ❌ TODO, FIXME, HACK comments
- ❌ `unwrap()`, `expect()`, `panic!`
- ❌ `println!`, `dbg!` macros
- ❌ Missing documentation
- ❌ Unformatted code
- ❌ Unsafe code blocks

### Enforcement Mechanisms
1. **Pre-compile Hook:** `scripts/strict-check.sh`
2. **CI Pipeline:** GitHub Actions on every push
3. **Local Lints:** Cargo config with deny-by-default
4. **Clippy Config:** Aggressive thresholds

## 📁 Project Structure

```
shrivenq/
├── Cargo.toml           # Workspace configuration
├── clippy.toml          # Strict clippy settings
├── .cargo/
│   └── config.toml      # Build flags and aliases
├── .github/
│   └── workflows/       # CI/CD pipelines
├── docs/                # Documentation
│   ├── SPRINT_STATUS.md # Sprint tracking
│   └── PROJECT_OVERVIEW.md
├── scripts/
│   └── strict-check.sh  # Quality check script
├── common/              # Core types and utilities
├── bus/                 # Event bus implementation
├── cli/                 # Command-line interface
└── [planned crates...]  # Future components
```

## 🔧 Development Workflow

### Building the Project
```bash
# Standard build
cargo build

# Release build with optimizations
cargo build --release

# Run strict quality checks
bash scripts/strict-check.sh
```

### Running ShrivenQ
```bash
# Start development environment with heartbeat
cargo run -p cli -- dev up --heartbeat-ms 500

# Check system health
cargo run -p cli -- dev ping
```

### Testing
```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_spsc_channel
```

## 🚀 Roadmap

### Phase 1: Foundation (Current)
- ✅ Sprint 1: Workspace & CLI
- ⏳ Sprint 2: WAL & Replay
- ⏳ Sprint 3: Feed Adapters & LOB

### Phase 2: Trading Core
- ⏳ Sprint 4: Strategy Runtime
- ⏳ Sprint 5: Live Integration
- ⏳ Sprint 6: Backtester

### Phase 3: Production (Future)
- Risk management suite
- Multi-venue support
- GPU acceleration
- Distributed deployment
- Advanced strategies

## 📈 Success Metrics

### Technical KPIs
- Latency: P99 < 1ms
- Uptime: > 99.9%
- Zero data loss
- Deterministic replay

### Business KPIs
- Sharpe ratio > 2.0
- Max drawdown < 10%
- Win rate > 60%
- Profit factor > 1.5

## 🤝 Contributing

### Code Standards
1. All code must pass `scripts/strict-check.sh`
2. Document all public APIs
3. Write tests for new functionality
4. Follow existing patterns and conventions

### Commit Guidelines
- Use conventional commits (feat:, fix:, docs:, etc.)
- Reference issue numbers
- Keep commits atomic and focused

## 📞 Contact

**Author:** Praveen Ayyasola  
**Email:** praveenkumar.avln@gmail.com  
**Repository:** https://github.com/praveen686/shrivenQ

## 📄 License

MIT OR Apache-2.0

---

*Last Updated: 2025-08-11*