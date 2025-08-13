#!/bin/bash
# Wrapper for compliance checking
# Uses internal tools with self-exclusion

set -eo pipefail

# Get the script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Path to internal compliance tool
BINARY_PATH="$PROJECT_ROOT/tools/sq-compliance-tools/sq-compliance/target/release/sq-compliance"

# In CI, build if needed (CI has Rust)
if [ -n "$CI" ] || [ -n "$GITHUB_ACTIONS" ]; then
    if [[ ! -f "$BINARY_PATH" ]]; then
        echo "Building compliance checker for CI..."
        (cd "$PROJECT_ROOT/tools/sq-compliance-tools/sq-compliance" && cargo build --release --quiet)
    fi
fi

# Build the binary if it doesn't exist (local development)
if [[ ! -f "$BINARY_PATH" ]]; then
    echo "Building compliance checker..."
    (cd "$PROJECT_ROOT/tools/sq-compliance-tools/sq-compliance" && cargo build --release --quiet)
fi

# Pass project path and all arguments to the Rust binary
exec "$BINARY_PATH" "$PROJECT_ROOT" "$@"