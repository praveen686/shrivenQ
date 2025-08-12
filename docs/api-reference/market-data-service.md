# Market Data Service API Reference

## Overview

The Market Data Service is a production-grade service for fetching, processing, and persisting market data including tick data, LOB snapshots, and real-time streaming data for both spot and derivatives markets.

## Commands

### `run` - Start Market Data Pipeline

Starts the complete market data collection pipeline with real-time streaming.

```bash
cargo run --bin market-data-service -- run \
  --symbols "NIFTY 50,NIFTY BANK" \
  --strike-range 10 \
  --data-dir ./data/market
```

**Options:**
- `--symbols` - Comma-separated list of spot symbols to track (default: "NIFTY 50,NIFTY BANK")
- `--strike-range` - Number of option strikes above/below spot to subscribe (default: 10)
- `--dry-run` - Run without making actual connections (for testing)
- `--data-dir` - Directory for storing market data (default: ./data/market)
- `--cache-dir` - Directory for caching instruments (default: ./cache)

### `fetch-instruments` - Update Instrument Cache

Fetches the latest instrument definitions from exchanges and updates the local cache.

```bash
cargo run --bin market-data-service -- fetch-instruments
```

This command:
- Connects to Zerodha API to fetch all available instruments
- Stores instrument metadata in JSON format
- Updates cache at `./cache/instruments/instruments.json`
- Should be run daily before market open

### `replay` - Replay Historical Data

Replays historical market data from WAL files for analysis or backtesting.

```bash
cargo run --bin market-data-service -- replay \
  --start "2024-01-15T09:15:00+05:30" \
  --end "2024-01-15T15:30:00+05:30" \
  --symbol "NIFTY"
```

**Options:**
- `--start` - Start time in ISO 8601 format (required)
- `--end` - End time in ISO 8601 format (required)  
- `--symbol` - Optional symbol filter (filters both by symbol name and venue)

**Features:**
- Replays both tick and LOB events from WAL storage
- Supports time-based filtering with nanosecond precision
- Symbol filtering with intelligent resolution using instrument store
- Provides detailed statistics including replay rate and event counts
- Falls back to venue-based filtering if instrument store is unavailable

**Example Output:**
```
Starting data replay from 2024-01-15T09:15:00+05:30 to 2024-01-15T15:30:00+05:30
Symbol filter: NIFTY
Loaded 15000 instruments for symbol filtering
Replaying tick events from ./data/market/ticks
Processed 10000 tick events
Completed replay of 45000 tick events
Replaying LOB events from ./data/market/lob
Completed replay of 12000 LOB events

📊 Replay Summary
=================
Time range: 2024-01-15T09:15:00+05:30 to 2024-01-15T15:30:00+05:30
Symbol filter: NIFTY
Total events replayed: 57000
  - Tick events: 45000
  - LOB events: 12000
  - Skipped (filtered): 3500
Replay rate: 285000 events/second
```

### `show-subscriptions` - Display Subscription Details

Shows all instruments that would be subscribed for a given symbol.

```bash
cargo run --bin market-data-service -- show-subscriptions --symbol "NIFTY 50"
```

**Output includes:**
- Spot instrument token
- Current month futures
- Next month futures  
- Option chain (calls and puts) based on strike range
- Total subscription count

### `monitor` - Real-time Monitoring Dashboard

Launches an interactive monitoring dashboard for the market data service.

```bash
cargo run --bin market-data-service -- monitor
```

**Dashboard Features:**
- **Storage Metrics:**
  - WAL directory count and types
  - Total segments and size
  - Average segment size

- **Pipeline Activity:**
  - Last update timestamp
  - Latest file modified
  - Pipeline health status (ACTIVE/SLOW/STALE/DEAD)

- **Performance Targets:**
  - LOB Updates: > 200k/sec
  - WAL Writes: > 80 MB/s
  - Replay Speed: > 3M events/min
  - Apply p50: < 200ns
  - Apply p99: < 900ns

- **System Info:**
  - Memory usage
  - Process statistics

## Data Storage

### WAL Event Types

The service stores multiple event types in the WAL:

#### TickEvent
```rust
pub struct TickEvent {
    pub ts: Ts,                    // Event timestamp
    pub venue: String,              // Trading venue (e.g., "zerodha")
    pub symbol: Symbol,             // Trading symbol (token)
    pub bid: Option<Px>,            // Best bid price
    pub ask: Option<Px>,            // Best ask price
    pub last: Option<Px>,           // Last traded price
    pub volume: Option<Qty>,        // Volume
}
```

