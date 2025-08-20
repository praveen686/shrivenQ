#!/bin/bash

# Test Status Checker Script
# Checks which services have tests and their passing status

echo "========================================="
echo "ShrivenQuant Test Infrastructure Status"
echo "========================================="
echo ""

# Services to check
SERVICES=(
    "oms"
    "portfolio-manager"
    "market-connector"
    "data-aggregator"
    "risk-manager"
    "execution-router"
    "trading-gateway"
    "reporting"
    "options-engine"
    "orderbook"
    "auth-service"
    "backtesting"
    "gateway"
    "common"
    "logging"
    "ml-inference"
    "monitoring"
    "secrets-manager"
    "sentiment-analyzer"
    "trading-strategies"
)

TOTAL_TESTS=0
PASSING_TESTS=0
FAILING_SERVICES=()
PASSING_SERVICES=()
NO_TEST_SERVICES=()

for service in "${SERVICES[@]}"; do
    echo "Checking $service..."
    
    # Run tests and capture output
    TEST_OUTPUT=$(cargo test -p $service --no-fail-fast 2>&1)
    
    # Check if tests exist and compile
    if echo "$TEST_OUTPUT" | grep -q "error: package ID specification"; then
        NO_TEST_SERVICES+=("$service")
        continue
    fi
    
    # Check for compilation errors
    if echo "$TEST_OUTPUT" | grep -q "error: could not compile"; then
        FAILING_SERVICES+=("$service (compilation error)")
        continue
    fi
    
    # Extract test results
    if echo "$TEST_OUTPUT" | grep -q "test result:"; then
        # Extract passed/failed counts
        RESULT=$(echo "$TEST_OUTPUT" | grep "test result:" | tail -1)
        
        if echo "$RESULT" | grep -q "0 passed"; then
            NO_TEST_SERVICES+=("$service")
        elif echo "$RESULT" | grep -q "FAILED"; then
            # Extract numbers
            PASSED=$(echo "$RESULT" | sed -n 's/.*\([0-9]\+\) passed.*/\1/p')
            FAILED=$(echo "$RESULT" | sed -n 's/.*\([0-9]\+\) failed.*/\1/p')
            TOTAL_TESTS=$((TOTAL_TESTS + PASSED + FAILED))
            PASSING_TESTS=$((PASSING_TESTS + PASSED))
            FAILING_SERVICES+=("$service ($PASSED passed, $FAILED failed)")
        else
            # All tests passed
            PASSED=$(echo "$RESULT" | sed -n 's/.*\([0-9]\+\) passed.*/\1/p')
            if [ ! -z "$PASSED" ] && [ "$PASSED" != "0" ]; then
                TOTAL_TESTS=$((TOTAL_TESTS + PASSED))
                PASSING_TESTS=$((PASSING_TESTS + PASSED))
                PASSING_SERVICES+=("$service ($PASSED tests)")
            else
                NO_TEST_SERVICES+=("$service")
            fi
        fi
    else
        NO_TEST_SERVICES+=("$service")
    fi
done

echo ""
echo "========================================="
echo "TEST SUMMARY"
echo "========================================="
echo ""
echo "✅ PASSING SERVICES (${#PASSING_SERVICES[@]}):"
for service in "${PASSING_SERVICES[@]}"; do
    echo "   - $service"
done

echo ""
echo "❌ FAILING SERVICES (${#FAILING_SERVICES[@]}):"
for service in "${FAILING_SERVICES[@]}"; do
    echo "   - $service"
done

echo ""
echo "⚠️  NO TESTS FOUND (${#NO_TEST_SERVICES[@]}):"
for service in "${NO_TEST_SERVICES[@]}"; do
    echo "   - $service"
done

echo ""
echo "========================================="
echo "STATISTICS"
echo "========================================="
echo "Total Tests: $TOTAL_TESTS"
echo "Passing Tests: $PASSING_TESTS"
echo "Failing Tests: $((TOTAL_TESTS - PASSING_TESTS))"
if [ $TOTAL_TESTS -gt 0 ]; then
    COVERAGE=$((PASSING_TESTS * 100 / TOTAL_TESTS))
    echo "Test Success Rate: ${COVERAGE}%"
fi

echo ""
echo "Services with tests: $((${#PASSING_SERVICES[@]} + ${#FAILING_SERVICES[@]}))"
echo "Services without tests: ${#NO_TEST_SERVICES[@]}"
echo "Total services checked: ${#SERVICES[@]}"