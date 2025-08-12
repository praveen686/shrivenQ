#!/bin/bash
# ShrivenQuant Build Orchestrator
# Coordinates all build processes with dependency management

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
NC='\033[0m'

echo -e "${PURPLE}🎼 ShrivenQuant Build Orchestrator${NC}"
echo "==================================="

# Configuration
BUILD_TYPE="${1:-all}"
PARALLEL_JOBS="${PARALLEL_JOBS:-$(nproc)}"
BUILD_LOG_DIR="build-logs"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")

# Create build log directory
mkdir -p "$BUILD_LOG_DIR"

# Build stages configuration
declare -A BUILD_STAGES=(
    ["pre-checks"]="Pre-build validation and setup"
    ["dependencies"]="Dependency analysis and updates"
    ["compile"]="Source code compilation"
    ["test"]="Comprehensive testing suite"
    ["benchmark"]="Performance benchmarking"
    ["package"]="Binary packaging and optimization"
    ["docker"]="Container image building"
    ["cross-compile"]="Multi-platform compilation"
    ["artifacts"]="Build artifact generation"
    ["deploy-prep"]="Deployment preparation"
)

# Build stage dependencies
declare -A STAGE_DEPS=(
    ["dependencies"]="pre-checks"
    ["compile"]="dependencies"
    ["test"]="compile"
    ["benchmark"]="compile"
    ["package"]="test benchmark"
    ["docker"]="package"
    ["cross-compile"]="package"
    ["artifacts"]="package docker cross-compile"
    ["deploy-prep"]="artifacts"
)

# Track completed stages
declare -A COMPLETED_STAGES=()
declare -A FAILED_STAGES=()

log_stage() {
    local stage="$1"
    local status="$2"
    local message="$3"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')

    echo "[$timestamp] [$stage] [$status] $message" >> "$BUILD_LOG_DIR/orchestrator_${TIMESTAMP}.log"

    case "$status" in
        "START") echo -e "${BLUE}🔄 Starting: $message${NC}" ;;
        "SUCCESS") echo -e "${GREEN}✅ Completed: $message${NC}" ;;
        "FAILURE") echo -e "${RED}❌ Failed: $message${NC}" ;;
        "SKIP") echo -e "${YELLOW}⏭️  Skipped: $message${NC}" ;;
    esac
}

check_stage_dependencies() {
    local stage="$1"
    local deps="${STAGE_DEPS[$stage]:-}"

    if [ -n "$deps" ]; then
        for dep in $deps; do
            if [ -z "${COMPLETED_STAGES[$dep]:-}" ]; then
                return 1
            fi
        done
    fi

    return 0
}

run_pre_checks() {
    log_stage "pre-checks" "START" "Running pre-build validation"

    local log_file="$BUILD_LOG_DIR/pre-checks_${TIMESTAMP}.log"

    {
        # Check Rust toolchain
        echo "Checking Rust toolchain..."
        rustc --version
        cargo --version

        # Check system dependencies
        echo "Checking system dependencies..."
        command -v git >/dev/null || { echo "git not found"; exit 1; }
        command -v pkg-config >/dev/null || { echo "pkg-config not found"; exit 1; }

        # Check project structure
        echo "Validating project structure..."
        [ -f "Cargo.toml" ] || { echo "Cargo.toml not found"; exit 1; }
        [ -d "crates" ] || { echo "crates directory not found"; exit 1; }

        # Run compliance check
        echo "Running compliance validation..."
        if [ -f "scripts/compliance/strict-check.sh" ]; then
            ./scripts/compliance/strict-check.sh
        fi

        echo "Pre-checks completed successfully"

    } > "$log_file" 2>&1

    if [ $? -eq 0 ]; then
        log_stage "pre-checks" "SUCCESS" "Pre-build validation completed"
        COMPLETED_STAGES["pre-checks"]=1
        return 0
    else
        log_stage "pre-checks" "FAILURE" "Pre-build validation failed"
        FAILED_STAGES["pre-checks"]=1
        return 1
    fi
}

