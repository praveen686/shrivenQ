#!/bin/bash
# Benchmark Baseline Update - Update performance baselines when needed

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "📈 Benchmark Baseline Update"

update_benchmarks() {
    echo "🔄 Updating performance baselines..."

    local benchmarks_dir=".benchmark-results"
    mkdir -p "$benchmarks_dir"

    # Run LOB benchmarks and save as baseline
    if cargo bench --package lob 2>&1 | tee "$benchmarks_dir/lob_bench.txt"; then
        cp "$benchmarks_dir/lob_bench.txt" "$benchmarks_dir/lob_baseline.txt"
        echo -e "${GREEN}✅ LOB benchmarks baseline updated${NC}"
    else
        echo -e "${YELLOW}⚠️  LOB benchmarks failed${NC}"
    fi

    # Run storage benchmarks if they exist
    if cargo bench --package storage 2>/dev/null | tee "$benchmarks_dir/storage_bench.txt"; then
        cp "$benchmarks_dir/storage_bench.txt" "$benchmarks_dir/storage_baseline.txt"
        echo -e "${GREEN}✅ Storage benchmarks baseline updated${NC}"
    else
        echo -e "${YELLOW}⚠️  No storage benchmarks found${NC}"
    fi

    # Run feeds benchmarks if they exist
    if cargo bench --package feeds 2>/dev/null | tee "$benchmarks_dir/feeds_bench.txt"; then
        cp "$benchmarks_dir/feeds_bench.txt" "$benchmarks_dir/feeds_baseline.txt"
        echo -e "${GREEN}✅ Feeds benchmarks baseline updated${NC}"
    else
        echo -e "${YELLOW}⚠️  No feeds benchmarks found${NC}"
    fi

    echo -e "${GREEN}✅ Benchmark baselines updated${NC}"
    return 0
}

update_benchmarks
