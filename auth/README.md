# Authentication Module

Clean, streamlined authentication for Zerodha and Binance trading platforms.

## Features

### Zerodha
- ✅ **Fully automated login** - No browser interaction required
- ✅ **TOTP 2FA support** - Automatic code generation
- ✅ **Session caching** - 12-hour token validity
- ✅ **Smart token management** - Validates before use

### Binance
- ✅ **Multi-market support** - Spot, USD-M Futures, COIN-M Futures
- ✅ **HMAC-SHA256 signing** - Secure API requests
- ✅ **Per-market credentials** - Separate keys for each market
- ✅ **Credential validation** - Test API connectivity

## Usage

### Zerodha Authentication

```rust
use auth::{ZerodhaAuth, ZerodhaConfig};

// Configure credentials
let config = ZerodhaConfig::new(
    user_id,
    password,
    totp_secret,
    api_key,
    api_secret,
);

// Create auth handler
let auth = ZerodhaAuth::new(config);

// Authenticate (uses cache if available)
let access_token = auth.authenticate().await?;
```

### Binance Authentication

```rust
use auth::{BinanceAuth, BinanceConfig, BinanceMarket};

// Create auth handler
let mut auth = BinanceAuth::new();

// Add market credentials
auth.add_market(BinanceConfig::new(
    spot_api_key,
    spot_api_secret,
    BinanceMarket::Spot,
));

auth.add_market(BinanceConfig::new(
    futures_api_key,
    futures_api_secret,
    BinanceMarket::UsdFutures,
));

// Sign requests
let signature = auth.sign_query(BinanceMarket::Spot, query)?;

// Validate credentials
let is_valid = auth.validate_credentials(BinanceMarket::Spot).await?;
```

## Testing

```bash
# Test Zerodha authentication
cargo run -p auth --bin test_zerodha --release

# Set up .env file with credentials first
cp .env.example .env
# Edit .env with your credentials
```

## Environment Variables

```env
# Zerodha
ZERODHA_USER_ID=your_user_id
ZERODHA_PASSWORD=your_password
ZERODHA_TOTP_SECRET=your_totp_secret
ZERODHA_API_KEY=your_api_key
ZERODHA_API_SECRET=your_api_secret

# Binance Spot
BINANCE_SPOT_API_KEY=your_spot_key
BINANCE_SPOT_API_SECRET=your_spot_secret

# Binance Futures
BINANCE_FUTURES_API_KEY=your_futures_key
BINANCE_FUTURES_API_SECRET=your_futures_secret
```

## Architecture

```
auth/
├── src/
│   ├── lib.rs         # Main module exports
│   ├── zerodha.rs     # Zerodha authentication
│   ├── binance.rs     # Binance authentication
│   └── bin/
│       └── test_zerodha.rs  # Test binary
├── Cargo.toml
└── README.md
```

## Performance

- Zerodha: Cached tokens avoid repeated logins
- Binance: Direct API signing, no token management needed
- Both: Async/await for non-blocking operations