run_dependencies() {
    log_stage "dependencies" "START" "Analyzing and updating dependencies"

    local log_file="$BUILD_LOG_DIR/dependencies_${TIMESTAMP}.log"

    {
        echo "Fetching dependencies..."
        cargo fetch

        echo "Checking for outdated dependencies..."
        if command -v cargo-outdated >/dev/null 2>&1; then
            cargo outdated
        fi

        echo "Running security audit..."
        if command -v cargo-audit >/dev/null 2>&1; then
            cargo audit
        fi

        echo "Dependency analysis completed"

    } > "$log_file" 2>&1

    if [ $? -eq 0 ]; then
        log_stage "dependencies" "SUCCESS" "Dependency analysis completed"
        COMPLETED_STAGES["dependencies"]=1
        return 0
    else
        log_stage "dependencies" "FAILURE" "Dependency analysis failed"
        FAILED_STAGES["dependencies"]=1
        return 1
    fi
}

run_compile() {
    log_stage "compile" "START" "Compiling source code"

    local log_file="$BUILD_LOG_DIR/compile_${TIMESTAMP}.log"

    {
        echo "Compiling workspace..."
        export RUSTFLAGS="-C opt-level=3 -C target-cpu=native"
        cargo build --release --workspace -j "$PARALLEL_JOBS"

        echo "Running clippy analysis..."
        cargo clippy --all-targets --all-features -- -D warnings

        echo "Checking code formatting..."
        cargo fmt --all -- --check

        echo "Compilation completed successfully"

    } > "$log_file" 2>&1

    if [ $? -eq 0 ]; then
        log_stage "compile" "SUCCESS" "Source compilation completed"
        COMPLETED_STAGES["compile"]=1
        return 0
    else
        log_stage "compile" "FAILURE" "Source compilation failed"
        FAILED_STAGES["compile"]=1
        return 1
    fi
}

run_test() {
    log_stage "test" "START" "Running comprehensive test suite"

    local log_file="$BUILD_LOG_DIR/test_${TIMESTAMP}.log"

    {
        echo "Running unit tests..."
        cargo test --workspace --lib -j "$PARALLEL_JOBS"

        echo "Running integration tests..."
        if [ -f "scripts/testing/run-integration-tests.sh" ]; then
            ./scripts/testing/run-integration-tests.sh
        fi

        echo "Checking test coverage..."
        if [ -f "scripts/testing/check-test-coverage.sh" ]; then
            ./scripts/testing/check-test-coverage.sh
        fi

        echo "Test suite completed successfully"

    } > "$log_file" 2>&1

    if [ $? -eq 0 ]; then
        log_stage "test" "SUCCESS" "Test suite completed"
        COMPLETED_STAGES["test"]=1
        return 0
    else
        log_stage "test" "FAILURE" "Test suite failed"
        FAILED_STAGES["test"]=1
        return 1
    fi
}

run_benchmark() {
    log_stage "benchmark" "START" "Running performance benchmarks"

    local log_file="$BUILD_LOG_DIR/benchmark_${TIMESTAMP}.log"

    {
        echo "Running performance benchmarks..."
        if [ -f "scripts/performance/performance-check.sh" ]; then
            ./scripts/performance/performance-check.sh
        fi

        echo "Updating benchmark baselines..."
        if [ -f "scripts/performance/update-benchmarks.sh" ]; then
            ./scripts/performance/update-benchmarks.sh
        fi

        echo "Benchmark suite completed successfully"

    } > "$log_file" 2>&1

    if [ $? -eq 0 ]; then
        log_stage "benchmark" "SUCCESS" "Performance benchmarks completed"
        COMPLETED_STAGES["benchmark"]=1
        return 0
    else
        log_stage "benchmark" "FAILURE" "Performance benchmarks failed"
        FAILED_STAGES["benchmark"]=1
        return 1
    fi
}

