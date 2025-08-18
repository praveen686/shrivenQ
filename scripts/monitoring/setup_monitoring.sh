#!/bin/bash

# ShrivenQuant Monitoring Setup
# Sets up Prometheus and Grafana for system monitoring

set -e

echo "=========================================="
echo "📊 ShrivenQuant Monitoring Setup"
echo "=========================================="
echo ""

# Create monitoring directory
mkdir -p monitoring/prometheus
mkdir -p monitoring/grafana/dashboards

# Create Prometheus configuration
cat > monitoring/prometheus/prometheus.yml << 'EOF'
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'risk-manager'
    static_configs:
      - targets: ['localhost:9053']
        labels:
          service: 'risk-manager'
  
  - job_name: 'execution-router'
    static_configs:
      - targets: ['localhost:9054']
        labels:
          service: 'execution-router'
  
  - job_name: 'market-connector'
    static_configs:
      - targets: ['localhost:9052']
        labels:
          service: 'market-connector'
  
  - job_name: 'data-aggregator'
    static_configs:
      - targets: ['localhost:9057']
        labels:
          service: 'data-aggregator'
  
  - job_name: 'trading-gateway'
    static_configs:
      - targets: ['localhost:9090']
        labels:
          service: 'trading-gateway'
EOF

# Create Grafana dashboard JSON
cat > monitoring/grafana/dashboards/shrivenquant.json << 'EOF'
{
  "dashboard": {
    "title": "ShrivenQuant Trading Platform",
    "panels": [
      {
        "title": "Order Throughput",
        "type": "graph",
        "targets": [
          {
            "expr": "rate(orders_submitted_total[5m])"
          }
        ]
      },
      {
        "title": "Risk Checks",
        "type": "graph",
        "targets": [
          {
            "expr": "rate(risk_checks_total[5m])"
          }
        ]
      },
      {
        "title": "Market Data Rate",
        "type": "graph",
        "targets": [
          {
            "expr": "rate(market_ticks_received_total[5m])"
          }
        ]
      },
      {
        "title": "System Latency",
        "type": "graph",
        "targets": [
          {
            "expr": "histogram_quantile(0.99, tick_to_trade_latency_histogram)"
          }
        ]
      }
    ]
  }
}
EOF

# Create docker-compose for monitoring stack
cat > monitoring/docker-compose.yml << 'EOF'
version: '3.8'

services:
  prometheus:
    image: prom/prometheus:latest
    container_name: shrivenquant-prometheus
    volumes:
      - ./prometheus/prometheus.yml:/etc/prometheus/prometheus.yml
      - prometheus_data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
    ports:
      - "9091:9090"
    network_mode: host

  grafana:
    image: grafana/grafana:latest
    container_name: shrivenquant-grafana
    volumes:
      - grafana_data:/var/lib/grafana
      - ./grafana/dashboards:/etc/grafana/provisioning/dashboards
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=shrivenquant
      - GF_INSTALL_PLUGINS=
    ports:
      - "3000:3000"
    network_mode: host

  alertmanager:
    image: prom/alertmanager:latest
    container_name: shrivenquant-alertmanager
    volumes:
      - alertmanager_data:/alertmanager
    ports:
      - "9093:9093"
    network_mode: host

volumes:
  prometheus_data:
  grafana_data:
  alertmanager_data:
EOF

# Create alert rules
cat > monitoring/prometheus/alerts.yml << 'EOF'
groups:
  - name: shrivenquant_alerts
    rules:
      - alert: HighOrderLatency
        expr: histogram_quantile(0.99, tick_to_trade_latency_histogram) > 10000
        for: 1m
        labels:
          severity: warning
        annotations:
          summary: "High order latency detected"
          description: "P99 latency is {{ $value }} microseconds"
      
      - alert: ServiceDown
        expr: up == 0
        for: 30s
        labels:
          severity: critical
        annotations:
          summary: "Service {{ $labels.service }} is down"
          description: "{{ $labels.service }} has been down for more than 30 seconds"
      
      - alert: RiskLimitBreached
        expr: daily_pnl < -50000
        for: 1s
        labels:
          severity: critical
        annotations:
          summary: "Daily loss limit breached"
          description: "Daily PnL is {{ $value }}"
      
      - alert: HighErrorRate
        expr: rate(errors_total[5m]) > 10
        for: 1m
        labels:
          severity: warning
        annotations:
          summary: "High error rate detected"
          description: "Error rate is {{ $value }} per second"
EOF

echo "✅ Monitoring configuration created"
echo ""
echo "To start monitoring stack:"
echo "  cd monitoring"
echo "  docker-compose up -d"
echo ""
echo "Access points:"
echo "  Prometheus: http://localhost:9091"
echo "  Grafana: http://localhost:3000 (admin/shrivenquant)"
echo "  Alertmanager: http://localhost:9093"
echo ""
echo "To view service metrics directly:"
echo "  Risk Manager: http://localhost:9053/metrics"
echo "  Trading Gateway: http://localhost:9090/metrics"