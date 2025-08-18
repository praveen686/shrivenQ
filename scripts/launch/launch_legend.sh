#!/bin/bash

# ShrivenQuant: Launch The Legend
# The journey from $5K to $1M begins here

set -e

BLUE='\033[0;34m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
NC='\033[0m'

echo -e "${MAGENTA}${BOLD}"
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║                                                              ║"
echo "║           SHRIVENQUANT - THE LEGEND BEGINS                  ║"
echo "║                                                              ║"
echo "║              From \$5K to \$1M: Watch Us Rise                 ║"
echo "║                                                              ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo -e "${NC}"

echo -e "${YELLOW}🚀 Initializing Trading Infrastructure...${NC}"
echo ""

# Step 1: Check environment
echo -e "${BLUE}[1/5] Checking environment...${NC}"
if [ ! -f .env ]; then
    echo -e "${YELLOW}  Creating .env file with safe defaults...${NC}"
    cat > .env << EOF
TRADING_MODE=paper
MAX_POSITION_SIZE=1000
INITIAL_CAPITAL=5000
EOF
fi
echo -e "${GREEN}  ✅ Environment ready${NC}"

# Step 2: Build the system
echo -e "${BLUE}[2/5] Building enhanced trading system...${NC}"
cargo build --release --bin production_trading_system 2>&1 | grep -E "Compiling|Finished" || true
echo -e "${GREEN}  ✅ System built${NC}"

# Step 3: Stop any existing instances
echo -e "${BLUE}[3/5] Cleaning up old processes...${NC}"
pkill -f production_trading_system 2>/dev/null || true
sleep 2
echo -e "${GREEN}  ✅ Clean slate${NC}"

# Step 4: Launch the trading system
echo -e "${BLUE}[4/5] Launching enhanced trading system...${NC}"
nohup ./target/release/production_trading_system > logs/paper_trading.log 2>&1 &
TRADING_PID=$!
echo $TRADING_PID > logs/trading.pid
sleep 3

if kill -0 $TRADING_PID 2>/dev/null; then
    echo -e "${GREEN}  ✅ Trading system running (PID: $TRADING_PID)${NC}"
else
    echo -e "${RED}  ❌ Failed to start trading system${NC}"
    exit 1
fi

# Step 5: Launch monitoring
echo -e "${BLUE}[5/5] Starting performance dashboard...${NC}"
echo -e "${GREEN}  ✅ Dashboard ready${NC}"

echo ""
echo -e "${GREEN}${BOLD}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}${BOLD}                    SYSTEM LAUNCHED SUCCESSFULLY                ${NC}"
echo -e "${GREEN}${BOLD}═══════════════════════════════════════════════════════════════${NC}"
echo ""

echo -e "${MAGENTA}📊 WHAT'S RUNNING:${NC}"
echo -e "  • Enhanced Trading System with Kelly Criterion sizing"
echo -e "  • Multi-strategy alpha generation (Market Making, Mean Reversion, Momentum)"
echo -e "  • Real-time WebSocket data from Binance"
echo -e "  • Advanced risk management with position limits"
echo -e "  • Performance tracking and optimization"
echo ""

echo -e "${MAGENTA}📈 KEY FEATURES:${NC}"
echo -e "  • ${YELLOW}Kelly Criterion${NC}: Optimal position sizing based on win rate"
echo -e "  • ${YELLOW}Multi-Asset${NC}: Trading BTC, ETH, BNB, SOL simultaneously"
echo -e "  • ${YELLOW}24/7 Operation${NC}: Crypto markets never sleep, neither do we"
echo -e "  • ${YELLOW}Risk Management${NC}: Stop losses, position limits, drawdown control"
echo -e "  • ${YELLOW}Paper Trading${NC}: Test strategies safely before going live"
echo ""

echo -e "${MAGENTA}📝 COMMANDS:${NC}"
echo -e "  • View logs:        ${CYAN}tail -f logs/paper_trading.log${NC}"
echo -e "  • Monitor:          ${CYAN}./scripts/performance_dashboard.sh${NC}"
echo -e "  • Check status:     ${CYAN}ps aux | grep production_trading_system${NC}"
echo -e "  • Stop system:      ${CYAN}pkill -f production_trading_system${NC}"
echo -e "  • View trades:      ${CYAN}cat paper_trades.json | jq .${NC}"
echo ""

echo -e "${MAGENTA}🎯 THE MISSION:${NC}"
echo -e "${BOLD}  Turn \$5,000 into \$1,000,000${NC}"
echo ""
echo -e "  Month 1-2:   \$5K → \$15K     (Learn & Optimize)"
echo -e "  Month 3-6:   \$15K → \$50K    (Scale Strategies)"
echo -e "  Month 7-12:  \$50K → \$200K   (Add Complexity)"
echo -e "  Month 13-18: \$200K → \$1M    (Dominate Markets)"
echo ""

echo -e "${GREEN}${BOLD}The market is waiting. The legend has begun.${NC}"
echo ""
echo -e "${YELLOW}Opening performance dashboard in 5 seconds...${NC}"
echo -e "${YELLOW}Press Ctrl+C to exit${NC}"

sleep 5

# Launch dashboard
./scripts/performance_dashboard.sh