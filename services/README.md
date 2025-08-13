# ShrivenQuant Microservices

## Service Architecture

### Core Services

#### 1. Gateway Service (`/gateway`)
- API Gateway and request routing
- Rate limiting and authentication
- WebSocket management for real-time data

#### 2. Auth Service (`/auth`)
- User authentication and authorization
- JWT token management
- API key validation
- Permission management

#### 3. Market Connector (`/market-connector`)
- Exchange connectivity management
- Order routing to multiple venues
- Market data normalization
- Connection pooling and failover

#### 4. Data Aggregator (`/data-aggregator`)
- Real-time data aggregation
- Order book reconstruction
- Trade and quote processing
- Data distribution to consumers

#### 5. Risk Manager (`/risk-manager`)
- Pre-trade risk checks
- Position limit enforcement
- Exposure monitoring
- Real-time P&L calculation
- Margin requirement validation

#### 6. Execution Router (`/execution-router`)
- Smart order routing
- Execution algorithm selection
- Order splitting and timing
- Transaction cost analysis

#### 7. Portfolio Manager (`/portfolio-manager`)
- Portfolio optimization
- Asset allocation
- Rebalancing logic
- Performance attribution

#### 8. Reporting Service (`/reporting`)
- Trade reporting
- Regulatory compliance reports
- Performance analytics
- Custom report generation

## Communication Patterns

- **gRPC**: Inter-service communication
- **Redis**: Pub/Sub for real-time events
- **Kafka**: Event streaming for audit trail
- **REST**: External API endpoints

## Deployment

Each service is containerized and deployed via Kubernetes.
See `/infrastructure/kubernetes/` for deployment manifests.