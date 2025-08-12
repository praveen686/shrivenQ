#!/bin/bash
# Hot Path Allocation Detection for Ultra-Low Latency Trading
# Zero tolerance for allocations in performance-critical code

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo "🧠 Hot Path Allocation Detection Starting..."

# Configuration
HOT_PATH_MODULES=(
    "engine/src/core.rs"
    "engine/src/execution.rs"
    "engine/src/position.rs"
    "engine/src/risk.rs"
    "engine/src/metrics.rs"
    "lob/src/orderbook.rs"
    "bus/src/lib.rs"
)

HOT_PATH_FUNCTIONS=(
    "on_tick"
    "send_order"
    "on_fill"
    "apply_fill"
    "check_order"
    "apply_update"
    "update_market"
)

FORBIDDEN_ALLOCATIONS=(
    "Vec::new"
    "Vec::with_capacity"
    "HashMap::new"
    "HashSet::new"
    "BTreeMap::new"
    "String::new"
    "format!"
    "println!"
    "eprintln!"
    "Box::new"
    "Rc::new"
    "Arc::new"
    "RefCell::new"
    "Mutex::new"
    "RwLock::new"
    "thread::spawn"
    "tokio::spawn"
    "async"
    "await"
)

# Function to scan for forbidden patterns in hot paths
scan_hot_path_allocations() {
    local violations=0

    echo "🔍 Scanning hot path modules for allocations..."

    for module in "${HOT_PATH_MODULES[@]}"; do
        if [[ ! -f "$module" ]]; then
            echo -e "${YELLOW}⚠️  Module not found: $module${NC}"
            continue
        fi

        echo -e "${BLUE}  📁 Scanning: $module${NC}"

        # Extract hot path functions
        local hot_functions=""
        for func in "${HOT_PATH_FUNCTIONS[@]}"; do
            if grep -q "fn $func" "$module"; then
                hot_functions+="$func "
            fi
        done

        if [[ -n "$hot_functions" ]]; then
            echo -e "${BLUE}    🎯 Hot functions found: $hot_functions${NC}"

            # Check each hot function for forbidden allocations
            for func in $hot_functions; do
                local func_content=$(awk "/fn $func/,/^}/" "$module" 2>/dev/null || echo "")

                if [[ -n "$func_content" ]]; then
                    for allocation in "${FORBIDDEN_ALLOCATIONS[@]}"; do
                        if echo "$func_content" | grep -q "$allocation"; then
                            echo -e "${RED}❌ VIOLATION: $allocation found in $func() in $module${NC}"

                            # Show the exact line
                            local line_num=$(echo "$func_content" | grep -n "$allocation" | cut -d: -f1 | head -1)
                            local context=$(echo "$func_content" | sed -n "${line_num}p" | xargs)
                            echo -e "${RED}   Line: $context${NC}"
                            violations=$((violations + 1))
                        fi
                    done
                fi
            done
        fi
    done

    return $violations
}

# Function to check for panic-inducing code
check_panic_sources() {
    echo "🚨 Checking for panic sources in hot paths..."

    local violations=0

    PANIC_SOURCES=(
        "unwrap()"
        "expect("
        "panic!"
        "unimplemented!"
        "todo!"
        "unreachable!()"
        "[index]"  # Direct indexing without bounds check
    )

    for module in "${HOT_PATH_MODULES[@]}"; do
        if [[ ! -f "$module" ]]; then
            continue
        fi

        echo -e "${BLUE}  📁 Checking panics in: $module${NC}"

        for panic_source in "${PANIC_SOURCES[@]}"; do
            if grep -n "$panic_source" "$module" >/dev/null 2>&1; then
                echo -e "${RED}❌ PANIC SOURCE: $panic_source found in $module${NC}"
                grep -n "$panic_source" "$module" | while read -r line; do
                    echo -e "${RED}   $line${NC}"
                done
                violations=$((violations + 1))
            fi
        done
    done

    return $violations
}