run_package() {
    log_stage "package" "START" "Creating optimized binary packages"

    local log_file="$BUILD_LOG_DIR/package_${TIMESTAMP}.log"

    {
        echo "Optimizing release binaries..."

        # Strip debug symbols
        if command -v strip >/dev/null 2>&1; then
            find target/release -maxdepth 1 -type f -executable -exec strip {} \;
        fi

        # Create distribution packages
        mkdir -p dist

        # Package main binary
        tar -czf "dist/shrivenquant-${TIMESTAMP}-linux-x64.tar.gz" \
            -C target/release cli README.md LICENSE

        echo "Binary packaging completed successfully"

    } > "$log_file" 2>&1

    if [ $? -eq 0 ]; then
        log_stage "package" "SUCCESS" "Binary packaging completed"
        COMPLETED_STAGES["package"]=1
        return 0
    else
        log_stage "package" "FAILURE" "Binary packaging failed"
        FAILED_STAGES["package"]=1
        return 1
    fi
}

run_docker() {
    log_stage "docker" "START" "Building container images"

    local log_file="$BUILD_LOG_DIR/docker_${TIMESTAMP}.log"

    {
        if [ -f "scripts/build/docker-build.sh" ]; then
            ./scripts/build/docker-build.sh
        else
            echo "Docker build script not found, skipping"
        fi

    } > "$log_file" 2>&1

    if [ $? -eq 0 ]; then
        log_stage "docker" "SUCCESS" "Container image building completed"
        COMPLETED_STAGES["docker"]=1
        return 0
    else
        log_stage "docker" "FAILURE" "Container image building failed"
        FAILED_STAGES["docker"]=1
        return 1
    fi
}

run_cross_compile() {
    log_stage "cross-compile" "START" "Cross-platform compilation"

    local log_file="$BUILD_LOG_DIR/cross-compile_${TIMESTAMP}.log"

    {
        if [ -f "scripts/build/cross-compile.sh" ]; then
            ./scripts/build/cross-compile.sh
        else
            echo "Cross-compile script not found, skipping"
        fi

    } > "$log_file" 2>&1

    if [ $? -eq 0 ]; then
        log_stage "cross-compile" "SUCCESS" "Cross-platform compilation completed"
        COMPLETED_STAGES["cross-compile"]=1
        return 0
    else
        log_stage "cross-compile" "FAILURE" "Cross-platform compilation failed"
        FAILED_STAGES["cross-compile"]=1
        return 1
    fi
}

