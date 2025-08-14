# ShrivenQuant Authentication Service

## 🚀 Fully Automated Zerodha Login

This service provides **completely automated authentication** for Zerodha, including:
- ✅ **Automatic TOTP generation** - No manual 2FA codes needed!
- ✅ **Session caching** - Avoid repeated logins
- ✅ **Token refresh** - Handles expiry automatically
- ✅ **WebSocket authentication** - For real-time market data
- ✅ **Order placement auth** - Full trading capabilities

## 📋 Prerequisites

1. **Zerodha Trading Account**
2. **Kite Connect API App** (create at https://kite.trade)
3. **2FA Setup** with authenticator app
4. **API Credentials**:
   - API Key
   - API Secret
   - TOTP Secret (from 2FA setup)

## 🔧 Quick Setup

### 1. Interactive Setup (Recommended)

```bash
# Run the automated setup script
./scripts/auth/setup-zerodha-auth.sh
```

This will:
- Guide you through credential setup
- Test the authentication
- Create systemd service (optional)

### 2. Manual Setup

Create a `.env` file in the project root:

```env
ZERODHA_USER_ID=your_user_id
ZERODHA_PASSWORD=your_password
ZERODHA_TOTP_SECRET=your_totp_secret
ZERODHA_API_KEY=your_api_key
ZERODHA_API_SECRET=your_api_secret
```

## 🎯 Running the Demo

```bash
# Run the authentication demo
cargo run --example zerodha_auto_login_demo

# Run integration tests
cargo test --ignored zerodha_automated_login -- --nocapture
```

## 💻 Using in Your Code

```rust
use auth::{ZerodhaAuth, ZerodhaConfig};

// Create configuration
let config = ZerodhaConfig::new(
    user_id,
    password,
    totp_secret,
    api_key,
    api_secret,
);

// Initialize auth service
let mut auth = ZerodhaAuth::new(config);

// Automated login with TOTP
let token = auth.authenticate().await?;

// Token is automatically cached
// Future calls use cached token
let token = auth.get_access_token().await?;

// Access user profile
let profile = auth.get_profile().await?;

// Get account margins
let margins = auth.get_margins().await?;
```

## 🔐 How TOTP Works

1. **One-time Setup**: When enabling 2FA on Zerodha, save the secret key
2. **Automatic Generation**: The service generates TOTP codes using this secret
3. **No Manual Entry**: Login is fully automated with generated codes
4. **Time-based**: Codes refresh every 30 seconds automatically

### Getting Your TOTP Secret

When setting up 2FA on Zerodha:
1. Choose "Authenticator App"
2. You'll see a QR code and a **secret key**
3. Copy the secret key (looks like: `JBSWY3DPEHPK3PXP`)
4. Use this in your configuration

## 📊 WebSocket Authentication

```rust
// Get authenticated token
let token = auth.authenticate().await?;

// Use for WebSocket connection
let ws_url = format!(
    "wss://ws.kite.trade?api_key={}&access_token={}",
    api_key,
    token
);

// Connect to WebSocket for live data
// ... WebSocket implementation
```

## 🔄 Session Management

The service handles sessions intelligently:

- **Caching**: Tokens are cached locally (default: `./cache/zerodha/`)
- **Validation**: Checks token validity before each use
- **Auto-refresh**: Gets new token when expired
- **Market hours aware**: Handles market hour transitions

## 🛡️ Security Best Practices

1. **Never commit credentials** - Use `.env` file (gitignored)
2. **Secure storage** - Consider using system keyring for production
3. **Limited scope** - Request only needed permissions
4. **Token rotation** - Tokens auto-expire after market hours
5. **HTTPS only** - All API calls use HTTPS

## 📝 Environment Variables

| Variable | Description | Required |
|----------|-------------|----------|
| `ZERODHA_USER_ID` | Your trading account ID | Yes |
| `ZERODHA_PASSWORD` | Account password | Yes |
| `ZERODHA_TOTP_SECRET` | 2FA secret key | Yes |
| `ZERODHA_API_KEY` | Kite Connect API key | Yes |
| `ZERODHA_API_SECRET` | Kite Connect API secret | Yes |
| `SHRIVEN_CACHE_DIR` | Cache directory | No (default: `./cache/zerodha`) |

## 🧪 Testing

```bash
# Run all auth tests
cargo test -p auth-service

# Run integration tests (requires credentials)
cargo test -p auth-service --ignored -- --nocapture

# Test specific functionality
cargo test -p auth-service test_zerodha_automated_login --ignored -- --nocapture
```

## 🚨 Troubleshooting

### Login Fails
- Verify credentials in `.env`
- Check TOTP secret is correct
- Ensure system time is synchronized

### Token Expires
- Normal after market hours
- Service auto-refreshes on next use

### WebSocket Disconnects
- Check network connectivity
- Verify token is valid
- Reconnect with fresh token

## 📚 API Documentation

For complete Zerodha API documentation, visit:
- [Kite Connect API](https://kite.trade/docs/connect/v3/)
- [WebSocket Streaming](https://kite.trade/docs/connect/v3/websocket/)

## 🤝 Support

For issues or questions:
1. Check the troubleshooting section
2. Run tests to verify setup
3. Check Zerodha API status
4. Open an issue on GitHub