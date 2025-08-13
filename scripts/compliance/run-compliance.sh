#!/bin/bash
# Wrapper for compliance checking
# Uses external tool locally, falls back to CI script in CI environment

set -eo pipefail

# Check if we're in CI environment
if [ -n "$CI" ] || [ -n "$GITHUB_ACTIONS" ]; then
    # In CI, use the simplified compliance script
    exec "$(dirname "$0")/run-compliance-ci.sh"
fi

# Local environment - use external compliance tool
BINARY_PATH="/home/praveen/sq-compliance-tools/sq-compliance/target/release/sq-compliance"
PROJECT_PATH="/home/praveen/ShrivenQuant"

# Build the binary if it doesn't exist
if [[ ! -f "$BINARY_PATH" ]]; then
    echo "Building compliance checker..."
    (cd "/home/praveen/sq-compliance-tools/sq-compliance" && cargo build --release --quiet)
fi

# Pass project path and all arguments to the Rust binary
exec "$BINARY_PATH" "$PROJECT_PATH" "$@"