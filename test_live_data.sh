#!/bin/bash
# Production-grade test script for live market data from Zerodha and Binance
# This tests the entire data pipeline from market-connector to data-aggregator

set -e

echo "========================================="
echo "ShrivenQuant Live Market Data Test"
echo "Testing: Zerodha + Binance Integration"
echo "========================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
MARKET_CONNECTOR_PORT=50052
DATA_AGGREGATOR_PORT=50057
TEST_DURATION=30  # seconds to run the test
LOG_DIR="./logs/test_$(date +%Y%m%d_%H%M%S)"

# Create log directory
mkdir -p "$LOG_DIR"

echo -e "${YELLOW}[1/5] Starting Market Connector Service...${NC}"
# Start market-connector with real WebSocket connections
cargo run --release -p market-connector --bin market-connector > "$LOG_DIR/market-connector.log" 2>&1 &
MARKET_PID=$!
echo "Market Connector PID: $MARKET_PID"

# Wait for market-connector to start
sleep 5

# Check if market-connector is running
if ! kill -0 $MARKET_PID 2>/dev/null; then
    echo -e "${RED}ERROR: Market Connector failed to start${NC}"
    cat "$LOG_DIR/market-connector.log"
    exit 1
fi

# Wait additional time for gRPC server to bind to port
sleep 3

echo -e "${YELLOW}[2/5] Starting Data Aggregator Service...${NC}"
# Start data-aggregator to consume and persist market data
cargo run --release -p data-aggregator --bin data-aggregator > "$LOG_DIR/data-aggregator.log" 2>&1 &
AGGREGATOR_PID=$!
echo "Data Aggregator PID: $AGGREGATOR_PID"

# Wait for data-aggregator to start (longer delay for gRPC server)
sleep 8

# Check if data-aggregator is running
if ! kill -0 $AGGREGATOR_PID 2>/dev/null; then
    echo -e "${RED}ERROR: Data Aggregator failed to start${NC}"
    cat "$LOG_DIR/data-aggregator.log"
    kill $MARKET_PID 2>/dev/null
    exit 1
fi

echo -e "${YELLOW}[3/5] Testing gRPC connectivity...${NC}"
# Test if services are responding on their ports
nc -zv localhost $MARKET_CONNECTOR_PORT || {
    echo -e "${RED}ERROR: Market Connector not responding on port $MARKET_CONNECTOR_PORT${NC}"
    kill $MARKET_PID $AGGREGATOR_PID 2>/dev/null
    exit 1
}

nc -zv localhost $DATA_AGGREGATOR_PORT || {
    echo -e "${RED}ERROR: Data Aggregator not responding on port $DATA_AGGREGATOR_PORT${NC}"
    kill $MARKET_PID $AGGREGATOR_PID 2>/dev/null
    exit 1
}

echo -e "${GREEN}✓ Services are running and responding${NC}"

echo -e "${YELLOW}[4/5] Collecting live market data for $TEST_DURATION seconds...${NC}"
echo "Monitoring logs for market data events..."

# Monitor for actual data events
BINANCE_COUNT=0
ZERODHA_COUNT=0
START_TIME=$(date +%s)

while [ $(($(date +%s) - START_TIME)) -lt $TEST_DURATION ]; do
    # Check for Binance data
    if grep -q "binance" "$LOG_DIR/market-connector.log" 2>/dev/null; then
        BINANCE_EVENTS=$(grep -c "binance" "$LOG_DIR/market-connector.log" 2>/dev/null || echo "0")
        if [ "$BINANCE_EVENTS" -gt "$BINANCE_COUNT" ]; then
            echo -e "${GREEN}✓ Received Binance market data: $BINANCE_EVENTS events${NC}"
            BINANCE_COUNT=$BINANCE_EVENTS
        fi
    fi
    
    # Check for Zerodha data
    if grep -q "zerodha\|kite" "$LOG_DIR/market-connector.log" 2>/dev/null; then
        ZERODHA_EVENTS=$(grep -c "zerodha\|kite" "$LOG_DIR/market-connector.log" 2>/dev/null || echo "0")
        if [ "$ZERODHA_EVENTS" -gt "$ZERODHA_COUNT" ]; then
            echo -e "${GREEN}✓ Received Zerodha market data: $ZERODHA_EVENTS events${NC}"
            ZERODHA_COUNT=$ZERODHA_EVENTS
        fi
    fi
    
    # Check WAL writes
    if [ -d "./data/wal" ]; then
        WAL_FILES=$(find ./data/wal -type f 2>/dev/null | wc -l)
        if [ "$WAL_FILES" -gt 0 ]; then
            WAL_SIZE=$(du -sh ./data/wal 2>/dev/null | cut -f1)
            echo -e "${GREEN}✓ WAL storage active: $WAL_FILES files, $WAL_SIZE total${NC}"
        fi
    fi
    
    sleep 2
