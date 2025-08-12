#!/bin/bash
# scripts/agent-compliance-check.sh

set -euo pipefail

# Set timeout for long-running commands
TIMEOUT_SECONDS=30

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo "🤖 Agent Compliance Check"
echo "========================"

VIOLATIONS=0

# 1. Check for prohibited patterns
echo -e "\n${BLUE}1. Scanning for prohibited patterns...${NC}"

# Hot path allocations
echo "   Checking for hot path allocations..."
# Get allocation count with timeout
ALLOCATIONS=0
if allocation_files=$(find crates/ -name "*.rs" -exec grep -l "Vec::new()\|String::new()\|Box::new()\|HashMap::new()" {} \; 2>/dev/null | grep -v test 2>/dev/null); then
    ALLOCATIONS=$(echo "$allocation_files" | wc -l 2>/dev/null || echo "0")
fi
if [ "$ALLOCATIONS" -gt 0 ]; then
    echo -e "${RED}❌ VIOLATION: Found $ALLOCATIONS files with hot path allocations${NC}"
    find crates/ -name "*.rs" -exec grep -Hn "Vec::new()\|String::new()\|Box::new()\|HashMap::new()" {} \; | \
        grep -v test | head -5 || true
    ((VIOLATIONS++))
fi

# Floating point money - MUST distinguish between external API and internal use
echo "   Checking for floating point money calculations..."
# Exclude: test files, Deserialize structs, websocket handlers, API responses, features, instrument fetchers
FLOAT_MONEY_FILES=$(find crates/ -name "*.rs" -exec grep -l ": f64.*price\|: f64.*amount\|price: f64\|amount: f64" {} \; 2>/dev/null | \
    grep -v test | \
    grep -v "Deserialize\|features\|websocket\|api\|feeds/src/binance\|feeds/src/zerodha" | \
    grep -v "loaders\|adapters\|instrument_fetcher" || true)

if [ -n "$FLOAT_MONEY_FILES" ]; then
    FLOAT_COUNT=$(echo "$FLOAT_MONEY_FILES" | wc -l)
    echo -e "${RED}❌ VIOLATION: Found $FLOAT_COUNT files with floating point money in INTERNAL calculations${NC}"
    echo "   External API deserialization is allowed, but internal calculations must use fixed-point!"
    echo "$FLOAT_MONEY_FILES" | head -5
    ((VIOLATIONS++))
else
    echo -e "${GREEN}✓ No floating point money in internal calculations${NC}"
fi

# Panic usage - ALL code should use proper error handling
echo "   Checking for panic/unwrap usage..."

# Use a temporary file to collect violations (avoids subshell variable issues)
PANIC_TEMP=$(mktemp)
PANIC_COUNT=0

# Check ALL Rust files - no unwrap(), expect(), or panic!() allowed
for file in $(find crates/ -name "*.rs" -type f); do
    # Check each file for unwrap/expect/panic
    while IFS= read -r line; do
        # Parse the grep output
        linenum=$(echo "$line" | cut -d: -f2)
        match=$(echo "$line" | cut -d: -f3-)

        # Validate line number
        if ! [[ "$linenum" =~ ^[0-9]+$ ]]; then
            continue
        fi

        # This is a violation - ALL code should handle errors properly
        echo "${file}:${linenum}: ${match}" >> "$PANIC_TEMP"
        PANIC_COUNT=$((PANIC_COUNT + 1))
    done < <(grep -Hn "panic!\|\.unwrap()\|\.expect(" "$file" 2>/dev/null || true)
done

if [ "$PANIC_COUNT" -gt 0 ]; then
    echo -e "${RED}❌ VIOLATION: Found $PANIC_COUNT panic/unwrap/expect usages${NC}"
    echo "   ALL code should use proper error handling with context"
    echo "   Use Result types and ? operator, or expect() with descriptive messages"
    head -10 "$PANIC_TEMP"
    if [ "$PANIC_COUNT" -gt 10 ]; then
        echo "   ... and $((PANIC_COUNT - 10)) more violations"
    fi
    ((VIOLATIONS++))
else
    echo -e "${GREEN}✓ No panic/unwrap/expect found - excellent error handling!${NC}"
fi

rm -f "$PANIC_TEMP"