#### LobSnapshot
```rust
pub struct LobSnapshot {
    pub ts: Ts,                     // Event timestamp
    pub symbol: Symbol,             // Trading symbol (token)
    pub venue: String,              // Venue/exchange
    pub bids: Vec<(Px, Qty)>,       // Bid levels (price, quantity)
    pub asks: Vec<(Px, Qty)>,       // Ask levels (price, quantity)
    pub sequence: u64,              // Sequence number for ordering
}
```

### Directory Structure

```
data/market/
├── ticks/                          # Tick data WAL
│   ├── 0000000001.wal
│   ├── 0000000002.wal
│   └── ...
├── lob/                            # LOB snapshot WAL
│   ├── 0000000001.wal
│   ├── 0000000002.wal
│   └── ...
└── snapshots/                      # Periodic LOB snapshots
    └── YYYYMMDD/
        └── HH/
            └── symbol_HHMMSS.bin
```

## Configuration

### Pipeline Configuration

The market data pipeline can be configured with:

```rust
PipelineConfig {
    data_dir: PathBuf,              // Data storage directory
    spot_symbols: Vec<String>,      // Spot symbols to track
    option_strike_range: u32,        // Number of strikes above/below
    strike_interval: f64,            // Strike interval (50 for NIFTY, 100 for BANKNIFTY)
    wal_segment_size: usize,         // WAL segment size (default: 100MB)
    snapshot_interval_secs: u64,     // LOB snapshot interval (default: 60s)
    enable_reconstruction: bool,     // Enable tick-to-LOB reconstruction
    max_queue_size: usize,           // Max queue size (default: 100,000)
    enable_compression: bool,        // Enable compression
}
```

### Environment Variables

Required environment variables for Zerodha integration:

```bash
export ZERODHA_USER_ID="your_user_id"
export ZERODHA_PASSWORD="your_password"
export ZERODHA_TOTP_SECRET="your_totp_secret"
export ZERODHA_API_KEY="your_api_key"
export ZERODHA_API_SECRET="your_api_secret"
```

## Performance Characteristics

### Measured Performance
- **WAL Writes:** 229 MB/s (2.86x target)
- **Replay Speed:** 298M events/min (99.5x target)  
- **Event Processing:** ~5M events/sec

### Latency (Measured)
- **Write Latency:** 0µs p50, 0µs p99
- **Replay Latency:** 1µs p50, 4µs p99
- **Recovery Time:** <1ms for typical WAL
- **Seek Time:** <1ms p99

See [Benchmark Results](/reports/benchmark/benchmark-results.md) for detailed measurements.

### Resource Usage
- **Memory:** ~500MB for typical option chain
- **Disk I/O:** Sequential writes, minimal random access
- **CPU:** Single-threaded per feed, multi-core for parallel feeds

## Error Handling

All operations use proper error handling with Result types:

- Network failures trigger automatic reconnection with exponential backoff
- WAL corruption is detected via CRC checks
- Missing instruments gracefully degrade to venue-based filtering
- All errors are logged with context for debugging

## Integration Examples

### Starting Market Data Collection

```rust
use feeds::MarketDataPipeline;
use common::instrument::InstrumentStore;

let pipeline_config = PipelineConfig {
    data_dir: PathBuf::from("./data/market"),
    spot_symbols: vec!["NIFTY 50".to_string()],
    option_strike_range: 10,
    strike_interval: 50.0,
    ..Default::default()
};

let instrument_store = Arc::new(InstrumentStore::new());
let pipeline = MarketDataPipeline::new(pipeline_config, instrument_store).await?;

// Initialize and start
pipeline.initialize_subscriptions().await?;
pipeline.start().await?;
```

### Replaying Historical Data

```rust
use storage::{Wal, WalEvent};
use common::Ts;

let wal = Wal::new("./data/market/ticks", None)?;
let mut stream = wal.stream::<WalEvent>(Some(start_ts))?;

while let Some(event) = stream.read_next_entry()? {
    match event {
        WalEvent::Tick(tick) => process_tick(tick),
        WalEvent::Lob(snapshot) => process_lob(snapshot),
        _ => {}
    }
}
```

## See Also

- [Architecture Overview](../architecture/README.md)
- [WAL Storage Documentation](./storage.md)
- [Feed Adapters](./feed-adapters.md)
- [LOB Processing](./lob.md)