done

echo -e "${YELLOW}[5/5] Test Results Summary${NC}"
echo "========================================="

# Analyze results
SUCCESS=true

# Check market-connector logs
echo -e "\n${YELLOW}Market Connector Analysis:${NC}"
if grep -q "WebSocket connected" "$LOG_DIR/market-connector.log" 2>/dev/null; then
    echo -e "${GREEN}✓ WebSocket connections established${NC}"
else
    echo -e "${RED}✗ No WebSocket connections found${NC}"
    SUCCESS=false
fi

if grep -q "BTCUSDT\|ETHUSDT" "$LOG_DIR/market-connector.log" 2>/dev/null; then
    CRYPTO_EVENTS=$(grep -c "BTCUSDT\|ETHUSDT" "$LOG_DIR/market-connector.log" 2>/dev/null || echo "0")
    echo -e "${GREEN}✓ Crypto market data received: $CRYPTO_EVENTS events${NC}"
else
    echo -e "${RED}✗ No crypto market data received${NC}"
    SUCCESS=false
fi

# Check data-aggregator logs
echo -e "\n${YELLOW}Data Aggregator Analysis:${NC}"
if grep -q "Connected to market-connector" "$LOG_DIR/data-aggregator.log" 2>/dev/null; then
    echo -e "${GREEN}✓ Successfully connected to market-connector${NC}"
else
    echo -e "${RED}✗ Failed to connect to market-connector${NC}"
    SUCCESS=false
fi

if grep -q "Processing market event\|WAL" "$LOG_DIR/data-aggregator.log" 2>/dev/null; then
    PROCESSED_EVENTS=$(grep -c "Processing market event" "$LOG_DIR/data-aggregator.log" 2>/dev/null || echo "0")
    echo -e "${GREEN}✓ Processed $PROCESSED_EVENTS market events${NC}"
else
    echo -e "${YELLOW}⚠ No market events processed yet${NC}"
fi

# Check WAL persistence
echo -e "\n${YELLOW}Data Persistence Analysis:${NC}"
if [ -d "./data/wal" ] && [ "$(find ./data/wal -type f 2>/dev/null | wc -l)" -gt 0 ]; then
    WAL_SIZE=$(du -sh ./data/wal 2>/dev/null | cut -f1)
    WAL_FILES=$(find ./data/wal -type f -name "*.wal" 2>/dev/null | wc -l)
    echo -e "${GREEN}✓ WAL persistence working: $WAL_FILES segments, $WAL_SIZE total${NC}"
    
    # Show sample of persisted data
    if [ -f "./data/wal/segment_00000000.wal" ]; then
        echo -e "${GREEN}✓ First WAL segment created successfully${NC}"
    fi
else
    echo -e "${RED}✗ No WAL data persisted${NC}"
    SUCCESS=false
fi

# Cleanup
echo -e "\n${YELLOW}Shutting down services...${NC}"
kill $MARKET_PID $AGGREGATOR_PID 2>/dev/null || true

# Wait for processes to terminate
sleep 2

# Final result
echo "========================================="
if [ "$SUCCESS" = true ]; then
    echo -e "${GREEN}✅ PRODUCTION TEST PASSED${NC}"
    echo -e "${GREEN}Successfully received and persisted live market data${NC}"
    echo -e "${GREEN}Binance Events: $BINANCE_COUNT${NC}"
    echo -e "${GREEN}Log files saved to: $LOG_DIR${NC}"
    exit 0
else
    echo -e "${RED}❌ PRODUCTION TEST FAILED${NC}"
    echo -e "${RED}Check logs at: $LOG_DIR${NC}"
    echo -e "\n${YELLOW}Recent market-connector logs:${NC}"
    tail -20 "$LOG_DIR/market-connector.log"
    echo -e "\n${YELLOW}Recent data-aggregator logs:${NC}"
    tail -20 "$LOG_DIR/data-aggregator.log"
    exit 1
fi