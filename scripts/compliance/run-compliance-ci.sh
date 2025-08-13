#!/bin/bash
# CI-specific compliance checker
# Builds and runs compliance tools in CI environment

set -eo pipefail

# For CI, we'll build a minimal compliance checker inline
# This is a temporary solution until we properly package the compliance tools

echo "Running CI compliance checks..."

# Basic compliance checks that can run in CI
check_errors() {
    echo "Checking for Err(_) patterns..."
    if grep -r "Err(_)" --include="*.rs" crates/ scripts/ 2>/dev/null | grep -v "test" | grep -v "bench"; then
        echo "❌ Found Err(_) patterns - must handle errors properly"
        return 1
    fi
    echo "✓ No Err(_) patterns found"
    return 0
}

check_unwraps() {
    echo "Checking for .unwrap() usage..."
    local count=$(grep -r "\.unwrap()" --include="*.rs" crates/ | grep -v "test" | grep -v "bench" | wc -l)
    if [ "$count" -gt "50" ]; then
        echo "❌ Too many .unwrap() calls: $count (max 50)"
        return 1
    fi
    echo "✓ Unwrap usage acceptable: $count"
    return 0
}

check_todos() {
    echo "Checking for TODO/FIXME markers..."
    if grep -r "TODO\|FIXME\|HACK\|XXX" --include="*.rs" crates/ 2>/dev/null | grep -v "test"; then
        echo "⚠️ Found TODO/FIXME markers (warning only)"
    else
        echo "✓ No TODO/FIXME markers found"
    fi
    return 0
}

# Run checks
FAILED=0

check_errors || FAILED=1
check_unwraps || FAILED=1
check_todos || true  # Don't fail on TODOs

if [ "$FAILED" -eq "1" ]; then
    echo "❌ Compliance check failed"
    exit 1
fi

echo "✅ Basic compliance checks passed"
exit 0