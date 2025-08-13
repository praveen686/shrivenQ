# Pre-commit Hooks & CI/CD Integration

## Overview

ShrivenQuant uses a **dual-layer protection** system:
1. **Local pre-commit hooks** - Catch issues before commit
2. **CI/CD pipeline** - Verify everything on GitHub

## How They Work Together

```
Developer Machine              GitHub Actions
     LOCAL                          CLOUD
       ↓                              ↓
[Pre-commit Hook]  ─── git push ──→ [CI/CD Pipeline]
       ↓                              ↓
  Quick checks                   Full validation
   (2-3 sec)                      (5-10 min)
       ↓                              ↓
  Block commit     ←─────────────  Block merge
  if failed                        if failed
```

## What Runs Where

### Pre-commit Hook (Local - 2-3 seconds)
```bash
✓ Comprehensive compliance check (14 checks)
✓ Code formatting
✓ Clippy warnings
✓ Test compilation
✓ Performance baseline check
✓ Risk limits validation
```

### CI/CD Pipeline (GitHub - 5-10 minutes)
```bash
✓ Everything from pre-commit PLUS:
✓ Full test suite
✓ Integration tests
✓ Security audit
✓ License check
✓ Benchmark suite
✓ Memory profiling
✓ Paper trading simulation
```

## The Complete Flow

### 1. Developer Makes Changes
```bash
# Edit code
vim crates/trading/engine/src/core.rs

# Try to commit
git add .
git commit -m "Optimize order execution"
```

### 2. Pre-commit Hook Runs (Local)
```
═══════════════════════════════════════
   ShrivenQuant Compliance Check      
═══════════════════════════════════════

🧭 Unified Compliance Check (Rust)
Running 14 checks in parallel...

✓ Release build successful
✓ Cast usage acceptable (10 files)
✓ No panic/unwrap/expect in production
✓ No std::HashMap usage in prod paths
✓ No floating point money in internal calculations
✓ No outstanding TODO/FIXME markers
✓ No ignored error patterns in prod
✓ Clone usage reasonable (50 calls)
✓ String allocations reasonable (100 sites)
✓ Underscore usage acceptable
✓ Magic numbers acceptable (10 found)
✓ Warning suppressions acceptable (15 found)
✓ Doc duplication acceptable (max repeat 2×)
✓ Function lengths look reasonable
✓ Inline attribute usage reasonable (50 uses)

📊 COMPLIANCE SCORE
Critical: 0  High: 0  Medium: 0  Low: 0
Score: 100/100  Status: EXCELLENT

✅ COMMIT AUTHORIZED
```

### 3. Developer Pushes to GitHub
```bash
git push origin feature/faster-orders
```

### 4. CI/CD Pipeline Runs (GitHub)
```yaml
GitHub Actions: dev-pipeline.yml triggered

Stage 1: Quick Validation ✓
  - Agent compliance check (rerun)
  - AI bypass detection (rerun)
  - TODO/FIXME check (rerun)
  - Format check (rerun)
  - Quick compile

Stage 2: Full Testing ✓
  - Unit tests
  - Integration tests
  - Doc tests
  - Clippy analysis
  - Unsafe code check
  - Numeric cast check
  - Panic detection

Stage 3: Performance ✓
  - Run benchmarks
  - Compare baseline
  - Check allocations
  - Memory profiling

Stage 4: Security ✓
  - Cargo audit
  - Secret scanning
  - License check

✅ All checks passed - Ready to merge!
```

## Why Both?

### Pre-commit Hooks (Local)
**Pros:**
- Instant feedback (2-3 seconds)
- Saves time - catch issues before push
- No waiting for CI/CD

**Cons:**
- Can be bypassed (`git commit --no-verify`)
- Only runs on developer's machine
- Might work differently on different OS

### CI/CD Pipeline (GitHub)
**Pros:**
- Cannot be bypassed
- Runs on clean environment
- Same for everyone
- More comprehensive tests

**Cons:**
- Takes longer (5-10 minutes)
- Requires push to trigger
- Uses GitHub Actions minutes

