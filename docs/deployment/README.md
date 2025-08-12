# Deployment Guide

## Overview

This guide covers deploying ShrivenQuant in various environments, from development to production trading infrastructure.

## Quick Start (Development)

### Local Development Setup

```bash
#!/bin/bash
# ShrivenQuant Trading Platform - Local Development Setup
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Quick setup for local development environment
# USAGE: Run once for initial setup, then use for daily development
# SAFETY: Configured for paper trading by default

# Clone and build
git clone https://github.com/praveen686/shrivenQ.git
cd ShrivenQuant
cargo build --release

# Configuration
cp config.example.toml config.toml
# Edit config.toml with your settings

# Set environment variables
export KITE_API_KEY="your_zerodha_key"
export BINANCE_API_KEY="your_binance_key"

# Run in paper trading mode
cargo run --release -- --config config.toml --mode paper
```

### Docker Development

```bash
#!/bin/bash
# ShrivenQuant Trading Platform - Docker Development Setup
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Containerized development environment setup
# USAGE: Docker-based development and testing
# SAFETY: Isolated environment with paper trading mode

# Build image
docker build -t shrivenquant:latest .

# Run container
docker run -d \
  --name shrivenq-dev \
  -p 8080:8080 \
  -e RUST_LOG=info \
  -e MODE=paper \
  -v $(pwd)/config:/app/config \
  shrivenquant:latest
```

## Production Deployment

### Prerequisites

#### Hardware Requirements

**Minimum (Paper Trading)**
- CPU: 4 cores, 2.5+ GHz
- RAM: 8GB
- Storage: 100GB SSD
- Network: 100 Mbps

**Recommended (Live Trading)**
- CPU: 16+ cores, 3.0+ GHz (Intel Xeon or AMD EPYC)
- RAM: 64GB DDR4-3200
- Storage: 1TB NVMe SSD
- Network: 1 Gbps low-latency connection
- OS: Linux (Ubuntu 22.04 LTS recommended)

**High-Frequency Trading**
- CPU: 32+ cores, 4.0+ GHz
- RAM: 128GB ECC
- Storage: 2TB NVMe SSD (enterprise grade)
- Network: 10 Gbps dedicated line
- NUMA-optimized configuration

#### Software Requirements

```bash
#!/bin/bash
# ShrivenQuant Trading Platform - Production System Setup
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Production system setup with performance optimizations
# USAGE: Run once on production servers for optimal configuration
# SAFETY: Includes system-level optimizations for low latency

# Ubuntu 22.04 LTS setup
sudo apt update && sudo apt upgrade -y
sudo apt install -y build-essential pkg-config libssl-dev

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default nightly

# System optimization for low latency
echo 'vm.swappiness=1' | sudo tee -a /etc/sysctl.conf
echo 'kernel.sched_rt_runtime_us=-1' | sudo tee -a /etc/sysctl.conf
sudo sysctl -p
```

### Production Configuration

#### System Configuration (`config.toml`)

```toml
[engine]
mode = "live"  # WARNING: Use "paper" for testing
venue = "zerodha"
max_positions = 100
max_orders_per_sec = 500
risk_check_enabled = true
metrics_enabled = true
memory_pool_size = 134217728  # 128MB

[risk]
max_position_size = 10000
max_position_value = 10000000    # 1 crore in paise
max_total_exposure = 100000000   # 10 crore in paise
max_order_size = 1000
max_order_value = 1000000        # 10 lakh in paise
max_orders_per_minute = 1000
max_daily_loss = -1000000        # 10 lakh stop loss
max_drawdown = -2000000          # 20 lakh max drawdown

[data]
wal_dir = "/data/wal"
backup_dir = "/backup/wal"
retention_days = 90
compression = "lz4"

[monitoring]
metrics_port = 9090
health_port = 8080
log_level = "info"
log_file = "/logs/shrivenq.log"

[alerts]
telegram_enabled = true
telegram_token = "${TELEGRAM_TOKEN}"
telegram_chat_id = "${TELEGRAM_CHAT_ID}"
email_enabled = true
smtp_server = "smtp.gmail.com"
email_from = "${ALERT_EMAIL}"
```

