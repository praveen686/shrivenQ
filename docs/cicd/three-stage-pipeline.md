# Three-Stage CI/CD Pipeline

## Overview

ShrivenQuant uses a three-stage deployment pipeline to ensure code quality, performance, and safety in our trading system.

```
Development → Test/Staging → Production
```

## Stage 1: Development Environment

### Purpose
Rapid development and experimentation with immediate feedback.

### Triggers
- Every push to feature branches
- Pull requests to main

### What Happens
```yaml
1. Code Formatting Check (10s)
   ↓
2. Quick Compilation (30s)
   ↓
3. Unit Tests (1min)
   ↓
4. Clippy Linting (30s)
   ↓
5. Security Scan (20s)
   ↓
6. Performance Check (2min)
```

### Key Features
- **Fast Feedback**: ~5 minute total runtime
- **Fail Fast**: Stops at first failure
- **Parallel Execution**: Tests run concurrently
- **Automatic**: No manual intervention

### Example Output
```
✅ Formatting: Passed
✅ Compilation: Passed
✅ Tests: 49/49 passed
✅ Clippy: No warnings
⚠️  Performance: order_execution degraded 5%
✅ Security: No vulnerabilities
```

## Stage 2: Test/Staging Environment

### Purpose
Integration testing and paper trading validation before production.

### Triggers
- Merge to main branch
- Manual deployment

### What Happens
```yaml
1. Build Release Candidates (5min)
   ↓
2. Integration Tests (10min)
   ↓
3. Performance Benchmarks (15min)
   ↓
4. Risk System Validation (5min)
   ↓
5. Paper Trading Simulation (60min)
   ↓
6. Deploy to Test Servers (5min)
   ↓
7. Smoke Tests (2min)
```

### Key Features
- **Realistic Testing**: Uses production-like data
- **Paper Trading**: Simulates real trades without money
- **Performance Baseline**: Compares against benchmarks
- **Automated Deployment**: Deploys to test servers

### Test Environment Configuration
```env
ENVIRONMENT=test
TRADING_MODE=paper
MAX_POSITION_SIZE=1000
MAX_DAILY_LOSS=100000
RISK_CHECK_ENABLED=true
MONITORING_ENABLED=true
```

### Paper Trading Validation
- Runs for 1 hour with historical data
- Validates order execution
- Checks risk limits
- Measures latencies
- Reports P&L

## Stage 3: Production Environment

### Purpose
Live trading deployment with maximum safety checks.

### Triggers
- Version tags (v1.0.0)
- Manual approval required

### What Happens
```yaml
1. Pre-Production Validation (10min)
   ↓
2. Build Optimized Binaries (10min)
   ↓
3. Performance Validation (20min)
   ↓
4. Manual Approval Gate (waiting...)
   ↓
5. Production Deployment (10min)
   ↓
6. Post-Deployment Monitoring (30min)
   ↓
7. Create GitHub Release (2min)
```

### Key Features
- **Manual Approval**: Requires 2 reviewers
- **Performance Gates**: Must meet latency targets
- **Deployment Window**: Only during market close
- **Automatic Rollback**: On failure detection

### Production Configuration
```env
ENVIRONMENT=production
TRADING_MODE=live
MAX_POSITION_SIZE=10000
MAX_DAILY_LOSS=5000000
RISK_CHECK_ENABLED=true
MONITORING_ENABLED=true
ALERTING_ENABLED=true
CPU_AFFINITY=true
PERFORMANCE_MODE=ultra_low_latency
```

### Safety Checks
1. **Version Format**: Must be semantic (v1.2.3)
2. **Security Audit**: No vulnerable dependencies
3. **License Check**: No GPL dependencies
4. **Performance Thresholds**:
   - Order latency < 100μs
   - Risk check < 50μs
   - Memory constant

## Pipeline Configuration

### Branch Strategy
```
feature/* → dev pipeline → PR to main
    ↓
main → test pipeline → auto-deploy to test
    ↓
tag v*.*.* → prod pipeline → manual deploy to prod
```

### GitHub Environments

#### Development
- No protection rules
- No secrets
- Auto-run on push

#### Test (Staging)
- Auto-deploy from main
- Test API keys only
- No approval needed

#### Production
- Required reviewers: 2
- Wait timer: 30 minutes
- Production secrets
- Manual deployment only

## How It Works in Practice