run_artifacts() {
    log_stage "artifacts" "START" "Generating build artifacts"

    local log_file="$BUILD_LOG_DIR/artifacts_${TIMESTAMP}.log"

    {
        echo "Collecting build artifacts..."

        # Create artifacts directory
        mkdir -p artifacts

        # Collect binaries
        cp -r target/release/cli artifacts/ 2>/dev/null || true
        cp -r releases/* artifacts/ 2>/dev/null || true
        cp -r dist/* artifacts/ 2>/dev/null || true

        # Generate checksums
        cd artifacts
        find . -type f -exec sha256sum {} \; > checksums.sha256
        cd - >/dev/null

        # Generate build report
        cat > artifacts/build-report.json <<EOF
{
    "timestamp": "${TIMESTAMP}",
    "git_hash": "$(git rev-parse HEAD 2>/dev/null || echo 'unknown')",
    "rust_version": "$(rustc --version)",
    "build_type": "${BUILD_TYPE}",
    "completed_stages": $(printf '%s\n' "${!COMPLETED_STAGES[@]}" | jq -R . | jq -s .),
    "failed_stages": $(printf '%s\n' "${!FAILED_STAGES[@]}" | jq -R . | jq -s .)
}
EOF

        echo "Build artifacts generated successfully"

    } > "$log_file" 2>&1

    if [ $? -eq 0 ]; then
        log_stage "artifacts" "SUCCESS" "Build artifact generation completed"
        COMPLETED_STAGES["artifacts"]=1
        return 0
    else
        log_stage "artifacts" "FAILURE" "Build artifact generation failed"
        FAILED_STAGES["artifacts"]=1
        return 1
    fi
}

run_deploy_prep() {
    log_stage "deploy-prep" "START" "Preparing deployment artifacts"

    local log_file="$BUILD_LOG_DIR/deploy-prep_${TIMESTAMP}.log"

    {
        echo "Preparing deployment packages..."

        # Create deployment directory
        mkdir -p deployment

        # Copy deployment-ready artifacts
        cp -r artifacts/* deployment/

        # Generate deployment manifest
        cat > deployment/deployment-manifest.yaml <<EOF
apiVersion: v1
kind: ConfigMap
metadata:
  name: shrivenquant-build-info
data:
  build_timestamp: "${TIMESTAMP}"
  git_hash: "$(git rev-parse HEAD 2>/dev/null || echo 'unknown')"
  rust_version: "$(rustc --version)"
  build_stages_completed: "$(echo "${!COMPLETED_STAGES[@]}" | tr ' ' ',')"
EOF

        echo "Deployment preparation completed successfully"

    } > "$log_file" 2>&1

    if [ $? -eq 0 ]; then
        log_stage "deploy-prep" "SUCCESS" "Deployment preparation completed"
        COMPLETED_STAGES["deploy-prep"]=1
        return 0
    else
        log_stage "deploy-prep" "FAILURE" "Deployment preparation failed"
        FAILED_STAGES["deploy-prep"]=1
        return 1
    fi
}

# Stage execution functions
declare -A STAGE_FUNCTIONS=(
    ["pre-checks"]="run_pre_checks"
    ["dependencies"]="run_dependencies"
    ["compile"]="run_compile"
    ["test"]="run_test"
    ["benchmark"]="run_benchmark"
    ["package"]="run_package"
    ["docker"]="run_docker"
    ["cross-compile"]="run_cross_compile"
    ["artifacts"]="run_artifacts"
    ["deploy-prep"]="run_deploy_prep"
)

# Determine which stages to run
case "$BUILD_TYPE" in
    "quick")
        STAGES_TO_RUN=("pre-checks" "dependencies" "compile" "test")
        ;;
    "release")
        STAGES_TO_RUN=("pre-checks" "dependencies" "compile" "test" "benchmark" "package" "artifacts")
        ;;
    "docker")
        STAGES_TO_RUN=("pre-checks" "dependencies" "compile" "test" "package" "docker")
        ;;
    "cross")
        STAGES_TO_RUN=("pre-checks" "dependencies" "compile" "test" "package" "cross-compile")
        ;;
    "all"|*)
        STAGES_TO_RUN=("pre-checks" "dependencies" "compile" "test" "benchmark" "package" "docker" "cross-compile" "artifacts" "deploy-prep")
        ;;
esac

# Execute build pipeline
echo -e "\n${BLUE}🚀 Starting build pipeline: ${BUILD_TYPE}${NC}"
echo "Stages to execute: ${STAGES_TO_RUN[*]}"
echo "Parallel jobs: $PARALLEL_JOBS"
echo "Build logs: $BUILD_LOG_DIR/"
echo ""

START_TIME=$(date +%s)

# Execute stages in order
for stage in "${STAGES_TO_RUN[@]}"; do
    if check_stage_dependencies "$stage"; then
        stage_func="${STAGE_FUNCTIONS[$stage]}"
        $stage_func
    else
        log_stage "$stage" "SKIP" "Dependencies not met, skipping stage"
    fi
done

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

# Build summary
echo -e "\n${PURPLE}📊 Build Summary${NC}"
echo "=================="
echo -e "Build type: ${BLUE}$BUILD_TYPE${NC}"
echo -e "Duration: ${BLUE}${DURATION}s${NC}"
echo -e "Completed stages: ${GREEN}$(echo "${!COMPLETED_STAGES[@]}" | wc -w)${NC}"
echo -e "Failed stages: ${RED}$(echo "${!FAILED_STAGES[@]}" | wc -w)${NC}"

if [ ${#FAILED_STAGES[@]} -eq 0 ]; then
    echo -e "\n${GREEN}🎉 Build pipeline completed successfully!${NC}"
    exit 0
else
    echo -e "\n${RED}❌ Build pipeline failed. Check logs in $BUILD_LOG_DIR/${NC}"
    echo "Failed stages: ${!FAILED_STAGES[@]}"
    exit 1
fi
