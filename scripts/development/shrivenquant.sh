#!/bin/bash

# ShrivenQuant Master Control Script
# Consolidated interface for all platform operations

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m' # No Color
BOLD='\033[1m'

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# Function to display header
show_header() {
    clear
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║${BOLD}         SHRIVENQUANT TRADING PLATFORM CONTROL CENTER        ${NC}${CYAN}║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════════╝${NC}"
    echo
}

# Function to display menu
show_menu() {
    echo -e "${YELLOW}${BOLD}MAIN MENU:${NC}"
    echo
    echo -e "${GREEN}[1]${NC} 🚀 Deployment"
    echo -e "    ${BLUE}a)${NC} Start All Services"
    echo -e "    ${BLUE}b)${NC} Start Live Trading"
    echo -e "    ${BLUE}c)${NC} Connect Real Exchanges"
    echo -e "    ${BLUE}d)${NC} Production Demo"
    echo
    echo -e "${GREEN}[2]${NC} 📊 Monitoring"
    echo -e "    ${BLUE}a)${NC} Check System Status"
    echo -e "    ${BLUE}b)${NC} Run Live Analytics"
    echo -e "    ${BLUE}c)${NC} Setup Monitoring"
    echo
    echo -e "${GREEN}[3]${NC} 🧪 Testing"
    echo -e "    ${BLUE}a)${NC} Run System Tests"
    echo -e "    ${BLUE}b)${NC} Test Live Data"
    echo -e "    ${BLUE}c)${NC} Test Market Data"
    echo -e "    ${BLUE}d)${NC} Test Trading Flow"
    echo -e "    ${BLUE}e)${NC} Test WebSocket Orderbook"
    echo
    echo -e "${GREEN}[4]${NC} 🛠️  Development"
    echo -e "    ${BLUE}a)${NC} Build All Services"
    echo -e "    ${BLUE}b)${NC} Run Tests"
    echo -e "    ${BLUE}c)${NC} Check Compilation"
    echo -e "    ${BLUE}d)${NC} Clean Build"
    echo
    echo -e "${GREEN}[5]${NC} 📈 Quick Actions"
    echo -e "    ${BLUE}a)${NC} Start Paper Trading"
    echo -e "    ${BLUE}b)${NC} View Orderbook"
    echo -e "    ${BLUE}c)${NC} Check P&L"
    echo -e "    ${BLUE}d)${NC} Stop All Services"
    echo
    echo -e "${GREEN}[6]${NC} 🔧 Utilities"
    echo -e "    ${BLUE}a)${NC} Setup Zerodha Auth"
    echo -e "    ${BLUE}b)${NC} View Logs"
    echo -e "    ${BLUE}c)${NC} Clear WAL Data"
    echo -e "    ${BLUE}d)${NC} Backup Configuration"
    echo
    echo -e "${RED}[q]${NC} Quit"
    echo
}

# Function to execute deployment scripts
deployment_menu() {
    case $1 in
        a) "$SCRIPT_DIR/deployment/start_services.sh" ;;
        b) "$SCRIPT_DIR/deployment/start_live_trading.sh" ;;
        c) "$SCRIPT_DIR/deployment/connect_real_exchanges.sh" ;;
        d) "$SCRIPT_DIR/deployment/production_demo.sh" ;;
        *) echo -e "${RED}Invalid option${NC}" ;;
    esac
}

# Function to execute monitoring scripts
monitoring_menu() {
    case $1 in
        a) "$SCRIPT_DIR/monitoring/check_system_status.sh" ;;
        b) "$SCRIPT_DIR/monitoring/run_live_analytics.sh" ;;
        c) "$SCRIPT_DIR/monitoring/setup_monitoring.sh" ;;
        *) echo -e "${RED}Invalid option${NC}" ;;
    esac
}

# Function to execute testing scripts
testing_menu() {
    case $1 in
        a) "$SCRIPT_DIR/testing/system_test.sh" ;;
        b) "$SCRIPT_DIR/testing/test_live_data.sh" ;;
        c) "$SCRIPT_DIR/testing/test_market_data.sh" ;;
        d) "$SCRIPT_DIR/testing/test_trading_flow.sh" ;;
        e) "$SCRIPT_DIR/testing/test_websocket_orderbook.sh" ;;
        *) echo -e "${RED}Invalid option${NC}" ;;
    esac
}

# Function to execute development tasks
development_menu() {
    case $1 in
        a) 
            echo -e "${GREEN}Building all services...${NC}"
            cd "$ROOT_DIR"
            cargo build --release --all
            ;;
        b)
            echo -e "${GREEN}Running tests...${NC}"
            cd "$ROOT_DIR"
            cargo test --all
            ;;
        c)
            echo -e "${GREEN}Checking compilation...${NC}"
            cd "$ROOT_DIR"
            cargo check --all
            ;;
        d)
            echo -e "${GREEN}Cleaning build...${NC}"
            cd "$ROOT_DIR"
            cargo clean
            ;;
        *) echo -e "${RED}Invalid option${NC}" ;;
    esac
}

