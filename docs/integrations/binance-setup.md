# Binance Integration Guide

## Overview

ShrivenQuant provides comprehensive Binance integration supporting both Spot and Futures markets with automated authentication, WebSocket data streaming, and order management. Access all functionality through the unified API Gateway's REST interface.

## Features

### Supported Markets
- **Spot Trading**: Full spot market support with real-time data
- **USD-M Futures**: Perpetual and quarterly futures (USDT margined)
- **COIN-M Futures**: Inverse perpetual contracts (coin margined)

### Core Capabilities
- ✅ Automated HMAC-SHA256 signature generation
- ✅ Listen key management for WebSocket authentication
- ✅ Real-time market data via WebSocket
- ✅ Order placement and management
- ✅ Account balance monitoring
- ✅ Testnet and mainnet support
- ✅ gRPC framework integration
- ✅ Session caching (60-minute listen keys)
- ✅ **Unified API Gateway**: REST interface for all Binance functionality
- ✅ **Rate Limiting**: Automatic Binance API limit management
- ✅ **Fixed-Point Precision**: Exact financial calculations

## Configuration

### Environment Variables

Add to your `.env` file:

```env
# Binance Spot
BINANCE_SPOT_API_KEY=your_spot_api_key
BINANCE_SPOT_API_SECRET=your_spot_api_secret

# Binance Futures (USD-M)
BINANCE_FUTURES_API_KEY=your_futures_api_key
BINANCE_FUTURES_API_SECRET=your_futures_api_secret

# Binance COIN-M Futures
BINANCE_COIN_FUTURES_API_KEY=your_coin_futures_api_key
BINANCE_COIN_FUTURES_API_SECRET=your_coin_futures_api_secret

# Testnet Mode (defaults to true for safety)
BINANCE_TESTNET=true

# JWT Configuration
JWT_SECRET=your-jwt-secret
TOKEN_EXPIRY=3600
```

### Testnet Setup

1. **Create Testnet Account**:
   - Visit https://testnet.binance.vision/
   - Register a new account or login
   - Generate API keys for testing

2. **Activate Testnet**:
   - Testnet accounts come with test funds
   - API keys work immediately after creation
   - No KYC required for testnet

3. **Available Test Assets**:
   - BTC, ETH, USDT, BNB (with test balances)
   - All major trading pairs available
   - Realistic market data for testing

## Quick Start with API Gateway

### Authentication via API Gateway

```http
POST /auth/login
Content-Type: application/json

{
  "username": "your_binance_username",
  "password": "your_password", 
  "exchange": "BINANCE"
}
```

Response:
```json
{
  "success": true,
  "data": {
    "access_token": "eyJ...",
    "refresh_token": "eyJ...", 
    "expires_in": 3600,
    "user": {
      "username": "your_binance_username",
      "exchange": "BINANCE",
      "permissions": ["PLACE_ORDERS", "VIEW_POSITIONS"]
    }
  }
}
```

### Submit Binance Order via API Gateway

```http
POST /execution/orders
Authorization: Bearer <access_token>
Content-Type: application/json

{
  "symbol": "BTCUSDT",
  "side": "BUY", 
  "quantity": "0.0010",
  "order_type": "LIMIT",
  "limit_price": "50000.0000",
  "exchange": "BINANCE"
}
```

### Get Market Data via API Gateway

```http
GET /market-data/snapshot?symbols=BTCUSDT,ETHUSDT&exchange=BINANCE
Authorization: Bearer <access_token>
```

### WebSocket via API Gateway

```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

// Subscribe to Binance market data
ws.send(JSON.stringify({
  type: 'subscribe_market_data',
  symbols: ['BTCUSDT', 'ETHUSDT'],
  exchange: 'BINANCE'
}));
```

## Direct Service Usage Examples

### Basic Authentication

```rust
use auth_service::providers::binance_enhanced::{
    BinanceAuth, BinanceConfig, BinanceEndpoint
};

// Load from environment
let config = BinanceConfig::from_env_file(BinanceEndpoint::Spot)?;
let auth = BinanceAuth::new(config);

// Test connectivity
auth.ping().await?;

// Get account info
let account = auth.get_account_info().await?;
println!("Can trade: {}", account.can_trade);
```

