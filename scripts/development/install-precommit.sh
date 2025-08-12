#!/bin/bash
# Pre-commit Hook Installation Script for ShrivenQuant

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
NC='\033[0m'

echo -e "${PURPLE}🚀 ShrivenQuant Pre-Commit Hook Installation${NC}"
echo -e "${PURPLE}============================================${NC}"

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Function to install system dependencies
install_dependencies() {
    echo -e "${BLUE}📦 Installing system dependencies...${NC}"

    # Python and pip
    if ! command_exists python3; then
        echo -e "${YELLOW}⚠️  Python3 not found, installing...${NC}"
        if command_exists apt-get; then
            sudo apt-get update && sudo apt-get install -y python3 python3-pip
        elif command_exists brew; then
            brew install python3
        else
            echo -e "${RED}❌ Cannot install Python3, please install manually${NC}"
            exit 1
        fi
    fi

    # pre-commit
    if ! command_exists pre-commit; then
        echo -e "${YELLOW}⚠️  pre-commit not found, installing...${NC}"
        pip3 install pre-commit
    fi

    # Additional tools
    local tools=(
        "jq"           # JSON processing
        "bc"           # Calculator for performance checks
        "valgrind"     # Memory checking
    )

    for tool in "${tools[@]}"; do
        if ! command_exists "$tool"; then
            echo -e "${YELLOW}⚠️  Installing $tool...${NC}"
            if command_exists apt-get; then
                sudo apt-get install -y "$tool"
            elif command_exists brew; then
                brew install "$tool"
            else
                echo -e "${YELLOW}⚠️  Could not install $tool, some checks may be skipped${NC}"
            fi
        fi
    done

    echo -e "${GREEN}✅ Dependencies installed${NC}"
}

# Function to install Rust tools
install_rust_tools() {
    echo -e "${BLUE}🦀 Installing Rust tools...${NC}"

    # Ensure we have the latest Rust
    if ! command_exists rustc; then
        echo -e "${RED}❌ Rust not found, please install Rust first${NC}"
        echo -e "${BLUE}ℹ️  Run: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${NC}"
        exit 1
    fi

    # Install nightly for SIMD features
    rustup install nightly
    rustup component add rustfmt clippy --toolchain nightly

    # Useful Cargo extensions
    local cargo_tools=(
        "cargo-audit"      # Security auditing
        "cargo-tarpaulin"  # Code coverage
        "cargo-expand"     # Macro expansion
        "cargo-public-api" # API compatibility
    )

    for tool in "${cargo_tools[@]}"; do
        if ! command_exists "$tool"; then
            echo -e "${YELLOW}⚠️  Installing $tool...${NC}"
            cargo install "$tool" || echo -e "${YELLOW}⚠️  Failed to install $tool, continuing...${NC}"
        fi
    done

    echo -e "${GREEN}✅ Rust tools installed${NC}"
}

# Function to setup pre-commit
setup_precommit() {
    echo -e "${BLUE}🔧 Setting up pre-commit hooks...${NC}"

    # Ensure we're in a git repository
    if [[ ! -d ".git" ]]; then
        echo -e "${RED}❌ Not in a git repository${NC}"
        exit 1
    fi

    # Install pre-commit hooks
    pre-commit install
    pre-commit install --hook-type commit-msg
    pre-commit install --hook-type pre-push

    # Test hook installation
    if pre-commit run --all-files >/dev/null 2>&1; then
        echo -e "${GREEN}✅ Pre-commit hooks installed successfully${NC}"
    else
        echo -e "${YELLOW}⚠️  Pre-commit hooks installed but some checks failed${NC}"
        echo -e "${YELLOW}   Run 'pre-commit run --all-files' to see details${NC}"
    fi
}

# Function to create commit message template
setup_commit_template() {
    echo -e "${BLUE}💬 Setting up commit message template...${NC}"

    cat > .gitmessage << 'EOF'
# <type>(<scope>): <subject>
#
# <body>
#
# <footer>
#
# Type should be one of:
# * feat:     A new feature
# * fix:      A bug fix
# * docs:     Documentation only changes
# * style:    Changes that do not affect the meaning of the code
# * refactor: A code change that neither fixes a bug nor adds a feature
# * perf:     A code change that improves performance
# * test:     Adding missing tests or correcting existing tests
# * build:    Changes that affect the build system or external dependencies
# * ci:       Changes to our CI configuration files and scripts
# * chore:    Other changes that don't modify src or test files
# * revert:   Reverts a previous commit
#
# Examples:
# feat(engine): add ultra-low latency order processing
# fix(lob): resolve price-time priority issue
# perf(risk): optimize branch-free risk checks
# docs(api): update trading engine documentation
EOF

    git config commit.template .gitmessage
    echo -e "${GREEN}✅ Commit message template configured${NC}"
}

