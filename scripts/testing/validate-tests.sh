#!/bin/bash

# ============================================================================
# ShrivenQuant Test Validation for Pre-Commit
# Ensures all tests pass before allowing commit
# ============================================================================

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}🧪 Running Test Validation...${NC}"

# Quick test to fail fast
echo "  Running quick test suite..."
if ! cargo test --all --all-features --quiet 2>/dev/null; then
    echo -e "${RED}❌ Tests Failed!${NC}"
    echo ""
    echo "Failed tests detected. Running detailed test report..."
    echo ""
    
    # Show which tests failed
    cargo test --all --all-features 2>&1 | grep -A 5 -E "(FAILED|error:|panicked)" || true
    
    echo ""
    echo -e "${RED}Fix failing tests before committing!${NC}"
    echo "Run tests locally with: cargo test --all --all-features"
    exit 1
fi

# Count test statistics
UNIT_TESTS=$(cargo test --lib --all-features -- --list 2>/dev/null | grep -c "^test " || echo "0")
DOC_TESTS=$(cargo test --doc --all-features -- --list 2>/dev/null | grep -c "^test " || echo "0")
TOTAL_TESTS=$((UNIT_TESTS + DOC_TESTS))

echo -e "${GREEN}✅ All ${TOTAL_TESTS} tests passed!${NC}"
echo "  - Unit tests: ${UNIT_TESTS}"
echo "  - Doc tests: ${DOC_TESTS}"

# Verify test coverage for critical modules
echo ""
echo "Verifying critical module test coverage..."

CRITICAL_MODULES=("engine" "lob" "feeds" "storage")
for module in "${CRITICAL_MODULES[@]}"; do
    if cargo test --package "$module" --quiet 2>/dev/null; then
        echo -e "  ${GREEN}✓${NC} ${module}"
    else
        echo -e "  ${RED}✗${NC} ${module} - FAILED"
        exit 1
    fi
done

echo -e "${GREEN}Test validation complete!${NC}"
exit 0