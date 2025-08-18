# 📁 ShrivenQuant Scripts Directory

**Last Updated**: January 18, 2025  
**Maintained By**: CTO  
**Status**: REORGANIZED & CONSOLIDATED

## 📋 Directory Structure

```
scripts/
├── code-quality/         # Code quality and maintenance scripts ✨ NEW
│   ├── fix_unwrap_calls.sh     # Consolidated unwrap/test management
│   ├── migrate_tests.sh        # [DEPRECATED - use fix_unwrap_calls.sh]
│   └── remove_production_unwraps.sh # [DEPRECATED - use fix_unwrap_calls.sh]
├── deployment/           # Production deployment and startup
│   ├── connect_real_exchanges.sh
│   ├── production_demo.sh
│   ├── start_live_trading.sh
│   └── start_services.sh
├── development/          # Development workflow scripts ✨ NEW
│   └── shrivenquant.sh  # Master control script
├── monitoring/           # System monitoring and analytics
│   ├── check_system_status.sh
│   ├── performance_dashboard.sh
│   ├── run_live_analytics.sh
│   └── setup_monitoring.sh
├── testing/              # Testing and validation scripts
│   ├── system_test.sh
│   ├── test_live_data.sh
│   ├── test_market_data.sh
│   ├── test_trading_flow.sh
│   └── test_websocket_orderbook.sh
└── utils/                # Utility scripts (future)
```

## 🚀 Quick Start

### Using the Master Control Script

```bash
# Interactive menu
./scripts/shrivenquant.sh

# Direct commands
./scripts/shrivenquant.sh start    # Start all services
./scripts/shrivenquant.sh stop     # Stop all services
./scripts/shrivenquant.sh status   # Check system status
./scripts/shrivenquant.sh trade    # Start paper trading
./scripts/shrivenquant.sh test     # Run system tests
./scripts/shrivenquant.sh demo     # Run production demo
```

## 📋 Script Categories

### 🚀 Deployment Scripts
- **start_services.sh**: Launches all microservices in correct order
- **start_live_trading.sh**: Initiates live trading with real market connections
- **connect_real_exchanges.sh**: Establishes connections to Binance and Zerodha
- **production_demo.sh**: Demonstrates full system capabilities

### 📊 Monitoring Scripts
- **check_system_status.sh**: Real-time health check of all services
- **run_live_analytics.sh**: Displays live market analytics and metrics
- **setup_monitoring.sh**: Configures monitoring infrastructure

### 🧪 Testing Scripts
- **system_test.sh**: Comprehensive system integration tests
- **test_live_data.sh**: Validates live market data reception
- **test_market_data.sh**: Tests market data processing pipeline
- **test_trading_flow.sh**: End-to-end trading workflow validation
- **test_websocket_orderbook.sh**: WebSocket orderbook connectivity test

### 🔐 Authentication Scripts
- **setup-zerodha-auth.sh**: Configures Zerodha API authentication

## 💡 Usage Examples

### Start Complete Trading System
```bash
# Start all services and begin paper trading
./scripts/shrivenquant.sh
# Select: 1a (Start All Services)
# Then: 5a (Start Paper Trading)
```

### Run System Health Check
```bash
./scripts/shrivenquant.sh status
```

### Test Market Connectivity
```bash
./scripts/testing/test_websocket_orderbook.sh
```

### View Live Analytics
```bash
./scripts/monitoring/run_live_analytics.sh
```

## 🔧 Environment Requirements

- Rust toolchain (cargo)
- Active internet connection for market data
- Configured .env file with exchange credentials
- Minimum 8GB RAM for full system operation

## 📝 Notes

- All scripts include error handling and logging
- Scripts automatically check for required dependencies
- Use `shrivenquant.sh` as the primary interface for consistency
- Individual scripts can be run directly for specific tasks

## 🛡️ Safety Features

- All scripts use `set -e` for fail-fast behavior
- Services are started with proper health checks
- Automatic cleanup on script termination
- Non-destructive operations by default (confirmations required for data deletion)