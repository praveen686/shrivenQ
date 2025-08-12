#!/bin/bash
set -euo pipefail

# ShrivenQ Strict Pre-Compile Checks
# Zero tolerance for code quality issues

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "================================================"
echo "     ShrivenQ Strict Pre-Compile Checks        "
echo "================================================"

FAILED=0

# Check for forbidden patterns in code
echo -e "\n${YELLOW}Checking for forbidden patterns...${NC}"
FORBIDDEN_PATTERNS=(
    "TODO"
    "FIXME"
    "HACK"
    "XXX"
    "STUB"
    "unimplemented!"
    "unreachable!"
    "panic!"
    "unwrap()"
    "expect("
    "dbg!"
    "println!"
    "print!"
    "eprintln!"
    "eprint!"
)

for pattern in "${FORBIDDEN_PATTERNS[@]}"; do
    echo -n "  Checking for $pattern... "
    FOUND=$(rg "$pattern" --type rust -g '!target/*' -g '!*.pb.rs' -g '!**/benches/**' . 2>/dev/null || true)
    if [ -n "$FOUND" ]; then
        echo -e "${RED}FOUND${NC}"
        echo "    Found in:"
        echo "$FOUND" | head -5
        FAILED=1
    else
        echo -e "${GREEN}OK${NC}"
    fi
done

# Run clippy with strict settings
echo -e "\n${YELLOW}Running clippy with strict settings...${NC}"
if cargo clippy --all-targets --all-features -- \
    -D warnings \
    -D clippy::all \
    -D clippy::pedantic \
    -D clippy::nursery \
    -D clippy::cargo \
    -D clippy::unwrap_used \
    -D clippy::expect_used \
    -D clippy::panic \
    -D clippy::unimplemented \
    -D clippy::todo \
    -D clippy::dbg_macro \
    -D clippy::print_stdout \
    -D clippy::print_stderr \
    2>&1 | tee /tmp/clippy_output.txt | grep -E "(error|warning)" > /dev/null; then
    echo -e "${RED}  Clippy found issues${NC}"
    cat /tmp/clippy_output.txt
    FAILED=1
else
    echo -e "${GREEN}  Clippy passed${NC}"
fi

# Check for dead code
echo -e "\n${YELLOW}Checking for dead code...${NC}"
if cargo build --all-targets 2>&1 | grep -E "(dead_code|unused)" > /dev/null; then
    echo -e "${RED}  Found dead code${NC}"
    cargo build --all-targets 2>&1 | grep -E "(dead_code|unused)"
    FAILED=1
else
    echo -e "${GREEN}  No dead code${NC}"
fi

# Run rustfmt check
echo -e "\n${YELLOW}Checking code formatting...${NC}"
if ! cargo fmt --all -- --check > /dev/null 2>&1; then
    echo -e "${RED}  Code needs formatting${NC}"
    echo "  Run: cargo fmt --all"
    FAILED=1
else
    echo -e "${GREEN}  Code is properly formatted${NC}"
fi

# Check documentation
echo -e "\n${YELLOW}Checking documentation...${NC}"
if cargo doc --no-deps --all-features 2>&1 | grep -E "(warning|error)" > /dev/null; then
    echo -e "${RED}  Documentation issues found${NC}"
    cargo doc --no-deps --all-features 2>&1 | grep -E "(warning|error)"
    FAILED=1
else
    echo -e "${GREEN}  Documentation OK${NC}"
fi

# Run tests
echo -e "\n${YELLOW}Running tests...${NC}"
if ! cargo test --all-targets --all-features --quiet > /dev/null 2>&1; then
    echo -e "${RED}  Tests failed${NC}"
    cargo test --all-targets --all-features
    FAILED=1
else
    echo -e "${GREEN}  All tests passed${NC}"
fi

echo -e "\n================================================"
if [ $FAILED -eq 1 ]; then
    echo -e "${RED}STRICT CHECK FAILED${NC}"
    echo "Fix all issues before committing!"
    exit 1
else
    echo -e "${GREEN}ALL CHECKS PASSED${NC}"
    echo "Code meets ShrivenQ quality standards!"
fi
