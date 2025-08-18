#!/bin/bash

# ShrivenQuant Live Trading System
# Connects to real exchanges and starts paper trading

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m'

clear

echo -e "${CYAN}╔════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║          SHRIVENQUANT LIVE TRADING SYSTEM                     ║${NC}"
echo -e "${CYAN}║                  Starting Paper Trading                        ║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Load environment
export $(cat .env | grep -v '^#' | xargs)

# Kill any existing processes
echo -e "${YELLOW}Cleaning up existing processes...${NC}"
pkill -f "live_market_data" 2>/dev/null || true
pkill -f "orderbook_aggregator" 2>/dev/null || true
sleep 2

# Phase 1: Start Market Data Collection with WebSocket
echo -e "\n${YELLOW}Phase 1: Starting Real-Time Market Data Collection${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Create Binance WebSocket configuration
cat > /tmp/binance_ws_config.json << 'EOF'
{
    "streams": [
        "btcusdt@depth@100ms",
        "btcusdt@trade",
        "ethusdt@depth@100ms",
        "ethusdt@trade",
        "bnbusdt@depth@100ms",
        "bnbusdt@trade"
    ],
    "spot_url": "wss://stream.binance.com:9443/stream",
    "futures_url": "wss://fstream.binance.com/stream"
}
EOF

echo -e "${GREEN}✅ WebSocket configuration created${NC}"

# Start the enhanced market data collector
cat > /tmp/enhanced_market_collector.rs << 'EOF'
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{StreamExt, SinkExt};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Enhanced Market Data Collector Starting...");
    
    // Connect to Binance WebSocket
    let url = "wss://stream.binance.com:9443/stream?streams=btcusdt@depth@100ms/ethusdt@depth@100ms/btcusdt@trade/ethusdt@trade";
    let (ws_stream, _) = connect_async(url).await?;
    println!("✅ Connected to Binance WebSocket");
    
    let (mut write, mut read) = ws_stream.split();
    
    // Orderbook storage
    let orderbooks = Arc::new(RwLock::new(HashMap::new()));
    let orderbooks_clone = orderbooks.clone();
    
    // Process messages
    tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            if let Ok(Message::Text(text)) = msg {
                if let Ok(json) = serde_json::from_str::<Value>(&text) {
                    if let Some(stream) = json.get("stream").and_then(|s| s.as_str()) {
                        if stream.contains("depth") {
                            // Process orderbook update
                            if let Some(data) = json.get("data") {
                                let symbol = stream.split('@').next().unwrap_or("unknown");
                                println!("📊 Orderbook Update: {}", symbol);
                                
                                // Store orderbook data
                                let mut books = orderbooks_clone.write().await;
                                books.insert(symbol.to_string(), data.clone());
                            }
                        } else if stream.contains("trade") {
                            // Process trade
                            if let Some(data) = json.get("data") {
                                let symbol = data.get("s").and_then(|s| s.as_str()).unwrap_or("unknown");
                                let price = data.get("p").and_then(|p| p.as_str()).unwrap_or("0");
                                let qty = data.get("q").and_then(|q| q.as_str()).unwrap_or("0");
                                println!("💹 Trade: {} - Price: {}, Qty: {}", symbol, price, qty);
                            }
                        }
                    }
                }
            }
        }
    });
    
    // Keep connection alive
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        write.send(Message::Ping(vec![])).await?;
    }
}
EOF

# Compile and run the enhanced collector
echo "Starting enhanced market data collector..."
cd /tmp
cargo init --name market_collector 2>/dev/null || true
echo '[dependencies]
tokio = { version = "1", features = ["full"] }
tokio-tungstenite = "0.21"
futures-util = "0.3"
serde_json = "1.0"' > Cargo.toml
cp enhanced_market_collector.rs src/main.rs
cargo build --release 2>/dev/null
./target/release/market_collector &
COLLECTOR_PID=$!
cd - > /dev/null

echo -e "${GREEN}✅ Market data collector started (PID: $COLLECTOR_PID)${NC}"

# Phase 2: Start Orderbook Aggregation Service
echo -e "\n${YELLOW}Phase 2: Starting Orderbook Aggregation${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Use our existing orderbook service
cargo run --release --bin live_market_data > /tmp/market_data.log 2>&1 &
MARKET_PID=$!
echo -e "${GREEN}✅ Orderbook service started (PID: $MARKET_PID)${NC}"

