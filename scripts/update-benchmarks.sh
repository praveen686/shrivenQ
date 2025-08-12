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

    local benchmarks_dir="benchmarks"
    mkdir -p "$benchmarks_dir"

    # Run benchmarks and save as new baselines
    if cargo bench --bench engine_benchmarks -- --output-format json > "$benchmarks_dir/engine_baseline.json" 2>/dev/null; then
        echo -e "${GREEN}✅ Engine benchmarks baseline updated${NC}"
    fi

    if cargo bench --bench lob_benchmarks -- --output-format json > "$benchmarks_dir/lob_baseline.json" 2>/dev/null; then
        echo -e "${GREEN}✅ LOB benchmarks baseline updated${NC}"
    fi

    echo -e "${GREEN}✅ Benchmark baselines updated${NC}"
    return 0
}

update_benchmarks
