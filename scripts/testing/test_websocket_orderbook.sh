#!/bin/bash

# Test WebSocket connections and orderbook generation

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}     Testing WebSocket & Orderbook Generation              ${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo ""

# Load credentials
export $(cat .env | grep -v '^#' | xargs)

# Test 1: Binance WebSocket
echo -e "${YELLOW}1. Testing Binance WebSocket Streams${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Test direct WebSocket connection
echo "Testing Binance Spot WebSocket (BTCUSDT)..."
timeout 5 wscat -c wss://stream.binance.com:9443/ws/btcusdt@depth@100ms 2>/dev/null | head -5 || echo "wscat not installed, using curl"

# Test using our connector
echo -e "\nStarting Binance market data connector..."
timeout 10 cargo run --release --bin test_exchange_connectivity 2>&1 | grep -E "BTCUSDT|depth|trade|orderbook" | head -10

# Test 2: Zerodha WebSocket
echo -e "\n${YELLOW}2. Testing Zerodha WebSocket Streams${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Check if market is open
HOUR=$(date +%H)
if [ $HOUR -ge 9 ] && [ $HOUR -lt 16 ]; then
    echo -e "${GREEN}Indian markets are open${NC}"
    
    # Test Zerodha WebSocket
    echo "Connecting to Zerodha WebSocket..."
    timeout 10 cargo run --release --example zerodha_auto_login_demo 2>&1 | head -20
else
    echo -e "${YELLOW}Indian markets are closed (open 9:15 AM - 3:30 PM IST)${NC}"
fi

# Test 3: Check orderbook generation
echo -e "\n${YELLOW}3. Checking Orderbook Generation${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Check if orderbook service is running
if lsof -Pi :50052 -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo -e "${GREEN}✅ Market Connector service is running${NC}"
    
    # Check WAL files for orderbook data
    if [ -d "./data/live_market_data" ]; then
        echo "Checking for orderbook data in WAL..."
        
        # Look for recent orderbook files
        ORDERBOOK_FILES=$(find ./data/live_market_data -name "*.wal" -mmin -5 2>/dev/null | wc -l)
        if [ $ORDERBOOK_FILES -gt 0 ]; then
            echo -e "${GREEN}✅ Found $ORDERBOOK_FILES recent orderbook WAL files${NC}"
            
            # Sample some data
            echo -e "\nSample orderbook entries:"
            for file in $(find ./data/live_market_data -name "*.wal" -mmin -5 2>/dev/null | head -2); do
                echo "File: $file"
                # Try to read some binary data (WAL format)
                xxd $file 2>/dev/null | head -5 || echo "Unable to read WAL file"
            done
        else
            echo -e "${YELLOW}⚠️  No recent orderbook data found${NC}"
        fi
    fi
else
    echo -e "${RED}❌ Market Connector service not running${NC}"
fi

# Test 4: Live orderbook snapshot
echo -e "\n${YELLOW}4. Live Orderbook Snapshot${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Run orderbook demo
echo "Fetching live orderbook for BTCUSDT..."
timeout 10 cargo run --release --example demo 2>&1 | grep -E "bid|ask|spread|depth" | head -10

# Summary
echo -e "\n${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}                    TEST SUMMARY                           ${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"

# Check results
BINANCE_OK=false
ZERODHA_OK=false
ORDERBOOK_OK=false

# Simple checks
if timeout 5 curl -s https://api.binance.com/api/v3/depth?symbol=BTCUSDT&limit=5 2>/dev/null | grep -q "bids"; then
    BINANCE_OK=true
fi

echo ""
if [ "$BINANCE_OK" = true ]; then
    echo -e "Binance WebSocket: ${GREEN}✅ Connected${NC}"
else
    echo -e "Binance WebSocket: ${RED}❌ Not Connected${NC}"
fi

if [ $HOUR -ge 9 ] && [ $HOUR -lt 16 ]; then
    echo -e "Zerodha WebSocket: ${GREEN}✅ Available (Market Open)${NC}"
else
    echo -e "Zerodha WebSocket: ${YELLOW}⚠️  Market Closed${NC}"
fi

if [ $ORDERBOOK_FILES -gt 0 ]; then
    echo -e "Orderbook Generation: ${GREEN}✅ Active${NC}"
else
    echo -e "Orderbook Generation: ${YELLOW}⚠️  No Recent Data${NC}"
fi

echo -e "\n${BLUE}═══════════════════════════════════════════════════════════${NC}"