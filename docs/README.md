# ShrivenQuant Documentation

## Overview

Technical documentation for the ShrivenQuant algorithmic trading system - a Rust-based microservices platform currently in early development.

## ⚠️ Important Notice

**Current Status**: Development prototype, NOT production-ready
**Testing**: Never tested with real exchanges
**Safety**: Contains 134 unwrap() calls that will cause crashes

## Documentation Structure

### Essential Documents

#### System Status
- **[SYSTEM_STATUS.md](SYSTEM_STATUS.md)** - Accurate current state assessment
- **[Main README](../README.md)** - Project overview and warnings

#### Architecture
- **[Architecture Overview](architecture/README.md)** - System design and service layout
- **[Logging Architecture](architecture/LOGGING_ARCHITECTURE.md)** - Logging system design
- **[Trading Gateway](architecture/trading-gateway.md)** - Trading service details

#### Getting Started
- **[Quick Start Guide](getting-started/quick-start.md)** - Build and run instructions
- **[Getting Started](getting-started/getting-started.md)** - Detailed setup

#### Development
- **[Development Roadmap](development/ROADMAP.md)** - Realistic path to production
- **[Best Practices](development/best-practices.md)** - Coding standards
- **[Command Reference](development/command-reference.md)** - Useful commands

#### Service Documentation
Each service has its own README:
- [Auth Service](../services/auth/README.md)
- [Options Engine](../services/options-engine/README.md) - ✅ Working
- [ML Inference](../services/ml-inference/README.md) - Framework only
- [Sentiment Analyzer](../services/sentiment-analyzer/README.md) - Partial
- [Secrets Manager](../services/secrets-manager/README.md) - Basic
- [Logging Service](../services/logging/README.md) - Framework

#### Exchange Integration
- **[Binance Setup](integrations/binance-setup.md)** - Crypto exchange (untested)
- **[Zerodha Setup](integrations/zerodha-setup.md)** - Indian broker (untested)

#### Security
- **[Security Audit](security/SECURITY_AUDIT.md)** - Security issues and recommendations

## Quick Navigation

### I want to understand what this is
→ Read [SYSTEM_STATUS.md](SYSTEM_STATUS.md) for honest assessment

### I want to build and run it
→ Follow [Quick Start Guide](getting-started/quick-start.md)

### I want to know what's missing
→ Check [Development Roadmap](development/ROADMAP.md)

### I want to understand the architecture
→ Review [Architecture Overview](architecture/README.md)

## Current Capabilities

### ✅ What Works
- All 17 services compile
- Options pricing with Black-Scholes
- Basic microservices structure
- gRPC protocol definitions

### ❌ What Doesn't Work
- Exchange connections (never tested)
- Order execution (not implemented)
- Backtesting (not implemented)
- Real-time data (not connected)
- ML predictions (no models)
- Monitoring (stub only)

## Development Status

```
Component          Status      Notes
-----------------  ----------  --------------------------------
Core Structure     ✅ Done     Microservices architecture
Compilation        ✅ Works    With warnings
Options Pricing    ✅ Working  Black-Scholes implemented
Exchange Connect   ❌ Untested Never attempted
Order Execution    ❌ Missing   Not implemented
Backtesting       ❌ Missing   Critical gap
Risk Management   ⚠️ Framework No actual implementation
ML Models         ❌ None      Framework only
Production Ready  ❌ No        6-12 months away
```

## Critical Issues

1. **134 unwrap() calls** - Will panic in production
2. **No integration tests** - Unknown if services work together
3. **No error handling** - Services crash on errors
4. **Never tested** - No real market data or trades
5. **No monitoring** - Blind to system health

## Timeline to Production

With full-time development team:
- **Stabilization**: 1 month (fix crashes, add error handling)
- **Core Features**: 2 months (backtesting, exchange integration)
- **Production Prep**: 2-3 months (monitoring, security, testing)
- **Hardening**: 1-2 months (paper trading, optimization)

**Total: 6-8 months minimum**

## Documentation Standards

All documentation must be:
- **Accurate** - No false claims or hyperbole
- **Dated** - Include last update date
- **Honest** - Clear about limitations
- **Technical** - Focus on facts, not marketing

## Warning

This system is a development prototype. It will lose money if used for trading because it:
- Has never been tested with real markets
- Contains numerous crash points
- Lacks error recovery
- Has no proven strategies
- Is missing critical features

## Support

For questions or issues:
- Review existing documentation
- Check service README files
- Contact: praveenkumar.avln@gmail.com

---

*Last Updated: August 18, 2025*