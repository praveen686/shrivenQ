#!/bin/bash
# Test Coverage Check - Ensure adequate test coverage

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "📊 Test Coverage Analysis"

check_test_coverage() {
    echo "🔍 Analyzing test coverage..."

    # Use cargo-tarpaulin if available
    if command -v cargo-tarpaulin >/dev/null 2>&1; then
        local coverage=$(cargo tarpaulin --out Stdout | tail -1 | grep -o '[0-9.]*%' || echo "0%")
        local coverage_num=$(echo "$coverage" | sed 's/%//')

        if (( $(echo "$coverage_num >= 80" | bc -l) )); then
            echo -e "${GREEN}✅ Test coverage: $coverage (Good)${NC}"
            return 0
        elif (( $(echo "$coverage_num >= 60" | bc -l) )); then
            echo -e "${YELLOW}⚠️  Test coverage: $coverage (Needs improvement)${NC}"
            return 1
        else
            echo -e "${RED}❌ Test coverage: $coverage (Too low)${NC}"
            return 1
        fi
    else
        echo -e "${YELLOW}⚠️  cargo-tarpaulin not available${NC}"
        echo -e "${YELLOW}   Install with: cargo install cargo-tarpaulin${NC}"

        # Basic test count check
        local test_files=$(find . -name "*.rs" -exec grep -l "#\[test\]" {} \; | wc -l)
        local src_files=$(find . -name "*.rs" -not -path "./target/*" | wc -l)

        if [[ $src_files -gt 0 ]]; then
            local test_ratio=$((test_files * 100 / src_files))
            echo -e "${YELLOW}ℹ️  Test file ratio: $test_ratio% ($test_files test files / $src_files source files)${NC}"
        fi

        return 0
    fi
}

check_test_coverage
