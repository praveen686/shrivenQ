#!/bin/bash
# scripts/agent-setup.sh - Run this for every new agent session

echo "🤖 ShrivenQuant Agent Compliance Setup"
echo "======================================"

# 1. Verify agent has read the documentation
read -p "Have you read QUANTITATIVE_DEVELOPMENT_BEST_PRACTICES.md? (yes/no): " read_docs
if [ "$read_docs" != "yes" ]; then
    echo "❌ AGENT REJECTED: Must read documentation first"
    exit 1
fi

# 2. Test basic compliance knowledge
echo "📋 Testing compliance knowledge..."

echo "Q1: What arithmetic should be used for money calculations?"
read -p "Answer: " answer1
if [[ ! "$answer1" =~ "fixed-point"|"integer" ]]; then
    echo "❌ INCORRECT: Must use fixed-point/integer arithmetic"
    exit 1
fi

echo "Q2: What is the maximum latency budget for hot paths?"
read -p "Answer: " answer2
if [[ ! "$answer2" =~ "10.*μs"|"10.*us"|"10.*microsecond" ]]; then
    echo "❌ INCORRECT: Hot path budget is 10μs"
    exit 1
fi

echo "Q3: Can you use Vec::new() in hot paths?"
read -p "Answer (yes/no): " answer3
if [ "$answer3" != "no" ]; then
    echo "❌ INCORRECT: Never allocate in hot paths"
    exit 1
fi

# 3. Set up compliance environment
echo "✅ Basic compliance verified"
echo "🔧 Setting up compliance environment..."

# Create directories
mkdir -p .agent_certificates
mkdir -p .compliance_logs

# Export compliance flags
export SHRIVENQUANT_AGENT_COMPLIANCE=1
export RUSTFLAGS="-D warnings -D dead_code -D unused_imports"
export CARGO_INCREMENTAL=1

# Create agent compliance log
echo "$(date): Agent compliance setup completed" >> .agent_compliance.log

echo "✅ Agent setup complete"
echo "📖 Quick reference: docs/developer-guide/QUANTITATIVE_DEVELOPMENT_BEST_PRACTICES.md"
echo "🔍 Run './scripts/strict-check.sh' before any commit"
