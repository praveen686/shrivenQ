# ShrivenQ Instrument Service

Production-grade instrument management system for fetching and caching trading instruments from multiple exchanges.

## Features

### ✅ Complete Architecture
- **Daily automatic updates** at 8:00 AM IST
- **Multi-venue support** (Zerodha, Binance, extensible)
- **Persistent caching** with JSON storage
- **Efficient lookups** with multiple indices
- **Retry logic** with exponential backoff
- **Comprehensive monitoring** and error handling

### 📊 Data Structure
- Full instrument metadata (tokens, symbols, types, segments)
- Support for equities, indices, futures, options, currencies, commodities
- Option chains and active futures tracking
- Tick size and lot size information
- Expiry dates and strike prices for derivatives

### 🚀 Performance
- In-memory storage with RwLock for concurrent access
- Multiple lookup indices (by token, symbol, type)
- Lazy loading from cache on startup
- Incremental updates support (ETags ready)

## Usage

### Run the Service

```bash
# Start continuous service (fetches daily at 8 AM IST)
cargo run --bin instrument-service

# Run once and exit
cargo run --bin instrument-service run --once

# Fetch immediately
cargo run --bin instrument-service fetch

# Show cached instruments
cargo run --bin instrument-service show

# Show specific symbol
cargo run --bin instrument-service show --symbol NIFTY

# Show all indices
cargo run --bin instrument-service show --indices

# Show futures for underlying
cargo run --bin instrument-service show --futures NIFTY

# Validate cache
cargo run --bin instrument-service validate
```

### Environment Variables

Create a `.env` file with:
```bash
# Zerodha credentials
ZERODHA_API_KEY=your_api_key
ZERODHA_API_SECRET=your_api_secret
ZERODHA_USER_ID=your_user_id
ZERODHA_PASSWORD=your_password
ZERODHA_TOTP_SECRET=your_totp_secret
```

### Programmatic Usage

```rust
use feeds::instrument_fetcher::{InstrumentFetcher, InstrumentFetcherConfig};
use auth::{ZerodhaAuth, ZerodhaConfig};

// Create fetcher
let config = InstrumentFetcherConfig::default();
let zerodha_auth = ZerodhaAuth::new(zerodha_config);
let fetcher = InstrumentFetcher::new(config, Some(zerodha_auth), None)?;

// Fetch all instruments
fetcher.fetch_all().await?;

// Access the store
let store = fetcher.store();

// Lookup by token
let instrument = store.get_by_token(256265).await;

// Get all indices
let indices = store.get_indices().await;

// Get active futures
let nifty_futures = store.get_active_futures("NIFTY").await;

// Get option chain
let options = store.get_option_chain("NIFTY", expiry_date).await;
```

## Architecture

### Components

1. **InstrumentStore**: Core storage with multiple indices
   - By token (primary key)
   - By trading symbol
   - By exchange symbol
   - Active futures by underlying
   - Option chains by underlying and expiry
   - Indices collection

2. **InstrumentFetcher**: Service layer
   - Manages fetch scheduling
   - Handles authentication
   - Implements retry logic
   - Manages cache persistence

3. **Instrument Model**: Comprehensive data structure
   - Supports multiple instrument types
   - Venue-agnostic design
   - Extensible metadata

### Data Flow

```
Exchange APIs → Fetcher → Parser → Store → Cache
                  ↑                           ↓
                  └─── Scheduler (Daily) ←────┘
```

### Cache Structure

```
cache/instruments/
├── instruments.json     # All instruments data
└── metadata.json       # Last fetch time, counts, venues
```

## Key Instruments (Zerodha)

Common instrument tokens for reference:
- NIFTY 50: 256265
- BANKNIFTY: 260105
- SENSEX: 265
- INDIA VIX: 264969

Note: These may change. Always fetch fresh data.

## Monitoring

The service provides comprehensive logging:
- Info level: Major operations and summaries
- Debug level: Detailed fetch progress
- Error level: Failures and retries

Use `--debug` flag for verbose output.

## Error Handling

- **Network failures**: Automatic retry with exponential backoff
- **Auth failures**: Re-authentication attempted
- **Parse failures**: Individual instrument skips, continues with rest
- **Cache failures**: Falls back to fresh fetch

## Future Enhancements

- [ ] WebSocket-based real-time instrument updates
- [ ] Database backend support (PostgreSQL/SQLite)
- [ ] REST API for instrument queries
- [ ] Differential updates using ETags
- [ ] Multiple cache strategies (Redis, in-memory)
- [ ] Historical instrument tracking
- [ ] Symbol mapping across venues