### WebSocket Integration

```rust
// Create listen key for user data
let listen_key = auth.create_listen_key().await?;

// Get WebSocket URLs
let market_ws = auth.get_market_ws_url(&[
    "btcusdt@depth",
    "btcusdt@trade",
    "ethusdt@ticker"
]);

let user_ws = auth.get_user_ws_url().await?;

// Listen key auto-renewal (every 30 minutes)
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(1800)).await;
        auth.keepalive_listen_key().await.ok();
    }
});
```

### Order Management

```rust
// Test order (validation only)
auth.test_order(
    "BTCUSDT",    // symbol
    "BUY",        // side
    "LIMIT",      // type
    0.001,        // quantity
    Some(30000.0) // price
).await?;

// Place real order
let order = auth.place_order(
    "BTCUSDT",
    "BUY",
    "LIMIT",
    0.001,
    Some(30000.0)
).await?;

println!("Order ID: {}", order.order_id);
```

### gRPC Service

```rust
use auth_service::binance_service::create_binance_service;

// Create service
let service = create_binance_service().await?;

// Authenticate (username format: binance_spot or binance_futures)
let context = service.authenticate("binance_spot", "").await?;

// Generate JWT token
let token = service.generate_token(&context).await?;

// Check permissions
let can_trade = service.check_permission(
    &context, 
    Permission::PlaceOrders
).await;
```

## Architecture

### Component Structure

```
services/auth/
├── src/
│   ├── providers/
│   │   ├── binance_enhanced.rs    # Core Binance implementation
│   │   └── mod.rs                 # Provider traits
│   ├── binance_service.rs         # gRPC integration
│   └── lib.rs                      # Service definitions
└── examples/
    ├── binance_simple_usage.rs    # Basic example
    └── binance_testnet_test.rs    # Comprehensive test
```

### Key Components

1. **BinanceAuth**: Core authentication handler
   - HMAC signature generation
   - Request building and signing
   - Listen key management
   - Account info caching

2. **BinanceConfig**: Configuration management
   - Environment variable loading
   - Endpoint selection (Spot/Futures)
   - Testnet/Mainnet switching

3. **BinanceService**: gRPC integration
   - JWT token generation
   - Permission management
   - Multi-market support

## API Endpoints

### REST Endpoints

| Function | Spot | USD-M Futures | COIN-M Futures |
|----------|------|---------------|----------------|
| Connectivity | `/api/v3/ping` | `/fapi/v1/ping` | `/dapi/v1/ping` |
| Server Time | `/api/v3/time` | `/fapi/v1/time` | `/dapi/v1/time` |
| Account Info | `/api/v3/account` | `/fapi/v2/account` | `/dapi/v1/account` |
| Listen Key | `/api/v3/userDataStream` | `/fapi/v1/listenKey` | `/dapi/v1/listenKey` |
| Order | `/api/v3/order` | `/fapi/v1/order` | `/dapi/v1/order` |

### WebSocket Streams

```
# Market Data
wss://stream.binance.com:9443/ws/<symbol>@<stream>

# User Data
wss://stream.binance.com:9443/ws/<listenKey>

# Testnet Market Data
wss://testnet.binance.vision/ws/<symbol>@<stream>

# Testnet User Data  
wss://testnet.binance.vision/ws/<listenKey>
```

## Testing

### Run Tests

```bash
# Basic connectivity test
cargo run --example binance_simple_usage

# Comprehensive testnet test
cargo run --example binance_testnet_test

# Integration tests
cargo test -p auth-service --test binance_integration
```

### Test Coverage

- ✅ API connectivity
- ✅ Credential validation
- ✅ Account info retrieval
- ✅ Listen key creation/renewal
- ✅ Order validation
- ✅ WebSocket URL generation
- ✅ gRPC authentication flow

## Security Best Practices

