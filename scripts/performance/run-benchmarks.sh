#!/bin/bash

# ============================================================================
# ShrivenQuant Benchmark Runner
# Runs all performance benchmarks and tracks results
# ============================================================================

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}================================================${NC}"
echo -e "${BLUE}     ShrivenQuant Performance Benchmarks       ${NC}"
echo -e "${BLUE}================================================${NC}"

# Create results directory
RESULTS_DIR=".benchmark-results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
mkdir -p "$RESULTS_DIR/history"

# Track benchmark status
TOTAL_BENCHMARKS=0
PASSED_BENCHMARKS=0
FAILED_BENCHMARKS=0

# Performance requirements (in nanoseconds)
declare -A PERF_REQUIREMENTS=(
    ["v2_apply_fast"]=200
    ["v2_apply_validated"]=900
    ["apply_update"]=200
    ["best_bid_ask"]=50
    ["spread_ticks"]=100
)

# Function to run and validate benchmarks
run_benchmark() {
    local package=$1
    local bench_name=${2:-""}
    
    echo -e "\n${YELLOW}Benchmarking $package${NC}"
    echo "--------------------------------"
    
    local output_file="$RESULTS_DIR/${package}_bench.txt"
    
    # Run the benchmark
    if [ -n "$bench_name" ]; then
        cargo bench --package "$package" --bench "$bench_name" 2>&1 | tee "$output_file"
    else
        cargo bench --package "$package" 2>&1 | tee "$output_file"
    fi
    
    local exit_code=${PIPESTATUS[0]}
    
    if [ $exit_code -eq 0 ]; then
        echo -e "${GREEN}✅ $package benchmarks completed${NC}"
        ((PASSED_BENCHMARKS++))
        
        # Check against performance requirements
        for bench in "${!PERF_REQUIREMENTS[@]}"; do
            local threshold=${PERF_REQUIREMENTS[$bench]}
            local result=$(grep "$bench" "$output_file" 2>/dev/null | grep -oE '[0-9]+(\.[0-9]+)?\s*ns' | grep -oE '[0-9]+(\.[0-9]+)?' | head -1)
            
            if [ -n "$result" ]; then
                # Convert to integer for comparison (remove decimal part)
                local result_int=${result%.*}
                
                if [ "$result_int" -le "$threshold" ]; then
                    echo -e "  ${GREEN}✓${NC} $bench: ${result}ns ≤ ${threshold}ns"
                else
                    echo -e "  ${RED}✗${NC} $bench: ${result}ns > ${threshold}ns (EXCEEDS LIMIT)"
                    ((FAILED_BENCHMARKS++))
                fi
            fi
        done
        
        return 0
    else
        echo -e "${RED}❌ $package benchmarks failed${NC}"
        ((FAILED_BENCHMARKS++))
        return 1
    fi
}

# Function to compare with baseline
compare_with_baseline() {
    local package=$1
    local current_file="$RESULTS_DIR/${package}_bench.txt"
    local baseline_file="$RESULTS_DIR/${package}_baseline.txt"
    
    if [ ! -f "$baseline_file" ]; then
        echo -e "${YELLOW}  Creating new baseline for $package${NC}"
        cp "$current_file" "$baseline_file"
        return 0
    fi
    
    echo -e "\n${BLUE}Regression Analysis for $package${NC}"
    
    # Extract all benchmark results
    local benchmarks=$(grep "time:" "$current_file" | awk '{print $1}' | sort -u)
    
    local regression_detected=false
    for bench in $benchmarks; do
        local current=$(grep "^$bench" "$current_file" | grep -oE '[0-9]+(\.[0-9]+)?\s*ns' | grep -oE '[0-9]+(\.[0-9]+)?' | head -1)
        local baseline=$(grep "^$bench" "$baseline_file" | grep -oE '[0-9]+(\.[0-9]+)?\s*ns' | grep -oE '[0-9]+(\.[0-9]+)?' | head -1)
        
        if [ -n "$current" ] && [ -n "$baseline" ]; then
            # Calculate percentage change
            local change=$(echo "scale=2; (($current - $baseline) * 100) / $baseline" | bc 2>/dev/null || echo "0")
            
            if (( $(echo "$change > 10" | bc -l 2>/dev/null || echo "0") )); then
                echo -e "  ${RED}↓${NC} $bench: ${change}% slower (${baseline}ns → ${current}ns)"
                regression_detected=true
            elif (( $(echo "$change < -10" | bc -l 2>/dev/null || echo "0") )); then
                echo -e "  ${GREEN}↑${NC} $bench: ${change#-}% faster (${baseline}ns → ${current}ns)"
            else
                echo -e "  ${GREEN}≈${NC} $bench: stable (${change}% change)"
            fi
        fi
    done
    
    if [ "$regression_detected" = true ]; then
        echo -e "${RED}⚠️  Performance regression detected!${NC}"
        return 1
    else
        echo -e "${GREEN}✅ No significant regressions${NC}"
        return 0
    fi
}

# Main execution
echo -e "\n${BLUE}Starting Benchmark Suite${NC}"
echo "========================"

