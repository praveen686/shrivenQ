# ShrivenQuant Quick Start Guide

## Current Status
- **Development Phase** - Core services compile but not production-ready
- **Not tested** with real exchanges
- **No backtesting** capability

## Prerequisites

1. **Rust Installation**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup default stable
   ```

2. **Protocol Buffers**
   ```bash
   # Ubuntu/Debian
   sudo apt-get install protobuf-compiler
   
   # macOS
   brew install protobuf
   ```

## Building the System

1. **Clone the repository**
   ```bash
   git clone [repository-url]
   cd ShrivenQuant
   ```

2. **Build all services**
   ```bash
   cargo build --release
   ```
   This will compile all 17 services. Expect warnings but no errors.

3. **Run tests** (minimal coverage)
   ```bash
   cargo test
   ```

## Running Services

### Start Core Services (Example)

1. **Start Gateway** (REST API)
   ```bash
   cargo run --release -p gateway
   # Listens on http://localhost:8080
   ```

2. **Start Auth Service**
   ```bash
   cargo run --release -p auth
   # Listens on localhost:50051 (gRPC)
   ```

3. **Start Market Connector**
   ```bash
   cargo run --release -p market-connector
   # Listens on localhost:50052 (gRPC)
   ```

## Testing the System

### Health Check
```bash
curl http://localhost:8080/health
```

### View Service Logs
Services output to stdout. Use separate terminals or a process manager.

## Configuration

Currently hardcoded. To add credentials (not functional yet):

1. Set environment variables:
   ```bash
   export BINANCE_API_KEY="your-key"
   export BINANCE_SECRET="your-secret"
   export ZERODHA_API_KEY="your-key"
   export ZERODHA_SECRET="your-secret"
   ```

2. These are read but not properly integrated yet.

## What Works

- ✅ All services compile
- ✅ Basic gRPC communication
- ✅ Options pricing (Black-Scholes)
- ✅ Protocol buffer definitions

## What Doesn't Work

- ❌ Exchange connections (untested)
- ❌ Order execution (not implemented)
- ❌ Backtesting (not implemented)
- ❌ Real-time data (not connected)
- ❌ Trading strategies (not implemented)

## Common Issues

### Port Already in Use
```bash
# Find and kill process using port
lsof -i :8080
kill -9 [PID]
```

### Compilation Errors
```bash
# Clean and rebuild
cargo clean
cargo build --release
```

### Missing Protobuf Compiler
Install protobuf-compiler for your OS (see Prerequisites).

## Next Steps for Development

1. **Remove unwrap() calls** - 134 potential panic points
2. **Add error handling** - Services lack proper error recovery
3. **Test exchange connectivity** - Never tested with real exchanges
4. **Implement backtesting** - Essential for strategy development
5. **Add monitoring** - No visibility into system health

## ⚠️ WARNING

This system is NOT ready for:
- Production trading
- Paper trading
- Real money
- Customer use

It's suitable only for:
- Development
- Learning
- Architecture review

## Support

For issues or questions:
- Review [Architecture Documentation](../03-architecture/)
- Check [System Status](../01-status-updates/SYSTEM_STATUS.md)
- Contact: praveenkumar.avln@gmail.com