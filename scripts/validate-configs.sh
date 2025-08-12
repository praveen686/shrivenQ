#!/bin/bash
# Configuration Validation for Trading System
# Ensure all configurations are valid and safe for trading

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo "⚙️ Trading System Configuration Validation"

# Configuration files to validate
CONFIG_FILES=(
    "config/development.toml"
    "config/staging.toml"
    "config/production.toml"
    "config.toml"
    ".env.example"
    "docker-compose.yml"
    "Cargo.toml"
)

# Required configuration sections for trading
REQUIRED_SECTIONS=(
    "engine"
    "risk"
    "data"
    "monitoring"
)

# Critical risk parameters that must be validated
RISK_PARAMETERS=(
    "max_position_size"
    "max_position_value"
    "max_total_exposure"
    "max_order_size"
    "max_order_value"
    "max_daily_loss"
    "max_drawdown"
)

# Function to validate TOML syntax and structure
validate_toml_config() {
    local config_file="$1"

    if [[ ! -f "$config_file" ]]; then
        echo -e "${YELLOW}⚠️  Config file not found: $config_file${NC}"
        return 0
    fi

    echo -e "${BLUE}  📁 Validating: $config_file${NC}"

    # Check TOML syntax
    if ! python3 -c "import toml; toml.load('$config_file')" 2>/dev/null; then
        if ! cargo check --manifest-path /dev/null 2>/dev/null; then
            # Try with Rust's built-in TOML parser
            if ! cargo metadata --manifest-path "$config_file" --format-version 1 >/dev/null 2>&1; then
                echo -e "${RED}❌ Invalid TOML syntax in $config_file${NC}"
                return 1
            fi
        fi
    fi

    # Validate required sections
    local missing_sections=()
    for section in "${REQUIRED_SECTIONS[@]}"; do
        if ! grep -q "^\[$section\]" "$config_file"; then
            missing_sections+=("$section")
        fi
    done

    if [[ ${#missing_sections[@]} -gt 0 ]]; then
        echo -e "${RED}❌ Missing required sections in $config_file: ${missing_sections[*]}${NC}"
        return 1
    fi

    echo -e "${GREEN}✅ TOML structure valid${NC}"
    return 0
}

# Function to validate risk parameters
validate_risk_parameters() {
    local config_file="$1"

    if [[ ! -f "$config_file" ]]; then
        return 0
    fi

    echo -e "${BLUE}  🎯 Validating risk parameters in: $config_file${NC}"

    local violations=0

    # Check if risk section exists
    if ! grep -q "^\[risk\]" "$config_file"; then
        echo -e "${RED}❌ No risk section found in $config_file${NC}"
        return 1
    fi

    # Extract risk section
    local risk_section=$(awk '/^\[risk\]/,/^\[/' "$config_file" | head -n -1)

    # Validate each risk parameter
    for param in "${RISK_PARAMETERS[@]}"; do
        if ! echo "$risk_section" | grep -q "^$param"; then
            echo -e "${RED}❌ Missing risk parameter: $param${NC}"
            violations=$((violations + 1))
            continue
        fi

        # Extract parameter value
        local value=$(echo "$risk_section" | grep "^$param" | cut -d'=' -f2 | xargs)

        # Validate specific parameters
        case "$param" in
            "max_daily_loss"|"max_drawdown")
                # These should be negative values
                if [[ ! "$value" =~ ^-[0-9]+$ ]]; then
                    echo -e "${RED}❌ $param should be negative: $value${NC}"
                    violations=$((violations + 1))
                fi

                # Check if loss limits are reasonable (not too large)
                local abs_value=${value#-}
                if [[ "$abs_value" -gt 100000000 ]]; then  # > 10 crore
                    echo -e "${YELLOW}⚠️  $param is very high: $value (>10 crore)${NC}"
                fi
                ;;

            "max_position_size"|"max_order_size")
                # These should be positive integers
                if [[ ! "$value" =~ ^[0-9]+$ ]]; then
                    echo -e "${RED}❌ $param should be positive integer: $value${NC}"
                    violations=$((violations + 1))
                fi

                # Sanity check for reasonable values
                if [[ "$value" -gt 1000000 ]]; then  # > 10 lakh shares
                    echo -e "${YELLOW}⚠️  $param is very high: $value${NC}"
                fi
                ;;

            "max_position_value"|"max_total_exposure"|"max_order_value")
                # These should be positive values in paise
                if [[ ! "$value" =~ ^[0-9]+$ ]]; then
                    echo -e "${RED}❌ $param should be positive integer (paise): $value${NC}"
                    violations=$((violations + 1))
                fi
                ;;
        esac
    done

    # Validate logical relationships between parameters
    local max_order_size=$(echo "$risk_section" | grep "^max_order_size" | cut -d'=' -f2 | xargs)
    local max_position_size=$(echo "$risk_section" | grep "^max_position_size" | cut -d'=' -f2 | xargs)

    if [[ -n "$max_order_size" ]] && [[ -n "$max_position_size" ]]; then
        if [[ "$max_order_size" -gt "$max_position_size" ]]; then
            echo -e "${RED}❌ max_order_size ($max_order_size) > max_position_size ($max_position_size)${NC}"
            violations=$((violations + 1))
        fi
    fi

    if [[ $violations -eq 0 ]]; then
        echo -e "${GREEN}✅ Risk parameters valid${NC}"
    else
        echo -e "${RED}❌ $violations risk parameter violations found${NC}"
    fi

    return $violations
}

# Function to validate engine configuration
validate_engine_config() {
    local config_file="$1"

    if [[ ! -f "$config_file" ]]; then
        return 0
    fi

    echo -e "${BLUE}  🚀 Validating engine configuration in: $config_file${NC}"

    local violations=0

    # Check if engine section exists
    if ! grep -q "^\[engine\]" "$config_file"; then
        echo -e "${RED}❌ No engine section found in $config_file${NC}"
        return 1
    fi

    # Extract engine section
    local engine_section=$(awk '/^\[engine\]/,/^\[/' "$config_file" | head -n -1)

    # Validate execution mode
    if echo "$engine_section" | grep -q "^mode"; then
        local mode=$(echo "$engine_section" | grep "^mode" | cut -d'=' -f2 | xargs | tr -d '"')

        case "$mode" in
            "paper"|"live"|"backtest")
                echo -e "${GREEN}✅ Valid execution mode: $mode${NC}"

                # Warn about live mode
                if [[ "$mode" == "live" ]]; then
                    echo -e "${YELLOW}⚠️  LIVE TRADING MODE ENABLED - Ensure this is intentional!${NC}"
                fi
                ;;
            *)
                echo -e "${RED}❌ Invalid execution mode: $mode (must be paper/live/backtest)${NC}"
                violations=$((violations + 1))
                ;;
        esac
    else
        echo -e "${RED}❌ Missing execution mode in engine config${NC}"
        violations=$((violations + 1))
    fi

    # Validate venue
    if echo "$engine_section" | grep -q "^venue"; then
        local venue=$(echo "$engine_section" | grep "^venue" | cut -d'=' -f2 | xargs | tr -d '"')

        case "$venue" in
            "zerodha"|"binance")
                echo -e "${GREEN}✅ Valid venue: $venue${NC}"
                ;;
            *)
                echo -e "${RED}❌ Invalid venue: $venue (must be zerodha/binance)${NC}"
                violations=$((violations + 1))
                ;;
        esac
    else
        echo -e "${RED}❌ Missing venue in engine config${NC}"
        violations=$((violations + 1))
    fi

    # Validate performance parameters
    local max_positions=$(echo "$engine_section" | grep "^max_positions" | cut -d'=' -f2 | xargs)
    if [[ -n "$max_positions" ]]; then
        if [[ ! "$max_positions" =~ ^[0-9]+$ ]] || [[ "$max_positions" -lt 1 ]]; then
            echo -e "${RED}❌ Invalid max_positions: $max_positions${NC}"
            violations=$((violations + 1))
        elif [[ "$max_positions" -gt 10000 ]]; then
            echo -e "${YELLOW}⚠️  High max_positions: $max_positions${NC}"
        fi
    fi

    local max_orders_per_sec=$(echo "$engine_section" | grep "^max_orders_per_sec" | cut -d'=' -f2 | xargs)
    if [[ -n "$max_orders_per_sec" ]]; then
        if [[ ! "$max_orders_per_sec" =~ ^[0-9]+$ ]] || [[ "$max_orders_per_sec" -lt 1 ]]; then
            echo -e "${RED}❌ Invalid max_orders_per_sec: $max_orders_per_sec${NC}"
            violations=$((violations + 1))
        elif [[ "$max_orders_per_sec" -gt 10000 ]]; then
            echo -e "${YELLOW}⚠️  Very high max_orders_per_sec: $max_orders_per_sec${NC}"
        fi
    fi

    return $violations
}