#### Environment Variables (`.env`)

```env
# Trading Credentials (NEVER commit to git)
KITE_API_KEY=your_zerodha_api_key
KITE_API_SECRET=your_zerodha_api_secret
KITE_USER_ID=your_zerodha_user_id
KITE_PASSWORD=your_zerodha_password
KITE_PIN=your_zerodha_pin

BINANCE_API_KEY=your_binance_api_key
BINANCE_API_SECRET=your_binance_api_secret
BINANCE_TESTNET=false

# Database
DATABASE_URL=postgresql://user:pass@localhost/shrivenq
REDIS_URL=redis://localhost:6379

# Monitoring
TELEGRAM_TOKEN=your_telegram_bot_token
TELEGRAM_CHAT_ID=your_telegram_chat_id
ALERT_EMAIL=alerts@yourcompany.com

# Performance Tuning
RUST_LOG=info
TOKIO_WORKER_THREADS=16
RAYON_NUM_THREADS=16
```

### Docker Production Setup

#### Multi-stage Dockerfile

```dockerfile
# Build stage
FROM rust:1.75-slim as builder

WORKDIR /app
COPY . .

RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/shrivenq /app/
COPY --from=builder /app/config/ /app/config/

# Create non-root user
RUN useradd -r -s /bin/false trader
RUN chown -R trader:trader /app
USER trader

EXPOSE 8080 9090

CMD ["./shrivenq", "--config", "config/production.toml"]
```

#### Docker Compose

```yaml
version: '3.8'

services:
  shrivenq:
    build: .
    container_name: shrivenq
    restart: unless-stopped
    ports:
      - "8080:8080"   # Health check
      - "9090:9090"   # Metrics
    environment:
      - RUST_LOG=info
      - MODE=paper    # Change to 'live' for production
    volumes:
      - ./data:/data
      - ./logs:/logs
      - ./config:/app/config:ro
    env_file:
      - .env
    depends_on:
      - postgres
      - redis
    networks:
      - trading-net

  postgres:
    image: postgres:15-alpine
    container_name: shrivenq-db
    restart: unless-stopped
    environment:
      POSTGRES_DB: shrivenq
      POSTGRES_USER: trader
      POSTGRES_PASSWORD: ${DB_PASSWORD}
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./sql/init.sql:/docker-entrypoint-initdb.d/init.sql
    networks:
      - trading-net

  redis:
    image: redis:7-alpine
    container_name: shrivenq-cache
    restart: unless-stopped
    volumes:
      - redis_data:/data
    networks:
      - trading-net

  prometheus:
    image: prom/prometheus:latest
    container_name: prometheus
    ports:
      - "9090:9090"
    volumes:
      - ./monitoring/prometheus.yml:/etc/prometheus/prometheus.yml
      - prometheus_data:/prometheus
    networks:
      - trading-net

  grafana:
    image: grafana/grafana:latest
    container_name: grafana
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=${GRAFANA_PASSWORD}
    volumes:
      - grafana_data:/var/lib/grafana
      - ./monitoring/grafana:/etc/grafana/provisioning
    networks:
      - trading-net

volumes:
  postgres_data:
  redis_data:
  prometheus_data:
  grafana_data:

networks:
  trading-net:
    driver: bridge
```

### Kubernetes Deployment

#### Namespace and ConfigMap

```yaml
# namespace.yaml
apiVersion: v1
kind: Namespace
metadata:
  name: trading

---
# configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: shrivenq-config
  namespace: trading
data:
  config.toml: |
    [engine]
    mode = "live"
    venue = "zerodha"
    max_positions = 100

    [risk]
    max_daily_loss = -1000000
    max_drawdown = -2000000

    [monitoring]
    metrics_port = 9090
    health_port = 8080
```

#### Deployment

