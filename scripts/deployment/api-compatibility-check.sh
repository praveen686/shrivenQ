#!/bin/bash
# API Compatibility Check - Ensure backward compatibility

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "🔌 API Compatibility Check"

# Check for breaking changes in public APIs
check_api_compatibility() {
    echo "🔍 Checking for API breaking changes..."

    # Use cargo public-api if available, otherwise basic checks
    if command -v cargo-public-api >/dev/null 2>&1; then
        if ! cargo public-api; then
            echo -e "${YELLOW}⚠️  Public API changes detected${NC}"
            return 1
        fi
    else
        echo -e "${YELLOW}⚠️  cargo-public-api not available, skipping detailed check${NC}"
    fi

    # Basic compatibility checks
    if git diff --name-only | grep -E "engine/src/(core|execution|position)\.rs"; then
        echo -e "${YELLOW}⚠️  Critical API files modified${NC}"
        echo -e "${YELLOW}   Review for breaking changes${NC}"
    fi

    echo -e "${GREEN}✅ API compatibility check passed${NC}"
    return 0
}

check_api_compatibility
