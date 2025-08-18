#!/bin/bash

# ShrivenQuant Service Startup Script
# Launches all core services for the trading platform

set -e

echo "🚀 Starting ShrivenQuant Trading Platform..."
echo "==========================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to check if port is available
check_port() {
    if lsof -Pi :$1 -sTCP:LISTEN -t >/dev/null 2>&1; then
        echo -e "${RED}❌ Port $1 is already in use${NC}"
        return 1
    else
        echo -e "${GREEN}✅ Port $1 is available${NC}"
        return 0
    fi
}

# Check required ports
echo -e "\n${YELLOW}Checking ports...${NC}"
PORTS=(50051 50052 50053 50054 50055 8080)
PORT_NAMES=("Auth" "Market-Connector" "Risk-Manager" "Execution-Router" "Trading-Gateway" "API-Gateway")

for i in ${!PORTS[@]}; do
    echo -n "${PORT_NAMES[$i]} (${PORTS[$i]}): "
    check_port ${PORTS[$i]}
done

# Build all services
echo -e "\n${YELLOW}Building services...${NC}"
cargo build --release

# Start services in background
echo -e "\n${YELLOW}Starting services...${NC}"

# 1. Auth Service (Port 50051)
echo -e "${GREEN}Starting Auth Service...${NC}"
cargo run --release --bin auth-service 2>&1 | sed 's/^/[AUTH] /' &
AUTH_PID=$!
sleep 2

# 2. Market Connector (Port 50052)
echo -e "${GREEN}Starting Market Connector...${NC}"
cargo run --release --bin market-connector 2>&1 | sed 's/^/[MARKET] /' &
MARKET_PID=$!
sleep 2

# 3. Risk Manager (Port 50053)
echo -e "${GREEN}Starting Risk Manager...${NC}"
cargo run --release --bin risk-manager 2>&1 | sed 's/^/[RISK] /' &
RISK_PID=$!
sleep 2

# 4. Execution Router (Port 50054)
echo -e "${GREEN}Starting Execution Router...${NC}"
cargo run --release --bin execution-router 2>&1 | sed 's/^/[EXEC] /' &
EXEC_PID=$!
sleep 2

# 5. Trading Gateway (Port 50055)
echo -e "${GREEN}Starting Trading Gateway...${NC}"
cargo run --release --bin trading-gateway 2>&1 | sed 's/^/[TRADE] /' &
TRADE_PID=$!
sleep 2

# 6. API Gateway (Port 8080)
echo -e "${GREEN}Starting API Gateway...${NC}"
cargo run --release --bin gateway 2>&1 | sed 's/^/[API] /' &
API_PID=$!

echo -e "\n${GREEN}✅ All services started!${NC}"
echo "==========================================="
echo "Service PIDs:"
echo "  Auth Service:       $AUTH_PID"
echo "  Market Connector:   $MARKET_PID"
echo "  Risk Manager:       $RISK_PID"
echo "  Execution Router:   $EXEC_PID"
echo "  Trading Gateway:    $TRADE_PID"
echo "  API Gateway:        $API_PID"
echo "==========================================="
echo -e "${YELLOW}Press Ctrl+C to stop all services${NC}"

# Function to cleanup on exit
cleanup() {
    echo -e "\n${YELLOW}Stopping all services...${NC}"
    kill $AUTH_PID $MARKET_PID $RISK_PID $EXEC_PID $TRADE_PID $API_PID 2>/dev/null
    echo -e "${GREEN}Services stopped${NC}"
    exit 0
}

# Set trap to cleanup on Ctrl+C
trap cleanup INT

# Wait for all background processes
wait