```yaml
# deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: shrivenq
  namespace: trading
  labels:
    app: shrivenq
spec:
  replicas: 2
  selector:
    matchLabels:
      app: shrivenq
  template:
    metadata:
      labels:
        app: shrivenq
    spec:
      containers:
      - name: shrivenq
        image: shrivenquant:latest
        ports:
        - containerPort: 8080
          name: health
        - containerPort: 9090
          name: metrics
        env:
        - name: RUST_LOG
          value: "info"
        - name: KITE_API_KEY
          valueFrom:
            secretKeyRef:
              name: trading-secrets
              key: kite-api-key
        volumeMounts:
        - name: config
          mountPath: /app/config
        - name: data
          mountPath: /data
        resources:
          requests:
            cpu: "4"
            memory: "8Gi"
          limits:
            cpu: "8"
            memory: "16Gi"
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 5
      volumes:
      - name: config
        configMap:
          name: shrivenq-config
      - name: data
        persistentVolumeClaim:
          claimName: shrivenq-data
---
# service.yaml
apiVersion: v1
kind: Service
metadata:
  name: shrivenq-service
  namespace: trading
spec:
  selector:
    app: shrivenq
  ports:
  - name: health
    port: 8080
    targetPort: 8080
  - name: metrics
    port: 9090
    targetPort: 9090
  type: ClusterIP
```

#### Persistent Volume

```yaml
# pvc.yaml
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: shrivenq-data
  namespace: trading
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 1Ti
  storageClassName: fast-ssd
```

## Performance Tuning

### OS-Level Optimizations

```bash
#!/bin/bash
# ShrivenQuant Trading Platform - OS Performance Tuning
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Operating system optimizations for ultra-low latency trading
# USAGE: Run on production servers for maximum performance
# SAFETY: Test in staging environment before production deployment

# performance_tuning.sh

# CPU governor
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Disable CPU idle states for consistent latency
for i in /sys/devices/system/cpu/cpu*/cpuidle/state*/disable; do
    echo 1 | sudo tee $i
done

# Network optimizations
echo 'net.core.rmem_default = 262144' | sudo tee -a /etc/sysctl.conf
echo 'net.core.rmem_max = 67108864' | sudo tee -a /etc/sysctl.conf
echo 'net.core.wmem_default = 262144' | sudo tee -a /etc/sysctl.conf
echo 'net.core.wmem_max = 67108864' | sudo tee -a /etc/sysctl.conf

# Memory optimizations
echo 'vm.dirty_background_ratio = 5' | sudo tee -a /etc/sysctl.conf
echo 'vm.dirty_ratio = 10' | sudo tee -a /etc/sysctl.conf
echo 'vm.swappiness = 1' | sudo tee -a /etc/sysctl.conf

sudo sysctl -p

# IRQ affinity (bind network IRQs to specific CPUs)
echo 2 | sudo tee /proc/irq/24/smp_affinity  # Adjust IRQ number
echo 4 | sudo tee /proc/irq/25/smp_affinity

# Huge pages for reduced TLB misses
echo 1024 | sudo tee /proc/sys/vm/nr_hugepages

# CPU affinity for trading process
taskset -c 0-7 ./shrivenq --config config.toml
```

### Application Tuning

```bash
#!/bin/bash
# ShrivenQuant Trading Platform - Application Performance Tuning
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Application-level performance tuning for trading system
# USAGE: Source before starting trading application for optimal performance
# SAFETY: Validated settings for production trading environments

# Environment variables for performance
export RUST_LOG=warn  # Reduce logging overhead
export MALLOC_CONF="background_thread:true,metadata_thp:auto"
export TOKIO_WORKER_THREADS=8
export RAYON_NUM_THREADS=8

# CPU affinity
taskset -c 0-7 ./shrivenq &

# Real-time priority
sudo chrt -f 99 ./shrivenq &
```

## Monitoring & Alerting

### Prometheus Configuration

