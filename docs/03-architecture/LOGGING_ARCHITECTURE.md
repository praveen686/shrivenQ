# ShrivenQuant Logging Architecture

## Current Issues with `/logs` Directory

The `/logs` directory in the root is a **development-only** artifact and should NOT be used in production. Issues:

1. **Not scalable** - Local files don't work in distributed systems
2. **Not persistent** - Lost on container restart
3. **Not searchable** - Can't query across services
4. **Security risk** - PID files expose process information

## Proper Logging Architecture

### 1. Structured Logging
All services use structured JSON logging via the `tracing` crate:
```rust
tracing::info!(
    service = "trading-gateway",
    order_id = %order.id,
    symbol = %order.symbol,
    "Order executed successfully"
);
```

### 2. Centralized Collection
```
Services → Logging Service → External Systems
         ↓
    Local Buffer
    (for resilience)
```

### 3. Log Destinations

#### Development
- Console output (pretty-printed)
- Local files in `/tmp/shrivenquant/logs/` (auto-cleanup)

#### Staging/Production
- **Primary**: Elasticsearch for structured search
- **Secondary**: Loki for cost-effective storage
- **Metrics**: Prometheus (via OpenTelemetry)
- **Traces**: Jaeger/Tempo for distributed tracing

### 4. Log Levels

| Level | Use Case | Example |
|-------|----------|---------|
| TRACE | Detailed debugging | Order book updates |
| DEBUG | Development info | Strategy calculations |
| INFO  | Normal operations | Trade executions |
| WARN  | Potential issues | High latency detected |
| ERROR | Failures requiring attention | Connection lost |

### 5. Correlation IDs

Every request gets a unique correlation ID for tracing across services:
```
correlation_id: "17abc123-4567"
```

### 6. Log Rotation

Production logs are rotated based on:
- Size: 100MB max per file
- Age: 7 days retention
- Count: 10 backup files
- Compression: gzip after rotation

### 7. Migration Plan

1. **Phase 1**: Add structured logging to all services ✅
2. **Phase 2**: Deploy logging service
3. **Phase 3**: Remove `/logs` directory
4. **Phase 4**: Integrate with Elasticsearch/Loki

### 8. Security Considerations

- No sensitive data in logs (passwords, API keys, PII)
- Encrypted transport (TLS) to external systems
- Role-based access control for log viewing
- Audit logging for compliance

### 9. Performance Impact

- Async logging to prevent blocking
- Batched forwarding to external systems
- Local buffering for resilience
- Sampling for high-frequency events

## Directory Structure

```
/tmp/shrivenquant/logs/        # Development only
├── service-name/
│   ├── current.log
│   └── archived/
│       ├── 2024-01-15.log.gz
│       └── 2024-01-14.log.gz

/var/log/shrivenquant/          # Production (container-local)
├── service-name/
│   └── structured.json         # Collected by log agent
```

## Environment Variables

```bash
# Log level
RUST_LOG=info,shrivenquant=debug

# Log destination
LOG_OUTPUT=json  # or "pretty" for development

# External systems
ELASTICSEARCH_URL=https://logs.shrivenquant.internal
LOKI_URL=https://loki.shrivenquant.internal
```

## Removing `/logs` Directory

The `/logs` directory should be removed after:
1. Logging service is deployed
2. All services are configured to use centralized logging
3. Monitoring dashboards are updated

For now, it remains for backward compatibility during the transition.