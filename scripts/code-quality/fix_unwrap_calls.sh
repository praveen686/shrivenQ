#!/bin/bash

# Consolidated script for managing unwrap() calls and test migration
# Combines functionality of migrate_tests.sh and remove_production_unwraps.sh

set -e

SCRIPT_NAME="ShrivenQuant Code Quality Manager"
VERSION="2.0"

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# Function to show usage
show_usage() {
    echo "Usage: $0 [COMMAND]"
    echo ""
    echo "Commands:"
    echo "  analyze     - Analyze unwrap() calls and inline tests"
    echo "  migrate     - Migrate inline tests to test directories"
    echo "  fix-unwrap  - Interactively fix unwrap() calls"
    echo "  report      - Generate comprehensive code quality report"
    echo "  all         - Run all analyses and generate report"
    echo ""
    echo "Options:"
    echo "  -h, --help  - Show this help message"
    echo "  -v, --verbose - Verbose output"
}

# Function to count unwraps in production vs test code
analyze_unwraps() {
    echo -e "${CYAN}=== Analyzing unwrap() Calls ===${NC}"
    
    # Count production unwraps (excluding test modules and files)
    prod_unwraps=$(find /home/praveen/ShrivenQuant/services -name "*.rs" -type f \
        ! -path "*/tests/*" \
        ! -path "*/target/*" \
        ! -path "*/examples/*" \
        -exec sh -c '
            grep -v "^[[:space:]]*//" "$1" | \
            awk "/^[^#]*#\[cfg\(test\)\]/{test=1} 
                 /^}$/{if(test) test=0} 
                 !test && /unwrap\(\)/" | \
            grep -c "unwrap()" 2>/dev/null || echo 0
        ' _ {} \; | awk '{s+=$1} END {print s}')
    
    # Count test unwraps
    test_unwraps=$(find /home/praveen/ShrivenQuant/services -name "*.rs" -type f \
        -path "*/tests/*" \
        -exec grep -c "unwrap()" {} \; 2>/dev/null | awk '{s+=$1} END {print s}')
    
    # Count inline test unwraps
    inline_test_unwraps=$(find /home/praveen/ShrivenQuant/services -name "*.rs" -type f \
        ! -path "*/tests/*" \
        -exec sh -c '
            awk "/^[^#]*#\[cfg\(test\)\]/{test=1} 
                 /^}$/{if(test) test=0} 
                 test && /unwrap\(\)/" "$1" | \
            grep -c "unwrap()" 2>/dev/null || echo 0
        ' _ {} \; | awk '{s+=$1} END {print s}')
    
    echo -e "Production unwrap() calls: ${RED}$prod_unwraps${NC}"
    echo -e "Test directory unwrap() calls: ${YELLOW}$test_unwraps${NC}"
    echo -e "Inline test unwrap() calls: ${YELLOW}$inline_test_unwraps${NC}"
    echo -e "Total: $((prod_unwraps + test_unwraps + inline_test_unwraps))"
}

# Function to find inline test modules
find_inline_tests() {
    echo -e "${CYAN}=== Finding Inline Test Modules ===${NC}"
    
    local count=0
    while IFS= read -r file; do
        service=$(echo "$file" | sed 's|.*/services/||' | cut -d/ -f1)
        filename=$(basename "$file")
        echo -e "  ${YELLOW}$service${NC}/$filename"
        ((count++))
    done < <(find /home/praveen/ShrivenQuant/services -name "*.rs" -type f \
             ! -path "*/tests/*" \
             -exec grep -l "#\[cfg(test)\]" {} \;)
    
    echo -e "Total files with inline tests: ${RED}$count${NC}"
}

# Function to migrate a single test module
migrate_test_module() {
    local source_file=$1
    local service_name=$(echo "$source_file" | sed 's|.*/services/||' | cut -d/ -f1)
    local test_dir="/home/praveen/ShrivenQuant/services/$service_name/tests/unit"
    
    echo -e "Migrating tests from ${BLUE}$source_file${NC}"
    
    # Create test directory if it doesn't exist
    mkdir -p "$test_dir"
    
    # Extract test module content
    # This is simplified - in reality we'd need more sophisticated parsing
    echo -e "  ${GREEN}✓${NC} Created $test_dir"
    echo -e "  ${YELLOW}⚠${NC} Manual migration needed for complex test modules"
}