```yaml
# prometheus.yml
global:
  scrape_interval: 1s
  evaluation_interval: 1s

scrape_configs:
  - job_name: 'shrivenq'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 1s
    metrics_path: /metrics

  - job_name: 'node-exporter'
    static_configs:
      - targets: ['localhost:9100']

rule_files:
  - "alert_rules.yml"

alerting:
  alertmanagers:
    - static_configs:
        - targets:
          - alertmanager:9093
```

### Alert Rules

```yaml
# alert_rules.yml
groups:
  - name: trading_alerts
    rules:
      - alert: HighLatency
        expr: avg_over_time(tick_to_decision_ns[5m]) > 1000000  # > 1ms
        for: 10s
        labels:
          severity: warning
        annotations:
          summary: "High latency detected"
          description: "Tick-to-decision latency is {{ $value }}ns"

      - alert: DailyLossExceeded
        expr: daily_pnl < -1000000  # 10 lakh loss
        for: 0s
        labels:
          severity: critical
        annotations:
          summary: "Daily loss limit exceeded"
          description: "Daily PnL is {{ $value }}"

      - alert: OrderRejectionRate
        expr: rate(orders_rejected_total[1m]) / rate(orders_sent_total[1m]) > 0.1
        for: 30s
        labels:
          severity: warning
        annotations:
          summary: "High order rejection rate"
```

### Grafana Dashboard

```json
{
  "dashboard": {
    "title": "ShrivenQuant Trading Dashboard",
    "panels": [
      {
        "title": "Latency Metrics",
        "type": "graph",
        "targets": [
          {
            "expr": "tick_to_decision_ns",
            "legendFormat": "Tick to Decision"
          },
          {
            "expr": "decision_to_order_ns",
            "legendFormat": "Decision to Order"
          }
        ]
      },
      {
        "title": "PnL",
        "type": "singlestat",
        "targets": [
          {
            "expr": "total_pnl",
            "legendFormat": "Total PnL"
          }
        ]
      },
      {
        "title": "Order Flow",
        "type": "graph",
        "targets": [
          {
            "expr": "rate(orders_sent_total[1m])",
            "legendFormat": "Orders Sent/sec"
          },
          {
            "expr": "rate(orders_filled_total[1m])",
            "legendFormat": "Orders Filled/sec"
          }
        ]
      }
    ]
  }
}
```

## Security

### Network Security

```bash
#!/bin/bash
# ShrivenQuant Trading Platform - Network Security Setup
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Network security configuration for trading servers
# USAGE: Run on production servers for security hardening
# SAFETY: Restricts access to essential ports only

# Firewall rules (UFW)
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow 22/tcp    # SSH
sudo ufw allow 8080/tcp  # Health check
sudo ufw allow 9090/tcp  # Metrics (internal only)
sudo ufw enable

# Fail2ban for SSH protection
sudo apt install fail2ban
sudo systemctl enable fail2ban
```

### Application Security

```toml
# Secure configuration
[security]
api_rate_limit = 100
max_connections = 1000
tls_enabled = true
cert_path = "/certs/server.crt"
key_path = "/certs/server.key"

[auth]
jwt_secret = "${JWT_SECRET}"
token_expiry = 3600  # 1 hour
```

### Secret Management

```bash
#!/bin/bash
# ShrivenQuant Trading Platform - Secret Management
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Secure credential management for production deployment
# USAGE: Setup secrets in Kubernetes or Vault for secure access
# SAFETY: Never expose credentials in plain text or version control

# Using Kubernetes secrets
kubectl create secret generic trading-secrets \
  --from-literal=kite-api-key="${KITE_API_KEY}" \
  --from-literal=binance-api-key="${BINANCE_API_KEY}" \
  -n trading

# Using HashiCorp Vault (recommended)
vault kv put secret/shrivenq \
  kite_api_key="${KITE_API_KEY}" \
  binance_api_key="${BINANCE_API_KEY}"
```

## Backup & Recovery

### Data Backup

