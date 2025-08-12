# 🚀 ShrivenQuant Scripts & Automation

## Overview

This directory contains all automation scripts for the ShrivenQuant ultra-low latency trading platform. Scripts are organized into logical categories for easy discovery and maintenance.

## 📁 Directory Structure

```
scripts/
├── build/           # Build automation and compilation
├── compliance/      # Code quality and compliance checking
├── deployment/      # Deployment and release automation
├── development/     # Development workflow tools
├── performance/     # Performance testing and optimization
├── testing/         # Test execution and coverage
└── README.md        # This file
```

## 🏗️ Build Scripts (`build/`)

### `orchestrator.sh`
Main build pipeline orchestrator with dependency management.

**Usage:**
```bash
./scripts/build/orchestrator.sh [quick|release|docker|cross|all]
```

**Options:**
- `quick` - Fast build with tests (pre-checks, compile, test)
- `release` - Production build with optimizations
- `docker` - Build Docker containers
- `cross` - Cross-platform compilation
- `all` - Complete pipeline with all stages

**Features:**
- 10-stage dependency-managed pipeline
- Parallel execution with progress tracking
- Comprehensive build logs in `build-logs/`
- Automatic artifact generation

### `build.rs`
Rust build automation with performance optimizations.

**Note:** Requires `cargo-script` or `rust-script` to run directly.

**Usage:**
```bash
cargo build --release  # Use cargo directly for now
```

**Build Profiles:**
- `release` - Maximum optimizations (LTO, native CPU)
- `debug` - Fast compilation with debug symbols
- `bench` - Benchmark configuration
- `check` - Quick validation

### `cross-compile.sh`
Multi-platform binary generation.

**Usage:**
```bash
./scripts/build/cross-compile.sh
```

**Supported Targets:**
- Linux x64 (glibc and musl)
- Windows x64
- macOS x64
- Linux ARM64

**Output:** Binaries in `releases/` directory with checksums.

### `docker-build.sh`
Multi-stage Docker image builder.

**Usage:**
```bash
./scripts/build/docker-build.sh
```

**Image Variants:**
- `runtime` - Minimal production image (~20MB)
- `development` - Full dev environment with tools
- `testing` - Test runner with coverage tools
- `benchmark` - Performance profiling environment

## ✅ Compliance Scripts (`compliance/`)

### `strict-check.sh`
Primary compliance validation - **MUST PASS** before commits.

**Usage:**
```bash
./scripts/compliance/strict-check.sh
```

**Checks:**
- Forbidden patterns (TODO, FIXME, panic!, unwrap())
- Clippy with strict settings
- Dead code detection
- Code formatting
- Documentation completeness
- Test execution

### `agent-compliance-check.sh`
AI agent compliance validation for automated code generation.

**Usage:**
```bash
./scripts/compliance/agent-compliance-check.sh
```

**Validates:**
- Hot path allocations
- Floating-point money calculations
- Error handling patterns
- Agent anti-patterns

### `compliance-summary.sh`
Generates detailed compliance report with scoring.

**Usage:**
```bash
./scripts/compliance/compliance-summary.sh
```

**Output:**
- Critical violations count
- Performance impact assessment
- Compliance score (0-100)
- Detailed violation locations

### `initialize-agent.sh`
Initializes new AI agents with compliance framework.

**Usage:**
```bash
./scripts/compliance/initialize-agent.sh <agent_id>
```

### `validate-risk-limits.sh`
Validates risk management configurations.

**Usage:**
```bash
./scripts/compliance/validate-risk-limits.sh
```

## 🧪 Testing Scripts (`testing/`)

### `run-integration-tests.sh`
Executes full integration test suite.

**Usage:**
```bash
./scripts/testing/run-integration-tests.sh
```

### `check-test-coverage.sh`
Analyzes test coverage with detailed reports.

**Usage:**
```bash
./scripts/testing/check-test-coverage.sh
```

**Output:** HTML coverage report in `target/coverage/`

### `system-validation.sh`
End-to-end system validation.

**Usage:**
```bash
./scripts/testing/system-validation.sh
```

## ⚡ Performance Scripts (`performance/`)

### `performance-check.sh`
Runs performance benchmarks and regression detection.

**Usage:**
```bash
./scripts/performance/performance-check.sh
```

**Metrics:**
- Latency percentiles (p50, p99, p99.9)
- Throughput measurements
- Memory allocations
- CPU usage patterns

### `check-hot-path-allocations.sh`
Detects allocations in latency-critical paths.

**Usage:**
```bash
./scripts/performance/check-hot-path-allocations.sh
```

### `update-benchmarks.sh`
Updates baseline benchmark results.

**Usage:**
```bash
./scripts/performance/update-benchmarks.sh
```

## 🔧 Development Scripts (`development/`)

### `install-precommit.sh`
Sets up git pre-commit hooks.

**Usage:**
```bash
./scripts/development/install-precommit.sh
```

### `validate-configs.sh`
Validates all configuration files.

**Usage:**
```bash
./scripts/development/validate-configs.sh
```

## 📦 Deployment Scripts (`deployment/`)

### `api-compatibility-check.sh`
Ensures backward API compatibility.

**Usage:**
```bash
./scripts/deployment/api-compatibility-check.sh
```

## 🎯 Quick Start

### For New Contributors
```bash
# 1. Install pre-commit hooks
./scripts/development/install-precommit.sh

# 2. Run compliance check
./scripts/compliance/strict-check.sh

# 3. Run quick build
./scripts/build/orchestrator.sh quick
```

### Before Committing
```bash
# Run full compliance validation
./scripts/compliance/strict-check.sh

# If working with AI agents
./scripts/compliance/agent-compliance-check.sh
```

### For Release
```bash
# Full release pipeline
./scripts/build/orchestrator.sh release

# Cross-platform builds
./scripts/build/cross-compile.sh

# Docker images
./scripts/build/docker-build.sh
```

## ⚠️ Important Notes

1. **All scripts must pass `strict-check.sh` before commits**
2. **Build scripts require proper permissions** - run `chmod +x scripts/**/*.sh` if needed
3. **Docker builds require Docker daemon running**
4. **Cross-compilation may require additional toolchains**

## 🐛 Troubleshooting

### Build Scripts Not Running

1. **Permission Issues**
   ```bash
   chmod +x scripts/**/*.sh
   ```

2. **Missing Dependencies**
   - `build.rs` requires `cargo-script`: `cargo install cargo-script`
   - Docker builds need Docker installed
   - Cross-compilation needs target toolchains

3. **Path Issues**
   - Always run scripts from project root
   - Use relative paths: `./scripts/build/orchestrator.sh`

### Compliance Failures

1. **Check specific violations**
   ```bash
   ./scripts/compliance/compliance-summary.sh
   ```

2. **Auto-fix formatting**
   ```bash
   cargo fmt --all
   ```

3. **Fix clippy warnings**
   ```bash
   cargo clippy --fix --all-targets
   ```

## 📊 Performance Requirements

All scripts must meet these performance criteria:

- **Compliance checks**: < 30 seconds
- **Quick build**: < 2 minutes  
- **Full build**: < 5 minutes
- **Test suite**: < 3 minutes
- **Docker build**: < 10 minutes

## 🤝 Contributing

When adding new scripts:

1. Place in appropriate category directory
2. Add documentation to this README
3. Include usage examples
4. Add to pre-commit hooks if applicable
5. Ensure script passes compliance checks

## 📝 License

All scripts are part of the ShrivenQuant proprietary codebase.
