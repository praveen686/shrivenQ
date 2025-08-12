#!/bin/bash
# Risk Limits Validation - Critical for Trading Safety
# Ensures all risk parameters are within safe bounds

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "⚠️ Risk Limits Validation"

# Risk validation rules
validate_risk_config() {
    local config_file="$1"

    if [[ ! -f "$config_file" ]]; then
        echo -e "${YELLOW}⚠️  Config file not found: $config_file${NC}"
        return 0
    fi

    echo "🎯 Validating risk limits in: $config_file"

    # Check if running in live mode
    local mode=$(grep "^mode" "$config_file" | cut -d'=' -f2 | xargs | tr -d '"' || echo "paper")

    if [[ "$mode" == "live" ]]; then
        echo -e "${RED}🚨 LIVE TRADING MODE DETECTED${NC}"
        echo -e "${RED}   Extra strict validation applied${NC}"

        # Stricter limits for live trading
        local max_daily_loss_limit=-50000000  # 50 lakh max
        local max_position_limit=500000       # 5 lakh shares max
    else
        # More relaxed limits for paper trading
        local max_daily_loss_limit=-100000000  # 1 crore max
        local max_position_limit=1000000       # 10 lakh shares max
    fi

    local violations=0

    # Extract risk section
    local risk_section=$(awk '/^\[risk\]/,/^\[/' "$config_file" | head -n -1)

    if [[ -z "$risk_section" ]]; then
        echo -e "${RED}❌ No risk section found${NC}"
        return 1
    fi

    # Validate daily loss limit
    local max_daily_loss=$(echo "$risk_section" | grep "^max_daily_loss" | cut -d'=' -f2 | xargs)
    if [[ -n "$max_daily_loss" ]]; then
        if [[ "$max_daily_loss" -lt "$max_daily_loss_limit" ]]; then
            echo -e "${RED}❌ Daily loss limit too high: $max_daily_loss (limit: $max_daily_loss_limit)${NC}"
            violations=$((violations + 1))
        else
            echo -e "${GREEN}✅ Daily loss limit acceptable: $max_daily_loss${NC}"
        fi
    fi

    # Validate position size limits
    local max_position_size=$(echo "$risk_section" | grep "^max_position_size" | cut -d'=' -f2 | xargs)
    if [[ -n "$max_position_size" ]]; then
        if [[ "$max_position_size" -gt "$max_position_limit" ]]; then
            echo -e "${RED}❌ Position size too high: $max_position_size (limit: $max_position_limit)${NC}"
            violations=$((violations + 1))
        else
            echo -e "${GREEN}✅ Position size acceptable: $max_position_size${NC}"
        fi
    fi

    return $violations
}

# Main execution
main() {
    local total_violations=0

    # Check all config files
    for config in config/*.toml config.toml; do
        if [[ -f "$config" ]]; then
            local violations=0
            validate_risk_config "$config" || violations=$?
            total_violations=$((total_violations + violations))
        fi
    done

    if [[ $total_violations -eq 0 ]]; then
        echo -e "${GREEN}✅ All risk limits validated${NC}"
        return 0
    else
        echo -e "${RED}❌ $total_violations risk limit violations${NC}"
        return 1
    fi
}

main "$@"
