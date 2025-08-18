#!/bin/bash

# ShrivenQuant System Status Check
# Real-time monitoring of all services

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

clear
echo -e "${BLUE}╔══════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║       ShrivenQuant Trading Platform Status              ║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════════════════════╝${NC}"
echo ""

# Function to check service
check_service() {
    local name=$1
    local port=$2
    local desc=$3
    
    if lsof -Pi :$port -sTCP:LISTEN -t >/dev/null 2>&1; then
        local pid=$(lsof -Pi :$port -sTCP:LISTEN -t 2>/dev/null | head -1)
        echo -e "${GREEN}✅ $name${NC} (Port $port) - ${GREEN}RUNNING${NC} [PID: $pid]"
        echo -e "   └─ $desc"
    else
        echo -e "${RED}❌ $name${NC} (Port $port) - ${RED}STOPPED${NC}"
        echo -e "   └─ $desc"
    fi
}

echo -e "${YELLOW}Core Services:${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
check_service "Auth Service" 50051 "Authentication & API key management"
check_service "Market Connector" 50052 "Real-time market data from exchanges"
check_service "Risk Manager" 50053 "Position limits & risk controls"
check_service "Execution Router" 50054 "Smart order routing to venues"
check_service "Trading Gateway" 50055 "Strategy execution & signals"
check_service "API Gateway" 8080 "REST API & WebSocket interface"

echo ""
echo -e "${YELLOW}Performance Metrics:${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Check system resources
CPU=$(top -bn1 | grep "Cpu(s)" | sed "s/.*, *\([0-9.]*\)%* id.*/\1/" | awk '{print 100 - $1}' 2>/dev/null || echo "N/A")
MEM=$(free -m | awk 'NR==2{printf "%.1f", $3*100/$2 }' 2>/dev/null || echo "N/A")

echo -e "CPU Usage: ${GREEN}${CPU}%${NC}"
echo -e "Memory Usage: ${GREEN}${MEM}%${NC}"

# Check network connections
CONNECTIONS=$(netstat -an 2>/dev/null | grep ESTABLISHED | wc -l)
echo -e "Active Connections: ${GREEN}${CONNECTIONS}${NC}"

echo ""
echo -e "${YELLOW}Market Data Status:${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Check if market data is being received
if pgrep -f "live_market_data" > /dev/null; then
    echo -e "${GREEN}✅ Live market data streaming active${NC}"
else
    echo -e "${YELLOW}⚠️  Live market data not running${NC}"
fi

# Check WAL directories
if [ -d "./data/live_market_data" ]; then
    BINANCE_FILES=$(find ./data/live_market_data/binance_wal -type f 2>/dev/null | wc -l)
    ZERODHA_FILES=$(find ./data/live_market_data/zerodha_wal -type f 2>/dev/null | wc -l)
    echo -e "Binance WAL files: ${GREEN}${BINANCE_FILES}${NC}"
    echo -e "Zerodha WAL files: ${GREEN}${ZERODHA_FILES}${NC}"
fi

echo ""
echo -e "${YELLOW}Quick Actions:${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "1. Start all services:    ./start_services.sh"
echo "2. Test market data:      ./test_market_data.sh"
echo "3. View logs:            journalctl -f -u shrivenquant"
echo "4. Stop all services:    pkill -f 'target/release'"

echo ""
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"