# Function to validate environment variables
validate_env_variables() {
    echo -e "${BLUE}  🔐 Validating environment variables...${NC}"

    local violations=0

    # Check for .env.example
    if [[ -f ".env.example" ]]; then
        echo -e "${GREEN}✅ Found .env.example${NC}"

        # Check that .env.example doesn't contain real credentials
        if grep -q "your_" ".env.example"; then
            echo -e "${GREEN}✅ .env.example contains placeholder values${NC}"
        else
            echo -e "${YELLOW}⚠️  .env.example might contain real credentials${NC}"
        fi
    else
        echo -e "${YELLOW}⚠️  No .env.example found${NC}"
    fi

    # Check that .env is not committed
    if [[ -f ".env" ]]; then
        echo -e "${RED}❌ .env file found - this should not be committed${NC}"
        violations=$((violations + 1))
    else
        echo -e "${GREEN}✅ No .env file in repository${NC}"
    fi

    # Check gitignore for sensitive files
    if [[ -f ".gitignore" ]]; then
        if grep -q "\.env" ".gitignore"; then
            echo -e "${GREEN}✅ .env files are gitignored${NC}"
        else
            echo -e "${YELLOW}⚠️  .env not in .gitignore${NC}"
        fi
    fi

    return $violations
}

# Function to validate Docker configuration
validate_docker_config() {
    echo -e "${BLUE}  🐳 Validating Docker configuration...${NC}"

    local violations=0

    # Check docker-compose.yml
    if [[ -f "docker-compose.yml" ]]; then
        echo -e "${GREEN}✅ Found docker-compose.yml${NC}"

        # Check for YAML syntax
        if command -v python3 >/dev/null 2>&1; then
            if ! python3 -c "import yaml; yaml.safe_load(open('docker-compose.yml'))" 2>/dev/null; then
                echo -e "${RED}❌ Invalid YAML syntax in docker-compose.yml${NC}"
                violations=$((violations + 1))
            else
                echo -e "${GREEN}✅ Valid YAML syntax${NC}"
            fi
        fi

        # Check for security issues
        if grep -q "privileged.*true" "docker-compose.yml"; then
            echo -e "${YELLOW}⚠️  Privileged mode detected in Docker config${NC}"
        fi

        if grep -q "network_mode.*host" "docker-compose.yml"; then
            echo -e "${YELLOW}⚠️  Host networking detected in Docker config${NC}"
        fi
    fi

    # Check Dockerfile
    if [[ -f "Dockerfile" ]]; then
        echo -e "${GREEN}✅ Found Dockerfile${NC}"

        # Check for security best practices
        if ! grep -q "USER" "Dockerfile"; then
            echo -e "${YELLOW}⚠️  Dockerfile doesn't specify USER (security risk)${NC}"
        fi

        if grep -q "ADD.*http" "Dockerfile"; then
            echo -e "${YELLOW}⚠️  Dockerfile uses ADD with URL (prefer COPY)${NC}"
        fi
    fi

    return $violations
}