# std::HashMap usage
echo "   Checking for std::HashMap usage..."
STD_HASHMAP=0
for file in $(find crates/ -name "*.rs" | grep -v test); do
    if grep -q "std::collections::HashMap\|use std::collections::HashMap" "$file" 2>/dev/null; then
        STD_HASHMAP=$((STD_HASHMAP + 1))
    fi
done
if [ "$STD_HASHMAP" -gt 0 ]; then
    echo -e "${RED}❌ VIOLATION: Found $STD_HASHMAP files using std::HashMap (use FxHashMap)${NC}"
    ((VIOLATIONS++))
fi

# AI/Agent shortcuts and anti-patterns
echo "   Checking for AI/Agent shortcuts and anti-patterns..."

# Underscore prefix abuse (lazy unused variable handling)
UNDERSCORE_ABUSE=$(find crates/ -name "*.rs" -exec grep -Hn "let _[a-z].*=" {} \; | \
    grep -v test | grep -v "_phantom\|_guard\|_lock" | wc -l | tr -d ' ' || echo "0")
UNDERSCORE_ABUSE=$(echo "$UNDERSCORE_ABUSE" | head -1 | tr -d ' ')
if [ "$UNDERSCORE_ABUSE" -gt 0 ]; then
    echo -e "${RED}❌ VIOLATION: Found $UNDERSCORE_ABUSE lazy underscore variable usages${NC}"
    echo "   Use proper variable names or #[allow(unused_variables)] with justification"
    find crates/ -name "*.rs" -exec grep -Hn "let _[a-z].*=" {} \; | \
        grep -v test | grep -v "_phantom\|_guard\|_lock" | head -3 || true
    ((VIOLATIONS++))
fi

# TODO/FIXME/HACK shortcuts
echo "   Checking for unresolved shortcuts..."
SHORTCUTS=$(find crates/ -name "*.rs" -exec grep -Hn "TODO\|FIXME\|HACK\|XXX" {} \; | \
    grep -v test | wc -l | tr -d ' ' || echo "0")
SHORTCUTS=$(echo "$SHORTCUTS" | head -1 | tr -d ' ')
if [ "$SHORTCUTS" -gt 0 ]; then
    echo -e "${RED}❌ VIOLATION: Found $SHORTCUTS unresolved shortcuts (TODO/FIXME/HACK/XXX)${NC}"
    echo "   Complete implementation or create proper issues"
    find crates/ -name "*.rs" -exec grep -Hn "TODO\|FIXME\|HACK\|XXX" {} \; | \
        grep -v test | head -3 || true
    ((VIOLATIONS++))
fi

# Clone() shortcuts (expensive copies)
echo "   Checking for expensive clone() shortcuts..."
CLONE_SHORTCUTS=$(find crates/ -name "*.rs" -exec grep -Hn "\.clone()" {} \; 2>/dev/null | \
    grep -v test | grep -v "Clone" | wc -l 2>/dev/null || echo "0")
if [ "${CLONE_SHORTCUTS:-0}" -gt 5 ]; then
    echo -e "${YELLOW}⚠️  WARNING: Found $CLONE_SHORTCUTS clone() usages - review for performance${NC}"
    echo "   Prefer borrowing over cloning in hot paths"
fi

# Generic shortcuts (overuse of generics)
echo "   Checking for generic shortcuts..."
GENERIC_ABUSE=$(find crates/ -name "*.rs" -exec grep -Hn "fn.*<.*T.*>.*T" {} \; 2>/dev/null | \
    grep -v test | wc -l 2>/dev/null || echo "0")
if [ "${GENERIC_ABUSE:-0}" -gt 10 ]; then
    echo -e "${YELLOW}⚠️  WARNING: Found $GENERIC_ABUSE generic functions - avoid over-generification${NC}"
fi

# String allocation shortcuts
echo "   Checking for string allocation shortcuts..."
STRING_SHORTCUTS=$(find crates/ -name "*.rs" -exec grep -Hn "to_string()\|format!\|String::from" {} \; 2>/dev/null | \
    grep -v test | wc -l)
if [ "${STRING_SHORTCUTS:-0}" -gt 10 ]; then
    echo -e "${YELLOW}⚠️  WARNING: Found $STRING_SHORTCUTS string allocations - use &str when possible${NC}"
fi

# Collect() shortcuts (unnecessary collections)
echo "   Checking for unnecessary collect() shortcuts..."
COLLECT_SHORTCUTS=$(find crates/ -name "*.rs" -exec grep -Hn "collect()" {} \; 2>/dev/null | \
    grep -v test | wc -l)