1. **API Key Management**:
   - Never commit API keys to version control
   - Use environment variables or secure vaults
   - Rotate keys regularly
   - Use IP whitelist when possible

2. **Signature Security**:
   - HMAC-SHA256 signatures generated server-side
   - Timestamp validation (5-second window)
   - Receive window configuration

3. **Listen Key Management**:
   - Auto-renewal every 30 minutes
   - Graceful reconnection on expiry
   - Separate keys for different streams

4. **Rate Limiting**:
   - Respect Binance rate limits
   - Implement exponential backoff
   - Cache account info when possible

## Troubleshooting

### Common Issues

1. **Invalid Signature Error**:
   ```
   Solution: Ensure API secret is correct and timestamp is synchronized
   ```

2. **Listen Key Expired**:
   ```
   Solution: Implement auto-renewal with keepalive every 30 minutes
   ```

3. **Testnet Not Working**:
   ```
   Solution: Activate testnet account at https://testnet.binance.vision/
   ```

4. **Permission Denied**:
   ```
   Solution: Enable trading permissions in API key settings
   ```

### Debug Mode

Enable detailed logging:

```rust
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .init();
```

## Performance Optimization

1. **Connection Pooling**: Reuse HTTP client instances
2. **Listen Key Caching**: 60-minute validity
3. **Account Info Caching**: Reduce API calls
4. **Batch Operations**: Group multiple requests
5. **WebSocket Multiplexing**: Single connection for multiple streams

## Migration from Python

### Key Differences

| Feature | Python (ccxt) | Rust Implementation |
|---------|--------------|---------------------|
| Authentication | Manual | Automated with session caching |
| WebSocket | Separate library | Integrated |
| Type Safety | Runtime | Compile-time |
| Performance | ~100ms latency | <10ms latency |
| Memory Usage | ~50MB | ~5MB |

### Migration Path

1. Replace Python auth code with Rust service
2. Update WebSocket connections to use listen keys
3. Migrate order management to typed structs
4. Implement gRPC clients for other services

## Advanced Features

### Multi-Account Support

```rust
// Create multiple auth instances
let spot_auth = BinanceAuth::new(spot_config);
let futures_auth = BinanceAuth::new(futures_config);

// Use appropriate instance for each market
let spot_account = spot_auth.get_account_info().await?;
let futures_account = futures_auth.get_futures_account_info().await?;
```

### Custom Request Signing

```rust
// Build custom signed request
let mut params = BTreeMap::new();
params.insert("symbol", "BTCUSDT");
params.insert("interval", "1m");
params.insert("limit", "100");

let signed_url = auth.build_signed_request("/api/v3/klines", &mut params);
```

### Error Handling

```rust
match auth.place_order(...).await {
    Ok(order) => {
        info!("Order placed: {}", order.order_id);
    }
    Err(e) => {
        if e.to_string().contains("-2010") {
            // Insufficient balance
            warn!("Insufficient balance for order");
        } else if e.to_string().contains("-1021") {
            // Invalid timestamp
            error!("Time sync issue, retrying...");
        } else {
            error!("Order failed: {}", e);
        }
    }
}
```

## Support

For issues or questions:
1. Check the [troubleshooting](#troubleshooting) section
2. Review example code in `services/auth/examples/`
3. Enable debug logging for detailed traces
4. Test with testnet first before mainnet

## Related Documentation

- [API Gateway README](../services/gateway/README.md) - Complete REST API documentation
- [Authentication Service Architecture](./architecture/README.md)
- [Zerodha Integration Guide](./ZERODHA_INTEGRATION_GUIDE.md) 
- [Portfolio Manager Integration](./PORTFOLIO_MANAGER_INTEGRATION.md)
- [Development Guidelines](./developer-guide/DEVELOPMENT_GUIDELINES_V2.md)
- [Migration Status](./architecture/UNIFIED_MIGRATION_STATUS.md)

---

**Contact**: praveenkumar.avln@gmail.com  
**Last Updated**: 2025-08-14  
**API Gateway Version**: 1.0.0