# Function to validate Cargo.toml workspace
validate_cargo_workspace() {
    echo -e "${BLUE}  📦 Validating Cargo workspace...${NC}"

    local violations=0

    if [[ -f "Cargo.toml" ]]; then
        # Check workspace structure
        if grep -q "^\[workspace\]" "Cargo.toml"; then
            echo -e "${GREEN}✅ Workspace configuration found${NC}"

            # Validate member crates exist
            local members=$(grep -A 20 "^\[workspace\]" "Cargo.toml" | grep -A 15 "^members" | grep '"' | sed 's/.*"\(.*\)".*/\1/')

            for member in $members; do
                if [[ -d "$member" ]] && [[ -f "$member/Cargo.toml" ]]; then
                    echo -e "${GREEN}✅ Member crate exists: $member${NC}"
                else
                    echo -e "${RED}❌ Member crate missing: $member${NC}"
                    violations=$((violations + 1))
                fi
            done
        fi

        # Check for version consistency
        if grep -q "version.workspace = true" */Cargo.toml 2>/dev/null; then
            echo -e "${GREEN}✅ Using workspace version management${NC}"
        fi

        # Check for missing metadata
        if ! grep -q "authors.workspace = true" */Cargo.toml 2>/dev/null; then
            echo -e "${YELLOW}⚠️  Some crates missing workspace authors${NC}"
        fi

    else
        echo -e "${RED}❌ No Cargo.toml found${NC}"
        violations=$((violations + 1))
    fi

    return $violations
}