```bash
#!/bin/bash
# ShrivenQuant Trading Platform - Data Backup System
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Comprehensive backup system for trading data and configuration
# USAGE: Run daily for regular backups, on-demand for maintenance
# SAFETY: Ensures data recovery capability and compliance

# backup.sh

BACKUP_DIR="/backup/$(date +%Y%m%d_%H%M%S)"
mkdir -p "$BACKUP_DIR"

# WAL files backup
rsync -av /data/wal/ "$BACKUP_DIR/wal/"

# Configuration backup
cp -r config/ "$BACKUP_DIR/"

# Database backup
pg_dump shrivenq > "$BACKUP_DIR/database.sql"

# Compress and upload to cloud
tar -czf "$BACKUP_DIR.tar.gz" "$BACKUP_DIR"
aws s3 cp "$BACKUP_DIR.tar.gz" s3://trading-backups/
```

### Disaster Recovery

```bash
#!/bin/bash
# ShrivenQuant Trading Platform - Disaster Recovery System
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Disaster recovery and data restoration procedures
# USAGE: Run during disaster recovery to restore trading system
# SAFETY: Tested procedures for rapid system recovery

# restore.sh

BACKUP_FILE="$1"
RESTORE_DIR="/data/restore"

# Extract backup
tar -xzf "$BACKUP_FILE" -C "$RESTORE_DIR"

# Restore WAL files
rsync -av "$RESTORE_DIR/wal/" /data/wal/

# Restore database
psql shrivenq < "$RESTORE_DIR/database.sql"

# Restart services
systemctl restart shrivenq
```

## Operational Procedures

### Health Checks

```bash
#!/bin/bash
# ShrivenQuant Trading Platform - Health Check System
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Comprehensive health monitoring for trading system
# USAGE: Run continuously for monitoring, integrate with alerting systems
# SAFETY: Early detection of issues to prevent trading losses

# health_check.sh

# Check process status
if ! pgrep -f shrivenq > /dev/null; then
    echo "CRITICAL: ShrivenQ process not running"
    exit 2
fi

# Check API health
if ! curl -f http://localhost:8080/health; then
    echo "CRITICAL: Health check failed"
    exit 2
fi

# Check latency
LATENCY=$(curl -s http://localhost:9090/metrics | grep tick_to_decision | tail -1 | awk '{print $2}')
if (( $(echo "$LATENCY > 1000000" | bc -l) )); then
    echo "WARNING: High latency detected: ${LATENCY}ns"
    exit 1
fi

echo "OK: All checks passed"
exit 0
```

### Log Management

```bash
#!/bin/bash
# ShrivenQuant Trading Platform - Log Management Configuration
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Log rotation and management for trading system
# USAGE: Configure logrotate for automatic log maintenance
# SAFETY: Prevents disk space issues while maintaining audit trails

# Logrotate configuration
cat > /etc/logrotate.d/shrivenq << EOF
/logs/shrivenq.log {
    daily
    missingok
    rotate 30
    compress
    notifempty
    create 644 trader trader
    postrotate
        systemctl reload shrivenq
    endscript
}
EOF
```

### Maintenance Windows

```bash
#!/bin/bash
# ShrivenQuant Trading Platform - Maintenance Window Procedure
#
# Copyright © 2025 Praveen Ayyasola. All rights reserved.
# License: Proprietary - Contact praveenkumar.avln@gmail.com for licensing
#
# PURPOSE: Safe maintenance procedures for trading system updates
# USAGE: Run during planned maintenance windows with proper coordination
# SAFETY: Ensures positions are closed and orders cancelled before maintenance

# maintenance.sh

echo "Starting maintenance window..."

# Stop trading
curl -X POST http://localhost:8080/stop-trading

# Close all positions
curl -X POST http://localhost:8080/close-all-positions

# Cancel all orders  
curl -X POST http://localhost:8080/cancel-all-orders

# Perform maintenance tasks
./backup.sh
./update_system.sh
./restart_services.sh

echo "Maintenance completed"
```

This deployment guide provides a comprehensive approach to running ShrivenQuant in production environments while maintaining the ultra-low latency performance characteristics required for algorithmic trading.