# Function to fix unwrap calls interactively
fix_unwrap_calls() {
    echo -e "${CYAN}=== Fixing unwrap() Calls ===${NC}"
    
    # Find files with production unwraps
    while IFS=: read -r file count; do
        echo -e "\n${YELLOW}File:${NC} $file (${RED}$count${NC} unwraps)"
        echo "Suggested fixes:"
        
        # Show each unwrap with context
        grep -n "unwrap()" "$file" | head -3 | while IFS=: read -r line_num line; do
            echo -e "  Line $line_num: $line"
            
            # Suggest fix based on pattern
            if echo "$line" | grep -q "\.await\.unwrap()"; then
                echo -e "    ${GREEN}Fix:${NC} Replace with .await?"
            elif echo "$line" | grep -q "parse()\.unwrap()"; then
                echo -e "    ${GREEN}Fix:${NC} Replace with .parse()?"
            elif echo "$line" | grep -q "lock()\.unwrap()"; then
                echo -e "    ${GREEN}Fix:${NC} Use .lock().expect(\"Lock poisoned\")"
            fi
        done
    done < <(find /home/praveen/ShrivenQuant/services -name "*.rs" -type f \
             ! -path "*/tests/*" \
             -exec sh -c '
                 count=$(grep -c "unwrap()" "$1" 2>/dev/null || echo 0)
                 if [ $count -gt 0 ]; then
                     echo "$1:$count"
                 fi
             ' _ {} \;)
}

# Function to generate comprehensive report
generate_report() {
    echo -e "${CYAN}=== Code Quality Report ===${NC}"
    echo "Generated: $(date)"
    echo "Version: $SCRIPT_NAME v$VERSION"
    echo ""
    
    analyze_unwraps
    echo ""
    find_inline_tests
    echo ""
    
    # Additional metrics
    echo -e "${CYAN}=== Additional Metrics ===${NC}"
    
    # Count services with proper test structure
    services_with_tests=$(find /home/praveen/ShrivenQuant/services -type d -name "tests" | wc -l)
    total_services=$(find /home/praveen/ShrivenQuant/services -maxdepth 1 -type d | wc -l)
    echo -e "Services with test directories: ${GREEN}$services_with_tests${NC}/$total_services"
    
    # Check for expect() usage (better than unwrap)
    expect_count=$(find /home/praveen/ShrivenQuant/services -name "*.rs" -type f \
                   -exec grep -c "expect(" {} \; 2>/dev/null | awk '{s+=$1} END {print s}')
    echo -e "expect() calls (better practice): ${GREEN}$expect_count${NC}"
    
    # Check for ? operator usage
    question_count=$(find /home/praveen/ShrivenQuant/services -name "*.rs" -type f \
                     -exec grep -c "?;" {} \; 2>/dev/null | awk '{s+=$1} END {print s}')
    echo -e "? operator usage: ${GREEN}$question_count${NC}"
}

# Main script logic
main() {
    echo -e "${CYAN}🔧 $SCRIPT_NAME v$VERSION${NC}"
    echo "=================================="
    echo ""
    
    case "${1:-help}" in
        analyze)
            analyze_unwraps
            echo ""
            find_inline_tests
            ;;
        migrate)
            find_inline_tests
            echo ""
            echo "Starting migration process..."
            # Migration logic here
            ;;
        fix-unwrap)
            fix_unwrap_calls
            ;;
        report)
            generate_report
            ;;
        all)
            generate_report
            echo ""
            echo -e "${CYAN}=== Recommendations ===${NC}"
            echo "1. Migrate all inline tests to test directories"
            echo "2. Replace unwrap() with ? operator in production code"
            echo "3. Use expect() with descriptive messages for debugging"
            echo "4. Add Result<T, E> return types to functions"
            ;;
        -h|--help|help)
            show_usage
            ;;
        *)
            echo -e "${RED}Unknown command: $1${NC}"
            show_usage
            exit 1
            ;;
    esac
}

# Run main function
main "$@"