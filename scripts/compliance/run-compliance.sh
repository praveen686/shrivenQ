#!/bin/bash
# Wrapper for external sq-compliance tool
# The tool is located outside the project to avoid self-checking

set -eo pipefail

# Use external compliance tool
BINARY_PATH="/home/praveen/sq-compliance-tools/sq-compliance/target/release/sq-compliance"
PROJECT_PATH="/home/praveen/ShrivenQuant"

# Build the binary if it doesn't exist
if [[ ! -f "$BINARY_PATH" ]]; then
    echo "Building compliance checker..."
    (cd "/home/praveen/sq-compliance-tools/sq-compliance" && cargo build --release --quiet)
fi

# Pass project path and all arguments to the Rust binary
exec "$BINARY_PATH" "$PROJECT_PATH" "$@"