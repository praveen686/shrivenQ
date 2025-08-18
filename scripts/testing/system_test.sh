#!/bin/bash

# ShrivenQuant Full System Integration Test
# This script starts all services and verifies end-to-end functionality

set -e

echo "=========================================="
echo "🚀 ShrivenQuant System Integration Test"
echo "=========================================="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to check if port is open
check_port() {
    local port=$1
    local service=$2
    if nc -z 127.0.0.1 $port 2>/dev/null; then
        echo -e "${GREEN}✅ $service (port $port) is running${NC}"
        return 0
    else
        echo -e "${RED}❌ $service (port $port) is not responding${NC}"
        return 1
    fi
}

# Function to start service
start_service() {
    local binary=$1
    local name=$2
    local port=$3
    
    echo -e "${YELLOW}Starting $name...${NC}"
    ./target/debug/$binary > /tmp/$binary.log 2>&1 &
    local pid=$!
    echo $pid > /tmp/$binary.pid
    
    # Wait for service to start
    sleep 2
    
    if check_port $port "$name"; then
        return 0
    else
        echo -e "${RED}Failed to start $name${NC}"
        return 1
    fi
}

# Function to stop all services
cleanup() {
    echo ""
    echo -e "${YELLOW}Stopping all services...${NC}"
    
    for pidfile in /tmp/*.pid; do
        if [ -f "$pidfile" ]; then
            pid=$(cat $pidfile)
            if ps -p $pid > /dev/null 2>&1; then
                kill $pid 2>/dev/null || true
            fi
            rm -f $pidfile
        fi
    done
    
    echo -e "${GREEN}All services stopped${NC}"
}

# Set trap to cleanup on exit
trap cleanup EXIT

# Build all binaries first
echo "Building all services..."
cargo build --bins

echo ""
echo "Starting core services..."
echo "========================="

# Start services in dependency order
start_service "risk-manager" "Risk Manager" 50053
start_service "execution-router" "Execution Router" 50054
start_service "market-connector" "Market Connector" 50052
start_service "data-aggregator" "Data Aggregator" 50057

echo ""
echo "Service Status Check:"
echo "===================="
check_port 50053 "Risk Manager"
check_port 50054 "Execution Router"
check_port 50052 "Market Connector"
check_port 50057 "Data Aggregator"

echo ""
echo "Testing Inter-Service Communication..."
echo "======================================"

# Test risk manager health
echo -e "${YELLOW}Testing Risk Manager gRPC...${NC}"
if curl -s http://127.0.0.1:9053/metrics > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Risk Manager metrics endpoint working${NC}"
else
    echo -e "${RED}❌ Risk Manager metrics endpoint not responding${NC}"
fi

echo ""
echo "Testing Market Data Flow..."
echo "==========================="

# Check if market data is flowing
echo -e "${YELLOW}Checking market data logs...${NC}"
if grep -q "Connected to market-connector" /tmp/data-aggregator.log 2>/dev/null; then
    echo -e "${GREEN}✅ Data Aggregator connected to Market Connector${NC}"
else
    echo -e "${RED}❌ Data Aggregator not receiving market data${NC}"
fi

if grep -q "WebSocket connected" /tmp/market-connector.log 2>/dev/null; then
    echo -e "${GREEN}✅ Market Connector connected to exchange${NC}"
else
    echo -e "${YELLOW}⚠️  Market Connector may not be connected to exchange${NC}"
fi

echo ""
echo "System Test Summary:"
echo "==================="

# Count running services
running_count=0
for port in 50053 50054 50052 50057; do
    if nc -z 127.0.0.1 $port 2>/dev/null; then
        ((running_count++))
    fi
done

echo -e "${GREEN}Services Running: $running_count/4${NC}"

if [ $running_count -eq 4 ]; then
    echo -e "${GREEN}✅ All core services are operational${NC}"
    echo -e "${GREEN}✅ System is ready for trading${NC}"
else
    echo -e "${RED}❌ Some services are not running${NC}"
    echo -e "${YELLOW}Check logs in /tmp/ for details${NC}"
fi

echo ""
echo "Logs available at:"
echo "  /tmp/risk-manager.log"
echo "  /tmp/execution-router.log"
echo "  /tmp/market-connector.log"
echo "  /tmp/data-aggregator.log"

echo ""
echo "Press Enter to stop all services and exit..."
read
