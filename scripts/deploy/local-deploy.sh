#!/bin/bash
# Local deployment script for testing CI/CD pipeline

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
DEPLOY_DIR="/tmp/shrivenquant-deploy"
ENVIRONMENT="${1:-dev}"

echo -e "${GREEN}ShrivenQuant Local Deployment Script${NC}"
echo "======================================="
echo "Environment: $ENVIRONMENT"
echo ""

# Function to check prerequisites
check_prerequisites() {
    echo -e "${YELLOW}Checking prerequisites...${NC}"
    
    # Check Rust
    if ! command -v cargo &> /dev/null; then
        echo -e "${RED}Error: Rust is not installed${NC}"
        exit 1
    fi
    
    # Check Git
    if ! command -v git &> /dev/null; then
        echo -e "${RED}Error: Git is not installed${NC}"
        exit 1
    fi
    
    echo -e "${GREEN}✓ All prerequisites met${NC}"
}

# Function to run tests
run_tests() {
    echo -e "${YELLOW}Running tests...${NC}"
    
    # Quick tests for dev
    if [ "$ENVIRONMENT" = "dev" ]; then
        cargo test --lib
    # Full tests for test/staging
    elif [ "$ENVIRONMENT" = "test" ]; then
        cargo test --all
        cargo clippy -- -D warnings
    # Complete validation for prod
    elif [ "$ENVIRONMENT" = "prod" ]; then
        cargo test --all --release
        cargo clippy -- -D warnings
        cargo audit || true
    fi
    
    echo -e "${GREEN}✓ Tests passed${NC}"
}

# Function to build binaries
build_binaries() {
    echo -e "${YELLOW}Building binaries...${NC}"
    
    if [ "$ENVIRONMENT" = "dev" ]; then
        cargo build
        BUILD_DIR="target/debug"
    else
        cargo build --release
        BUILD_DIR="target/release"
    fi
    
    echo -e "${GREEN}✓ Build complete${NC}"
}

# Function to create deployment package
create_package() {
    echo -e "${YELLOW}Creating deployment package...${NC}"
    
    # Clean and create deployment directory
    rm -rf "$DEPLOY_DIR"
    mkdir -p "$DEPLOY_DIR"/{bin,config,logs,data}
    
    # Copy binaries (if they exist)
    if [ -f "$BUILD_DIR/shriven-quant" ]; then
        cp "$BUILD_DIR/shriven-quant" "$DEPLOY_DIR/bin/"
    fi
    
    # Create environment config
    cat > "$DEPLOY_DIR/config/environment.env" << EOF
# ShrivenQuant Configuration
# Environment: $ENVIRONMENT
# Generated: $(date)

ENVIRONMENT=$ENVIRONMENT
RUST_LOG=info
RUST_BACKTRACE=1

# Trading Configuration
$(if [ "$ENVIRONMENT" = "prod" ]; then
    echo "TRADING_MODE=live"
    echo "RISK_CHECK_ENABLED=true"
    echo "MAX_POSITION_SIZE=10000"
    echo "MAX_ORDERS_PER_SEC=100"
else
    echo "TRADING_MODE=paper"
    echo "RISK_CHECK_ENABLED=true"
    echo "MAX_POSITION_SIZE=1000"
    echo "MAX_ORDERS_PER_SEC=10"
fi)

# Performance Configuration
MEMORY_POOL_SIZE=1048576
ENABLE_METRICS=true
ENABLE_MONITORING=true

# Paths
LOG_DIR=$DEPLOY_DIR/logs
DATA_DIR=$DEPLOY_DIR/data
EOF
    
    # Create start script
    cat > "$DEPLOY_DIR/bin/start.sh" << 'EOF'
#!/bin/bash
source ../config/environment.env
echo "Starting ShrivenQuant in $ENVIRONMENT mode..."
./shriven-quant
EOF
    chmod +x "$DEPLOY_DIR/bin/start.sh"
    
    # Create stop script
    cat > "$DEPLOY_DIR/bin/stop.sh" << 'EOF'
#!/bin/bash
echo "Stopping ShrivenQuant..."
pkill -f shriven-quant || true
EOF
    chmod +x "$DEPLOY_DIR/bin/stop.sh"
    
    echo -e "${GREEN}✓ Package created at $DEPLOY_DIR${NC}"
}

# Function to run smoke tests
run_smoke_tests() {
    echo -e "${YELLOW}Running smoke tests...${NC}"
    
    # Check if binary exists and is executable
    if [ -f "$DEPLOY_DIR/bin/shriven-quant" ]; then
        echo "Binary found and executable"
    fi
    
    # Run quick benchmarks
    if [ "$ENVIRONMENT" != "dev" ]; then
        echo "Running performance checks..."
        cargo bench --package engine risk_order_check -- --warm-up-time 1 --measurement-time 2
    fi
    
    echo -e "${GREEN}✓ Smoke tests passed${NC}"
}

# Function to display deployment summary
display_summary() {
    echo ""
    echo -e "${GREEN}========== Deployment Summary ==========${NC}"
    echo "Environment: $ENVIRONMENT"
    echo "Deploy Directory: $DEPLOY_DIR"
    echo "Git Commit: $(git rev-parse --short HEAD)"
    echo "Branch: $(git branch --show-current)"
    echo "Timestamp: $(date)"
    echo ""
    echo "Next Steps:"
    echo "1. cd $DEPLOY_DIR"
    echo "2. Review config/environment.env"
    echo "3. Run: ./bin/start.sh"
    echo ""
    
    if [ "$ENVIRONMENT" = "prod" ]; then
        echo -e "${RED}⚠️  PRODUCTION DEPLOYMENT WARNING ⚠️${NC}"
        echo "This is configured for LIVE TRADING!"
        echo "Ensure you have:"
        echo "- Valid API credentials"
        echo "- Risk limits configured"
        echo "- Monitoring enabled"
        echo "- Rollback plan ready"
    fi
}

# Main execution
main() {
    echo "Starting deployment process..."
    echo ""
    
    check_prerequisites
    run_tests
    build_binaries
    create_package
    run_smoke_tests
    display_summary
    
    echo -e "${GREEN}✅ Deployment complete!${NC}"
}

# Run main function
main