# Function to check for async code in hot paths
check_async_code() {
    echo "⚡ Checking for async code in hot paths..."

    local violations=0

    for module in "${HOT_PATH_MODULES[@]}"; do
        if [[ ! -f "$module" ]]; then
            continue
        fi

        echo -e "${BLUE}  📁 Checking async in: $module${NC}"

        # Check for async functions in hot path modules
        if grep -n "async fn" "$module" >/dev/null 2>&1; then
            local async_functions=$(grep -n "async fn" "$module")

            # Check if any async functions are hot path functions
            for func in "${HOT_PATH_FUNCTIONS[@]}"; do
                if echo "$async_functions" | grep -q "async fn $func"; then
                    echo -e "${RED}❌ ASYNC VIOLATION: async fn $func found in $module${NC}"
                    violations=$((violations + 1))
                fi
            done
        fi

        # Check for .await in hot path functions
        for func in "${HOT_PATH_FUNCTIONS[@]}"; do
            if grep -q "fn $func" "$module"; then
                local func_content=$(awk "/fn $func/,/^}/" "$module" 2>/dev/null || echo "")
                if echo "$func_content" | grep -q "\.await"; then
                    echo -e "${RED}❌ AWAIT VIOLATION: .await found in $func() in $module${NC}"
                    violations=$((violations + 1))
                fi
            fi
        done
    done

    return $violations
}

# Function to check for logging in hot paths
check_logging_in_hot_paths() {
    echo "📝 Checking for logging in hot paths..."

    local violations=0

    LOGGING_MACROS=(
        "println!"
        "eprintln!"
        "dbg!"
        "log::"
        "trace!"
        "debug!"
        "info!"
        "warn!"
        "error!"
    )

    for module in "${HOT_PATH_MODULES[@]}"; do
        if [[ ! -f "$module" ]]; then
            continue
        fi

        echo -e "${BLUE}  📁 Checking logging in: $module${NC}"

        for func in "${HOT_PATH_FUNCTIONS[@]}"; do
            if grep -q "fn $func" "$module"; then
                local func_content=$(awk "/fn $func/,/^}/" "$module" 2>/dev/null || echo "")

                for log_macro in "${LOGGING_MACROS[@]}"; do
                    if echo "$func_content" | grep -q "$log_macro"; then
                        # Allow trace! and debug! as they're compile-time removable
                        if [[ "$log_macro" == "trace!" ]] || [[ "$log_macro" == "debug!" ]]; then
                            echo -e "${YELLOW}⚠️  DEBUG LOGGING: $log_macro in $func() in $module (OK if debug_assertions)${NC}"
                        else
                            echo -e "${RED}❌ LOGGING VIOLATION: $log_macro found in $func() in $module${NC}"
                            violations=$((violations + 1))
                        fi
                    fi
                done
            fi
        done
    done

    return $violations
}

# Function to check memory usage patterns
check_memory_patterns() {
    echo "💾 Analyzing memory usage patterns..."

    local violations=0

    # Check for stack allocation patterns that might be too large
    for module in "${HOT_PATH_MODULES[@]}"; do
        if [[ ! -f "$module" ]]; then
            continue
        fi

        echo -e "${BLUE}  📁 Checking memory patterns in: $module${NC}"

        # Look for large array allocations on stack
        if grep -n "\[.*; [0-9][0-9][0-9][0-9].*\]" "$module" >/dev/null 2>&1; then
            echo -e "${YELLOW}⚠️  Large stack allocation detected in $module${NC}"
            grep -n "\[.*; [0-9][0-9][0-9][0-9].*\]" "$module" | while read -r line; do
                echo -e "${YELLOW}   $line${NC}"
            done
        fi

        # Check for potential memory leaks (missing drops)
        if grep -n "mem::forget" "$module" >/dev/null 2>&1; then
            echo -e "${RED}❌ MEMORY LEAK: mem::forget found in $module${NC}"
            violations=$((violations + 1))
        fi
    done

    return $violations
}

# Function to run allocation tests
run_allocation_tests() {
    echo "🧪 Running allocation detection tests..."

    # Check if allocation testing is available
    if ! cargo test --help | grep -q "test"; then
        echo -e "${YELLOW}⚠️  Cargo test not available${NC}"
        return 0
    fi

    # Run tests that specifically check for allocations
    echo "  🔍 Running no-allocation tests..."

    # Look for tests with "no_alloc" in their name
    local no_alloc_tests=$(cargo test --dry-run 2>/dev/null | grep "no_alloc" || echo "")

    if [[ -n "$no_alloc_tests" ]]; then
        echo -e "${BLUE}  🧪 Found allocation tests: $no_alloc_tests${NC}"

        if ! cargo test --release -- no_alloc --exact; then
            echo -e "${RED}❌ No-allocation tests failed${NC}"
            return 1
        else
            echo -e "${GREEN}✅ No-allocation tests passed${NC}"
        fi
    else
        echo -e "${YELLOW}⚠️  No allocation tests found (recommended to add)${NC}"
    fi

    return 0
}

