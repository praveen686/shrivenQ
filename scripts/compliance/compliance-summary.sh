#!/bin/bash
# scripts/compliance-summary.sh - Generate comprehensive compliance report

set -euo pipefail

# Set timeout for long-running commands
TIMEOUT_SECONDS=30

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Get git commit ID for report naming
GIT_COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo "uncommitted")
REPORT_DIR="/home/praveen/ShrivenQuant/reports/compliance"
mkdir -p "$REPORT_DIR"
REPORT_FILE="$REPORT_DIR/compliance-report-${GIT_COMMIT}-$(date +%Y%m%d-%H%M%S).txt"

# Function to output to both terminal and file
output() {
    echo "$@" | tee -a "$REPORT_FILE"
}

output "📊 ShrivenQuant Compliance Summary Report"
output "========================================"
output "Generated: $(date)"
output "Git Commit: $GIT_COMMIT"
output "Agent: ${SHRIVENQUANT_AGENT_ID:-unknown}"

# Summary counters
CRITICAL_VIOLATIONS=0
HIGH_VIOLATIONS=0
MEDIUM_VIOLATIONS=0
LOW_VIOLATIONS=0

output -e "\n${BLUE}🔍 CRITICAL VIOLATIONS (Instant Rejection)${NC}"
output "============================================="

# Hot path allocations
ALLOCATIONS=$(timeout $TIMEOUT_SECONDS find crates/ -name "*.rs" -exec grep -Hn "Vec::new()\|String::new()\|Box::new()\|HashMap::new()" {} \; 2>/dev/null | grep -v test 2>/dev/null | wc -l 2>/dev/null || echo "0")
ALLOCATIONS=${ALLOCATIONS:-0}
if [ "$ALLOCATIONS" -gt 0 ]; then
    output -e "${RED}❌ Hot Path Allocations: $ALLOCATIONS occurrences${NC}"
    ((CRITICAL_VIOLATIONS++))
fi

# Panic/unwrap usage
PANICS=$(timeout $TIMEOUT_SECONDS find crates/ -name "*.rs" -exec grep -Hn "panic!\|unwrap()\|expect(" {} \; 2>/dev/null | grep -v test 2>/dev/null | wc -l 2>/dev/null || echo "0")
PANICS=${PANICS:-0}
if [ "$PANICS" -gt 0 ]; then
    output -e "${RED}❌ Panic/Unwrap Usage: $PANICS occurrences${NC}"
    ((CRITICAL_VIOLATIONS++))
fi

# TODO/FIXME shortcuts
SHORTCUTS=$(find crates/ -name "*.rs" -exec grep -Hn "TODO\|FIXME\|HACK\|XXX" {} \; 2>/dev/null | grep -v test | wc -l 2>/dev/null || echo "0")
if [ "$SHORTCUTS" -gt 0 ]; then
    output -e "${RED}❌ Unresolved Shortcuts: $SHORTCUTS occurrences${NC}"
    ((CRITICAL_VIOLATIONS++))
fi

# Underscore abuse
UNDERSCORE_ABUSE=$(find crates/ -name "*.rs" -exec grep -Hn "let _[a-z].*=" {} \; 2>/dev/null | grep -v test | grep -v "_phantom\|_guard\|_lock" | wc -l 2>/dev/null || echo "0")
if [ "$UNDERSCORE_ABUSE" -gt 0 ]; then
    output -e "${RED}❌ Underscore Variable Abuse: $UNDERSCORE_ABUSE occurrences${NC}"
    ((CRITICAL_VIOLATIONS++))
fi

# Error ignoring
PATTERN_SHORTCUTS=$(find crates/ -name "*.rs" -exec grep -Hn "Err(_)" {} \; 2>/dev/null | grep -v test | wc -l 2>/dev/null || echo "0")
if [ "$PATTERN_SHORTCUTS" -gt 0 ]; then
    output -e "${RED}❌ Ignored Error Patterns: $PATTERN_SHORTCUTS occurrences${NC}"
    ((CRITICAL_VIOLATIONS++))
fi

# Unimplemented without context
UNIMPLEMENTED=$(find crates/ -name "*.rs" -exec grep -Hn "unimplemented!()" {} \; 2>/dev/null | grep -v test | wc -l 2>/dev/null || echo "0")
if [ "$UNIMPLEMENTED" -gt 0 ]; then
    output -e "${RED}❌ Context-less Unimplemented: $UNIMPLEMENTED occurrences${NC}"
    ((CRITICAL_VIOLATIONS++))
fi

if [ "$CRITICAL_VIOLATIONS" -eq 0 ]; then
    output -e "${GREEN}✅ No critical violations found${NC}"
fi

