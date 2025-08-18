# Logging Service

## Overview
Centralized logging service for ShrivenQuant that collects, processes, and forwards logs from all microservices.

## Status: ✅ Compiles, ❌ Not Integrated

### What's Implemented
- gRPC service definition
- Log aggregation logic
- Batching support
- Proto definitions
- Basic forwarding framework

### What's Missing
- External system integration (Elasticsearch/Loki)
- Actual log forwarding
- Retention policies
- Log rotation
- Performance metrics
- Integration with other services

## Architecture

```
Services → Logging Service → External Systems
         ↓
    Local Buffer
```

## API

### gRPC Endpoints

#### `Log(LogRequest) → LogResponse`
Send a single log entry.

```proto
message LogRequest {
    string service = 1;
    string level = 2;      // TRACE, DEBUG, INFO, WARN, ERROR
    string message = 3;
    string fields = 4;     // JSON encoded
    string trace_id = 5;
    string span_id = 6;
    string correlation_id = 7;
}
```

#### `BatchLog(LogBatch) → LogResponse`
Send multiple log entries at once for efficiency.

#### `GetLogs(GetLogsRequest) → GetLogsResponse`
Query recent logs for a specific service.

## Configuration

Environment variables:
- `ELASTICSEARCH_URL` - Elasticsearch endpoint (not implemented)
- `LOKI_URL` - Loki endpoint (not implemented)
- `LOG_STDOUT` - Enable stdout logging
- `LOG_FILE` - File path for logs (not implemented)

## Running

```bash
cargo run --release -p logging
```

Service listens on port `50058`.

## Integration Status

| Service | Integrated | Notes |
|---------|------------|-------|
| auth | ❌ | Not using centralized logging |
| gateway | ❌ | Not using centralized logging |
| market-connector | ❌ | Not using centralized logging |
| All others | ❌ | Not using centralized logging |

## Known Issues

1. No actual log forwarding implemented
2. No retention or rotation
3. No performance optimization
4. Memory unbounded (could OOM)
5. No compression
6. No authentication
7. Not tested

## TODO

- [ ] Implement Elasticsearch forwarding
- [ ] Implement Loki forwarding
- [ ] Add authentication
- [ ] Add compression
- [ ] Implement retention policies
- [ ] Add metrics
- [ ] Integration tests
- [ ] Load testing
- [ ] Add to all services