# Function to generate allocation report
generate_allocation_report() {
    echo "📊 Generating allocation analysis report..."

    local report_file="allocation_analysis_report.md"

    cat > "$report_file" << EOF
# Hot Path Allocation Analysis Report

Generated: $(date)

## Summary

This report analyzes potential allocations and performance issues in hot path code.

## Scanned Modules

EOF

    for module in "${HOT_PATH_MODULES[@]}"; do
        echo "- $module" >> "$report_file"
    done

    cat >> "$report_file" << EOF

## Hot Path Functions

EOF

    for func in "${HOT_PATH_FUNCTIONS[@]}"; do
        echo "- $func()" >> "$report_file"
    done

    cat >> "$report_file" << EOF

## Analysis Results

### Allocation Violations
$(scan_hot_path_allocations 2>&1 | grep "❌" | wc -l) violations found

### Panic Sources
$(check_panic_sources 2>&1 | grep "❌" | wc -l) panic sources found

### Async Code Issues
$(check_async_code 2>&1 | grep "❌" | wc -l) async violations found

### Logging Issues
$(check_logging_in_hot_paths 2>&1 | grep "❌" | wc -l) logging violations found

## Recommendations

1. Use pre-allocated data structures where possible
2. Avoid heap allocations in hot paths
3. Use const functions for compile-time evaluation
4. Prefer stack allocation for small, fixed-size data
5. Use object pools for reusable objects
6. Profile regularly to catch regressions

EOF

    echo -e "${GREEN}✅ Report generated: $report_file${NC}"
}

# Main function
main() {
    echo "🎯 Hot Path Allocation Analysis"
    echo "==============================="

    local total_violations=0

    # Run all checks
    echo ""
    echo "1️⃣  Scanning for forbidden allocations..."
    local alloc_violations=0
    scan_hot_path_allocations || alloc_violations=$?
    total_violations=$((total_violations + alloc_violations))

    echo ""
    echo "2️⃣  Checking for panic sources..."
    local panic_violations=0
    check_panic_sources || panic_violations=$?
    total_violations=$((total_violations + panic_violations))

    echo ""
    echo "3️⃣  Checking for async code..."
    local async_violations=0
    check_async_code || async_violations=$?
    total_violations=$((total_violations + async_violations))

    echo ""
    echo "4️⃣  Checking for logging..."
    local logging_violations=0
    check_logging_in_hot_paths || logging_violations=$?
    total_violations=$((total_violations + logging_violations))

    echo ""
    echo "5️⃣  Analyzing memory patterns..."
    local memory_violations=0
    check_memory_patterns || memory_violations=$?
    total_violations=$((total_violations + memory_violations))

    echo ""
    echo "6️⃣  Running allocation tests..."
    local test_violations=0
    run_allocation_tests || test_violations=$?
    total_violations=$((total_violations + test_violations))

    echo ""
    echo "7️⃣  Generating report..."
    generate_allocation_report

    # Summary
    echo ""
    echo "📋 Analysis Summary"
    echo "==================="
    echo "Allocation violations: $alloc_violations"
    echo "Panic sources: $panic_violations"
    echo "Async violations: $async_violations"
    echo "Logging violations: $logging_violations"
    echo "Memory violations: $memory_violations"
    echo "Test failures: $test_violations"
    echo ""
    echo "Total violations: $total_violations"

    if [[ $total_violations -eq 0 ]]; then
        echo -e "${GREEN}🎉 No hot path allocation issues found!${NC}"
        echo -e "${GREEN}   Code is optimized for ultra-low latency${NC}"
        return 0
    else
        echo -e "${RED}❌ $total_violations allocation issues found!${NC}"
        echo -e "${RED}   Hot path optimization required before commit${NC}"
        return 1
    fi
}

# Execute main function
main "$@"
