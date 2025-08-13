#!/bin/bash

# ============================================================================
# ShrivenQuant Test Runner
# Runs all tests with proper reporting and failure tracking
# ============================================================================

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}================================================${NC}"
echo -e "${BLUE}     ShrivenQuant Test Suite                   ${NC}"
echo -e "${BLUE}================================================${NC}"

# Track failures
FAILED_TESTS=""
TOTAL_FAILED=0

# Function to run tests for a specific package
run_package_tests() {
    local package=$1
    echo -e "\n${YELLOW}Testing package: ${package}${NC}"
    
    if cargo test --package "$package" --all-features --quiet 2>/dev/null; then
        echo -e "${GREEN}  ✓ ${package} tests passed${NC}"
        return 0
    else
        echo -e "${RED}  ✗ ${package} tests failed${NC}"
        FAILED_TESTS="${FAILED_TESTS}\n  - ${package}"
        ((TOTAL_FAILED++))
        return 1
    fi
}

# Function to run specific test categories
run_test_category() {
    local category=$1
    local filter=$2
    echo -e "\n${YELLOW}Running ${category} tests...${NC}"
    
    if cargo test --all-features ${filter} --quiet 2>/dev/null; then
        echo -e "${GREEN}  ✓ ${category} tests passed${NC}"
        return 0
    else
        echo -e "${RED}  ✗ ${category} tests failed${NC}"
        FAILED_TESTS="${FAILED_TESTS}\n  - ${category}"
        ((TOTAL_FAILED++))
        return 1
    fi
}

# Count total tests
echo -e "\n${YELLOW}Counting tests...${NC}"
TOTAL_TESTS=$(cargo test --all --all-features -- --list 2>/dev/null | grep -E "^test " | wc -l || echo "0")
echo "  Found ${TOTAL_TESTS} tests"

# Run unit tests
echo -e "\n${BLUE}Running Unit Tests${NC}"
echo "================================"
run_test_category "Unit" "--lib"

# Run integration tests  
echo -e "\n${BLUE}Running Integration Tests${NC}"
echo "================================"
run_test_category "Integration" "--test '*'"

# Run doc tests
echo -e "\n${BLUE}Running Documentation Tests${NC}"
echo "================================"
run_test_category "Documentation" "--doc"

# Test individual critical packages
echo -e "\n${BLUE}Testing Critical Packages${NC}"
echo "================================"
for package in common lob engine feeds storage; do
    run_package_tests "$package" || true
done

# Run benchmarks in test mode (not actual benchmarks)
echo -e "\n${BLUE}Validating Benchmarks Compile${NC}"
echo "================================"
if cargo bench --no-run --all-features 2>/dev/null; then
    echo -e "${GREEN}  ✓ All benchmarks compile${NC}"
else
    echo -e "${RED}  ✗ Benchmark compilation failed${NC}"
    FAILED_TESTS="${FAILED_TESTS}\n  - Benchmark compilation"
    ((TOTAL_FAILED++))
fi

# Final summary
echo -e "\n${BLUE}================================================${NC}"
echo -e "${BLUE}                TEST SUMMARY                   ${NC}"
echo -e "${BLUE}================================================${NC}"

if [ $TOTAL_FAILED -eq 0 ]; then
    echo -e "${GREEN}✅ ALL TESTS PASSED!${NC}"
    echo -e "Total tests: ${TOTAL_TESTS}"
    exit 0
else
    echo -e "${RED}❌ TEST FAILURES DETECTED${NC}"
    echo -e "Failed categories (${TOTAL_FAILED}):"
    echo -e "${FAILED_TESTS}"
    echo -e "\nRun with verbose output:"
    echo "  cargo test --all --all-features -- --nocapture"
    exit 1
fi