# Function to execute quick actions
quick_actions_menu() {
    case $1 in
        a)
            echo -e "${GREEN}Starting paper trading...${NC}"
            cd "$ROOT_DIR"
            cargo run --release --bin production_trading_system
            ;;
        b)
            echo -e "${GREEN}Viewing orderbook...${NC}"
            cd "$ROOT_DIR"
            cargo run --release --bin test_websocket_orderbook
            ;;
        c)
            echo -e "${GREEN}Checking P&L...${NC}"
            if [ -f "$ROOT_DIR/paper_trades.json" ]; then
                cat "$ROOT_DIR/paper_trades.json"
            else
                echo "No trades found"
            fi
            ;;
        d)
            echo -e "${YELLOW}Stopping all services...${NC}"
            pkill -f "target/release" || true
            echo -e "${GREEN}All services stopped${NC}"
            ;;
        *) echo -e "${RED}Invalid option${NC}" ;;
    esac
}

# Function to execute utilities
utilities_menu() {
    case $1 in
        a)
            if [ -f "$SCRIPT_DIR/auth/setup-zerodha-auth.sh" ]; then
                "$SCRIPT_DIR/auth/setup-zerodha-auth.sh"
            else
                echo "Zerodha auth setup not available"
            fi
            ;;
        b)
            echo -e "${GREEN}Viewing logs...${NC}"
            tail -f "$ROOT_DIR"/logs/*.log 2>/dev/null || echo "No logs found"
            ;;
        c)
            echo -e "${YELLOW}Clearing WAL data...${NC}"
            read -p "Are you sure? (y/n): " -n 1 -r
            echo
            if [[ $REPLY =~ ^[Yy]$ ]]; then
                rm -rf "$ROOT_DIR"/data/*_wal
                echo -e "${GREEN}WAL data cleared${NC}"
            fi
            ;;
        d)
            echo -e "${GREEN}Backing up configuration...${NC}"
            timestamp=$(date +%Y%m%d_%H%M%S)
            backup_dir="$ROOT_DIR/backups/$timestamp"
            mkdir -p "$backup_dir"
            cp -r "$ROOT_DIR"/.env "$ROOT_DIR"/config "$backup_dir/" 2>/dev/null || true
            echo -e "${GREEN}Configuration backed up to $backup_dir${NC}"
            ;;
        *) echo -e "${RED}Invalid option${NC}" ;;
    esac
}

# Main program loop
main() {
    while true; do
        show_header
        show_menu
        
        echo -n "Enter choice [1-6 followed by a-d, or q to quit]: "
        read -r choice
        
        if [[ "$choice" == "q" ]]; then
            echo -e "${GREEN}Goodbye!${NC}"
            exit 0
        fi
        
        # Parse main choice and sub-choice
        main_choice="${choice:0:1}"
        sub_choice="${choice:1:1}"
        
        case $main_choice in
            1) deployment_menu "$sub_choice" ;;
            2) monitoring_menu "$sub_choice" ;;
            3) testing_menu "$sub_choice" ;;
            4) development_menu "$sub_choice" ;;
            5) quick_actions_menu "$sub_choice" ;;
            6) utilities_menu "$sub_choice" ;;
            *) echo -e "${RED}Invalid option${NC}" ;;
        esac
        
        echo
        echo -e "${YELLOW}Press Enter to continue...${NC}"
        read -r
    done
}

# Handle command line arguments for direct execution
if [ $# -gt 0 ]; then
    case "$1" in
        start)
            "$SCRIPT_DIR/deployment/start_services.sh"
            ;;
        stop)
            pkill -f "target/release" || true
            echo "All services stopped"
            ;;
        status)
            "$SCRIPT_DIR/monitoring/check_system_status.sh"
            ;;
        trade)
            cd "$ROOT_DIR"
            cargo run --release --bin production_trading_system
            ;;
        test)
            "$SCRIPT_DIR/testing/system_test.sh"
            ;;
        demo)
            "$SCRIPT_DIR/deployment/production_demo.sh"
            ;;
        help|--help|-h)
            echo "ShrivenQuant Control Script"
            echo "Usage: $0 [command]"
            echo ""
            echo "Commands:"
            echo "  start   - Start all services"
            echo "  stop    - Stop all services"
            echo "  status  - Check system status"
            echo "  trade   - Start paper trading"
            echo "  test    - Run system tests"
            echo "  demo    - Run production demo"
            echo "  help    - Show this help message"
            echo ""
            echo "Run without arguments for interactive menu"
            ;;
        *)
            echo "Unknown command: $1"
            echo "Run '$0 help' for usage information"
            exit 1
            ;;
    esac
else
    # Run interactive menu
    main
fi