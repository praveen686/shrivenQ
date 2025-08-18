# ShrivenQuant Architecture

## Overview

ShrivenQuant uses a microservices architecture with gRPC for inter-service communication. The system is designed for algorithmic trading but is currently in early development.

## Architecture Principles

1. **Microservices**: Each service has a single responsibility
2. **gRPC Communication**: Protocol buffers for service communication
3. **Rust First**: All services written in Rust for performance
4. **Event Driven**: Asynchronous message passing

## System Components

### Service Layer

```
┌─────────────────────────────────────────────────────────┐
│                    API Gateway                           │
│                  (REST → gRPC)                          │
└─────────────────────────────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        ▼                   ▼                   ▼
┌──────────────┐   ┌──────────────┐   ┌──────────────┐
│Auth Service  │   │Market Data   │   │Risk Manager  │
└──────────────┘   └──────────────┘   └──────────────┘
        │                   │                   │
        ▼                   ▼                   ▼
┌──────────────┐   ┌──────────────┐   ┌──────────────┐
│Execution     │   │Portfolio     │   │OMS           │
│Router        │   │Manager       │   │              │
└──────────────┘   └──────────────┘   └──────────────┘
```

### Data Flow

```
Market Data → Orderbook → Strategies → Risk Check → Execution
     ↓            ↓           ↓            ↓           ↓
  Storage    Analytics    Signals    Monitoring    Reporting
```

## Service Descriptions

### Core Trading Services

| Service | Port | Purpose | Status |
|---------|------|---------|--------|
| gateway | 8080 | REST API gateway | Framework |
| auth | 50051 | Authentication | Basic impl |
| market-connector | 50052 | Exchange connectivity | Untested |
| risk-manager | 50053 | Risk management | Framework |
| execution-router | 50054 | Order routing | Framework |
| oms | 50060 | Order management | Framework |
| trading-gateway | 50061 | Strategy orchestration | Framework |

### Analytics Services

| Service | Port | Purpose | Status |
|---------|------|---------|--------|
| data-aggregator | 50062 | Data processing | Basic |
| portfolio-manager | 50063 | Portfolio optimization | Basic |
| reporting | 50064 | Performance analytics | Minimal |
| orderbook | 50065 | Order book management | Basic |
| options-engine | 50055 | Options pricing | Working |

### Infrastructure Services

| Service | Port | Purpose | Status |
|---------|------|---------|--------|
| logging | 50058 | Centralized logging | Framework |
| monitoring | 50059 | System monitoring | Stub |
| secrets-manager | N/A | Credential encryption | CLI only |
| discovery | 50066 | Service discovery | Stub |

### ML/AI Services

| Service | Port | Purpose | Status |
|---------|------|---------|--------|
| ml-inference | 50056 | ML predictions | No models |
| sentiment-analyzer | 50057 | Social sentiment | No API keys |

## Communication Patterns

### Synchronous (gRPC)
- Service-to-service calls
- Request/response pattern
- ~1-5ms latency

### Asynchronous (Event Bus)
- Market data distribution
- Order updates
- Risk alerts

## Data Storage

Currently no persistent storage implemented. Planned:
- TimescaleDB for time-series data
- PostgreSQL for transactional data
- Redis for caching
- S3 for historical data

## Security

### Current State
- Basic JWT tokens (not validated)
- No TLS/mTLS
- No service authentication
- Secrets in code

### Required for Production
- mTLS between services
- OAuth2/OIDC for users
- Vault for secrets
- Network segmentation

## Performance Characteristics

### Current (Theoretical)
- Options pricing: ~100ns
- Risk checks: Framework only
- Order routing: Not measured

### Target
- Market data: <100μs latency
- Risk checks: <1ms
- Order execution: <5ms end-to-end

## Deployment

### Current
- Local development only
- No containerization
- No orchestration

### Target
- Kubernetes deployment
- Service mesh (Istio)
- Multi-region support

## Known Architectural Issues

1. **No Service Discovery**: Services use hardcoded addresses
2. **No Circuit Breakers**: Services will cascade failures
3. **No Load Balancing**: Single instance per service
4. **No Rate Limiting**: Vulnerable to overload
5. **No Caching**: Every request hits backend
6. **No Database**: Everything in memory
7. **No Message Queue**: Direct coupling

## Migration Path to Production

### Phase 1: Stabilization (Current)
- Remove unwrap() calls
- Add error handling
- Implement logging

### Phase 2: Integration
- Connect services
- Add databases
- Implement caching

### Phase 3: Hardening
- Add monitoring
- Implement security
- Performance testing

### Phase 4: Production
- Kubernetes deployment
- Multi-region setup
- Disaster recovery

## Conclusion

The architecture provides a solid foundation but requires significant work before production use. Current state is suitable for development and learning only.