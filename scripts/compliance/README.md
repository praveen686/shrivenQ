# ShrivenQuant Compliance Tools

This directory contains wrapper scripts for the compliance and remediation tools.

**Note:** The actual compliance tools (`sq-compliance` and `sq-remediator`) are located at `/home/praveen/sq-compliance-tools/` to avoid self-checking issues.

## 🚀 Quick Start

```bash
# Run compliance check
/home/praveen/sq-compliance-tools/sq-compliance/target/release/sq-compliance /home/praveen/ShrivenQuant

# Auto-fix violations (wrapper script)
scripts/compliance/auto-fix.sh

# Run strict compliance (CI mode)
/home/praveen/sq-compliance-tools/sq-compliance/target/release/sq-compliance /home/praveen/ShrivenQuant --strict
```

## Available Tools

### 1. `sq-compliance` - High-Performance Compliance Checker
**Location:** `/home/praveen/sq-compliance-tools/sq-compliance/`

Rust-based parallel compliance checker with 14 comprehensive checks:
- **Performance**: Processes 1000+ files in <100ms
- **Configuration**: Flexible thresholds and allowlists via `compliance.toml`
- **CI-Ready**: JSON output and configurable exit codes
- **Git-Aware**: Respects .gitignore patterns

**Usage:**
```bash
# Basic check
/home/praveen/sq-compliance-tools/sq-compliance/target/release/sq-compliance /home/praveen/ShrivenQuant

# Strict mode (all thresholds = 0)
/home/praveen/sq-compliance-tools/sq-compliance/target/release/sq-compliance /home/praveen/ShrivenQuant --strict

# Show top offenders
/home/praveen/sq-compliance-tools/sq-compliance/target/release/sq-compliance /home/praveen/ShrivenQuant --details
```

### 2. `sq-remediator` - Automatic Violation Fixer
**Location:** `/home/praveen/sq-compliance-tools/sq-remediator/`

Safely fixes common violations with compile-check validation:
- **Auto-Fixes**: Numeric casts, HashMap→FxHashMap, unwrap→?, error handling
- **Safety**: Creates backups, validates compilation, auto-rollback on failure
- **Smart**: Workspace-aware dependency management

**Usage:**
```bash
# Preview changes (dry-run)
/home/praveen/sq-compliance-tools/sq-remediator/target/release/sq-remediator --dry-run /home/praveen/ShrivenQuant

# Apply all fixes
/home/praveen/sq-compliance-tools/sq-remediator/target/release/sq-remediator /home/praveen/ShrivenQuant

# Fix specific rules only
/home/praveen/sq-compliance-tools/sq-remediator/target/release/sq-remediator --rules safe_casts,hashmap_fx /home/praveen/ShrivenQuant
```

### 3. `auto-fix.sh` - Combined Workflow
Wrapper script that runs both checker and remediator:

```bash
# Dry-run mode
./auto-fix.sh --dry-run

# Apply fixes
./auto-fix.sh

# Fix specific rules
./auto-fix.sh --rules safe_casts,hashmap_fx
```

### 4. `validate-risk-limits.sh`
Trading-specific risk validation for:
- Position limits
- Leverage constraints
- Exposure checks
- Risk metrics validation

**Usage:**
```bash
./validate-risk-limits.sh
```

## Compliance Checks

### Critical Violations (Block Commits)
1. **panic_unwrap** - No panic! or .unwrap() in production code
2. **float_money** - No f32/f64 for money calculations
3. **std_hashmap** - Use FxHashMap instead of std::HashMap
4. **numeric_casts** - All numeric casts must be annotated
5. **ignored_errors** - No Err(_) pattern (handle errors properly)
6. **underscore_abuse** - No lazy underscore variables

### Performance Warnings
7. **clone_overuse** - Review clone() calls in hot paths
8. **string_allocs** - Minimize String allocations
9. **magic_numbers** - Use named constants for magic values
10. **inline_attrs** - Control #[inline] usage

### Code Quality
11. **todos** - No TODO/FIXME/HACK markers in production
12. **large_funcs** - Functions should be under 50 lines
13. **warning_suppressions** - Minimize #[allow(...)] attributes
14. **doc_duplication** - Avoid copy-paste documentation

## Auto-Fix Capabilities

| Violation | Auto-Fix | Manual Review | Notes |
|-----------|----------|---------------|-------|
| Numeric casts | ✅ | | Adds SAFETY comments |
| HashMap usage | ✅ | | Converts to FxHashMap |
| Unwrap usage | ✅ | ✅ | Converts to ? where possible |
| Err(_) patterns | ✅ | | Adds logging |
| Float money | | ✅ | Requires semantic understanding |
| Panic usage | | ✅ | Needs error handling design |
| Large functions | | ✅ | Requires refactoring |
| Magic numbers | | ✅ | Needs domain knowledge |

## Configuration

Edit `compliance.toml` in project root:

```toml
# Thresholds
max_unsafe_cast_files = 10
max_inline_attrs = 50
max_todos = 0

# Per-check allowlists
[allowlist.panic_unwrap]
paths = ["scripts/**", "**/tests/**"]

[allowlist.float_money]
paths = ["crates/market-data/feeds/src/apis/**"]

# CI settings
[ci]
min_score = 80
fail_on_critical = true
warn_exit_code = 2
```

## Exit Codes

- `0` - All checks passed
- `1` - Critical/high violations or score below minimum
- `2` - Warnings present (configurable via `warn_exit_code`)

## Reports

Compliance reports are saved to `reports/compliance/` in both:
- Text format: `compliance-report-{commit}-{timestamp}.txt`
- JSON format: `compliance-report-{commit}-{timestamp}.json`

## Building from Source

```bash
# Build compliance checker
cd /home/praveen/sq-compliance-tools/sq-compliance
cargo build --release

# Build remediator
cd /home/praveen/sq-compliance-tools/sq-remediator
cargo build --release
```

## Integration

### Pre-commit Hook
```yaml
- repo: local
  hooks:
    - id: compliance-check
      name: ShrivenQuant Compliance
      entry: /home/praveen/sq-compliance-tools/sq-compliance/target/release/sq-compliance /home/praveen/ShrivenQuant
      language: system
      pass_filenames: false
```

### CI Pipeline
```yaml
- name: Compliance Check
  run: |
    /home/praveen/sq-compliance-tools/sq-compliance/target/release/sq-compliance /home/praveen/ShrivenQuant --strict
```