output -e "\n${BLUE}⚠️  HIGH PRIORITY ISSUES${NC}"
output "========================"

# std::HashMap usage
STD_HASHMAP=$(find crates/ -name "*.rs" -exec grep -l "std::collections::HashMap" {} \; 2>/dev/null | grep -v test | wc -l 2>/dev/null || echo "0")
if [ "$STD_HASHMAP" -gt 0 ]; then
    output -e "${YELLOW}⚠️  std::HashMap Usage: $STD_HASHMAP files${NC}"
    ((HIGH_VIOLATIONS++))
fi

# Floating point money
FLOAT_MONEY=$(find crates/ -name "*.rs" -exec grep -l "f32\|f64" {} \; 2>/dev/null | xargs grep -l "price\|money\|amount\|value" 2>/dev/null | grep -v test | wc -l 2>/dev/null || echo "0")
if [ "$FLOAT_MONEY" -gt 0 ]; then
    output -e "${YELLOW}⚠️  Potential Float Money: $FLOAT_MONEY files${NC}"
    ((HIGH_VIOLATIONS++))
fi

if [ "$HIGH_VIOLATIONS" -eq 0 ]; then
    output -e "${GREEN}✅ No high priority issues found${NC}"
fi

output -e "\n${BLUE}📋 MEDIUM PRIORITY ISSUES${NC}"
output "========================="

# Excessive clones
CLONE_SHORTCUTS=$(find crates/ -name "*.rs" -exec grep -Hn "\.clone()" {} \; 2>/dev/null | grep -v test | wc -l 2>/dev/null || echo "0")
if [ "$CLONE_SHORTCUTS" -gt 10 ]; then
    output -e "${YELLOW}⚠️  Excessive Clone Usage: $CLONE_SHORTCUTS occurrences${NC}"
    ((MEDIUM_VIOLATIONS++))
fi

# String allocations
STRING_SHORTCUTS=$(find crates/ -name "*.rs" -exec grep -Hn "to_string()\|format!\|String::from" {} \; 2>/dev/null | grep -v test | wc -l 2>/dev/null || echo "0")
if [ "$STRING_SHORTCUTS" -gt 15 ]; then
    output -e "${YELLOW}⚠️  String Allocations: $STRING_SHORTCUTS occurrences${NC}"
    ((MEDIUM_VIOLATIONS++))
fi

# Default shortcuts
DEFAULT_SHORTCUTS=$(find crates/ -name "*.rs" -exec grep -Hn "Default::default()" {} \; 2>/dev/null | grep -v test | wc -l 2>/dev/null || echo "0")
if [ "$DEFAULT_SHORTCUTS" -gt 15 ]; then
    output -e "${YELLOW}⚠️  Default Shortcuts: $DEFAULT_SHORTCUTS occurrences${NC}"
    ((MEDIUM_VIOLATIONS++))
fi

if [ "$MEDIUM_VIOLATIONS" -eq 0 ]; then
    output -e "${GREEN}✅ No medium priority issues found${NC}"
fi

output -e "\n${BLUE}ℹ️  LOW PRIORITY ISSUES${NC}"
output "======================"

# Magic numbers
MAGIC_NUMBERS=$(find crates/ -name "*.rs" -exec grep -Hn "\b[0-9]\{4,\}\b" {} \; 2>/dev/null | grep -v test | grep -v "const\|static" | wc -l 2>/dev/null || echo "0")
if [ "$MAGIC_NUMBERS" -gt 10 ]; then
    output -e "${YELLOW}ℹ️  Magic Numbers: $MAGIC_NUMBERS occurrences${NC}"
    ((LOW_VIOLATIONS++))
fi

# Warning suppressions
WARNING_SUPPRESSION=$(find crates/ -name "*.rs" -exec grep -Hn "#\[allow(" {} \; 2>/dev/null | grep -v test | wc -l 2>/dev/null || echo "0")
if [ "$WARNING_SUPPRESSION" -gt 15 ]; then
    output -e "${YELLOW}ℹ️  Warning Suppressions: $WARNING_SUPPRESSION occurrences${NC}"
    ((LOW_VIOLATIONS++))
fi

if [ "$LOW_VIOLATIONS" -eq 0 ]; then
    output -e "${GREEN}✅ No low priority issues found${NC}"
fi

# Calculate compliance score
TOTAL_VIOLATIONS=$((CRITICAL_VIOLATIONS + HIGH_VIOLATIONS + MEDIUM_VIOLATIONS + LOW_VIOLATIONS))
MAX_SCORE=100

# Critical violations are heavily weighted
SCORE_DEDUCTION=$((CRITICAL_VIOLATIONS * 25 + HIGH_VIOLATIONS * 10 + MEDIUM_VIOLATIONS * 3 + LOW_VIOLATIONS * 1))
COMPLIANCE_SCORE=$((MAX_SCORE - SCORE_DEDUCTION))
if [ "$COMPLIANCE_SCORE" -lt 0 ]; then
    COMPLIANCE_SCORE=0
