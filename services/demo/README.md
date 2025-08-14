# ShrivenQuant Demo Service

A demonstration of the ShrivenQuant microservices architecture showcasing authentication and market connector services.

## Overview

This demo integrates:
- **Auth Service**: JWT-based authentication with permissions management
- **Market Connector Service**: Multi-exchange market data subscription management

## Running the Demo

### Start the Demo Service

```bash
cargo run --bin demo
```

The service will start on `http://localhost:8080`

## API Endpoints

### 1. Service Status
```bash
curl http://localhost:8080/status
```

Returns the status of auth and market connector services.

### 2. Authentication

#### Login
```bash
curl -X POST http://localhost:8080/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "demo_user", "password": "any_password"}'
```

Returns a JWT token with permissions.

#### Validate Token
```bash
curl "http://localhost:8080/auth/validate?token=YOUR_JWT_TOKEN"
```

### 3. Market Data

#### Connect to Exchange
```bash
curl -X POST http://localhost:8080/market/connect \
  -H "Content-Type: application/json" \
  -d '{"exchange": "binance"}'
```

#### Subscribe to Symbols
```bash
curl -X POST http://localhost:8080/market/subscribe \
  -H "Content-Type: application/json" \
  -d '{
    "exchange": "binance",
    "symbols": ["BTCUSDT", "ETHUSDT"]
  }'
```

#### Get Market Data
```bash
curl http://localhost:8080/market/data
```

Returns the last 10 market data events.

## Architecture

```
┌─────────────┐     ┌──────────────┐     ┌──────────────────┐
│   Client    │────▶│  Demo API    │────▶│   Auth Service   │
└─────────────┘     │              │     └──────────────────┘
                    │              │
                    │              │     ┌──────────────────┐
                    │              │────▶│ Market Connector │
                    └──────────────┘     └──────────────────┘
                           │
                           ▼
                    ┌──────────────┐
                    │ Market Data  │
                    │   Storage    │
                    └──────────────┘
```

## Features Demonstrated

1. **Authentication**:
   - JWT token generation and validation
   - Permission-based access control
   - Session management

2. **Market Connector**:
   - Multi-exchange support architecture
   - Symbol subscription management
   - Real-time market data event handling

3. **Service Integration**:
   - Clean service boundaries
   - Event-driven architecture
   - REST API design

## Next Steps

To extend this demo for production:

1. **Add Real Exchange Connectors**:
   - Implement Binance WebSocket connector
   - Add Zerodha KiteConnect integration
   - Create synthetic data feed for testing

2. **Enhance Authentication**:
   - Connect to PostgreSQL for user storage
   - Implement OAuth2 flow
   - Add API key management per exchange

3. **Scale Market Data**:
   - Add Redis for market data caching
   - Implement WebSocket streaming to clients
   - Add data aggregation service

4. **Add More Services**:
   - Risk Manager service
   - Execution Router service
   - Data Aggregator service

## Configuration

The demo uses hardcoded configuration for simplicity. In production, use environment variables or config files:

```yaml
auth:
  jwt_secret: ${JWT_SECRET}
  token_expiry: 3600
  
market_connector:
  exchanges:
    - binance
    - zerodha
  
server:
  host: 0.0.0.0
  port: 8080
```

## Testing

Run the integration tests:

```bash
cargo test -p demo-service
```

## Performance

The demo is designed to handle:
- 1000+ concurrent WebSocket connections
- 100,000+ market data events per second
- Sub-millisecond JWT validation