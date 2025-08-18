#!/bin/bash

# ShrivenQuant Safe Deployment Script
# Deploys paper trading with multiple safeguards

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║     SHRIVENQUANT SAFE DEPLOYMENT - PAPER TRADING ONLY       ║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Safety checks
echo -e "${YELLOW}🔒 Running safety checks...${NC}"

# Check for .env file
if [ ! -f .env ]; then
    echo -e "${RED}❌ Error: .env file not found${NC}"
    echo "Please create .env with exchange credentials"
    exit 1
fi

# Ensure we're in paper trading mode
if ! grep -q "TRADING_MODE=paper" .env 2>/dev/null; then
    echo -e "${YELLOW}⚠️  Adding TRADING_MODE=paper to .env for safety${NC}"
    echo "TRADING_MODE=paper" >> .env
fi

# Set position limits
if ! grep -q "MAX_POSITION_SIZE" .env 2>/dev/null; then
    echo -e "${YELLOW}⚠️  Setting MAX_POSITION_SIZE=1000 (USD) for safety${NC}"
    echo "MAX_POSITION_SIZE=1000" >> .env
fi

# Build with safety checks
echo -e "${GREEN}🔨 Building services with release optimizations...${NC}"
cargo build --release --all 2>&1 | grep -E "error|warning" || true

# Check for compilation errors
if cargo build --release --all 2>&1 | grep -q "error\["; then
    echo -e "${RED}❌ Compilation errors detected. Fix before deployment.${NC}"
    exit 1
fi

# Create necessary directories
echo -e "${GREEN}📁 Creating required directories...${NC}"
mkdir -p logs data/wal data/benchmarks

# Stop any existing services
echo -e "${YELLOW}🛑 Stopping existing services...${NC}"
pkill -f "target/release" || true
sleep 2

# Function to start service with monitoring
start_service() {
    local name=$1
    local binary=$2
    local port=$3
    local log_file="logs/${name}.log"
    
    echo -e "${GREEN}Starting ${name} on port ${port}...${NC}"
    
    # Start with output redirection and error handling
    nohup ./target/release/${binary} > ${log_file} 2>&1 &
    local pid=$!
    
    # Wait and check if service started
    sleep 2
    if kill -0 $pid 2>/dev/null; then
        echo -e "${GREEN}✅ ${name} started (PID: $pid)${NC}"
        echo $pid > "logs/${name}.pid"
    else
        echo -e "${RED}❌ ${name} failed to start. Check ${log_file}${NC}"
        tail -5 ${log_file}
        return 1
    fi
}

# Start core services with safeguards
echo ""
echo -e "${BLUE}🚀 Starting services with safeguards...${NC}"

# Start services in order
start_service "auth-service" "auth-service" 50051
start_service "market-connector" "market-connector" 50052
start_service "risk-manager" "risk-manager" 50053
start_service "execution-router" "execution-router" 50054
start_service "trading-gateway" "trading-gateway" 50059

# Wait for services to stabilize
echo -e "${YELLOW}⏳ Waiting for services to stabilize...${NC}"
sleep 5

# Health checks
echo ""
echo -e "${BLUE}🏥 Running health checks...${NC}"

check_service() {
    local name=$1
    local port=$2
    
    if nc -z localhost $port 2>/dev/null; then
        echo -e "${GREEN}✅ ${name} is responding on port ${port}${NC}"
        return 0
    else
        echo -e "${RED}❌ ${name} is not responding on port ${port}${NC}"
        return 1
    fi
}

check_service "Auth Service" 50051
check_service "Market Connector" 50052
check_service "Risk Manager" 50053
check_service "Execution Router" 50054
check_service "Trading Gateway" 50059

# Start paper trading with limits
echo ""
echo -e "${BLUE}📈 Starting paper trading system...${NC}"

# Create paper trading config
cat > logs/paper_trading_config.json << EOF
{
    "mode": "paper",
    "max_position_size": 1000,
    "max_orders_per_minute": 10,
    "require_risk_check": true,
    "enable_circuit_breaker": true,
    "max_daily_loss": 100,
    "exchanges": ["binance_testnet"],
    "symbols": ["BTCUSDT", "ETHUSDT"]
}
EOF

# Start paper trading with safeguards
echo -e "${GREEN}Starting production trading system in PAPER mode...${NC}"
nohup ./target/release/production_trading_system > logs/paper_trading.log 2>&1 &
PAPER_PID=$!
echo $PAPER_PID > logs/paper_trading.pid

# Monitor initial behavior
echo ""
echo -e "${BLUE}📊 Monitoring initial behavior...${NC}"
sleep 5

# Check if paper trading is running
if kill -0 $PAPER_PID 2>/dev/null; then
    echo -e "${GREEN}✅ Paper trading system is running${NC}"
    
    # Show recent activity
    echo ""
    echo -e "${YELLOW}Recent trading activity:${NC}"
    tail -20 logs/paper_trading.log | grep -E "TRADE|ORDER|Connected|ERROR" || echo "Waiting for market data..."
else
    echo -e "${RED}❌ Paper trading system crashed. Check logs:${NC}"
    tail -10 logs/paper_trading.log
    exit 1
fi

# Setup monitoring
echo ""
echo -e "${BLUE}📊 Setting up monitoring...${NC}"

# Create monitoring script
cat > logs/monitor.sh << 'EOF'
#!/bin/bash
while true; do
    clear
    echo "=== ShrivenQuant Paper Trading Monitor ==="
    echo "Time: $(date)"
    echo ""
    echo "Service Status:"
    for pid_file in logs/*.pid; do
        if [ -f "$pid_file" ]; then
            service=$(basename $pid_file .pid)
            pid=$(cat $pid_file)
            if kill -0 $pid 2>/dev/null; then
                echo "  ✅ $service (PID: $pid)"
            else
                echo "  ❌ $service (STOPPED)"
            fi
        fi
    done
    echo ""
    echo "Recent Trading Activity:"
    tail -5 logs/paper_trading.log | grep -E "TRADE|ORDER" || echo "  No recent trades"
    echo ""
    echo "Errors (last hour):"
    find logs -name "*.log" -mmin -60 -exec grep -l ERROR {} \; | wc -l | xargs echo "  Error count:"
    echo ""
    echo "Press Ctrl+C to exit monitoring"
    sleep 10
done
EOF
chmod +x logs/monitor.sh

# Display summary
echo ""
echo -e "${GREEN}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║                  DEPLOYMENT SUCCESSFUL                      ║${NC}"
echo -e "${GREEN}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${BLUE}📊 System Status:${NC}"
echo "  • Mode: PAPER TRADING ONLY"
echo "  • Max Position: $1000"
echo "  • Risk Checks: ENABLED"
echo "  • Circuit Breaker: ENABLED"
echo ""
echo -e "${BLUE}📝 Logs:${NC}"
echo "  • Service logs: logs/*.log"
echo "  • Paper trading: logs/paper_trading.log"
echo ""
echo -e "${BLUE}🔧 Commands:${NC}"
echo "  • Monitor: ./logs/monitor.sh"
echo "  • Stop all: pkill -f target/release"
echo "  • Check health: nc -z localhost 50051-50059"
echo ""
echo -e "${GREEN}✨ Paper trading is now running with safeguards!${NC}"
echo -e "${YELLOW}⚠️  This is PAPER TRADING only - no real money at risk${NC}"