fi

output -e "\n${BLUE}📊 OVERALL COMPLIANCE SCORE${NC}"
output "=========================="
output "Critical Violations: $CRITICAL_VIOLATIONS (-25 points each)"
output "High Priority:       $HIGH_VIOLATIONS (-10 points each)"
output "Medium Priority:     $MEDIUM_VIOLATIONS (-3 points each)"
output "Low Priority:        $LOW_VIOLATIONS (-1 point each)"
output ""
output "Score Deduction:     $SCORE_DEDUCTION points"

if [ "$COMPLIANCE_SCORE" -ge 90 ]; then
    output -e "Compliance Score:    ${GREEN}$COMPLIANCE_SCORE/100 - EXCELLENT${NC}"
    STATUS="EXCELLENT"
elif [ "$COMPLIANCE_SCORE" -ge 70 ]; then
    output -e "Compliance Score:    ${YELLOW}$COMPLIANCE_SCORE/100 - GOOD${NC}"
    STATUS="GOOD"
elif [ "$COMPLIANCE_SCORE" -ge 50 ]; then
    output -e "Compliance Score:    ${YELLOW}$COMPLIANCE_SCORE/100 - NEEDS IMPROVEMENT${NC}"
    STATUS="NEEDS_IMPROVEMENT"
else
    output -e "Compliance Score:    ${RED}$COMPLIANCE_SCORE/100 - CRITICAL${NC}"
    STATUS="CRITICAL"
fi

output -e "\n${BLUE}🎯 RECOMMENDATIONS${NC}"
output "=================="

if [ "$CRITICAL_VIOLATIONS" -gt 0 ]; then
    output -e "${RED}🚨 IMMEDIATE ACTION REQUIRED:${NC}"
    output "   - Fix all critical violations before proceeding"
    output "   - Review: docs/developer-guide/QUANTITATIVE_DEVELOPMENT_BEST_PRACTICES.md"
    output "   - Run: ./scripts/agent-compliance-check.sh for detailed locations"
fi

if [ "$HIGH_VIOLATIONS" -gt 0 ]; then
    output -e "${YELLOW}⚡ HIGH PRIORITY:${NC}"
    output "   - Replace std::HashMap with FxHashMap in hot paths"
    output "   - Convert floating point money calculations to fixed-point"
fi

if [ "$MEDIUM_VIOLATIONS" -gt 0 ]; then
    output -e "${YELLOW}📋 MEDIUM PRIORITY:${NC}"
    output "   - Reduce clone() usage with better borrowing"
    output "   - Minimize string allocations in performance-critical code"
    output "   - Use explicit initialization over Default::default()"
fi

if [ "$LOW_VIOLATIONS" -gt 0 ]; then
    output -e "${BLUE}ℹ️  CLEANUP:${NC}"
    output "   - Replace magic numbers with named constants"
    output "   - Fix warnings instead of suppressing them"
fi

# Save compliance report JSON
JSON_REPORT_FILE="$REPORT_DIR/compliance-report-${GIT_COMMIT}-$(date +%Y%m%d-%H%M%S).json"
cat > "$JSON_REPORT_FILE" << EOF
{
  "timestamp": "$(date -Iseconds)",
  "agent_id": "${SHRIVENQUANT_AGENT_ID:-unknown}",
  "violations": {
    "critical": $CRITICAL_VIOLATIONS,
    "high": $HIGH_VIOLATIONS,
    "medium": $MEDIUM_VIOLATIONS,
    "low": $LOW_VIOLATIONS,
    "total": $TOTAL_VIOLATIONS
  },
  "compliance_score": $COMPLIANCE_SCORE,
  "status": "$STATUS",
  "commit_authorized": $([ "$CRITICAL_VIOLATIONS" -eq 0 ] && echo "true" || echo "false")
}
EOF

output -e "\n${BLUE}📄 REPORTS SAVED:${NC}"
output "  Text Report: $REPORT_FILE"
output "  JSON Report: $JSON_REPORT_FILE"

# Final determination
output -e "\n${BLUE}======================================${NC}"
if [ "$CRITICAL_VIOLATIONS" -eq 0 ]; then
    output -e "${GREEN}✅ COMMIT AUTHORIZED${NC}"
    output -e "${GREEN}   No critical violations detected${NC}"
    exit 0
else
    output -e "${RED}❌ COMMIT REJECTED${NC}"
    output -e "${RED}   Fix $CRITICAL_VIOLATIONS critical violations first${NC}"
    exit 1
fi
