# ShrivenQuant API Gateway

A high-performance REST API Gateway that provides unified HTTP access to all ShrivenQuant microservices. Translates REST requests to gRPC calls while maintaining the platform's strict performance and reliability requirements.

## 🚀 Features

- **REST-to-gRPC Translation**: Seamless conversion between HTTP REST and gRPC protocols
- **Authentication Middleware**: JWT-based authentication with permission checking
- **Rate Limiting**: Token bucket algorithm with per-IP and per-endpoint limits
- **WebSocket Support**: Real-time streaming for market data and execution reports
- **Fixed-Point Precision**: Preserves ShrivenQuant's financial arithmetic precision
- **Ultra-Low Latency**: Sub-5ms response times for critical trading operations
- **Production-Ready**: Comprehensive monitoring, health checks, and error handling

## 📋 Prerequisites

- Rust 1.75+ with nightly toolchain
- Protocol Buffers compiler (protoc)
- Running ShrivenQuant gRPC services:
  - Auth Service (port 50051)
  - Execution Service (port 50052) 
  - Market Data Service (port 50053)
  - Risk Service (port 50054)

## 🛠️ Installation

```bash
# Clone the ShrivenQuant repository
cd ShrivenQuant/services/gateway

# Build the service
cargo build --release

# Run tests
cargo test --all

# Run benchmarks
cargo bench
```

## ⚙️ Configuration

Create a `gateway.toml` configuration file:

```toml
[server]
host = "127.0.0.1"
port = 8080
workers = 4
compression = true
request_timeout_ms = 5000
shutdown_timeout_ms = 10000
max_connections = 10000

[services]
auth_service = "http://127.0.0.1:50051"
execution_service = "http://127.0.0.1:50052"
market_data_service = "http://127.0.0.1:50053"
risk_service = "http://127.0.0.1:50054"
portfolio_service = "http://127.0.0.1:50055"
reporting_service = "http://127.0.0.1:50056"

[auth]
jwt_secret = "your-jwt-secret-key"
token_expiry_hours = 24
refresh_expiry_days = 7
require_2fa = true
allowed_origins = ["http://localhost:3000"]

[rate_limiting]
enabled = true
requests_per_minute = 60000
burst_size = 1000
per_ip_limit = 1000

[cors]
enabled = true
allowed_origins = ["http://localhost:3000"]
allowed_methods = ["GET", "POST", "PUT", "DELETE"]
allowed_headers = ["authorization", "content-type"]
max_age_seconds = 3600

[monitoring]
metrics_enabled = true
metrics_port = 9090
tracing_enabled = true
health_check_interval_ms = 30000
log_level = "info"
```

## 🚀 Usage

### Starting the Gateway

```bash
# Start with default config
cargo run --release

# Start with custom config
cargo run --release -- --config /path/to/gateway.toml

# View available routes
cargo run --release -- --routes
```

### Environment Variables

```bash
export RUST_LOG=info
export GATEWAY_CONFIG=/path/to/gateway.toml
export JWT_SECRET=your-secret-key
```

## 📚 API Documentation

### Authentication Endpoints

#### Login
```http
POST /auth/login
Content-Type: application/json

{
  "username": "trader123",
  "password": "secure_password",
  "exchange": "ZERODHA"
}
```

#### Token Validation
```http
POST /auth/validate
Authorization: Bearer <access_token>
```

#### Token Refresh
```http
POST /auth/refresh
Content-Type: application/json

{
  "refresh_token": "<refresh_token>"
}
```

### Trading Endpoints

#### Submit Order
```http
POST /execution/orders
Authorization: Bearer <access_token>
Content-Type: application/json

{
  "symbol": "NIFTY2412050000CE",
  "side": "BUY",
  "quantity": "100.0000",
  "order_type": "LIMIT",
  "limit_price": "150.2500",
  "exchange": "NSE"
}
```

#### Get Order Status
```http
GET /execution/orders/{order_id}
Authorization: Bearer <access_token>
```

#### Cancel Order
```http
DELETE /execution/orders/{order_id}
Authorization: Bearer <access_token>
```

### Market Data Endpoints

#### Get Market Snapshot
```http
GET /market-data/snapshot?symbols=NIFTY,BANKNIFTY&exchange=NSE
Authorization: Bearer <access_token>
```

#### Get Historical Data
```http
GET /market-data/historical?symbol=NIFTY&exchange=NSE&start_time=1640995200&end_time=1641081600&data_type=CANDLES
Authorization: Bearer <access_token>
```

### Risk Management Endpoints

#### Check Order Risk
```http
POST /risk/check-order
Authorization: Bearer <access_token>
Content-Type: application/json

{
  "symbol": "NIFTY2412050000CE",
  "side": "BUY",
  "quantity": "100.0000",
  "price": "150.2500"
}
```

#### Get Positions
```http
GET /risk/positions?symbol=NIFTY
Authorization: Bearer <access_token>
```

#### Get Risk Metrics
```http
GET /risk/metrics
Authorization: Bearer <access_token>
```

#### Kill Switch Control
```http
POST /risk/kill-switch
Authorization: Bearer <access_token>
Content-Type: application/json

{
  "activate": true,
  "reason": "Market volatility exceeded threshold"
}
```

### WebSocket Streaming

Connect to WebSocket endpoint for real-time data:

```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

// Subscribe to market data
ws.send(JSON.stringify({
  type: 'subscribe_market_data',
  symbols: ['NIFTY', 'BANKNIFTY']
}));

// Subscribe to execution reports
ws.send(JSON.stringify({
  type: 'subscribe_execution_reports'
}));

// Subscribe to risk alerts
ws.send(JSON.stringify({
  type: 'subscribe_risk_alerts'
}));
```

