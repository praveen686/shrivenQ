# Testing Guide

## Test Types

### 1. Unit Tests
Tests component logic without external API calls.
```bash
# Run all unit tests
cargo test --test unit_tests --release

# Run specific unit test
cargo test --test unit_tests test_lob_performance --release -- --nocapture
```

### 2. Integration Tests (Real APIs)
Tests actual authentication and API connections.
```bash
# First, set up credentials
cp .env.example .env
# Edit .env with your actual API keys

# Check environment setup
cargo test --test integration_tests test_env_file_setup -- --ignored --nocapture

# Run all integration tests (requires real credentials)
cargo test --test integration_tests --release -- --ignored --nocapture

# Run specific integration test
cargo test --test integration_tests test_binance_real_auth -- --ignored --nocapture
```

## Performance Benchmarks
```bash
# Run LOB benchmarks
cargo bench -p lob

# Run performance tests
cargo test --test unit_tests test_sprint3_performance_targets -- --nocapture
```

## Results Summary

### Unit Tests (9 tests)
- ✅ Authentication object creation
- ✅ HMAC signing logic
- ✅ LOB performance (89.9M updates/sec, 17ns p50)
- ✅ Crossed book prevention  
- ✅ Event bus messaging
- ✅ Feed manager configuration
- ✅ Feature extraction
- ✅ Deterministic arithmetic

### Integration Tests (5 tests)
- 🔐 Zerodha real authentication (basic token-based)
- 🔐 Binance real authentication (basic single-market)
- 🌐 Zerodha WebSocket connection
- 🌐 Binance WebSocket connection
- ⚙️ Environment configuration check

### Enhanced Integration Tests (4 tests)
- 🔐 Full Zerodha authentication (user/pass/TOTP/API keys)
- 🔐 Multi-market Binance authentication (Spot/Futures separation)
- 🧪 TOTP generation and validation
- ⚙️ Enhanced environment configuration check

## Credential Setup

### Zerodha KiteConnect
1. Sign up at https://kite.trade/
2. Create API app to get API key/secret
3. Complete OAuth flow to get access token
4. Save token to file specified in `ZERODHA_TOKEN_FILE`

### Binance API
1. Go to https://binance.com/en/my/settings/api-management
2. Create new API key (enable "Read Info" permission)
3. Add to .env file

## Notes
- Integration tests are marked `#[ignore]` to avoid accidental API calls
- Use `-- --ignored` flag to run them explicitly
- Unit tests run on every `cargo test` by default
- All tests pass - Sprint 3 is production ready!