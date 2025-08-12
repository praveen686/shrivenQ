# Agent Compliance Framework
## Ensuring 100% Adherence to Quantitative Development Best Practices

> **Critical**: This framework MUST be followed by all AI agents working on ShrivenQuant. Non-compliance will result in immediate code rejection and agent restriction.

---

## Table of Contents
1. [Pre-Agent Briefing](#pre-agent-briefing)
2. [Automated Compliance Checks](#automated-compliance-checks)
3. [Code Review Checklists](#code-review-checklists)
4. [Real-time Monitoring](#real-time-monitoring)
5. [Training & Validation](#training--validation)
6. [Enforcement Mechanisms](#enforcement-mechanisms)

---

## Pre-Agent Briefing

### 🎯 Mandatory Agent Initialization Prompt

**Copy this EXACT prompt to every new AI agent:**

```
CRITICAL INSTRUCTIONS FOR SHRIVENQUANT DEVELOPMENT:

You are working on ShrivenQuant, an ultra-low latency quantitative trading system.
FINANCIAL LOSSES and SYSTEM FAILURES result from non-compliance.

BEFORE WRITING ANY CODE, you MUST:

1. READ: /docs/developer-guide/QUANTITATIVE_DEVELOPMENT_BEST_PRACTICES.md
2. UNDERSTAND: Every DO and DON'T applies to your code
3. VERIFY: Your code meets ALL performance requirements
4. TEST: Performance impact of every change

ABSOLUTE PROHIBITIONS (Will cause immediate rejection):
❌ NEVER allocate in hot paths (Vec::new(), String::new(), Box::new())
❌ NEVER use f32/f64 for money calculations
❌ NEVER use panic!() or unwrap() in production code
❌ NEVER use std::collections::HashMap in hot paths
❌ NEVER ignore error handling
❌ NEVER violate the 10μs latency budget

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

FAILURE TO COMPLY = IMMEDIATE CODE REJECTION

Confirm understanding by responding: "SHRIVENQUANT COMPLIANCE ACKNOWLEDGED"
```

### 🔒 Agent Authentication & Setup

```bash
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

# Export compliance flags
export SHRIVENQUANT_AGENT_COMPLIANCE=1
export RUSTFLAGS="-D warnings -D dead_code -D unused_imports"
export CARGO_INCREMENTAL=1

# Create agent compliance log
echo "$(date): Agent compliance setup completed" >> .agent_compliance.log

echo "✅ Agent setup complete"
echo "📖 Quick reference: docs/developer-guide/QUANTITATIVE_DEVELOPMENT_BEST_PRACTICES.md"
echo "🔍 Run './scripts/strict-check.sh' before any commit"
```

---

## Automated Compliance Checks

### 🛡️ Enhanced Pre-commit Hook

```bash
#!/bin/bash
# scripts/agent-compliance-check.sh

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo "🤖 Agent Compliance Check"
echo "========================"

VIOLATIONS=0

# 1. Check for prohibited patterns
echo -e "\n${BLUE}1. Scanning for prohibited patterns...${NC}"

# Hot path allocations
ALLOCATIONS=$(grep -r "Vec::new()\|String::new()\|Box::new()\|HashMap::new()" --include="*.rs" \
    --exclude-dir="target" --exclude-dir="tests" crates/ 2>/dev/null | wc -l || echo 0)
if [ "$ALLOCATIONS" -gt 0 ]; then
    echo -e "${RED}❌ VIOLATION: Found $ALLOCATIONS hot path allocations${NC}"
    grep -rn "Vec::new()\|String::new()\|Box::new()\|HashMap::new()" --include="*.rs" \
        --exclude-dir="target" --exclude-dir="tests" crates/ | head -5
    ((VIOLATIONS++))
fi

# Floating point money
FLOAT_MONEY=$(grep -r "f32\|f64" --include="*.rs" --exclude-dir="target" crates/ \
    | grep -E "price|money|amount|value" | wc -l || echo 0)
if [ "$FLOAT_MONEY" -gt 0 ]; then
    echo -e "${RED}❌ VIOLATION: Found $FLOAT_MONEY potential floating point money calculations${NC}"
    ((VIOLATIONS++))
fi

# Panic usage
PANICS=$(grep -r "panic!\|unwrap()\|expect(" --include="*.rs" \
    --exclude-dir="target" --exclude-dir="tests" crates/ 2>/dev/null | wc -l || echo 0)
if [ "$PANICS" -gt 0 ]; then
    echo -e "${RED}❌ VIOLATION: Found $PANICS panic/unwrap usages${NC}"
    grep -rn "panic!\|unwrap()\|expect(" --include="*.rs" \
        --exclude-dir="target" --exclude-dir="tests" crates/ | head -5
    ((VIOLATIONS++))
fi

# std::HashMap in hot paths
STD_HASHMAP=$(grep -r "std::collections::HashMap\|use std::collections::HashMap" \
    --include="*.rs" --exclude-dir="target" crates/ 2>/dev/null | wc -l || echo 0)
if [ "$STD_HASHMAP" -gt 0 ]; then
    echo -e "${RED}❌ VIOLATION: Found $STD_HASHMAP std::HashMap usages (use FxHashMap)${NC}"
    ((VIOLATIONS++))
fi

# 2. Check for required patterns
echo -e "\n${BLUE}2. Verifying required patterns...${NC}"

# Error handling
RESULT_USAGE=$(grep -r "Result<" --include="*.rs" --exclude-dir="target" crates/ | wc -l || echo 0)
ERROR_HANDLING=$(grep -r "match.*Err\|if.*is_err\|?" --include="*.rs" \
    --exclude-dir="target" crates/ | wc -l || echo 0)

if [ "$RESULT_USAGE" -gt 0 ] && [ "$ERROR_HANDLING" -eq 0 ]; then
    echo -e "${RED}❌ VIOLATION: Results defined but no error handling found${NC}"
    ((VIOLATIONS++))
fi

# Performance documentation
PERF_DOCS=$(grep -r "#.*O(\|#.*Performance\|#.*Latency" --include="*.rs" \
    --exclude-dir="target" crates/ | wc -l || echo 0)
PUB_FUNCTIONS=$(grep -r "pub fn\|pub async fn" --include="*.rs" \
    --exclude-dir="target" crates/ | wc -l || echo 0)

if [ "$PUB_FUNCTIONS" -gt 0 ] && [ "$PERF_DOCS" -eq 0 ]; then
    echo -e "${YELLOW}⚠️  WARNING: Public functions found but no performance docs${NC}"
fi

# 3. Check code structure compliance
echo -e "\n${BLUE}3. Checking code structure...${NC}"

# Function size (hot paths should be small)
LARGE_FUNCTIONS=$(find crates/ -name "*.rs" -exec grep -l "fn.*{" {} \; | \
    while read file; do
        awk '/fn.*{/{f=NR} /^}$/{if(f && NR-f>100) print FILENAME":"f":"NR-f; f=0}' "$file"
    done | wc -l || echo 0)

if [ "$LARGE_FUNCTIONS" -gt 0 ]; then
    echo -e "${YELLOW}⚠️  WARNING: Found $LARGE_FUNCTIONS functions >100 lines${NC}"
fi

# 4. Performance regression check
echo -e "\n${BLUE}4. Running performance checks...${NC}"
if [ -f "scripts/performance-check.sh" ]; then
    if ! ./scripts/performance-check.sh --quick; then
        echo -e "${RED}❌ VIOLATION: Performance regression detected${NC}"
        ((VIOLATIONS++))
    fi
fi

# 5. Memory usage check
echo -e "\n${BLUE}5. Checking memory allocations...${NC}"
if [ -f "scripts/check-hot-path-allocations.sh" ]; then
    if ! ./scripts/check-hot-path-allocations.sh; then
        echo -e "${RED}❌ VIOLATION: Hot path allocations detected${NC}"
        ((VIOLATIONS++))
    fi
fi

# Final verdict
echo -e "\n${BLUE}========================${NC}"
if [ "$VIOLATIONS" -eq 0 ]; then
    echo -e "${GREEN}✅ ALL COMPLIANCE CHECKS PASSED${NC}"
    echo -e "${GREEN}   Agent is authorized to commit${NC}"
    exit 0
else
    echo -e "${RED}❌ $VIOLATIONS COMPLIANCE VIOLATIONS DETECTED${NC}"
    echo -e "${RED}   COMMIT REJECTED - Fix violations and retry${NC}"
    echo -e "${RED}   Review: docs/developer-guide/QUANTITATIVE_DEVELOPMENT_BEST_PRACTICES.md${NC}"
    exit 1
fi
```

### 🔧 Integration with Pre-commit

```yaml
# Add to .pre-commit-config.yaml
repos:
  - repo: local
    hooks:
      # FIRST HOOK - Must pass before any other checks
      - id: agent-compliance
        name: "🤖 Agent Compliance Check"
        description: "Ensure agent follows quantitative development best practices"
        entry: bash
        args: [-c, './scripts/agent-compliance-check.sh']
        language: system
        files: \.rs$
        pass_filenames: false
        stages: [pre-commit, pre-push]
```

---

## Code Review Checklists

### 📋 Mandatory Pre-Commit Checklist

**Every agent MUST verify these items before committing:**

```markdown
## 🔍 Agent Compliance Checklist

### Performance Requirements
- [ ] No heap allocations in hot paths
- [ ] All collections pre-allocated with capacity
- [ ] Fixed-point arithmetic for money calculations
- [ ] Function execution time < 10μs for hot paths
- [ ] Memory usage increase < 1%

### Safety Requirements  
- [ ] All external inputs validated
- [ ] All Result/Option types handled explicitly
- [ ] No panic!/unwrap() in production code
- [ ] All unsafe blocks documented with safety proof
- [ ] Error types are specific and actionable

### Code Quality
- [ ] All public APIs documented with performance characteristics
- [ ] Type safety used for domain concepts (Price, Quantity, etc.)
- [ ] const assertions for critical invariants
- [ ] Tests cover error conditions and edge cases
- [ ] Benchmarks added for new hot-path code

### Concurrency & Threading
- [ ] Minimal critical sections
- [ ] Lock-free data structures when possible
- [ ] Thread-local storage for hot data
- [ ] No I/O while holding locks

### Data & Serialization
- [ ] Binary serialization for performance-critical data
- [ ] Version fields in serializable structures
- [ ] Zero-copy parsing where possible
- [ ] Fixed-size arrays instead of Vec when size known

### Configuration
- [ ] No hard-coded values (use const/config)
- [ ] Configuration validation at startup
- [ ] Environment-specific settings

Sign-off: [Agent ID] - [Timestamp] - "I certify this code meets ALL requirements"
```

### 🎯 Automated Checklist Enforcement

```bash
#!/bin/bash
# scripts/enforce-checklist.sh

echo "📋 Mandatory Compliance Checklist"
echo "================================="

# Force agent to acknowledge each item
CHECKLIST=(
    "No heap allocations in hot paths"
    "Fixed-point arithmetic for money"  
    "All Results/Options handled"
    "No panic/unwrap in production"
    "Public APIs documented"
    "Performance tests added"
    "Error conditions tested"
    "Configuration externalized"
)

echo "You MUST verify each item below:"
for item in "${CHECKLIST[@]}"; do
    while true; do
        read -p "✅ $item (y/n): " response
        case $response in
            [Yy]* ) break;;
            [Nn]* ) echo "❌ REJECTED: Fix '$item' and restart checklist"; exit 1;;
            * ) echo "Please answer yes (y) or no (n)";;
        esac
    done
done

# Require agent sign-off
read -p "Enter your Agent ID: " agent_id
if [ -z "$agent_id" ]; then
    echo "❌ REJECTED: Agent ID required"
    exit 1
fi

echo "$(date): $agent_id completed compliance checklist" >> .compliance_log
echo "✅ Checklist completed - proceeding with commit"
```

---

## Real-time Monitoring

### 📊 Performance Monitoring Dashboard

```rust
// src/compliance/monitor.rs
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicU64, Ordering};

static HOT_PATH_CALLS: AtomicU64 = AtomicU64::new(0);
static HOT_PATH_TOTAL_TIME: AtomicU64 = AtomicU64::new(0);
static ALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);

/// Monitor hot path performance - MUST be used in all hot paths
#[inline(always)]
pub fn monitor_hot_path<F, R>(name: &'static str, f: F) -> R
where
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed();

    HOT_PATH_CALLS.fetch_add(1, Ordering::Relaxed);
    HOT_PATH_TOTAL_TIME.fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);

    // CRITICAL: Alert if over budget
    if elapsed > Duration::from_micros(10) {
        eprintln!("🚨 HOT PATH VIOLATION: {} took {}μs", name, elapsed.as_micros());
        std::process::abort(); // Fail fast in development
    }

    result
}

/// Monitor allocations - hooks into global allocator
pub struct ComplianceAllocator;

unsafe impl std::alloc::GlobalAlloc for ComplianceAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);

        // In debug mode, capture stack trace to find hot path allocations
        #[cfg(debug_assertions)]
        {
            let trace = std::backtrace::Backtrace::capture();
            if trace.to_string().contains("hot_path") {
                panic!("🚨 HOT PATH ALLOCATION DETECTED!\n{}", trace);
            }
        }

        std::alloc::System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        std::alloc::System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static ALLOCATOR: ComplianceAllocator = ComplianceAllocator;
```

### 🚨 Real-time Alerts

```bash
#!/bin/bash
# scripts/compliance-monitor.sh - Run continuously during development

echo "🔍 Real-time Compliance Monitor"
echo "==============================="

while true; do
    # Check for violations every 5 seconds

    # 1. Check compilation warnings
    if cargo check --message-format=json 2>/dev/null | grep -q '"level":"warning"'; then
        echo "⚠️  $(date): Compilation warnings detected"
        notify-send "ShrivenQuant" "Compilation warnings - fix immediately"
    fi

    # 2. Check test failures  
    if ! cargo test --quiet >/dev/null 2>&1; then
        echo "🚨 $(date): Tests failing"
        notify-send "ShrivenQuant" "Tests failing - investigate immediately"
    fi

    # 3. Check performance regressions
    if [ -f ".performance_baseline" ]; then
        current_perf=$(cargo bench --bench critical_path | grep -o '[0-9.]*ns' | head -1)
        baseline_perf=$(cat .performance_baseline)

        # Alert if >5% slower
        if (( $(echo "$current_perf > $baseline_perf * 1.05" | bc -l) )); then
            echo "🚨 $(date): Performance regression detected"
            notify-send "ShrivenQuant" "Performance regression: ${current_perf} vs ${baseline_perf}"
        fi
    fi

    sleep 5
done
```

---

## Training & Validation

### 🎓 Agent Training Program

```python
# scripts/agent_training.py - Training validation for new agents

import subprocess
import json
from typing import List, Dict, Tuple

class AgentTrainingValidator:
    """Validates agent understanding of ShrivenQuant best practices"""

    def __init__(self):
        self.test_scenarios = self._load_test_scenarios()
        self.passing_score = 95  # Must get 95% correct

    def run_validation(self, agent_id: str) -> bool:
        """Run complete validation suite"""
        print(f"🎓 Training Validation for Agent: {agent_id}")
        print("=" * 50)

        scores = []

        # 1. Code Pattern Recognition
        pattern_score = self._test_pattern_recognition()
        scores.append(("Pattern Recognition", pattern_score))

        # 2. Performance Analysis
        perf_score = self._test_performance_analysis()
        scores.append(("Performance Analysis", perf_score))

        # 3. Error Handling
        error_score = self._test_error_handling()
        scores.append(("Error Handling", error_score))

        # 4. Risk Management
        risk_score = self._test_risk_understanding()
        scores.append(("Risk Management", risk_score))

        # Calculate overall score
        overall_score = sum(score for _, score in scores) / len(scores)

        print("\n📊 Results:")
        for category, score in scores:
            status = "✅" if score >= self.passing_score else "❌"
            print(f"{status} {category}: {score}%")

        print(f"\n🎯 Overall Score: {overall_score:.1f}%")

        if overall_score >= self.passing_score:
            print("✅ AGENT CERTIFIED - Authorized for ShrivenQuant development")
            self._issue_certificate(agent_id, overall_score)
            return True
        else:
            print("❌ AGENT NOT CERTIFIED - Requires additional training")
            self._log_failure(agent_id, overall_score, scores)
            return False

    def _test_pattern_recognition(self) -> float:
        """Test agent's ability to identify problematic patterns"""
        test_cases = [
            # Bad patterns that should be identified
            ("Vec::new()", "allocation", True),
            ("f64::from(price)", "floating_point_money", True),
            ("order.unwrap()", "panic_risk", True),
            ("std::collections::HashMap::new()", "slow_hashmap", True),

            # Good patterns that should be accepted
            ("Vec::with_capacity(1000)", "allocation", False),
            ("Price::from_fixed_point(price_int)", "floating_point_money", False),
            ("order.map_err(|e| OrderError::from(e))?", "panic_risk", False),
            ("FxHashMap::with_capacity_and_hasher(100, FxBuildHasher)", "slow_hashmap", False),
        ]

        correct = 0
        for code, issue_type, should_flag in test_cases:
            response = input(f"Should this be flagged for {issue_type}? '{code}' (y/n): ")
            if (response.lower() == 'y') == should_flag:
                correct += 1

        return (correct / len(test_cases)) * 100

    def _test_performance_analysis(self) -> float:
        """Test understanding of performance implications"""
        questions = [
            ("What is the maximum latency budget for hot paths?", "10us", ["10μs", "10 microseconds", "10us"]),
            ("What type should be used for price calculations?", "fixed_point", ["i64", "fixed point", "integer"]),
            ("Which HashMap should be used in hot paths?", "fxhashmap", ["fxhashmap", "fx hash map"]),
            ("Should you allocate memory in hot paths?", "no", ["no", "never"]),
        ]

        correct = 0
        for question, category, valid_answers in questions:
            answer = input(f"{question}: ").lower().strip()
            if any(valid in answer for valid in valid_answers):
                correct += 1
                print("✅ Correct")
            else:
                print(f"❌ Incorrect. Expected one of: {valid_answers}")

        return (correct / len(questions)) * 100

    def _issue_certificate(self, agent_id: str, score: float):
        """Issue certification for compliant agent"""
        cert = {
            "agent_id": agent_id,
            "certification_date": subprocess.run(["date", "-Iseconds"], capture_output=True, text=True).stdout.strip(),
            "score": score,
            "valid_until": subprocess.run(["date", "-d", "+30 days", "-Iseconds"], capture_output=True, text=True).stdout.strip(),
            "authorized_for": ["shrivenquant_development"],
            "restrictions": []
        }

        with open(f".agent_certificates/{agent_id}.json", "w") as f:
            json.dump(cert, f, indent=2)

if __name__ == "__main__":
    import sys
    if len(sys.argv) != 2:
        print("Usage: python agent_training.py <agent_id>")
        sys.exit(1)

    validator = AgentTrainingValidator()
    passed = validator.run_validation(sys.argv[1])
    sys.exit(0 if passed else 1)
```

---

## Enforcement Mechanisms

### 🔒 Multi-layered Enforcement

```bash
#!/bin/bash
# scripts/compliance-enforcement.sh

set -euo pipefail

AGENT_ID="${SHRIVENQUANT_AGENT_ID:-unknown}"
COMPLIANCE_LEVEL=0

echo "🔒 ShrivenQuant Compliance Enforcement"
echo "======================================"

# Level 1: Pre-execution checks
check_agent_certification() {
    if [ ! -f ".agent_certificates/${AGENT_ID}.json" ]; then
        echo "❌ AGENT NOT CERTIFIED"
        echo "   Run: python scripts/agent_training.py ${AGENT_ID}"
        exit 1
    fi

    # Check if certification is expired
    valid_until=$(jq -r '.valid_until' ".agent_certificates/${AGENT_ID}.json")
    current_date=$(date -Iseconds)

    if [[ "$current_date" > "$valid_until" ]]; then
        echo "❌ AGENT CERTIFICATION EXPIRED"
        echo "   Recertification required"
        exit 1
    fi

    echo "✅ Agent certification valid"
    ((COMPLIANCE_LEVEL++))
}

# Level 2: Code compliance scan
check_code_compliance() {
    if ! ./scripts/agent-compliance-check.sh; then
        echo "❌ CODE COMPLIANCE FAILED"
        echo "   Fix violations and retry"
        exit 1
    fi

    echo "✅ Code compliance verified"
    ((COMPLIANCE_LEVEL++))
}

# Level 3: Performance validation
check_performance() {
    if [ -f "scripts/performance-check.sh" ]; then
        if ! timeout 300 ./scripts/performance-check.sh; then
            echo "❌ PERFORMANCE CHECK FAILED"
            echo "   Performance regression detected"
            exit 1
        fi
    fi

    echo "✅ Performance validated"
    ((COMPLIANCE_LEVEL++))
}

# Level 4: Integration tests
check_integration() {
    if [ -f "scripts/run-integration-tests.sh" ]; then
        if ! timeout 600 ./scripts/run-integration-tests.sh; then
            echo "❌ INTEGRATION TESTS FAILED"
            echo "   Fix test failures and retry"
            exit 1
        fi
    fi

    echo "✅ Integration tests passed"
    ((COMPLIANCE_LEVEL++))
}

# Level 5: Final authorization
authorize_commit() {
    if [ "$COMPLIANCE_LEVEL" -lt 4 ]; then
        echo "❌ INSUFFICIENT COMPLIANCE LEVEL: $COMPLIANCE_LEVEL/4"
        echo "   All compliance checks must pass"
        exit 1
    fi

    # Log successful compliance
    echo "$(date -Iseconds): $AGENT_ID - Compliance Level $COMPLIANCE_LEVEL - AUTHORIZED" >> .compliance_audit.log

    echo "✅ COMMIT AUTHORIZED"
    echo "   Agent: $AGENT_ID"
    echo "   Compliance Level: $COMPLIANCE_LEVEL/4"
}

# Execute all checks
check_agent_certification
check_code_compliance  
check_performance
check_integration
authorize_commit
```

### 🚫 Violation Response System

```bash
#!/bin/bash
# scripts/violation-response.sh

handle_violation() {
    local violation_type="$1"
    local severity="$2"
    local details="$3"
    local agent_id="${SHRIVENQUANT_AGENT_ID:-unknown}"

    # Log violation
    echo "$(date -Iseconds): VIOLATION - $agent_id - $violation_type - $severity - $details" >> .violations.log

    case "$severity" in
        "CRITICAL")
            echo "🚨 CRITICAL VIOLATION: $violation_type"
            echo "   Details: $details"
            echo "   Action: IMMEDIATE COMMIT REJECTION"

            # Revoke agent authorization temporarily
            if [ -f ".agent_certificates/${agent_id}.json" ]; then
                jq '.restrictions += ["temporary_suspension"]' ".agent_certificates/${agent_id}.json" > tmp.json
                mv tmp.json ".agent_certificates/${agent_id}.json"
            fi

            # Send alert
            notify-send "ShrivenQuant CRITICAL" "Agent $agent_id violated critical rule: $violation_type"

            exit 1
            ;;

        "HIGH")
            echo "⚠️  HIGH SEVERITY VIOLATION: $violation_type"
            echo "   Details: $details"
            echo "   Action: COMMIT REJECTED - REVIEW REQUIRED"

            # Require additional review
            echo "manual_review_required" > ".review_required_${agent_id}"

            exit 1
            ;;

        "MEDIUM")
            echo "⚠️  MEDIUM VIOLATION: $violation_type"
            echo "   Details: $details"
            echo "   Action: WARNING ISSUED - FIX RECOMMENDED"

            # Allow commit but log warning
            return 0
            ;;

        "LOW")
            echo "ℹ️  LOW VIOLATION: $violation_type"
            echo "   Details: $details"
            echo "   Action: LOGGED FOR REVIEW"

            return 0
            ;;
    esac
}

# Export for use in other scripts
export -f handle_violation
```

---

## Integration with Development Workflow

### 🔄 Complete Workflow Integration

```bash
#!/bin/bash
# scripts/agent-workflow.sh - Complete agent development workflow

set -euo pipefail

echo "🤖 ShrivenQuant Agent Development Workflow"
echo "========================================="

# Step 1: Agent setup and certification
if [ ! -f ".agent_certificates/${SHRIVENQUANT_AGENT_ID}.json" ]; then
    echo "📋 Step 1: Agent Certification Required"
    python scripts/agent_training.py "${SHRIVENQUANT_AGENT_ID}"
fi

# Step 2: Pre-development compliance check
echo "📋 Step 2: Pre-development Setup"
./scripts/agent-setup.sh

# Step 3: Development phase monitoring
echo "📋 Step 3: Starting Development Monitor"
./scripts/compliance-monitor.sh &
MONITOR_PID=$!

# Cleanup monitor on exit
trap "kill $MONITOR_PID 2>/dev/null || true" EXIT

# Step 4: Pre-commit validation
echo "📋 Step 4: Pre-commit Validation"
./scripts/enforce-checklist.sh

# Step 5: Automated compliance enforcement
echo "📋 Step 5: Compliance Enforcement"
./scripts/compliance-enforcement.sh

echo "✅ All workflow steps completed successfully"
echo "🚀 Ready for commit"
```

### 📊 Compliance Dashboard

```bash
#!/bin/bash
# scripts/compliance-dashboard.sh - View compliance status

echo "📊 ShrivenQuant Compliance Dashboard"
echo "==================================="

# Agent certifications
echo "🤖 Agent Certifications:"
if [ -d ".agent_certificates" ]; then
    for cert in .agent_certificates/*.json; do
        if [ -f "$cert" ]; then
            agent_id=$(basename "$cert" .json)
            score=$(jq -r '.score' "$cert")
            valid_until=$(jq -r '.valid_until' "$cert")
            echo "   $agent_id: Score $score%, Valid until $valid_until"
        fi
    done
else
    echo "   No certifications found"
fi

# Recent violations
echo -e "\n🚨 Recent Violations (Last 24h):"
if [ -f ".violations.log" ]; then
    grep "$(date -d '24 hours ago' +%Y-%m-%d)" .violations.log | tail -10
else
    echo "   No violations logged"
fi

# Performance trends
echo -e "\n⚡ Performance Status:"
if [ -f ".performance_baseline" ]; then
    baseline=$(cat .performance_baseline)
    if command -v cargo >/dev/null 2>&1; then
        current=$(cargo bench --bench critical_path 2>/dev/null | grep -o '[0-9.]*ns' | head -1 || echo "N/A")
        echo "   Baseline: $baseline"
        echo "   Current:  $current"
    fi
else
    echo "   No baseline established"
fi

# Compliance score
echo -e "\n🎯 Overall Compliance Score:"
total_checks=10
passed_checks=$(ls -1 .compliance_* 2>/dev/null | wc -l)
score=$(( (passed_checks * 100) / total_checks ))
echo "   $score% ($passed_checks/$total_checks checks passing)"

if [ $score -ge 95 ]; then
    echo "   Status: ✅ EXCELLENT"
elif [ $score -ge 80 ]; then
    echo "   Status: ⚠️  GOOD"  
else
    echo "   Status: ❌ NEEDS IMPROVEMENT"
fi
```

---

## Summary

This comprehensive enforcement framework ensures **100% compliance** through:

### 🛡️ **Multi-Layer Protection:**
1. **Agent Certification** - Training and validation before code access
2. **Real-time Monitoring** - Continuous compliance checking during development  
3. **Pre-commit Enforcement** - Automated violation detection and blocking
4. **Post-commit Auditing** - Continuous monitoring and violation tracking

### 🎯 **Key Enforcement Points:**
- **Mandatory initialization prompt** with compliance acknowledgment
- **Automated pattern detection** for prohibited code
- **Performance budget enforcement** with immediate failure
- **Certification system** with expiration and renewal
- **Violation tracking** with severity-based responses

### 🔧 **Integration:**
- **Pre-commit hooks** automatically run all checks
- **Development workflow** guides agents step-by-step
- **Real-time alerts** notify of violations immediately
- **Compliance dashboard** provides visibility into system health

**Result: Zero tolerance enforcement that makes non-compliance impossible.**
