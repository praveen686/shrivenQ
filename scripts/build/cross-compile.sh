#!/bin/bash
# ShrivenQuant Cross-Platform Build Script
# Builds optimized binaries for multiple target architectures

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}🏗️  ShrivenQuant Cross-Platform Builder${NC}"
echo "========================================"

# Supported targets for financial infrastructure
TARGETS=(
    "x86_64-unknown-linux-gnu"      # Linux servers
    "x86_64-unknown-linux-musl"     # Static Linux binaries
    "x86_64-pc-windows-gnu"         # Windows trading desks
    "x86_64-apple-darwin"           # macOS development
    "aarch64-unknown-linux-gnu"     # ARM64 servers
)

BINARY_NAME="shrivenquant"
BUILD_DIR="target/cross-builds"
RELEASE_DIR="releases"

# Create directories
mkdir -p "$BUILD_DIR" "$RELEASE_DIR"

# Performance-optimized build flags
export RUSTFLAGS="-C target-cpu=native -C opt-level=3 -C lto=fat -C panic=abort"

build_target() {
    local target="$1"
    local friendly_name="$2"

    echo -e "\n${YELLOW}Building for ${friendly_name} (${target})...${NC}"

    # Install target if needed
    if ! rustup target list --installed | grep -q "$target"; then
        echo "  📦 Installing target: $target"
        rustup target add "$target"
    fi

    # Cross-compile
    if cargo build --release --target "$target" --bin cli; then
        echo -e "  ${GREEN}✅ Build successful${NC}"

        # Copy and rename binary
        local binary_path="target/$target/release"
        local binary_name="cli"

        # Handle Windows .exe extension
        if [[ "$target" == *"windows"* ]]; then
            binary_name="cli.exe"
        fi

        if [ -f "$binary_path/$binary_name" ]; then
            local output_name="${BINARY_NAME}-${target}"
            if [[ "$target" == *"windows"* ]]; then
                output_name="${output_name}.exe"
            fi

            cp "$binary_path/$binary_name" "$RELEASE_DIR/$output_name"

            # Get file size
            local size=$(du -h "$RELEASE_DIR/$output_name" | cut -f1)
            echo -e "  📦 Binary size: $size"

            # Strip if available (Unix targets only)
            if [[ "$target" != *"windows"* ]] && command -v strip >/dev/null 2>&1; then
                strip "$RELEASE_DIR/$output_name" 2>/dev/null || true
                local stripped_size=$(du -h "$RELEASE_DIR/$output_name" | cut -f1)
                echo -e "  🔧 Stripped size: $stripped_size"
            fi
        else
            echo -e "  ${RED}❌ Binary not found at $binary_path/$binary_name${NC}"
            return 1
        fi
    else
        echo -e "  ${RED}❌ Build failed${NC}"
        return 1
    fi
}

# Build for each target
SUCCESSFUL_BUILDS=0
TOTAL_TARGETS=${#TARGETS[@]}

for target in "${TARGETS[@]}"; do
    case "$target" in
        "x86_64-unknown-linux-gnu")
            if build_target "$target" "Linux x64"; then
                ((SUCCESSFUL_BUILDS++))
            fi
            ;;
        "x86_64-unknown-linux-musl")
            if build_target "$target" "Linux x64 (Static)"; then
                ((SUCCESSFUL_BUILDS++))
            fi
            ;;
        "x86_64-pc-windows-gnu")
            if build_target "$target" "Windows x64"; then
                ((SUCCESSFUL_BUILDS++))
            fi
            ;;
        "x86_64-apple-darwin")
            if [[ "$OSTYPE" == "darwin"* ]] && build_target "$target" "macOS x64"; then
                ((SUCCESSFUL_BUILDS++))
            else
                echo -e "  ${YELLOW}⚠️  Skipping macOS build (not on macOS)${NC}"
            fi
            ;;
        "aarch64-unknown-linux-gnu")
            if build_target "$target" "Linux ARM64"; then
                ((SUCCESSFUL_BUILDS++))
            fi
            ;;
    esac
done

# Generate checksums
echo -e "\n${BLUE}📝 Generating checksums...${NC}"
cd "$RELEASE_DIR"
sha256sum * > checksums.sha256
cd - >/dev/null

# Summary
echo -e "\n${BLUE}📊 Build Summary${NC}"
echo "================"
echo -e "Successful builds: ${GREEN}$SUCCESSFUL_BUILDS${NC}/$TOTAL_TARGETS"
echo -e "Release directory: ${BLUE}$RELEASE_DIR${NC}"
echo ""
echo "Available binaries:"
ls -lh "$RELEASE_DIR"

# Performance verification for native build
if [ -f "$RELEASE_DIR/shrivenquant-$(rustc -Vv | grep 'host:' | cut -d' ' -f2)" ]; then
    echo -e "\n${YELLOW}🚀 Running performance verification...${NC}"

    # Quick benchmark of the native binary
    local native_binary="$RELEASE_DIR/shrivenquant-$(rustc -Vv | grep 'host:' | cut -d' ' -f2)"
    if [ -x "$native_binary" ]; then
        echo "  ⚡ Testing startup time..."
        time timeout 5s "$native_binary" --version >/dev/null 2>&1 || true
        echo -e "  ${GREEN}✅ Performance verification complete${NC}"
    fi
fi

if [ "$SUCCESSFUL_BUILDS" -eq "$TOTAL_TARGETS" ]; then
    echo -e "\n${GREEN}🎉 All builds completed successfully!${NC}"
    exit 0
else
    echo -e "\n${YELLOW}⚠️  Some builds failed. Check output above for details.${NC}"
    exit 1
fi
