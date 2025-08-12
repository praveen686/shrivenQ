#!/bin/bash
# scripts/initialize-agent.sh - Complete agent initialization

set -euo pipefail

AGENT_ID="${1:-$(whoami)_$(date +%s)}"

echo "🚀 ShrivenQuant Agent Initialization"
echo "==================================="
echo "Agent ID: $AGENT_ID"

# Export agent ID for session
export SHRIVENQUANT_AGENT_ID="$AGENT_ID"

# Step 1: Present critical instructions
cat << 'EOF'

⚠️  CRITICAL INSTRUCTIONS FOR SHRIVENQUANT DEVELOPMENT ⚠️

You are working on ShrivenQuant, an ultra-low latency quantitative trading system.
FINANCIAL LOSSES and SYSTEM FAILURES result from non-compliance.

BEFORE WRITING ANY CODE, you MUST:
✅ READ: docs/developer-guide/QUANTITATIVE_DEVELOPMENT_BEST_PRACTICES.md
✅ UNDERSTAND: Every DO and DON'T applies to your code
✅ VERIFY: Your code meets ALL performance requirements
✅ TEST: Performance impact of every change

ABSOLUTE PROHIBITIONS (Will cause immediate rejection):
❌ NEVER allocate in hot paths (Vec::new(), String::new(), Box::new())
❌ NEVER use f32/f64 for money calculations
❌ NEVER use panic!() or unwrap() in production code
❌ NEVER use std::collections::HashMap in hot paths
❌ NEVER ignore error handling with Err(_)
❌ NEVER violate the 10μs latency budget
❌ NEVER use underscore prefixes to ignore unused variables (let _unused = ...)
❌ NEVER leave TODO/FIXME/HACK comments without completion
❌ NEVER use clone() as a shortcut to avoid borrowing
❌ NEVER use unimplemented!() without detailed context
❌ NEVER return placeholder values (0, false, None) for unfinished code

MANDATORY REQUIREMENTS:
✅ Use fixed-point arithmetic for all prices
✅ Pre-allocate all collections with known capacity
✅ Use FxHashMap instead of std::HashMap
✅ Handle ALL error cases explicitly
✅ Document performance characteristics
✅ Add benchmarks for new hot-path code

COMPLIANCE VERIFICATION:
- Run: ./scripts/strict-check.sh before any commit
- Verify: All pre-commit hooks pass
- Measure: Performance impact < 1% degradation
- Document: All public APIs and performance guarantees

EOF

# Step 2: Require acknowledgment
while true; do
    read -p "Do you acknowledge and agree to follow ALL requirements above? (yes/no): " response
    case $response in
        [Yy]es* )
            echo "✅ Requirements acknowledged"
            break
            ;;
        [Nn]o* )
            echo "❌ AGENT INITIALIZATION CANCELLED"
            echo "   Cannot proceed without acknowledgment"
            exit 1
            ;;
        * )
            echo "Please answer 'yes' or 'no'"
            ;;
    esac
done

# Step 3: Run agent setup
echo -e "\n🔧 Running agent setup..."
if ! ./scripts/agent-setup.sh; then
    echo "❌ Agent setup failed"
    exit 1
fi

# Step 4: Create agent session file
cat > ".agent_session_${AGENT_ID}" << EOF
{
  "agent_id": "$AGENT_ID",
  "initialization_date": "$(date -Iseconds)",
  "compliance_acknowledged": true,
  "session_status": "active",
  "required_reading": [
    "docs/developer-guide/QUANTITATIVE_DEVELOPMENT_BEST_PRACTICES.md",
    "docs/developer-guide/AGENT_COMPLIANCE_FRAMEWORK.md"
  ],
  "performance_budget": "10μs",
  "violation_count": 0
}
EOF

# Step 5: Final instructions
echo -e "\n✅ Agent initialization complete!"
echo "=================================="
echo "Agent ID: $AGENT_ID"
echo "Session file: .agent_session_${AGENT_ID}"
echo ""
echo "🔍 NEXT STEPS:"
echo "1. Read: docs/developer-guide/QUANTITATIVE_DEVELOPMENT_BEST_PRACTICES.md"
echo "2. Before any commit, run: ./scripts/agent-compliance-check.sh"
echo "3. Use: ./scripts/strict-check.sh for comprehensive validation"
echo ""
echo "⚡ QUICK REFERENCE:"
echo "- Max latency: 10μs for hot paths"
echo "- Use: i64 for prices (NOT f64)"
echo "- Use: FxHashMap (NOT std::HashMap)"
echo "- Handle: ALL errors explicitly"
echo "- Test: Performance impact of changes"
echo ""
echo "🚨 REMEMBER: Non-compliance = Immediate rejection"

# Step 6: Add to shell profile if requested
read -p "Add SHRIVENQUANT_AGENT_ID to your shell profile? (y/n): " add_profile
if [[ "$add_profile" =~ ^[Yy] ]]; then
    echo "export SHRIVENQUANT_AGENT_ID='$AGENT_ID'" >> ~/.bashrc
    echo "✅ Added to ~/.bashrc"
fi

echo "🚀 Ready for compliant development!"