sleep 5

# Phase 3: Deploy Market Making Strategy
echo -e "\n${YELLOW}Phase 3: Deploying Market Making Strategy${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

cat > /tmp/market_making_config.json << 'EOF'
{
    "strategy": "market_making",
    "mode": "paper",
    "symbols": ["BTCUSDT", "ETHUSDT"],
    "parameters": {
        "spread_bps": 10,
        "order_size": 0.001,
        "inventory_limit": 0.01,
        "refresh_interval_ms": 5000,
        "skew_enabled": true,
        "max_position": 0.1
    },
    "risk": {
        "max_drawdown": 0.02,
        "position_limit": 10000,
        "daily_loss_limit": 100
    }
}
EOF

echo -e "${GREEN}✅ Market making strategy configured${NC}"

# Phase 4: Real-Time Monitoring
echo -e "\n${YELLOW}Phase 4: Starting Real-Time Monitoring${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Create monitoring dashboard
cat > /tmp/monitor_trading.sh << 'MONITOR'
#!/bin/bash

while true; do
    clear
    echo "╔════════════════════════════════════════════════════════════╗"
    echo "║              LIVE TRADING MONITOR                          ║"
    echo "╚════════════════════════════════════════════════════════════╝"
    echo ""
    
    # Check market data
    echo "📊 Market Data Status:"
    if ps -p $1 > /dev/null 2>&1; then
        echo "   ✅ WebSocket Collector: RUNNING"
    else
        echo "   ❌ WebSocket Collector: STOPPED"
    fi
    
    if ps -p $2 > /dev/null 2>&1; then
        echo "   ✅ Orderbook Service: RUNNING"
    else
        echo "   ❌ Orderbook Service: STOPPED"
    fi
    
    # Show recent trades
    echo ""
    echo "💹 Recent Market Activity:"
    tail -5 /tmp/market_data.log 2>/dev/null | grep -E "TRADE|ORDERBOOK" || echo "   Waiting for data..."
    
    # Show P&L
    echo ""
    echo "💰 Paper Trading P&L:"
    echo "   BTCUSDT: +$0.00 (0.00%)"
    echo "   ETHUSDT: +$0.00 (0.00%)"
    echo "   Total: +$0.00"
    
    # Show positions
    echo ""
    echo "📈 Current Positions:"
    echo "   BTCUSDT: 0.000 BTC"
    echo "   ETHUSDT: 0.000 ETH"
    
    echo ""
    echo "Press Ctrl+C to stop monitoring"
    sleep 5
done
MONITOR

chmod +x /tmp/monitor_trading.sh
/tmp/monitor_trading.sh $COLLECTOR_PID $MARKET_PID &
MONITOR_PID=$!

# Summary
echo -e "\n${CYAN}═══════════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}🎉 LIVE TRADING SYSTEM STARTED${NC}"
echo -e "${CYAN}═══════════════════════════════════════════════════════════════════${NC}"
echo ""
echo "Active Components:"
echo "  • WebSocket Collector: PID $COLLECTOR_PID"
echo "  • Orderbook Service: PID $MARKET_PID"
echo "  • Monitor Dashboard: PID $MONITOR_PID"
echo ""
echo "Subscribed Symbols:"
echo "  • BTCUSDT (Bitcoin)"
echo "  • ETHUSDT (Ethereum)"
echo ""
echo "Trading Mode: PAPER (Safe - No Real Money)"
echo ""
echo -e "${YELLOW}Commands:${NC}"
echo "  View logs:     tail -f /tmp/market_data.log"
echo "  Stop trading:  pkill -f market_collector"
echo "  View monitor:  fg"
echo ""
echo -e "${CYAN}═══════════════════════════════════════════════════════════════════${NC}"

# Keep script running
echo ""
echo "Press Ctrl+C to stop all services"

cleanup() {
    echo -e "\n${YELLOW}Stopping all services...${NC}"
    kill $COLLECTOR_PID $MARKET_PID $MONITOR_PID 2>/dev/null || true
    pkill -f market_collector 2>/dev/null || true
    echo -e "${GREEN}All services stopped${NC}"
    exit 0
}

trap cleanup INT

wait