#!/bin/bash

# Setup script for Zerodha automated authentication
# This script helps configure and test the automated login system

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Zerodha Automated Authentication Setup${NC}"
echo -e "${GREEN}========================================${NC}"
echo

# Function to check if environment variable is set
check_env_var() {
    local var_name=$1
    local var_value="${!var_name}"
    
    if [ -z "$var_value" ]; then
        echo -e "${RED}❌ $var_name is not set${NC}"
        return 1
    else
        # Mask sensitive information
        if [[ "$var_name" == *"PASSWORD"* ]] || [[ "$var_name" == *"SECRET"* ]]; then
            echo -e "${GREEN}✅ $var_name is set (hidden)${NC}"
        else
            echo -e "${GREEN}✅ $var_name is set: ${var_value:0:8}...${NC}"
        fi
        return 0
    fi
}

# Function to setup environment variables
setup_env() {
    echo -e "${YELLOW}Setting up environment variables...${NC}"
    echo
    
    # Check for .env file
    if [ -f "$PROJECT_ROOT/.env" ]; then
        echo "Loading existing .env file..."
        source "$PROJECT_ROOT/.env"
    fi
    
    # Check required variables
    local all_set=true
    
    check_env_var "ZERODHA_USER_ID" || all_set=false
    check_env_var "ZERODHA_PASSWORD" || all_set=false
    check_env_var "ZERODHA_TOTP_SECRET" || all_set=false
    check_env_var "ZERODHA_API_KEY" || all_set=false
    check_env_var "ZERODHA_API_SECRET" || all_set=false
    
    if [ "$all_set" = false ]; then
        echo
        echo -e "${YELLOW}Some environment variables are missing.${NC}"
        echo "Would you like to set them up now? (y/n)"
        read -r response
        
        if [[ "$response" == "y" ]]; then
            setup_credentials
        else
            echo -e "${RED}Cannot proceed without all credentials.${NC}"
            exit 1
        fi
    fi
}

# Function to setup credentials interactively
setup_credentials() {
    echo
    echo -e "${YELLOW}Setting up Zerodha credentials...${NC}"
    echo
    
    # Create .env.example if it doesn't exist
    cat > "$PROJECT_ROOT/.env.example" <<EOF
# Zerodha Authentication Configuration
ZERODHA_USER_ID=your_user_id
ZERODHA_PASSWORD=your_password
ZERODHA_TOTP_SECRET=your_totp_secret
ZERODHA_API_KEY=your_api_key
ZERODHA_API_SECRET=your_api_secret
EOF
    
    echo "Please enter your Zerodha credentials:"
    echo
    
    read -p "User ID (trading code): " user_id
    read -sp "Password: " password
    echo
    read -p "TOTP Secret (from authenticator app setup): " totp_secret
    read -p "API Key (from Kite Connect app): " api_key
    read -sp "API Secret (from Kite Connect app): " api_secret
    echo
    
    # Save to .env file
    cat > "$PROJECT_ROOT/.env" <<EOF
# Zerodha Authentication Configuration
# Generated on $(date)
ZERODHA_USER_ID=$user_id
ZERODHA_PASSWORD=$password
ZERODHA_TOTP_SECRET=$totp_secret
ZERODHA_API_KEY=$api_key
ZERODHA_API_SECRET=$api_secret
EOF
    
    echo -e "${GREEN}✅ Credentials saved to .env file${NC}"
    echo
    
    # Add .env to .gitignore if not already there
    if ! grep -q "^.env$" "$PROJECT_ROOT/.gitignore" 2>/dev/null; then
        echo ".env" >> "$PROJECT_ROOT/.gitignore"
        echo -e "${GREEN}✅ Added .env to .gitignore${NC}"
    fi
}

# Function to test authentication
test_auth() {
    echo
    echo -e "${YELLOW}Testing automated authentication...${NC}"
    echo
    
    cd "$PROJECT_ROOT"
    
    # Build the auth service if needed
    echo "Building auth service..."
    cargo build -p auth-service --release
    
    # Run the integration test
    echo
    echo "Running authentication test..."
    echo
    
    if cargo test -p auth-service --test zerodha_integration_test test_zerodha_automated_login -- --ignored --nocapture; then
        echo
        echo -e "${GREEN}✅ Authentication test successful!${NC}"
        echo -e "${GREEN}Your Zerodha automated login is working correctly.${NC}"
        return 0
    else
        echo
        echo -e "${RED}❌ Authentication test failed${NC}"
        echo "Please check your credentials and try again."
        return 1
    fi
}

# Function to show TOTP setup instructions
show_totp_setup() {
    echo
    echo -e "${YELLOW}TOTP Setup Instructions:${NC}"
    echo
    echo "1. When setting up 2FA on Zerodha:"
    echo "   - Choose 'Authenticator App' option"
    echo "   - You'll see a QR code and a secret key"
    echo "   - Copy the secret key (it looks like: JBSWY3DPEHPK3PXP)"
    echo
    echo "2. Save this secret key securely - you'll need it for automated login"
    echo
    echo "3. You can use this secret with any TOTP app:"
    echo "   - Google Authenticator"
    echo "   - Authy"
    echo "   - Microsoft Authenticator"
    echo
    echo "4. The automated login will generate TOTP codes using this secret"
    echo
}

# Function to create systemd service for auth
create_service() {
    echo
    echo -e "${YELLOW}Creating systemd service for auth...${NC}"
    echo
    
    local service_file="/etc/systemd/system/shrivenquant-auth.service"
    
    sudo tee "$service_file" > /dev/null <<EOF
[Unit]
Description=ShrivenQuant Auth Service
After=network.target

[Service]
Type=simple
User=$USER
WorkingDirectory=$PROJECT_ROOT
Environment="RUST_LOG=info"
EnvironmentFile=$PROJECT_ROOT/.env
ExecStart=$PROJECT_ROOT/target/release/auth-service
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF
    
    echo -e "${GREEN}✅ Service file created${NC}"
    
    # Reload systemd and enable service
    sudo systemctl daemon-reload
    sudo systemctl enable shrivenquant-auth.service
    
    echo -e "${GREEN}✅ Service enabled${NC}"
    echo
    echo "To start the service: sudo systemctl start shrivenquant-auth"
    echo "To check status: sudo systemctl status shrivenquant-auth"
    echo "To view logs: journalctl -u shrivenquant-auth -f"
}

# Main menu
main_menu() {
    while true; do
        echo
        echo -e "${GREEN}What would you like to do?${NC}"
        echo "1. Setup/Update credentials"
        echo "2. Test authentication"
        echo "3. Show TOTP setup instructions"
        echo "4. Create systemd service"
        echo "5. Exit"
        echo
        read -p "Enter your choice (1-5): " choice
        
        case $choice in
            1)
                setup_credentials
                ;;
            2)
                setup_env
                test_auth
                ;;
            3)
                show_totp_setup
                ;;
            4)
                create_service
                ;;
            5)
                echo -e "${GREEN}Goodbye!${NC}"
                exit 0
                ;;
            *)
                echo -e "${RED}Invalid choice. Please try again.${NC}"
                ;;
        esac
    done
}

# Main execution
main() {
    # Check if running from correct directory
    if [ ! -f "$PROJECT_ROOT/Cargo.toml" ]; then
        echo -e "${RED}Error: This script must be run from the ShrivenQuant project root${NC}"
        exit 1
    fi
    
    # Check for required tools
    if ! command -v cargo &> /dev/null; then
        echo -e "${RED}Error: Rust/Cargo is not installed${NC}"
        exit 1
    fi
    
    # Start the setup
    main_menu
}

# Run main function
main