## Configuration Files

### Pre-commit Hook Location
```
.git/hooks/pre-commit          # The actual hook
scripts/compliance/*.sh         # Scripts it calls
scripts/performance/*.sh        # Performance checks
```

### CI/CD Pipeline Location
```
.github/workflows/dev-pipeline.yml    # Dev environment
.github/workflows/test-pipeline.yml   # Test/staging
.github/workflows/prod-pipeline.yml   # Production
```

## What If Someone Bypasses Local Hooks?

They can bypass locally:
```bash
git commit --no-verify -m "Skip checks"  # BAD!
git push
```

But CI/CD will catch them:
```
❌ GitHub Actions: dev-pipeline.yml FAILED

Stage 1: Quick Validation ✗
  - Agent compliance check FAILED
  - TODO markers found in code
  - Format check FAILED

PR cannot be merged until fixed!
```

## Synchronization

The CI/CD pipeline includes ALL pre-commit checks:

| Check | Pre-commit | CI/CD Dev | CI/CD Test | CI/CD Prod |
|-------|------------|-----------|------------|------------|
| Agent compliance | ✓ | ✓ | ✓ | ✓ |
| AI bypass detection | ✓ | ✓ | ✓ | ✓ |
| TODO/FIXME | ✓ | ✓ | ✓ | ✓ |
| Formatting | ✓ | ✓ | ✓ | ✓ |
| Clippy | ✓ | ✓ | ✓ | ✓ |
| Tests | Quick | Full | Full | Full |
| Benchmarks | Baseline | Run | Compare | Validate |
| Integration tests | ✗ | ✗ | ✓ | ✓ |
| Paper trading | ✗ | ✗ | ✓ | ✓ |
| Security audit | ✗ | ✓ | ✓ | ✓ |

## Adding New Checks

To add a new check that runs both locally and in CI/CD:

### 1. Add to Pre-commit Hook
```bash
# Edit .git/hooks/pre-commit
echo "Running new check..."
./scripts/compliance/new-check.sh || FAILED=1
```

### 2. Add to CI/CD Pipeline
```yaml
# Edit .github/workflows/dev-pipeline.yml
- name: New Compliance Check
  run: |
    echo "Running new check..."
    ./scripts/compliance/new-check.sh
```

### 3. Keep Them Synchronized
Both should run the SAME script to ensure consistency.

## Performance Considerations

### Pre-commit (Must be FAST)
- Target: <3 seconds total
- Only essential checks
- Skip heavy benchmarks
- Quick test subset

### CI/CD (Can be thorough)
- Target: <10 minutes for dev
- Full test suite
- Complete benchmarks
- Integration tests

## Troubleshooting

### Pre-commit Not Running
```bash
# Make sure hook is executable
chmod +x .git/hooks/pre-commit

# Check if bypassed
git config --get core.hooksPath  # Should be empty or .git/hooks
```

### CI/CD Not Matching Pre-commit
```bash
# Ensure scripts are executable in CI
chmod +x scripts/compliance/*.sh
chmod +x scripts/performance/*.sh

# Commit the permission changes
git add scripts/
git commit -m "Fix script permissions"
```

### Different Results Local vs CI
Common causes:
1. **Different Rust versions** - Use `rust-toolchain.toml`
2. **Missing dependencies** - Document in README
3. **OS differences** - Test on Linux (GitHub uses Ubuntu)
4. **Environment variables** - Set same in both

## Best Practices

1. **Never bypass pre-commit** unless emergency
2. **Keep checks fast locally** (<3 seconds)
3. **Run same scripts** in both places
4. **Fix locally first** - don't push broken code
5. **Monitor CI/CD time** - keep under 10 minutes

## Summary

- **Pre-commit hooks** = First line of defense (local, fast)
- **CI/CD pipeline** = Final verification (cloud, thorough)
- **Both run same checks** = Consistency
- **Cannot bypass CI/CD** = Guaranteed quality

This dual-layer approach ensures:
- Fast feedback for developers
- Guaranteed quality for repository
- No broken code in main branch
- Consistent standards for all contributors