# Count total benchmarks
TOTAL_BENCHMARKS=$(find crates -name "*.rs" -path "*/benches/*" | wc -l)
echo "Found $TOTAL_BENCHMARKS benchmark files"

# Run LOB benchmarks (most critical)
echo -e "\n${BLUE}1. Order Book (LOB) Benchmarks${NC}"
run_benchmark "lob" || true

# Run apply_update benchmark specifically
if [ -f "crates/market-data/lob/benches/apply_update.rs" ]; then
    echo -e "\n${BLUE}2. Apply Update Benchmark${NC}"
    run_benchmark "lob" "apply_update" || true
fi

# Run v2_comparison benchmark
if [ -f "crates/market-data/lob/benches/v2_comparison.rs" ]; then
    echo -e "\n${BLUE}3. V2 Comparison Benchmark${NC}"
    run_benchmark "lob" "v2_comparison" || true
fi

# Run storage benchmarks if they exist
if cargo bench --package storage --no-run 2>/dev/null; then
    echo -e "\n${BLUE}4. Storage Benchmarks${NC}"
    run_benchmark "storage" || true
fi

# Run feeds benchmarks if they exist
if cargo bench --package feeds --no-run 2>/dev/null; then
    echo -e "\n${BLUE}5. Feeds Benchmarks${NC}"
    run_benchmark "feeds" || true
fi

# Compare with baselines
echo -e "\n${BLUE}================================================${NC}"
echo -e "${BLUE}           Regression Analysis                 ${NC}"
echo -e "${BLUE}================================================${NC}"

REGRESSION_COUNT=0
for package in lob storage feeds; do
    if [ -f "$RESULTS_DIR/${package}_bench.txt" ]; then
        compare_with_baseline "$package" || ((REGRESSION_COUNT++))
    fi
done

# Save results to history
echo -e "\n${BLUE}Saving Results${NC}"
for file in "$RESULTS_DIR"/*.txt; do
    if [ -f "$file" ]; then
        cp "$file" "$RESULTS_DIR/history/$(basename "$file" .txt)_$TIMESTAMP.txt"
    fi
done
echo "Results saved to $RESULTS_DIR/history/*_$TIMESTAMP.txt"

# Generate summary report
SUMMARY_FILE="$RESULTS_DIR/summary_$TIMESTAMP.md"
{
    echo "# ShrivenQuant Performance Report"
    echo "Generated: $(date)"
    echo ""
    echo "## Summary"
    echo "- Total Benchmarks: $TOTAL_BENCHMARKS"
    echo "- Passed: $PASSED_BENCHMARKS"
    echo "- Failed: $FAILED_BENCHMARKS"
    echo "- Regressions: $REGRESSION_COUNT"
    echo ""
    echo "## Critical Metrics"
    echo "| Benchmark | Result | Requirement | Status |"
    echo "|-----------|--------|-------------|--------|"
    
    for bench in "${!PERF_REQUIREMENTS[@]}"; do
        local threshold=${PERF_REQUIREMENTS[$bench]}
        local result=$(grep "$bench" "$RESULTS_DIR/lob_bench.txt" 2>/dev/null | grep -oE '[0-9]+(\.[0-9]+)?\s*ns' | grep -oE '[0-9]+(\.[0-9]+)?' | head -1)
        
        if [ -n "$result" ]; then
            local result_int=${result%.*}
            local status="✅ PASS"
            if [ "$result_int" -gt "$threshold" ]; then
                status="❌ FAIL"
            fi
            echo "| $bench | ${result}ns | ≤${threshold}ns | $status |"
        fi
    done
} > "$SUMMARY_FILE"

echo -e "\n${BLUE}Summary report: $SUMMARY_FILE${NC}"

# Final verdict
echo -e "\n${BLUE}================================================${NC}"
echo -e "${BLUE}                FINAL VERDICT                  ${NC}"
echo -e "${BLUE}================================================${NC}"

if [ $FAILED_BENCHMARKS -eq 0 ] && [ $REGRESSION_COUNT -eq 0 ]; then
    echo -e "${GREEN}✅ ALL PERFORMANCE CHECKS PASSED!${NC}"
    echo "System maintains ultra-low latency requirements"
    
    # Update baselines on success
    for file in "$RESULTS_DIR"/*_bench.txt; do
        if [ -f "$file" ]; then
            cp "$file" "${file%_bench.txt}_baseline.txt"
        fi
    done
    echo -e "${GREEN}Baselines updated${NC}"
    
    exit 0
else
    echo -e "${RED}❌ PERFORMANCE CHECKS FAILED${NC}"
    echo ""
    echo "Issues detected:"
    [ $FAILED_BENCHMARKS -gt 0 ] && echo "  - $FAILED_BENCHMARKS benchmarks exceeded latency limits"
    [ $REGRESSION_COUNT -gt 0 ] && echo "  - $REGRESSION_COUNT packages showed performance regression"
    echo ""
    echo "Actions required:"
    echo "  1. Review changes in hot path code"
    echo "  2. Run profiler: cargo flamegraph"
    echo "  3. Check allocations: valgrind --tool=massif"
    echo "  4. Review summary: cat $SUMMARY_FILE"
    
    exit 1
fi