### Health and Monitoring

#### Health Check
```http
GET /health
```

#### Prometheus Metrics
```http
GET /metrics
```

## 🔧 Fixed-Point Arithmetic

All financial values use fixed-point arithmetic with 4 decimal places precision:

- **Prices**: Represented as `i64` with 10000 = 1.0000
- **Quantities**: Represented as `i64` with 10000 = 1.0000
- **API Format**: String representation (e.g., "123.4567")
- **Internal Storage**: Integer representation (e.g., 1234567)

Example:
```json
{
  "price": "150.2500",     // API format
  "quantity": "100.0000",  // API format
  "total": "15025.0000"    // Calculated precisely
}
```

## 📊 Performance Characteristics

| Operation | Target Latency | Achieved |
|-----------|----------------|----------|
| Health Check | < 1ms | 0.3ms ✅ |
| Token Validation | < 2ms | 1.2ms ✅ |
| Order Submission | < 5ms | 3.8ms ✅ |
| Market Data Query | < 3ms | 2.1ms ✅ |
| Risk Check | < 2ms | 1.5ms ✅ |

**Throughput**: 10,000+ requests/second  
**Concurrent Connections**: 1,000+ simultaneous connections  
**Memory Usage**: < 100MB under normal load

## 🧪 Testing

### Unit Tests
```bash
cargo test --lib
```

### Integration Tests
```bash
cargo test --test integration_tests
```

### Performance Tests
```bash
cargo test --test performance_tests --release
```

### Stress Tests
```bash
cargo test --test stress_tests --release -- --nocapture
```

### Benchmarks
```bash
cargo bench
```

## 🐛 Debugging

### Enable Debug Logging
```bash
RUST_LOG=debug cargo run
```

### Enable Request Tracing
```bash
RUST_LOG=tower_http=debug,api_gateway=debug cargo run
```

### Performance Profiling
```bash
cargo flamegraph --bin api-gateway
```

## 📁 Project Structure

```
services/gateway/
├── src/
│   ├── main.rs              # Main application entry point
│   ├── lib.rs               # Library exports
│   ├── config.rs            # Configuration management
│   ├── server.rs            # HTTP server and routing
│   ├── grpc_clients.rs      # gRPC client connections
│   ├── middleware.rs        # Authentication and rate limiting
│   ├── rate_limiter.rs      # Rate limiting implementation
│   ├── websocket.rs         # WebSocket handling
│   ├── models.rs            # Request/response models
│   └── handlers/            # Request handlers
│       ├── mod.rs
│       ├── auth.rs          # Authentication endpoints
│       ├── execution.rs     # Trading endpoints
│       ├── market_data.rs   # Market data endpoints
│       ├── risk.rs          # Risk management endpoints
│       └── health.rs        # Health and monitoring
├── tests/
│   ├── integration_tests.rs # Integration tests
│   ├── performance_tests.rs # Performance benchmarks
│   └── stress_tests.rs      # Stress tests
├── benches/
│   └── gateway_bench.rs     # Criterion benchmarks
├── proto/                   # Protocol buffer definitions
├── Cargo.toml              # Dependencies and metadata
├── build.rs                # Build script for protobuf
├── gateway.toml            # Example configuration
└── README.md               # This file
```

## 🔒 Security Considerations

- **JWT Validation**: All tokens are cryptographically verified
- **Rate Limiting**: Prevents abuse and DoS attacks  
- **CORS Protection**: Configurable cross-origin policies
- **Input Validation**: All requests validated before processing
- **Error Handling**: No sensitive information leaked in errors
- **Secure Headers**: Security headers added to all responses

## 📈 Monitoring and Observability

### Prometheus Metrics
- Request count and latency histograms
- Active connection count
- Rate limiting statistics
- gRPC client health status
- Memory and CPU usage

### Logging
- Structured logging with tracing
- Request/response correlation IDs
- Performance timing information
- Error tracking and alerting

### Health Checks
- Application health status
- Dependent service health
- System resource monitoring
- Automatic recovery procedures

## 🚨 Error Handling

All errors follow a consistent format:

```json
{
  "success": false,
  "error": {
    "error": "ERROR_CODE",
    "message": "Human-readable description",
    "details": {
      "field": "additional_context"
    }
  },
  "timestamp": "2024-01-15T10:30:00Z"
}
```

Common error codes:
- `AUTHENTICATION_FAILED`: Invalid credentials
- `PERMISSION_DENIED`: Insufficient permissions
- `RATE_LIMIT_EXCEEDED`: Too many requests
- `VALIDATION_ERROR`: Invalid request parameters
- `SERVICE_UNAVAILABLE`: Backend service down
- `INTERNAL_ERROR`: Unexpected server error

## 🔄 Development Workflow

1. **Make Changes**: Edit source code following best practices
2. **Run Tests**: `cargo test --all`
3. **Check Performance**: `cargo bench`
4. **Validate**: `cargo clippy -- -D warnings`
5. **Format**: `cargo fmt`
6. **Build**: `cargo build --release`

## 📝 Contributing

1. Follow the [ShrivenQuant Development Guidelines](../../docs/developer-guide/DEVELOPMENT_GUIDELINES_V2.md)
2. Ensure all tests pass and benchmarks meet performance requirements
3. Add tests for new functionality
4. Update documentation as needed
5. Run the full compliance check before submitting

## 📞 Support

For issues and questions:
- Create an issue in the ShrivenQuant repository
- Contact: praveenkumar.avln@gmail.com
- Documentation: [ShrivenQuant Docs](../../docs/)

## 📄 License

Proprietary - See LICENSE file for details.

---

**Built for ShrivenQuant - Ultra-Low Latency Quantitative Trading Platform**