# Function to generate configuration report
generate_config_report() {
    echo "📊 Generating configuration validation report..."

    local report_file="config_validation_report.md"

    cat > "$report_file" << EOF
# Configuration Validation Report

Generated: $(date)

## Validated Files

EOF

    for config_file in "${CONFIG_FILES[@]}"; do
        if [[ -f "$config_file" ]]; then
            echo "- ✅ $config_file" >> "$report_file"
        else
            echo "- ❌ $config_file (missing)" >> "$report_file"
        fi
    done

    cat >> "$report_file" << EOF

## Validation Results

### Risk Parameters
$(validate_risk_parameters "config.toml" 2>&1 | grep -E "✅|❌|⚠️" | wc -l) checks performed

### Engine Configuration
$(validate_engine_config "config.toml" 2>&1 | grep -E "✅|❌|⚠️" | wc -l) checks performed

### Environment Variables
$(validate_env_variables 2>&1 | grep -E "✅|❌|⚠️" | wc -l) checks performed

### Docker Configuration
$(validate_docker_config 2>&1 | grep -E "✅|❌|⚠️" | wc -l) checks performed

## Recommendations

1. Always validate configurations before deployment
2. Use environment-specific config files
3. Never commit sensitive credentials
4. Test configurations in staging environment
5. Monitor configuration drift in production

EOF

    echo -e "${GREEN}✅ Report generated: $report_file${NC}"
}

# Main function
main() {
    echo "🎯 Trading System Configuration Validation"
    echo "==========================================="

    local total_violations=0

    echo ""
    echo "1️⃣  Validating TOML configurations..."
    for config_file in "${CONFIG_FILES[@]}"; do
        if [[ "$config_file" == *.toml ]]; then
            local violations=0
            validate_toml_config "$config_file" || violations=$?
            total_violations=$((total_violations + violations))
        fi
    done

    echo ""
    echo "2️⃣  Validating risk parameters..."
    for config_file in config/*.toml config.toml; do
        if [[ -f "$config_file" ]]; then
            local violations=0
            validate_risk_parameters "$config_file" || violations=$?
            total_violations=$((total_violations + violations))
        fi
    done

    echo ""
    echo "3️⃣  Validating engine configuration..."
    for config_file in config/*.toml config.toml; do
        if [[ -f "$config_file" ]]; then
            local violations=0
            validate_engine_config "$config_file" || violations=$?
            total_violations=$((total_violations + violations))
        fi
    done

    echo ""
    echo "4️⃣  Validating environment variables..."
    local violations=0
    validate_env_variables || violations=$?
    total_violations=$((total_violations + violations))

    echo ""
    echo "5️⃣  Validating Docker configuration..."
    local violations=0
    validate_docker_config || violations=$?
    total_violations=$((total_violations + violations))

    echo ""
    echo "6️⃣  Validating Cargo workspace..."
    local violations=0
    validate_cargo_workspace || violations=$?
    total_violations=$((total_violations + violations))

    echo ""
    echo "7️⃣  Generating report..."
    generate_config_report

    # Summary
    echo ""
    echo "📋 Validation Summary"
    echo "===================="
    echo "Total violations found: $total_violations"

    if [[ $total_violations -eq 0 ]]; then
        echo -e "${GREEN}🎉 All configurations are valid!${NC}"
        echo -e "${GREEN}   System configuration ready for trading${NC}"
        return 0
    else
        echo -e "${RED}❌ Configuration validation failed!${NC}"
        echo -e "${RED}   Fix $total_violations issues before commit${NC}"
        return 1
    fi
}

# Execute main function
main "$@"
