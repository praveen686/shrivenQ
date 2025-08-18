#!/bin/bash

# Script to help migrate inline test code to dedicated test directories
# This maintains clean separation between production and test code

set -e

echo "🧪 ShrivenQuant Test Migration Tool"
echo "===================================="
echo ""

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to check for test modules in a service
check_service_tests() {
    local service_path=$1
    local service_name=$(basename $service_path)
    
    echo -e "${YELLOW}Checking $service_name...${NC}"
    
    # Find files with test modules
    local test_files=$(find $service_path/src -name "*.rs" -type f -exec grep -l "#\[cfg(test)\]" {} \; 2>/dev/null || true)
    
    if [ -z "$test_files" ]; then
        echo -e "  ${GREEN}✓${NC} No inline tests found"
        return 0
    fi
    
    echo -e "  ${RED}✗${NC} Found inline tests in:"
    for file in $test_files; do
        echo "    - $(basename $file)"
        
        # Count unwrap() calls in test code
        local unwraps=$(grep -A 100 "#\[cfg(test)\]" $file | grep -c "unwrap()" || true)
        if [ $unwraps -gt 0 ]; then
            echo -e "      ${YELLOW}⚠${NC} Contains $unwraps unwrap() calls in tests"
        fi
    done
    
    # Create test directory structure if it doesn't exist
    if [ ! -d "$service_path/tests" ]; then
        echo -e "  ${YELLOW}Creating test directory structure...${NC}"
        mkdir -p $service_path/tests/{unit,integration}
        echo -e "  ${GREEN}✓${NC} Created $service_path/tests/"
    fi
    
    return 1
}

# Function to count total unwrap() calls
count_unwraps() {
    local path=$1
    local total_unwraps=$(find $path -name "*.rs" -type f -exec grep -h "unwrap()" {} \; 2>/dev/null | wc -l)
    echo $total_unwraps
}

# Main execution
echo "📊 Current Status:"
echo "------------------"

# Count total unwrap() calls
total_unwraps=$(count_unwraps /home/praveen/ShrivenQuant/services)
echo -e "Total unwrap() calls in services: ${RED}$total_unwraps${NC}"

# Check for unwrap() in test vs production code
test_unwraps=$(find /home/praveen/ShrivenQuant/services -name "*.rs" -type f -exec sh -c 'grep -A 100 "#\[cfg(test)\]" "$1" | grep -c "unwrap()" || true' _ {} \; | awk '{s+=$1} END {print s}')
prod_unwraps=$((total_unwraps - test_unwraps))

echo -e "  Production code: ${RED}$prod_unwraps${NC}"
echo -e "  Test code: ${YELLOW}$test_unwraps${NC}"
echo ""

echo "🔍 Scanning Services:"
echo "---------------------"

# Check each service
services_with_tests=0
for service_dir in /home/praveen/ShrivenQuant/services/*/; do
    if check_service_tests $service_dir; then
        :
    else
        ((services_with_tests++))
    fi
done

echo ""
echo "📈 Summary:"
echo "-----------"
echo -e "Services with inline tests: ${YELLOW}$services_with_tests${NC}"
echo ""

if [ $services_with_tests -gt 0 ]; then
    echo "📝 Recommendations:"
    echo "-------------------"
    echo "1. Move all #[cfg(test)] modules to dedicated test directories"
    echo "2. Use rstest fixtures for common test data"
    echo "3. Replace unwrap() with proper error handling in production code"
    echo "4. Use test-utils crate for shared test utilities"
    echo ""
    echo "Example migration:"
    echo "  - Move tests from src/lib.rs to tests/unit/lib_tests.rs"
    echo "  - Move integration tests to tests/integration/"
    echo "  - Update imports to use the service crate name"
else
    echo -e "${GREEN}✅ All services follow the testing architecture!${NC}"
fi

echo ""
echo "🚀 Next Steps:"
echo "--------------"
echo "1. Run: cargo test --all"
echo "2. Run: cargo tarpaulin --out Html"
echo "3. Review: /home/praveen/ShrivenQuant/docs/TESTING_ARCHITECTURE.md"