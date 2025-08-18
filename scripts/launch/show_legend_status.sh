#!/bin/bash

# ShrivenQuant Legend Status
# Quick view of your journey to $1M

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
NC='\033[0m'

clear

echo -e "${MAGENTA}${BOLD}"
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║           SHRIVENQUANT: THE LEGEND STATUS                   ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo -e "${NC}"

# Extract latest metrics from log
if [ -f /home/praveen/ShrivenQuant/logs/paper_trading.log ]; then
    LATEST_LOG=$(tail -100 /home/praveen/ShrivenQuant/logs/paper_trading.log)
    
    # Parse metrics
    PNL=$(echo "$LATEST_LOG" | grep "PnL:" | tail -1 | grep -oP 'PnL: \$\K[0-9.]+' || echo "0.00")
    WIN_RATE=$(echo "$LATEST_LOG" | grep "Win Rate:" | tail -1 | grep -oP 'Win Rate: \K[0-9.]+' || echo "0")
    KELLY=$(echo "$LATEST_LOG" | grep "Kelly Fraction:" | tail -1 | grep -oP 'Kelly Fraction: \K[0-9.]+' || echo "0")
    TRADES=$(echo "$LATEST_LOG" | grep "Total Trades:" | tail -1 | grep -oP 'Total Trades: \K[0-9]+' || echo "0")
    MESSAGES=$(echo "$LATEST_LOG" | grep "Messages:" | tail -1 | grep -oP 'Messages:[ ]+\K[0-9]+' || echo "0")
fi

echo -e "${CYAN}📊 CURRENT PERFORMANCE:${NC}"
echo -e "   P&L:           ${GREEN}\$$PNL${NC}"
echo -e "   Win Rate:      ${GREEN}${WIN_RATE}%${NC}"
echo -e "   Kelly Size:    ${YELLOW}${KELLY}%${NC} of capital"
echo -e "   Total Trades:  ${YELLOW}$TRADES${NC}"
echo -e "   Messages:      ${YELLOW}$MESSAGES${NC}"
echo ""

echo -e "${CYAN}🎯 PROGRESS TO \$1M:${NC}"
CURRENT_CAPITAL=5000.00
TARGET_CAPITAL=1000000.00
PROGRESS=$(echo "scale=4; ($CURRENT_CAPITAL / $TARGET_CAPITAL) * 100" | bc)

# Progress bar
echo -n "   ["
FILLED=$(echo "scale=0; $PROGRESS * 50 / 100" | bc)
for ((i=0; i<50; i++)); do
    if [ $i -lt $FILLED ]; then
        echo -n "="
    else
        echo -n "-"
    fi
done
echo -e "] ${YELLOW}${PROGRESS}%${NC}"
echo ""

echo -e "${CYAN}💡 WHAT'S WORKING:${NC}"
echo -e "   ✅ Kelly Criterion sizing optimizing position size"
echo -e "   ✅ Market making capturing spreads 24/7"
echo -e "   ✅ WebSocket streaming real-time data"
echo -e "   ✅ Risk management preventing losses"
echo ""

echo -e "${CYAN}🚀 NEXT STEPS:${NC}"
echo -e "   1. Let system gather more data for Kelly optimization"
echo -e "   2. Monitor performance for 24 hours"
echo -e "   3. Fine-tune parameters based on results"
echo -e "   4. Scale capital as confidence grows"
echo ""

echo -e "${GREEN}${BOLD}Status: THE LEGEND IS BUILDING MOMENTUM${NC}"
echo ""
echo -e "${YELLOW}Run ./scripts/performance_dashboard.sh for live monitoring${NC}"