if [ "${COLLECT_SHORTCUTS:-0}" -gt 5 ]; then
    echo -e "${YELLOW}⚠️  WARNING: Found $COLLECT_SHORTCUTS collect() usages - prefer iterators${NC}"
fi

# Default::default() shortcuts
echo "   Checking for Default::default() shortcuts..."
DEFAULT_SHORTCUTS=$(find crates/ -name "*.rs" -exec grep -Hn "Default::default()" {} \; 2>/dev/null | \
    grep -v test | wc -l)
if [ "${DEFAULT_SHORTCUTS:-0}" -gt 10 ]; then
    echo -e "${YELLOW}⚠️  WARNING: Found $DEFAULT_SHORTCUTS Default::default() - prefer explicit initialization${NC}"
fi

# Any/anyhow shortcuts for error handling
echo "   Checking for lazy error handling..."
ANYHOW_SHORTCUTS=$(find crates/ -name "*.rs" -exec grep -Hn "anyhow::" {} \; 2>/dev/null | \
    grep -v test | wc -l)
if [ "${ANYHOW_SHORTCUTS:-0}" -gt 3 ]; then
    echo -e "${YELLOW}⚠️  WARNING: Found $ANYHOW_SHORTCUTS anyhow usages - prefer specific error types${NC}"
fi

# Pattern matching shortcuts (ignoring errors with _)
echo "   Checking for pattern matching shortcuts..."
PATTERN_SHORTCUTS=$(find crates/ -name "*.rs" -exec grep -Hn "Err(_)" {} \; 2>/dev/null | \
    grep -v test | wc -l)
if [ "${PATTERN_SHORTCUTS:-0}" -gt 3 ]; then
    echo -e "${RED}❌ VIOLATION: Found $PATTERN_SHORTCUTS ignored error patterns Err(_)${NC}"
    echo "   Handle specific error types, don't ignore with underscore"
    find crates/ -name "*.rs" -exec grep -Hn "Err(_)" {} \; 2>/dev/null | \
        grep -v test | head -3
    ((VIOLATIONS++))
fi

# Unimplemented shortcuts
echo "   Checking for unimplemented shortcuts..."
UNIMPLEMENTED=$(find crates/ -name "*.rs" -exec grep -Hn "unimplemented!()" {} \; 2>/dev/null | \
    grep -v test | wc -l)
if [ "${UNIMPLEMENTED:-0}" -gt 0 ]; then
    echo -e "${RED}❌ VIOLATION: Found $UNIMPLEMENTED unimplemented!() without context${NC}"
    echo "   Use unimplemented!(\"reason\") or complete the implementation"
    ((VIOLATIONS++))
fi

# Placeholder returns
echo "   Checking for placeholder returns..."
PLACEHOLDER_RETURNS=$(find crates/ -name "*.rs" -exec grep -Hn "return.*0\|return.*false\|return.*None" {} \; 2>/dev/null | \
    grep -v test | grep -E "//.*TODO|//.*FIXME|//.*placeholder" | wc -l)
if [ "${PLACEHOLDER_RETURNS:-0}" -gt 0 ]; then
    echo -e "${RED}❌ VIOLATION: Found $PLACEHOLDER_RETURNS placeholder return values${NC}"
    echo "   Complete implementation instead of returning placeholder values"
    ((VIOLATIONS++))
fi

# Magic numbers (common agent pattern)
echo "   Checking for magic numbers..."
MAGIC_NUMBERS=$(find crates/ -name "*.rs" -exec grep -Hn "\b[0-9]\{4,\}\b" {} \; 2>/dev/null | \
    grep -v test | grep -v "const\|static" | wc -l)
if [ "${MAGIC_NUMBERS:-0}" -gt 5 ]; then
    echo -e "${YELLOW}⚠️  WARNING: Found $MAGIC_NUMBERS potential magic numbers${NC}"
    echo "   Use named constants for significant numeric values"
fi

# Suppressed warnings (allow attribute abuse)
echo "   Checking for warning suppression abuse..."
WARNING_SUPPRESSION=$(find crates/ -name "*.rs" -exec grep -Hn "#\[allow(" {} \; 2>/dev/null | \
    grep -v test | wc -l)
