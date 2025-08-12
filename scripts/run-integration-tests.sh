#!/bin/bash
# Integration Tests Runner - Critical system tests

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "🔄 Running Integration Tests"

run_integration_tests() {
    echo "🧪 Running integration test suite..."

    # Run integration tests if they exist
    if find tests/ -name "*.rs" -type f 2>/dev/null | grep -q integration; then
        if ! cargo test --test integration; then
            echo -e "${RED}❌ Integration tests failed${NC}"
            return 1
        fi
    else
        echo -e "${YELLOW}⚠️  No integration tests found${NC}"
    fi

    # Run doc tests
    if ! cargo test --doc; then
        echo -e "${RED}❌ Documentation tests failed${NC}"
        return 1
    fi

    echo -e "${GREEN}✅ Integration tests passed${NC}"
    return 0
}

run_integration_tests
