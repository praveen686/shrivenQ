# ShrivenQuant - Algorithmic Trading System

## ⚠️ Development Status: NOT PRODUCTION READY

**Reality Check**: This is a Rust-based microservices trading system in early development. While the architecture is solid, it has NOT been tested with real markets.

## Quick Status

```
✅ Compiles: Yes (with warnings)
✅ Architecture: Microservices with gRPC
⚠️  Testing: Minimal coverage
❌ Production Ready: No
❌ Exchange Tested: No
❌ Backtesting: Not implemented
❌ Live Trading: Never attempted
```

## Honest Assessment

### What Actually Works
- **Compilation**: All 17 services compile with Rust 2024
- **Proto Definitions**: gRPC interfaces defined
- **Options Pricing**: Black-Scholes with Greeks calculations
- **Basic Structure**: Microservices architecture established

### What Doesn't Work
- **No Real Trading**: Never executed a real trade
- **No Exchange Testing**: Connections not verified
- **No Backtesting**: Cannot test strategies
- **No ML Models**: Framework only, no trained models
- **134 unwrap() calls**: Will panic in production
- **No Integration Tests**: Services not tested together

## System Components

| Service | Reality | Description |
|---------|---------|-------------|
| auth | Compiles only | Authentication framework |
| gateway | Compiles only | API gateway structure |
| market-connector | Untested | Exchange connectivity |
| data-aggregator | Untested | Data processing |
| risk-manager | Framework only | Risk management |
| execution-router | Untested | Order routing |
| portfolio-manager | Basic logic | Portfolio optimization |
| reporting | Minimal | Analytics framework |
| orderbook | Basic impl | Order book management |
| trading-gateway | Untested | Strategy orchestration |
| oms | Framework | Order management |
| options-engine | ✅ Working | Black-Scholes pricing |
| monitoring | Stub | System monitoring |
| secrets-manager | Basic | Credential encryption |
| ml-inference | No models | ML predictions framework |
| sentiment-analyzer | No API keys | Reddit sentiment |
| logging | Basic | Centralized logging |

## Critical Issues

### 🔴 Production Blockers
1. **134 unwrap() calls** - Guaranteed crashes
2. **Zero integration tests** - Unknown if services work together
3. **No error handling** - Services will fail ungracefully
4. **No real data testing** - Never connected to exchanges
5. **No backtesting** - Cannot validate strategies
6. **No monitoring** - Blind to system health
7. **Hardcoded values** - Configuration scattered
8. **No authentication** - Services unsecured

## Building

```bash
# Clone and build
git clone [repository]
cd ShrivenQuant
cargo build --release  # Builds with warnings

# Run a service (example)
cargo run --release -p gateway
```

## Architecture

The system uses a microservices architecture with gRPC:

```
Gateway → Services → Exchanges (never tested)
   ↓          ↓           ↓
Logging     OMS    Market Data
```

## Time to Production

**Realistic estimate with full-time development:**
- Minimum Viable Product: 3-4 months
- Production Ready: 6-8 months  
- Battle Tested: 12+ months

## What's Needed for Production

### Immediate (Critical)
1. Remove all unwrap() calls
2. Implement error handling
3. Add integration tests
4. Test exchange connections
5. Implement backtesting

### Short Term (Essential)
1. Monitoring & alerting
2. Kubernetes deployment
3. Database setup
4. Configuration management
5. Security implementation

### Long Term (Important)
1. Performance optimization
2. ML model training
3. Strategy development
4. Compliance features
5. Disaster recovery

## Testing

```bash
# Current test coverage: ~5%
cargo test  # Runs minimal tests
```

## Documentation

- [System Status](docs/SYSTEM_STATUS.md) - Detailed current state
- [Service Documentation](services/) - Individual service READMEs
- [Architecture](docs/architecture/) - System design docs

## Directory Structure

```
ShrivenQuant/
├── services/       # 17 microservices
├── proto/          # gRPC definitions  
├── scripts/        # Utility scripts
├── docs/           # Documentation
└── tests/          # (mostly empty)
```

## Configuration

Currently hardcoded throughout. Needs externalization:
```bash
# These don't work yet:
BINANCE_API_KEY=xxx
ZERODHA_API_KEY=xxx
```

## Known Limitations

1. **Never tested with real markets**
2. **No paper trading capability**
3. **No historical data handling**
4. **No live market data streaming**
5. **No order execution tested**
6. **No risk management active**
7. **No position tracking**
8. **No P&L calculation**

## Warning

**DO NOT USE FOR REAL TRADING**

This system is a work-in-progress and will lose money if used for actual trading. It lacks:
- Error recovery
- Testing
- Monitoring  
- Security
- Proven strategies
- Exchange certification

## Contact

Praveen Ayyasola <praveenkumar.avln@gmail.com>

---

**Status**: Educational/Development prototype only. Not suitable for any form of trading.