if [ "${WARNING_SUPPRESSION:-0}" -gt 10 ]; then
    echo -e "${YELLOW}⚠️  WARNING: Found $WARNING_SUPPRESSION warning suppressions${NC}"
    echo "   Fix warnings instead of suppressing them"
fi

# Overly broad imports (use *)
echo "   Checking for overly broad imports..."
STAR_IMPORTS=$(find crates/ -name "*.rs" -exec grep -Hn "use.*::.*\*" {} \; 2>/dev/null | \
    grep -v test | wc -l)
if [ "${STAR_IMPORTS:-0}" -gt 5 ]; then
    echo -e "${YELLOW}⚠️  WARNING: Found $STAR_IMPORTS wildcard imports${NC}"
    echo "   Use specific imports for better compile times and clarity"
fi

# 2. Check for required patterns
echo -e "\n${BLUE}2. Verifying required patterns...${NC}"

# Error handling
echo "   Checking error handling..."
RESULT_FILES=$(find crates/ -name "*.rs" -exec grep -l "Result<" {} \; 2>/dev/null | grep -v test | wc -l)
ERROR_HANDLING=$(find crates/ -name "*.rs" -exec grep -l "match.*Err\|if.*is_err\|?" {} \; 2>/dev/null | grep -v test | wc -l)

if [ "$RESULT_FILES" -gt 0 ] && [ "$ERROR_HANDLING" -eq 0 ]; then
    echo -e "${RED}❌ VIOLATION: Results defined but no error handling found${NC}"
    ((VIOLATIONS++))
fi

# Performance documentation
echo "   Checking performance documentation..."
PERF_DOCS=$(find crates/ -name "*.rs" -exec grep -l "#.*O(\|#.*Performance\|#.*Latency" {} \; 2>/dev/null | wc -l)
PUB_FUNCTIONS=$(find crates/ -name "*.rs" -exec grep -l "pub fn\|pub async fn" {} \; 2>/dev/null | grep -v test | wc -l)

if [ "$PUB_FUNCTIONS" -gt 5 ] && [ "$PERF_DOCS" -eq 0 ]; then
    echo -e "${YELLOW}⚠️  WARNING: Public functions found but limited performance docs${NC}"
fi

# 3. Check code structure compliance
echo -e "\n${BLUE}3. Checking code structure...${NC}"

# Function size (hot paths should be small)
echo "   Checking function sizes..."
LARGE_FUNCTIONS=0
for file in $(find crates/ -name "*.rs" 2>/dev/null); do
    if [ -f "$file" ]; then
        large_count=$(awk '/^[[:space:]]*fn [^{]*{|^[[:space:]]*pub fn [^{]*{/{f=NR} /^[[:space:]]*}$/{if(f && NR-f>50) {count++} f=0} END{print count+0}' "$file")
        LARGE_FUNCTIONS=$((LARGE_FUNCTIONS + large_count))
    fi
done

if [ "$LARGE_FUNCTIONS" -gt 0 ]; then
    echo -e "${YELLOW}⚠️  WARNING: Found $LARGE_FUNCTIONS functions >50 lines${NC}"
fi

# 4. Quick performance check
echo -e "\n${BLUE}4. Running quick performance checks...${NC}"
if command -v cargo >/dev/null 2>&1; then
    echo "   Building with optimizations..."
    if ! cargo build --release --quiet 2>/dev/null; then
        echo -e "${RED}❌ VIOLATION: Release build failed${NC}"
        ((VIOLATIONS++))
    fi
fi

# Final verdict
echo -e "\n${BLUE}========================${NC}"
if [ "$VIOLATIONS" -eq 0 ]; then
    echo -e "${GREEN}✅ ALL COMPLIANCE CHECKS PASSED${NC}"
    echo -e "${GREEN}   Agent is authorized to proceed${NC}"
    echo "$(date): Compliance check passed" >> .agent_compliance.log
    exit 0
else
    echo -e "${RED}❌ $VIOLATIONS COMPLIANCE VIOLATIONS DETECTED${NC}"
    echo -e "${RED}   COMMIT REJECTED - Fix violations and retry${NC}"
    echo -e "${RED}   Review: docs/developer-guide/QUANTITATIVE_DEVELOPMENT_BEST_PRACTICES.md${NC}"
    echo "$(date): Compliance check failed - $VIOLATIONS violations" >> .agent_compliance.log
    exit 1
fi
