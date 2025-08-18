#!/bin/bash

# Script to remove unwrap() calls from production code
# This script identifies and helps fix unwrap() calls that can cause panics

set -e

echo "🔧 ShrivenQuant Production unwrap() Removal Tool"
echo "================================================="
echo ""

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Function to count unwraps excluding test code
count_production_unwraps() {
    local path=$1
    # Exclude test modules, test directories, and example files
    find "$path" -name "*.rs" -type f \
        ! -path "*/tests/*" \
        ! -path "*/target/*" \
        ! -path "*/examples/*" \
        ! -name "*test*.rs" \
        -exec sh -c '
            # Count unwraps not in test modules
            grep -v "^[[:space:]]*//" "$1" | \
            awk "/^[^#]*#\[cfg\(test\)\]/{test=1} 
                 /^[^#]*mod tests/{if(test) test=2} 
                 /^}$/{if(test==2) test=0} 
                 !test && /unwrap\(\)/" | \
            grep -c "unwrap()" || echo 0
        ' _ {} \; | awk '{s+=$1} END {print s}'
}

# Function to find files with production unwraps
find_production_unwrap_files() {
    local path=$1
    find "$path" -name "*.rs" -type f \
        ! -path "*/tests/*" \
        ! -path "*/target/*" \
        ! -path "*/examples/*" \
        ! -name "*test*.rs" \
        -exec sh -c '
            # Check if file has non-test unwraps
            count=$(grep -v "^[[:space:]]*//" "$1" | \
                   awk "/^[^#]*#\[cfg\(test\)\]/{test=1} 
                        /^[^#]*mod tests/{if(test) test=2} 
                        /^}$/{if(test==2) test=0} 
                        !test && /unwrap\(\)/" | \
                   grep -c "unwrap()" || echo 0)
            if [ $count -gt 0 ]; then
                echo "$1:$count"
            fi
        ' _ {} \;
}

# Main execution
echo "📊 Analyzing unwrap() calls..."
echo "------------------------------"

total_unwraps=$(count_production_unwraps /home/praveen/ShrivenQuant/services)
echo -e "Total production unwrap() calls: ${RED}$total_unwraps${NC}"
echo ""

if [ $total_unwraps -eq 0 ]; then
    echo -e "${GREEN}✅ No production unwrap() calls found!${NC}"
    exit 0
fi

echo "📁 Files with production unwrap() calls:"
echo "----------------------------------------"

find_production_unwrap_files /home/praveen/ShrivenQuant/services | while IFS=: read -r file count; do
    service=$(echo "$file" | sed 's|.*/services/||' | cut -d/ -f1)
    filename=$(basename "$file")
    echo -e "${YELLOW}$service${NC}/$filename: ${RED}$count${NC} unwrap() calls"
done | sort

echo ""
echo "🔧 Recommended Fixes:"
echo "--------------------"
echo "1. Replace .unwrap() with ? operator for Result types"
echo "2. Use .expect() with descriptive messages for debugging"
echo "3. Use match or if let for Option types"
echo "4. Use .unwrap_or_default() for safe defaults"
echo ""

echo "📝 Example Transformations:"
echo "---------------------------"
echo -e "${RED}// BAD - Will panic${NC}"
echo "let value = some_result.unwrap();"
echo ""
echo -e "${GREEN}// GOOD - Proper error handling${NC}"
echo "let value = some_result?;"
echo ""
echo -e "${GREEN}// GOOD - With context${NC}"
echo "let value = some_result.context(\"Failed to get value\")?;"
echo ""
echo -e "${GREEN}// GOOD - For Options${NC}"
echo "let value = some_option.ok_or_else(|| anyhow!(\"Value not found\"))?;"

# Generate fix suggestions for common patterns
echo ""
echo "🔍 Common Patterns Found:"
echo "------------------------"

# Find common unwrap patterns
grep -r "unwrap()" /home/praveen/ShrivenQuant/services \
    --include="*.rs" \
    --exclude-dir=tests \
    --exclude-dir=target \
    | grep -v "#\[cfg(test)\]" \
    | head -5 \
    | while read -r line; do
        echo -e "${BLUE}Found:${NC} $(echo "$line" | cut -d: -f2-)"
        
        # Suggest fix based on pattern
        if echo "$line" | grep -q "\.await\.unwrap()"; then
            echo -e "${GREEN}Fix:${NC} Add ? operator: .await?"
        elif echo "$line" | grep -q "parse()\.unwrap()"; then
            echo -e "${GREEN}Fix:${NC} Use .parse()? or .parse().context(\"Failed to parse\")?)"
        elif echo "$line" | grep -q "lock()\.unwrap()"; then
            echo -e "${GREEN}Fix:${NC} Handle poisoned mutex: .lock().map_err(|e| anyhow!(\"Lock poisoned: {}\", e))?"
        elif echo "$line" | grep -q "Some(.*\.unwrap())"; then
            echo -e "${GREEN}Fix:${NC} Use .and_then() or proper Option handling"
        fi
        echo ""
    done

echo "🚀 Next Steps:"
echo "--------------"
echo "1. Start with critical services (execution-router, oms, risk-manager)"
echo "2. Add proper error types to function signatures"
echo "3. Use anyhow::Result for flexible error handling"
echo "4. Test error paths with integration tests"