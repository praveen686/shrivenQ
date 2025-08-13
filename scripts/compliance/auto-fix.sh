#!/bin/bash
# Auto-fix compliance violations with sq-remediator

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

echo -e "${CYAN}🔧 ShrivenQuant Auto-Fix Tool${NC}"
echo "========================================="

# Parse arguments
DRY_RUN=""
RULES=""
SKIP_CHECK=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --dry-run)
            DRY_RUN="--dry-run"
            echo -e "${YELLOW}Running in DRY-RUN mode (no files will be modified)${NC}"
            shift
            ;;
        --rules)
            RULES="--rules $2"
            shift 2
            ;;
        --skip-compile-check)
            SKIP_CHECK="--skip-compile-check"
            shift
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --dry-run           Show changes without applying them"
            echo "  --rules <rules>     Only fix specific rules (comma-separated)"
            echo "                      Available: hashmap_fx, unwrap_to_try, safe_casts,"
            echo "                               handle_errors, float_money"
            echo "  --skip-compile-check  Skip compilation check after fixes"
            echo "  --help              Show this help message"
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# Get project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Set tool paths
REMEDIATOR="$PROJECT_ROOT/tools/sq-compliance-tools/sq-remediator/target/release/sq-remediator"
CHECKER="$PROJECT_ROOT/tools/sq-compliance-tools/sq-compliance/target/release/sq-compliance"
PROJECT_PATH="$PROJECT_ROOT"

# Build remediator if needed
if [ ! -f "$REMEDIATOR" ]; then
    echo -e "${YELLOW}Building remediator...${NC}"
    (cd "$PROJECT_ROOT/tools/sq-compliance-tools/sq-remediator" && cargo build --release --quiet)
fi

# Build compliance checker if needed
if [ ! -f "$CHECKER" ]; then
    echo -e "${YELLOW}Building compliance checker...${NC}"
    (cd "$PROJECT_ROOT/tools/sq-compliance-tools/sq-compliance" && cargo build --release --quiet)
fi

# Run initial compliance check
echo -e "\n${BLUE}→ Running initial compliance check...${NC}"
$CHECKER $PROJECT_PATH || true

# Run remediator
echo -e "\n${BLUE}→ Running auto-fix...${NC}"
$REMEDIATOR $DRY_RUN $RULES $SKIP_CHECK --verbose $PROJECT_PATH

if [ -z "$DRY_RUN" ]; then
    # Run compliance check again to show improvement
    echo -e "\n${BLUE}→ Running post-fix compliance check...${NC}"
    $CHECKER $PROJECT_PATH || true
    
    echo -e "\n${GREEN}✓ Auto-fix complete!${NC}"
    echo -e "${YELLOW}Note: Some violations require manual review and cannot be auto-fixed.${NC}"
else
    echo -e "\n${YELLOW}Dry-run complete. Use without --dry-run to apply fixes.${NC}"
fi