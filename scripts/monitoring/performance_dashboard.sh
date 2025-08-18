#!/bin/bash

# ShrivenQuant Performance Dashboard
# Real-time monitoring of trading system performance

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color
BOLD='\033[1m'

while true; do
    clear
    echo -e "${BLUE}${BOLD}╔══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}${BOLD}║        SHRIVENQUANT PERFORMANCE DASHBOARD - LIVE            ║${NC}"
    echo -e "${BLUE}${BOLD}╚══════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${CYAN}Time: $(date '+%Y-%m-%d %H:%M:%S')${NC}"
    echo ""
    
    # Check if system is running
    if pgrep -f "production_trading_system" > /dev/null; then
        echo -e "${GREEN}✅ System Status: RUNNING${NC}"
        PID=$(pgrep -f "production_trading_system")
        echo -e "   PID: $PID"
        
        # Get memory and CPU usage
        if [ ! -z "$PID" ]; then
            CPU=$(ps -p $PID -o %cpu= | tr -d ' ')
            MEM=$(ps -p $PID -o %mem= | tr -d ' ')
            echo -e "   CPU: ${CPU}% | Memory: ${MEM}%"
        fi
    else
        echo -e "${RED}❌ System Status: NOT RUNNING${NC}"
    fi
    
    echo ""
    echo -e "${MAGENTA}${BOLD}📊 TRADING METRICS:${NC}"
    
    # Parse recent trades from log if exists
    if [ -f /home/praveen/ShrivenQuant/logs/paper_trading.log ]; then
        TOTAL_TRADES=$(grep -c "TRADE:" /home/praveen/ShrivenQuant/logs/paper_trading.log 2>/dev/null || echo "0")
        echo -e "   Total Trades: ${YELLOW}$TOTAL_TRADES${NC}"
        
        # Get last trade
        LAST_TRADE=$(grep "TRADE:" /home/praveen/ShrivenQuant/logs/paper_trading.log | tail -1 2>/dev/null)
        if [ ! -z "$LAST_TRADE" ]; then
            echo -e "   Last Trade: ${CYAN}$(echo $LAST_TRADE | cut -d' ' -f3-)${NC}"
        fi
    fi
    
    # Parse paper trades JSON if exists
    if [ -f paper_trades.json ]; then
        TRADE_COUNT=$(jq length paper_trades.json 2>/dev/null || echo "0")
        echo -e "   Recorded Trades: ${YELLOW}$TRADE_COUNT${NC}"
    fi
    
    # Show WebSocket status
    echo ""
    echo -e "${MAGENTA}${BOLD}🌐 WEBSOCKET STATUS:${NC}"
    # Use ss instead of netstat (more modern and available)
    if ss -tan | grep -q ":9443.*ESTAB"; then
        echo -e "   ${GREEN}✅ Connected to Binance${NC}"
        CONNECTIONS=$(ss -tan | grep -c ":9443.*ESTAB")
        echo -e "   Active Connections: $CONNECTIONS"
    elif lsof -i :9443 2>/dev/null | grep -q "ESTABLISHED"; then
        echo -e "   ${GREEN}✅ Connected to Binance${NC}"
        CONNECTIONS=$(lsof -i :9443 2>/dev/null | grep -c "ESTABLISHED")
        echo -e "   Active Connections: $CONNECTIONS"
    else
        # Check if system is trying to connect
        if grep -q "Connected to Binance" /home/praveen/ShrivenQuant/logs/paper_trading.log 2>/dev/null; then
            LAST_CONNECT=$(grep "Connected to Binance" /home/praveen/ShrivenQuant/logs/paper_trading.log | tail -1)
            echo -e "   ${YELLOW}⚠️  Connection status unclear${NC}"
            echo -e "   Last known: $LAST_CONNECT"
        else
            echo -e "   ${RED}❌ Not connected to Binance${NC}"
        fi
    fi
    
    # System resources
    echo ""
    echo -e "${MAGENTA}${BOLD}💻 SYSTEM RESOURCES:${NC}"
    echo -e "   CPU Load: $(uptime | awk -F'load average:' '{print $2}')"
    echo -e "   Memory: $(free -h | awk '/^Mem:/ {printf "%s / %s (%.1f%%)", $3, $2, ($3/$2)*100}')"
    echo -e "   Disk: $(df -h / | awk 'NR==2 {printf "%s / %s (%s)", $3, $2, $5}')"
    
    # Check for errors in log
    echo ""
    echo -e "${MAGENTA}${BOLD}⚠️  RECENT ERRORS:${NC}"
    if [ -f /home/praveen/ShrivenQuant/logs/paper_trading.log ]; then
        ERROR_COUNT=$(grep -c "ERROR\|error" /home/praveen/ShrivenQuant/logs/paper_trading.log 2>/dev/null || echo "0")
        if [ "$ERROR_COUNT" -gt "0" ]; then
            echo -e "   ${RED}Found $ERROR_COUNT errors${NC}"
            echo -e "   Last Error:"
            grep -i "error" /home/praveen/ShrivenQuant/logs/paper_trading.log | tail -1 | sed 's/^/   /'
        else
            echo -e "   ${GREEN}No errors detected${NC}"
        fi
    fi
    
    # Performance summary
    echo ""
    echo -e "${MAGENTA}${BOLD}📈 PERFORMANCE SUMMARY:${NC}"
    if [ -f performance_report.json ]; then
        CAPITAL=$(jq -r '.capital' performance_report.json 2>/dev/null || echo "N/A")
        PNL=$(jq -r '.total_pnl' performance_report.json 2>/dev/null || echo "N/A")
        WIN_RATE=$(jq -r '.win_rate' performance_report.json 2>/dev/null || echo "N/A")
        
        echo -e "   Capital: ${GREEN}\$$CAPITAL${NC}"
        echo -e "   P&L: ${GREEN}\$$PNL${NC}"
        echo -e "   Win Rate: ${YELLOW}$WIN_RATE${NC}"
    else
        echo -e "   ${YELLOW}Waiting for performance data...${NC}"
    fi
    
    # Show next steps
    echo ""
    echo -e "${CYAN}${BOLD}🎯 TARGET: \$5K → \$1M${NC}"
    echo ""
    echo -e "${YELLOW}Press Ctrl+C to exit dashboard${NC}"
    
    sleep 5
done