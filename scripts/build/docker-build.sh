#!/bin/bash
# ShrivenQuant Docker Multi-Stage Build Script
# Creates optimized Docker images for different deployment scenarios

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}🐳 ShrivenQuant Docker Builder${NC}"
echo "==============================="

# Configuration
IMAGE_NAME="shrivenquant"
REGISTRY="${DOCKER_REGISTRY:-ghcr.io/praveen686}"
VERSION=$(git rev-parse --short HEAD 2>/dev/null || echo "latest")
BUILD_DATE=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Build variants
VARIANTS=(
    "runtime:Minimal runtime image"
    "development:Full development environment"
    "testing:Testing and CI environment"
    "benchmark:Performance benchmarking image"
)

create_dockerfile_runtime() {
    cat > Dockerfile.runtime <<EOF
# ShrivenQuant Runtime Image - Ultra-minimal for production
FROM rust:1.75-slim as builder

# Install system dependencies
RUN apt-get update && apt-get install -y \\
    pkg-config \\
    libssl-dev \\
    && rm -rf /var/lib/apt/lists/*

# Set up build environment
WORKDIR /build
COPY . .

# Build with maximum optimizations
ENV RUSTFLAGS="-C target-cpu=native -C opt-level=3 -C lto=fat -C panic=abort"
RUN cargo build --release --bin cli

# Runtime stage - minimal distroless image
FROM gcr.io/distroless/cc-debian12:latest

# Metadata
LABEL org.opencontainers.image.title="ShrivenQuant"
LABEL org.opencontainers.image.description="Ultra-Low Latency Trading Platform"
LABEL org.opencontainers.image.version="$VERSION"
LABEL org.opencontainers.image.created="$BUILD_DATE"
LABEL org.opencontainers.image.source="https://github.com/praveen686/shrivenQ"

# Copy binary
COPY --from=builder /build/target/release/cli /usr/local/bin/shrivenquant

# Create non-root user
USER 1000:1000

# Default configuration
ENV RUST_LOG=info
ENV SHRIVENQUANT_CONFIG_PATH=/etc/shrivenquant

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \\
    CMD ["/usr/local/bin/shrivenquant", "health", "check"]

# Default command
ENTRYPOINT ["/usr/local/bin/shrivenquant"]
CMD ["--help"]
EOF
}

create_dockerfile_development() {
    cat > Dockerfile.development <<EOF
# ShrivenQuant Development Environment
FROM rust:1.75-bullseye

# Install system dependencies and tools
RUN apt-get update && apt-get install -y \\
    pkg-config \\
    libssl-dev \\
    build-essential \\
    cmake \\
    git \\
    vim \\
    tmux \\
    htop \\
    curl \\
    jq \\
    && rm -rf /var/lib/apt/lists/*

# Install Rust development tools
RUN rustup component add clippy rustfmt rust-analyzer && \\
    cargo install cargo-watch cargo-audit cargo-outdated cargo-deny

# Set up development environment
WORKDIR /workspace
ENV PATH="/workspace/target/debug:/workspace/target/release:\$PATH"

# Copy project files
COPY . .

# Pre-build dependencies for faster iteration
RUN cargo fetch && \\
    cargo build --all-targets

# Development configuration
ENV RUST_LOG=debug
ENV RUST_BACKTRACE=1
ENV CARGO_INCREMENTAL=1

# Default command for development
CMD ["cargo", "watch", "-x", "check"]
EOF
}

create_dockerfile_testing() {
    cat > Dockerfile.testing <<EOF
# ShrivenQuant Testing Environment
FROM rust:1.75-bullseye

# Install testing dependencies
RUN apt-get update && apt-get install -y \\
    pkg-config \\
    libssl-dev \\
    valgrind \\
    gdb \\
    strace \\
    && rm -rf /var/lib/apt/lists/*

# Install test coverage and benchmark tools
RUN cargo install cargo-tarpaulin cargo-criterion

WORKDIR /app
COPY . .

# Build test dependencies
RUN cargo fetch && \\
    cargo test --no-run --all-targets

# Testing configuration
ENV RUST_LOG=debug
ENV RUST_TEST_THREADS=1
ENV TARPAULIN_TIMEOUT=300

# Default testing command
CMD ["cargo", "test", "--all-targets", "--all-features"]
EOF
}

create_dockerfile_benchmark() {
    cat > Dockerfile.benchmark <<EOF
# ShrivenQuant Benchmarking Environment
FROM rust:1.75-bullseye

# Install performance profiling tools
RUN apt-get update && apt-get install -y \\
    pkg-config \\
    libssl-dev \\
    linux-perf \\
    valgrind \\
    heaptrack \\
    && rm -rf /var/lib/apt/lists/*

# Install Rust benchmarking tools
RUN cargo install cargo-criterion flamegraph

# Performance-optimized build environment
ENV RUSTFLAGS="-C target-cpu=native -C opt-level=3"

WORKDIR /benchmarks
COPY . .

# Pre-build benchmarks
RUN cargo build --release --benches

# Benchmarking configuration
ENV CRITERION_HOME=/benchmarks/target/criterion
ENV RUST_LOG=info

# Default benchmark command
CMD ["cargo", "bench", "--all"]
EOF
}

build_image() {
    local variant="$1"
    local description="$2"
    local dockerfile="Dockerfile.$variant"
    local image_tag="$REGISTRY/$IMAGE_NAME:$variant-$VERSION"
    local latest_tag="$REGISTRY/$IMAGE_NAME:$variant-latest"

    echo -e "\n${YELLOW}Building $description...${NC}"
    echo "Image: $image_tag"

    # Create Dockerfile for variant
    case "$variant" in
        "runtime") create_dockerfile_runtime ;;
        "development") create_dockerfile_development ;;
        "testing") create_dockerfile_testing ;;
        "benchmark") create_dockerfile_benchmark ;;
    esac

    # Build image
    if docker build -f "$dockerfile" -t "$image_tag" -t "$latest_tag" .; then
        echo -e "${GREEN}✅ Built $image_tag${NC}"

        # Get image size
        local size=$(docker images "$image_tag" --format "table {{.Size}}" | tail -n 1)
        echo -e "📦 Image size: $size"

        # Clean up Dockerfile
        rm -f "$dockerfile"

        return 0
    else
        echo -e "${RED}❌ Failed to build $image_tag${NC}"
        rm -f "$dockerfile"
        return 1
    fi
}

# Ensure Docker is available
if ! command -v docker >/dev/null 2>&1; then
    echo -e "${RED}❌ Docker is not installed or not in PATH${NC}"
    exit 1
fi

# Check if Docker daemon is running
if ! docker info >/dev/null 2>&1; then
    echo -e "${RED}❌ Docker daemon is not running${NC}"
    exit 1
fi

# Build each variant
SUCCESSFUL_BUILDS=0
TOTAL_VARIANTS=${#VARIANTS[@]}

for variant_info in "${VARIANTS[@]}"; do
    IFS=':' read -r variant description <<< "$variant_info"

    if build_image "$variant" "$description"; then
        ((SUCCESSFUL_BUILDS++))
    fi
done

# Summary
echo -e "\n${BLUE}📊 Docker Build Summary${NC}"
echo "======================="
echo -e "Successful builds: ${GREEN}$SUCCESSFUL_BUILDS${NC}/$TOTAL_VARIANTS"
echo ""

# List built images
echo "Built images:"
docker images "$REGISTRY/$IMAGE_NAME" --format "table {{.Repository}}:{{.Tag}}\t{{.Size}}\t{{.CreatedAt}}"

# Security scan (if trivy is available)
if command -v trivy >/dev/null 2>&1; then
    echo -e "\n${YELLOW}🛡️  Running security scans...${NC}"

    for variant_info in "${VARIANTS[@]}"; do
        IFS=':' read -r variant description <<< "$variant_info"
        local image_tag="$REGISTRY/$IMAGE_NAME:$variant-$VERSION"

        echo "Scanning $image_tag..."
        trivy image --quiet --format table "$image_tag" || true
    done
fi

# Push images (if registry credentials are available)
if [ -n "${DOCKER_REGISTRY:-}" ] && [ -n "${DOCKER_USERNAME:-}" ]; then
    echo -e "\n${BLUE}🚀 Pushing images to registry...${NC}"

    # Login to registry
    echo "$DOCKER_PASSWORD" | docker login "$DOCKER_REGISTRY" -u "$DOCKER_USERNAME" --password-stdin

    for variant_info in "${VARIANTS[@]}"; do
        IFS=':' read -r variant description <<< "$variant_info"
        local image_tag="$REGISTRY/$IMAGE_NAME:$variant-$VERSION"
        local latest_tag="$REGISTRY/$IMAGE_NAME:$variant-latest"

        echo "Pushing $image_tag..."
        docker push "$image_tag"
        docker push "$latest_tag"
    done

    echo -e "${GREEN}✅ All images pushed successfully${NC}"
fi

if [ "$SUCCESSFUL_BUILDS" -eq "$TOTAL_VARIANTS" ]; then
    echo -e "\n${GREEN}🎉 All Docker builds completed successfully!${NC}"
    exit 0
else
    echo -e "\n${YELLOW}⚠️  Some Docker builds failed. Check output above for details.${NC}"
    exit 1
fi
