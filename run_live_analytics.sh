#!/bin/bash
# Run the ShrivenQuant Live Orderbook Analytics Dashboard
# This shows real-time orderbook with advanced analytics

set -e

echo "═══════════════════════════════════════════════════════════════"
echo "  ShrivenQuant Live Orderbook Analytics"
echo "  Featuring: VPIN, Kyle's Lambda, PIN, Toxicity Detection"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Check if market connector is running
if ! nc -z localhost 50052 2>/dev/null; then
    echo -e "${YELLOW}[1/3] Starting Market Connector Service...${NC}"
    
    # Start market-connector in background
    cargo run --release -p market-connector --bin market-connector > /tmp/market-connector.log 2>&1 &
    MARKET_PID=$!
    echo "Market Connector PID: $MARKET_PID"
    
    # Wait for it to start
    echo -n "Waiting for Market Connector to start"
    for i in {1..10}; do
        if nc -z localhost 50052 2>/dev/null; then
            echo -e " ${GREEN}✓${NC}"
            break
        fi
        echo -n "."
        sleep 1
    done
    
    if ! nc -z localhost 50052 2>/dev/null; then
        echo -e " ${RED}✗${NC}"
        echo -e "${RED}ERROR: Market Connector failed to start${NC}"
        echo "Check logs at: /tmp/market-connector.log"
        tail -20 /tmp/market-connector.log
        exit 1
    fi
else
    echo -e "${GREEN}✓ Market Connector already running${NC}"
    MARKET_PID=""
fi

echo -e "${YELLOW}[2/3] Building Live Analytics Dashboard...${NC}"
cargo build --release --example live_analytics 2>&1 | grep -E "Compiling|Finished" || true

echo -e "${YELLOW}[3/3] Starting Live Analytics Dashboard...${NC}"
echo ""
echo -e "${CYAN}Dashboard Features:${NC}"
echo "  📊 Real-time orderbook depth visualization"
echo "  📈 Best Bid/Offer with spread analysis"
echo "  ⚖️  Multi-level imbalance calculations"
echo "  🔬 Market microstructure analytics:"
echo "     • VPIN (Volume-Synchronized PIN)"
echo "     • Kyle's Lambda (Price Impact)"
echo "     • PIN (Probability of Informed Trading)"
echo "     • Toxicity Score (Adverse Selection)"
echo "  ⚡ Performance metrics with latency percentiles"
echo ""
echo -e "${GREEN}Starting dashboard...${NC}"
echo "────────────────────────────────────────────────────"
echo ""

# Function to cleanup on exit
cleanup() {
    echo ""
    echo -e "${YELLOW}Shutting down...${NC}"
    if [ ! -z "$MARKET_PID" ]; then
        kill $MARKET_PID 2>/dev/null || true
        echo "Stopped Market Connector"
    fi
    exit 0
}

# Set up trap for cleanup
trap cleanup INT TERM

# Run the live analytics dashboard
cargo run --release --example live_analytics

# Cleanup if we reach here
cleanup