# Function to create development workflow shortcuts
create_shortcuts() {
    echo -e "${BLUE}⚡ Creating development shortcuts...${NC}"

    cat > scripts/dev-shortcuts.sh << 'EOF'
#!/bin/bash
# Development Shortcuts for ShrivenQuant

alias sq-fmt="cargo fmt --all"
alias sq-check="cargo clippy --all-targets --all-features -- -D warnings"
alias sq-test="cargo test --all-features"
alias sq-bench="cargo bench"
alias sq-build-release="cargo build --release --all-targets"
alias sq-audit="cargo audit"
alias sq-perf="./scripts/performance-check.sh"
alias sq-validate="./scripts/system-validation.sh"

# Quick validation pipeline
alias sq-quick="cargo fmt --all --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-features"

# Full validation (like pre-commit)
alias sq-full="./scripts/system-validation.sh"

echo "ShrivenQuant development shortcuts loaded:"
echo "  sq-fmt        - Format all code"
echo "  sq-check      - Run clippy checks"
echo "  sq-test       - Run all tests"
echo "  sq-bench      - Run benchmarks"
echo "  sq-build-release - Release build"
echo "  sq-audit      - Security audit"
echo "  sq-perf       - Performance checks"
echo "  sq-validate   - Full system validation"
echo "  sq-quick      - Quick validation"
echo "  sq-full       - Complete validation"
EOF

    chmod +x scripts/dev-shortcuts.sh
    echo -e "${GREEN}✅ Development shortcuts created${NC}"
    echo -e "${BLUE}ℹ️  Source with: source scripts/dev-shortcuts.sh${NC}"
}

# Function to verify installation
verify_installation() {
    echo -e "${BLUE}🔍 Verifying installation...${NC}"

    local checks=(
        "pre-commit --version"
        "cargo --version"
        "rustc --version"
    )

    local failed=0
    for check in "${checks[@]}"; do
        if eval "$check" >/dev/null 2>&1; then
            echo -e "${GREEN}✅ $check${NC}"
        else
            echo -e "${RED}❌ $check${NC}"
            failed=1
        fi
    done

    # Test pre-commit configuration
    if pre-commit validate-config; then
        echo -e "${GREEN}✅ Pre-commit configuration valid${NC}"
    else
        echo -e "${RED}❌ Pre-commit configuration invalid${NC}"
        failed=1
    fi

    if [[ $failed -eq 0 ]]; then
        echo -e "${GREEN}🎉 Installation verified successfully!${NC}"
    else
        echo -e "${RED}❌ Installation verification failed${NC}"
        exit 1
    fi
}

# Function to show usage instructions
show_usage() {
    echo -e "${PURPLE}📋 Usage Instructions${NC}"
    echo -e "${PURPLE}===================${NC}"
    echo ""
    echo -e "${GREEN}Pre-commit hooks are now active!${NC}"
    echo ""
    echo -e "${BLUE}What happens now:${NC}"
    echo "• Every commit will run quality checks automatically"
    echo "• Commits will be blocked if checks fail"
    echo "• Push operations will run additional validations"
    echo ""
    echo -e "${BLUE}Manual commands:${NC}"
    echo "• ${YELLOW}pre-commit run --all-files${NC}     - Run all hooks on all files"
    echo "• ${YELLOW}./scripts/system-validation.sh${NC}  - Full system validation"
    echo "• ${YELLOW}./scripts/performance-check.sh${NC}  - Performance regression check"
    echo "• ${YELLOW}source scripts/dev-shortcuts.sh${NC} - Load development shortcuts"
    echo ""
    echo -e "${BLUE}Skip hooks (emergency only):${NC}"
    echo "• ${YELLOW}git commit --no-verify${NC}          - Skip pre-commit hooks"
    echo "• ${YELLOW}git push --no-verify${NC}            - Skip pre-push hooks"
    echo ""
    echo -e "${RED}⚠️  WARNING: Only skip hooks in emergencies!${NC}"
    echo -e "${RED}   Our trading system demands the highest quality standards${NC}"
}

# Main installation function
main() {
    echo -e "${GREEN}Welcome to ShrivenQuant Pre-Commit Setup!${NC}"
    echo ""
    echo "This will install and configure:"
    echo "• Pre-commit hooks for code quality"
    echo "• Performance regression testing"
    echo "• Security vulnerability scanning"
    echo "• Trading system specific validations"
    echo ""

    read -p "Continue with installation? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Installation cancelled."
        exit 0
    fi

    echo ""
    echo -e "${PURPLE}🔧 Starting Installation Process${NC}"
    echo ""

    # Run installation steps
    install_dependencies
    echo ""

    install_rust_tools
    echo ""

    setup_precommit
    echo ""

    setup_commit_template
    echo ""

    create_shortcuts
    echo ""

    verify_installation
    echo ""

    show_usage

    echo ""
    echo -e "${PURPLE}🎉 ShrivenQuant Pre-Commit Setup Complete!${NC}"
    echo -e "${GREEN}   Your ultra-low latency trading system is now protected${NC}"
    echo -e "${GREEN}   by the most comprehensive code quality system available!${NC}"
}

# Run main installation
main "$@"
