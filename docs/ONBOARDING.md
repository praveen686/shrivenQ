# 🚀 ShrivenQuant Developer Onboarding Guide

**Welcome to ShrivenQuant!** This guide will help you get up and running with our algorithmic trading system.

## 📌 Project Reality Check

**MUST UNDERSTAND BEFORE ANY WORK:**

1. **This is a development prototype** - Pre-alpha software, not production-ready
2. **Limited exchange testing** - Framework exists but needs live validation
3. **Active development** - Breaking changes expected
4. **Security first** - Never expose real credentials
5. **Test everything** - 110 tests passing, targeting 80% coverage

## 📋 Prerequisites

### Required Software
- **Rust**: 1.75+ (install via [rustup](https://rustup.rs/))
- **Git**: For version control
- **Docker**: For containerization (optional for now)
- **PostgreSQL**: 14+ (future requirement)
- **Redis**: 6+ (future requirement)

### Development Tools
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install development tools
cargo install cargo-watch      # Auto-rebuild on changes
cargo install cargo-tarpaulin  # Code coverage
cargo install cargo-clippy     # Linting
cargo install cargo-fmt        # Formatting
```

## 🏗️ Project Setup

### 1. Clone Repository
```bash
git clone https://github.com/your-org/ShrivenQuant.git
cd ShrivenQuant
```

### 2. Build Project
```bash
# Build all services
cargo build --workspace

# Build in release mode (optimized)
cargo build --release

# Check compilation without building
cargo check
```

### 3. Run Tests
```bash
# Run all tests (110 currently passing)
cargo test --workspace

# Run specific service tests
cargo test -p auth-service  # 43 tests
cargo test -p oms           # 17 tests

# Run with output
cargo test -- --nocapture
```

## 🔐 Credentials Setup

### Using Secrets Manager (Recommended)

1. **Start Secrets Manager**
   ```bash
   export MASTER_PASSWORD="your_secure_password"
   cargo run -p secrets-manager --bin secrets-manager-server
   ```

2. **Store Credentials**
   ```bash
   # Zerodha credentials
   cargo run -p secrets-manager --bin secrets-manager store ZERODHA_USER_ID "your_id"
   cargo run -p secrets-manager --bin secrets-manager store ZERODHA_PASSWORD "your_pass"
   cargo run -p secrets-manager --bin secrets-manager store ZERODHA_TOTP_SECRET "totp_secret"
   cargo run -p secrets-manager --bin secrets-manager store ZERODHA_API_KEY "api_key"
   cargo run -p secrets-manager --bin secrets-manager store ZERODHA_API_SECRET "api_secret"
   
   # Binance credentials
   cargo run -p secrets-manager --bin secrets-manager store BINANCE_SPOT_API_KEY "api_key"
   cargo run -p secrets-manager --bin secrets-manager store BINANCE_SPOT_API_SECRET "api_secret"
   ```

### Using .env File (Fallback)

Create `.env` file in project root:
```env
# Zerodha
ZERODHA_USER_ID=your_user_id
ZERODHA_PASSWORD=your_password
ZERODHA_TOTP_SECRET=your_totp_secret
ZERODHA_API_KEY=your_api_key
ZERODHA_API_SECRET=your_api_secret

# Binance
BINANCE_SPOT_API_KEY=your_api_key
BINANCE_SPOT_API_SECRET=your_api_secret
BINANCE_TESTNET=true
```

## 🏛️ Architecture Overview

```
ShrivenQuant/
├── services/           # 20 Microservices
│   ├── auth/          # ✅ Authentication (43 tests passing)
│   ├── oms/           # ✅ Order management (17 tests passing)
│   ├── market-connector/ # ✅ Exchange connectivity (12 tests)
│   ├── risk-manager/  # ✅ Risk management (tests passing)
│   ├── portfolio-manager/ # ✅ Portfolio tracking (14 tests)
│   ├── secrets-manager/   # ✅ Credential management (NEW!)
│   ├── orderbook/     # ❌ Needs fixing (compilation errors)
│   └── ...            # 13 more services
├── proto/             # Protocol buffer definitions
├── docs/              # Documentation
├── scripts/           # Utility scripts
└── tests/             # Integration tests
```

### Service Status Summary

| Status | Count | Services |
|--------|-------|----------|
| ✅ Working | 7 | Auth, OMS, Portfolio, Market Connector, Risk, Reporting, Trading Gateway |
| ⚠️ Partial | 5 | API Gateway, Execution Router, Options Engine, Backtesting, Data Aggregator |
| ❌ Broken | 1 | Orderbook |
| 🆕 New | 1 | Secrets Manager |
| 📝 No Tests | 6 | Various services need test implementation |

## 🚦 Running Services

### Start Core Services

```bash
# 1. Secrets Manager (always start first)
export MASTER_PASSWORD="your_password"
cargo run -p secrets-manager --bin secrets-manager-server &

# 2. Auth Service
cargo run -p auth-service &

# 3. OMS
cargo run -p oms &

# 4. Market Connector
cargo run -p market-connector &

# 5. Risk Manager
cargo run -p risk-manager &
```

### Test Exchange Connection

```bash
# Test Zerodha authentication
cargo run -p auth-service --bin zerodha -- auth

# Expected output:
# [INFO] Connected to secrets manager
# [INFO] Loading Zerodha configuration from secrets manager
# [INFO] Successfully retrieved credential for key: ZERODHA_USER_ID
```

## 📊 Development Workflow

### 1. Check Current Status
```bash
# View dashboard
cat docs/DASHBOARD.md

# Check test status
cargo test --workspace 2>&1 | grep "test result"

# View recent changes
git log --oneline -10
```

### 2. Pick a Task

**Priority Areas:**
1. Fix orderbook compilation errors
2. Increase test coverage (currently 40%, target 80%)
3. Add missing tests to services
4. Database integration
5. Live exchange testing

### 3. Development Cycle

```bash
# 1. Create feature branch
git checkout -b feature/your-feature

# 2. Make changes
# Edit files...

# 3. Run tests
cargo test -p affected-service

# 4. Check quality
cargo clippy -- -D warnings
cargo fmt --check

# 5. Commit with descriptive message
git add .
git commit -m "feat(service): Add feature description"
```

### 4. Testing Guidelines

#### Write Tests First
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;
    
    #[rstest]
    #[case(2, 2, 4)]
    #[case(0, 5, 5)]
    fn test_calculation(
        #[case] a: i32,
        #[case] b: i32,
        #[case] expected: i32
    ) {
        assert_eq!(add(a, b), expected);
    }
}
```

#### Integration Tests
```rust
// tests/integration/service_test.rs
#[tokio::test]
async fn test_service_flow() {
    let service = Service::new().await.unwrap();
    // Test complete flow
}
```

## 🐛 Common Issues & Solutions

### Compilation Errors
```bash
# Clean and rebuild
cargo clean && cargo build

# Update dependencies
cargo update

# Check specific service
cargo check -p service-name
```

### Test Failures
```bash
# Debug single test
RUST_BACKTRACE=1 cargo test test_name -- --nocapture

# Run ignored tests
cargo test -- --ignored
```

### Secrets Manager Issues
```bash
# Check if running
ps aux | grep secrets-manager

# Restart service
pkill -f secrets-manager
export MASTER_PASSWORD="your_password"
cargo run -p secrets-manager --bin secrets-manager-server &
```

## 📚 Essential Documentation

### Must Read
1. [Dashboard](docs/DASHBOARD.md) - Current project status
2. [Secrets Manager](docs/services/SECRETS_MANAGER.md) - Credential management
3. [Testing Guide](docs/05-testing/README.md) - Test infrastructure
4. [Architecture](docs/03-architecture/README.md) - System design

### Quick References
- **Test Status**: 110/153 passing (71.9%)
- **Services Working**: 7/20 (35%)
- **Code Coverage**: ~40%
- **Target Coverage**: 80%
- **Auth Service**: Fixed! 43 tests passing
- **Secrets Integration**: Complete

## 🎯 Week-by-Week Goals

### Week 1: Environment Setup
- [ ] Build all services
- [ ] Run test suite (110 tests should pass)
- [ ] Set up secrets manager
- [ ] Read dashboard and architecture docs
- [ ] Understand service communication

### Week 2: Code Familiarization
- [ ] Deep dive into one working service (Auth/OMS recommended)
- [ ] Write or fix 5 tests
- [ ] Fix one compilation warning
- [ ] Make first PR

### Week 3: Active Development
- [ ] Take ownership of one service
- [ ] Increase its test coverage by 20%
- [ ] Document your changes
- [ ] Review someone else's PR

### Week 4: Advanced Tasks
- [ ] Fix a broken service or add missing tests
- [ ] Implement a small feature
- [ ] Improve performance in one area
- [ ] Update documentation

## 🚨 Critical Guidelines

### Security Rules
1. **NEVER commit credentials** - Use secrets manager
2. **NEVER use real trading accounts** - Testnet only
3. **ALWAYS validate inputs** - Prevent injection attacks
4. **ALWAYS use TLS** - For production services

### Code Quality Standards
1. **NO `unwrap()` in production** - Use `?` or proper error handling
2. **MUST compile with** `cargo clippy -- -D warnings`
3. **MUST have tests** - Minimum 80% coverage target
4. **MUST document public APIs** - Use `///` doc comments

### Trading Safety
1. **USE TESTNET FIRST** - Never test with real money
2. **IMPLEMENT RISK CHECKS** - Position limits, stop losses
3. **LOG EVERYTHING** - Audit trail for compliance
4. **PAPER TRADE FIRST** - Validate strategies

## 🤝 Getting Help

### Self-Help First
1. Check existing documentation
2. Read test files for examples
3. Search codebase for similar patterns
4. Check GitHub issues

### Communication Channels
- GitHub Issues - Bug reports and features
- Pull Requests - Code reviews and discussions
- Code Comments - Inline documentation

## ✅ Onboarding Checklist

### Day 1
- [ ] Environment setup complete
- [ ] Project builds successfully
- [ ] Tests run (110 passing)
- [ ] Secrets manager configured

### Week 1
- [ ] All services attempted to build
- [ ] Dashboard reviewed
- [ ] One service understood deeply
- [ ] First test written

### Month 1
- [ ] 10+ tests written/fixed
- [ ] 3+ PRs submitted
- [ ] 1 service owned
- [ ] Documentation contributed

## 🎊 Welcome to ShrivenQuant!

Remember:
- **Start small** - Fix a test, then a service
- **Ask questions** - No question is too basic
- **Test thoroughly** - Quality over speed
- **Document everything** - Future you will thank you

You're joining at an exciting time - the auth service just got fixed, secrets management is integrated, and we're pushing toward production readiness!

**Current Momentum:**
- 📈 45% overall progress (up 5%)
- ✅ 110 tests passing (up from 84)
- 🔐 Secrets management integrated
- 🚀 Auth service resurrected

**Your Mission:**
Help us reach 80% test coverage and fix the remaining services!

---

*Last Updated: August 20, 2025*
*Location: /docs/ONBOARDING.md*
*Version: Post-Auth-Fix Era*