### Scenario 1: Feature Development
```bash
# You're working on a new feature
git checkout -b feature/faster-risk-checks
# Make changes...
git push

# GitHub automatically:
✅ Runs dev pipeline (5 min)
✅ Shows results on PR
✅ Blocks merge if tests fail
```

### Scenario 2: Deploying to Test
```bash
# Merge PR to main
git checkout main
git merge feature/faster-risk-checks
git push

# GitHub automatically:
✅ Builds release candidate
✅ Runs full test suite
✅ Deploys to test environment
✅ Runs paper trading simulation
✅ Sends Slack notification
```

### Scenario 3: Production Release
```bash
# Create release tag
git tag v1.2.0
git push --tags

# GitHub:
✅ Runs all validations
⏸️ Waits for manual approval
✅ Deploys to production
✅ Monitors for 30 minutes
✅ Creates GitHub release
```

## Performance Gates

### Development
- Tests must pass
- No new warnings

### Test/Staging
- Benchmarks within 10% of baseline
- Paper trading profitable
- Zero errors in 1-hour run

### Production
- Benchmarks within 5% of baseline
- Latency < 100μs p99
- Memory usage constant
- Zero allocations in hot path

## Rollback Procedures

### Automatic Rollback (Test)
Triggered when:
- Tests fail after deployment
- Performance degrades >20%
- Error rate >1%

### Manual Rollback (Production)
```bash
# Quick rollback
./scripts/rollback-production.sh v1.1.9

# Or SSH to server
ssh prod-server
cd /opt/shrivenquant
./bin/stop.sh
cp bin/shriven-quant.backup bin/shriven-quant
./bin/start.sh
```

## Monitoring & Alerts

### What's Monitored
- Order latency (every tick)
- Memory usage (every second)
- Error rate (continuous)
- P&L (real-time)
- Connection status (heartbeat)

### Alert Thresholds
```yaml
Critical:
  - Latency > 200μs
  - Memory growth > 20%
  - Any panic/crash
  - Connection lost

Warning:
  - Latency > 150μs
  - Memory growth > 10%
  - Error rate > 0.1%
  - Risk limit 80% reached
```

## Local Testing

Test the pipeline locally:
```bash
# Test dev environment
./scripts/deploy/local-deploy.sh dev

# Test staging environment
./scripts/deploy/local-deploy.sh test

# Test production build (DO NOT USE FOR REAL TRADING)
./scripts/deploy/local-deploy.sh prod
```

## Security Considerations

### Secrets Management
- Never commit secrets
- Use GitHub Secrets for API keys
- Rotate keys regularly
- Different keys for each environment

### Required Secrets
```yaml
# Test Environment
TEST_ZERODHA_API_KEY
TEST_ZERODHA_API_SECRET
TEST_BINANCE_API_KEY
TEST_BINANCE_API_SECRET

# Production Environment
PROD_ZERODHA_API_KEY
PROD_ZERODHA_API_SECRET
PROD_BINANCE_API_KEY
PROD_BINANCE_API_SECRET
SLACK_WEBHOOK_URL
SSH_DEPLOY_KEY
```

## Compliance & Audit

### What's Logged
- Every deployment (who, when, what)
- Test results
- Performance benchmarks
- Approval records
- Rollback events

### Retention
- Logs: 90 days
- Artifacts: 30 days
- Releases: Forever

## Troubleshooting

### Pipeline Fails at Format Check
```bash
cargo fmt
git add .
git commit -m "Format code"
git push
```

### Pipeline Fails at Clippy
```bash
cargo clippy --fix
git add .
git commit -m "Fix clippy warnings"
git push
```

### Pipeline Fails at Performance
```bash
# Check what regressed
cargo bench --package engine

# Fix the regression
# Update baseline if legitimate
cargo bench --package engine -- --save-baseline new-baseline
```

### Production Deployment Stuck
1. Check GitHub Actions log
2. Verify approvers available
3. Check deployment window (market hours)
4. Manual override if emergency

## Best Practices

1. **Never skip tests** (except emergency hotfix)
2. **Always deploy to test first**
3. **Monitor for 30 minutes after production deploy**
4. **Keep rollback script ready**
5. **Document every production change**
6. **Review test results before approving production**

## Summary

Our three-stage pipeline ensures:
- **Quality**: Nothing broken reaches production
- **Performance**: No degradation goes unnoticed
- **Safety**: Multiple approval gates
- **Compliance**: Full audit trail
- **Speed**: Fast feedback for developers

The pipeline is designed to catch issues early, test thoroughly, and deploy safely - critical for a trading system handling real money.